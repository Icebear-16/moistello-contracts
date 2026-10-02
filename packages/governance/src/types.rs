use soroban_sdk::{contracttype,contracterror,contractevent,Address,BytesN,Symbol,Vec,Val};

#[contracttype]
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum ProposalStatus{Draft,Active,Succeeded,Queued,Executed,Defeated,Cancelled}

#[contracttype]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum VoteType{For,Against,Abstain}

#[contracttype]
#[derive(Clone,Debug)]
pub struct ProposalAction{
    pub target_contract:Address,
    pub method:Symbol,
    pub args:Vec<Val>,
}

#[contracttype]
#[derive(Clone,Debug)]
pub struct Proposal{
    pub id:u64,
    pub proposer:Address,
    pub deposit_amount:i128,
    pub action:ProposalAction,
    pub description:BytesN<32>,
    pub status:ProposalStatus,
    pub created_at:u64,
    pub voting_ends_at:u64,
    pub timelock_ends_at:u64,
    pub votes_for:i128,
    pub votes_against:i128,
    pub votes_abstain:i128,
}

#[contracttype]
#[derive(Clone,Debug)]
pub struct VoteRecord{
    pub voter:Address,
    pub vote:VoteType,
    pub vote_power:i128,
    pub timestamp:u64,
}

/// Deviates from the uxupgrade.md spec's `quorum_bps` (percentage of total
/// governance-token supply): this workspace has no governance-token contract
/// (uxupgrade.md's "Current State" claim that one exists does not match the
/// actual repo — no such package exists). Vote power here is one-address-
/// one-vote (see `cast_vote`), so quorum is expressed as an absolute vote
/// count instead of a percentage of a supply that doesn't exist anywhere
/// to query.
#[contracttype]
#[derive(Clone,Debug)]
pub struct GovernanceConfig{
    pub proposal_deposit:i128,
    pub voting_period_seconds:u64,
    pub timelock_seconds:u64,
    pub quorum_votes:u32,
    pub pass_threshold_bps:u32,
    pub min_proposal_deposit:i128,
}

/// A `GovernanceConfig` update queued via `queue_config_update`, executable
/// only after `executable_at` — a fixed 48h from `queued_at` (see
/// `CONFIG_TIMELOCK_SECONDS` in contract.rs). Unlike `Proposal.timelock_ends_at`
/// (derived from the admin-adjustable `GovernanceConfig.timelock_seconds`),
/// this delay is a hardcoded constant so the admin cannot shorten or remove
/// it by first queuing a config change that lowers it.
#[contracttype]
#[derive(Clone,Debug)]
pub struct PendingConfigUpdate{
    pub new_config:GovernanceConfig,
    pub queued_at:u64,
    pub executable_at:u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey{
    Admin,
    Config,
    ProposalCount,
    Proposal(u64),
    Vote(u64,Address),
    Deposit(u64),
    PendingConfig,
    ProposalsByStatus(ProposalStatus),
    StakingContract,
}

#[contracterror]
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum GovernanceError{
    NotInitialized=1,
    AlreadyInitialized=2,
    Unauthorized=3,
    ContractPaused=4,
    InvalidConfig=5,
    InsufficientDeposit=6,
    ProposalNotFound=7,
    VotingNotActive=8,
    VotingEnded=9,
    AlreadyVoted=10,
    TimelockNotElapsed=11,
    ProposalNotSucceeded=12,
    QuorumNotMet=13,
    ProposalNotDraftOrActive=14,
    NotProposer=15,
    VotingAlreadyStarted=16,
    ConfigUpdateAlreadyQueued=17,
    NoPendingConfigUpdate=18,
}

#[contractevent(topics=["proposal"])]
#[derive(Clone,Debug)]
pub struct ProposalCreated{#[topic]pub id:u64,#[topic]pub proposer:Address,pub deposit_amount:i128,pub voting_ends_at:u64}

#[contractevent(topics=["vote"])]
#[derive(Clone,Debug)]
pub struct VoteCast{#[topic]pub id:u64,#[topic]pub voter:Address,pub vote:VoteType,pub vote_power:i128}

#[contractevent(topics=["status"])]
#[derive(Clone,Debug)]
pub struct ProposalStatusChanged{#[topic]pub id:u64,pub status:ProposalStatus}

#[contractevent(topics=["executed"])]
#[derive(Clone,Debug)]
pub struct ProposalExecuted{#[topic]pub id:u64,#[topic]pub executed_by:Address}

#[contractevent(topics=["cancelled"])]
#[derive(Clone,Debug)]
pub struct ProposalCancelled{#[topic]pub id:u64,#[topic]pub cancelled_by:Address}

#[contractevent(topics=["cfg_upd"])]
#[derive(Clone,Debug)]
pub struct ConfigUpdated{#[topic]pub updated_by:Address}

#[contractevent(topics=["cfg_queue"])]
#[derive(Clone,Debug)]
pub struct ConfigUpdateQueued{#[topic]pub queued_by:Address,pub executable_at:u64}

#[contractevent(topics=["cfg_cncl"])]
#[derive(Clone,Debug)]
pub struct ConfigUpdateCancelled{#[topic]pub cancelled_by:Address}

#[contractevent(topics=["deposit_refund"])]
#[derive(Clone,Debug)]
pub struct DepositRefunded{#[topic]pub id:u64,#[topic]pub proposer:Address,pub amount:i128}

#[contractevent(topics=["deposit_forfeit"])]
#[derive(Clone,Debug)]
pub struct DepositForfeited{#[topic]pub id:u64,#[topic]pub proposer:Address,pub amount:i128}
