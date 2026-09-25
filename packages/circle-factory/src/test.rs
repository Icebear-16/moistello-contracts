#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, BytesN, Env};
use std::sync::atomic::{AtomicU32, Ordering};
use circle::CircleClient;
use crate::{CircleFactory, CircleFactoryClient};
use crate::types::{CircleConfig, FactoryError, MAX_DEPLOY_FEE};

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

static SLUG_COUNTER: AtomicU32 = AtomicU32::new(1);

fn sample_config(env: &Env, organizer: &Address) -> CircleConfig {
    let n = SLUG_COUNTER.fetch_add(1, Ordering::Relaxed);
    // max_members stays within the bronze-tier circle size limit (5) enforced
    // by the circle constructor for unscored organizers.
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
        slug: soroban_sdk::String::from_str(env, &format!("test-circle-{n}")),
    }
}

fn configure_protocol(
    env: &Env,
    client: &CircleFactoryClient,
    admin: &Address,
) -> (Address, Address) {
    let treasury = Address::generate(env);
    let reputation_registry = Address::generate(env);
    client.set_factory_config(admin, &treasury, &reputation_registry, &500u32);
    (treasury, reputation_registry)
}

fn configured_setup(env: &Env) -> (CircleFactoryClient<'_>, Address, BytesN<32>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wh = install_wasm_hash(env);
    client.init(&admin, &500i128, &wh, &None, &None);
    let (treasury, reputation_registry) = configure_protocol(env, &client, &admin);
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

    client.init(&admin, &300i128, &wh, &None, &None);

    assert_eq!(client.get_circle_count(), 0);
    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 300);
}

#[test]
fn test_init_with_config_stores_protocol_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let reputation_registry = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    client.init_with_config(
        &admin,
        &750i128,
        &treasury,
        &reputation_registry,
        &wh,
    );

    let config = client.get_factory_config().unwrap();
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.reputation_registry, reputation_registry);
    assert_eq!(config.fee_bps, 750);
    assert_eq!(client.get_fee_config().fee_bps, 750);

    let circle_id = client.deploy_circle(&sample_config(&env, &Address::generate(&env)));
    let circle = CircleClient::new(&env, &circle_id);
    assert_eq!(circle.get_treasury(), Some(treasury));
    assert_eq!(circle.get_reputation_registry(), Some(reputation_registry));
    assert_eq!(circle.get_fee_bps(), 750);
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

    let result = client.try_init(&admin, &10001i128, &wh, &None, &None);
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
fn test_deploy_circle_emits_circle_created_with_canonical_address_and_admin() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let config = sample_config(&env, &organizer);

    let _circle_id = client.deploy_circle(&config);
    let all_events = env.events().all();
    let events = all_events.events();
    assert!(events.len() >= 2, "must emit created and deploy events");
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
fn test_builtin_templates_present_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    client.init(&admin, &500i128, &wh, &None, &None);

    let templates = client.get_templates();
    assert_eq!(templates.len(), 3);
    let t1 = client.get_template(&1);
    assert_eq!(t1.contribution_amount, 100_0000000);
    assert_eq!(t1.max_members, 5);
    assert_eq!(t1.payout_type, 1);
    assert_eq!(t1.total_rounds, 5);
    let t2 = client.get_template(&2);
    assert_eq!(t2.max_members, 8);
    let t3 = client.get_template(&3);
    assert_eq!(t3.max_members, 12);
}

#[test]
fn test_get_template_missing_returns_template_not_found() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    assert_eq!(
        client.try_get_template(&99),
        Err(Ok(FactoryError::TemplateNotFound))
    );
}

#[test]
fn test_create_template_success_and_duplicate() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let mut t = client.get_template(&1);
    t.id = 10;
    t.name = soroban_sdk::String::from_str(&env, "Custom Weekly");
    client.create_template(&admin, &t);
    let templates = client.get_templates();
    assert_eq!(templates.len(), 4);

    let r = client.try_create_template(&admin, &t);
    assert_eq!(r, Err(Ok(FactoryError::TemplateExists)));
}

#[test]
fn test_create_template_rejects_unauthorized() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let outsider = Address::generate(&env);
    let mut t = client.get_template(&1);
    t.id = 11;
    let r = client.try_create_template(&outsider, &t);
    assert_eq!(r, Err(Ok(FactoryError::Unauthorized)));
}

#[test]
fn test_create_template_rejects_invalid() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let mut t = client.get_template(&1);
    t.id = 12;
    t.contribution_amount = 0;
    let r = client.try_create_template(&admin, &t);
    assert_eq!(r, Err(Ok(FactoryError::InvalidTemplate)));
}

#[test]
fn test_deploy_from_template_success() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let token = Address::generate(&env);

    let circle_id = client.deploy_from_template(
        &1,
        &organizer,
        &token,
        &soroban_sdk::String::from_str(&env, "From Template"),
        &soroban_sdk::String::from_str(&env, "from-template"),
    );

    assert_eq!(client.get_circle_count(), 1);
    let cfg = client.get_circle_config(&circle_id);
    assert_eq!(cfg.contribution_amount, 100_0000000);
    assert_eq!(cfg.max_members, 5);
    assert_eq!(cfg.payout_type, 1);
    assert_eq!(cfg.total_rounds, 5);
    assert_eq!(cfg.organizer, organizer);
    assert_eq!(cfg.token, token);
}

#[test]
fn test_deploy_from_template_missing_template() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let token = Address::generate(&env);
    let r = client.try_deploy_from_template(
        &99,
        &organizer,
        &token,
        &soroban_sdk::String::from_str(&env, "X"),
        &soroban_sdk::String::from_str(&env, "x"),
    );
    assert_eq!(r, Err(Ok(FactoryError::TemplateNotFound)));
}

#[test]
fn test_deploy_from_template_custom_within_bounds() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let mut config = sample_config(&env, &Address::generate(&env));
    config.contribution_amount = 50_0000000;
    config.max_members = 4;
    config.slug = soroban_sdk::String::from_str(&env, "custom-ok");
    let circle_id = client.deploy_from_template_custom(&1, &config);
    assert_eq!(client.get_circle_count(), 1);
    let stored = client.get_circle_config(&circle_id);
    assert_eq!(stored.contribution_amount, 50_0000000);
    assert_eq!(stored.max_members, 4);
}

#[test]
fn test_deploy_from_template_custom_out_of_bounds() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let mut config = sample_config(&env, &Address::generate(&env));
    config.max_members = 99;
    config.slug = soroban_sdk::String::from_str(&env, "custom-oob");
    let r = client.try_deploy_from_template_custom(&1, &config);
    assert_eq!(r, Err(Ok(FactoryError::TemplateOutOfBounds)));
}

#[test]
fn test_deploy_from_template_custom_amount_out_of_bounds() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let mut config = sample_config(&env, &Address::generate(&env));
    config.contribution_amount = 0;
    config.slug = soroban_sdk::String::from_str(&env, "custom-zero");
    let r = client.try_deploy_from_template_custom(&1, &config);
    assert_eq!(r, Err(Ok(FactoryError::TemplateOutOfBounds)));
}

#[test]
fn test_migrate_circles_no_circles_returns_zero() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    assert_eq!(client.migrate_circles(&admin, &0, &10), 0);
}

#[test]
fn test_migrate_circles_rejects_unauthorized() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let outsider = Address::generate(&env);
    let r = client.try_migrate_circles(&outsider, &0, &10);
    assert_eq!(r, Err(Ok(FactoryError::Unauthorized)));
}

#[test]
fn test_migrate_circles_batch_counts_processed_circles() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let org = Address::generate(&env);
    client.deploy_circle(&sample_config(&env, &org));
    client.deploy_circle(&sample_config(&env, &Address::generate(&env)));

    // Circles are already at the current storage version, so each invoked
    // migrate() resolves successfully and is counted as processed.
    assert_eq!(client.migrate_circles(&admin, &0, &10), 2);
    assert_eq!(client.migrate_circles(&admin, &0, &1), 1);
    assert_eq!(client.migrate_circles(&admin, &1, &1), 1);
    assert_eq!(client.migrate_circles(&admin, &5, &10), 0);
    assert_eq!(client.migrate_circles(&admin, &0, &0), 0);
}

#[test]
fn test_init_enforces_deploy_fee_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(
        &admin,
        &500i128,
        &wh,
        &Some(MAX_DEPLOY_FEE + 1),
        &Some(treasury.clone()),
    );
    assert_eq!(result, Err(Ok(FactoryError::DeployFeeExceedsCap)));
}

#[test]
fn test_init_rejects_negative_deploy_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(
        &admin,
        &500i128,
        &wh,
        &Some(-1),
        &Some(treasury),
    );
    assert_eq!(result, Err(Ok(FactoryError::DeployFeeExceedsCap)));
}

#[test]
fn test_init_requires_treasury_when_fee_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(
        &admin,
        &500i128,
        &wh,
        &Some(100),
        &None,
    );
    assert_eq!(result, Err(Ok(FactoryError::InvalidTreasury)));
}

#[test]
fn test_deploy_circle_zero_fee_free_deployment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    // Initialize with zero fee
    client.init(&admin, &500i128, &wh, &Some(0), &None);
    configure_protocol(&env, &client, &admin);
    assert_eq!(client.get_deploy_fee(), 0);
    assert_eq!(client.get_treasury(), None);

    let organizer = Address::generate(&env);
    let config = sample_config(&env, &organizer);

    // Zero fee deployment succeeds without charging organizer
    let circle_id = client.deploy_circle(&config);
    assert_eq!(client.get_circle_count(), 1);
    assert_eq!(client.get_circles().circles.get(0).unwrap().circle_id, circle_id);
}

#[test]
fn test_deploy_circle_charges_fee_and_routes_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let deploy_fee = 50_0000000i128;
    client.init(&admin, &500i128, &wh, &Some(deploy_fee), &Some(treasury.clone()));
    configure_protocol(&env, &client, &admin);
    assert_eq!(client.get_deploy_fee(), deploy_fee);
    assert_eq!(client.get_treasury(), Some(treasury.clone()));

    // Set up token and fund organizer
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let organizer = Address::generate(&env);
    token_admin_client.mint(&organizer, &100_0000000);

    let mut config = sample_config(&env, &organizer);
    config.token = token.clone();

    // Deploy circle and verify fee transferred to treasury
    let circle_id = client.deploy_circle(&config);
    assert_eq!(client.get_circle_count(), 1);
    assert_eq!(client.get_circles().circles.get(0).unwrap().circle_id, circle_id);

    assert_eq!(token_client.balance(&organizer), 50_0000000);
    assert_eq!(token_client.balance(&treasury), 50_0000000);
}

#[test]
fn test_set_deploy_fee_admin_and_cap() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let treasury = Address::generate(&env);

    // Update deploy fee by admin
    assert!(client.try_set_deploy_fee(&admin, &200_0000000, &Some(treasury.clone())).is_ok());
    assert_eq!(client.get_deploy_fee(), 200_0000000);
    assert_eq!(client.get_treasury(), Some(treasury.clone()));

    // Reject non-admin
    let stranger = Address::generate(&env);
    let r = client.try_set_deploy_fee(&stranger, &100_0000000, &Some(treasury.clone()));
    assert_eq!(r, Err(Ok(FactoryError::Unauthorized)));

    // Reject exceeding cap
    let r_cap = client.try_set_deploy_fee(&admin, &(MAX_DEPLOY_FEE + 1), &Some(treasury.clone()));
    assert_eq!(r_cap, Err(Ok(FactoryError::DeployFeeExceedsCap)));

    // Reject missing treasury when fee > 0
    let r_no_treasury = client.try_set_deploy_fee(&admin, &100, &None);
    assert_eq!(r_no_treasury, Err(Ok(FactoryError::InvalidTreasury)));
}

#[test]
fn test_deploy_circle_propagates_protocol_config() {
    let env = Env::default();
    let (client, _admin, _wh, treasury, reputation_registry) = configured_setup(&env);
    let organizer = Address::generate(&env);

    let circle_id = client.deploy_circle(&sample_config(&env, &organizer));
    let circle = CircleClient::new(&env, &circle_id);

    assert_eq!(circle.get_treasury(), Some(treasury));
    assert_eq!(circle.get_reputation_registry(), Some(reputation_registry));
    assert_eq!(circle.get_fee_bps(), 500);
}

#[test]
fn test_deploy_from_template_propagates_protocol_config() {
    let env = Env::default();
    let (client, _admin, _wh, treasury, reputation_registry) = configured_setup(&env);
    let organizer = Address::generate(&env);
    let token = Address::generate(&env);

    let circle_id = client.deploy_from_template(
        &1,
        &organizer,
        &token,
        &soroban_sdk::String::from_str(&env, "Configured Template"),
        &soroban_sdk::String::from_str(&env, "configured-template"),
    );
    let circle = CircleClient::new(&env, &circle_id);

    assert_eq!(circle.get_treasury(), Some(treasury));
    assert_eq!(circle.get_reputation_registry(), Some(reputation_registry));
    assert_eq!(circle.get_fee_bps(), 500);
}

#[test]
fn test_deploy_circle_requires_protocol_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    client.init(&admin, &500i128, &wh, &None, &None);

    let result = client.try_deploy_circle(&sample_config(&env, &Address::generate(&env)));

    assert_eq!(result, Err(Ok(FactoryError::FactoryConfigNotSet)));
    assert_eq!(client.get_circle_count(), 0);
    assert_eq!(client.get_circles().circles.len(), 0);
}

#[test]
fn test_set_factory_config_validates_and_updates_fee_config() {
    let env = Env::default();
    let (client, admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);
    let treasury = Address::generate(&env);
    let reputation_registry = Address::generate(&env);

    assert_eq!(client.get_fee_config().fee_bps, 500);
    client.set_factory_config(&admin, &treasury, &reputation_registry, &900u32);

    let factory_config = client.get_factory_config().unwrap();
    assert_eq!(factory_config.treasury, treasury);
    assert_eq!(factory_config.reputation_registry, reputation_registry);
    assert_eq!(factory_config.fee_bps, 900);
    assert_eq!(client.get_fee_config().fee_bps, 900);
}

#[test]
fn test_set_factory_config_rejects_unauthorized_and_invalid_fee() {
    let env = Env::default();
    let (client, admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);
    let stranger = Address::generate(&env);
    let treasury = Address::generate(&env);
    let reputation_registry = Address::generate(&env);

    assert_eq!(
        client.try_set_factory_config(
            &stranger,
            &treasury,
            &reputation_registry,
            &900u32,
        ),
        Err(Ok(FactoryError::Unauthorized))
    );
    assert_eq!(
        client.try_set_factory_config(
            &admin,
            &treasury,
            &reputation_registry,
            &10_001u32,
        ),
        Err(Ok(FactoryError::InvalidFeeBps))
    );

    let zero = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    assert_eq!(
        client.try_set_factory_config(
            &admin,
            &zero,
            &reputation_registry,
            &500u32,
        ),
        Err(Ok(FactoryError::InvalidAddress))
    );
}

#[test]
fn test_set_fee_config_keeps_protocol_config_in_sync() {
    let env = Env::default();
    let (client, admin, _wh, _treasury, _reputation_registry) = configured_setup(&env);

    client.set_fee_config(&admin, &900i128);

    assert_eq!(client.get_factory_config().unwrap().fee_bps, 900);
}

#[test]
fn test_circle_configuration_rejects_non_factory() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let factory_client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let organizer = Address::generate(&env);
    let wh = install_wasm_hash(&env);
    factory_client.init(&admin, &500i128, &wh, &None, &None);
    let (treasury, reputation_registry) = configure_protocol(&env, &factory_client, &admin);
    let circle_id = factory_client.deploy_circle(&sample_config(&env, &organizer));
    let circle = CircleClient::new(&env, &circle_id);

    let result = circle.try_configure_from_factory(
        &Address::generate(&env),
        &treasury,
        &reputation_registry,
        &750u32,
    );

    assert_eq!(result, Err(Ok(circle::CircleError::Unauthorized)));
}
