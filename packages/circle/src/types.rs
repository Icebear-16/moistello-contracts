use soroban_sdk::{contracttype,contracterror,Address,String,BytesN};
pub const PAYOUT_RANDOM:u32=0;pub const PAYOUT_FIXED:u32=1;pub const PAYOUT_AUCTION:u32=2;pub const PAYOUT_VOTE:u32=3;
pub const STATUS_PENDING:u32=0;pub const STATUS_ACTIVE:u32=1;pub const STATUS_COMPLETED:u32=2;pub const STATUS_CANCELLED:u32=3;pub const STATUS_DISPUTED:u32=4;
pub const MEMBER_ACTIVE:u32=0;pub const MEMBER_EXITED:u32=1;pub const MEMBER_DEFAULTED:u32=2;
pub const RESOLVE_DISMISS:u32=1;pub const RESOLVE_PENALIZE:u32=2;pub const RESOLVE_FORCE_PAYOUT:u32=3;
#[contracttype]#[derive(Clone,Debug)]
pub struct CircleConfig{pub organizer:Address,pub token:Address,pub name:String,pub contribution_amount:i128,pub max_members:u32,pub payout_type:u32,pub total_rounds:u32,pub contribution_deadline_seconds:u64,pub min_moi_score:u32,pub collateral_amount:i128,pub penalty_bps:u32,pub grace_period_seconds:u64,pub max_strikes:u32,pub slug:String,pub max_withdrawal_per_tx:i128,pub daily_withdrawal_limit:i128,pub min_duration_seconds:u64}
#[contracttype]#[derive(Clone,Debug)]
pub struct Circle{pub id:Address,pub token:Address,pub name:String,pub organizer:Address,pub factory:Address,pub contribution_amount:i128,pub max_members:u32,pub member_count:u32,pub payout_type:u32,pub total_rounds:u32,pub current_round:u32,pub status:u32,pub started_at:u64,pub created_at:u64,pub contribution_deadline_seconds:u64,pub min_moi_score:u32,pub collateral_amount:i128,pub penalty_bps:u32,pub grace_period_seconds:u64,pub max_strikes:u32,pub payout_bitmap:u128,pub total_payouts:i128,pub total_fees:i128,pub slug:String,pub max_withdrawal_per_tx:i128,pub daily_withdrawal_limit:i128,pub min_duration_seconds:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct Member{pub address:Address,pub position:u32,pub joined_at:u64,pub strikes:u32,pub status:u32,pub exited_at:u64,pub total_contributions:i128,pub total_received:i128}
#[contracttype]#[derive(Clone,Debug)]pub struct Contribution{pub member:Address,pub round:u32,pub amount:i128,pub timestamp:u64,pub on_time:bool,pub time_weight:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct PayoutRecipient{pub recipient:Address,pub round:u32,pub amount:i128,pub fee:i128,pub payout_type:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct AuctionBid{pub bidder:Address,pub discount_bips:u32,pub round:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct VoteEntry{pub voter:Address,pub vote_for:Address,pub round:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeEntry{pub raised_by:Address,pub evidence_hash:BytesN<32>,pub raised_at:u64,pub resolved_at:u64,pub resolution:u32,pub resolved_by:Address}
#[contracttype]#[derive(Clone)]pub enum DataKey{Circle,Admin,Factory,Members,Contributions,Payouts,Bids,Votes,Dispute,FeeBps,Treasury,Allowlist,Token,Referrals,ReputationRegistry,OracleContract,FallbackOracle,WithdrawalDay,WithdrawalAmount}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]pub enum CircleError{NotInitialized=1,NotActive=2,CircleFull=3,AlreadyMember=4,NotMember=5,InsufficientMoiScore=6,RoundNotCurrent=7,InvalidAmount=8,PaymentDeadlinePassed=9,MaxStrikesReached=10,NotOrganizer=11,ContractPaused=12,InvalidInviteCode=13,AuctionAlreadyResolved=14,VoteQuorumNotMet=15,AlreadyContributed=16,AlreadyVoted=17,AlreadyBidded=18,PayoutAlreadyExecuted=19,InvalidPayoutType=20,InvalidRound=21,ContributionMismatch=22,CircleNotFull=23,NotEnoughVotes=24,DisputeAlreadyRaised=25,NoActiveDispute=26,Unauthorized=27,InvalidBid=28,InvalidMemberStatus=29,EmptyPayoutOrder=30,CircleSizeExceedsTier=31,ContributionExceedsTier=32,VecAccessError=33,AllowlistNotPermitted=34,InsufficientContractBalance=35,SelfReferral=36,OracleUnavailable=37,NotImplemented=38,ZeroPayoutAmount=39,WithdrawalCapExceeded=40,DailyWithdrawalLimitExceeded=41,CircleDurationTooShort=42}
#[contracttype]#[derive(Clone,Debug)]pub struct MemberJoined{pub member:Address,pub position:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct ContributionRecorded{pub member:Address,pub round:u32,pub amount:i128,pub on_time:bool}
#[contracttype]#[derive(Clone,Debug)]pub struct PayoutExecuted{pub recipient:Address,pub round:u32,pub amount:i128,pub fee:i128,pub payout_type:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct MemberExited{pub member:Address,pub penalty:i128}
#[contracttype]#[derive(Clone,Debug)]pub struct MemberDefaulted{pub member:Address,pub strikes:u32}
#[contracttype]#[derive(Clone,Debug)]#[derive(Default)]
pub struct CircleCompleted{pub total_payouts:i128}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleCancelled{pub circle_id:Address,pub cancelled_by:Address,pub cancelled_at:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeRaised{pub member:Address,pub evidence_hash:BytesN<32>}
#[contracttype]#[derive(Clone,Debug)]pub struct AuctionBidPlaced{pub bidder:Address,pub discount_bips:u32,pub round:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct VoteCast{pub voter:Address,pub vote_for:Address,pub round:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct ReferralRegistered{pub referrer:Address,pub referred:Address,pub bonus_pct:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct OracleFallbackUsed{pub round:u32,pub primary_oracle:Address,pub fallback_oracle:Address}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum PayoutType{Random=0,Fixed=1,Auction=2,Vote=3}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum CircleStatus{Pending=0,Active=1,Completed=2,Cancelled=3,Disputed=4}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum MemberStatus{Active=0,Exited=1,Defaulted=2}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum DisputeResolution{Dismiss=1,Penalize=2,ForcePayout=3,Refund=4}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum CircleFrequency{Daily=0,Weekly=1,Biweekly=2,Monthly=3}
#[contracttype]#[derive(Clone,Debug)]pub struct Referral{pub referrer:Address,pub referred:Address,pub bonus_pct:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct Streak{pub member:Address,pub current_streak:u32,pub longest_streak:u32,pub last_round:u32}
use soroban_sdk::{contracterror, contracttype, Address, BytesN, String};
pub const PAYOUT_RANDOM: u32 = 0;
pub const PAYOUT_FIXED: u32 = 1;
pub const PAYOUT_AUCTION: u32 = 2;
pub const PAYOUT_VOTE: u32 = 3;
pub const STATUS_PENDING: u32 = 0;
pub const STATUS_ACTIVE: u32 = 1;
pub const STATUS_COMPLETED: u32 = 2;
pub const STATUS_CANCELLED: u32 = 3;
pub const STATUS_DISPUTED: u32 = 4;
pub const MEMBER_ACTIVE: u32 = 0;
pub const MEMBER_EXITED: u32 = 1;
pub const MEMBER_DEFAULTED: u32 = 2;
pub const RESOLVE_DISMISS: u32 = 1;
pub const RESOLVE_PENALIZE: u32 = 2;
pub const RESOLVE_FORCE_PAYOUT: u32 = 3;
pub const RESOLVE_REFUND: u32 = 4;
pub const AUCTION_MODE_ENGLISH: u32 = 0;
pub const AUCTION_MODE_DUTCH: u32 = 1;
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleConfig {
    pub organizer: Address,
    pub token: Address,
    pub name: String,
    pub contribution_amount: i128,
    pub max_members: u32,
    pub payout_type: u32,
    pub total_rounds: u32,
    pub contribution_deadline_seconds: u64,
    pub min_moi_score: u32,
    pub collateral_amount: i128,
    pub penalty_bps: u32,
    pub grace_period_seconds: u64,
    pub max_strikes: u32,
    pub slug: String,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Circle {
    pub id: Address,
    pub token: Address,
    pub name: String,
    pub organizer: Address,
    pub factory: Address,
    pub contribution_amount: i128,
    pub max_members: u32,
    pub member_count: u32,
    pub payout_type: u32,
    pub total_rounds: u32,
    pub current_round: u32,
    pub status: u32,
    pub started_at: u64,
    pub created_at: u64,
    pub contribution_deadline_seconds: u64,
    pub min_moi_score: u32,
    pub collateral_amount: i128,
    pub penalty_bps: u32,
    pub grace_period_seconds: u64,
    pub max_strikes: u32,
    pub payout_bitmap: u128,
    pub total_payouts: i128,
    pub total_fees: i128,
    pub slug: String,
    pub health_score: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Member {
    pub address: Address,
    pub position: u32,
    pub joined_at: u64,
    pub strikes: u32,
    pub status: u32,
    pub exited_at: u64,
    pub total_contributions: i128,
    pub total_received: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Contribution {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
    pub timestamp: u64,
    pub on_time: bool,
    pub time_weight: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct PayoutRecipient {
    pub recipient: Address,
    pub round: u32,
    pub amount: i128,
    pub fee: i128,
    pub payout_type: u32,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoundFeeLedger {
    pub round: u32,
    pub payout_fee: i128,
    pub late_penalty_fee: i128,
    pub total_fee: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuctionBid {
    pub bidder: Address,
    pub discount_bips: u32,
    pub round: u32,
    pub timestamp: u64,
    /// Tokens escrowed with the bid. Refunded to losing bidders in batches
    /// after the auction resolves (`refund_losing_bids`).
    pub deposit: i128,
}
/// Configuration for a Dutch-style auction on `round`: `discount_bips` starts at
/// `start_bips` at `start_ledger` and decays by `decay_bips_per_ledger` per elapsed
/// ledger down to a floor of `floor_bips`. The first bid clears at whatever the
/// current decayed price is. If no bid lands within `expiry_ledgers` of
/// `start_ledger`, the auction is expired and must be reconfigured.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DutchAuctionConfig {
    pub round: u32,
    pub start_bips: u32,
    pub floor_bips: u32,
    pub decay_bips_per_ledger: u32,
    pub start_ledger: u32,
    pub expiry_ledgers: u32,
    pub resolved: bool,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteEntry {
    pub voter: Address,
    pub vote_for: Address,
    pub round: u32,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeEntry {
    pub raised_by: Address,
    pub evidence_hash: BytesN<32>,
    pub raised_at: u64,
    pub resolved_at: u64,
    pub resolution: u32,
    pub resolved_by: Address,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeResolutionRecord {
    pub raised_by: Address,
    pub resolution: u32,
    pub outcome_code: u32,
    pub resolved_by: Address,
    pub resolved_at: u64,
}
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Circle,
    Admin,
    Factory,
    Members,
    Contributions,
    Payouts,
    Bids,
    Votes,
    Dispute,
    DisputeResolution,
    FeeBps,
    Treasury,
    Allowlist,
    Token,
    Referrals,
    Streaks,
    ReputationRegistry,
    OracleContract,
    FallbackOracle,
    RoundConfigSnapshot(u32),
    PayoutScheduled(u32),
    DutchAuction(u32),
    RoundFeeLedger(u32),
    /// Winner of a resolved English auction for this round. Presence means
    /// losing-bid refunds may begin.
    AuctionWinner(u32),
    /// #325: holds the round number most recently swept by
    /// `check_contribution_deadline`. Kept as a single instance entry rather
    /// than one persistent entry per round, because a sweep resolves the round
    /// immediately and long-running circles would otherwise accumulate an
    /// entry per round against the ledger-entry budget.
    RoundEnforced,
}
pub use common::types::ErrorEnvelope;
#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircleError {
    NotInitialized = 1,
    NotActive = 2,
    CircleFull = 3,
    AlreadyMember = 4,
    NotMember = 5,
    InsufficientMoiScore = 6,
    RoundNotCurrent = 7,
    InvalidAmount = 8,
    PaymentDeadlinePassed = 9,
    MaxStrikesReached = 10,
    NotOrganizer = 11,
    ContractPaused = 12,
    InvalidInviteCode = 13,
    AuctionAlreadyResolved = 14,
    VoteQuorumNotMet = 15,
    AlreadyContributed = 16,
    AlreadyVoted = 17,
    AlreadyBidded = 18,
    PayoutAlreadyExecuted = 19,
    InvalidPayoutType = 20,
    InvalidRound = 21,
    ContributionMismatch = 22,
    CircleNotFull = 23,
    NotEnoughVotes = 24,
    DisputeAlreadyRaised = 25,
    NoActiveDispute = 26,
    Unauthorized = 27,
    InvalidBid = 28,
    InvalidMemberStatus = 29,
    EmptyPayoutOrder = 30,
    CircleSizeExceedsTier = 31,
    ContributionExceedsTier = 32,
    VecAccessError = 33,
    AllowlistNotPermitted = 34,
    InsufficientContractBalance = 35,
    SelfReferral = 36,
    OracleUnavailable = 37,
    NotImplemented = 38,
    ZeroPayoutAmount = 39,
    PayoutAlreadyScheduled = 59,
    DutchAuctionNotConfigured = 60,
    DutchAuctionExpired = 61,
    InvalidDutchConfig = 62,
    /// Losing-bid refunds were requested before the auction winner was recorded.
    AuctionNotResolved = 63,
    /// #329: the round cannot be resolved yet because at least one active
    /// member has not contributed and the contribution window is still open.
    InvalidContributionRound = 64,
    /// #325: deadline enforcement was requested before the round's
    /// contribution window (deadline plus grace) had actually closed.
    DeadlineNotPassed = 65,
    /// #323: a guarded entry point was re-entered while already executing.
    /// Defence in depth only — the Soroban host already prohibits re-entering
    /// a contract that is on the call stack, so this should be unreachable
    /// while that host policy holds. See the module docs in `contract.rs`.
    ReentrantCall = 66,
}

impl CircleError {
    pub fn to_envelope(
        &self,
        env: &soroban_sdk::Env,
        details: &str,
        request_id: u64,
    ) -> ErrorEnvelope {
        let (code, msg) = match self {
            CircleError::NotInitialized => (1, "Circle not initialized"),
            CircleError::NotActive => (2, "Circle not active"),
            CircleError::CircleFull => (3, "Circle is full"),
            CircleError::AlreadyMember => (4, "Already a member"),
            CircleError::NotMember => (5, "Not a member"),
            CircleError::InsufficientMoiScore => (6, "Insufficient MoiScore"),
            CircleError::RoundNotCurrent => (7, "Round not current"),
            CircleError::InvalidAmount => (8, "Invalid amount"),
            CircleError::PaymentDeadlinePassed => (9, "Payment deadline passed"),
            CircleError::MaxStrikesReached => (10, "Max strikes reached"),
            CircleError::NotOrganizer => (11, "Not organizer"),
            CircleError::ContractPaused => (12, "Contract paused"),
            CircleError::InvalidInviteCode => (13, "Invalid invite code"),
            CircleError::AuctionAlreadyResolved => (14, "Auction already resolved"),
            CircleError::VoteQuorumNotMet => (15, "Vote quorum not met"),
            CircleError::AlreadyContributed => (16, "Already contributed"),
            CircleError::AlreadyVoted => (17, "Already voted"),
            CircleError::AlreadyBidded => (18, "Already placed bid"),
            CircleError::PayoutAlreadyExecuted => (19, "Payout already executed"),
            CircleError::InvalidPayoutType => (20, "Invalid payout type"),
            CircleError::InvalidRound => (21, "Invalid round"),
            CircleError::ContributionMismatch => (22, "Contribution mismatch"),
            CircleError::CircleNotFull => (23, "Circle not full"),
            CircleError::NotEnoughVotes => (24, "Not enough votes"),
            CircleError::DisputeAlreadyRaised => (25, "Dispute already raised"),
            CircleError::NoActiveDispute => (26, "No active dispute"),
            CircleError::Unauthorized => (27, "Unauthorized"),
            CircleError::InvalidBid => (28, "Invalid bid"),
            CircleError::InvalidMemberStatus => (29, "Invalid member status"),
            CircleError::EmptyPayoutOrder => (30, "Empty payout order"),
            CircleError::CircleSizeExceedsTier => (31, "Circle size exceeds tier"),
            CircleError::ContributionExceedsTier => (32, "Contribution exceeds tier"),
            CircleError::VecAccessError => (33, "Vector access error"),
            CircleError::AllowlistNotPermitted => (34, "Allowlist not permitted"),
            CircleError::InsufficientContractBalance => (35, "Insufficient contract balance"),
            CircleError::SelfReferral => (36, "Self referral not allowed"),
            CircleError::OracleUnavailable => (37, "Oracle unavailable"),
            CircleError::NotImplemented => (38, "Not implemented"),
            CircleError::ZeroPayoutAmount => (39, "Zero payout amount"),
            CircleError::PayoutAlreadyScheduled => (59, "Payout already scheduled"),
            CircleError::DutchAuctionNotConfigured => (60, "Dutch auction not configured"),
            CircleError::DutchAuctionExpired => (61, "Dutch auction expired"),
            CircleError::InvalidDutchConfig => (62, "Invalid Dutch auction config"),
            CircleError::AuctionNotResolved => (63, "Auction not resolved"),
            CircleError::InvalidContributionRound => (64, "Round has outstanding contributions"),
            CircleError::DeadlineNotPassed => (65, "Contribution deadline not passed"),
            CircleError::ReentrantCall => (66, "Reentrant call rejected"),
        };
        ErrorEnvelope::new(env, code, msg, details, request_id)
    }

    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(CircleError::NotInitialized),
            2 => Some(CircleError::NotActive),
            3 => Some(CircleError::CircleFull),
            4 => Some(CircleError::AlreadyMember),
            5 => Some(CircleError::NotMember),
            6 => Some(CircleError::InsufficientMoiScore),
            7 => Some(CircleError::RoundNotCurrent),
            8 => Some(CircleError::InvalidAmount),
            9 => Some(CircleError::PaymentDeadlinePassed),
            10 => Some(CircleError::MaxStrikesReached),
            11 => Some(CircleError::NotOrganizer),
            12 => Some(CircleError::ContractPaused),
            13 => Some(CircleError::InvalidInviteCode),
            14 => Some(CircleError::AuctionAlreadyResolved),
            15 => Some(CircleError::VoteQuorumNotMet),
            16 => Some(CircleError::AlreadyContributed),
            17 => Some(CircleError::AlreadyVoted),
            18 => Some(CircleError::AlreadyBidded),
            19 => Some(CircleError::PayoutAlreadyExecuted),
            20 => Some(CircleError::InvalidPayoutType),
            21 => Some(CircleError::InvalidRound),
            22 => Some(CircleError::ContributionMismatch),
            23 => Some(CircleError::CircleNotFull),
            24 => Some(CircleError::NotEnoughVotes),
            25 => Some(CircleError::DisputeAlreadyRaised),
            26 => Some(CircleError::NoActiveDispute),
            27 => Some(CircleError::Unauthorized),
            28 => Some(CircleError::InvalidBid),
            29 => Some(CircleError::InvalidMemberStatus),
            30 => Some(CircleError::EmptyPayoutOrder),
            31 => Some(CircleError::CircleSizeExceedsTier),
            32 => Some(CircleError::ContributionExceedsTier),
            33 => Some(CircleError::VecAccessError),
            34 => Some(CircleError::AllowlistNotPermitted),
            35 => Some(CircleError::InsufficientContractBalance),
            36 => Some(CircleError::SelfReferral),
            37 => Some(CircleError::OracleUnavailable),
            38 => Some(CircleError::NotImplemented),
            39 => Some(CircleError::ZeroPayoutAmount),
            59 => Some(CircleError::PayoutAlreadyScheduled),
            60 => Some(CircleError::DutchAuctionNotConfigured),
            61 => Some(CircleError::DutchAuctionExpired),
            62 => Some(CircleError::InvalidDutchConfig),
            63 => Some(CircleError::AuctionNotResolved),
            64 => Some(CircleError::InvalidContributionRound),
            65 => Some(CircleError::DeadlineNotPassed),
            66 => Some(CircleError::ReentrantCall),
            _ => None,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MemberJoined {
    pub member: Address,
    pub position: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContributionRecorded {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
    pub on_time: bool,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct PayoutExecuted {
    pub recipient: Address,
    pub round: u32,
    pub amount: i128,
    pub fee: i128,
    pub payout_type: u32,
}
/// Issue #369: emitted whenever the circle's health score is recomputed
/// (currently: after every trigger_payout). `on_time_rate`, `completion_rate`,
/// and `member_retention` are basis points (0-10000) so off-chain consumers
/// don't need to guess a fixed-point scale.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleHealthUpdated {
    pub health_score: u32,
    pub on_time_rate_bps: u32,
    pub completion_rate_bps: u32,
    pub member_retention_bps: u32,
    pub round: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct MemberExited {
    pub member: Address,
    pub penalty: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct MemberDefaulted {
    pub member: Address,
    pub strikes: u32,
}
#[contracttype]
#[derive(Clone, Debug, Default)]
pub struct CircleCompleted {
    pub total_payouts: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleCancelled {
    pub circle_id: Address,
    pub cancelled_by: Address,
    pub cancelled_at: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeRaised {
    pub member: Address,
    pub evidence_hash: BytesN<32>,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeResolved {
    pub member: Address,
    pub resolution: u32,
    pub outcome_code: u32,
    pub resolved_by: Address,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuctionBidPlaced {
    pub bidder: Address,
    pub discount_bips: u32,
    pub round: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuctionLoserRefunded {
    pub bidder: Address,
    pub round: u32,
    pub amount: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteCast {
    pub voter: Address,
    pub vote_for: Address,
    pub round: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReferralRegistered {
    pub referrer: Address,
    pub referred: Address,
    pub bonus_pct: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct OracleFallbackUsed {
    pub round: u32,
    pub primary_oracle: Address,
    pub fallback_oracle: Address,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct LatePenaltyApplied {
    pub member: Address,
    pub round: u32,
    pub penalty: i128,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PayoutType {
    Random = 0,
    Fixed = 1,
    Auction = 2,
    Vote = 3,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CircleStatus {
    Pending = 0,
    Active = 1,
    Completed = 2,
    Cancelled = 3,
    Disputed = 4,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MemberStatus {
    Active = 0,
    Exited = 1,
    Defaulted = 2,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DisputeResolution {
    Dismiss = 1,
    Penalize = 2,
    ForcePayout = 3,
    Refund = 4,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CircleFrequency {
    Daily = 0,
    Weekly = 1,
    Biweekly = 2,
    Monthly = 3,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Referral {
    pub referrer: Address,
    pub referred: Address,
    pub bonus_pct: u32,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Streak {
    pub member: Address,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub last_round: u32,
}
