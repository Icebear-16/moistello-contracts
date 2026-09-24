#![cfg_attr(not(test), no_std)]
mod contract;
#[cfg(test)]
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

#[contract]
pub struct CircleFactory;

#[contractimpl]
impl CircleFactory {
    pub fn init(
        env: Env,
        admin: Address,
        fee_bps: i128,
        circle_wasm_hash: BytesN<32>,
    ) -> Result<(), types::FactoryError> {
        contract::init(&env, &admin, fee_bps, &circle_wasm_hash)
    }

    pub fn init_with_config(
        env: Env,
        admin: Address,
        fee_bps: i128,
        treasury: Address,
        reputation_registry: Address,
        circle_wasm_hash: BytesN<32>,
    ) -> Result<(), types::FactoryError> {
        contract::init_with_config(
            &env,
            &admin,
            fee_bps,
            &treasury,
            &reputation_registry,
            &circle_wasm_hash,
        )
    }

    pub fn deploy_circle(
        env: Env,
        config: types::CircleConfig,
    ) -> Result<Address, types::FactoryError> {
        contract::deploy_circle(&env, &config)
    }

    pub fn get_circles(env: Env) -> types::CircleRegistry {
        contract::get_circles(&env)
    }

    pub fn get_circle_config(
        env: Env,
        cid: Address,
    ) -> Result<types::CircleConfig, types::FactoryError> {
        contract::get_circle_config(&env, &cid)
    }

    pub fn get_circle_count(env: Env) -> u32 {
        contract::get_circle_count(&env)
    }

    pub fn get_fee_config(env: Env) -> types::FeeConfig {
        contract::get_fee_config(&env)
    }

    pub fn get_factory_config(env: Env) -> Option<types::FactoryConfig> {
        contract::get_factory_config(&env)
    }

    pub fn set_factory_config(
        env: Env,
        admin: Address,
        treasury: Address,
        reputation_registry: Address,
        fee_bps: u32,
    ) -> Result<(), types::FactoryError> {
        contract::set_factory_config(&env, &admin, &treasury, &reputation_registry, fee_bps)
    }

    pub fn set_fee_config(
        env: Env,
        admin: Address,
        fee_bps: i128,
    ) -> Result<(), types::FactoryError> {
        contract::set_fee_config(&env, &admin, fee_bps)
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), types::FactoryError> {
        contract::pause(&env, &admin)
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), types::FactoryError> {
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
        assert!(true);
    }
}
