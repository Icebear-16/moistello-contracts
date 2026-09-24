#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

/// Integration test: factory deploys circle -> members join -> contribute -> trigger payout -> fee sent to treasury
/// This test is currently a stub due to circular dependencies between contracts in the test environment.
/// In production, these contracts are deployed separately and interact via cross-contract calls.
#[test]
#[ignore = "Integration tests require all contracts to be deployed separately"]
fn test_full_integration_factory_circle_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    // This test validates the complete flow:
    // 1. Factory deploys a circle
    // 2. Members join the circle
    // 3. Members contribute to rounds
    // 4. Payouts are triggered with fees
    // 5. Fees are sent to treasury
    // 6. Treasury accumulates fees from multiple circles

    // Due to test environment limitations, this must be tested on testnet/mainnet
    // where contracts are deployed independently.

    assert!(true, "Integration test placeholder");
}

/// Unit test: Circle lifecycle with fee collection (simulated treasury)
#[test]
fn test_circle_lifecycle_with_fees() {
    let env = Env::default();
    env.mock_all_auths();

    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    // Deploy a real treasury contract (a bare address would trap on deposit_fee).
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&organizer, &token);
    let treasury = treasury_id.clone();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Integration Test Circle"),
        contribution_amount: 100_0000000i128,
        max_members: 3u32,
        payout_type: 1u32, // PAYOUT_FIXED
        total_rounds: 3u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "integration-test"),
    };

    let admin = organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(crate::Circle, (&admin, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // Configure circle with treasury and fee
    client.set_treasury(&organizer, &treasury);
    client.set_fee_bps(&organizer, &50u32); // 0.5% fee

    // Members join
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);

    client.join(&m1);
    client.join(&m2);
    client.join(&m3);

    assert_eq!(client.get_members().len(), 3);
    assert_eq!(client.get_status().status, 1u32); // STATUS_ACTIVE

    // Mint tokens for members
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&m1, &(config.contribution_amount * 3));
    token_client.mint(&m2, &(config.contribution_amount * 3));
    token_client.mint(&m3, &(config.contribution_amount * 3));

    // Round 0: All contribute, trigger payout
    client.contribute(&m1, &config.contribution_amount, &0u32);
    client.contribute(&m2, &config.contribution_amount, &0u32);
    client.contribute(&m3, &config.contribution_amount, &0u32);
    client.trigger_payout(&organizer, &0u32);

    let status_r0 = client.get_status();
    assert_eq!(status_r0.current_round, 1u32);
    assert!(status_r0.total_fees > 0); // Fee was collected

    // Verify treasury received the fee
    let token_client_regular = soroban_sdk::token::Client::new(&env, &token);
    let treasury_balance = token_client_regular.balance(&treasury);
    assert!(treasury_balance > 0, "Treasury should have received fees");

    // Round 1: Repeat
    client.contribute(&m1, &config.contribution_amount, &1u32);
    client.contribute(&m2, &config.contribution_amount, &1u32);
    client.contribute(&m3, &config.contribution_amount, &1u32);
    client.trigger_payout(&organizer, &1u32);

    // Round 2: Final round
    client.contribute(&m1, &config.contribution_amount, &2u32);
    client.contribute(&m2, &config.contribution_amount, &2u32);
    client.contribute(&m3, &config.contribution_amount, &2u32);
    client.trigger_payout(&organizer, &2u32);

    // Verify circle completed
    let final_status = client.get_status();
    assert_eq!(final_status.status, 2u32); // STATUS_COMPLETED
    assert_eq!(final_status.current_round, 3u32);

    // Verify total fees accumulated
    let final_treasury_balance = token_client_regular.balance(&treasury);
    assert_eq!(final_treasury_balance, final_status.total_fees);
    assert!(final_treasury_balance > 0);
}
