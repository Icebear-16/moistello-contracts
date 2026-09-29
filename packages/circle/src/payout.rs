use crate::types::*;
use common::vrf;
use soroban_sdk::{Address, Env, Map, Vec};

fn ensure_active_unpaid_member(circle: &Circle, member: &Member) -> Result<(), CircleError> {
    if member.status != MEMBER_ACTIVE {
        return Err(CircleError::InvalidMemberStatus);
    }
    if (circle.payout_bitmap & (1u128 << member.position)) != 0 {
        return Err(CircleError::PayoutAlreadyExecuted);
    }
    Ok(())
}

pub fn resolve_random(env: &Env, circle: &Circle, round: u32) -> Result<Address, CircleError> {
    let positions = vrf::shuffle_positions(env, circle.max_members, round * 1000)
        .map_err(|_| CircleError::InvalidAmount)?;
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr: Map<u32, Address> = Map::new(env);
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            pos_to_addr.set(m.position, m.address.clone());
        }
    }
    for i in 0..positions.len() {
        let pos = positions.get(i).ok_or(CircleError::NotInitialized)?;
        if (circle.payout_bitmap & (1u128 << pos)) == 0 {
            if let Some(addr) = pos_to_addr.get(pos) {
                return Ok(addr);
            }
        }
    }
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE && (circle.payout_bitmap & (1u128 << m.position)) == 0 {
            return Ok(m.address);
        }
    }
    Err(CircleError::PayoutAlreadyExecuted)
}

pub fn resolve_fixed(env: &Env, circle: &Circle, round: u32) -> Result<Address, CircleError> {
    let pos = (round % circle.max_members) as u32;
    if (circle.payout_bitmap & (1u128 << pos)) != 0 {
        return Err(CircleError::PayoutAlreadyExecuted);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr: Map<u32, Address> = Map::new(env);
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            pos_to_addr.set(m.position, m.address.clone());
        }
    }
    pos_to_addr.get(pos).ok_or(CircleError::NotMember)
}

pub fn resolve_auction(
    env: &Env,
    circle: &Circle,
    round: u32,
) -> Result<(Address, u32), CircleError> {
    let bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut max_bps: u32 = 0;
    let mut winner: Option<AuctionBid> = None;
    for i in 0..bids.len() {
        let b = bids.get(i).ok_or(CircleError::NotInitialized)?;
        if b.round == round && b.discount_bips >= max_bps {
            max_bps = b.discount_bips;
            winner = Some(b);
        }
    }
    let winner_bid = winner.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == winner_bid.bidder {
            ensure_active_unpaid_member(circle, &m)?;
            return Ok((winner_bid.bidder, winner_bid.discount_bips));
        }
    }
    Err(CircleError::NotMember)
}

pub fn resolve_vote(env: &Env, circle: &Circle, round: u32) -> Result<Address, CircleError> {
    let votes: Vec<VoteEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::Votes)
        .unwrap_or_else(|| Vec::new(env));
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut active_members: Map<Address, Member> = Map::new(env);
    let mut saw_inactive_member = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            active_members.set(m.address.clone(), m);
        } else {
            saw_inactive_member = true;
        }
    }
    let quorum = (active_members.len() / 2) + 1;
    let mut tally: Map<Address, u32> = Map::new(env);
    let mut match_count: u32 = 0;
    for i in 0..votes.len() {
        let v = votes.get(i).ok_or(CircleError::NotInitialized)?;
        if v.round == round {
            match_count = match_count
                .checked_add(1)
                .ok_or(CircleError::InvalidAmount)?;
            let c = tally.get(v.vote_for.clone()).unwrap_or(0);
            tally.set(v.vote_for.clone(), c.checked_add(1).ok_or(CircleError::InvalidAmount)?);
        }
    }
    if match_count < quorum {
        return Err(CircleError::VoteQuorumNotMet);
    }
    let mut best: Option<(Address, Member)> = None;
    let mut best_count: u32 = 0;
    for (addr, count) in tally.iter() {
        // A vote may name an address that has since exited or defaulted, or one
        // that was never a member at all. Skip those candidates and fall through
        // to the next-highest tally rather than paying an ineligible address.
        let m = match active_members.get(addr.clone()) {
            Some(m) => m,
            None => continue,
        };
        if count > best_count {
            best_count = count;
            best = Some((addr, m));
        }
    }
    match best {
        Some((winner, member)) => {
            ensure_active_unpaid_member(circle, &member)?;
            Ok(winner)
        }
        // Nobody voted for an eligible member. If the circle still holds a
        // departed member, surface that specific condition so callers can tell
        // "voted for someone who left" apart from "voted for a non-member".
        None if saw_inactive_member => Err(CircleError::InvalidMemberStatus),
        None => Err(CircleError::NotMember),
    }
}
