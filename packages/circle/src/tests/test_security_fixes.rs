#![cfg(test)]

//! Security-fix tests for #356 (address validation), #358 (tx expiry),
//! #359 (multisig), #360 (oracle signatures + TTL cache).

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};

use crate::{Circle, CircleClient, CircleError};
use crate::types::CircleConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_config(env: &Env, token: &Address) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Sec Circle"),
        contribution_amount: 100_i128,
        max_members: 2,
        payout_type: 1, // FIXED — deterministic payouts
        total_rounds: 2,
        contribution_deadline_seconds: 60,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(env, "sec-circle"),
    }
}

fn setup_circle(env: &Env) -> (CircleClient<'_>, Address, Address) {
    env.mock_all_auths();
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
    let config = create_config(env, &token);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, (&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin, token)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn setup_active_circle(env: &Env) -> (CircleClient<'_>, Address, Address, Address, Address) {
    let (client, admin, token) = setup_circle(env);
    let m1 = Address::generate(env);
    let m2 = Address::generate(env);
    client.join(&m1);
    client.join(&m2);
    mint(env, &token, &m1, 1_000);
    mint(env, &token, &m2, 1_000);
    client.contribute(&m1, &100, &0);
    client.contribute(&m2, &100, &0);
    (client, admin, token, m1, m2)
}

const ZERO_STRKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(env, ZERO_STRKEY))
}

// ---------------------------------------------------------------------------
// #356 — address validation
// ---------------------------------------------------------------------------

#[test]
fn test_zero_address_rejected_on_join() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    let zero = zero_address(&env);
    let res = client.try_join(&zero);
    assert_eq!(res, Err(Ok(CircleError::ZeroAddress)));
}

#[test]
fn test_zero_address_rejected_on_admin_setters() {
    let env = Env::default();
    let (client, admin, _token) = setup_circle(&env);
    let zero = zero_address(&env);
    assert_eq!(
        client.try_set_treasury(&admin, &zero),
        Err(Ok(CircleError::ZeroAddress))
    );
    assert_eq!(
        client.try_set_oracle(&admin, &zero),
        Err(Ok(CircleError::ZeroAddress))
    );
}

#[test]
fn test_valid_addresses_still_accepted() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    let member = Address::generate(&env);
    assert!(client.try_join(&member).is_ok());
}

// ---------------------------------------------------------------------------
// #358 — transaction expiry
// ---------------------------------------------------------------------------

#[test]
fn test_contribute_with_expiry_happy_path() {
    let env = Env::default();
    let (client, _admin, token) = setup_circle(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);
    mint(&env, &token, &m1, 1_000);
    let cur = env.ledger().sequence();
    assert!(client
        .try_contribute_with_expiry(&m1, &100, &0, &(cur + 50))
        .is_ok());
}

#[test]
fn test_expired_transaction_rejected() {
    let env = Env::default();
    let (client, _admin, token) = setup_circle(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);
    mint(&env, &token, &m1, 1_000);
    let cur = env.ledger().sequence();
    let bound = cur + 10;
    env.ledger().set_sequence_number(bound + 1);
    assert_eq!(
        client.try_contribute_with_expiry(&m1, &100, &0, &bound),
        Err(Ok(CircleError::TxExpired))
    );
    assert_eq!(
        client.try_check_tx_expiry(&bound),
        Err(Ok(CircleError::TxExpired))
    );
}

#[test]
fn test_zero_expiry_bound_rejected() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    assert_eq!(
        client.try_check_tx_expiry(&0),
        Err(Ok(CircleError::InvalidExpiryBound))
    );
}

#[test]
fn test_trigger_payout_with_expiry() {
    let env = Env::default();
    let (client, admin, _token, _m1, _m2) = setup_active_circle(&env);
    let cur = env.ledger().sequence();
    assert!(client
        .try_trigger_payout_with_expiry(&admin, &0, &(cur + 25))
        .is_ok());
    assert_eq!(client.get_status().current_round, 1);
}

// ---------------------------------------------------------------------------
// #359 — multisig
// ---------------------------------------------------------------------------

#[test]
fn test_single_sig_payout_by_default() {
    let env = Env::default();
    let (client, admin, _token, _m1, _m2) = setup_active_circle(&env);
    assert!(client.get_multisig_config().is_none());
    assert!(client.try_trigger_payout(&admin, &0).is_ok());
}

#[test]
fn test_configure_multisig_rejects_bad_config() {
    let env = Env::default();
    let (client, admin, _token) = setup_circle(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(a.clone());
    admins.push_back(b.clone());
    // Threshold 0 and threshold > n both invalid.
    assert_eq!(
        client.try_configure_multisig(&admin, &admins, &0),
        Err(Ok(CircleError::InvalidMultisigConfig))
    );
    assert_eq!(
        client.try_configure_multisig(&admin, &admins, &3),
        Err(Ok(CircleError::InvalidMultisigConfig))
    );
    // Non-admin cannot configure.
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_configure_multisig(&stranger, &admins, &2),
        Err(Ok(CircleError::Unauthorized))
    );
}

#[test]
fn test_multisig_2of2_payout() {
    let env = Env::default();
    let (client, admin, _token, _m1, _m2) = setup_active_circle(&env);
    let second = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(second.clone());
    client.configure_multisig(&admin, &admins, &2);

    let cfg = client.get_multisig_config().unwrap();
    assert_eq!(cfg.threshold, 2);
    assert_eq!(cfg.admins.len(), 2);

    // Single-sig path now fails closed.
    assert_eq!(
        client.try_trigger_payout(&admin, &0),
        Err(Ok(CircleError::MultisigThresholdNotMet))
    );
    assert_eq!(
        client.try_set_fee_bps(&admin, &100),
        Err(Ok(CircleError::MultisigThresholdNotMet))
    );

    let action = BytesN::from_array(&env, &[9u8; 32]);

    // No approvals yet — execution rejected, no replay possible.
    assert_eq!(
        client.try_trigger_payout_multisig(&admin, &0, &action),
        Err(Ok(CircleError::MultisigNoApprovals))
    );

    // One approval is not enough for 2-of-2.
    client.approve_action(&admin, &action);
    assert_eq!(client.get_action_approvals(&action).len(), 1);
    assert_eq!(
        client.try_trigger_payout_multisig(&admin, &0, &action),
        Err(Ok(CircleError::MultisigThresholdNotMet))
    );

    // Duplicate approval rejected.
    assert_eq!(
        client.try_approve_action(&admin, &action),
        Err(Ok(CircleError::MultisigAlreadyApproved))
    );

    // Second approval reaches threshold — execution succeeds...
    client.approve_action(&second, &action);
    let action_fee = BytesN::from_array(&env, &[8u8; 32]);
    client.approve_action(&admin, &action_fee);
    client.approve_action(&second, &action_fee);
    // Fee path works under the same 2-of-2 approvals (separate action id).
    assert!(client
        .try_set_fee_bps_multisig(&admin, &100, &action_fee)
        .is_ok());
    let res = client.try_trigger_payout_multisig(&admin, &0, &action);
    assert!(res.is_ok());
    assert_eq!(client.get_status().current_round, 1);

    // ...and the action id is single-use (approvals cleared, no replay).
    assert_eq!(client.get_action_approvals(&action).len(), 0);
    assert_eq!(
        client.try_trigger_payout_multisig(&admin, &1, &action),
        Err(Ok(CircleError::MultisigNoApprovals))
    );
}

#[test]
fn test_multisig_1of2_single_approver_ok() {
    let env = Env::default();
    let (client, admin, _token, _m1, _m2) = setup_active_circle(&env);
    let second = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(second.clone());
    client.configure_multisig(&admin, &admins, &1);

    // A single approval meets a 1-of-2 threshold; execution itself still
    // requires organizer/admin rights, so the admin executes.
    let action = BytesN::from_array(&env, &[3u8; 32]);
    client.approve_action(&second, &action);
    assert!(client
        .try_trigger_payout_multisig(&admin, &0, &action)
        .is_ok());
}

#[test]
fn test_multisig_outsider_rejected() {
    let env = Env::default();
    let (client, admin, _token, _m1, _m2) = setup_active_circle(&env);
    let second = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(second.clone());
    client.configure_multisig(&admin, &admins, &2);

    // Outsider cannot approve...
    let outsider = Address::generate(&env);
    let action = BytesN::from_array(&env, &[5u8; 32]);
    assert_eq!(
        client.try_approve_action(&outsider, &action),
        Err(Ok(CircleError::Unauthorized))
    );
    // ...and outsider cannot execute either.
    client.approve_action(&admin, &action);
    client.approve_action(&second, &action);
    assert_eq!(
        client.try_trigger_payout_multisig(&outsider, &0, &action),
        Err(Ok(CircleError::Unauthorized))
    );
}

// ---------------------------------------------------------------------------
// #360 — oracle signatures + source check + TTL cache
// ---------------------------------------------------------------------------

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_rate(env: Env, rate: i128) {
        env.storage().instance().set(&soroban_sdk::symbol_short!("rate"), &rate);
    }
    pub fn set_signed(env: Env, rate: i128, sig: BytesN<64>) {
        env.storage().instance().set(&soroban_sdk::symbol_short!("rate"), &rate);
        env.storage().instance().set(&soroban_sdk::symbol_short!("sig"), &sig);
    }
    pub fn yld_rate(env: Env, _round: u32) -> i128 {
        env.storage().instance().get(&soroban_sdk::symbol_short!("rate")).unwrap_or(500)
    }
    pub fn yld_sig(env: Env, _round: u32) -> (i128, BytesN<64>) {
        let rate: i128 = env.storage().instance().get(&soroban_sdk::symbol_short!("rate")).unwrap_or(500);
        let sig: BytesN<64> = env
            .storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("sig"))
            .unwrap_or(BytesN::from_array(&env, &[0u8; 64]));
        (rate, sig)
    }
}

fn setup_circle_with_mock_oracle(env: &Env) -> (CircleClient<'_>, Address, Address) {
    let (client, admin, _token) = setup_circle(env);
    let oracle_id = env.register(MockOracle, ());
    client.set_oracle(&admin, &oracle_id);
    (client, admin, oracle_id)
}

#[test]
fn test_no_oracle_returns_zero_rate() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    assert_eq!(client.get_yield_rate(&0), 0);
}

#[test]
fn test_unsigned_oracle_rate_and_cache_ttl() {
    let env = Env::default();
    let (client, _admin, oracle_id) = setup_circle_with_mock_oracle(&env);
    let oracle_client = crate::tests::test_security_fixes::MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_rate(&250);

    // First fetch hits the oracle and populates the cache.
    assert_eq!(client.get_yield_rate(&0), 250);
    let cache = client.get_oracle_cache().unwrap();
    assert_eq!(cache.rate, 250);
    assert_eq!(cache.round, 0);

    // Change the oracle rate — same round within TTL still serves cache.
    oracle_client.set_rate(&999);
    assert_eq!(client.get_yield_rate(&0), 250);

    // Past the TTL the new rate is refetched.
    let cached_ledger = cache.ledger;
    env.ledger().set_sequence_number(cached_ledger + 51);
    assert_eq!(client.get_yield_rate(&0), 999);
}

#[test]
fn test_wrong_oracle_source_rejected() {
    let env = Env::default();
    let (client, _admin, oracle_id) = setup_circle_with_mock_oracle(&env);
    assert!(client.try_check_oracle_source(&oracle_id).is_ok());
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_check_oracle_source(&stranger),
        Err(Ok(CircleError::WrongOracle))
    );
}

fn oracle_message(rate: i128, round: u32) -> std::vec::Vec<u8> {
    let mut v = rate.to_be_bytes().to_vec();
    v.extend_from_slice(&round.to_be_bytes());
    v
}

#[test]
fn test_signed_oracle_valid_signature() {
    use ed25519_dalek::{Signer, SigningKey};
    let env = Env::default();
    let (client, admin, oracle_id) = setup_circle_with_mock_oracle(&env);

    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let pubkey = signing.verifying_key();
    client.set_oracle_pubkey(&admin, &BytesN::from_array(&env, pubkey.as_bytes()));

    let rate = 425i128;
    let round = 0u32;
    let sig = signing.sign(&oracle_message(rate, round));
    let oracle_client = crate::tests::test_security_fixes::MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_signed(&rate, &BytesN::from_array(&env, &sig.to_bytes()));

    assert_eq!(client.get_yield_rate(&round), rate);
}

#[test]
fn test_signed_oracle_invalid_signature_rejected() {
    use ed25519_dalek::{Signer, SigningKey};
    let env = Env::default();
    let (client, admin, oracle_id) = setup_circle_with_mock_oracle(&env);

    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let pubkey = signing.verifying_key();
    client.set_oracle_pubkey(&admin, &BytesN::from_array(&env, pubkey.as_bytes()));

    // Sign a *different* rate than the oracle reports — verification must fail.
    let sig = signing.sign(&oracle_message(111, 0));
    let oracle_client = crate::tests::test_security_fixes::MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_signed(&777, &BytesN::from_array(&env, &sig.to_bytes()));

    // Host traps on bad signature (fail-closed): any Err is a rejection.
    assert!(client.try_get_yield_rate(&0).is_err());
}
