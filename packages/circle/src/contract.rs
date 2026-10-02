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
    symbol_short,
    xdr::ToXdr,
    Address, BytesN, Env, IntoVal, Map, Symbol, Vec,
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
        || config.contribution_amount < 0
        || config.total_rounds == 0
        || config.payout_type > 3
    {
        return Err(CircleError::InvalidAmount);
    }
    if let Some(registry_address) = get_reputation_registry(env) {
        let registry_client =
            reputation_registry::ReputationRegistryClient::new(env, &registry_address);
        if config.max_members > registry_client.calc_max_size(&config.organizer) {
            return Err(CircleError::CircleSizeExceedsTier);
        }
        if config.contribution_amount > registry_client.calc_max_contrib(&config.organizer) {
            return Err(CircleError::ContributionExceedsTier);
        }
    } else if reputation_registry::storage::get_score(env, &config.organizer) > 0 {
        if config.max_members > scoring::max_circle_size(env, &config.organizer) {
            return Err(CircleError::CircleSizeExceedsTier);
        }
        if config.contribution_amount > scoring::max_contribution(env, &config.organizer) {
            return Err(CircleError::ContributionExceedsTier);
        }
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
        max_withdrawal_per_tx: config.max_withdrawal_per_tx,
        daily_withdrawal_limit: config.daily_withdrawal_limit,
        min_duration_seconds: config.min_duration_seconds,
        health_score: 100,
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
    common::vrf::init_vrf(env, None, admin).map_err(|_| CircleError::InvalidAmount)?;
    env.storage()
        .persistent()
        .set(&DataKey::Payouts, &Vec::<PayoutRecipient>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Bids, &Vec::<AuctionBid>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Votes, &Vec::<VoteEntry>::new(env));
    let initial_hash = compute_circle_config_hash(env, &circle);
    env.storage()
        .persistent()
        .set(&DataKey::RoundConfigSnapshot(0), &initial_hash);
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
// ---------------------------------------------------------------------------
// Contribution window helpers (#325 / #329)
// ---------------------------------------------------------------------------

/// Addresses that recorded a contribution for `round`.
///
/// Derived from the canonical `Contributions` log rather than the
/// `(Address, u32) -> bool` map `contribute` also maintains. Both are correct,
/// but the payout path already has `Contributions` in its footprint, so reading
/// it costs no extra ledger entry — and the payout path is close to the
/// invocation ledger-entry budget on long-running circles.
fn round_contributors(env: &Env, round: u32) -> Vec<Address> {
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut contributors: Vec<Address> = Vec::new(env);
    for i in 0..contributions.len() {
        if let Some(c) = contributions.get(i) {
            if c.round == round {
                contributors.push_back(c.member.clone());
            }
        }
    }
    contributors
}

/// Total tokens actually contributed for `round`.
///
/// `trigger_payout_internal` previously sized the pool as
/// `contribution_amount * member_count`, which silently assumed a fully
/// funded round. That is exactly what breaks #325: once the window shuts and a
/// member defaults, the circle holds less than the pool implies and settlement
/// fails on insufficient balance, leaving the round wedged. For a fully funded
/// round the two values are identical, so this only changes the under-funded
/// case.
fn round_contributed_total(env: &Env, round: u32) -> i128 {
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut total: i128 = 0;
    for i in 0..contributions.len() {
        if let Some(c) = contributions.get(i) {
            if c.round == round {
                total = total.saturating_add(c.amount);
            }
        }
    }
    total
}

fn contributed_in(contributors: &Vec<Address>, member: &Address) -> bool {
    for i in 0..contributors.len() {
        if let Some(a) = contributors.get(i) {
            if a == *member {
                return true;
            }
        }
    }
    false
}

/// Timestamp at which on-time contributions for the current round stop being
/// accepted. Mirrors the basis used inside `contribute` (`started_at` plus
/// `contribution_deadline_seconds`) so enforcement and acceptance can never
/// disagree about when the window shut.
fn round_contribution_deadline(circle: &Circle) -> Option<u64> {
    if circle.contribution_deadline_seconds == 0 {
        // No deadline configured: the window never closes on its own.
        return None;
    }
    circle
        .started_at
        .checked_add(circle.contribution_deadline_seconds)
}

/// Timestamp after which `contribute` starts rejecting contributions outright.
///
/// This is the deadline plus any grace period, i.e. the point at which a
/// member can no longer contribute late either. It is the earliest moment at
/// which a member can be treated as having defaulted for the round.
fn round_forfeit_time(circle: &Circle) -> Option<u64> {
    let deadline = round_contribution_deadline(circle)?;
    match circle.grace_period_seconds {
        0 => Some(deadline),
        grace => deadline.checked_add(grace),
    }
}

/// Whether the round's contribution window has closed for good, i.e. members
/// can no longer contribute (not even late) and non-contributors can be
/// struck. `false` when no deadline is configured.
pub fn contribution_window_closed(env: &Env, circle: &Circle) -> bool {
    match round_forfeit_time(circle) {
        Some(forfeit_at) => env.ledger().timestamp() > forfeit_at,
        None => false,
    }
}

/// Number of active members that still owe a contribution for `round`.
///
/// Only `MEMBER_ACTIVE` members count: exited members are refunded collateral
/// and defaulted members have already been penalised.
fn active_members_awaiting(members: &Vec<Member>, contributors: &Vec<Address>) -> u32 {
    let mut awaiting = 0u32;
    for i in 0..members.len() {
        let m = match members.get(i) {
            Some(m) => m,
            None => continue,
        };
        if m.status == MEMBER_ACTIVE && !contributed_in(contributors, &m.address) {
            awaiting = awaiting.saturating_add(1);
        }
    }
    awaiting
}

/// Sweeps the current round once its contribution window has closed, striking
/// every active member who never contributed and resolving the round.
///
/// #325: `contribute` already refuses late contributions once the window shuts,
/// but nothing advanced the round or penalised non-contributors, so a single
/// absent member could wedge the circle permanently: the admin's
/// `trigger_payout` had no contributor to pay out and the round never moved on.
/// This is permissionless and idempotent per round, and it auto-resolves the
/// round afterwards so the circle cannot stall.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// - `Ok(())` once the round has been swept and resolved
/// - `Err(CircleError::ContractPaused)` if the contract is paused
/// - `Err(CircleError::ReentrantCall)` if re-entered while already executing
/// - `Err(CircleError::NotActive)` if the circle is not ACTIVE
/// - `Err(CircleError::InvalidRound)` if there is no open round left to sweep
/// - `Err(CircleError::DeadlineNotPassed)` if the contribution window is still open
///
/// # Authorization
/// None. Anyone may sweep a round once its window has demonstrably closed.
///
/// # Panics
/// Never panics. All errors are returned as typed CircleError variants.
pub fn check_contribution_deadline(env: &Env) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if circle.current_round >= circle.total_rounds {
        return Err(CircleError::InvalidRound);
    }
    if !contribution_window_closed(env, &circle) {
        return Err(CircleError::DeadlineNotPassed);
    }
    let round = circle.current_round;
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let contributors = round_contributors(env, round);

    // #325: the window has shut, so anyone still missing has defaulted.
    strike_non_contributors(env, &circle, &contributors, &mut members);

    // Settle immediately rather than waiting on the admin, so a circle cannot
    // stall on members who are never going to pay. `trigger_payout_internal`
    // sizes the pool from what was actually contributed, so a partially funded
    // round pays out the smaller real pool instead of over-drawing.
    trigger_payout_internal(env, round)
}

/// Applies #325's automatic strike to every active member who has not
/// contributed for `round`. Called from `trigger_payout_internal` on the way to
/// settling a round whose contribution window has already closed, so it runs at
/// most once per round and only after contributions are genuinely impossible.
fn strike_non_contributors(
    env: &Env,
    circle: &Circle,
    contributors: &Vec<Address>,
    members: &mut Vec<Member>,
) {
    let mut changed = false;
    for i in 0..members.len() {
        let m = match members.get(i) {
            Some(m) => m,
            None => continue,
        };
        if m.status != MEMBER_ACTIVE || contributed_in(contributors, &m.address) {
            continue;
        }
        changed = true;
        let mut updated = m;
        // Saturating: a long-lived circle must not wrap a strike counter back
        // to zero and silently erase a member's penalty history.
        updated.strikes = updated.strikes.saturating_add(1);
        if updated.strikes >= circle.max_strikes {
            updated.status = MEMBER_DEFAULTED;
            scoring::record_default(env, &updated.address);
        }
        env.events().publish(
            (env.current_contract_address(), symbol_short!("default")),
            MemberDefaulted {
                member: updated.address.clone(),
                strikes: updated.strikes,
            },
        );
        members.set(i, updated);
    }
    if changed {
        env.storage().persistent().set(&DataKey::Members, &*members);
    }
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
    let mut circle: Circle = env
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
    if is_payout_scheduled(env, round) {
        return Err(CircleError::PayoutAlreadyScheduled);
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
            env.storage()
                .persistent()
                .set(&symbol_short!("contribs"), &m);
            m
        });
    if contribution_map
        .get((member.clone(), round))
        .unwrap_or(false)
    {
        return Err(CircleError::AlreadyContributed);
    }
    let now = env.ledger().timestamp();
    let on_time = if circle.contribution_deadline_seconds > 0 {
        let deadline = circle
            .started_at
            .checked_add(circle.contribution_deadline_seconds)
            .ok_or(CircleError::InvalidAmount)?;
        if now <= deadline {
            true
        } else if circle.grace_period_seconds > 0 {
            let grace_deadline = deadline
                .checked_add(circle.grace_period_seconds)
                .ok_or(CircleError::InvalidAmount)?;
            if now <= grace_deadline {
                false
            } else {
                return Err(CircleError::PaymentDeadlinePassed);
            }
        } else {
            return Err(CircleError::PaymentDeadlinePassed);
        }
    } else {
        true
    };

    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    token_client.transfer(member, &circle.id, &amount);

    if !on_time && circle.penalty_bps > 0 {
        let penalty = math::calculate_penalty(amount, circle.penalty_bps as i128)
            .map_err(|_| CircleError::InvalidAmount)?;
        if penalty > 0 {
            if let Some(treasury) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::Treasury)
            {
                token_client.transfer(&circle.id, &treasury, &penalty);
            }
            circle.total_fees = math::safe_add(circle.total_fees, penalty)
                .map_err(|_| CircleError::InvalidAmount)?;
            record_round_fee(env, round, 0, penalty)?;
            env.storage().instance().set(&DataKey::Circle, &circle);
            env.events().publish(
                (env.current_contract_address(), symbol_short!("late_pen")),
                LatePenaltyApplied {
                    member: member.clone(),
                    round,
                    penalty,
                },
            );
        }
    }

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
    if on_time {
        scoring::record_on_time_payment(env, member, &circle.id, amount, round);
        let _ = update_streak_internal(env, member, round);
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
    // Issue #219: `circle_id` intentionally appears twice in the call below
    // — `treasury::deposit` requires `from == circle_id` (see
    // packages/treasury/src/contract.rs), so this circle contract pays the
    // fee from its own balance and is tracked under its own id. That part
    // was already correct; the actual bug was the authorization shape.
    //
    // The real call graph is circle -> treasury.deposit_fee -> token.transfer
    // (treasury's `deposit` internally calls `from.require_auth()`, i.e.
    // `circle_id.require_auth()`, then transfers the token itself). The
    // previous auth entry only pre-authorized a *direct* circle -> token
    // transfer with no sub-invocation, which doesn't match that graph, so
    // Soroban's auth-entry matching could reject it at runtime. The entry
    // below authorizes the actual top-level call (circle_id authorizing
    // `treasury.deposit_fee`), with the token transfer nested as the
    // sub-invocation treasury performs on circle_id's behalf.
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: treasury.clone(),
                fn_name: Symbol::new(env, "deposit_fee"),
                args: soroban_sdk::vec![
                    env,
                    circle_id.into_val(env),
                    amount.into_val(env),
                    circle_id.into_val(env),
                ],
            },
            sub_invocations: soroban_sdk::vec![
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
            ],
        }),
    ]);
    treasury::TreasuryClient::new(env, treasury).deposit_fee(circle_id, &amount, circle_id);
}

fn empty_round_fee_ledger(round: u32) -> RoundFeeLedger {
    RoundFeeLedger {
        round,
        payout_fee: 0,
        late_penalty_fee: 0,
        total_fee: 0,
    }
}

fn record_round_fee(
    env: &Env,
    round: u32,
    payout_fee_delta: i128,
    late_penalty_fee_delta: i128,
) -> Result<(), CircleError> {
    let mut ledger: RoundFeeLedger = env
        .storage()
        .persistent()
        .get(&DataKey::RoundFeeLedger(round))
        .unwrap_or_else(|| empty_round_fee_ledger(round));
    ledger.payout_fee = math::safe_add(ledger.payout_fee, payout_fee_delta)
        .map_err(|_| CircleError::InvalidAmount)?;
    ledger.late_penalty_fee = math::safe_add(ledger.late_penalty_fee, late_penalty_fee_delta)
        .map_err(|_| CircleError::InvalidAmount)?;
    ledger.total_fee = math::safe_add(ledger.payout_fee, ledger.late_penalty_fee)
        .map_err(|_| CircleError::InvalidAmount)?;
    env.storage()
        .persistent()
        .set(&DataKey::RoundFeeLedger(round), &ledger);
    Ok(())
}

pub fn trigger_payout(env: &Env, caller: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
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
    trigger_payout_internal(env, round)
}

/// Resolves `round` and advances `current_round`. Split out of
/// `trigger_payout` so #325's deadline sweep can settle a round without
/// impersonating the admin. Callers are responsible for having already
/// authenticated and for holding the reentrancy guard.
fn trigger_payout_internal(env: &Env, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let mut circle: Circle = env
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
    if circle.min_duration_seconds > 0 {
        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(circle.started_at);
        if elapsed < circle.min_duration_seconds {
            return Err(CircleError::CircleDurationTooShort);
        }
    }
    let (recipient, payout_type) = match circle.payout_type {
        PAYOUT_RANDOM => (payout::resolve_random(env, &circle, round)?, PAYOUT_RANDOM),
        PAYOUT_FIXED => (payout::resolve_fixed(env, &circle, round)?, PAYOUT_FIXED),
        PAYOUT_AUCTION => {
            let (w, _) = payout::resolve_auction(env, &circle, round)?;
            // #436: record the winner so losing deposits can be refunded in
            // capped continuation calls instead of inside this payout.
            env.storage()
                .persistent()
                .set(&DataKey::AuctionWinner(round), &w);
            (w, PAYOUT_AUCTION)
        }
        PAYOUT_VOTE => (payout::resolve_vote(env, &circle, round)?, PAYOUT_VOTE),
        _ => return Err(CircleError::InvalidPayoutType),
    };
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;

    // Read once and shared with the deadline sweep's own accounting. This entry
    // is already in the payout path's footprint, so it costs no extra ledger
    // entry.
    let contributors = round_contributors(env, round);

    if !contribution_window_closed(env, &circle)
        && active_members_awaiting(&members, &contributors) > 0
    {
        // #329: never advance past members who still owe a contribution while
        // their window is open. Without this a member could be skipped silently
        // and the round would move on before they were ever able to pay.
        //
        // Placed after recipient resolution but before any transfer, so it still
        // blocks every state change and token movement while leaving the more
        // specific recipient errors (e.g. an auction winner that has since
        // exited) reachable.
        return Err(CircleError::InvalidContributionRound);
    }
    let pool = round_contributed_total(env, round);
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0u32);
    let (net, fee) =
        math::apply_fee(pool, fee_bps as i128).map_err(|_| CircleError::InvalidAmount)?;
    if net <= 0 {
        // A round whose contribution pool nets to zero after fees (e.g. all
        // fees consumed it, or contribution_amount is configured as zero)
        // used to hard-error here with no state change — since `round ==
        // circle.current_round` is required to even reach this function,
        // that left the round permanently stuck: every retry hit the exact
        // same ZeroPayoutAmount error forever, with current_round never
        // advancing. Short-circuit instead: settle the round (mark the
        // resolved recipient's position paid, advance current_round,
        // complete the circle if this was the last round) with a
        // zero-amount payout record for the audit trail, rather than
        // leaving the circle wedged.
        let now = env.ledger().timestamp();
        let mut members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .ok_or(CircleError::NotInitialized)?;
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::VecAccessError)?;
            if m.address == recipient {
                circle.payout_bitmap |= 1u128 << m.position;
                break;
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
            amount: 0,
            fee: 0,
            payout_type,
            timestamp: now,
        });
        env.storage().persistent().set(&DataKey::Payouts, &payouts);
        env.storage().persistent().set(&DataKey::Members, &members);
        circle.current_round = circle
            .current_round
            .checked_add(1)
            .ok_or(CircleError::InvalidAmount)?;
        if circle.current_round >= circle.total_rounds {
            circle.status = STATUS_COMPLETED;
        } else {
            let next_round_hash = compute_circle_config_hash(env, &circle);
            env.storage()
                .persistent()
                .set(&DataKey::RoundConfigSnapshot(circle.current_round), &next_round_hash);
        }
        // Issue #369: this branch still advances a round, so health is
        // still recomputed even though no funds moved.
        let contributions: Vec<Contribution> = env
            .storage()
            .persistent()
            .get(&DataKey::Contributions)
            .unwrap_or_else(|| Vec::new(env));
        let health = calculate_health_score(env, &members, &contributions);
        circle.health_score = health.score;
        env.storage().instance().set(&DataKey::Circle, &circle);
        env.events().publish(
            (env.current_contract_address(), symbol_short!("health")),
            CircleHealthUpdated {
                health_score: health.score,
                on_time_rate_bps: health.on_time_rate_bps,
                completion_rate_bps: health.completion_rate_bps,
                member_retention_bps: health.member_retention_bps,
                round,
            },
        );
        env.events().publish(
            (env.current_contract_address(), symbol_short!("payout")),
            PayoutExecuted {
                recipient,
                round,
                amount: 0,
                fee: 0,
                payout_type,
            },
        );
        return Ok(());
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
    // Issue #216: mark the recipient's position as paid in payout_bitmap.
    // resolve_random/resolve_fixed/resolve_auction/resolve_vote all check
    // this bitmap to avoid re-selecting an already-paid position, but
    // nothing ever set it, so those checks were always no-ops and the same
    // position could be paid out every round.
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == recipient {
            circle.payout_bitmap |= 1u128 << m.position;
            break;
        }
    }
    let mut distributed: i128 = 0;
    let net_u = net as u128;
    // Issue #218: the weighted distribution below spreads the pool across
    // every contributing member by time-weighted share — that's the
    // intended payout mechanic for PAYOUT_RANDOM only. For fixed/auction/
    // vote, the resolved `recipient` above is who the round's payout goes
    // to; running this loop for those types paid out to arbitrary members
    // instead of (or as well as) the resolved recipient, over-distributing
    // relative to `net`. The "dust" step below already pays whatever
    // `distributed` didn't cover to `recipient` — with this loop skipped
    // for non-random types, `distributed` stays 0 and dust covers the
    // full `net`, i.e. the resolved recipient gets the whole payout.
    if payout_type == PAYOUT_RANDOM {
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
    }
    if fee > 0 {
        if let Some(treasury) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Treasury)
        {
            deposit_protocol_fee(env, &circle.token, &treasury, &circle.id, fee);
        }
        record_round_fee(env, round, fee, 0)?;
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
    } else {
        let next_round_hash = compute_circle_config_hash(env, &circle);
        env.storage()
            .persistent()
            .set(&DataKey::RoundConfigSnapshot(circle.current_round), &next_round_hash);
    }
    // Issue #369: recompute the health score after every round, from the
    // just-updated Members list and full contribution history.
    let health = calculate_health_score(env, &members, &all_contributions);
    circle.health_score = health.score;
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("health")),
        CircleHealthUpdated {
            health_score: health.score,
            on_time_rate_bps: health.on_time_rate_bps,
            completion_rate_bps: health.completion_rate_bps,
            member_retention_bps: health.member_retention_bps,
            round,
        },
    );
    env.events().publish(
        (env.current_contract_address(), symbol_short!("payout")),
        PayoutExecuted {
            recipient,
            round,
            amount: distributed,
            fee,
            payout_type,
        },
    );
    if circle.status == STATUS_COMPLETED {
        env.events().publish(
            (env.current_contract_address(), symbol_short!("complete")),
            CircleCompleted {
                total_payouts: circle.total_payouts,
            },
        );
        if circle.collateral_amount > 0 {
            for i in 0..members.len() {
                let m = members.get(i).ok_or(CircleError::NotInitialized)?;
                if m.status == MEMBER_ACTIVE {
                    token_client.transfer(&circle.id, &m.address, &circle.collateral_amount);
                }
            }
        }
        let final_members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .ok_or(CircleError::NotInitialized)?;
        for i in 0..final_members.len() {
            let m = final_members.get(i).ok_or(CircleError::NotInitialized)?;
            if m.status == MEMBER_ACTIVE {
                scoring::record_circle_completion(env, &m.address);
            }
        }
    }
    Ok(())
}
/// Allows the organizer or admin to withdraw accumulated treasury funds.
///
/// Enforces a per-transaction cap and a rolling daily limit to prevent
/// draining the treasury under a compromised organizer key.  Amounts above
/// the single-transaction cap require a member vote via the existing
/// `vote_payout` flow.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `caller`: Address requesting the withdrawal (must be organizer or admin)
/// - `amount`: Amount to withdraw from the treasury
///
/// # Returns
/// - `Ok(())` on successful withdrawal
/// - `Err(CircleError::Unauthorized)` if caller is neither organizer nor admin
/// - `Err(CircleError::InvalidAmount)` if amount is not positive
/// - `Err(CircleError::WithdrawalCapExceeded)` if amount exceeds max_withdrawal_per_tx
/// - `Err(CircleError::DailyWithdrawalLimitExceeded)` if daily limit is exceeded
///
/// # Authorization
/// Requires authentication from the `caller` address.
///
/// # Notes
/// - Daily window resets at midnight UTC (based on ledger timestamp).
/// - The treasury balance is checked before transfer to avoid host errors.
pub fn withdraw_treasury(
    env: &Env,
    caller: &Address,
    amount: i128,
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
    if amount <= 0 {
        return Err(CircleError::InvalidAmount);
    }
    if circle.max_withdrawal_per_tx > 0 && amount > circle.max_withdrawal_per_tx {
        return Err(CircleError::WithdrawalCapExceeded);
    }
    let now = env.ledger().timestamp();
    let day_key = (now / 86400) as u64;
    let mut withdrawal_day: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WithdrawalDay)
        .unwrap_or(0);
    let mut daily_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::WithdrawalAmount)
        .unwrap_or(0);
    if withdrawal_day != day_key {
        withdrawal_day = day_key;
        daily_amount = 0;
    }
    if circle.daily_withdrawal_limit > 0
        && daily_amount.saturating_add(amount) > circle.daily_withdrawal_limit
    {
        return Err(CircleError::DailyWithdrawalLimitExceeded);
    }
    let treasury: Address = env
        .storage()
        .instance()
        .get(&DataKey::Treasury)
        .ok_or(CircleError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let treasury_balance = token_client.balance(&circle.id);
    if treasury_balance < amount {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&circle.id, caller, &amount);
    daily_amount = math::safe_add(daily_amount, amount).map_err(|_| CircleError::InvalidAmount)?;
    env.storage().instance().set(&DataKey::WithdrawalDay, &withdrawal_day);
    env.storage()
        .instance()
        .set(&DataKey::WithdrawalAmount, &daily_amount);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("withdraw")),
        soroban_sdk::Val::from(amount),
    );
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
    // A round configured for Dutch mode (init_dutch_auction) clears on the
    // first dutch_auction_bid() call at whatever price has decayed to — it
    // does not accept ordinary English (highest-discount) bids, since the
    // two clearing mechanisms are mutually exclusive per round.
    if env
        .storage()
        .persistent()
        .get::<DataKey, DutchAuctionConfig>(&DataKey::DutchAuction(round))
        .is_some()
    {
        return Err(CircleError::InvalidPayoutType);
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
    let deposit = circle.contribution_amount;
    if deposit > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        token_client.transfer(bidder, &circle.id, &deposit);
    }
    bids.push_back(AuctionBid {
        bidder: bidder.clone(),
        discount_bips,
        round,
        timestamp: env.ledger().timestamp(),
        deposit,
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

/// Hard cap on losing-bid refunds performed in one invocation (#436).
/// Callers pass a smaller `limit`; anything above this is clamped so a
/// single transaction cannot walk an unbounded bidder list.
pub const LOSER_REFUND_BATCH_CAP: u32 = 20;

/// Refunds up to `limit` losing bidders for a resolved auction round.
///
/// Returns how many losing bids are still unpaid. When the return value is
/// greater than zero, call again (a continuation) until it returns zero.
/// The winner's deposit stays in the circle; only non-winners for `round`
/// are paid back, each at most once.
pub fn refund_losing_bids(
    env: &Env,
    caller: &Address,
    round: u32,
    limit: u32,
) -> Result<u32, CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    caller.require_auth();
    if limit == 0 {
        return Err(CircleError::InvalidAmount);
    }
    let winner: Address = env
        .storage()
        .persistent()
        .get(&DataKey::AuctionWinner(round))
        .ok_or(CircleError::AuctionNotResolved)?;
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    let cap = if limit > LOSER_REFUND_BATCH_CAP {
        LOSER_REFUND_BATCH_CAP
    } else {
        limit
    };
    let bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let mut kept: Vec<AuctionBid> = Vec::new(env);
    let mut refunded: u32 = 0;
    let mut remaining: u32 = 0;
    for i in 0..bids.len() {
        let bid = bids.get(i).ok_or(CircleError::VecAccessError)?;
        let is_loser = bid.round == round && bid.bidder != winner;
        if is_loser && refunded < cap {
            if bid.deposit > 0 {
                token_client.transfer(&circle.id, &bid.bidder, &bid.deposit);
            }
            env.events().publish(
                (env.current_contract_address(), symbol_short!("refund")),
                AuctionLoserRefunded {
                    bidder: bid.bidder.clone(),
                    round,
                    amount: bid.deposit,
                },
            );
            refunded = refunded.checked_add(1).ok_or(CircleError::InvalidAmount)?;
        } else {
            if is_loser {
                remaining = remaining.checked_add(1).ok_or(CircleError::InvalidAmount)?;
            }
            kept.push_back(bid);
        }
    }
    env.storage().persistent().set(&DataKey::Bids, &kept);
    Ok(remaining)
}

/// Configures round `round`'s auction to clear via Dutch (decaying-price)
/// mode instead of the default English (highest-bid) mode. Organizer-only.
/// The price (`discount_bips`) starts at `start_bips` and decays linearly by
/// `decay_bips_per_ledger` per elapsed ledger down to a floor of
/// `floor_bips`; the first `dutch_auction_bid` call clears at whichever
/// price has decayed to by then. If no bid lands within `expiry_ledgers`
/// ledgers of this call, the auction expires unclaimed (see
/// `dutch_auction_bid`'s `DutchAuctionExpired`).
pub fn init_dutch_auction(
    env: &Env,
    caller: &Address,
    round: u32,
    start_bips: u32,
    floor_bips: u32,
    decay_bips_per_ledger: u32,
    expiry_ledgers: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    caller.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if *caller != circle.organizer {
        return Err(CircleError::NotOrganizer);
    }
    if circle.payout_type != PAYOUT_AUCTION {
        return Err(CircleError::InvalidPayoutType);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if start_bips > 10000
        || floor_bips > start_bips
        || decay_bips_per_ledger == 0
        || expiry_ledgers == 0
    {
        return Err(CircleError::InvalidDutchConfig);
    }
    let bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..bids.len() {
        let b = bids.get(i).ok_or(CircleError::VecAccessError)?;
        if b.round == round {
            return Err(CircleError::AlreadyBidded);
        }
    }
    env.storage().persistent().set(
        &DataKey::DutchAuction(round),
        &DutchAuctionConfig {
            round,
            start_bips,
            floor_bips,
            decay_bips_per_ledger,
            start_ledger: env.ledger().sequence(),
            expiry_ledgers,
            resolved: false,
        },
    );
    Ok(())
}

/// Returns the current decayed clearing price (in bips) for round `round`'s
/// Dutch auction, and whether it has expired unclaimed.
pub fn dutch_auction_price(env: &Env, round: u32) -> Result<(u32, bool), CircleError> {
    let config: DutchAuctionConfig = env
        .storage()
        .persistent()
        .get(&DataKey::DutchAuction(round))
        .ok_or(CircleError::DutchAuctionNotConfigured)?;
    let elapsed = env.ledger().sequence().saturating_sub(config.start_ledger);
    let decayed = config
        .start_bips
        .saturating_sub(config.decay_bips_per_ledger.saturating_mul(elapsed));
    let price = decayed.max(config.floor_bips);
    let expired = !config.resolved && elapsed >= config.expiry_ledgers;
    Ok((price, expired))
}

/// Places the clearing bid for round `round`'s Dutch auction at whatever
/// price has decayed to by the current ledger. The first caller wins — there
/// is no higher-bid competition in Dutch mode, since the decaying price
/// itself is the mechanism that finds a clearing price.
///
/// Returns the `discount_bips` the auction cleared at.
pub fn dutch_auction_bid(env: &Env, bidder: &Address, round: u32) -> Result<u32, CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    bidder.require_auth();
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let mut config: DutchAuctionConfig = env
        .storage()
        .persistent()
        .get(&DataKey::DutchAuction(round))
        .ok_or(CircleError::DutchAuctionNotConfigured)?;
    if config.resolved {
        return Err(CircleError::AuctionAlreadyResolved);
    }
    let elapsed = env.ledger().sequence().saturating_sub(config.start_ledger);
    if elapsed >= config.expiry_ledgers {
        return Err(CircleError::DutchAuctionExpired);
    }
    let decayed = config
        .start_bips
        .saturating_sub(config.decay_bips_per_ledger.saturating_mul(elapsed));
    let clearing_bips = decayed.max(config.floor_bips);

    let mut bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    bids.push_back(AuctionBid {
        bidder: bidder.clone(),
        discount_bips: clearing_bips,
        round,
        timestamp: env.ledger().timestamp(),
        deposit: 0,
    });
    env.storage().persistent().set(&DataKey::Bids, &bids);

    config.resolved = true;
    env.storage()
        .persistent()
        .set(&DataKey::DutchAuction(round), &config);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("bid")),
        AuctionBidPlaced {
            bidder: bidder.clone(),
            discount_bips: clearing_bips,
            round,
        },
    );
    Ok(clearing_bips)
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
    if circle.status == STATUS_COMPLETED || circle.status == STATUS_CANCELLED {
        return Err(CircleError::NotActive);
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

pub fn dispute(env: &Env, member: &Address, evidence_hash: &BytesN<32>) -> Result<(), CircleError> {
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
    if resolution > RESOLVE_REFUND {
        return Err(CircleError::InvalidAmount);
    }
    let outcome_code = resolution;
    match resolution {
        RESOLVE_DISMISS | RESOLVE_FORCE_PAYOUT => {
            circle.status = STATUS_ACTIVE;
        }
        RESOLVE_PENALIZE => {
            circle.status = STATUS_ACTIVE;
            if circle.collateral_amount > 0 {
                let token_address = circle.token.clone();
                let token_client = soroban_sdk::token::Client::new(env, &token_address);
                if let Some(treasury) = env
                    .storage()
                    .instance()
                    .get::<DataKey, Address>(&DataKey::Treasury)
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &treasury,
                        &circle.collateral_amount,
                    );
                }
            }
            let mut members: Vec<Member> = env
                .storage()
                .persistent()
                .get(&DataKey::Members)
                .unwrap_or_else(|| Vec::new(env));
            for i in 0..members.len() {
                if let Some(mut m) = members.get(i) {
                    if m.address == dispute.raised_by {
                        m.strikes = m.strikes.saturating_add(1);
                        m.status = MEMBER_DEFAULTED;
                        members.set(i, m);
                        break;
                    }
                }
            }
            env.storage().persistent().set(&DataKey::Members, &members);
        }
        RESOLVE_REFUND => {
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
    let record = DisputeResolutionRecord {
        raised_by: dispute.raised_by.clone(),
        resolution,
        outcome_code,
        resolved_by: admin.clone(),
        resolved_at: dispute.resolved_at,
    };
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Dispute, &dispute);
    env.storage()
        .persistent()
        .set(&DataKey::DisputeResolution, &record);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("disp_res")),
        DisputeResolved {
            member: dispute.raised_by,
            resolution,
            outcome_code,
            resolved_by: admin.clone(),
        },
    );
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
            health_score: 100,
        })
}

pub fn get_dispute_resolution(env: &Env) -> Option<DisputeResolutionRecord> {
    env.storage().persistent().get(&DataKey::DisputeResolution)
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
    if recipients.len() == 0 || recipients.len() > 10 || recipients.len() != amounts.len() {
        return Err(CircleError::InvalidAmount);
    }

    // Get fee_bps from storage (#256)
    let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);

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

        // Issue #216: reject (rather than silently re-pay) a recipient whose
        // position has already been marked paid in payout_bitmap, and mark
        // it paid before moving on to the next recipient. Without this,
        // resolve_random/resolve_fixed/resolve_auction/resolve_vote's own
        // bitmap checks were meaningless — nothing in batch_payout ever set
        // the bit, so the same position could be paid out repeatedly.
        let mut recipient_position: Option<u32> = None;
        for j in 0..members.len() {
            let m = members.get(j).ok_or(CircleError::VecAccessError)?;
            if m.address == recipient {
                recipient_position = Some(m.position);
                break;
            }
        }
        if let Some(pos) = recipient_position {
            if (circle.payout_bitmap & (1u128 << pos)) != 0 {
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            circle.payout_bitmap |= 1u128 << pos;
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
                // Was hardcoded to 0 even though `fee` was already computed
                // above — under-reported every fee-bearing batch payout to
                // event consumers/indexers even though the correct amount
                // was (and still is) recorded in `PayoutRecipient` and
                // actually transferred to the treasury.
                fee,
                payout_type: circle.payout_type,
            },
        );
    }
    env.storage().instance().set(&DataKey::Circle, &circle);
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
pub fn claim_referral_bonus(env: &Env, referrer: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    // #323: this path performs a token `transfer`, so it takes the same guard
    // as every other mutating entry point.
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    // The payout target must authorise the transfer. Without this any caller
    // could name an arbitrary `referrer` and sweep the circle's whole balance.
    referrer.require_auth();
    let token_address: Address = if let Some(t) = env.storage().instance().get(&DataKey::Token) {
        t
    } else {
        let circle: Circle = env
            .storage()
            .instance()
            .get(&DataKey::Circle)
            .ok_or(CircleError::NotInitialized)?;
        circle.token
    };
    let token_client = soroban_sdk::token::Client::new(env, &token_address);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance <= 0 {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&env.current_contract_address(), referrer, &contract_balance);
    Ok(())
}
pub fn update_streak_internal(env: &Env, member: &Address, round: u32) -> Result<(), CircleError> {
    let mut streaks: Vec<Streak> = env
        .storage()
        .persistent()
        .get(&DataKey::Streaks)
        .unwrap_or_else(|| Vec::new(env));

    let mut found = false;
    for i in 0..streaks.len() {
        let mut s = streaks.get(i).ok_or(CircleError::VecAccessError)?;
        if s.member == *member {
            found = true;
            if s.current_streak == 0 {
                s.current_streak = 1;
                if s.longest_streak == 0 {
                    s.longest_streak = 1;
                }
                s.last_round = round;
            } else if round == s.last_round + 1 {
                s.current_streak += 1;
                if s.current_streak > s.longest_streak {
                    s.longest_streak = s.current_streak;
                }
                s.last_round = round;
            } else if round > s.last_round + 1 {
                s.current_streak = 1;
                s.last_round = round;
            }
            streaks.set(i, s);
            break;
        }
    }
    if !found {
        streaks.push_back(Streak {
            member: member.clone(),
            current_streak: 1,
            longest_streak: 1,
            last_round: round,
        });
    }
    env.storage().persistent().set(&DataKey::Streaks, &streaks);
    Ok(())
}

pub fn update_streak(env: &Env, member: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    member.require_auth();
    update_streak_internal(env, member, round)
}

pub fn claim_streak_bonus(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    // #323: this path performs a token `transfer`, so it takes the same guard
    // as every other mutating entry point.
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    member.require_auth();
    let token_address: Address = if let Some(t) = env.storage().instance().get(&DataKey::Token) {
        t
    } else {
        let circle: Circle = env
            .storage()
            .instance()
            .get(&DataKey::Circle)
            .ok_or(CircleError::NotInitialized)?;
        circle.token
    };
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
    env.storage()
        .persistent()
        .get(&DataKey::Streaks)
        .unwrap_or_else(|| Vec::new(env))
}
pub fn get_member_streak(env: &Env, member: &Address) -> Streak {
    let streaks: Vec<Streak> = env
        .storage()
        .persistent()
        .get(&DataKey::Streaks)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..streaks.len() {
        if let Some(s) = streaks.get(i) {
            if s.member == *member {
                return s;
            }
        }
    }
    Streak {
        member: member.clone(),
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

/// Issue #369: circle health score based on contribution consistency.
pub struct HealthScoreResult {
    /// 0-100.
    pub score: u32,
    pub on_time_rate_bps: u32,
    pub completion_rate_bps: u32,
    pub member_retention_bps: u32,
}

/// Computes a 0-100 health score from three signals, each expressed as
/// basis points (0-10000) of their respective denominator:
///
/// - `on_time_rate`: contributions made on time / all contributions ever
///   recorded for this circle. Reflects payment discipline.
/// - `completion_rate`: members who have *not* been marked `MEMBER_DEFAULTED`
///   / total members. Reflects how many members are meeting their
///   obligations well enough to avoid the strike-based default path.
/// - `member_retention`: members who have *not* voluntarily exited
///   (`MEMBER_EXITED`) / total members. Distinct from completion_rate: an
///   exit isn't a default, but churn is still a health signal on its own.
///
/// The three are weighted 40/35/25 (on-time payment behavior is the
/// strongest predictor of an at-risk circle; retention matters but a
/// planned, orderly exit is less concerning than an active default) and
/// averaged into a single 0-100 score. A circle with no members and no
/// contributions yet (e.g. health checked before it's even active) scores
/// 100 — there's no evidence of a problem yet, matching the optimistic
/// `health_score: 100` default set at circle creation.
pub fn calculate_health_score(
    _env: &Env,
    members: &Vec<Member>,
    contributions: &Vec<Contribution>,
) -> HealthScoreResult {
    const BPS_SCALE: u32 = 10_000;

    let mut on_time_count: u32 = 0;
    let mut total_contributions: u32 = 0;
    for i in 0..contributions.len() {
        if let Some(c) = contributions.get(i) {
            total_contributions += 1;
            if c.on_time {
                on_time_count += 1;
            }
        }
    }
    let on_time_rate_bps = if total_contributions == 0 {
        BPS_SCALE
    } else {
        on_time_count.saturating_mul(BPS_SCALE) / total_contributions
    };

    let mut member_count: u32 = 0;
    let mut defaulted_count: u32 = 0;
    let mut exited_count: u32 = 0;
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            member_count += 1;
            if m.status == MEMBER_DEFAULTED {
                defaulted_count += 1;
            } else if m.status == MEMBER_EXITED {
                exited_count += 1;
            }
        }
    }
    let (completion_rate_bps, member_retention_bps) = if member_count == 0 {
        (BPS_SCALE, BPS_SCALE)
    } else {
        (
            (member_count - defaulted_count).saturating_mul(BPS_SCALE) / member_count,
            (member_count - exited_count).saturating_mul(BPS_SCALE) / member_count,
        )
    };

    let weighted = on_time_rate_bps as u64 * 40
        + completion_rate_bps as u64 * 35
        + member_retention_bps as u64 * 25;
    let score = (weighted / (BPS_SCALE as u64 * 100)) as u32;

    HealthScoreResult {
        score: score.min(100),
        on_time_rate_bps,
        completion_rate_bps,
        member_retention_bps,
    }
}

pub fn compute_circle_config_hash(env: &Env, circle: &Circle) -> BytesN<32> {
    let config = CircleConfig {
        organizer: circle.organizer.clone(),
        token: circle.token.clone(),
        name: circle.name.clone(),
        contribution_amount: circle.contribution_amount,
        max_members: circle.max_members,
        payout_type: circle.payout_type,
        total_rounds: circle.total_rounds,
        contribution_deadline_seconds: circle.contribution_deadline_seconds,
        min_moi_score: circle.min_moi_score,
        collateral_amount: circle.collateral_amount,
        penalty_bps: circle.penalty_bps,
        grace_period_seconds: circle.grace_period_seconds,
        max_strikes: circle.max_strikes,
        slug: circle.slug.clone(),
    };
    let xdr_bytes = config.to_xdr(env);
    env.crypto().sha256(&xdr_bytes).into()
}

pub fn query_round_config(env: &Env, round: u32) -> Result<BytesN<32>, CircleError> {
    env.storage()
        .persistent()
        .get(&DataKey::RoundConfigSnapshot(round))
        .ok_or(CircleError::InvalidRound)
}

pub fn get_round_fee_ledger(env: &Env, round: u32) -> RoundFeeLedger {
    env.storage()
        .persistent()
        .get(&DataKey::RoundFeeLedger(round))
        .unwrap_or_else(|| empty_round_fee_ledger(round))
}

pub fn schedule_payout(env: &Env, caller: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    caller.require_auth();
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
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    env.storage()
        .persistent()
        .set(&DataKey::PayoutScheduled(round), &true);
    Ok(())
}

pub fn is_payout_scheduled(env: &Env, round: u32) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::PayoutScheduled(round))
        .unwrap_or(false)
}
