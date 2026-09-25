use soroban_sdk::{Address,Env,Vec,Map};
use crate::types::*;
use common::{bitmap, vrf};

pub fn resolve_random(env:&Env,circle:&Circle,_round:u32)->Result<Address,CircleError>{
    let positions=vrf::shuffle_positions(env,circle.max_members).map_err(|_| CircleError::InvalidAmount)?;
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr:Map<u32,Address>=Map::new(env);
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status==MEMBER_ACTIVE{
            pos_to_addr.set(m.position,m.address.clone());
        }
    }
    for i in 0..positions.len(){
        let pos=positions.get(i).ok_or(CircleError::NotInitialized)?;
        let paid = bitmap::is_set(circle.payout_bitmap, pos).map_err(|_| CircleError::InvalidAmount)?;
        if !paid {
            if let Some(addr)=pos_to_addr.get(pos){
                return Ok(addr);
            }
        }
    }
    Err(CircleError::PayoutAlreadyExecuted)
}

pub fn resolve_fixed(env:&Env,circle:&Circle,round:u32)->Result<Address,CircleError>{
    let pos=(round%circle.max_members)as u32;
    if bitmap::is_set(circle.payout_bitmap, pos).map_err(|_| CircleError::InvalidAmount)? {
        return Err(CircleError::PayoutAlreadyExecuted);
    }
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr:Map<u32,Address>=Map::new(env);
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status==MEMBER_ACTIVE{
            pos_to_addr.set(m.position,m.address.clone());
        }
    }
    pos_to_addr.get(pos).ok_or(CircleError::NotMember)
}

pub fn resolve_auction(env:&Env,circle:&Circle,round:u32)->Result<(Address,u32),CircleError>{
    let bids:Vec<AuctionBid>=env.storage().persistent().get(&DataKey::Bids).unwrap_or_else(||Vec::new(env));
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut max_bps:u32=0;
    let mut winner:Option<AuctionBid>=None;
    for i in 0..bids.len(){
        let b=bids.get(i).ok_or(CircleError::NotInitialized)?;
        if b.round==round&&b.discount_bips>=max_bps{max_bps=b.discount_bips;winner=Some(b);}
    }
    let winner_bid=winner.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address==winner_bid.bidder{
            if bitmap::is_set(circle.payout_bitmap, m.position).map_err(|_| CircleError::InvalidAmount)? {
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok((winner_bid.bidder,winner_bid.discount_bips));
        }
    }
    Err(CircleError::NotMember)
}

pub fn resolve_vote(env:&Env,circle:&Circle,round:u32)->Result<Address,CircleError>{
    let votes:Vec<VoteEntry>=env.storage().persistent().get(&DataKey::Votes).unwrap_or_else(||Vec::new(env));
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let active=count_active(env)?;
    let quorum=(active/2)+1;
    let mut tally:Map<Address,u32>=Map::new(env);
    let mut match_count:u32=0;
    for i in 0..votes.len(){
        let v=votes.get(i).ok_or(CircleError::NotInitialized)?;
        if v.round==round{
            match_count=match_count.checked_add(1).ok_or(CircleError::InvalidAmount)?;
            let c=tally.get(v.vote_for.clone()).unwrap_or(0);
            tally.set(v.vote_for.clone(),c.checked_add(1).ok_or(CircleError::InvalidAmount)?);
        }
    }
    if match_count<quorum{return Err(CircleError::VoteQuorumNotMet);}
    let mut best_addr:Option<Address>=None;
    let mut best_count:u32=0;
    for(addr,count)in tally.iter(){
        if count>best_count{best_count=count;best_addr=Some(addr);}
    }
    let winner=best_addr.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address==winner{
            if bitmap::is_set(circle.payout_bitmap, m.position).map_err(|_| CircleError::InvalidAmount)? {
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok(winner);
        }
    }
    Err(CircleError::NotMember)
}

fn count_active(env:&Env)->Result<u32,CircleError>{
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut c:u32=0;
    for i in 0..members.len(){
        if members.get(i).ok_or(CircleError::NotInitialized)?.status==MEMBER_ACTIVE{
            c=c.checked_add(1).ok_or(CircleError::InvalidAmount)?;
        }
    }
    Ok(c)
}

