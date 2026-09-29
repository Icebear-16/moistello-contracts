#![cfg(test)]

//! Regression suite for #101 — vote payout must never pay an ineligible address.
//!
//! `resolve_vote` used to pick the highest-voted address without re-checking
//! membership, so a vote cast for a member who has since exited/defaulted (or
//! for a non-member address) could be resolved as the payout recipient. The
//! resolver now tallies only active members and falls through to the
//! next-highest eligible candidate, returning `NotMember` when none exists.

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{DataKey, Member, PayoutRecipient, VoteEntry, MEMBER_EXITED};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

const CONTRIBUTION: i128 = 100_0000000;

type Client<'a> = CircleClient<'a>;

fn setup(env: &Env) -> (Client<'_>, Address, Vec<Address>) {
    env.mock_all_auths();
    let organizer = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let factory = Address::generate(env);
    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Vote validation"),
        contribution_amount: CONTRIBUTION,
        max_members: 3,
        payout_type: crate::types::PAYOUT_VOTE,
        total_rounds: 1,
        contribution_deadline_seconds: 604800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(env, "vote-validation"),
    };
    let contract_id =
        env.register(Circle, CircleArgs::__constructor(&organizer, &factory, &config));
    let client = Client::new(env, &contract_id);

    let token_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    let mut members = Vec::new(env);
    for _ in 0..3 {
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + 101);
        let member = Address::generate(env);
        token_client.mint(&member, &100_000_0000000);
        client.join(&member);
        members.push_back(member);
    }
    for i in 0..members.len() {
        client.contribute(&members.get(i).unwrap(), &CONTRIBUTION, &0);
    }

    (client, organizer, members)
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
fn resolve_vote_pays_the_highest_voted_active_member() {
    let env = Env::default();
    let (client, organizer, members) = setup(&env);
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
fn resolve_vote_falls_back_when_top_candidate_has_exited() {
    let env = Env::default();
    let (client, organizer, members) = setup(&env);
    let exited = members.get(0).unwrap();
    let fallback = members.get(1).unwrap();
    let mut votes = Vec::new(&env);
    for i in 0..2 {
        votes.push_back(VoteEntry {
            voter: members.get(i).unwrap(),
            vote_for: exited.clone(),
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
    set_member_status(&env, &client.address, &exited, MEMBER_EXITED);

    assert!(client.try_trigger_payout(&organizer, &0).is_ok());
    assert_eq!(first_payout_recipient(&env, &client.address), Some(fallback));
}

#[test]
fn resolve_vote_ignores_candidates_that_are_not_members() {
    let env = Env::default();
    let (client, organizer, members) = setup(&env);
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
fn resolve_vote_rejects_when_no_candidate_is_eligible() {
    let env = Env::default();
    let (client, organizer, members) = setup(&env);
    let zero = Address::from_string(&String::from_str(
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
