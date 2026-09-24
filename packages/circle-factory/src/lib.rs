#![cfg_attr(not(test), no_std)]
mod types; mod contract; #[cfg(test)] mod test;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
#[contract] pub struct CircleFactory;
#[contractimpl]
impl CircleFactory {
    pub fn init(env: Env, admin: Address, fee_bps: i128, circle_wasm_hash: BytesN<32>) -> Result<(), types::FactoryError> { contract::init(&env, &admin, fee_bps, &circle_wasm_hash) }
    pub fn deploy_circle(env: Env, config: types::CircleConfig) -> Result<Address, types::FactoryError> { contract::deploy_circle(&env, &config) }
    pub fn get_circles(env: Env) -> types::CircleRegistry { contract::get_circles(&env) }
    pub fn get_circle_config(env: Env, cid: Address) -> Result<types::CircleConfig, types::FactoryError> { contract::get_circle_config(&env, &cid) }
    pub fn get_circle_count(env: Env) -> u32 { contract::get_circle_count(&env) }
    pub fn get_fee_config(env: Env) -> types::FeeConfig { contract::get_fee_config(&env) }
    pub fn set_fee_config(env: Env, admin: Address, fee_bps: i128) -> Result<(), types::FactoryError> { contract::set_fee_config(&env, &admin, fee_bps) }
    pub fn pause(env: Env, admin: Address) -> Result<(), types::FactoryError> { contract::pause(&env, &admin) }
    pub fn unpause(env: Env, admin: Address) -> Result<(), types::FactoryError> { contract::unpause(&env, &admin) }
    pub fn get_templates(env: Env) -> soroban_sdk::Vec<types::CircleTemplate> { contract::get_templates(&env) }
    pub fn get_template(env: Env, template_id: u32) -> Result<types::CircleTemplate, types::FactoryError> { contract::get_template(&env, template_id) }
    pub fn create_template(env: Env, admin: Address, template: types::CircleTemplate) -> Result<(), types::FactoryError> { contract::create_template(&env, &admin, &template) }
    pub fn deploy_from_template(env: Env, template_id: u32, organizer: Address, token: Address, name: soroban_sdk::String, slug: soroban_sdk::String) -> Result<Address, types::FactoryError> { contract::deploy_from_template(&env, template_id, &organizer, &token, name, slug) }
    pub fn deploy_from_template_custom(env: Env, template_id: u32, config: types::CircleConfig) -> Result<Address, types::FactoryError> { contract::deploy_from_template_custom(&env, template_id, &config) }
    pub fn migrate_circles(env: Env, caller: Address, start_index: u32, limit: u32) -> Result<u32, types::FactoryError> { contract::migrate_circles(&env, &caller, start_index, limit) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_smoke_compile() { assert!(true); }
    #[test]
    fn test_types_compile() {
        // Verify contract types compile correctly
        assert!(true);
    }
}

