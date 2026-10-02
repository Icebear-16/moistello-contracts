#![cfg_attr(not(test), no_std)]

mod contract;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct Staking;

#[contractimpl]
impl Staking {
    pub fn init(env: Env, admin: Address, token: Address) {
        contract::init(&env, &admin, &token);
    }

    pub fn stake(
        env: Env,
        user: Address,
        amount: i128,
        period_months: u32,
    ) -> Result<(), types::StakingError> {
        contract::stake(&env, &user, amount, period_months)
    }

    pub fn unstake(env: Env, user: Address) -> Result<(), types::StakingError> {
        contract::unstake(&env, &user)
    }

    pub fn claim(env: Env, user: Address) -> Result<(), types::StakingError> {
        contract::claim(&env, &user)
    }

    pub fn get_voting_power(env: Env, user: Address) -> i128 {
        contract::get_voting_power(&env, &user)
    }

    pub fn get_stake(env: Env, user: Address) -> Option<types::StakePosition> {
        contract::get_stake(&env, &user)
    }

    /// Query stake age, start ledger, unlock ledger, amount, and accrued rewards for an account
    pub fn query_stake_info(env: Env, account: Address) -> types::StakeInfo {
        contract::query_stake_info(&env, &account)
    }

    pub fn top_up_stake(
        env: Env,
        user: Address,
        additional_amount: i128,
    ) -> Result<(), types::StakingError> {
        contract::top_up_stake(&env, &user, additional_amount)
    }

    /// Query path for unlock eligibility. Uses the same inclusive boundary
    /// as `unstake` (`now >= unlock_time`).
    pub fn is_stake_unlocked(env: Env, user: Address) -> bool {
        contract::is_stake_unlocked(&env, &user)
    }

    pub fn get_unbonding(env: Env, user: Address) -> Option<types::UnbondingPosition> {
        contract::get_unbonding(&env, &user)
    }

    pub fn get_total_staked(env: Env) -> i128 {
        contract::get_total_staked(&env)
    }

    /// Returns only the raw staked token amount for a user (0 if none).
    /// Convenience alternative to get_stake() for callers that don't need
    /// the full StakePosition.
    pub fn get_stake_amount(env: Env, user: Address) -> i128 {
        contract::get_stake_amount(&env, &user)
    }

    /// Returns the list of all addresses that currently hold an active stake.
    /// Maintained automatically by stake() / unstake().
    pub fn get_all_stakers(env: Env) -> soroban_sdk::Vec<Address> {
        contract::get_all_stakers(&env)
    }

    /// #445 — Return a page of active stakers for indexer sync.
    ///
    /// `cursor` is the zero-based start index; `limit` is capped at
    /// `MAX_STAKERS_PAGE_SIZE` (50).  Each entry carries the staker's address
    /// and their current staked token amount.  When `page.next_cursor == page.total`
    /// the list is exhausted.
    pub fn query_stakers_page(env: Env, cursor: u32, limit: u32) -> types::StakersPage {
        contract::query_stakers_page(&env, cursor, limit)
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), types::StakingError> {
        contract::pause(&env, &admin)
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), types::StakingError> {
        contract::unpause(&env, &admin)
    }

    pub fn update_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), types::StakingError> {
        contract::update_admin(&env, &current_admin, &new_admin)
    }

    pub fn get_slash_notice_period(env: Env) -> u64 {
        contract::get_slash_notice_period(&env)
    }

    pub fn set_slash_notice_period(env: Env, admin: Address, period_seconds: u64) -> Result<(), types::StakingError> {
        contract::set_slash_notice_period(&env, &admin, period_seconds)
    }

    pub fn get_slash_notice(env: Env, user: Address) -> Option<(i128, u64, Address)> {
        contract::get_slash_notice(&env, &user)
    }

    pub fn slash(env: Env, admin: Address, user: Address, amount: i128) -> Result<(), types::StakingError> {
        contract::slash(&env, &admin, &user, amount)
    }

    pub fn cancel_slash(env: Env, admin: Address, user: Address) -> Result<(), types::StakingError> {
        contract::cancel_slash(&env, &admin, &user)
    }

    pub fn execute_slash(env: Env, admin: Address, user: Address) -> Result<(), types::StakingError> {
        contract::execute_slash(&env, &admin, &user)
    /// #523 — Slash a staker's collateral proportional to shortfall.
    /// Returns the actual amount slashed (min of shortfall and stake).
    pub fn slash(
        env: Env,
        admin: Address,
        user: Address,
        shortfall: i128,
    ) -> Result<i128, types::StakingError> {
        contract::slash(&env, &admin, &user, shortfall)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_smoke_compile() {
        assert!(true);
    }

    #[test]
    fn test_types_compile() {
        // Verify contract types compile correctly
        assert!(true);
    }
}
