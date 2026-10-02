use soroban_sdk::{symbol_short, token, Address, Env, Vec};

use crate::types::*;
use common::pause;
use common::math;

/// Initialize the staking contract
pub fn init(env: &Env, admin: &Address, token: &Address) {
    admin.require_auth();

    // Store admin
    env.storage().instance().set(&DataKey::Admin, admin);

    // Store token address
    env.storage().instance().set(&DataKey::Token, token);

    // Initialize paused state to false
    env.storage()
        .instance()
        .set(&symbol_short!("paused"), &false);

    // Initialize total staked to 0
    env.storage().instance().set(&DataKey::TotalStaked, &0i128);

    // Initialize empty staker registry used by get_all_stakers()
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &Vec::<Address>::new(env));
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
    if amount < MIN_STAKE_AMOUNT {
        return Err(StakingError::InvalidAmount);
    }

    // Validate and convert period
    let period = StakingPeriod::from_u32(period_months).ok_or(StakingError::InvalidPeriod)?;

    // Check if user already has an active stake
    if env.storage().instance().has(&DataKey::Stake(user.clone())) {
        return Err(StakingError::AlreadyStaked);
    }

    // Get token address
    let token_address: Address = env
        .storage()
        .instance()
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

    // Calculate unlock time and ledger bounds
    let current_time = env.ledger().timestamp();
    let current_ledger = env.ledger().sequence();
    let period_seconds = period.as_seconds();
    let period_ledgers = (period_seconds / 5) as u32;
    let unlock_time = current_time
        .checked_add(period_seconds)
        .ok_or(StakingError::Overflow)?;
    let unlock_ledger = current_ledger
        .checked_add(period_ledgers)
        .ok_or(StakingError::Overflow)?;

    // Create stake position
    let stake_position = StakePosition {
        amount,
        period,
        start_time: current_time,
        unlock_time,
        voting_power,
        start_ledger: current_ledger,
        unlock_ledger,
    };

    // Store stake position
    env.storage()
        .instance()
        .set(&DataKey::Stake(user.clone()), &stake_position);

    // Update total staked
    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalStaked, &new_total);

    // Register user in the staker list (used by get_all_stakers)
    let mut stakers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));
    stakers.push_back(user.clone());
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &stakers);

    // Emit event
    Staked {
        user: user.clone(),
        amount,
        period: period_months,
        multiplier,
        voting_power,
    }
    .publish(env);

    Ok(())
}

/// Initiate unstake - tokens enter 14-day unbonding period.
///
/// The stake's lock period must have elapsed before unstaking is permitted.
/// Uses `is_period_elapsed` for consistent boundary semantics across
/// the contract: `current_time >= unlock_time` (inclusive boundary).
pub fn unstake(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Get stake position
    let stake_position: StakePosition = env
        .storage()
        .instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;

    // #433: Enforce lock period — use >= (inclusive) so that staking for
    // exactly N months unlocks at the exact boundary timestamp.
    let current_time = env.ledger().timestamp();
    if !is_period_elapsed(current_time, stake_position.unlock_time) {
        return Err(StakingError::StakeNotUnlocked);
    }

    // Calculate unbonding times
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
    env.storage()
        .instance()
        .set(&DataKey::Unbonding(user.clone()), &unbonding_position);

    // Remove stake position
    env.storage()
        .instance()
        .remove(&DataKey::Stake(user.clone()));

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
    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_sub(stake_position.amount)
        .ok_or(StakingError::InsufficientBalance)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalStaked, &new_total);

    // Emit event
    UnstakeInitiated {
        user: user.clone(),
        amount: stake_position.amount,
        claimable_time,
    }
    .publish(env);

    Ok(())
}

/// Claim tokens after unbonding period completes
pub fn claim(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Get unbonding position
    let unbonding_position: UnbondingPosition = env
        .storage()
        .instance()
        .get(&DataKey::Unbonding(user.clone()))
        .ok_or(StakingError::NoUnbondingPosition)?;

    // #433: Use unified boundary helper — same inclusive semantics as unstake.
    let current_time = env.ledger().timestamp();
    if !is_period_elapsed(current_time, unbonding_position.claimable_time) {
        return Err(StakingError::UnbondingNotComplete);
    }

    // Get token address
    let token_address: Address = env
        .storage()
        .instance()
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
    env.storage()
        .instance()
        .remove(&DataKey::Unbonding(user.clone()));

    // Emit event
    Claimed {
        user: user.clone(),
        amount: unbonding_position.amount,
    }
    .publish(env);

    Ok(())
}

/// Get user's current voting power for governance
pub fn get_voting_power(env: &Env, user: &Address) -> i128 {
    // Check for active stake
    if let Some(stake_position) = env
        .storage()
        .instance()
        .get::<DataKey, StakePosition>(&DataKey::Stake(user.clone()))
    {
        // Emit event for governance indexing
        VotingPowerQueried {
            user: user.clone(),
            voting_power: stake_position.voting_power,
        }
        .publish(env);

        return stake_position.voting_power;
    }

    // Check for unbonding position (no voting power during unbonding)
    if env
        .storage()
        .instance()
        .has(&DataKey::Unbonding(user.clone()))
    {
        VotingPowerQueried {
            user: user.clone(),
            voting_power: 0,
        }
        .publish(env);

        return 0;
    }

    // No stake or unbonding position
    VotingPowerQueried {
        user: user.clone(),
        voting_power: 0,
    }
    .publish(env);

    0
}

/// Get user's stake position
pub fn get_stake(env: &Env, user: &Address) -> Option<StakePosition> {
    env.storage().instance().get(&DataKey::Stake(user.clone()))
}

/// Query stake age, start ledger, unlock ledger, amount, and accrued rewards for an account
pub fn query_stake_info(env: &Env, account: &Address) -> StakeInfo {
    if let Some(pos) = get_stake(env, account) {
        StakeInfo {
            amount: pos.amount,
            start_ledger: pos.start_ledger,
            unlock_ledger: pos.unlock_ledger,
            accrued_rewards: 0i128,
        }
    } else {
        StakeInfo {
            amount: 0i128,
            start_ledger: 0u32,
            unlock_ledger: 0u32,
            accrued_rewards: 0i128,
        }
    }
}

/// Top up an active stake position with additional tokens
pub fn top_up_stake(
    env: &Env,
    user: &Address,
    additional_amount: i128,
) -> Result<(), StakingError> {
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;
    user.require_auth();

    if additional_amount < MIN_STAKE_AMOUNT {
        return Err(StakingError::InvalidAmount);
    }

    let mut stake_position: StakePosition = env
        .storage()
        .instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;

    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;

    let token_client = token::Client::new(env, &token_address);
    token_client.transfer(user, &env.current_contract_address(), &additional_amount);

    let multiplier = stake_position.period.multiplier();
    let add_vp = additional_amount
        .checked_mul(multiplier as i128)
        .ok_or(StakingError::Overflow)?;

    stake_position.amount = stake_position
        .amount
        .checked_add(additional_amount)
        .ok_or(StakingError::Overflow)?;
    stake_position.voting_power = stake_position
        .voting_power
        .checked_add(add_vp)
        .ok_or(StakingError::Overflow)?;

    env.storage()
        .instance()
        .set(&DataKey::Stake(user.clone()), &stake_position);

    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_add(additional_amount)
        .ok_or(StakingError::Overflow)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalStaked, &new_total);

    Staked {
        user: user.clone(),
        amount: additional_amount,
        period: stake_position.period as u32,
        multiplier,
        voting_power: stake_position.voting_power,
    }
    .publish(env);

    Ok(())
}

/// Query-path unlock check. Shares `is_period_elapsed` with `unstake` and
/// `claim`, so the exact boundary timestamp is eligible on both paths.
pub fn is_stake_unlocked(env: &Env, user: &Address) -> bool {
    match get_stake(env, user) {
        Some(stake) => is_period_elapsed(env.ledger().timestamp(), stake.unlock_time),
        None => false,
    }
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
    env.storage()
        .instance()
        .get(&DataKey::Unbonding(user.clone()))
}

/// Get total staked amount
pub fn get_total_staked(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0)
}

/// #445 — Return a page of active stakers starting at `cursor`, up to `limit` entries.
///
/// Iteration order is stable (insertion order of the `StakerList`).  `limit` is
/// silently capped at `MAX_STAKERS_PAGE_SIZE` so a single call can never return an
/// unbounded result.  Pass the returned `next_cursor` as the next call's `cursor`
/// to walk the full list; the list is exhausted when `next_cursor == total`.
pub fn query_stakers_page(env: &Env, cursor: u32, limit: u32) -> StakersPage {
    let stakers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));

    let total = stakers.len();
    let page_size = limit.min(MAX_STAKERS_PAGE_SIZE);

    let start = cursor.min(total);
    let end = start.saturating_add(page_size).min(total);

    let mut entries: Vec<StakerEntry> = Vec::new(env);
    let mut i = start;
    while i < end {
        let address = stakers.get(i).unwrap();
        let amount: i128 = env
            .storage()
            .instance()
            .get::<DataKey, StakePosition>(&DataKey::Stake(address.clone()))
            .map(|pos| pos.amount)
            .unwrap_or(0);
        entries.push_back(StakerEntry { address, amount });
        i += 1;
    }

    StakersPage {
        entries,
        next_cursor: end,
        total,
    }
}

/// Pause the contract (admin only)
pub fn pause(env: &Env, admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env
        .storage()
        .instance()
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
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;

    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }

    pause::unpause(env, admin).map_err(|_| StakingError::ContractPaused)?;
    Ok(())
}

/// Update admin (admin only)
pub fn update_admin(
    env: &Env,
    current_admin: &Address,
    new_admin: &Address,
) -> Result<(), StakingError> {
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;

    if current_admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }

    current_admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, new_admin);
    Ok(())
}

/// #433 — Unified period-boundary check.
///
/// Returns `true` when `now >= deadline` (inclusive). This single helper is
/// the **only** place where the boundary comparison lives, so every call site
/// (unstake lock check, unbonding claim check) uses the same semantics:
/// the action is permitted **at** the exact deadline timestamp, not just
/// strictly after.
#[inline]
pub(crate) fn is_period_elapsed(now: u64, deadline: u64) -> bool {
    now >= deadline
}

/// #523 — Slash a staker's collateral proportional to shortfall amount.
///
/// Slashing is proportional to the shortfall, bounded by the staked collateral.
/// Rounding favors the protocol (ceil division ensures the protocol is never
/// short-changed by truncation).
pub fn slash(
    env: &Env,
    admin: &Address,
    user: &Address,
    shortfall: i128,
) -> Result<i128, StakingError> {
    // Verify admin authorization
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    admin.require_auth();

    // Get stake position
    let stake_position: StakePosition = env
        .storage()
        .instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;

    // Calculate slash amount: min(shortfall, stake)
    // Rounding favors the protocol - any fractional amount is rounded up
    let slash_amount = if shortfall <= 0 {
        return Err(StakingError::InvalidAmount);
    } else if shortfall >= stake_position.amount {
        stake_position.amount
    } else {
        shortfall
    };

    // Update stake position
    let new_amount = stake_position.amount
        .checked_sub(slash_amount)
        .ok_or(StakingError::InsufficientBalance)?;
    
    if new_amount == 0 {
        // Remove stake entirely if slashed to zero
        env.storage().instance().remove(&DataKey::Stake(user.clone()));
        
        // Remove from staker list
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
        env.storage().persistent().set(&DataKey::StakerList, &updated);
    } else {
        // Reduce stake and recalculate voting power
        let multiplier = stake_position.period.multiplier();
        let new_voting_power = new_amount
            .checked_mul(multiplier as i128)
            .ok_or(StakingError::Overflow)?;
        
        let updated_position = StakePosition {
            amount: new_amount,
            voting_power: new_voting_power,
            ..stake_position
        };
        env.storage()
            .instance()
            .set(&DataKey::Stake(user.clone()), &updated_position);
    }

    // Update total staked
    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_sub(slash_amount)
        .ok_or(StakingError::InsufficientBalance)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalStaked, &new_total);

    // Emit slash event
    Slashed {
        user: user.clone(),
        amount: slash_amount,
        shortfall,
    }
    .publish(env);

    Ok(slash_amount)
}

/// Get the configured slash notice period in seconds.
pub fn get_slash_notice_period(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SlashNoticePeriod)
        .unwrap_or(86400) // default 24 hours
}

/// Set the slash notice period in seconds (admin only).
pub fn set_slash_notice_period(env: &Env, admin: &Address, period_seconds: u64) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    admin.require_auth();
    env.storage().instance().set(&DataKey::SlashNoticePeriod, &period_seconds);
    Ok(())
}

/// Get the active slash notice for a user, if any.
pub fn get_slash_notice(env: &Env, user: &Address) -> Option<(i128, u64, Address)> {
    env.storage()
        .instance()
        .get(&DataKey::SlashNotice(user.clone()))
}

/// Propose a slash against a staker. Creates a notice that must elapse
/// before the slash can be executed. Only the contract admin may propose.
///
/// # Arguments
/// * `env` - Execution environment
/// * `admin` - Contract admin proposing the slash
/// * `user` - Address of the staker to slash
/// * `amount` - Amount to slash (must be positive and <= user's stake)
///
/// # Returns
/// * `Ok(())` if the notice was created
/// * `Err(StakingError::Unauthorized)` if caller is not admin
/// * `Err(StakingError::InvalidAmount)` if amount is not positive
/// * `Err(StakingError::NoActiveStake)` if user has no active stake
/// * `Err(StakingError::SlashNoticePeriodActive)` if a notice already exists
pub fn slash(
    env: &Env,
    admin: &Address,
    user: &Address,
    amount: i128,
) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    admin.require_auth();
    
    if amount <= 0 {
        return Err(StakingError::InvalidAmount);
    }
    
    let stake_position: StakePosition = env.storage().instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;
    
    if amount > stake_position.amount {
        return Err(StakingError::InvalidAmount);
    }
    
    // Check if there's already an active notice
    if env.storage().instance().has(&DataKey::SlashNotice(user.clone())) {
        return Err(StakingError::SlashNoticePeriodActive);
    }
    
    let notice_period = get_slash_notice_period(env);
    let now = env.ledger().timestamp();
    let notice_until = now
        .checked_add(notice_period)
        .ok_or(StakingError::Overflow)?;
    
    env.storage().instance().set(&DataKey::SlashNotice(user.clone()), &(amount, notice_until, admin.clone()));
    
    SlashNoticeCreated {
        executor: admin.clone(),
        user: user.clone(),
        amount,
        notice_until,
    }.publish(env);
    
    Ok(())
}

/// Cancel an active slash notice during the notice period (admin only).
///
/// # Arguments
/// * `env` - Execution environment
/// * `admin` - Contract admin cancelling the slash
/// * `user` - Address of the staker whose slash notice is being cancelled
///
/// # Returns
/// * `Ok(())` if the notice was cancelled
/// * `Err(StakingError::Unauthorized)` if caller is not admin
/// * `Err(StakingError::NoActiveSlashNotice)` if no notice exists
/// * `Err(StakingError::SlashNoticeExpired)` if the notice period has elapsed
pub fn cancel_slash(
    env: &Env,
    admin: &Address,
    user: &Address,
) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    admin.require_auth();
    
    let notice: (i128, u64, Address) = env.storage().instance()
        .get(&DataKey::SlashNotice(user.clone()))
        .ok_or(StakingError::NoActiveSlashNotice)?;
    
    let now = env.ledger().timestamp();
    if now >= notice.1 {
        return Err(StakingError::SlashNoticeExpired);
    }
    
    env.storage().instance().remove(&DataKey::SlashNotice(user.clone()));
    
    SlashCancelled {
        executor: admin.clone(),
        user: user.clone(),
        amount: notice.0,
    }.publish(env);
    
    Ok(())
}

/// Execute an active slash after the notice period has elapsed (admin only).
///
/// # Arguments
/// * `env` - Execution environment
/// * `admin` - Contract admin executing the slash
/// * `user` - Address of the staker to slash
///
/// # Returns
/// * `Ok(())` if the slash was executed
/// * `Err(StakingError::Unauthorized)` if caller is not admin
/// * `Err(StakingError::NoActiveSlashNotice)` if no notice exists
/// * `Err(StakingError::SlashNoticeExpired)* if the notice period has not elapsed
pub fn execute_slash(
    env: &Env,
    admin: &Address,
    user: &Address,
) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    admin.require_auth();
    
    let notice: (i128, u64, Address) = env.storage().instance()
        .get(&DataKey::SlashNotice(user.clone()))
        .ok_or(StakingError::NoActiveSlashNotice)?;
    
    let now = env.ledger().timestamp();
    if now < notice.1 {
        return Err(StakingError::SlashNoticePeriodActive);
    }
    
    let slash_amount = notice.0;
    
    // Get the stake position
    let mut stake_position: StakePosition = env.storage().instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;
    
    // Calculate the new amount after slash
    let new_amount = math::safe_sub(stake_position.amount, slash_amount)
        .map_err(|_| StakingError::InvalidAmount)?;
    
    // Update the stake position
    stake_position.amount = new_amount;
    stake_position.voting_power = new_amount * stake_position.period.multiplier() as i128;
    env.storage().instance().set(&DataKey::Stake(user.clone()), &stake_position);
    
    // Update total staked
    let total_staked: i128 = env.storage().instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = math::safe_sub(total_staked, slash_amount)
        .map_err(|_| StakingError::InvalidAmount)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);
    
    // Remove the notice
    env.storage().instance().remove(&DataKey::SlashNotice(user.clone()));
    
    SlashExecuted {
        executor: admin.clone(),
        user: user.clone(),
        amount: slash_amount,
    }.publish(env);
    
    Ok(())
}
