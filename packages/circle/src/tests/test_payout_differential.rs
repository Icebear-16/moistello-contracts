#![cfg(test)]

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

use crate::types::CircleConfig;
use crate::{Circle, CircleArgs, CircleClient};

// ---------------------------------------------------------------------------
// Differential Reference Models
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutOutcome {
    pub shares: std::vec::Vec<i128>,
    pub fee: i128,
    pub net: i128,
    pub dust: i128,
    pub total_distributed: i128,
}

/// Legacy Winner-Take-All Model:
/// Round winner receives the entire net pot, fee is sent to treasury,
/// non-winners receive zero.
pub fn legacy_winner_take_all(
    pool: i128,
    fee_bps: u32,
    num_members: usize,
    winner_idx: usize,
) -> PayoutOutcome {
    let fee = (pool * (fee_bps as i128)) / 10_000;
    let net = pool - fee;

    let mut shares = std::vec![0i128; num_members];
    if num_members > 0 && winner_idx < num_members {
        shares[winner_idx] = net;
    }

    PayoutOutcome {
        shares,
        fee,
        net,
        dust: 0,
        total_distributed: net,
    }
}

/// Legacy Proportional / Equal-Share Model:
/// Net pot is split evenly across all members, with integer remainder dust
/// going to the round winner.
pub fn legacy_equal_share(
    pool: i128,
    fee_bps: u32,
    num_members: usize,
    winner_idx: usize,
) -> PayoutOutcome {
    let fee = (pool * (fee_bps as i128)) / 10_000;
    let net = pool - fee;

    if num_members == 0 {
        return PayoutOutcome {
            shares: std::vec![],
            fee,
            net,
            dust: net,
            total_distributed: 0,
        };
    }

    let base_share = net / (num_members as i128);
    let dust = net - (base_share * (num_members as i128));

    let mut shares = std::vec![base_share; num_members];
    if winner_idx < num_members {
        shares[winner_idx] += dust;
    }

    let total_distributed = shares.iter().sum();

    PayoutOutcome {
        shares,
        fee,
        net,
        dust,
        total_distributed,
    }
}

/// Optimized Time-Weighted Payout Model (as implemented in circle::contract):
/// Each member receives a share proportional to their contribution amount and
/// time held in the pool. Any remainder dust from integer division is transferred
/// to the round winner. If total_weighted is 0, winner receives full net.
pub fn optimized_time_weighted(
    contributions: &[(i128, u64)], // (amount, timestamp) per member
    fee_bps: u32,
    current_time: u64,
    winner_idx: usize,
) -> PayoutOutcome {
    let num_members = contributions.len();
    let pool: i128 = contributions.iter().map(|(amt, _)| *amt).sum();
    let fee = (pool * (fee_bps as i128)) / 10_000;
    let net = pool - fee;

    if num_members == 0 || net <= 0 {
        return PayoutOutcome {
            shares: std::vec![0; num_members],
            fee,
            net,
            dust: 0,
            total_distributed: 0,
        };
    }

    let mut weights = std::vec![0u128; num_members];
    let mut total_weighted = 0u128;

    for (i, &(amount, timestamp)) in contributions.iter().enumerate() {
        let time_held = (current_time as u128).saturating_sub(timestamp as u128);
        let w = (amount as u128).saturating_mul(time_held);
        weights[i] = w;
        total_weighted = total_weighted.saturating_add(w);
    }

    let mut shares = std::vec![0i128; num_members];
    let mut distributed = 0i128;

    if total_weighted > 0 {
        let net_u = net as u128;
        for i in 0..num_members {
            let w = weights[i];
            let share = if distributed == 0 && w == total_weighted {
                net
            } else {
                (net_u.saturating_mul(w) / total_weighted) as i128
            };
            shares[i] = share;
            distributed += share;
        }
    }

    let dust = if distributed < net {
        let remainder = net - distributed;
        if winner_idx < num_members {
            shares[winner_idx] += remainder;
        }
        distributed += remainder;
        remainder
    } else {
        0
    };

    PayoutOutcome {
        shares,
        fee,
        net,
        dust,
        total_distributed: distributed,
    }
}

// ---------------------------------------------------------------------------
// Differential Property & Invariant Tests
// ---------------------------------------------------------------------------

#[test]
fn test_conservation_of_funds_invariant_randomized() {
    let mut rng = StdRng::seed_from_u64(0x439_439_439);

    for _ in 0..1_000 {
        let num_members: usize = rng.gen_range(2..=50);
        let contribution_amount: i128 = rng.gen_range(100..=10_000_000);
        let fee_bps: u32 = rng.gen_range(0..=2_500); // 0% to 25% fee
        let winner_idx: usize = rng.gen_range(0..num_members);

        let start_time: u64 = 1_000_000;
        let mut contributions = std::vec::Vec::with_capacity(num_members);
        for _ in 0..num_members {
            let offset: u64 = rng.gen_range(0..=86_400);
            contributions.push((contribution_amount, start_time + offset));
        }

        let resolution_time = start_time + 86_400 + rng.gen_range(1..=100_000);
        let pool = contribution_amount * (num_members as i128);

        let legacy_res = legacy_winner_take_all(pool, fee_bps, num_members, winner_idx);
        let equal_res = legacy_equal_share(pool, fee_bps, num_members, winner_idx);
        let opt_res = optimized_time_weighted(&contributions, fee_bps, resolution_time, winner_idx);

        // Invariant 1: Total distributed + fee == pool across all models
        assert_eq!(legacy_res.total_distributed + legacy_res.fee, pool);
        assert_eq!(equal_res.total_distributed + equal_res.fee, pool);
        assert_eq!(opt_res.total_distributed + opt_res.fee, pool);

        // Invariant 2: Net amount is identical across all models
        assert_eq!(legacy_res.net, opt_res.net);
        assert_eq!(equal_res.net, opt_res.net);

        // Invariant 3: Sum of shares exactly equals net
        let opt_sum: i128 = opt_res.shares.iter().sum();
        let legacy_sum: i128 = legacy_res.shares.iter().sum();
        let equal_sum: i128 = equal_res.shares.iter().sum();

        assert_eq!(opt_sum, opt_res.net);
        assert_eq!(legacy_sum, legacy_res.net);
        assert_eq!(equal_sum, equal_res.net);

        // Invariant 4: Dust is bounded by member count
        assert!(opt_res.dust >= 0);
        assert!(opt_res.dust < num_members as i128);
    }
}

#[test]
fn test_equal_timestamp_convergence_property() {
    let mut rng = StdRng::seed_from_u64(0x123_456_789);

    for _ in 0..200 {
        let num_members: usize = rng.gen_range(2..=30);
        let contribution_amount: i128 = rng.gen_range(100..=5_000_000);
        let fee_bps: u32 = rng.gen_range(0..=1_000);
        let winner_idx: usize = rng.gen_range(0..num_members);

        let same_timestamp: u64 = 1_700_000_000;
        let resolution_time = same_timestamp + rng.gen_range(60..=86_400);

        let contributions = std::vec![(contribution_amount, same_timestamp); num_members];
        let pool = contribution_amount * (num_members as i128);

        let equal_res = legacy_equal_share(pool, fee_bps, num_members, winner_idx);
        let opt_res = optimized_time_weighted(&contributions, fee_bps, resolution_time, winner_idx);

        // When all timestamps and amounts are equal, optimized time-weighted
        // payout MUST converge identically to the legacy equal share payout.
        assert_eq!(opt_res.shares, equal_res.shares);
        assert_eq!(opt_res.dust, equal_res.dust);
        assert_eq!(opt_res.total_distributed, equal_res.total_distributed);
    }
}

#[test]
fn test_early_contribution_advantage_property() {
    // Member 0 deposits early at T=100
    // Member 1 deposits late at T=900
    // Member 2 is winner, deposits at T=500
    // Resolution at T=1000
    let contributions = std::vec![(1_000i128, 100u64), (1_000i128, 900u64), (1_000i128, 500u64)];
    let opt_res = optimized_time_weighted(&contributions, 0, 1000, 2);

    // Holding times:
    // Member 0: 900s
    // Member 1: 100s
    // Member 2: 500s
    // Total weight = 900,000 + 100,000 + 500,000 = 1,500,000
    // Member 0 weight share = 900/1500 = 60%
    // Member 1 weight share = 100/1500 = 6.66%
    // Member 2 weight share = 500/1500 = 33.33%

    assert!(opt_res.shares[0] > opt_res.shares[2]);
    assert!(opt_res.shares[2] > opt_res.shares[1]);

    // Compared to legacy winner-take-all where only winner receives anything:
    let legacy_res = legacy_winner_take_all(3_000, 0, 3, 2);
    assert_eq!(legacy_res.shares[0], 0);
    assert_eq!(legacy_res.shares[1], 0);
    assert_eq!(legacy_res.shares[2], 3_000);
}

#[test]
fn test_zero_holding_time_fallback() {
    // When contributions happen at the exact resolution timestamp, total_weighted is 0.
    // The fallback sends the entire net pot to the round recipient without panicking.
    let contributions = std::vec![(500i128, 100u64), (500i128, 100u64)];
    let opt_res = optimized_time_weighted(&contributions, 200, 100, 1);

    let legacy_res = legacy_winner_take_all(1_000, 200, 2, 1);
    assert_eq!(opt_res.shares[1], opt_res.net);
    assert_eq!(opt_res.shares[0], 0);
    assert_eq!(opt_res.shares, legacy_res.shares);
}

// ---------------------------------------------------------------------------
// On-Chain Differential Testing against Local Soroban Ledger
// ---------------------------------------------------------------------------

fn setup_differential_test_circle<'a>(
    env: &'a Env,
    num_members: u32,
    contribution_amount: i128,
    fee_bps: u32,
) -> (
    CircleClient<'a>,
    Address,
    Address,
    Address,
    treasury::TreasuryClient<'a>,
) {
    let organizer = Address::generate(env);
    let factory = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();

    let config = CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Differential Payout Circle"),
        contribution_amount,
        max_members: num_members,
        payout_type: 1, // PAYOUT_FIXED: deterministic winner (round % max_members)
        total_rounds: num_members,
        contribution_deadline_seconds: 604800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 86400,
        max_strikes: 3,
        slug: String::from_str(env, "diff-payout"),
    };

    let contract_id = env.register(Circle, CircleArgs::__constructor(&organizer, &factory, &config));
    let client = CircleClient::new(env, &contract_id);

    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(env, &treasury_id);
    treasury_client.init(&organizer, &token);

    client.set_treasury(&organizer, &treasury_id);
    client.set_fee_bps(&organizer, &fee_bps);

    (client, organizer, token, treasury_id, treasury_client)
}

fn mint_asset(env: &Env, token: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(env, token).mint(to, &amount);
}

#[test]
fn test_on_chain_payout_differential_staggered_contributions() {
    let env = Env::default();
    env.mock_all_auths();

    let contribution = 10_0000000i128;
    let fee_bps = 500u32; // 5% fee
    let num_members = 3u32;

    let (client, organizer, token, treasury_id, _treasury_client) =
        setup_differential_test_circle(&env, num_members, contribution, fee_bps);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let members = [m0.clone(), m1.clone(), m2.clone()];

    for m in &members {
        client.join(m);
        mint_asset(&env, &token, m, contribution);
    }

    // Staggered contributions:
    // m0 at T=100, m1 at T=200, m2 at T=300
    env.ledger().set_timestamp(100);
    client.contribute(&m0, &contribution, &0);

    env.ledger().set_timestamp(200);
    client.contribute(&m1, &contribution, &0);

    env.ledger().set_timestamp(300);
    client.contribute(&m2, &contribution, &0);

    // Resolve at T=400.
    // Round 0 winner with PAYOUT_FIXED is position (0 % 3) = 0 (m0).
    let resolution_time = 400u64;
    env.ledger().set_timestamp(resolution_time);

    let pool = contribution * 3;
    let expected_diff = optimized_time_weighted(
        &[
            (contribution, 100),
            (contribution, 200),
            (contribution, 300),
        ],
        fee_bps,
        resolution_time,
        0, // winner_idx = 0
    );
    let legacy_diff = legacy_winner_take_all(pool, fee_bps, 3, 0);

    // Contract pre-state
    assert_eq!(token_client.balance(&client.address), pool);

    // Act
    client.trigger_payout(&organizer, &0);

    // Assert: Treasury received fee
    assert_eq!(token_client.balance(&treasury_id), expected_diff.fee);
    assert_eq!(token_client.balance(&treasury_id), legacy_diff.fee);

    // Assert: Contract has zero residual balance (exact fund conservation)
    assert_eq!(token_client.balance(&client.address), 0);

    // Assert: Each member received EXACTLY their computed time-weighted share
    let bal_m0 = token_client.balance(&m0);
    let bal_m1 = token_client.balance(&m1);
    let bal_m2 = token_client.balance(&m2);

    assert_eq!(bal_m0, expected_diff.shares[0]);
    assert_eq!(bal_m1, expected_diff.shares[1]);
    assert_eq!(bal_m2, expected_diff.shares[2]);

    // Differential assertion against legacy baseline:
    // In legacy winner-take-all:
    // m0 would get 100% of net (legacy_diff.shares[0]), while m1 and m2 get 0.
    // Under optimized calculation:
    // m0 still gets the highest share (longer hold time + dust), but m1 and m2 also
    // receive their earned proportional yield.
    assert!(bal_m0 < legacy_diff.shares[0]);
    assert!(bal_m1 > legacy_diff.shares[1]);
    assert!(bal_m2 > legacy_diff.shares[2]);
    assert!(bal_m0 > bal_m1);
    assert!(bal_m1 > bal_m2);

    // Conservation check: sum of all member balances + treasury == initial pool
    assert_eq!(bal_m0 + bal_m1 + bal_m2 + token_client.balance(&treasury_id), pool);
}

#[test]
fn test_on_chain_payout_differential_simultaneous_contributions() {
    let env = Env::default();
    env.mock_all_auths();

    let contribution = 10_0000000i128;
    let fee_bps = 200u32; // 2% fee
    let num_members = 2u32;

    let (client, organizer, token, treasury_id, _treasury_client) =
        setup_differential_test_circle(&env, num_members, contribution, fee_bps);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);

    client.join(&m0);
    client.join(&m1);
    mint_asset(&env, &token, &m0, contribution);
    mint_asset(&env, &token, &m1, contribution);

    // Both contribute at identical timestamp T=1,000
    env.ledger().set_timestamp(1_000);
    client.contribute(&m0, &contribution, &0);
    client.contribute(&m1, &contribution, &0);

    // Resolve at T=1,500. Round 0 winner = m0.
    env.ledger().set_timestamp(1_500);

    let pool = contribution * 2;
    let expected_equal = legacy_equal_share(pool, fee_bps, 2, 0);

    client.trigger_payout(&organizer, &0);

    let bal_m0 = token_client.balance(&m0);
    let bal_m1 = token_client.balance(&m1);

    // When timestamps are identical, on-chain time-weighted matches legacy equal share
    assert_eq!(bal_m0, expected_equal.shares[0]);
    assert_eq!(bal_m1, expected_equal.shares[1]);
    assert_eq!(token_client.balance(&treasury_id), expected_equal.fee);
    assert_eq!(token_client.balance(&client.address), 0);
}
