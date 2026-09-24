#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String, token};

use crate::types::CircleConfig;
use crate::{Circle, CircleClient};

fn create_config(env: &Env) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: Address::generate(env),
        name: String::from_str(env, "Analytics Test"),
        contribution_amount: 100_0000000i128,
        max_members: 3u32,
        payout_type: 1u32,
        total_rounds: 2u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "analytics-test"),
    }
}

fn setup(env: &Env) -> (CircleClient<'_>, Address, Address, CircleConfig) {
    env.mock_all_auths();
    let mut config = create_config(env);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    config.token = token_contract.address();
    let token = config.token.clone();
    let contract_id = env.register(Circle, (&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin, token, config)
}

#[test]
fn test_analytics_initialized_on_construction() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    let analytics = client.get_analytics();
    assert_eq!(analytics.total_members, 0);
    assert_eq!(analytics.total_contributions, 0);
    assert_eq!(analytics.contribution_count, 0);
    assert_eq!(analytics.total_payouts, 0);
    assert_eq!(analytics.total_defaults, 0);
}

#[test]
fn test_analytics_tracks_joins() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
    });
    let (client, _, _, _) = setup(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);
    assert_eq!(client.get_analytics().total_members, 2);
    let stats = client.get_member_stats(&m1);
    assert_eq!(stats.member, m1);
    assert_eq!(stats.joined_at, 1_000_000);
    assert_eq!(client.get_all_member_stats().len(), 2);
}

#[test]
fn test_analytics_tracks_contributions_and_receipts_through_payout() {
    let env = Env::default();
    let (client, admin, token, config) = setup(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);
    client.join(&m3);

    let token_client = token::StellarAssetClient::new(&env, &token);
    for m in [&m1, &m2, &m3] {
        token_client.mint(m, &(config.contribution_amount * 3));
    }
    let circle_token = token::Client::new(&env, &token);
    for m in [&m1, &m2, &m3] {
        circle_token.transfer(m, &client.address, &config.contribution_amount);
        client.contribute(m, &config.contribution_amount, &0);
    }

    let analytics = client.get_analytics();
    assert_eq!(
        analytics.total_contributions,
        config.contribution_amount * 3
    );
    assert_eq!(analytics.contribution_count, 3);
    let stats = client.get_member_stats(&m1);
    assert_eq!(stats.total_contributed, config.contribution_amount);
    assert_eq!(stats.contribution_count, 1);

    client.trigger_payout(&admin, &0);

    let analytics = client.get_analytics();
    assert!(analytics.total_payouts > 0);
    assert!(analytics.avg_completion_time > 0 || analytics.total_payouts > 0);
    let received: i128 = client
        .get_all_member_stats()
        .iter()
        .map(|s| s.total_received)
        .sum::<i128>();
    assert_eq!(received, analytics.total_payouts);
}

#[test]
fn test_analytics_batch_invite_syncs_members() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    let mut invitees = soroban_sdk::Vec::new(&env);
    for _ in 0..3 {
        invitees.push_back(Address::generate(&env));
    }
    client.batch_invite(&admin, &invitees);
    assert_eq!(client.get_analytics().total_members, 3);
    assert_eq!(client.get_all_member_stats().len(), 3);
}
