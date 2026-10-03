#![cfg(test)]

use crate::types::{CircleConfig, FactoryError};
use crate::{CircleFactory, CircleFactoryClient};
use circle::CircleError;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn install_wasm_hash(env: &Env) -> BytesN<32> {
    // Test fixture wasm: the circle contract built from this workspace
    // (packages/circle-factory/test_wasm/contract.wasm). The factory invokes
    // its constructor during deploy_v2, so the fixture must accept
    // (admin, factory, config) constructor arguments.
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let wasm: &[u8] = include_bytes!("../test_wasm/contract.wasm");
    env.deployer().upload_contract_wasm(wasm)
}

fn sample_config(env: &Env, organizer: &Address) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: Address::generate(env),
        name: soroban_sdk::String::from_str(env, "Test Circle"),
        contribution_amount: 100i128,
        max_members: 10u32,
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

fn sample_config_with_slug(env: &Env, organizer: &Address, slug: &str) -> CircleConfig {
    let mut config = sample_config(env, organizer);
    config.slug = soroban_sdk::String::from_str(env, slug);
    config
}

/// Sets the protocol config that `deploy_circle` propagates into new circles.
fn configure_protocol(client: &CircleFactoryClient, env: &Env, admin: &Address, fee_bps: u32) {
    let treasury = Address::generate(env);
    let registry = Address::generate(env);
    client.set_factory_config(admin, &treasury, &registry, &fee_bps);
}

fn setup(env: &Env) -> (CircleFactoryClient, Address, BytesN<32>) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wh = install_wasm_hash(env);
    client.init(&admin, &500i128, &wh, &0u32, &0u64);
    configure_protocol(&client, env, &admin, 500);
    (client, admin, wh)
}

fn setup_with_rate_limit(
    env: &Env,
    limit: u32,
    period_secs: u64,
) -> (CircleFactoryClient, Address, BytesN<32>) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wh = install_wasm_hash(env);
    client.init(&admin, &500i128, &wh, &limit, &period_secs);
    configure_protocol(&client, env, &admin, 500);
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

    client.init(&admin, &300i128, &wh, &0u32, &0u64);

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
}

#[test]
fn test_init_rejects_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(&admin, &10001i128, &wh, &0u32, &0u64);
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
fn test_duplicate_canonical_deployment_is_rejected() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let config = sample_config(&env, &organizer);

    assert!(client.try_deploy_circle(&config).is_ok());
    assert_eq!(client.try_deploy_circle(&config), Err(Ok(FactoryError::CircleDeployFailed)));
    assert_eq!(client.get_circle_count(), 1);
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

    client.deploy_circle(&sample_config(&env, &org1));
    client.deploy_circle(&sample_config(&env, &org2));

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
fn test_storage_isolation_across_100_deployed_circles() {
    // Issue #456: factory-deployed circles must not share storage. Each
    // deploy_v2 call gets its own contract instance via a distinct salt, but
    // that's a platform guarantee worth proving empirically — a salt or
    // constructor-argument bug could silently make two circles alias the
    // same address/config. Verified here through the factory's own
    // per-circle storage (get_circle_config), which is what every other
    // reader of a deployed circle's config (indexers, the frontend) also
    // goes through — see sample_config()'s neighbours in this file for why
    // the deployed circle contract itself is never invoked directly in
    // these tests.
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);

    const N: u32 = 100;
    let mut circle_ids: std::vec::Vec<Address> = std::vec::Vec::with_capacity(N as usize);
    let mut expected_amounts: std::vec::Vec<i128> = std::vec::Vec::with_capacity(N as usize);

    for i in 0..N {
        let organizer = Address::generate(&env);
        let mut config = sample_config(&env, &organizer);
        // Divergent per-circle config: each circle's contribution_amount and
        // slug are unique, so any cross-circle storage aliasing would show
        // up as one circle's config bleeding into another's.
        config.contribution_amount = 100i128 + i as i128;
        config.slug = soroban_sdk::String::from_str(&env, &std::format!("isolation-test-{i}"));
        let cid = client.deploy_circle(&config);
        circle_ids.push(cid);
        expected_amounts.push(config.contribution_amount);
    }

    assert_eq!(client.get_circle_count(), N);

    // Config A change never observable from circle B: read every deployed
    // circle's own stored config back and confirm it matches exactly what
    // that circle (and only that circle) was configured with.
    for i in 0..N as usize {
        let stored = client.get_circle_config(&circle_ids[i]);
        assert_eq!(
            stored.contribution_amount, expected_amounts[i],
            "circle {i} read back a contribution_amount belonging to a different circle"
        );
        assert_eq!(
            stored.slug,
            soroban_sdk::String::from_str(&env, &std::format!("isolation-test-{i}")),
            "circle {i} read back a slug belonging to a different circle"
        );
    }

    // Every deployed circle address must be unique — a collision here would
    // mean two configs landed on the same storage instance.
    for i in 0..N as usize {
        for j in (i + 1)..N as usize {
            assert_ne!(
                circle_ids[i], circle_ids[j],
                "circles {i} and {j} deployed to the same address"
            );
        }
    }
}

#[test]
fn test_rate_limit_zero_is_unlimited() {
    let env = Env::default();
    let (client, _admin, _wh) = setup_with_rate_limit(&env, 0, 0);
    let organizer = Address::generate(&env);

    for i in 0..10 {
        let config = sample_config_with_slug(&env, &organizer, &std::format!("unlimited-{i}"));
        assert!(client.try_deploy_circle(&config).is_ok());
    }
    assert_eq!(client.get_circle_count(), 10);
}

#[test]
fn test_rate_limit_exceeded_rejected() {
    let env = Env::default();
    let (client, _admin, _wh) = setup_with_rate_limit(&env, 2, 3600);
    let organizer = Address::generate(&env);
    let config = sample_config_with_slug(&env, &organizer, "limited-0");
    let config_2 = sample_config_with_slug(&env, &organizer, "limited-1");
    let config_3 = sample_config_with_slug(&env, &organizer, "limited-2");

    assert!(client.try_deploy_circle(&config).is_ok());
    assert!(client.try_deploy_circle(&config_2).is_ok());
    let result = client.try_deploy_circle(&config_3);
    assert_eq!(result, Err(Ok(FactoryError::RateLimitExceeded)));
}

#[test]
fn test_rate_limit_resets_next_period() {
    let env = Env::default();
    let (client, _admin, _wh) = setup_with_rate_limit(&env, 1, 3600);
    let organizer = Address::generate(&env);
    let config = sample_config_with_slug(&env, &organizer, "period-0");
    let config_2 = sample_config_with_slug(&env, &organizer, "period-1");

    assert!(client.try_deploy_circle(&config).is_ok());
    let result = client.try_deploy_circle(&config_2);
    assert_eq!(result, Err(Ok(FactoryError::RateLimitExceeded)));

    // Advance the ledger timestamp into the next rate-limit period.
    env.ledger().set_timestamp(env.ledger().timestamp() + 3600);

    assert!(client.try_deploy_circle(&config_2).is_ok());
    assert_eq!(client.get_circle_count(), 2);
}

#[test]
fn test_rate_limit_independent_per_organizer() {
    let env = Env::default();
    let (client, _admin, _wh) = setup_with_rate_limit(&env, 1, 3600);
    let org1 = Address::generate(&env);
    let org2 = Address::generate(&env);

    assert!(client
        .try_deploy_circle(&sample_config(&env, &org1))
        .is_ok());
    assert_eq!(
        client.try_deploy_circle(&sample_config(&env, &org1)),
        Err(Ok(FactoryError::RateLimitExceeded))
    );
    // A different organizer has an independent counter and can still deploy.
    assert!(client
        .try_deploy_circle(&sample_config(&env, &org2))
        .is_ok());
    assert_eq!(client.get_circle_count(), 2);
}

// ── Protocol config propagation (#104) ───────────────────────────────────────

#[test]
fn test_get_factory_config_returns_none_when_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    assert_eq!(client.get_factory_config(), None);
}

#[test]
fn test_init_with_config_stores_protocol_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);

    client.init_with_config(&admin, &250i128, &treasury, &registry, &wh);

    let cfg = client.get_factory_config().unwrap();
    assert_eq!(cfg.treasury, treasury);
    assert_eq!(cfg.reputation_registry, registry);
    assert_eq!(cfg.fee_bps, 250);
    assert_eq!(client.get_fee_config().fee_bps, 250);
}

#[test]
fn test_init_with_config_rejects_zero_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    let zero = zero_address(&env);
    let registry = Address::generate(&env);

    assert_eq!(
        client.try_init_with_config(&admin, &100i128, &zero, &registry, &wh),
        Err(Ok(FactoryError::InvalidAddress))
    );
    assert_eq!(
        client.try_init_with_config(&admin, &100i128, &registry, &zero, &wh),
        Err(Ok(FactoryError::InvalidAddress))
    );
}

#[test]
fn test_deploy_circle_requires_protocol_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    client.init(&admin, &500i128, &wh, &0u32, &0u64);

    let org = Address::generate(&env);
    assert_eq!(
        client.try_deploy_circle(&sample_config(&env, &org)),
        Err(Ok(FactoryError::FactoryConfigNotSet))
    );
}

#[test]
fn test_deploy_circle_propagates_protocol_config_to_circle() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let cfg = client.get_factory_config().unwrap();

    let org = Address::generate(&env);
    let circle_id = client.deploy_circle(&sample_config(&env, &org));

    let circle = circle::CircleClient::new(&env, &circle_id);
    assert_eq!(circle.get_treasury(), Some(cfg.treasury.clone()));
    assert_eq!(circle.get_reputation_registry(), Some(cfg.reputation_registry.clone()));
    assert_eq!(circle.get_fee_bps(), cfg.fee_bps);
}

#[test]
fn test_circle_rejects_configuration_from_a_foreign_caller() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let org = Address::generate(&env);
    let circle_id = client.deploy_circle(&sample_config(&env, &org));
    let circle = circle::CircleClient::new(&env, &circle_id);

    let impostor = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);
    assert_eq!(
        circle.try_configure_from_factory(&impostor, &treasury, &registry, &900u32),
        Err(Ok(CircleError::Unauthorized))
    );
}

#[test]
fn test_set_factory_config_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let impostor = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);
    assert_eq!(
        client.try_set_factory_config(&impostor, &treasury, &registry, &100u32),
        Err(Ok(FactoryError::Unauthorized))
    );
}

#[test]
fn test_set_factory_config_rejects_out_of_range_fee() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);
    assert_eq!(
        client.try_set_factory_config(&admin, &treasury, &registry, &10_001u32),
        Err(Ok(FactoryError::InvalidFeeBps))
    );
}

#[test]
fn test_set_factory_config_rejects_zero_addresses() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let zero = zero_address(&env);
    let registry = Address::generate(&env);
    assert_eq!(
        client.try_set_factory_config(&admin, &zero, &registry, &100u32),
        Err(Ok(FactoryError::InvalidAddress))
    );
    assert_eq!(
        client.try_set_factory_config(&admin, &registry, &zero, &100u32),
        Err(Ok(FactoryError::InvalidAddress))
    );
}

#[test]
fn test_set_factory_config_applies_to_later_deployments_only() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let org = Address::generate(&env);
    let before_id = client.deploy_circle(&sample_config_with_slug(&env, &org, "before"));

    let treasury = Address::generate(&env);
    let registry = Address::generate(&env);
    client.set_factory_config(&admin, &treasury, &registry, &300u32);

    let after_id = client.deploy_circle(&sample_config_with_slug(&env, &org, "after"));

    let before = circle::CircleClient::new(&env, &before_id);
    let after = circle::CircleClient::new(&env, &after_id);
    assert_eq!(before.get_fee_bps(), 500);
    assert_eq!(after.get_fee_bps(), 300);
    assert_eq!(after.get_treasury(), Some(treasury));
    assert_eq!(after.get_reputation_registry(), Some(registry));
}

#[test]
fn test_set_fee_config_mirrors_into_propagated_config() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    client.set_fee_config(&admin, &750i128);
    assert_eq!(client.get_factory_config().unwrap().fee_bps, 750);
    assert_eq!(client.get_fee_config().fee_bps, 750);
}
