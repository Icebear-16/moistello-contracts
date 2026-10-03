#![cfg(test)]

use crate::types::{
    ReputationError, ACTIVITY_COMPLETE, ACTIVITY_CONTRIBUTE, ACTIVITY_DEFAULT, ACTIVITY_JOIN,
    TIER_BRONZE, TIER_DIAMOND, TIER_GOLD, TIER_PLATINUM, TIER_SILVER,
};
use crate::{ReputationRegistry, ReputationRegistryClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(env: &Env) -> (ReputationRegistryClient, Address) {
    env.mock_all_auths();
    let contract_id = env.register(ReputationRegistry, ());
    let client = ReputationRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);
    (client, admin)
}

#[test]
fn test_init_and_default_score() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(ReputationRegistry, ());
    let client = ReputationRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let user = Address::generate(&env);
    let score = client.get_score(&user);
    assert_eq!(score.score, 0);
    assert_eq!(score.tier, TIER_BRONZE);
    assert_eq!(score.total_circles, 0);
}

#[test]
fn test_record_activity_increases_score() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.record_activity(&user, &ACTIVITY_JOIN, &100);

    let score = client.get_score(&user);
    assert_eq!(score.score, 100);
    assert_eq!(score.total_circles, 1);
}

#[test]
fn test_multiple_activities_accumulate() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.record_activity(&user, &ACTIVITY_JOIN, &200);
    client.record_activity(&user, &ACTIVITY_CONTRIBUTE, &150);
    client.record_activity(&user, &ACTIVITY_COMPLETE, &300);

    let score = client.get_score(&user);
    assert_eq!(score.score, 650);
    assert_eq!(score.total_circles, 1);
    assert_eq!(score.completed_circles, 1);
}

#[test]
fn test_score_caps_at_1000() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.record_activity(&user, &ACTIVITY_JOIN, &600);
    client.record_activity(&user, &ACTIVITY_CONTRIBUTE, &500);

    let score = client.get_score(&user);
    assert_eq!(score.score, 1000);
}

#[test]
fn test_default_resets_streak() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.record_activity(&user, &ACTIVITY_CONTRIBUTE, &100);
    client.record_activity(&user, &ACTIVITY_CONTRIBUTE, &100);
    assert_eq!(client.get_score(&user).streak_count, 2);

    client.record_activity(&user, &ACTIVITY_DEFAULT, &0);
    assert_eq!(client.get_score(&user).streak_count, 0);
}

#[test]
fn test_tier_progression() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    assert_eq!(client.get_score(&user).tier, TIER_BRONZE);

    client.record_activity(&user, &ACTIVITY_JOIN, &200);
    assert_eq!(client.get_score(&user).tier, TIER_SILVER);

    client.record_activity(&user, &ACTIVITY_COMPLETE, &200);
    assert_eq!(client.get_score(&user).tier, TIER_GOLD);

    client.record_activity(&user, &ACTIVITY_CONTRIBUTE, &200);
    assert_eq!(client.get_score(&user).tier, TIER_PLATINUM);

    client.record_activity(&user, &ACTIVITY_COMPLETE, &200);
    assert_eq!(client.get_score(&user).tier, TIER_DIAMOND);
}

#[test]
fn test_record_rejects_invalid_activity_type() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    let result = client.try_record_activity(&user, &5u32, &100);
    assert_eq!(result, Err(Ok(ReputationError::InvalidActivityType)));
}

#[test]
fn test_record_rejects_invalid_impact() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    let result = client.try_record_activity(&user, &ACTIVITY_JOIN, &1001);
    assert_eq!(result, Err(Ok(ReputationError::InvalidScoreImpact)));
}

#[test]
fn test_get_history_filters_by_user() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.record_activity(&user_a, &ACTIVITY_JOIN, &100);
    client.record_activity(&user_b, &ACTIVITY_CONTRIBUTE, &50);
    client.record_activity(&user_a, &ACTIVITY_COMPLETE, &200);

    let history_a = client.get_history(&user_a, &0, &100);
    assert_eq!(history_a.len(), 2);

    let history_b = client.get_history(&user_b, &0, &100);
    assert_eq!(history_b.len(), 1);
}

#[test]
fn test_get_score_nonexistent_user() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    let score = client.get_score(&user);
    assert_eq!(score.score, 0);
    assert_eq!(score.total_circles, 0);
}

#[test]
fn test_activity_emits_event() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.record_activity(&user, &ACTIVITY_JOIN, &100);

    // let events = env.events().all();
    // let last
    // let (_id, topics, _data)
    // let topic0
    // assert_eq
}

#[test]
fn test_pause_blocks_record() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let user = Address::generate(&env);

    client.pause_registry(&admin);
    let result = client.try_record_activity(&user, &ACTIVITY_JOIN, &100);
    assert_eq!(result, Err(Ok(ReputationError::ContractPaused)));

    client.unpause_registry(&admin);
    assert!(client
        .try_record_activity(&user, &ACTIVITY_JOIN, &100)
        .is_ok());
}

#[test]
fn test_pause_unauthorized() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let stranger = Address::generate(&env);

    let result = client.try_pause_registry(&stranger);
    assert!(result.is_err());
}

#[test]
fn test_pause_blocks_get_score() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let user = Address::generate(&env);

    client.pause_registry(&admin);
    let result = client.try_get_score(&user);
    assert_eq!(result, Err(Ok(ReputationError::ContractPaused)));

    client.unpause_registry(&admin);
    assert!(client.try_get_score(&user).is_ok());
}
