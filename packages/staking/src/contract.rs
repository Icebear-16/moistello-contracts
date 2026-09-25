use soroban_sdk::{token, symbol_short, Address, Env, Vec};

use crate::types::*;
use common::pause;

/// Initialize the staking contract
pub fn init(env: &Env, admin: &Address, token: &Address) {
    admin.require_auth();
    
    // Store admin
    env.storage().instance().set(&DataKey::Admin, admin);
    
    // Store token address
    env.storage().instance().set(&DataKey::Token, token);
    
    // Initialize paused state to false
    env.storage().instance().set(&symbol_short!("paused"), &false);
    
    // Initialize total staked to 0
    env.storage().instance().set(&DataKey::TotalStaked, &0i128);

    // Initialize empty staker registry used by get_all_stakers()
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &Vec::<Address>::new(env));

    // Initialize default reward config (0% APY initially)
    env.storage().instance().set(&DataKey::RewardConfig, &RewardConfig {
        apy_bps: 0,
        updated_at: env.ledger().timestamp(),
    });
}

/// Stake MOI tokens to increase governance voting power
pub fn stake(
    env: &Env,
    user: &Address,
    amount: i128,
    period_months: u32,
) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Validate amount
    if amount <= 0 {
        return Err(StakingError::InvalidAmount);
    }
    
    // Validate and convert period
    let period = StakingPeriod::from_u32(period_months)
        .ok_or(StakingError::InvalidPeriod)?;
    
    // Check if user already has an active stake
    if env.storage().instance().has(&DataKey::Stake(user.clone())) {
        return Err(StakingError::AlreadyStaked);
    }
    
    // Get token address
    let token_address: Address = env.storage().instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;
    
    // Transfer tokens from user to contract
    let token_client = token::Client::new(env, &token_address);
    token_client.transfer(user, &env.current_contract_address(), &amount);
    
    // Calculate voting power with multiplier
    let multiplier = period.multiplier();
    let voting_power = amount
        .checked_mul(multiplier as i128)
        .ok_or(StakingError::Overflow)?;
    
    // Calculate unlock time
    let current_time = env.ledger().timestamp();
    let unlock_time = current_time
        .checked_add(period.as_seconds())
        .ok_or(StakingError::Overflow)?;
    
    // Create stake position
    let stake_position = StakePosition {
        amount,
        period,
        start_time: current_time,
        unlock_time,
        voting_power,
    };
    
    // Store stake position
    env.storage().instance().set(&DataKey::Stake(user.clone()), &stake_position);

    // Initialize reward state for user
    env.storage().instance().set(&DataKey::RewardState(user.clone()), &AccruedRewardState {
        accrued_amount: 0,
        last_accrual_time: current_time,
    });
    
    // Append user to persistent stakers list (for get_all_stakers query).
    // We only add them here because the AlreadyStaked guard above guarantees
    // they are not already in the list.
    {
        let mut stakers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::StakerList)
            .unwrap_or_else(|| Vec::new(env));
        stakers.push_back(user.clone());
        env.storage().persistent().set(&DataKey::StakerList, &stakers);
    }
    
    // Update total staked
    let total_staked: i128 = env.storage().instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);

    
    // Emit event
    Staked {
        user: user.clone(),
        amount,
        period: period_months,
        multiplier,
        voting_power,
    }.publish(env);
    
    Ok(())
}

/// Initiate unstake - tokens enter 14-day unbonding period
pub fn unstake(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;
    
    user.require_auth();
    
    // Get stake position
    let stake_position: StakePosition = env.storage().instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;
    
    // Enforce unlock time duration limit (optional - allow unstaking even if not unlocked)
    // let current_time = env.ledger().timestamp();
    // if current_time < stake_position.unlock_time {
    //     return Err(StakingError::StakeNotUnlocked);
    // }
    
    // Calculate unbonding times
    let current_time = env.ledger().timestamp();
    let claimable_time = current_time
        .checked_add(UNBONDING_PERIOD_SECONDS)
        .ok_or(StakingError::Overflow)?;
    
    // Create unbonding position
    let unbonding_position = UnbondingPosition {
        amount: stake_position.amount,
        unbonding_start_time: current_time,
        claimable_time,
    };
    
    // Store unbonding position
    env.storage().instance().set(&DataKey::Unbonding(user.clone()), &unbonding_position);
    
    // Freeze accrued rewards up to unstake time
    let final_accrued = get_accrued_rewards(env, user);
    env.storage().instance().set(&DataKey::RewardState(user.clone()), &AccruedRewardState {
        accrued_amount: final_accrued,
        last_accrual_time: current_time,
    });

    // Remove stake position
    env.storage().instance().remove(&DataKey::Stake(user.clone()));
    
    // Remove user from staker list
    let stakers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));
    let mut updated: Vec<Address> = Vec::new(env);
    for s in stakers.iter() {
        if s != *user {
            updated.push_back(s);
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &updated);

    // Update total staked
    let total_staked: i128 = env.storage().instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_sub(stake_position.amount)
        .ok_or(StakingError::InsufficientBalance)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);
    
    // Emit event
    UnstakeInitiated {
        user: user.clone(),
        amount: stake_position.amount,
        claimable_time,
    }.publish(env);
    
    Ok(())
}

/// Claim tokens after unbonding period completes
pub fn claim(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Get unbonding position
    let unbonding_position: UnbondingPosition = env.storage().instance()
        .get(&DataKey::Unbonding(user.clone()))
        .ok_or(StakingError::NoUnbondingPosition)?;

    // Check if unbonding period is complete
    let current_time = env.ledger().timestamp();
    if current_time < unbonding_position.claimable_time {
        return Err(StakingError::UnbondingNotComplete);
    }

    // Get token address
    let token_address: Address = env.storage().instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;

    let token_client = token::Client::new(env, &token_address);

    // --- Preflight balance check (fix for #102) ---
    // Verify the contract holds enough tokens before attempting the transfer.
    // token::Client::transfer() panics on failure (Soroban host function), which
    // would produce an opaque SDK error rather than our typed StakingError.
    // By checking first we:
    //   (a) surface a machine-readable error code the Go client can classify, and
    //   (b) make the check → transfer → remove-state ordering explicit and correct.
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < unbonding_position.amount {
        return Err(StakingError::InsufficientContractBalance);
    }

    // Transfer tokens back to user.
    // In Soroban, if this call panics the entire transaction is rolled back, so
    // the unbonding position (written below) would never be committed.
    // We still perform the preflight above to return a typed error whenever
    // possible, and we only remove the position AFTER this line returns —
    // guaranteeing correct ordering and preventing silent fund loss.
    token_client.transfer(&env.current_contract_address(), user, &unbonding_position.amount);

    // Remove unbonding position only after the transfer has returned
    // successfully (reaching this line means the host call did not panic).
    env.storage().instance().remove(&DataKey::Unbonding(user.clone()));

    // Emit event
    Claimed {
        user: user.clone(),
        amount: unbonding_position.amount,
    }.publish(env);

    Ok(())
}

/// Get user's current voting power for governance
pub fn get_voting_power(env: &Env, user: &Address) -> i128 {
    // Check for active stake
    if let Some(stake_position) = env.storage().instance()
        .get::<DataKey, StakePosition>(&DataKey::Stake(user.clone()))
    {

        
        return stake_position.voting_power;
    }
    
    // Check for unbonding position (no voting power during unbonding)
    if env.storage().instance().has(&DataKey::Unbonding(user.clone())) {

        
        return 0;
    }
    
    // No stake or unbonding position

    
    0
}

/// Get user's stake position
pub fn get_stake(env: &Env, user: &Address) -> Option<StakePosition> {
    env.storage().instance().get(&DataKey::Stake(user.clone()))
}

/// Get the raw staked token amount for a user.
///
/// Returns `0` if the user has no active stake. This is a convenience query
/// for clients that only need the amount and not the full `StakePosition`.
pub fn get_stake_amount(env: &Env, user: &Address) -> i128 {
    env.storage()
        .instance()
        .get::<DataKey, StakePosition>(&DataKey::Stake(user.clone()))
        .map(|pos| pos.amount)
        .unwrap_or(0)
}

/// Return the list of all addresses that currently hold an active stake.
///
/// The list is maintained in persistent storage: an address is added when
/// `stake()` is called and removed when `unstake()` is called.  Addresses
/// in the unbonding period are therefore not included.
///
/// Off-chain callers can use this to iterate over active stakers, e.g. for
/// governance snapshot builds or leaderboard queries.
pub fn get_all_stakers(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env))
}

/// Get user's unbonding position
pub fn get_unbonding(env: &Env, user: &Address) -> Option<UnbondingPosition> {
    env.storage().instance().get(&DataKey::Unbonding(user.clone()))
}

/// Get total staked amount
pub fn get_total_staked(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalStaked).unwrap_or(0)
}


/// Pause the contract (admin only)
pub fn pause(env: &Env, admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    pause::pause(env, admin).map_err(|_| StakingError::ContractPaused)?;
    Ok(())
}

/// Unpause the contract (admin only)
pub fn unpause(env: &Env, admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    pause::unpause(env, admin).map_err(|_| StakingError::ContractPaused)?;
    Ok(())
}

/// Update admin (admin only)
pub fn update_admin(env: &Env, current_admin: &Address, new_admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if current_admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    current_admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, new_admin);
    Ok(())
}

fn calculate_piecewise_accrual(stake: &StakePosition, apy_bps: u32, from_time: u64, to_time: u64) -> i128 {
    if to_time <= from_time || apy_bps == 0 || stake.amount <= 0 {
        return 0;
    }
    let effective_end = to_time.min(stake.unlock_time);
    if effective_end <= from_time {
        return 0;
    }
    let duration = effective_end - from_time;
    let effective_rate = (apy_bps as i128).saturating_mul(stake.period.multiplier() as i128);
    let numerator = stake.amount
        .saturating_mul(effective_rate)
        .saturating_mul(duration as i128);
    let denominator = (SECONDS_PER_YEAR as i128) * 10_000;
    numerator / denominator
}

/// Updates the staking reward APY configuration.
///
/// Recalculates accrued rewards for all currently active stakers at the old rate
/// up to the current timestamp, and applies the new rate forward with no double-counting.
pub fn set_reward_config(env: &Env, admin: &Address, apy_bps: u32) -> Result<(), StakingError> {
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    admin.require_auth();

    let now = env.ledger().timestamp();
    let old_cfg: RewardConfig = env.storage().instance()
        .get(&DataKey::RewardConfig)
        .unwrap_or(RewardConfig { apy_bps: 0, updated_at: now });

    // Mid-stake config change: recalculate accrued rewards for all active stakers at old rate
    let stakers: Vec<Address> = env.storage().persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));

    for user in stakers.iter() {
        if let Some(stake_pos) = env.storage().instance().get::<DataKey, StakePosition>(&DataKey::Stake(user.clone())) {
            let mut state: AccruedRewardState = env.storage().instance()
                .get(&DataKey::RewardState(user.clone()))
                .unwrap_or(AccruedRewardState {
                    accrued_amount: 0,
                    last_accrual_time: stake_pos.start_time,
                });
            if now > state.last_accrual_time {
                let additional = calculate_piecewise_accrual(&stake_pos, old_cfg.apy_bps, state.last_accrual_time, now);
                state.accrued_amount = state.accrued_amount.saturating_add(additional);
                state.last_accrual_time = now;
                env.storage().instance().set(&DataKey::RewardState(user.clone()), &state);
            }
        }
    }

    env.storage().instance().set(&DataKey::RewardConfig, &RewardConfig { apy_bps, updated_at: now });
    RewardConfigUpdated {
        old_apy_bps: old_cfg.apy_bps,
        new_apy_bps: apy_bps,
        updated_at: now,
    }.publish(env);

    Ok(())
}

/// Returns the current reward configuration.
pub fn get_reward_config(env: &Env) -> RewardConfig {
    env.storage().instance().get(&DataKey::RewardConfig).unwrap_or(RewardConfig { apy_bps: 0, updated_at: 0 })
}

/// Returns the total accrued rewards for a user across all piecewise intervals.
pub fn get_accrued_rewards(env: &Env, user: &Address) -> i128 {
    let stake_pos = match env.storage().instance().get::<DataKey, StakePosition>(&DataKey::Stake(user.clone())) {
        Some(p) => p,
        None => {
            return env.storage().instance()
                .get::<DataKey, AccruedRewardState>(&DataKey::RewardState(user.clone()))
                .map(|s| s.accrued_amount)
                .unwrap_or(0);
        }
    };

    let state: AccruedRewardState = env.storage().instance()
        .get(&DataKey::RewardState(user.clone()))
        .unwrap_or(AccruedRewardState {
            accrued_amount: 0,
            last_accrual_time: stake_pos.start_time,
        });

    let cfg: RewardConfig = env.storage().instance()
        .get(&DataKey::RewardConfig)
        .unwrap_or(RewardConfig { apy_bps: 0, updated_at: env.ledger().timestamp() });

    let now = env.ledger().timestamp();
    let forward_reward = if now > state.last_accrual_time {
        calculate_piecewise_accrual(&stake_pos, cfg.apy_bps, state.last_accrual_time, now)
    } else {
        0
    };

    state.accrued_amount.saturating_add(forward_reward)
}

/// Returns the effective APY in bps for a user taking into account staking period multiplier.
pub fn get_effective_apy(env: &Env, user: &Address) -> u32 {
    let cfg: RewardConfig = env.storage().instance()
        .get(&DataKey::RewardConfig)
        .unwrap_or(RewardConfig { apy_bps: 0, updated_at: 0 });
    if let Some(stake_pos) = env.storage().instance().get::<DataKey, StakePosition>(&DataKey::Stake(user.clone())) {
        cfg.apy_bps.saturating_mul(stake_pos.period.multiplier())
    } else {
        cfg.apy_bps
    }
}

/// Claims and transfers accrued staking rewards to the user.
pub fn distribute_rewards(env: &Env, user: &Address) -> Result<i128, StakingError> {
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;
    user.require_auth();

    let total_reward = get_accrued_rewards(env, user);
    if total_reward <= 0 {
        return Ok(0);
    }

    let token_address: Address = env.storage().instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;
    let token_client = token::Client::new(env, &token_address);

    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < total_reward {
        return Err(StakingError::InsufficientContractBalance);
    }

    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::RewardState(user.clone()), &AccruedRewardState {
        accrued_amount: 0,
        last_accrual_time: now,
    });

    token_client.transfer(&env.current_contract_address(), user, &total_reward);

    RewardsDistributed {
        user: user.clone(),
        amount: total_reward,
    }.publish(env);

    Ok(total_reward)
}
