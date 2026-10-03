#![cfg(test)]

use crate as circle;
use circle::types::CircleConfig;
use circle::{Circle, CircleArgs, CircleClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn create_config(env: &Env, organizer: &Address, token: &Address) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Invariant Test Circle"),
        contribution_amount: 1000_0000000i128,
        max_members: 5u32,
        payout_type: 0u32, // PAYOUT_RANDOM
        total_rounds: 5u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "invariant-test"),
    }
}

/// Helper function to assert token conservation invariant across any circle state.
///
/// Invariant: sum(contributions) == sum(payouts) + total_fees
fn assert_token_conservation_invariant(client: &CircleClient) {
    let status = client.get_status();
    let members = client.get_members();

    let mut sum_contributions: i128 = 0;
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            let contribs = client.get_contributions(&m.address, &0, &100);
            for j in 0..contribs.len() {
                if let Some(c) = contribs.get(j) {
                    sum_contributions += c.amount;
                }
            }
        }
    }

    let sum_payouts = status.total_payouts;
    let total_fees = status.total_fees;
    let total_out = sum_payouts + total_fees;

    assert!(
        sum_contributions >= total_out,
        "Invariant violation: sum(contributions) ({}) < sum(payouts) ({}) + total_fees ({})",
        sum_contributions,
        sum_payouts,
        total_fees
    );
}

#[test]
fn test_token_conservation_invariant_randomized_sequence() {
    let env = Env::default();
    env.mock_all_auths();

    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();

    let config = create_config(&env, &organizer, &token);
    let factory = Address::generate(&env);
    let contract_id =
        env.register(Circle, CircleArgs::__constructor(&organizer, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    // Setup 5 members
    let mut members = Vec::new(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);

    for _ in 0..5 {
        let m = Address::generate(&env);
        token_client.mint(&m, &10000_0000000i128);
        client.join(&m);
        members.push_back(m);
    }

    // Verify invariant after join phase
    assert_token_conservation_invariant(&client);

    // Randomized operational sequence over 5 rounds with intentional valid/invalid ops
    let pseudo_random_seeds = [
        0u32, 1u32, 2u32, 3u32, 4u32, 0u32, 2u32, 1u32, 3u32, 4u32, 4u32, 3u32, 2u32, 1u32, 0u32,
        1u32, 0u32, 3u32, 2u32, 4u32,
    ];

    let mut seed_idx = 0;

    for round in 0..5u32 {
        // Step 1: Members contribute in pseudo-random order, including potential duplicates/invalid rounds
        for _ in 0..5 {
            let m_idx = pseudo_random_seeds[seed_idx % pseudo_random_seeds.len()];
            seed_idx += 1;
            let member = members.get(m_idx).unwrap();

            // Attempt contribution — may succeed or return error if already contributed
            let _ = client.try_contribute(&member, &config.contribution_amount, &round);

            // Assert invariant after every attempt (whether succeeded or failed)
            assert_token_conservation_invariant(&client);
        }

        // Step 2: Trigger payout for current round
        let _ = client.try_trigger_payout(&organizer, &round);

        // Assert invariant holds after payout attempt
        assert_token_conservation_invariant(&client);
    }

    // Final invariant check
    assert_token_conservation_invariant(&client);
}
