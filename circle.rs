// src/contracts/circle.rs
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, token, Address, Env};

#[contracterror]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CircleError {
    AlreadyInitialized = 1,
    InvalidToken = 2,
    InvalidFeeBps = 3,
    Overflow = 4,
    FeeExceedsLimit = 5,
    DiscountBipsExceeded = 6,
    InvalidAmount = 7,
    InsufficientBalance = 8,
    CircleConfigNotFound = 9,
    CollateralAlreadyStaked = 10,
    MemberRecordNotFound = 11,
    NotDefaulted = 12,
    NoCollateralToSlash = 13,
    RefundNotPermitted = 14,
    NoCollateralToRefund = 15,
    Unauthorized = 16,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleConfig {
    pub token: Address,
    pub base_amount: i128,
    pub collateral_amount: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberState {
    Active = 1,
    Defaulted = 2,
    Completed = 3,
    Exited = 4,
}

#[contracttype]
pub enum DataKey {
    Config,
    FeeBps,
    MemberBalance(Address),
    Collateral(Address),
    MemberState(Address),
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn initialize(env: Env, config: CircleConfig) -> Result<(), CircleError> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(CircleError::AlreadyInitialized);
        }

        // Validate that the token address exists on-chain and is a valid deployed contract/asset
        if !Self::token_exists(&env, &config.token) {
            return Err(CircleError::InvalidToken);
        }

        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    pub fn set_fee_bps(env: Env, fee_bps: u32) -> Result<(), CircleError> {
        if fee_bps > 10000 {
            return Err(CircleError::InvalidFeeBps);
        }

        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        Ok(())
    }

    pub fn calculate_payout_fee(env: Env, contribution_amount: i128) -> Result<i128, CircleError> {
        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);

        if contribution_amount <= 0 || fee_bps == 0 {
            return Ok(0);
        }

        let amount_u128 = contribution_amount as u128;
        let fee_bps_u128 = fee_bps as u128;

        let product = amount_u128
            .checked_mul(fee_bps_u128)
            .ok_or(CircleError::Overflow)?;

        let fee = product / 10000u128;

        if fee > i128::MAX as u128 {
            return Err(CircleError::FeeExceedsLimit);
        }

        Ok(fee as i128)
    }

    pub fn auction_bid(
        env: Env,
        bidder: Address,
        base_amount: i128,
        discount_bips: u32,
    ) -> Result<i128, CircleError> {
        bidder.require_auth();

        if discount_bips > 5000 {
            return Err(CircleError::DiscountBipsExceeded);
        }

        if base_amount <= 0 {
            return Err(CircleError::InvalidAmount);
        }

        let discount_multiplier = 10000u128 - discount_bips as u128;
        let amount_u128 = base_amount as u128;

        let discounted_amount = (amount_u128 * discount_multiplier) / 10000u128;
        let final_payable = discounted_amount as i128;

        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MemberBalance(bidder.clone()))
            .unwrap_or(0);

        if balance < final_payable {
            return Err(CircleError::InsufficientBalance);
        }

        Ok(final_payable)
    }

    pub fn join_with_collateral(env: Env, member: Address) -> Result<(), CircleError> {
        member.require_auth();

        let config: CircleConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(CircleError::CircleConfigNotFound)?;

        if config.collateral_amount <= 0 {
            return Ok(());
        }

        let existing_collateral: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Collateral(member.clone()))
            .unwrap_or(0);

        if existing_collateral > 0 {
            return Err(CircleError::CollateralAlreadyStaked);
        }

        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&member, &env.current_contract_address(), &config.collateral_amount);

        env.storage()
            .persistent()
            .set(&DataKey::Collateral(member.clone()), &config.collateral_amount);
        env.storage()
            .persistent()
            .set(&DataKey::MemberState(member.clone()), &MemberState::Active);

        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "CollateralStaked"), member),
            config.collateral_amount,
        );
        Ok(())
    }

    pub fn slash_collateral(
        env: Env,
        member: Address,
        treasury: Address,
    ) -> Result<(), CircleError> {
        let state: MemberState = env
            .storage()
            .persistent()
            .get(&DataKey::MemberState(member.clone()))
            .ok_or(CircleError::MemberRecordNotFound)?;

        if state != MemberState::Defaulted {
            return Err(CircleError::NotDefaulted);
        }

        let collateral: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Collateral(member.clone()))
            .unwrap_or(0);

        if collateral <= 0 {
            return Err(CircleError::NoCollateralToSlash);
        }

        let config: CircleConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(CircleError::CircleConfigNotFound)?;

        env.storage()
            .persistent()
            .set(&DataKey::Collateral(member.clone()), &0i128);

        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&env.current_contract_address(), &treasury, &collateral);

        env.events()
            .publish((soroban_sdk::Symbol::new(&env, "CollateralSlashed"), member), collateral);
        Ok(())
    }

    pub fn refund_collateral(env: Env, member: Address) -> Result<(), CircleError> {
        member.require_auth();

        let state: MemberState = env
            .storage()
            .persistent()
            .get(&DataKey::MemberState(member.clone()))
            .ok_or(CircleError::MemberRecordNotFound)?;

        if state != MemberState::Completed && state != MemberState::Exited {
            return Err(CircleError::RefundNotPermitted);
        }

        let collateral: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Collateral(member.clone()))
            .unwrap_or(0);

        if collateral <= 0 {
            return Err(CircleError::NoCollateralToRefund);
        }

        let config: CircleConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(CircleError::CircleConfigNotFound)?;

        env.storage()
            .persistent()
            .set(&DataKey::Collateral(member.clone()), &0i128);

        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&env.current_contract_address(), &member, &collateral);

        env.events()
            .publish((soroban_sdk::Symbol::new(&env, "CollateralRefunded"), member), collateral);
        Ok(())
    }

    fn token_exists(env: &Env, token: &Address) -> bool {
        env.try_invoke_contract::<u32, soroban_sdk::Error>(
            token,
            &soroban_sdk::Symbol::new(env, "decimals"),
            soroban_sdk::vec![env],
        )
        .is_ok()
    }

    #[cfg(test)]
    pub fn set_member_balance(env: Env, member: Address, balance: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::MemberBalance(member), &balance);
    }

    #[cfg(test)]
    pub fn set_member_state(env: Env, member: Address, state: MemberState) {
        env.storage()
            .persistent()
            .set(&DataKey::MemberState(member), &state);
    }

    #[cfg(test)]
    pub fn set_collateral(env: Env, member: Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::Collateral(member), &amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_init_rejects_non_existent_token() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let fake_token = Address::generate(&env);
        let config = CircleConfig {
            token: fake_token,
            base_amount: 1000,
            collateral_amount: 0,
        };
        let result = client.try_initialize(&config);
        assert_eq!(result, Err(Ok(CircleError::InvalidToken)));
    }

    #[test]
    fn test_init_already_initialized() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let config = CircleConfig {
            token,
            base_amount: 1000,
            collateral_amount: 0,
        };
        assert!(client.try_initialize(&config).is_ok());
        let result = client.try_initialize(&config);
        assert_eq!(result, Err(Ok(CircleError::AlreadyInitialized)));
    }

    #[test]
    fn test_set_fee_bps_bounds_rejection() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let result = client.try_set_fee_bps(&10001u32);
        assert_eq!(result, Err(Ok(CircleError::InvalidFeeBps)));
    }

    #[test]
    fn test_auction_bid_rejects_excessive_discount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let result = client.try_auction_bid(&bidder, &1000i128, &10000u32);
        assert_eq!(result, Err(Ok(CircleError::DiscountBipsExceeded)));
    }

    #[test]
    fn test_auction_bid_rejects_invalid_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let result = client.try_auction_bid(&bidder, &0i128, &1000u32);
        assert_eq!(result, Err(Ok(CircleError::InvalidAmount)));
    }

    #[test]
    fn test_auction_bid_rejects_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let result = client.try_auction_bid(&bidder, &1000i128, &1000u32);
        assert_eq!(result, Err(Ok(CircleError::InsufficientBalance)));
    }

    #[test]
    fn test_join_with_collateral_already_staked() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let config = CircleConfig {
            token,
            base_amount: 1000,
            collateral_amount: 50,
        };
        client.initialize(&config);
        let member = Address::generate(&env);
        client.set_collateral(&member, &50);
        let result = client.try_join_with_collateral(&member);
        assert_eq!(result, Err(Ok(CircleError::CollateralAlreadyStaked)));
    }

    #[test]
    fn test_slash_collateral_not_defaulted() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.set_member_state(&member, &MemberState::Active);
        let result = client.try_slash_collateral(&member, &treasury);
        assert_eq!(result, Err(Ok(CircleError::NotDefaulted)));
    }

    #[test]
    fn test_slash_collateral_no_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.set_member_state(&member, &MemberState::Defaulted);
        let result = client.try_slash_collateral(&member, &treasury);
        assert_eq!(result, Err(Ok(CircleError::NoCollateralToSlash)));
    }

    #[test]
    fn test_refund_collateral_not_permitted() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        client.set_member_state(&member, &MemberState::Active);
        let result = client.try_refund_collateral(&member);
        assert_eq!(result, Err(Ok(CircleError::RefundNotPermitted)));
    }

    #[test]
    fn test_refund_collateral_no_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CircleContract, ());
        let client = CircleContractClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        client.set_member_state(&member, &MemberState::Completed);
        let result = client.try_refund_collateral(&member);
        assert_eq!(result, Err(Ok(CircleError::NoCollateralToRefund)));
    }
}
