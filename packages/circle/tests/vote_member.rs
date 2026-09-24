use circle::{Circle, CircleArgs, CircleClient, CircleError};
use common::types::CircleConfig;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug)]
struct StoredVote {
    voter: Address,
    vote_for: Address,
    round: u32,
    timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
struct StoredPayout {
    recipient: Address,
    round: u32,
    amount: i128,
    fee: i128,
    payout_type: u32,
    timestamp: u64,
}

fn setup(env: &Env) -> (CircleClient<'_>, Address, Address, Vec<Address>) {
    env.mock_all_auths();
    let organizer = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let config = CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Vote validation"),
        contribution_amount: 100,
        max_members: 3,
        payout_type: 3,
        total_rounds: 1,
        contribution_deadline_seconds: 60,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(env, "vote-validation"),
    };
    let factory = Address::generate(env);
    let contract_id =
        env.register(Circle, CircleArgs::__constructor(&organizer, &factory, &config));
    let client = CircleClient::new(env, &contract_id);
    let mut members = Vec::new(env);
    for _ in 0..3 {
        let member = Address::generate(env);
        assert!(client.try_join(&member).is_ok());
        members.push_back(member);
    }
    let token_client = StellarAssetClient::new(env, &token);
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        token_client.mint(&member, &1_000);
        assert!(client.try_contribute(&member, &100, &0).is_ok());
    }
    (client, organizer, token, members)
}

fn set_votes(env: &Env, contract_id: &Address, votes: &Vec<StoredVote>) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&soroban_sdk::vec![env, symbol_short!("Votes")], votes);
    });
}

fn first_payout_recipient(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        let payouts: Vec<StoredPayout> = env
            .storage()
            .persistent()
            .get(&soroban_sdk::vec![env, symbol_short!("Payouts")])
            .unwrap_or_else(|| Vec::new(env));
        payouts.get(0).map(|payout| payout.recipient)
    })
}

#[test]
fn resolve_vote_uses_active_winner() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let winner = members.get(0).unwrap();

    client.vote_payout(&members.get(0).unwrap(), &winner, &0);
    client.vote_payout(&members.get(1).unwrap(), &winner, &0);
    client.vote_payout(&members.get(2).unwrap(), &winner, &0);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(winner));
}

#[test]
fn resolve_vote_falls_back_from_inactive_winner() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let inactive = members.get(0).unwrap();
    let fallback = members.get(1).unwrap();

    client.vote_payout(&members.get(0).unwrap(), &inactive, &0);
    client.vote_payout(&members.get(1).unwrap(), &inactive, &0);
    client.vote_payout(&members.get(2).unwrap(), &fallback, &0);
    client.exit_circle(&inactive);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(fallback));
}

#[test]
fn resolve_vote_falls_back_from_non_member_winner() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let outsider = Address::generate(&env);
    let fallback = members.get(1).unwrap();
    let mut votes = Vec::new(&env);
    votes.push_back(StoredVote {
        voter: members.get(0).unwrap(),
        vote_for: outsider.clone(),
        round: 0,
        timestamp: 0,
    });
    votes.push_back(StoredVote {
        voter: members.get(1).unwrap(),
        vote_for: outsider,
        round: 0,
        timestamp: 0,
    });
    votes.push_back(StoredVote {
        voter: members.get(2).unwrap(),
        vote_for: fallback.clone(),
        round: 0,
        timestamp: 0,
    });
    set_votes(&env, &client.address, &votes);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(fallback));
}

#[test]
fn resolve_vote_rejects_when_no_active_winner_exists() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let outsider =
        Address::from_str(&env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF");
    let mut votes = Vec::new(&env);
    for i in 0..3 {
        votes.push_back(StoredVote {
            voter: members.get(i).unwrap(),
            vote_for: outsider.clone(),
            round: 0,
            timestamp: 0,
        });
    }
    set_votes(&env, &client.address, &votes);

    assert_eq!(client.try_trigger_payout(&organizer, &0), Err(Ok(CircleError::NotMember)));
}
