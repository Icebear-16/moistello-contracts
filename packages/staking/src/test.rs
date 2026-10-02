#![cfg(test)]

use crate::types::{
    DataKey, StakingError, StakingPeriod, MAX_STAKERS_PAGE_SIZE, MIN_STAKE_AMOUNT,
    UNBONDING_PERIOD_SECONDS,
};
use crate::Staking;
use crate::StakingClient;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

fn setup_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Deploy a test token
    let token_contract_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = StellarAssetClient::new(&env, &token_contract_id);

    // Mint tokens to user
    token_admin_client.mint(&user, &1_000_000_0000); // 10,000 tokens

    (env, admin, user, token_contract_id)
}

fn at_unlock(env: &Env, client: &StakingClient, user: &Address) {
    if let Some(stake) = client.get_stake(user) {
        env.ledger().set_timestamp(stake.unlock_time);
    }
}

fn deploy_staking_contract<'a>(
    env: &'a Env,
    admin: &Address,
    token: &Address,
) -> StakingClient<'a> {
    let staking_contract_id = env.register(Staking, ());
    let staking_client = StakingClient::new(env, &staking_contract_id);

    staking_client.init(admin, token);

    staking_client
}

#[test]
fn test_init() {
    let (env, admin, _, token) = setup_test_env();

    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Verify admin is stored
    let stored_admin: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, admin);

    // Verify token is stored
    let stored_token: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Token).unwrap()
    });
    assert_eq!(stored_token, token);

    // Verify paused is false
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
    assert!(!paused);

    // Verify total staked is 0
    let total_staked: i128 = staking_client.get_total_staked();
    assert_eq!(total_staked, 0);
}

#[test]
fn test_stake_one_month_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 1; // 1 month = 1x multiplier

    let result = staking_client.try_stake(&user, &amount, &period_months);
    assert!(result.is_ok());

    // Verify stake position
    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().amount, amount);
    assert_eq!(stake.as_ref().unwrap().period, StakingPeriod::OneMonth);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount); // 1x multiplier

    // Verify total staked
    let total_staked = staking_client.get_total_staked();
    assert_eq!(total_staked, amount);

    // Verify voting power
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, amount);
}

#[test]
fn test_stake_three_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 3; // 3 months = 2x multiplier

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 2); // 2x multiplier
}

#[test]
fn test_stake_six_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 6; // 6 months = 3x multiplier

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 3); // 3x multiplier
}

#[test]
fn test_stake_twelve_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 12; // 12 months = 5x multiplier

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 5); // 5x multiplier
}

#[test]
fn test_stake_invalid_amount_zero() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &0, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_invalid_amount_negative() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &-100, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_rejects_dust_amount_below_minimum() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &(MIN_STAKE_AMOUNT - 1), &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
    assert!(staking_client.get_stake(&user).is_none());
    assert_eq!(staking_client.get_total_staked(), 0);
}

#[test]
fn test_stake_invalid_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &100_0000000, &2); // Invalid period
    assert_eq!(result, Err(Ok(StakingError::InvalidPeriod)));
}

#[test]
fn test_stake_already_staked() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // First stake
    staking_client.stake(&user, &100_0000000, &1);

    // Try to stake again
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::AlreadyStaked)));
}

#[test]
fn test_stake_insufficient_token_balance() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // User only has 10,000 tokens, try to stake 100,000
    let result = staking_client.try_stake(&user, &100_000_0000000, &1);
    assert!(result.is_err());
}

#[test]
fn test_unstake_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    let result = staking_client.try_unstake(&user);
    assert!(result.is_ok());

    // Verify stake is removed
    assert!(staking_client.get_stake(&user).is_none());

    // Verify unbonding position exists
    let unbonding = staking_client.get_unbonding(&user);
    assert_eq!(unbonding.as_ref().unwrap().amount, amount);

    // Verify total staked is reduced
    let total_staked = staking_client.get_total_staked();
    assert_eq!(total_staked, 0);

    // Verify voting power is 0 during unbonding
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_unstake_no_active_stake() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_unstake(&user);
    assert_eq!(result, Err(Ok(StakingError::NoActiveStake)));
}

#[test]
fn test_claim_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    staking_client.unstake(&user);

    // Fast-forward past unbonding period
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);

    // Claim
    let result = staking_client.try_claim(&user);
    assert!(result.is_ok());

    // Verify unbonding position is removed
    assert!(staking_client.get_unbonding(&user).is_none());

    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_claim_unbonding_not_complete() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    staking_client.unstake(&user);

    // Try to claim immediately (unbonding not complete)
    let result = staking_client.try_claim(&user);
    assert_eq!(result, Err(Ok(StakingError::UnbondingNotComplete)));
}

#[test]
fn test_claim_no_unbonding_position() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_claim(&user);
    assert_eq!(result, Err(Ok(StakingError::NoUnbondingPosition)));
}

#[test]
fn test_pause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Pause contract
    staking_client.pause(&admin);

    // Verify paused state
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
    assert!(paused);

    // Try to stake while paused
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::ContractPaused)));
}

#[test]
fn test_unpause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Pause contract
    staking_client.pause(&admin);

    // Unpause contract
    staking_client.unpause(&admin);

    // Verify unpaused state
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
    assert!(!paused);

    // Should be able to stake now
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert!(result.is_ok());
}

#[test]
fn test_pause_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Try to pause as non-admin
    let result = staking_client.try_pause(&user);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

#[test]
fn test_update_admin() {
    let (env, admin, _, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let new_admin = Address::generate(&env);
    staking_client.update_admin(&admin, &new_admin);

    // Verify new admin
    let stored_admin: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, new_admin);

    // Old admin should no longer be able to pause
    let result = staking_client.try_pause(&admin);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));

    // New admin should be able to pause
    let result = staking_client.try_pause(&new_admin);
    assert!(result.is_ok());
}

#[test]
fn test_update_admin_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let new_admin = Address::generate(&env);

    // Try to update admin as non-admin
    let result = staking_client.try_update_admin(&user, &new_admin);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

#[test]
fn test_get_voting_power_no_stake() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_get_voting_power_during_unbonding() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake
    staking_client.stake(&user, &100_0000000, &3);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    staking_client.unstake(&user);

    // Voting power should be 0 during unbonding
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_full_staking_lifecycle() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // 1. Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &6);

    // Verify voting power (3x multiplier for 6 months)
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, amount * 3);

    // 2. Unstake
    at_unlock(&env, &staking_client, &user);
    staking_client.unstake(&user);

    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);

    // 3. Fast-forward past unbonding
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);

    // 4. Claim
    staking_client.claim(&user);

    // Verify no positions remain
    assert!(staking_client.get_stake(&user).is_none());
    assert!(staking_client.get_unbonding(&user).is_none());
}

#[test]
fn test_multiple_users_staking() {
    let (env, admin, _, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Mint tokens to users
    let token_admin_client = StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&user1, &1_000_0000000);
    token_admin_client.mint(&user2, &1_000_0000000);
    token_admin_client.mint(&user3, &1_000_0000000);

    // Each user stakes with different periods
    staking_client.stake(&user1, &100_0000000, &1); // 1x
    staking_client.stake(&user2, &100_0000000, &3); // 2x
    staking_client.stake(&user3, &100_0000000, &12); // 5x

    // Verify individual voting powers
    assert_eq!(staking_client.get_voting_power(&user1), 100_0000000);
    assert_eq!(staking_client.get_voting_power(&user2), 200_0000000);
    assert_eq!(staking_client.get_voting_power(&user3), 500_0000000);

    // Verify total staked
    assert_eq!(staking_client.get_total_staked(), 300_0000000);
}

#[test]
fn test_staking_period_as_seconds() {
    assert_eq!(StakingPeriod::OneMonth.as_seconds(), 30 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::ThreeMonths.as_seconds(), 90 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::SixMonths.as_seconds(), 180 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::TwelveMonths.as_seconds(), 360 * 24 * 60 * 60);
}

#[test]
fn test_staking_period_from_u32() {
    assert_eq!(StakingPeriod::from_u32(1), Some(StakingPeriod::OneMonth));
    assert_eq!(StakingPeriod::from_u32(3), Some(StakingPeriod::ThreeMonths));
    assert_eq!(StakingPeriod::from_u32(6), Some(StakingPeriod::SixMonths));
    assert_eq!(StakingPeriod::from_u32(12), Some(StakingPeriod::TwelveMonths));
    assert_eq!(StakingPeriod::from_u32(2), None);
    assert_eq!(StakingPeriod::from_u32(24), None);
}

// =============================================================================
// Issue #467 – slashing notice period before execution
// =============================================================================

#[test]
fn test_slash_notice_created() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake first
    staking_client.stake(&user, &100_0000000, &1);
    
    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);
    
    // Create slash notice
    let result = staking_client.try_slash(&admin, &user, &50_0000000);
    assert!(result.is_ok());
    
    // Verify notice exists
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_some());
    let (amount, notice_until, executor) = notice.unwrap();
    assert_eq!(amount, 50_0000000);
    assert_eq!(executor, admin);
}

#[test]
fn test_slash_cancelled_during_notice_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake first
    staking_client.stake(&user, &100_0000000, &1);
    
    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);
    
    // Create slash notice
    staking_client.slash(&admin, &user, &50_0000000);
    
    // Cancel slash during notice period
    let result = staking_client.try_cancel_slash(&admin, &user);
    assert!(result.is_ok());
    
    // Verify notice is removed
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_none());
}

#[test]
fn test_slash_executed_after_notice_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake first
    staking_client.stake(&user, &100_0000000, &1);
    
    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);
    
    // Create slash notice
    staking_client.slash(&admin, &user, &50_0000000);
    
    // Advance past notice period
    env.ledger().set_timestamp(env.ledger().timestamp() + 20);
    
    // Execute slash
    let result = staking_client.try_execute_slash(&admin, &user);
    assert!(result.is_ok());
    
    // Verify stake is reduced
    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().amount, 50_0000000);
    
    // Verify notice is removed
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_none());
}

#[test]
fn test_slash_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake first
    staking_client.stake(&user, &100_0000000, &1);
    
    // Stranger tries to slash
    let stranger = Address::generate(&env);
    let result = staking_client.try_slash(&stranger, &user, &50_0000000);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
// ── Tests for get_stake_amount ────────────────────────────────────────────────

/// Happy path: staking sets the correct amount readable via get_stake_amount.
#[test]
fn test_get_stake_amount_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let amount = 200_0000000i128; // 200 tokens
    client.stake(&user, &amount, &3);

    assert_eq!(client.get_stake_amount(&user), amount);
}

/// No stake → get_stake_amount returns 0 (not an error).
#[test]
fn test_get_stake_amount_no_stake_returns_zero() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    assert_eq!(client.get_stake_amount(&user), 0);
}

/// After unstake the stake entry is removed; get_stake_amount must return 0.
#[test]
fn test_get_stake_amount_after_unstake_returns_zero() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    client.stake(&user, &100_0000000, &1);
    assert_eq!(client.get_stake_amount(&user), 100_0000000);

    at_unlock(&env, &client, &user);
    client.unstake(&user);
    assert_eq!(client.get_stake_amount(&user), 0);
}

/// Staking with all valid period variants returns the correct raw amount.
#[test]
fn test_get_stake_amount_all_periods() {
    for period in [1u32, 3, 6, 12] {
        let (env, admin, user, token) = setup_test_env();
        let client = deploy_staking_contract(&env, &admin, &token);

        let amount = 50_0000000i128;
        client.stake(&user, &amount, &period);
        // get_stake_amount returns the raw token amount regardless of multiplier
        assert_eq!(client.get_stake_amount(&user), amount);
    }
}

/// After the full lifecycle (stake → unstake → claim) get_stake_amount is 0.
#[test]
fn test_get_stake_amount_after_full_lifecycle() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    client.stake(&user, &100_0000000, &1);
    at_unlock(&env, &client, &user);
    client.unstake(&user);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    client.claim(&user);

    assert_eq!(client.get_stake_amount(&user), 0);
}

// ── Tests for get_all_stakers ─────────────────────────────────────────────────

/// No stakers yet → get_all_stakers returns an empty list.
#[test]
fn test_get_all_stakers_empty() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 0);
}

/// One user stakes → get_all_stakers contains exactly that user.
#[test]
fn test_get_all_stakers_single_user() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    client.stake(&user, &100_0000000, &1);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers.get(0).unwrap(), user);
}

/// Multiple users stake → all appear in get_all_stakers.
#[test]
fn test_get_all_stakers_multiple_users() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let token_admin = StellarAssetClient::new(&env, &token);
    token_admin.mint(&user1, &1_000_0000000);
    token_admin.mint(&user2, &1_000_0000000);
    token_admin.mint(&user3, &1_000_0000000);

    client.stake(&user1, &100_0000000, &1);
    client.stake(&user2, &200_0000000, &3);
    client.stake(&user3, &300_0000000, &6);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 3);

    // All three addresses must be present (order: insertion order)
    let has_user1 = stakers.iter().any(|a| a == user1);
    let has_user2 = stakers.iter().any(|a| a == user2);
    let has_user3 = stakers.iter().any(|a| a == user3);
    assert!(has_user1, "user1 not found in stakers list");
    assert!(has_user2, "user2 not found in stakers list");
    assert!(has_user3, "user3 not found in stakers list");
}

/// After unstake the user is removed from get_all_stakers.
#[test]
fn test_get_all_stakers_after_unstake_removes_entry() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    client.stake(&user, &100_0000000, &1);
    assert_eq!(client.get_all_stakers().len(), 1);

    at_unlock(&env, &client, &user);
    client.unstake(&user);
    assert_eq!(client.get_all_stakers().len(), 0);
}

/// Mixed scenario: some users unstake, others remain — list stays accurate.
#[test]
fn test_get_all_stakers_partial_unstake() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let token_admin = StellarAssetClient::new(&env, &token);
    token_admin.mint(&user1, &1_000_0000000);
    token_admin.mint(&user2, &1_000_0000000);
    token_admin.mint(&user3, &1_000_0000000);

    client.stake(&user1, &100_0000000, &1);
    client.stake(&user2, &100_0000000, &1);
    client.stake(&user3, &100_0000000, &1);

    // user2 unstakes at the lock boundary
    at_unlock(&env, &client, &user2);
    client.unstake(&user2);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 2);
    assert!(stakers.iter().any(|a| a == user1), "user1 should still be in list");
    assert!(!stakers.iter().any(|a| a == user2), "user2 should be removed");
    assert!(stakers.iter().any(|a| a == user3), "user3 should still be in list");
}

/// Re-staking after full lifecycle adds the user back to the list.
#[test]
fn test_get_all_stakers_restake_after_full_lifecycle() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    // First stake cycle
    client.stake(&user, &100_0000000, &1);
    at_unlock(&env, &client, &user);
    client.unstake(&user);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    client.claim(&user);

    // List should be empty after full cycle
    assert_eq!(client.get_all_stakers().len(), 0);

    // Re-stake
    client.stake(&user, &50_0000000, &3);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers.get(0).unwrap(), user);
}

// ── #445 — query_stakers_page ─────────────────────────────────────────────────

/// Empty list → page has zero entries, next_cursor and total are both 0.
#[test]
fn test_query_stakers_page_empty_list() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let page = client.query_stakers_page(&0, &10);
    assert_eq!(page.total, 0);
    assert_eq!(page.entries.len(), 0);
    assert_eq!(page.next_cursor, 0);
}

/// Single staker — full page fetch returns the staker with the correct amount.
#[test]
fn test_query_stakers_page_single_staker() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000i128;
    client.stake(&user, &amount, &1);

    let page = client.query_stakers_page(&0, &10);
    assert_eq!(page.total, 1);
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries.get(0).unwrap().address, user);
    assert_eq!(page.entries.get(0).unwrap().amount, amount);
    assert_eq!(page.next_cursor, 1);
}

/// `limit` above `MAX_STAKERS_PAGE_SIZE` is silently capped.
#[test]
fn test_query_stakers_page_limit_capped() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let token_admin = StellarAssetClient::new(&env, &token);
    let per_user_balance = 1_000_0000000i128;
    let stake_amount = 10_0000000i128;

    // Stake (MAX_STAKERS_PAGE_SIZE + 5) users so the cap is observable.
    let n = (MAX_STAKERS_PAGE_SIZE + 5) as usize;
    for _ in 0..n {
        let u = Address::generate(&env);
        token_admin.mint(&u, &per_user_balance);
        client.stake(&u, &stake_amount, &1);
    }

    // Request more than the cap — must receive exactly MAX_STAKERS_PAGE_SIZE entries.
    let page = client.query_stakers_page(&0, &(MAX_STAKERS_PAGE_SIZE + 100));
    assert_eq!(page.entries.len(), MAX_STAKERS_PAGE_SIZE);
    assert_eq!(page.total, n as u32);
    assert_eq!(page.next_cursor, MAX_STAKERS_PAGE_SIZE);
}

/// Pagination is stable: walking all pages with limit=10 covers every staker exactly once.
#[test]
fn test_query_stakers_page_stable_order_across_pages() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let token_admin = StellarAssetClient::new(&env, &token);
    let stake_amount = 10_0000000i128;
    let n: u32 = 35; // intentionally not a multiple of page size

    let mut expected_order: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let u = Address::generate(&env);
        token_admin.mint(&u, &1_000_0000000i128);
        client.stake(&u, &stake_amount, &1);
        expected_order.push_back(u);
    }

    let page_size: u32 = 10;
    let mut cursor: u32 = 0;
    let mut collected: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);

    loop {
        let page = client.query_stakers_page(&cursor, &page_size);
        for entry in page.entries.iter() {
            collected.push_back(entry.address.clone());
        }
        cursor = page.next_cursor;
        if cursor >= page.total {
            break;
        }
    }

    assert_eq!(collected.len(), n);
    for i in 0..n {
        assert_eq!(
            collected.get(i).unwrap(),
            expected_order.get(i).unwrap(),
            "staker at position {i} does not match insertion order"
        );
    }
}

/// cursor beyond the end of the list returns an empty page with next_cursor == total.
#[test]
fn test_query_stakers_page_cursor_past_end() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    client.stake(&user, &50_0000000, &1);

    let page = client.query_stakers_page(&999, &10);
    assert_eq!(page.total, 1);
    assert_eq!(page.entries.len(), 0);
    assert_eq!(page.next_cursor, 1); // clamped to total
}

/// Each entry carries the correct staked amount (not voting power).
#[test]
fn test_query_stakers_page_amounts_correct() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let token_admin = StellarAssetClient::new(&env, &token);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    token_admin.mint(&user_a, &1_000_0000000i128);
    token_admin.mint(&user_b, &1_000_0000000i128);

    // Different periods → different voting power but same raw amounts.
    client.stake(&user_a, &200_0000000, &3); // 2x VP, but amount = 200
    client.stake(&user_b, &300_0000000, &6); // 3x VP, but amount = 300

    let page = client.query_stakers_page(&0, &10);
    assert_eq!(page.total, 2);

    let entry_a = page.entries.get(0).unwrap();
    let entry_b = page.entries.get(1).unwrap();
    assert_eq!(entry_a.amount, 200_0000000);
    assert_eq!(entry_b.amount, 300_0000000);
}

/// 200-staker scenario: all pages together equal exactly 200 unique entries (#445 AC).
#[test]
fn test_query_stakers_page_two_hundred_stakers() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let token_admin = StellarAssetClient::new(&env, &token);
    let n: u32 = 200;

    for _ in 0..n {
        let u = Address::generate(&env);
        token_admin.mint(&u, &1_000_0000000i128);
        client.stake(&u, &10_0000000, &1);
    }

    // Walk all pages and count unique entries.
    let mut total_seen: u32 = 0;
    let mut cursor: u32 = 0;

    loop {
        let page = client.query_stakers_page(&cursor, &MAX_STAKERS_PAGE_SIZE);
        assert_eq!(page.total, n, "total must remain stable across pages");
        total_seen += page.entries.len();
        cursor = page.next_cursor;
        if cursor >= page.total {
            break;
        }
    }

    assert_eq!(total_seen, n, "paginating over all pages must yield exactly {n} entries");
    // We needed exactly ceil(200/50) = 4 pages.
    assert_eq!(cursor, n);
}

/// #433: query and withdraw agree at the exact unlock timestamp, and one
/// second earlier both reject.
#[test]
fn test_unstake_exact_unlock_boundary() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);
    client.stake(&user, &100_0000000, &1);
    let unlock_time = client.get_stake(&user).unwrap().unlock_time;

    env.ledger().set_timestamp(unlock_time - 1);
    assert!(!client.is_stake_unlocked(&user));
    assert_eq!(client.try_unstake(&user), Err(Ok(StakingError::StakeNotUnlocked)));

    env.ledger().set_timestamp(unlock_time);
    assert!(client.is_stake_unlocked(&user));
    client.unstake(&user);

    let unbonding = client.get_unbonding(&user).unwrap();
    env.ledger().set_timestamp(unbonding.claimable_time - 1);
    assert!(client.try_claim(&user).is_err());
    env.ledger().set_timestamp(unbonding.claimable_time);
    client.claim(&user);
    assert!(client.get_unbonding(&user).is_none());
}

#[test]
fn test_query_stake_info_active_topped_up_and_unstaked() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    // 1. Unstaked case: zeroed StakeInfo
    let unstaked_info = client.query_stake_info(&user);
    assert_eq!(unstaked_info.amount, 0);
    assert_eq!(unstaked_info.start_ledger, 0);
    assert_eq!(unstaked_info.unlock_ledger, 0);
    assert_eq!(unstaked_info.accrued_rewards, 0);

    // 2. Active stake case
    let amount = 100_0000000;
    let period_months = 1;
    let start_seq = env.ledger().sequence();
    client.stake(&user, &amount, &period_months);

    let active_info = client.query_stake_info(&user);
    assert_eq!(active_info.amount, amount);
    assert_eq!(active_info.start_ledger, start_seq);
    let expected_unlock_ledger = start_seq + (30 * 24 * 60 * 60 / 5);
    assert_eq!(active_info.unlock_ledger, expected_unlock_ledger);
    assert_eq!(active_info.accrued_rewards, 0);

    // 3. Topped-up case
    let additional_amount = 50_0000000;
    client.top_up_stake(&user, &additional_amount);

    let topped_up_info = client.query_stake_info(&user);
    assert_eq!(topped_up_info.amount, amount + additional_amount);
    assert_eq!(topped_up_info.start_ledger, start_seq);
    assert_eq!(topped_up_info.unlock_ledger, expected_unlock_ledger);
    assert_eq!(topped_up_info.accrued_rewards, 0);
}
