#![cfg_attr(not(test), no_std)]
mod contract;
#[cfg(test)]
mod test;
mod types;
use soroban_sdk::{contract, contractimpl, Address, Env};
#[contract]
pub struct Treasury;
#[contractimpl]
impl Treasury {
    pub fn init(env: Env, admin: Address, token: Address) -> Result<(), types::TreasuryError> {
        contract::init(&env, &admin, &token)
    }
    pub fn deposit_fee(
        env: Env,
        from: Address,
        amount: i128,
        circle_id: Address,
    ) -> Result<(), types::TreasuryError> {
        contract::deposit(&env, &from, amount, &circle_id)
    }
    pub fn withdraw(
        env: Env,
        admin: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), types::TreasuryError> {
        contract::withdraw(&env, &admin, &to, amount)
    }
    pub fn get_balance(env: Env) -> i128 {
        contract::get_balance(&env)
    }
    pub fn get_deposits(env: Env) -> soroban_sdk::Vec<types::Deposit> {
        contract::get_deposits(&env)
    }
    pub fn rescue_tokens(
        env: Env,
        admin: Address,
        to: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), types::TreasuryError> {
        contract::rescue_tokens(&env, &admin, &to, &token, amount)
    }
    pub fn pause(env: Env, admin: Address) -> Result<(), types::TreasuryError> {
        contract::pause(&env, &admin)
    }
    pub fn unpause(env: Env, admin: Address) -> Result<(), types::TreasuryError> {
        contract::unpause(&env, &admin)
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
