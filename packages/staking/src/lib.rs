#![cfg_attr(not(test), no_std)]

mod types;
mod contract;

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

    pub fn pause(env: Env, admin: Address) -> Result<(), types::StakingError> {
        contract::pause(&env, &admin)
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), types::StakingError> {
        contract::unpause(&env, &admin)
    }

    pub fn update_admin(env: Env, current_admin: Address, new_admin: Address) -> Result<(), types::StakingError> {
        contract::update_admin(&env, &current_admin, &new_admin)
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
