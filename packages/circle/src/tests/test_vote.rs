#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{CircleConfig, DataKey, Member, PayoutRecipient, VoteEntry};
use crate::{Circle, CircleClient, CircleError};

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
    let contract_id = env.register(Circle, (&organizer, &factory, &config));
    let client = CircleClient::new(env, &contract_id);
    let mut members = Vec::new(env);

    for _ in 0..3 {
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + 101);
        let member = Address::generate(env);
        client.join(&member);
        members.push_back(member);
    }

    let token_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        token_client.mint(&member, &1_000);
        client.contribute(&member, &100, &0);
    }

    (client, organizer, token, members)
}

fn store_votes(env: &Env, contract_id: &Address, votes: &Vec<VoteEntry>) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKey::Votes, votes);
    });
}

fn set_member_status(env: &Env, contract_id: &Address, member: &Address, status: u32) {
    env.as_contract(contract_id, || {
        let mut members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).unwrap();
        for i in 0..members.len() {
            let mut entry = members.get(i).unwrap();
            if entry.address == *member {
                entry.status = status;
                members.set(i, entry);
            }
        }
        env.storage().persistent().set(&DataKey::Members, &members);
    });
}

fn first_payout_recipient(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        let payouts: Vec<PayoutRecipient> = env
            .storage()
            .persistent()
            .get(&DataKey::Payouts)
            .unwrap_or_else(|| Vec::new(env));
        payouts.get(0).map(|payout| payout.recipient)
    })
}

#[test]
fn resolve_vote_uses_active_member_winner() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let winner = members.get(0).unwrap();
    let mut votes = Vec::new(&env);
    for i in 0..3 {
        votes.push_back(VoteEntry {
            voter: members.get(i).unwrap(),
            vote_for: winner.clone(),
            round: 0,
            timestamp: 0,
        });
    }
    store_votes(&env, &client.address, &votes);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(winner));
}

#[test]
fn resolve_vote_falls_back_from_inactive_winner() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let inactive = members.get(0).unwrap();
    let fallback = members.get(1).unwrap();
    let mut votes = Vec::new(&env);
    for i in 0..2 {
        votes.push_back(VoteEntry {
            voter: members.get(i).unwrap(),
            vote_for: inactive.clone(),
            round: 0,
            timestamp: 0,
        });
    }
    votes.push_back(VoteEntry {
        voter: members.get(2).unwrap(),
        vote_for: fallback.clone(),
        round: 0,
        timestamp: 0,
    });
    store_votes(&env, &client.address, &votes);
    set_member_status(&env, &client.address, &inactive, crate::types::MEMBER_EXITED);

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
    for i in 0..2 {
        votes.push_back(VoteEntry {
            voter: members.get(i).unwrap(),
            vote_for: outsider.clone(),
            round: 0,
            timestamp: 0,
        });
    }
    votes.push_back(VoteEntry {
        voter: members.get(2).unwrap(),
        vote_for: fallback.clone(),
        round: 0,
        timestamp: 0,
    });
    store_votes(&env, &client.address, &votes);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(fallback));
}

#[test]
fn resolve_vote_rejects_zero_address_when_no_eligible_winner_exists() {
    let env = Env::default();
    let (client, organizer, _token, members) = setup(&env);
    let zero = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let mut votes = Vec::new(&env);
    for i in 0..3 {
        votes.push_back(VoteEntry {
            voter: members.get(i).unwrap(),
            vote_for: zero.clone(),
            round: 0,
            timestamp: 0,
        });
    }
    store_votes(&env, &client.address, &votes);

    assert_eq!(client.try_trigger_payout(&organizer, &0), Err(Ok(CircleError::NotMember)));
}
