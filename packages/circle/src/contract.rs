use crate::oracle;
// `payout` is imported as a module-level path. `load_round_details` is an
// internal helper used only within payout.rs and must NOT be re-exported here;
// doing so would create an unused import (fixes #271).
use crate::payout;
use crate::types::*;
// Reentrancy protection comes from the shared common module — there is intentionally
// no local reentrancy.rs in this package. See packages/common/src/reentrancy.rs for
// the canonical implementation and the rationale for centralisation.
use common::reentrancy::ReentrancyGuard;
use common::{math, pause};
use reputation_registry::scoring;
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    symbol_short, Address, BytesN, Env, IntoVal, Map, Vec,
};

/// Initializes a new circle contract with the provided configuration.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Administrator address with elevated privileges
/// - `factory`: Factory contract address that deployed this circle
/// - `config`: Circle configuration including organizer, token, contribution amount, max members, payout type, and rounds
///
/// # Returns
/// - `Ok(())` on successful initialization
/// - `Err(CircleError::InvalidAmount)` if config validation fails (max_members < 2, contribution_amount <= 0, total_rounds == 0, or payout_type > 3)
/// - `Err(CircleError::CircleSizeExceedsTier)` if max_members exceeds organizer's tier limit
/// - `Err(CircleError::ContributionExceedsTier)` if contribution_amount exceeds organizer's tier limit
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn init(
    env: &Env,
    admin: &Address,
    factory: &Address,
    config: &CircleConfig,
) -> Result<(), CircleError> {
    if config.max_members < 2
        || config.contribution_amount <= 0
        || config.total_rounds == 0
        || config.payout_type > 3
    {
        return Err(CircleError::InvalidAmount);
    }
    if config.max_members > scoring::max_circle_size(env, &config.organizer) {
        return Err(CircleError::CircleSizeExceedsTier);
    }
    if config.contribution_amount > scoring::max_contribution(env, &config.organizer) {
        return Err(CircleError::ContributionExceedsTier);
    }
    let circle = Circle {
        id: env.current_contract_address(),
        token: config.token.clone(),
        name: config.name.clone(),
        organizer: config.organizer.clone(),
        factory: factory.clone(),
        contribution_amount: config.contribution_amount,
        max_members: config.max_members,
        member_count: 0,
        payout_type: config.payout_type,
        total_rounds: config.total_rounds,
        current_round: 0,
        status: STATUS_PENDING,
        started_at: 0,
        created_at: env.ledger().timestamp(),
        contribution_deadline_seconds: config.contribution_deadline_seconds,
        min_moi_score: config.min_moi_score,
        collateral_amount: config.collateral_amount,
        penalty_bps: config.penalty_bps,
        grace_period_seconds: config.grace_period_seconds,
        max_strikes: config.max_strikes,
        payout_bitmap: 0,
        total_payouts: 0,
        total_fees: 0,
        slug: config.slug.clone(),
    };
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Factory, factory);
    env.storage()
        .persistent()
        .set(&DataKey::Members, &Vec::<Member>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Contributions, &Vec::<Contribution>::new(env));
    common::vrf::init_vrf(env, None).map_err(|_| CircleError::InvalidAmount)?;
    env.storage()
        .persistent()
        .set(&DataKey::Payouts, &Vec::<PayoutRecipient>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Bids, &Vec::<AuctionBid>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Votes, &Vec::<VoteEntry>::new(env));
    Ok(())
}
/// Allows a member to join an active circle.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address of the member attempting to join
///
/// # Returns
/// - `Ok(())` on successful join
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails or circle status is DISPUTED/COMPLETED
/// - `Err(CircleError::InsufficientMoiScore)` if member's MoiScore is below minimum threshold
/// - `Err(CircleError::AllowlistNotPermitted)` if allowlist is configured and member is not on it
/// - `Err(CircleError::AlreadyMember)` if member has already joined
/// - `Err(CircleError::CircleFull)` if max_members limit has been reached
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `member` address.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn join(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status != STATUS_PENDING {
        return Err(CircleError::NotActive);
    }
    let score = scoring::get_score(env, member);
    if score < circle.min_moi_score {
        return Err(CircleError::InsufficientMoiScore);
    }
    let allowlist: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Allowlist)
        .unwrap_or_else(|| Vec::new(env));
    if allowlist.len() > 0 {
        let mut permitted = false;
        for i in 0..allowlist.len() {
            if allowlist.get(i).ok_or(CircleError::VecAccessError)? == *member {
                permitted = true;
                break;
            }
        }
        if !permitted {
            return Err(CircleError::AllowlistNotPermitted);
        }
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..members.len() {
        if members.get(i).ok_or(CircleError::VecAccessError)?.address == *member {
            return Err(CircleError::AlreadyMember);
        }
    }
    if members.len() as u32 >= circle.max_members {
        return Err(CircleError::CircleFull);
    }
    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        token_client.transfer(member, &circle.id, &circle.collateral_amount);
    }
    let now = env.ledger().timestamp();
    let pos = members.len() as u32;
    members.push_back(Member {
        address: member.clone(),
        position: pos,
        joined_at: now,
        strikes: 0,
        status: MEMBER_ACTIVE,
        exited_at: 0,
        total_contributions: 0,
        total_received: 0,
    });
    circle.member_count = circle
        .member_count
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    if circle.member_count >= circle.max_members && circle.status == STATUS_PENDING {
        circle.status = STATUS_ACTIVE;
        circle.started_at = now;
    }
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Members, &members);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("joined")),
        MemberJoined {
            member: member.clone(),
            position: pos,
        },
    );
    Ok(())
}
/// Records a contribution from a circle member for the current round.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address of the contributing member
/// - `amount`: Contribution amount (must exactly match circle's configured contribution_amount)
/// - `round`: Round number (must match circle's current_round)
///
/// # Returns
/// - `Ok(())` on successful contribution
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails or circle status is not ACTIVE
/// - `Err(CircleError::RoundNotCurrent)` if provided round does not match current_round
/// - `Err(CircleError::ContributionMismatch)` if amount does not match configured contribution_amount
/// - `Err(CircleError::NotMember)` if member has not joined the circle
/// - `Err(CircleError::InvalidMemberStatus)` if member status is not ACTIVE
/// - `Err(CircleError::AlreadyContributed)` if member has already contributed for this round
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `member` address.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn contribute(
    env: &Env,
    member: &Address,
    amount: i128,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if amount != circle.contribution_amount {
        return Err(CircleError::ContributionMismatch);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            found = true;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut contribution_map: Map<(Address, u32), bool> = env
        .storage()
        .persistent()
        .get(&symbol_short!("contribs"))
        .unwrap_or_else(|| {
            let mut m = Map::new(env);
            for i in 0..contributions.len() {
                if let Some(c) = contributions.get(i) {
                    m.set((c.member.clone(), c.round), true);
                }
            }
            env.storage().persistent().set(&symbol_short!("contribs"), &m);
            m
        });
    if contribution_map.get((member.clone(), round)).unwrap_or(false) {
        return Err(CircleError::AlreadyContributed);
    }
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    token_client.transfer(member, &circle.id, &amount);
    let now = env.ledger().timestamp();
    let on_time = now
        <= circle
            .started_at
            .checked_add(circle.contribution_deadline_seconds)
            .ok_or(CircleError::InvalidAmount)?;
    contributions.push_back(Contribution {
        member: member.clone(),
        round,
        amount,
        timestamp: now,
        on_time,
        time_weight: now,
    });
    env.storage()
        .persistent()
        .set(&DataKey::Contributions, &contributions);
    contribution_map.set((member.clone(), round), true);
    env.storage()
        .persistent()
        .set(&symbol_short!("contribs"), &contribution_map);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("contrib")),
        ContributionRecorded {
            member: member.clone(),
            round,
            amount,
            on_time,
        },
    );
    let registry_opt = env.storage().instance().get::<_, Address>(&DataKey::ReputationRegistry);
    let mut fallback = true;
    if let Some(registry) = &registry_opt {
        let args: soroban_sdk::Vec<soroban_sdk::Val> = (member.clone(), 1u32, 1u32).into_val(env);
        if env.try_invoke_contract::<(), soroban_sdk::Error>(registry, &soroban_sdk::Symbol::new(env, "record"), args).is_ok() {
            fallback = false;
        }
    }
    if fallback {
        scoring::record_on_time_payment(env, member, &circle.id, amount, round);
    }
    Ok(())
}
/// Triggers payout for the current round based on the circle's payout type.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `caller`: Address triggering the payout (must be organizer or admin)
/// - `round`: Round number (must match circle's current_round)
///
/// # Returns
/// - `Ok(())` on successful payout
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails or circle status is not ACTIVE
/// - `Err(CircleError::Unauthorized)` if caller is neither organizer nor admin
/// - `Err(CircleError::RoundNotCurrent)` if provided round does not match current_round
/// - `Err(CircleError::InvalidPayoutType)` if payout_type is invalid
/// - `Err(CircleError::InvalidAmount)` if math operations fail
/// - Other errors propagated from payout resolution functions
///
/// # Authorization
/// Requires caller to be the circle organizer or admin.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.

fn deposit_protocol_fee(
    env: &Env,
    token: &Address,
    treasury: &Address,
    circle_id: &Address,
    amount: i128,
) {
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: symbol_short!("transfer"),
                args: soroban_sdk::vec![
                    env,
                    circle_id.into_val(env),
                    treasury.into_val(env),
                    amount.into_val(env),
                ],
            },
            sub_invocations: soroban_sdk::vec![env],
        }),
    ]);
    treasury::TreasuryClient::new(env, treasury).deposit_fee(circle_id, &amount, circle_id);
}

pub fn trigger_payout(env: &Env, caller: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if caller != &circle.organizer && caller != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let (recipient, payout_type) = match circle.payout_type {
        PAYOUT_RANDOM => (payout::resolve_random(env, &circle, round)?, PAYOUT_RANDOM),
        PAYOUT_FIXED => (payout::resolve_fixed(env, &circle, round)?, PAYOUT_FIXED),
        PAYOUT_AUCTION => {
            let (w, _) = payout::resolve_auction(env, &circle, round)?;
            (w, PAYOUT_AUCTION)
        }
        PAYOUT_VOTE => (payout::resolve_vote(env, &circle, round)?, PAYOUT_VOTE),
        _ => return Err(CircleError::InvalidPayoutType),
    };
    let pool = math::safe_mul(circle.contribution_amount, circle.member_count as i128)
        .map_err(|_| CircleError::InvalidAmount)?;
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0u32);
    let (net, fee) =
        math::apply_fee(pool, fee_bps as i128).map_err(|_| CircleError::InvalidAmount)?;
    if net <= 0 {
        return Err(CircleError::ZeroPayoutAmount);
    }
    // Fetch yield rate for observability; zero if no oracle configured.
    let _yield_rate_bps = oracle::get_yield_rate(env, round)?;
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let now = env.ledger().timestamp();
    let all_contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut total_weighted: u128 = 0;
    let mut member_weighted: Map<Address, u128> = Map::new(env);
    for i in 0..all_contributions.len() {
        let c = all_contributions
            .get(i)
            .ok_or(CircleError::VecAccessError)?;
        if c.round == round {
            let time_held = (now as u128).saturating_sub(c.timestamp as u128);
            let w = (c.amount as u128).saturating_mul(time_held);
            total_weighted = total_weighted.saturating_add(w);
            let prev = member_weighted.get(c.member.clone()).unwrap_or(0);
            member_weighted.set(c.member.clone(), prev.saturating_add(w));
        }
    }
    let mut payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    payouts.push_back(PayoutRecipient {
        recipient: recipient.clone(),
        round,
        amount: net,
        fee,
        payout_type,
        timestamp: now,
    });
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut distributed: i128 = 0;
    let net_u = net as u128;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if let Some(w) = member_weighted.get(m.address.clone()) {
            if total_weighted > 0 {
                let share = if distributed == 0 && w == total_weighted {
                    net
                } else {
                    (net_u.saturating_mul(w) / total_weighted) as i128
                };
                if share > 0 {
                    token_client.transfer(&circle.id, &m.address, &share);
                    distributed = math::safe_add(distributed, share)
                        .map_err(|_| CircleError::InvalidAmount)?;
                    payouts.push_back(PayoutRecipient {
                        recipient: m.address.clone(),
                        round,
                        amount: share,
                        fee: 0,
                        payout_type,
                        timestamp: now,
                    });
                    for j in 0..members.len() {
                        let mut m2 = members.get(j).ok_or(CircleError::VecAccessError)?;
                        if m2.address == m.address {
                            m2.total_received = math::safe_add(m2.total_received, share)
                                .map_err(|_| CircleError::InvalidAmount)?;
                            members.set(j, m2);
                        }
                    }
                }
            }
        }
    }
    if fee > 0 {
        if let Some(treasury) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Treasury)
        {
            deposit_protocol_fee(env, &circle.token, &treasury, &circle.id, fee);
        }
    }
    if distributed < net {
        let dust = math::safe_sub(net, distributed).map_err(|_| CircleError::InvalidAmount)?;
        token_client.transfer(&circle.id, &recipient, &dust);
        payouts.push_back(PayoutRecipient {
            recipient: recipient.clone(),
            round,
            amount: dust,
            fee: 0,
            payout_type,
            timestamp: now,
        });
        for j in 0..members.len() {
            let mut m2 = members.get(j).ok_or(CircleError::VecAccessError)?;
            if m2.address == recipient.clone() {
                m2.total_received = math::safe_add(m2.total_received, dust)
                    .map_err(|_| CircleError::InvalidAmount)?;
                members.set(j, m2);
            }
        }
        distributed = math::safe_add(distributed, dust).map_err(|_| CircleError::InvalidAmount)?;
    }
    circle.current_round = circle
        .current_round
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    circle.total_payouts = math::safe_add(circle.total_payouts, distributed)
        .map_err(|_| CircleError::InvalidAmount)?;
    circle.total_fees =
        math::safe_add(circle.total_fees, fee).map_err(|_| CircleError::InvalidAmount)?;
    if circle.current_round >= circle.total_rounds {
        circle.status = STATUS_COMPLETED;
    }
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("payout")),
        PayoutExecuted {
            recipient: recipient.clone(),
            round,
            amount: distributed,
            fee,
            payout_type,
        },
    );
    let registry_opt = env.storage().instance().get::<_, Address>(&DataKey::ReputationRegistry);
    if let Some(registry) = &registry_opt {
        let args: soroban_sdk::Vec<soroban_sdk::Val> = (recipient.clone(), 4u32, 1u32).into_val(env);
        let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(registry, &soroban_sdk::Symbol::new(env, "record"), args);
    }
    if circle.status == STATUS_COMPLETED {
        env.events().publish(
            (env.current_contract_address(), symbol_short!("complete")),
            CircleCompleted {
                total_payouts: circle.total_payouts,
            },
        );
        let mut active_count = 0;
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::NotInitialized)?;
            if m.status == MEMBER_ACTIVE {
                active_count += 1;
            }
        }
        let balance = token_client.balance(&circle.id);
        let share = if active_count > 0 { balance / (active_count as i128) } else { 0 };
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::NotInitialized)?;
            if m.status == MEMBER_ACTIVE {
                if share > 0 {
                    token_client.transfer(&circle.id, &m.address, &share);
                }
                let mut fallback = true;
                if let Some(registry) = &registry_opt {
                    let args: soroban_sdk::Vec<soroban_sdk::Val> = (m.address.clone(), 2u32, 5u32).into_val(env);
                    if env.try_invoke_contract::<(), soroban_sdk::Error>(registry, &soroban_sdk::Symbol::new(env, "record"), args).is_ok() {
                        fallback = false;
                    }
                }
                if fallback {
                    scoring::record_circle_completion(env, &m.address);
                }
            }
        }
    }
    Ok(())
}
/// Submits a bid for auction-based payout rounds.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `bidder`: Address placing the bid
/// - `discount_bips`: Discount rate in basis points (0-10000, where 10000 = 100%)
/// - `round`: Round number (must match circle's current_round)
///
/// # Returns
/// - `Ok(())` on successful bid placement
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails
/// - `Err(CircleError::InvalidPayoutType)` if circle's payout_type is not PAYOUT_AUCTION
/// - `Err(CircleError::InvalidBid)` if discount_bips > 10000
/// - `Err(CircleError::RoundNotCurrent)` if provided round does not match current_round
/// - `Err(CircleError::AlreadyBidded)` if bidder has already placed a bid for this round
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `bidder` address.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn auction_bid(
    env: &Env,
    bidder: &Address,
    discount_bips: u32,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    bidder.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.payout_type != PAYOUT_AUCTION {
        return Err(CircleError::InvalidPayoutType);
    }
    if discount_bips > 10000 {
        return Err(CircleError::InvalidBid);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let mut bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..bids.len() {
        let b = bids.get(i).ok_or(CircleError::VecAccessError)?;
        if b.bidder == *bidder && b.round == round {
            return Err(CircleError::AlreadyBidded);
        }
    }
    bids.push_back(AuctionBid {
        bidder: bidder.clone(),
        discount_bips,
        round,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Bids, &bids);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("bid")),
        AuctionBidPlaced {
            bidder: bidder.clone(),
            discount_bips,
            round,
        },
    );
    Ok(())
}
/// Casts a vote for a payout recipient in vote-based payout rounds.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `voter`: Address casting the vote (must be an active member)
/// - `vote_for`: Address being voted for as payout recipient
/// - `round`: Round number (must match circle's current_round)
///
/// # Returns
/// - `Ok(())` on successful vote
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails
/// - `Err(CircleError::InvalidPayoutType)` if circle's payout_type is not PAYOUT_VOTE
/// - `Err(CircleError::RoundNotCurrent)` if provided round does not match current_round
/// - `Err(CircleError::NotMember)` if voter is not a member
/// - `Err(CircleError::InvalidMemberStatus)` if voter's status is not ACTIVE
/// - `Err(CircleError::AlreadyVoted)` if voter has already voted for this round
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `voter` address.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn vote_payout(
    env: &Env,
    voter: &Address,
    vote_for: &Address,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    voter.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.payout_type != PAYOUT_VOTE {
        return Err(CircleError::InvalidPayoutType);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut is_member = false;
    let mut is_vote_for_member = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *voter {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            is_member = true;
        }
        if m.address == *vote_for {
            if m.status == MEMBER_ACTIVE {
                is_vote_for_member = true;
            }
        }
    }
    if !is_member || !is_vote_for_member {
        return Err(CircleError::NotMember);
    }
    let mut votes: Vec<VoteEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::Votes)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..votes.len() {
        let v = votes.get(i).ok_or(CircleError::VecAccessError)?;
        if v.voter == *voter && v.round == round {
            return Err(CircleError::AlreadyVoted);
        }
    }
    votes.push_back(VoteEntry {
        voter: voter.clone(),
        vote_for: vote_for.clone(),
        round,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Votes, &votes);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("vote")),
        VoteCast {
            voter: voter.clone(),
            vote_for: vote_for.clone(),
            round,
        },
    );
    Ok(())
}
/// Allows a member to exit the circle with an early withdrawal penalty.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address of the member exiting
///
/// # Returns
/// - `Ok(())` on successful exit
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails or circle status is COMPLETED
/// - `Err(CircleError::InvalidMemberStatus)` if member's status is not ACTIVE
/// - `Err(CircleError::InvalidAmount)` if penalty calculation fails
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `member` address.
///
/// # Notes
/// - 5% penalty is calculated on total contributions made
/// - Member's collateral is returned if configured
/// - Member status is set to MEMBER_EXITED
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn exit(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status == STATUS_COMPLETED {
        return Err(CircleError::NotActive);
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut penalty: i128 = 0;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            let contributions: Vec<Contribution> = env
                .storage()
                .persistent()
                .get(&DataKey::Contributions)
                .unwrap_or_else(|| Vec::new(env));
            let mut ctotal: i128 = 0;
            for j in 0..contributions.len() {
                let c = contributions.get(j).ok_or(CircleError::VecAccessError)?;
                if c.member == *member {
                    ctotal =
                        math::safe_add(ctotal, c.amount).map_err(|_| CircleError::InvalidAmount)?;
                }
            }
            penalty =
                math::calculate_percentage(ctotal, 500).map_err(|_| CircleError::InvalidAmount)?;
            m.status = MEMBER_EXITED;
            m.exited_at = env.ledger().timestamp();
            members.set(i, m);
        }
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        token_client.transfer(&circle.id, member, &circle.collateral_amount);
    }
    env.events().publish(
        (env.current_contract_address(), symbol_short!("exited")),
        MemberExited {
            member: member.clone(),
            penalty,
        },
    );
    Ok(())
}
/// Reports a member for late payment and increments their strike count.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `reporter`: Address reporting the late payment
/// - `late_member`: Address of the member being reported
/// - `round`: Round number for the late payment
///
/// # Returns
/// - `Ok(())` on successful report
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails or circle status is not ACTIVE
/// - `Err(CircleError::NotMember)` if late_member has no late contribution for the specified round
/// - `Err(CircleError::VecAccessError)` if vector access fails
///
/// # Authorization
/// Requires authentication from the `reporter` address.
///
/// # Notes
/// - Member must have a contribution marked as not on_time for the specified round
/// - If strikes >= max_strikes, member status is set to MEMBER_DEFAULTED
/// - Default event is published when member is defaulted
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn report_late(
    env: &Env,
    reporter: &Address,
    late_member: &Address,
    round: u32,
) -> Result<(), CircleError> {
    reporter.require_auth();
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut found = false;
    for i in 0..contributions.len() {
        let c = contributions.get(i).ok_or(CircleError::VecAccessError)?;
        if c.member == *late_member && c.round == round && !c.on_time {
            found = true;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *late_member {
            m.strikes = m.strikes.wrapping_add(1);
            m.strikes = m.strikes.checked_add(1).ok_or(CircleError::InvalidAmount)?;
            if m.strikes >= circle.max_strikes {
                m.status = MEMBER_DEFAULTED;
                scoring::record_default(env, &m.address);
                env.events().publish(
                    (env.current_contract_address(), symbol_short!("default")),
                    MemberDefaulted {
                        member: late_member.clone(),
                        strikes: m.strikes,
                    },
                );
            }
            members.set(i, m);
        }
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    Ok(())
}
/// Cancels a pending circle before it starts and refunds any collected collateral.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `caller`: Address of the organizer requesting cancellation
///
/// # Returns
/// - `Ok(())` on successful cancellation
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if circle is not in pending status or reentrancy guard fails
/// - `Err(CircleError::NotOrganizer)` if caller is not the circle organizer
///
/// # Authorization
/// Requires authentication from the organizer `caller`.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn cancel_circle(env: &Env, caller: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    caller.require_auth();

    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;

    if *caller != circle.organizer {
        return Err(CircleError::NotOrganizer);
    }

    if circle.status != STATUS_PENDING {
        return Err(CircleError::NotActive);
    }

    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    if contributions.len() > 0 {
        return Err(CircleError::ContributionsExist);
    }

    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;

    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::VecAccessError)?;
            if m.status == MEMBER_ACTIVE {
                token_client.transfer(&circle.id, &m.address, &circle.collateral_amount);
            }
        }
    }

    circle.status = STATUS_CANCELLED;
    env.storage().instance().set(&DataKey::Circle, &circle);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("cancel")),
        CircleCancelled {
            circle_id: circle.id.clone(),
            cancelled_by: caller.clone(),
            cancelled_at: env.ledger().timestamp(),
        },
    );

    Ok(())
}

pub fn cancel(env: &Env, caller: &Address) -> Result<(), CircleError> {
    cancel_circle(env, caller)
}

/// Raises a dispute against the circle, pausing all operations until resolved.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address raising the dispute
/// - `evidence_hash`: Hash of evidence supporting the dispute
///
/// # Returns
/// - `Ok(())` on successful dispute creation
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::NotActive)` if reentrancy guard fails
/// - `Err(CircleError::DisputeAlreadyRaised)` if circle status is already DISPUTED or a dispute entry exists
///
/// # Authorization
/// Requires authentication from the `member` address.
///
/// # Notes
/// - Circle status is immediately set to STATUS_DISPUTED
/// - Dispute entry is stored with timestamp and evidence hash
/// - All circle operations are blocked until dispute is resolved
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn raise_dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status == STATUS_DISPUTED {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            found = true;
            break;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    if env
        .storage()
        .persistent()
        .get::<DataKey, DisputeEntry>(&DataKey::Dispute)
        .is_some()
    {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    circle.status = STATUS_DISPUTED;
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(
        &DataKey::Dispute,
        &DisputeEntry {
            raised_by: member.clone(),
            evidence_hash: evidence_hash.clone(),
            raised_at: env.ledger().timestamp(),
            resolved_at: 0,
            resolution: 0,
            resolved_by: env.current_contract_address(),
        },
    );
    env.events().publish(
        (env.current_contract_address(), symbol_short!("disputed")),
        DisputeRaised {
            member: member.clone(),
            evidence_hash: evidence_hash.clone(),
        },
    );
    Ok(())
}

pub fn dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    raise_dispute(env, member, evidence_hash)
}
/// Resolves an active dispute and restores circle to ACTIVE status.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address resolving the dispute
/// - `resolution`: Resolution type (1=DISMISS, 2=PENALIZE, 3=FORCE_PAYOUT)
///
/// # Returns
/// - `Ok(())` on successful resolution
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
/// - `Err(CircleError::NoActiveDispute)` if no dispute entry exists
/// - `Err(CircleError::InvalidAmount)` if resolution value is invalid
///
/// # Authorization
/// Requires authentication from the admin address and admin must match stored admin.
///
/// # Notes
/// - Circle status is restored to STATUS_ACTIVE after resolution
/// - Resolution and timestamp are recorded in the dispute entry
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn resolve_dispute(env: &Env, admin: &Address, resolution: u32) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    let mut dispute: DisputeEntry = env
        .storage()
        .persistent()
        .get(&DataKey::Dispute)
        .ok_or(CircleError::NoActiveDispute)?;
    if resolution > 4 {
        return Err(CircleError::InvalidAmount);
    }
    match resolution {
        RESOLVE_DISMISS | RESOLVE_PENALIZE | RESOLVE_FORCE_PAYOUT => {
            circle.status = STATUS_ACTIVE;
        }
        4 => {
            circle.status = STATUS_CANCELLED;
            // Refund all members' contributions
            let token_address = circle.token.clone();
            let token_client = soroban_sdk::token::Client::new(env, &token_address);
            let mut members: Vec<Member> = env
                .storage()
                .persistent()
                .get(&DataKey::Members)
                .unwrap_or_else(|| Vec::new(env));
            for i in 0..members.len() {
                if let Some(mut m) = members.get(i) {
                    if m.total_contributions > 0 {
                        token_client.transfer(
                            &env.current_contract_address(),
                            &m.address,
                            &m.total_contributions,
                        );
                        m.total_contributions = 0;
                        members.set(i, m);
                    }
                }
            }
            env.storage().persistent().set(&DataKey::Members, &members);
        }
        _ => return Err(CircleError::InvalidAmount),
    }
    dispute.resolved_at = env.ledger().timestamp();
    dispute.resolution = resolution;
    dispute.resolved_by = admin.clone();
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Dispute, &dispute);
    Ok(())
}
/// Returns the current status of the circle including all configuration and state data.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// Circle struct containing all circle state or a default Circle if not initialized.
///
/// # Panics
/// Never panics. Returns default Circle if storage is empty.
pub fn get_status(env: &Env) -> Circle {
    env.storage()
        .instance()
        .get(&DataKey::Circle)
        .unwrap_or(Circle {
            id: env.current_contract_address(),
            token: env.current_contract_address(),
            name: soroban_sdk::String::from_str(env, ""),
            organizer: env.current_contract_address(),
            factory: env.current_contract_address(),
            contribution_amount: 0,
            max_members: 0,
            member_count: 0,
            payout_type: 0,
            total_rounds: 0,
            current_round: 0,
            status: 3,
            started_at: 0,
            created_at: 0,
            contribution_deadline_seconds: 0,
            min_moi_score: 0,
            collateral_amount: 0,
            penalty_bps: 0,
            grace_period_seconds: 0,
            max_strikes: 0,
            payout_bitmap: 0,
            total_payouts: 0,
            total_fees: 0,
            slug: soroban_sdk::String::from_str(env, ""),
        })
}
/// Returns all members who have joined the circle.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// Vector of Member structs containing address, position, joined_at, strikes, status, and contribution/receipt totals.
///
/// # Panics
/// Never panics. Returns empty vector if no members exist.
pub fn get_members(env: &Env) -> Vec<Member> {
    env.storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env))
}
/// Returns all contributions made by a specific member.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address of the member to query
///
/// # Returns
/// Vector of Contribution structs for the specified member containing round, amount, timestamp, and on_time status.
///
/// # Panics
/// Never panics. Returns empty vector if member has made no contributions.
pub fn get_contributions(
    env: &Env,
    member: &Address,
    page: u32,
    page_size: u32,
) -> Vec<Contribution> {
    let all: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut out = Vec::new(env);
    let start = page.saturating_mul(page_size);
    let end = start.saturating_add(page_size);
    let mut count = 0u32;
    for i in 0..all.len() {
        if let Some(c) = all.get(i) {
            if c.member == *member {
                if count >= start && count < end {
                    out.push_back(c);
                }
                count += 1;
            }
        }
    }
    out
}
/// Calculates the potential payout amount for a member in the current round.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `member`: Address of the member to query
///
/// # Returns
/// - `Some(amount)` if the member is eligible for payout in the current round
/// - `None` if circle is not ACTIVE, member is not eligible, or calculation fails
///
/// # Notes
/// - Calculation is based on payout type (FIXED, RANDOM, AUCTION, or VOTE)
/// - For FIXED: member must be at the round's rotation position
/// - For RANDOM: any active member can receive
/// - For AUCTION: member must have winning bid
/// - For VOTE: member must have most votes
///
/// # Panics
/// Never panics. Returns None on any error or ineligibility.
pub fn get_pending_payout(env: &Env, member: &Address) -> Option<i128> {
    let circle: Circle = match env.storage().instance().get(&DataKey::Circle) {
        Some(c) => c,
        None => return None,
    };
    if circle.status != STATUS_ACTIVE {
        return None;
    }
    let pool = match math::safe_mul(circle.contribution_amount, circle.member_count as i128) {
        Ok(p) => p,
        Err(_) => return None,
    };
    let (net, _) = match math::apply_fee(pool, 0) {
        Ok(v) => v,
        Err(_) => return None,
    };
    let round = circle.current_round;
    let members: Vec<Member> = match env.storage().persistent().get(&DataKey::Members) {
        Some(m) => m,
        None => return None,
    };
    match circle.payout_type {
        PAYOUT_FIXED => {
            let pos = round % circle.max_members;
            if (circle.payout_bitmap & (1u128 << pos)) != 0 {
                return None;
            }
            for i in 0..members.len() {
                if let Some(m) = members.get(i) {
                    if m.address == *member && m.position == pos && m.status == MEMBER_ACTIVE {
                        return Some(net);
                    }
                }
            }
            None
        }
        PAYOUT_RANDOM => {
            for i in 0..members.len() {
                if let Some(m) = members.get(i) {
                    if m.address == *member && m.status == MEMBER_ACTIVE {
                        if (circle.payout_bitmap & (1u128 << m.position)) != 0 {
                            return None;
                        }
                        return Some(net);
                    }
                }
            }
            None
        }
        PAYOUT_AUCTION => {
            let bids: Vec<AuctionBid> = match env.storage().persistent().get(&DataKey::Bids) {
                Some(b) => b,
                None => return None,
            };
            let mut min_bps: u32 = 10001;
            for i in 0..bids.len() {
                if let Some(b) = bids.get(i) {
                    if b.round == round && b.discount_bips < min_bps {
                        min_bps = b.discount_bips;
                    }
                }
            }
            if min_bps > 10000 {
                return None;
            }
            let mut winner: Option<Address> = None;
            for i in 0..bids.len() {
                if let Some(b) = bids.get(i) {
                    if b.round == round && b.discount_bips == min_bps {
                        winner = Some(b.bidder.clone());
                        break;
                    }
                }
            }
            match winner {
                Some(w) => {
                    if w == *member {
                        Some(net)
                    } else {
                        None
                    }
                }
                None => None,
            }
        }
        PAYOUT_VOTE => {
            let votes: Vec<VoteEntry> = match env.storage().persistent().get(&DataKey::Votes) {
                Some(v) => v,
                None => return None,
            };
            let mut tally: Map<Address, u32> = Map::new(env);
            for i in 0..votes.len() {
                if let Some(v) = votes.get(i) {
                    if v.round == round {
                        let c = tally.get(v.vote_for.clone()).unwrap_or(0);
                        tally.set(v.vote_for.clone(), c + 1);
                    }
                }
            }
            let mut best_addr: Option<Address> = None;
            let mut best_count: u32 = 0;
            for (addr, count) in tally.iter() {
                if count > best_count {
                    best_count = count;
                    best_addr = Some(addr);
                }
            }
            match best_addr {
                Some(a) => {
                    if a == *member {
                        Some(net)
                    } else {
                        None
                    }
                }
                None => None,
            }
        }
        _ => None,
    }
}
/// Pauses the circle, preventing all state-mutating operations.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address requesting the pause
///
/// # Returns
/// - `Ok(())` on successful pause
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
/// - `Err(CircleError::ContractPaused)` if already paused
///
/// # Authorization
/// Only the stored admin can pause the contract.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn pause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| CircleError::ContractPaused)
}
/// Unpauses the circle, allowing state-mutating operations to resume.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address requesting the unpause
///
/// # Returns
/// - `Ok(())` on successful unpause
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
/// - `Err(CircleError::ContractPaused)` if pause operation fails
///
/// # Authorization
/// Only the stored admin can unpause the contract.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn unpause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| CircleError::ContractPaused)
}
/// Sets the fee percentage in basis points (1 bps = 0.01%).
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address setting the fee
/// - `fee_bps`: Fee in basis points (0-10000, where 10000 = 100%)
///
/// # Returns
/// - `Ok(())` on successful fee update
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
/// - `Err(CircleError::InvalidAmount)` if fee_bps > 10000
///
/// # Authorization
/// Requires authentication from admin and admin must match stored admin.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn batch_invite(
    env: &Env,
    caller: &Address,
    members: &Vec<Address>,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if caller != &circle.organizer {
        return Err(CircleError::NotOrganizer);
    }
    caller.require_auth();
    if circle.status == STATUS_DISPUTED || circle.status == STATUS_COMPLETED {
        return Err(CircleError::NotActive);
    }
    let mut members_vec: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));
    for mi in 0..members.len() {
        let member = members.get(mi).ok_or(CircleError::VecAccessError)?;
        let score = scoring::get_score(env, &member);
        if score < circle.min_moi_score {
            return Err(CircleError::InsufficientMoiScore);
        }
        for i in 0..members_vec.len() {
            if members_vec
                .get(i)
                .ok_or(CircleError::VecAccessError)?
                .address
                == member
            {
                return Err(CircleError::AlreadyMember);
            }
        }
        if members_vec.len() as u32 >= circle.max_members {
            return Err(CircleError::CircleFull);
        }
        let now = env.ledger().timestamp();
        let pos = members_vec.len() as u32;
        members_vec.push_back(Member {
            address: member.clone(),
            position: pos,
            joined_at: now,
            strikes: 0,
            status: MEMBER_ACTIVE,
            exited_at: 0,
            total_contributions: 0,
            total_received: 0,
        });
    }
    env.storage()
        .persistent()
        .set(&DataKey::Members, &members_vec);
    let circle_status = circle.status;
    let member_count = members_vec.len() as u32;
    let max_members = circle.max_members;
    let mut stored_circle = env
        .storage()
        .instance()
        .get::<DataKey, Circle>(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    stored_circle.member_count = member_count;
    if member_count >= max_members && circle_status == STATUS_PENDING {
        stored_circle.status = STATUS_ACTIVE;
        stored_circle.started_at = env.ledger().timestamp();
    }
    env.storage()
        .instance()
        .set(&DataKey::Circle, &stored_circle);
    for mi in 0..members_vec.len() {
        let member = members_vec.get(mi).ok_or(CircleError::VecAccessError)?;
        env.events().publish(
            (env.current_contract_address(), symbol_short!("joined")),
            MemberJoined {
                member: member.address.clone(),
                position: member.position,
            },
        );
    }
    Ok(())
}
pub fn batch_payout(
    env: &Env,
    caller: &Address,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if caller != &circle.organizer && caller != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if recipients.len() == 0 || recipients.len() > 10 || recipients.len() != amounts.len() {
        return Err(CircleError::InvalidAmount);
    }

    // Get fee_bps from storage (#256)
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0);

    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let now = env.ledger().timestamp();
    let mut payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    
    for i in 0..recipients.len() {
        let recipient = recipients.get(i).ok_or(CircleError::VecAccessError)?;
        let amount = amounts.get(i).ok_or(CircleError::VecAccessError)?;
        if amount <= 0 {
            return Err(CircleError::InvalidAmount);
        }

        // Calculate fee (#256)
        let fee = if fee_bps > 0 {
            (amount * (fee_bps as i128)) / 10000
        } else {
            0
        };
        let net_amount = amount - fee;

        // Transfer net amount to recipient
        token_client.transfer(&circle.id, &recipient, &net_amount);

        // Transfer fee to treasury if fee > 0
        if fee > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .ok_or(CircleError::NotInitialized)?;
            token_client.transfer(&circle.id, &treasury, &fee);
        }

        payouts.push_back(PayoutRecipient {
            recipient: recipient.clone(),
            round,
            amount: net_amount,
            fee,
            payout_type: circle.payout_type,
            timestamp: now,
        });
        for j in 0..members.len() {
            let mut member = members.get(j).ok_or(CircleError::VecAccessError)?;
            if member.address == recipient {
                member.total_received = math::safe_add(member.total_received, net_amount)
                    .map_err(|_| CircleError::InvalidAmount)?;
                members.set(j, member);
                break;
            }
        }
        env.events().publish(
            (env.current_contract_address(), symbol_short!("payout")),
            PayoutExecuted {
                recipient,
                round,
                amount,
                fee: 0,
                payout_type: circle.payout_type,
            },
        );
    }
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    Ok(())
}
pub fn register_referral(
    env: &Env,
    referrer: &Address,
    referred: &Address,
    bonus_pct: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    referrer.require_auth();
    if referrer == referred {
        return Err(CircleError::SelfReferral);
    }
    if bonus_pct > 10000 {
        return Err(CircleError::InvalidAmount);
    }
    let mut referrals: Vec<Referral> = env
        .storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..referrals.len() {
        let r = referrals.get(i).ok_or(CircleError::VecAccessError)?;
        if r.referrer == *referrer && r.referred == *referred {
            return Err(CircleError::AlreadyMember);
        }
    }
    referrals.push_back(Referral {
        referrer: referrer.clone(),
        referred: referred.clone(),
        bonus_pct,
        timestamp: env.ledger().timestamp(),
    });
    env.storage()
        .persistent()
        .set(&DataKey::Referrals, &referrals);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("referral")),
        ReferralRegistered {
            referrer: referrer.clone(),
            referred: referred.clone(),
            bonus_pct,
        },
    );
    Ok(())
}
pub fn claim_referral_bonus(
    env: &Env,
    referrer: &Address,
) -> Result<(), CircleError> {
    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(CircleError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &token_address);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance <= 0 {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&env.current_contract_address(), referrer, &contract_balance);
    Ok(())
}
pub fn update_streak(_env: &Env, _member: &Address, _round: u32) -> Result<(), CircleError> {
    Err(CircleError::NotImplemented)
}
pub fn claim_streak_bonus(
    env: &Env,
    member: &Address,
) -> Result<(), CircleError> {
    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(CircleError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &token_address);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance <= 0 {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&env.current_contract_address(), member, &contract_balance);
    Ok(())
}
pub fn get_referrals(env: &Env) -> Vec<Referral> {
    env.storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env))
}
pub fn get_streaks(env: &Env) -> Vec<Streak> {
    Vec::new(env)
}
pub fn get_member_streak(_env: &Env, _member: &Address) -> Streak {
    Streak {
        member: _member.clone(),
        current_streak: 0,
        longest_streak: 0,
        last_round: 0,
    }
}
// Closes #201: set_reputation_registry correctly writes to DataKey::ReputationRegistry
pub fn set_reputation_registry(
    env: &Env,
    admin: &Address,
    registry: &Address,
) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    env.storage()
        .instance()
        .set(&DataKey::ReputationRegistry, registry);
    Ok(())
}
pub fn get_reputation_registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ReputationRegistry)
}
pub fn set_treasury(env: &Env, admin: &Address, treasury: &Address) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    env.storage().instance().set(&DataKey::Treasury, treasury);
    Ok(())
}
/// Updates the token address used for contributions and payouts.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address updating the token
/// - `token`: New token contract address
///
/// # Returns
/// - `Ok(())` on successful token update
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
/// - `Err(CircleError::NotInitialized)` if circle is not initialized
///
/// # Authorization
/// Requires authentication from admin and admin must match stored admin.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    let mut circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    circle.token = token.clone();
    env.storage().instance().set(&DataKey::Circle, &circle);
    Ok(())
}
pub fn set_fee_bps(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    if fee_bps > 10_000 {
        return Err(CircleError::InvalidAmount);
    }
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    Ok(())
}

/// Sets the allowlist of addresses permitted to join the circle.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address setting the allowlist
/// - `allowlist`: Vector of addresses permitted to join
///
/// # Returns
/// - `Ok(())` on successful allowlist update
/// - `Err(CircleError::Unauthorized)` if caller is not the admin
///
/// # Authorization
/// Requires authentication from admin and admin must match stored admin.
///
/// # Notes
/// - Empty allowlist permits anyone to join
/// - Non-empty allowlist restricts joins to listed addresses only
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
// Closes #202: set_allowlist correctly writes to DataKey::Allowlist
pub fn set_allowlist(
    env: &Env,
    admin: &Address,
    allowlist: Vec<Address>,
) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    env.storage()
        .persistent()
        .set(&DataKey::Allowlist, &allowlist);
    Ok(())
}

/// Returns the current allowlist of addresses permitted to join.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// Vector of addresses on the allowlist. Empty vector means all addresses are permitted.
///
/// # Panics
/// Never panics. Returns empty vector if allowlist is not configured.
pub fn get_allowlist(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Allowlist)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_oracle(env: &Env, admin: &Address, oracle: &Address) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    oracle::set_primary_oracle(env, oracle);
    Ok(())
}

pub fn set_fallback_oracle(
    env: &Env,
    admin: &Address,
    oracle: &Address,
) -> Result<(), CircleError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    oracle::set_fallback_oracle(env, oracle);
    Ok(())
}

pub fn get_oracle(env: &Env) -> Option<Address> {
    oracle::get_primary_oracle(env)
}

pub fn get_fallback_oracle(env: &Env) -> Option<Address> {
    oracle::get_fallback_oracle(env)
}
