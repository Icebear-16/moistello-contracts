#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{Circle as CircleState, CircleConfig, DataKey, Member, MEMBER_DEFAULTED};
use crate::{Circle, CircleClient, CircleError};

fn create_config(env: &Env) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: Address::generate(env),
        name: String::from_str(env, "Migration Test Circle"),
        contribution_amount: 100_0000000i128,
        max_members: 5u32,
        payout_type: 1u32,
        total_rounds: 5u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "migration-test"),
    }
}

fn setup_circle(env: &Env) -> (CircleClient<'_>, Address, Address, CircleConfig) {
    env.mock_all_auths();
    let config = create_config(env);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, (&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin, factory, config)
}

fn setup_legacy_circle(env: &Env) -> (CircleClient<'_>, Address, Address, CircleConfig) {
    env.mock_all_auths();
    let config = create_config(env);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, (&admin, &factory, &config));
    let client = CircleClient::new(env, &contract_id);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&crate::types::DataKey::StorageVersion);
    });
    (client, admin, factory, config)
}

#[test]
fn test_new_circle_starts_at_current_storage_version() {
    let env = Env::default();
    let (client, _, _, _) = setup_circle(&env);
    assert_eq!(client.get_storage_version(), 2);
    client.verify_state();
}

#[test]
fn test_legacy_circle_reports_version_1_and_migration_pending() {
    let env = Env::default();
    let (client, _, _, _) = setup_legacy_circle(&env);
    assert_eq!(client.get_storage_version(), 1);
    assert_eq!(
        client.try_verify_state(),
        Err(Ok(CircleError::MigrationPending))
    );
}

#[test]
fn test_migrate_legacy_circle_success() {
    let env = Env::default();
    let (client, admin, _, _) = setup_legacy_circle(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);

    client.migrate(&admin);

    assert_eq!(client.get_storage_version(), 2);
    client.verify_state();
    let analytics = client.get_analytics();
    assert_eq!(analytics.total_members, 2);
    assert_eq!(client.get_all_member_stats().len(), 2);
}

#[test]
fn test_migrate_is_idempotent_at_current_version() {
    let env = Env::default();
    let (client, admin, _, _) = setup_circle(&env);
    client.migrate(&admin);
    client.migrate(&admin);
    assert_eq!(client.get_storage_version(), 2);
    client.verify_state();
}

#[test]
fn test_migrate_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _, _, _) = setup_legacy_circle(&env);
    let outsider = Address::generate(&env);
    assert_eq!(
        client.try_migrate(&outsider),
        Err(Ok(CircleError::Unauthorized))
    );
}

#[test]
fn test_migrate_allowed_for_factory_caller() {
    let env = Env::default();
    let (client, _, factory, _) = setup_legacy_circle(&env);
    client.migrate(&factory);
    assert_eq!(client.get_storage_version(), 2);
    client.verify_state();
}

#[test]
fn test_migrate_rejects_incompatible_future_version() {
    let env = Env::default();
    let (client, admin, _, _) = setup_circle(&env);
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&crate::types::DataKey::StorageVersion, &3u32);
    });
    assert_eq!(
        client.try_migrate(&admin),
        Err(Ok(CircleError::IncompatibleStorageVersion))
    );
}

#[test]
fn test_migrate_rolls_back_on_validation_failure() {
    let env = Env::default();
    let (client, admin, _, _) = setup_legacy_circle(&env);
    client.join(&Address::generate(&env));
    client.join(&Address::generate(&env));

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &1u32);
        let mut c: CircleState = env
            .storage()
            .instance()
            .get(&DataKey::Circle)
            .expect("circle");
        c.member_count = 99;
        env.storage().instance().set(&DataKey::Circle, &c);
    });

    assert_eq!(
        client.try_migrate(&admin),
        Err(Ok(CircleError::MigrationValidationFailed))
    );
    assert_eq!(client.get_storage_version(), 1);
}

#[test]
fn test_migrate_recompute_counts_defaults() {
    let env = Env::default();
    let (client, admin, _, config) = setup_legacy_circle(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);
    client.join(&m3);

    env.as_contract(&client.address, || {
        let mut members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .expect("members");
        let mut m = members.get(0).expect("member");
        m.status = MEMBER_DEFAULTED;
        members.set(0, m);
        env.storage().persistent().set(&DataKey::Members, &members);
        let _ = &config;
    });

    client.migrate(&admin);
    let analytics = client.get_analytics();
    assert_eq!(analytics.total_defaults, 1);
    client.verify_state();
}
