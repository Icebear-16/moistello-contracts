//! Governance contract — all public handler functions follow the **uniform
//! Soroban-native `&Env`-based API pattern**: every function takes `env: &Env`
//! as its first parameter and returns `Result<_, GovernanceError>` (or a plain
//! value for pure reads).  There are no `ExecCtx`, `QueryCtx`, or
//! `MessageInfo` parameters; those belong to CosmWasm and must never appear
//! here.  All mutation functions perform access-control checks first, before
//! touching storage.
use crate::types::*;
use common::{math, pause};
use soroban_sdk::{Address, BytesN, Env, Val, Vec};

const BPS_DENOM: i128 = 10_000;

/// Fixed delay a queued `GovernanceConfig` update must wait before it can be
/// executed. Deliberately a hardcoded constant rather than sourced from
/// `GovernanceConfig` itself: if it were configurable, an admin could queue
/// a config change that zeroes it out and instantly self-approve any future
/// change, defeating the whole point of a parameter-change timelock.
const CONFIG_TIMELOCK_SECONDS: u64 = 172_800; // 48 hours

fn validate_config(config: &GovernanceConfig) -> Result<(), GovernanceError> {
    if config.min_proposal_deposit < 0 || config.proposal_deposit < config.min_proposal_deposit {
        return Err(GovernanceError::InvalidConfig);
    }
    if config.pass_threshold_bps == 0 || config.pass_threshold_bps > BPS_DENOM as u32 {
        return Err(GovernanceError::InvalidConfig);
    }
    Ok(())
}

fn add_proposal_to_status_index(env: &Env, status: &ProposalStatus, proposal_id: u64) {
    let mut list: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status.clone()))
        .unwrap_or_else(|| Vec::new(env));
    list.push_back(proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::ProposalsByStatus(status.clone()), &list);
}

fn remove_proposal_from_status_index(env: &Env, status: &ProposalStatus, proposal_id: u64) {
    let list: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status.clone()))
        .unwrap_or_else(|| Vec::new(env));
    let mut new_list = Vec::new(env);
    for i in 0..list.len() {
        if let Some(id) = list.get(i) {
            if id != proposal_id {
                new_list.push_back(id);
            }
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::ProposalsByStatus(status.clone()), &new_list);
}

pub fn init(env: &Env, admin: &Address, config: &GovernanceConfig) -> Result<(), GovernanceError> {
    admin.require_auth();
    if env.storage().instance().has(&DataKey::Admin) {
        return Err(GovernanceError::AlreadyInitialized);
    }
    validate_config(config)?;
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Config, config);
    env.storage().instance().set(&DataKey::ProposalCount, &0u64);
    Ok(())
}

/// Creates a proposal and stakes `deposit_amount` (bookkept as an i128
/// balance on this contract, matching the rest of this workspace's
/// bookkeeping-only pattern — no real token contract is integrated here).
/// Proposals go straight to `Active` (voting begins immediately): no
/// separate draft-editing period was specified in the source issue, so
/// `Draft` is included in `ProposalStatus` for lifecycle fidelity but is
/// not currently a reachable state.
pub fn create_proposal(
    env: &Env,
    proposer: &Address,
    deposit_amount: i128,
    action: ProposalAction,
    description: BytesN<32>,
) -> Result<u64, GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    proposer.require_auth();
    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;
    if deposit_amount < config.min_proposal_deposit || deposit_amount <= 0 {
        return Err(GovernanceError::InsufficientDeposit);
    }
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::ProposalCount)
        .unwrap_or(0);
    let now = env.ledger().timestamp();
    let voting_ends_at = now
        .checked_add(config.voting_period_seconds)
        .ok_or(GovernanceError::InvalidConfig)?;
    let proposal = Proposal {
        id,
        proposer: proposer.clone(),
        deposit_amount,
        action,
        description,
        status: ProposalStatus::Active,
        created_at: now,
        voting_ends_at,
        timelock_ends_at: 0,
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
    };
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(id), &proposal);
    env.storage()
        .persistent()
        .set(&DataKey::Deposit(id), &deposit_amount);
    env.storage().instance().set(
        &DataKey::ProposalCount,
        &id.checked_add(1).ok_or(GovernanceError::InvalidConfig)?,
    );
    add_proposal_to_status_index(env, &ProposalStatus::Active, id);
    ProposalCreated {
        id,
        proposer: proposer.clone(),
        deposit_amount,
        voting_ends_at,
    }
    .publish(env);
    Ok(id)
}

/// Vote power is flat one-address-one-vote (no governance-token balance
/// exists anywhere in this workspace to weight by; see the `GovernanceConfig`
/// doc comment in types.rs for the same reasoning behind `quorum_votes`).
pub fn cast_vote(
    env: &Env,
    voter: &Address,
    proposal_id: u64,
    vote: VoteType,
) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    voter.require_auth();
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::VotingNotActive);
    }
    let now = env.ledger().timestamp();
    if now > proposal.voting_ends_at {
        return Err(GovernanceError::VotingEnded);
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::Vote(proposal_id, voter.clone()))
    {
        return Err(GovernanceError::AlreadyVoted);
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::Delegation(voter.clone()))
    {
        return Err(GovernanceError::Unauthorized);
    }

    let mut total_vote_power: i128 = 0;

    // Own power is read live at vote time (#435), not from a proposal snapshot.
    let p = get_vote_power(env, voter);
    total_vote_power =
        math::safe_add(total_vote_power, p).map_err(|_| GovernanceError::InvalidConfig)?;

    env.storage().persistent().set(
        &DataKey::Vote(proposal_id, voter.clone()),
        &VoteRecord {
            voter: voter.clone(),
            vote: vote.clone(),
            vote_power: p,
            timestamp: now,
        },
    );
    VoteCast {
        id: proposal_id,
        voter: voter.clone(),
        vote: vote.clone(),
        vote_power: p,
    }
    .publish(env);

    // Process delegators
    let delegators: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Delegators(voter.clone()))
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..delegators.len() {
        if let Some(d) = delegators.get(i) {
            if !env
                .storage()
                .persistent()
                .has(&DataKey::Vote(proposal_id, d.clone()))
            {
                let dp = get_vote_power(env, &d);
                total_vote_power = math::safe_add(total_vote_power, dp)
                    .map_err(|_| GovernanceError::InvalidConfig)?;

                env.storage().persistent().set(
                    &DataKey::Vote(proposal_id, d.clone()),
                    &VoteRecord {
                        voter: d.clone(),
                        vote: vote.clone(),
                        vote_power: dp,
                        timestamp: now,
                    },
                );
                VoteCast {
                    id: proposal_id,
                    voter: d.clone(),
                    vote: vote.clone(),
                    vote_power: dp,
                }
                .publish(env);
            }
        }
    }

    match vote {
        VoteType::For => {
            proposal.votes_for = math::safe_add(proposal.votes_for, total_vote_power)
                .map_err(|_| GovernanceError::InvalidConfig)?;
        }
        VoteType::Against => {
            proposal.votes_against = math::safe_add(proposal.votes_against, total_vote_power)
                .map_err(|_| GovernanceError::InvalidConfig)?;
        }
        VoteType::Abstain => {
            proposal.votes_abstain = math::safe_add(proposal.votes_abstain, total_vote_power)
                .map_err(|_| GovernanceError::InvalidConfig)?;
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    Ok(())
}

pub fn delegate(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    delegator.require_auth();

    if delegator == delegatee {
        return Err(GovernanceError::CircularDelegation);
    }

    // Check for transitive delegation (cannot delegate to someone who has already delegated)
    if env
        .storage()
        .persistent()
        .has(&DataKey::Delegation(delegatee.clone()))
    {
        return Err(GovernanceError::CircularDelegation);
    }

    // Check if delegator has delegators of their own (cannot delegate if you act as a delegatee)
    let delegators: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Delegators(delegator.clone()))
        .unwrap_or_else(|| Vec::new(env));
    if delegators.len() > 0 {
        return Err(GovernanceError::CircularDelegation);
    }

    if let Some(old_delegatee) = env
        .storage()
        .persistent()
        .get::<DataKey, Address>(&DataKey::Delegation(delegator.clone()))
    {
        remove_delegator(env, &old_delegatee, delegator);
    }

    env.storage()
        .persistent()
        .set(&DataKey::Delegation(delegator.clone()), delegatee);
    add_delegator(env, delegatee, delegator);

    Delegated {
        delegator: delegator.clone(),
        delegatee: delegatee.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn revoke_delegation(env: &Env, delegator: &Address) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    delegator.require_auth();

    if let Some(old_delegatee) = env
        .storage()
        .persistent()
        .get::<DataKey, Address>(&DataKey::Delegation(delegator.clone()))
    {
        remove_delegator(env, &old_delegatee, delegator);
        env.storage()
            .persistent()
            .remove(&DataKey::Delegation(delegator.clone()));
        DelegationRevoked {
            delegator: delegator.clone(),
        }
        .publish(env);
    }
    Ok(())
}

fn add_delegator(env: &Env, delegatee: &Address, delegator: &Address) {
    let mut list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Delegators(delegatee.clone()))
        .unwrap_or_else(|| Vec::new(env));
    list.push_back(delegator.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Delegators(delegatee.clone()), &list);
}

fn remove_delegator(env: &Env, delegatee: &Address, delegator: &Address) {
    let list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Delegators(delegatee.clone()))
        .unwrap_or_else(|| Vec::new(env));
    let mut new_list = Vec::new(env);
    for i in 0..list.len() {
        if let Some(d) = list.get(i) {
            if d != *delegator {
                new_list.push_back(d);
            }
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::Delegators(delegatee.clone()), &new_list);
}

/// Finalize a proposal after its voting period has ended.
///
/// Design note (#244): This function is intentionally permissionless (no caller
/// authentication check required), following the Compound Governor / OpenZeppelin
/// Governor standard. Anyone or any automated keeper can trigger finalization
/// once `voting_ends_at` has passed.
///
/// Security & Timelock Guarantees:
/// To prevent fast-tracking execution, when a proposal succeeds, its execution
/// timelock (`timelock_ends_at`) is anchored to the finalization timestamp:
/// `now + config.timelock_seconds`. Thus, even if finalized immediately after voting
/// ends, execution is strictly locked until the full timelock delay has elapsed,
/// ensuring sufficient review time and preventing surprise executions.
pub fn finalize_proposal(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::VotingNotActive);
    }
    let now = env.ledger().timestamp();
    if now <= proposal.voting_ends_at {
        return Err(GovernanceError::VotingNotActive);
    }
    let passed = proposal_passes(&proposal, &config)?;
    remove_proposal_from_status_index(env, &ProposalStatus::Active, proposal_id);
    let deposit = env
        .storage()
        .persistent()
        .get(&DataKey::Deposit(proposal_id))
        .unwrap_or(proposal.deposit_amount);
    env.storage()
        .persistent()
        .remove(&DataKey::Deposit(proposal_id));

    if passed {
        proposal.timelock_ends_at = now
            .checked_add(config.timelock_seconds)
            .ok_or(GovernanceError::InvalidConfig)?;
        proposal.status = ProposalStatus::Queued;
        add_proposal_to_status_index(env, &ProposalStatus::Queued, proposal_id);
        ProposalStatusChanged {
            id: proposal_id,
            status: ProposalStatus::Queued,
        }
        .publish(env);
        DepositRefunded {
            id: proposal_id,
            proposer: proposal.proposer.clone(),
            amount: deposit,
        }
        .publish(env);
    } else {
        proposal.status = ProposalStatus::Defeated;
        add_proposal_to_status_index(env, &ProposalStatus::Defeated, proposal_id);
        ProposalStatusChanged {
            id: proposal_id,
            status: ProposalStatus::Defeated,
        }
        .publish(env);
        DepositForfeited {
            id: proposal_id,
            proposer: proposal.proposer.clone(),
            amount: deposit,
        }
        .publish(env);
    }
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    Ok(())
}

pub(crate) fn proposal_passes(
    proposal: &Proposal,
    config: &GovernanceConfig,
) -> Result<bool, GovernanceError> {
    let directional_votes = math::safe_add(proposal.votes_for, proposal.votes_against)
        .map_err(|_| GovernanceError::InvalidConfig)?;
    let total_participation = math::safe_add(directional_votes, proposal.votes_abstain)
        .map_err(|_| GovernanceError::InvalidConfig)?;
    if total_participation < config.quorum_votes as i128 || directional_votes <= 0 {
        return Ok(false);
    }

    let votes_for_scaled = math::safe_mul(proposal.votes_for, BPS_DENOM)
        .map_err(|_| GovernanceError::InvalidConfig)?;
    let support_bps = math::safe_div(votes_for_scaled, directional_votes)
        .map_err(|_| GovernanceError::InvalidConfig)?;

    Ok(support_bps >= config.pass_threshold_bps as i128)
}

/// Permissionless execution after the timelock has elapsed.
pub fn execute_proposal(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Queued {
        return Err(GovernanceError::ProposalNotSucceeded);
    }
    let now = env.ledger().timestamp();
    if now < proposal.timelock_ends_at {
        return Err(GovernanceError::TimelockNotElapsed);
    }
    let args: Vec<Val> = proposal.action.args.clone();
    env.invoke_contract::<Val>(&proposal.action.target_contract, &proposal.action.method, args);
    proposal.status = ProposalStatus::Executed;
    remove_proposal_from_status_index(env, &ProposalStatus::Queued, proposal_id);
    add_proposal_to_status_index(env, &ProposalStatus::Executed, proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    ProposalExecuted {
        id: proposal_id,
        executed_by: env.current_contract_address(),
    }
    .publish(env);
    Ok(())
}

/// Cancel own proposal — only while Active and before any votes have been
/// cast (adapted from "before voting starts": voting begins immediately at
/// creation in this implementation, see `create_proposal`). Refunds the
/// bookkept deposit.
pub fn cancel_proposal(
    env: &Env,
    caller: &Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    caller.require_auth();
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if &proposal.proposer != caller {
        return Err(GovernanceError::NotProposer);
    }
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::ProposalNotDraftOrActive);
    }
    if proposal.votes_for + proposal.votes_against + proposal.votes_abstain > 0 {
        return Err(GovernanceError::VotingAlreadyStarted);
    }
    proposal.status = ProposalStatus::Cancelled;
    remove_proposal_from_status_index(env, &ProposalStatus::Active, proposal_id);
    add_proposal_to_status_index(env, &ProposalStatus::Cancelled, proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    let deposit = env
        .storage()
        .persistent()
        .get(&DataKey::Deposit(proposal_id))
        .unwrap_or(proposal.deposit_amount);
    env.storage()
        .persistent()
        .remove(&DataKey::Deposit(proposal_id));
    ProposalCancelled {
        id: proposal_id,
        cancelled_by: caller.clone(),
    }
    .publish(env);
    DepositRefunded {
        id: proposal_id,
        proposer: caller.clone(),
        amount: deposit,
    }
    .publish(env);
    Ok(())
}

/// Execution grace period / window after deadline before an unexecuted proposal expires (7 days).
pub const PROPOSAL_EXECUTION_WINDOW: u64 = 604_800;

/// Permissionless finalization / state cleanup of expired proposals past deadline without execution.
/// Refunds creation deposit and emits ProposalExpired event.
pub fn expire_proposal(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let now = env.ledger().timestamp();
    let is_expired = match proposal.status {
        ProposalStatus::Active => {
            let deadline = proposal
                .voting_ends_at
                .checked_add(PROPOSAL_EXECUTION_WINDOW)
                .ok_or(GovernanceError::InvalidConfig)?;
            now > deadline
        }
        ProposalStatus::Queued => {
            let deadline = proposal
                .timelock_ends_at
                .checked_add(PROPOSAL_EXECUTION_WINDOW)
                .ok_or(GovernanceError::InvalidConfig)?;
            now > deadline
        }
        _ => false,
    };

    if !is_expired {
        return Err(GovernanceError::ProposalNotExpired);
    }

    let old_status = proposal.status.clone();
    proposal.status = ProposalStatus::Expired;
    remove_proposal_from_status_index(env, &old_status, proposal_id);
    add_proposal_to_status_index(env, &ProposalStatus::Expired, proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);

    let deposit_amount = proposal.deposit_amount;
    env.storage()
        .persistent()
        .remove(&DataKey::Deposit(proposal_id));

    ProposalExpired {
        id: proposal_id,
        proposer: proposal.proposer.clone(),
        deposit_refunded: deposit_amount,
    }
    .publish(env);

    ProposalStatusChanged {
        id: proposal_id,
        status: ProposalStatus::Expired,
    }
    .publish(env);

    Ok(())
}

/// Queues a `GovernanceConfig` change, executable no sooner than
/// `CONFIG_TIMELOCK_SECONDS` (48h) from now. Only one update may be queued
/// at a time — cancel the pending one first to replace it — so the
/// executable timestamp a caller observes can't be silently pushed back out
/// by re-queuing.
pub fn queue_config_update(
    env: &Env,
    admin: &Address,
    new_config: GovernanceConfig,
) -> Result<(), GovernanceError> {
    admin.require_auth();
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    if env.storage().instance().has(&DataKey::PendingConfig) {
        return Err(GovernanceError::ConfigUpdateAlreadyQueued);
    }
    validate_config(&new_config)?;
    let now = env.ledger().timestamp();
    let executable_at = now
        .checked_add(CONFIG_TIMELOCK_SECONDS)
        .ok_or(GovernanceError::InvalidConfig)?;
    env.storage().instance().set(
        &DataKey::PendingConfig,
        &PendingConfigUpdate {
            new_config,
            queued_at: now,
            executable_at,
        },
    );
    ConfigUpdateQueued {
        queued_by: admin.clone(),
        executable_at,
    }
    .publish(env);
    Ok(())
}

/// Permissionless, like `execute_proposal`: the timelock is the control,
/// not who happens to submit the transaction once it has elapsed.
pub fn execute_config_update(env: &Env) -> Result<(), GovernanceError> {
    let pending: PendingConfigUpdate = env
        .storage()
        .instance()
        .get(&DataKey::PendingConfig)
        .ok_or(GovernanceError::NoPendingConfigUpdate)?;
    let now = env.ledger().timestamp();
    if now < pending.executable_at {
        return Err(GovernanceError::TimelockNotElapsed);
    }
    env.storage()
        .instance()
        .set(&DataKey::Config, &pending.new_config);
    env.storage().instance().remove(&DataKey::PendingConfig);
    ConfigUpdated {
        updated_by: env.current_contract_address(),
    }
    .publish(env);
    Ok(())
}

/// Admin-only: cancel a queued config update at any point during (or after)
/// the timelock, before it has been executed.
pub fn cancel_config_update(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    admin.require_auth();
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    if !env.storage().instance().has(&DataKey::PendingConfig) {
        return Err(GovernanceError::NoPendingConfigUpdate);
    }
    env.storage().instance().remove(&DataKey::PendingConfig);
    ConfigUpdateCancelled {
        cancelled_by: admin.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn get_pending_config_update(env: &Env) -> Option<PendingConfigUpdate> {
    env.storage().instance().get(&DataKey::PendingConfig)
}

pub fn get_proposal(env: &Env, id: u64) -> Result<Proposal, GovernanceError> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(id))
        .ok_or(GovernanceError::ProposalNotFound)
}

/// Efficient status-indexed lookup (fixes #243)
pub fn get_proposals(env: &Env, status: ProposalStatus, limit: u32) -> Vec<Proposal> {
    let ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status))
        .unwrap_or_else(|| Vec::new(env));
    let mut out = Vec::new(env);
    let mut i = 0u32;
    while i < ids.len() && (out.len() as u32) < limit {
        if let Some(id) = ids.get(i) {
            if let Some(p) = env
                .storage()
                .persistent()
                .get::<DataKey, Proposal>(&DataKey::Proposal(id))
            {
                out.push_back(p);
            }
        }
        i += 1;
    }
    out
}

fn proposal_metadata(proposal: Proposal) -> ProposalMetadata {
    ProposalMetadata {
        id: proposal.id,
        proposer: proposal.proposer,
        description: proposal.description,
        status: proposal.status,
        created_at: proposal.created_at,
        voting_ends_at: proposal.voting_ends_at,
        timelock_ends_at: proposal.timelock_ends_at,
    }
}

pub fn get_proposal_metadata_page(env: &Env, cursor: u64, limit: u32) -> ProposalMetadataPage {
    let total: u64 = env
        .storage()
        .instance()
        .get(&DataKey::ProposalCount)
        .unwrap_or(0);
    let capped_limit = if limit > 50 { 50 } else { limit };
    let mut entries = Vec::new(env);
    let mut id = cursor;
    while id < total && (entries.len() as u32) < capped_limit {
        if let Some(proposal) = env
            .storage()
            .persistent()
            .get::<DataKey, Proposal>(&DataKey::Proposal(id))
        {
            entries.push_back(proposal_metadata(proposal));
        }
        id += 1;
    }
    ProposalMetadataPage {
        entries,
        next_cursor: id,
        total,
    }
}

pub fn get_vote(env: &Env, proposal_id: u64, voter: &Address) -> Option<VoteRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Vote(proposal_id, voter.clone()))
}

/// Flat vote power (see `cast_vote` doc comment).
pub fn get_vote_power(env: &Env, voter: &Address) -> i128 {
    if let Some(staking_addr) = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::StakingContract)
    {
        env.invoke_contract(
            &staking_addr,
            &soroban_sdk::Symbol::new(env, "get_voting_power"),
            soroban_sdk::vec![env, voter.to_val()],
        )
    } else {
        1
    }
}

pub fn set_staking_contract(
    env: &Env,
    admin: &Address,
    staking: &Address,
) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    admin.require_auth();
    env.storage()
        .instance()
        .set(&DataKey::StakingContract, staking);
    Ok(())
}

pub fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| GovernanceError::ContractPaused)
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| GovernanceError::ContractPaused)
}

pub fn get_deposit(env: &Env, id: u64) -> Option<i128> {
    env.storage().persistent().get(&DataKey::Deposit(id))
}
