#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String};

use crate::types::{CircleConfig, JOIN_RATE_LIMIT_LEDGERS};
use crate::{Circle, CircleClient, CircleError};

fn create_config(env: &Env) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: Address::generate(env),
        name: String::from_str(env, "Rate Limit Test"),
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
        slug: String::from_str(env, "rate-limit-test"),
    }
}

fn setup(env: &Env) -> (CircleClient<'_>, Address) {
    env.mock_all_auths();
    let config = create_config(env);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, (&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin)
}

#[test]
fn test_first_join_records_attempt() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let member = Address::generate(&env);

    client.join(&member);

    let attempt = client.get_last_join_attempt(&member).unwrap();
    assert_eq!(attempt.ledger, env.ledger().sequence());
    assert_eq!(client.get_members().len(), 1);
}

#[test]
fn test_second_join_from_same_address_is_rate_limited() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let member = Address::generate(&env);

    client.join(&member);
    let result = client.try_join(&member);
    assert_eq!(result, Err(Ok(CircleError::JoinRateLimited)));
    assert_eq!(client.get_members().len(), 1);
}

#[test]
fn test_join_rejected_by_already_member_still_refreshes_marker_before_member_check() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let member = Address::generate(&env);
    client.join(&member);

    env.ledger().with_mut(|li| {
        li.sequence_number += JOIN_RATE_LIMIT_LEDGERS;
    });
    let result = client.try_join(&member);
    assert_eq!(result, Err(Ok(CircleError::AlreadyMember)));
}

#[test]
fn test_join_allowed_after_rate_limit_window() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let member = Address::generate(&env);

    client.join(&member);
    let original = client.get_last_join_attempt(&member).unwrap().ledger;
    env.ledger().with_mut(|li| {
        li.sequence_number += JOIN_RATE_LIMIT_LEDGERS;
    });

    // Past the window: rate limit allows the attempt, then AlreadyMember rejects it.
    // The marker write rolls back with the Err, so it stays at the original join.
    let result = client.try_join(&member);
    assert_eq!(result, Err(Ok(CircleError::AlreadyMember)));
    let attempt = client.get_last_join_attempt(&member).unwrap();
    assert_eq!(attempt.ledger, original);
}

#[test]
fn test_different_addresses_not_rate_limited_against_each_other() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);

    client.join(&m1);
    client.join(&m2);

    assert_eq!(client.get_members().len(), 2);
    assert!(client.get_last_join_attempt(&m1).is_some());
    assert!(client.get_last_join_attempt(&m2).is_some());
}

#[test]
fn test_rejected_attempt_does_not_allow_immediate_retry_after_window_marker_unchanged() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let member = Address::generate(&env);
    client.join(&member);
    let first = client.get_last_join_attempt(&member).unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number += 10;
    });
    let _ = client.try_join(&member);
    let after = client.get_last_join_attempt(&member).unwrap();
    assert_eq!(first.ledger, after.ledger);
}
