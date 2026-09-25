#![cfg(test)]

use soroban_sdk::{Address, Env};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use crate::Staking;
use crate::types::{StakingError, StakingPeriod, DataKey, UNBONDING_PERIOD_SECONDS};
use crate::StakingClient;

fn setup_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    
    // Deploy a test token
    let token_contract_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let token_admin_client = StellarAssetClient::new(&env, &token_contract_id);
    
    // Mint tokens to user
    token_admin_client.mint(&user, &1_000_000_0000); // 10,000 tokens
    
    (env, admin, user, token_contract_id)
}

fn deploy_staking_contract<'a>(env: &'a Env, admin: &Address, token: &Address) -> StakingClient<'a> {
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
        env.storage().instance().get(&soroban_sdk::symbol_short!("paused")).unwrap()
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
    
    // Unstake
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
    
    // Unstake
    staking_client.unstake(&user);
    
    // Fast-forward past unbonding period
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    
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
    
    // Unstake
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
        env.storage().instance().get(&soroban_sdk::symbol_short!("paused")).unwrap()
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
        env.storage().instance().get(&soroban_sdk::symbol_short!("paused")).unwrap()
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
    staking_client.unstake(&user);
    
    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
    
    // 3. Fast-forward past unbonding
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    
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
    staking_client.stake(&user1, &100_0000000, &1);  // 1x
    staking_client.stake(&user2, &100_0000000, &3);  // 2x
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
    client.unstake(&user);
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
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

    // user2 unstakes
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
    client.unstake(&user);
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    client.claim(&user);

    // List should be empty after full cycle
    assert_eq!(client.get_all_stakers().len(), 0);

    // Re-stake
    client.stake(&user, &50_0000000, &3);

    let stakers = client.get_all_stakers();
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers.get(0).unwrap(), user);
}

#[test]
fn test_reward_config_init_and_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    // Initial APY is 0 bps
    let cfg = client.get_reward_config();
    assert_eq!(cfg.apy_bps, 0);

    // Non-admin update fails
    let r = client.try_set_reward_config(&user, &1000);
    assert_eq!(r, Err(Ok(StakingError::Unauthorized)));

    // Admin update succeeds
    assert!(client.try_set_reward_config(&admin, &1000).is_ok());
    let cfg2 = client.get_reward_config();
    assert_eq!(cfg2.apy_bps, 1000);
}

#[test]
fn test_mid_stake_config_change_piecewise_recomputation() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    // Initial rate: 1000 bps (10% APY)
    client.set_reward_config(&admin, &1000);

    // User stakes 1,000 tokens for 12 months (multiplier = 5)
    let stake_amount = 1000_0000000i128;
    client.stake(&user, &stake_amount, &12);

    // Effective APY with 12-month lock (5x multiplier)
    assert_eq!(client.get_effective_apy(&user), 5000); // 50% effective APY

    // Advance by a quarter of a year: 365 * 24 * 3600 / 4 = 7_884_000 seconds
    let quarter_year: u64 = 7_884_000;
    env.ledger().set_timestamp(env.ledger().timestamp() + quarter_year);

    // Expected reward for first interval (0.25 year at 50% effective APY):
    // 1000 * 0.50 * 0.25 = 125 tokens = 125_0000000
    let reward_first_half = client.get_accrued_rewards(&user);
    assert_eq!(reward_first_half, 125_0000000);

    // Mid-stake config change: Admin updates rate to 2000 bps (20% APY)
    // Effective APY becomes 2000 * 5 = 10000 (100%)
    client.set_reward_config(&admin, &2000);
    assert_eq!(client.get_effective_apy(&user), 10000);

    // Advance by another quarter of a year
    env.ledger().set_timestamp(env.ledger().timestamp() + quarter_year);

    // Expected reward for second interval (0.25 year at 100% effective APY):
    // 1000 * 1.00 * 0.25 = 250 tokens = 250_0000000
    // Total reward = 125 + 250 = 375 tokens
    let total_reward = client.get_accrued_rewards(&user);
    assert_eq!(total_reward, 375_0000000);
}

#[test]
fn test_multiple_mid_stake_config_changes_no_double_counting() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    // User stakes 1,000 tokens for 12 months (multiplier = 5)
    let stake_amount = 1000_0000000i128;
    client.set_reward_config(&admin, &400); // 4% base, 20% effective
    client.stake(&user, &stake_amount, &12);

    let quarter_year: u64 = 15_768_000 / 2; // 7_884_000 s

    // Interval 1: 0.25 year at 20% effective = 1000 * 0.20 * 0.25 = 50 tokens
    env.ledger().set_timestamp(env.ledger().timestamp() + quarter_year);
    assert_eq!(client.get_accrued_rewards(&user), 50_0000000);

    // Interval 2: change to 800 bps (40% effective) for 0.25 year = 1000 * 0.40 * 0.25 = 100 tokens
    client.set_reward_config(&admin, &800);
    env.ledger().set_timestamp(env.ledger().timestamp() + quarter_year);
    assert_eq!(client.get_accrued_rewards(&user), 150_0000000);

    // Interval 3: change to 1200 bps (60% effective) for 0.25 year = 1000 * 0.60 * 0.25 = 150 tokens
    client.set_reward_config(&admin, &1200);
    env.ledger().set_timestamp(env.ledger().timestamp() + quarter_year);
    assert_eq!(client.get_accrued_rewards(&user), 300_0000000);
}

#[test]
fn test_distribute_rewards_transfers_and_clears_accrual() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let token_admin_client = StellarAssetClient::new(&env, &token);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    // Fund contract with reward reserve
    token_admin_client.mint(&client.address, &10_000_0000000);

    client.set_reward_config(&admin, &1000); // 10% base
    client.stake(&user, &1000_0000000, &1); // 1 month, multiplier = 1

    let initial_user_bal = token_client.balance(&user);

    // Advance 1 year: 365 days
    env.ledger().set_timestamp(env.ledger().timestamp() + 365 * 24 * 3600);

    // 1000 staked for 1 month lockup: unlocks at 30 days, so accrues for 30 days
    let accrued = client.get_accrued_rewards(&user);
    assert!(accrued > 0);

    // Claim rewards
    let claimed = client.distribute_rewards(&user);
    assert_eq!(claimed, accrued);

    // User balance increased by claimed amount
    assert_eq!(token_client.balance(&user), initial_user_bal + claimed);

    // Accrued rewards reset to 0
    assert_eq!(client.get_accrued_rewards(&user), 0);
}

