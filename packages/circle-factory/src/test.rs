#![cfg(test)]

use crate::types::{CircleConfig, FactoryError};
use crate::{CircleFactory, CircleFactoryClient};
use circle::CircleClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

fn install_wasm_hash(env: &Env) -> BytesN<32> {
    let wasm: &[u8] = include_bytes!("../test_wasm/contract.wasm");
    env.deployer().upload_contract_wasm(wasm)
}

fn sample_config(env: &Env, organizer: &Address) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: Address::generate(env),
        name: soroban_sdk::String::from_str(env, "Test Circle"),
        contribution_amount: 100i128,
        max_members: 5u32,
        payout_type: 0u32,
        total_rounds: 5u32,
        contribution_deadline_seconds: 86400u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 3600u64,
        max_strikes: 3u32,
        slug: soroban_sdk::String::from_str(env, "test-circle"),
    }
}

fn configured_setup(env: &Env) -> (CircleFactoryClient<'_>, Address, BytesN<32>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wh = install_wasm_hash(env);
    client.init(&admin, &500i128, &wh);
    let treasury = Address::generate(env);
    let reputation_registry = Address::generate(env);
    client.set_factory_config(&admin, &treasury, &reputation_registry, &500u32);
    (client, admin, wh, treasury, reputation_registry)
}

fn setup(env: &Env) -> (CircleFactoryClient<'_>, Address, BytesN<32>) {
    let (client, admin, wh, _, _) = configured_setup(env);
    (client, admin, wh)
}

#[test]
fn test_init_stores_admin_and_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    client.init(&admin, &300i128, &wh);

    assert_eq!(client.get_circle_count(), 0);
    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 300);
}

#[test]
fn test_get_fee_config_returns_default_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);

    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 0);
    assert_eq!(client.get_factory_config(), None);
}

#[test]
fn test_init_rejects_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(&admin, &10001i128, &wh);
    assert_eq!(result, Err(Ok(FactoryError::InvalidFeeBps)));
}

#[test]
fn test_deploy_circle_success() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let config = sample_config(&env, &organizer);

    let circle_id = client.deploy_circle(&config);

    assert_eq!(client.get_circle_count(), 1);
    let registry = client.get_circles();
    assert_eq!(registry.circles.len(), 1);
    assert_eq!(registry.circles.get(0).unwrap().organizer, organizer);
    assert_eq!(registry.circles.get(0).unwrap().circle_id, circle_id);
}

#[test]
fn test_deploy_circle_rejects_invalid_config() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let mut config = sample_config(&env, &Address::generate(&env));
    config.max_members = 1;

    let result = client.try_deploy_circle(&config);
    assert_eq!(result, Err(Ok(FactoryError::InvalidConfig)));
}

#[test]
fn test_multiple_circles_increment_count() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let org1 = Address::generate(&env);
    let org2 = Address::generate(&env);

    let mut first = sample_config(&env, &org1);
    first.slug = soroban_sdk::String::from_str(&env, "test-circle-1");
    let mut second = sample_config(&env, &org2);
    second.slug = soroban_sdk::String::from_str(&env, "test-circle-2");
    client.deploy_circle(&first);
    client.deploy_circle(&second);

    assert_eq!(client.get_circle_count(), 2);
}

#[test]
fn test_deploy_circle_emits_event() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);

    client.deploy_circle(&sample_config(&env, &organizer));
}

#[test]
fn test_empty_circles() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    assert_eq!(client.get_circle_count(), 0);
    assert_eq!(client.get_circles().circles.len(), 0);
}

#[test]
fn test_set_fee_config_updates() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);

    client.set_fee_config(&admin, &750i128);

    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 750);
}

#[test]
fn test_set_fee_config_rejects_out_of_bounds() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);

    let r1 = client.try_set_fee_config(&admin, &-1i128);
    assert_eq!(r1, Err(Ok(FactoryError::InvalidFeeBps)));

    let r2 = client.try_set_fee_config(&admin, &10001i128);
    assert_eq!(r2, Err(Ok(FactoryError::InvalidFeeBps)));
}

#[test]
fn test_pause_unpause_blocks_deploy() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let config = sample_config(&env, &Address::generate(&env));

    client.pause(&admin);
    let r = client.try_deploy_circle(&config);
    assert_eq!(r, Err(Ok(FactoryError::ContractPaused)));

    client.unpause(&admin);
    assert!(client.try_deploy_circle(&config).is_ok());
}

#[test]
fn test_deploy_circle_propagates_factory_config() {
    let env = Env::default();
    let (client, _admin, _wh, treasury, reputation_registry) = configured_setup(&env);
    let organizer = Address::generate(&env);

    let circle_id = client.deploy_circle(&sample_config(&env, &organizer));
    let circle = CircleClient::new(&env, &circle_id);

    assert_eq!(circle.get_treasury(), Some(treasury));
    assert_eq!(circle.get_reputation_registry(), Some(reputation_registry));
    assert_eq!(circle.get_fee_bps(), 500);

    let result = circle.try_configure_from_factory(
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &500u32,
    );
    assert_eq!(result, Err(Ok(circle::CircleError::Unauthorized)));
}

#[test]
fn test_deploy_circle_requires_factory_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    client.init(&admin, &500i128, &wh);

    let result = client.try_deploy_circle(&sample_config(&env, &Address::generate(&env)));

    assert_eq!(result, Err(Ok(FactoryError::FactoryConfigNotSet)));
    assert_eq!(client.get_circle_count(), 0);
}

#[test]
fn test_set_factory_config_rejects_unauthorized() {
    let env = Env::default();
    let (client, _admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);
    let stranger = Address::generate(&env);

    let result = client.try_set_factory_config(
        &stranger,
        &Address::generate(&env),
        &Address::generate(&env),
        &500u32,
    );

    assert_eq!(result, Err(Ok(FactoryError::Unauthorized)));
}

#[test]
fn test_set_factory_config_rejects_invalid_fee() {
    let env = Env::default();
    let (client, admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);

    let result = client.try_set_factory_config(
        &admin,
        &Address::generate(&env),
        &Address::generate(&env),
        &10_001u32,
    );

    assert_eq!(result, Err(Ok(FactoryError::InvalidFeeBps)));
}

#[test]
fn test_init_with_config_stores_factory_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let reputation_registry = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    client.init_with_config(&admin, &750i128, &treasury, &reputation_registry, &wh);

    let config = client.get_factory_config().unwrap();
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.reputation_registry, reputation_registry);
    assert_eq!(config.fee_bps, 750);

    let circle_id = client.deploy_circle(&sample_config(&env, &Address::generate(&env)));
    let circle = CircleClient::new(&env, &circle_id);
    assert_eq!(circle.get_treasury(), Some(treasury));
    assert_eq!(circle.get_reputation_registry(), Some(reputation_registry));
    assert_eq!(circle.get_fee_bps(), 750);
}

#[test]
fn test_set_fee_config_updates_factory_config() {
    let env = Env::default();
    let (client, admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);

    client.set_fee_config(&admin, &900i128);

    assert_eq!(client.get_factory_config().unwrap().fee_bps, 900);
}
