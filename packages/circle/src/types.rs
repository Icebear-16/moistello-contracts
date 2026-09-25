
use soroban_sdk::{contracttype,contracterror,Address,String,BytesN,Vec};
// Re-export canonical CircleConfig from common to avoid duplicate type definitions (#320).
pub use common::types::CircleConfig;
pub const PAYOUT_RANDOM:u32=0;pub const PAYOUT_FIXED:u32=1;pub const PAYOUT_AUCTION:u32=2;pub const PAYOUT_VOTE:u32=3;
pub const STATUS_PENDING:u32=0;pub const STATUS_ACTIVE:u32=1;pub const STATUS_COMPLETED:u32=2;pub const STATUS_CANCELLED:u32=3;pub const STATUS_DISPUTED:u32=4;
pub const MEMBER_ACTIVE:u32=0;pub const MEMBER_EXITED:u32=1;pub const MEMBER_DEFAULTED:u32=2;
pub const RESOLVE_DISMISS:u32=1;pub const RESOLVE_PENALIZE:u32=2;pub const RESOLVE_FORCE_PAYOUT:u32=3;
pub const JOIN_RATE_LIMIT_LEDGERS:u32=100;
#[contracttype]#[derive(Clone,Debug)]
pub struct Circle{pub id:Address,pub token:Address,pub name:String,pub organizer:Address,pub factory:Address,pub contribution_amount:i128,pub max_members:u32,pub member_count:u32,pub payout_type:u32,pub total_rounds:u32,pub current_round:u32,pub status:u32,pub started_at:u64,pub created_at:u64,pub contribution_deadline_seconds:u64,pub min_moi_score:u32,pub collateral_amount:i128,pub penalty_bps:u32,pub grace_period_seconds:u64,pub max_strikes:u32,pub payout_bitmap:u128,pub total_payouts:i128,pub total_fees:i128,pub slug:String}
#[contracttype]#[derive(Clone,Debug)]pub struct Member{pub address:Address,pub position:u32,pub joined_at:u64,pub strikes:u32,pub status:u32,pub exited_at:u64,pub total_contributions:i128,pub total_received:i128}
#[contracttype]#[derive(Clone,Debug)]pub struct Contribution{pub member:Address,pub round:u32,pub amount:i128,pub timestamp:u64,pub on_time:bool,pub time_weight:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct PayoutRecipient{pub recipient:Address,pub round:u32,pub amount:i128,pub fee:i128,pub payout_type:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct AuctionBid{pub bidder:Address,pub discount_bips:u32,pub round:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct VoteEntry{pub voter:Address,pub vote_for:Address,pub round:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeEntry{pub raised_by:Address,pub evidence_hash:BytesN<32>,pub raised_at:u64,pub resolved_at:u64,pub resolution:u32,pub resolved_by:Address}
#[contracttype]#[derive(Clone)]pub enum DataKey{Circle,Admin,Factory,Members,Contributions,Payouts,Bids,Votes,Dispute,FeeBps,Treasury,Allowlist,Token,Referrals,ReputationRegistry,OracleContract,FallbackOracle,MultisigAdmins,MultisigThreshold,OraclePubkey,OracleCache,MultisigApprovals,Streaks,StreakBonusConfig,PendingResolution,Analytics,MemberStatsMap,JoinAttempt(Address),StorageVersion,OrganizerCandidate,OrganizerVotes}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]pub enum CircleError{NotInitialized=1,NotActive=2,CircleFull=3,AlreadyMember=4,NotMember=5,InsufficientMoiScore=6,RoundNotCurrent=7,InvalidAmount=8,PaymentDeadlinePassed=9,MaxStrikesReached=10,NotOrganizer=11,ContractPaused=12,InvalidInviteCode=13,AuctionAlreadyResolved=14,VoteQuorumNotMet=15,AlreadyContributed=16,AlreadyVoted=17,AlreadyBidded=18,PayoutAlreadyExecuted=19,InvalidPayoutType=20,InvalidRound=21,ContributionMismatch=22,CircleNotFull=23,NotEnoughVotes=24,DisputeAlreadyRaised=25,NoActiveDispute=26,Unauthorized=27,InvalidBid=28,InvalidMemberStatus=29,EmptyPayoutOrder=30,CircleSizeExceedsTier=31,ContributionExceedsTier=32,VecAccessError=33,AllowlistNotPermitted=34,InsufficientContractBalance=35,SelfReferral=36,OracleUnavailable=37,NotImplemented=38,ZeroPayoutAmount=39,ContributionsExist=40,InvalidAddress=41,ZeroAddress=42,TxExpired=43,InvalidExpiryBound=44,MultisigThresholdNotMet=45,InvalidMultisigConfig=46,InvalidOracleSignature=47,WrongOracle=48,MultisigAlreadyApproved=49,MultisigNoApprovals=50,NoPendingResolution=51,ResolutionTimelockActive=52,ResolutionChallenged=53,InvalidBonusPct=54,JoinRateLimited=55,MigrationValidationFailed=56,IncompatibleStorageVersion=57,MigrationPending=58}
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
#[contracttype]#[derive(Clone,Debug)]pub struct FactoryConfigured{pub factory:Address,pub treasury:Address,pub reputation_registry:Address,pub fee_bps:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeResolutionProposed{pub admin:Address,pub resolution:u32,pub execute_after:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeResolutionChallenged{pub member:Address}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct DisputeWindowExtended{pub challenger:Address,pub extension_seconds:u64,pub new_execute_after:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct DisputeResolved{pub admin:Address,pub resolution:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct BatchExitExecuted{pub round:u32,pub exited_count:u32}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct OrganizerReplaced{pub old_organizer:Address,pub new_organizer:Address}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum PayoutType{Random=0,Fixed=1,Auction=2,Vote=3}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum CircleStatus{Pending=0,Active=1,Completed=2,Cancelled=3,Disputed=4}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum MemberStatus{Active=0,Exited=1,Defaulted=2}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum DisputeResolution{Dismiss=1,Penalize=2,ForcePayout=3,Refund=4}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub enum CircleFrequency{Daily=0,Weekly=1,Biweekly=2,Monthly=3}
#[contracttype]#[derive(Clone,Debug)]pub struct Referral{pub referrer:Address,pub referred:Address,pub bonus_pct:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct Streak{pub member:Address,pub current_streak:u32,pub longest_streak:u32,pub last_round:u32}
/// Cached oracle yield rate with the ledger it was fetched at (#360).
/// Served without a cross-contract call while `current_ledger - ledger <= TTL`.
#[contracttype]#[derive(Clone,Debug)]pub struct OracleCache{pub rate:i128,pub ledger:u32,pub round:u32}
/// Stored N-of-M admin configuration (#359). `None` (no storage entry) means
/// single-admin mode — existing behaviour, unchanged.
#[contracttype]#[derive(Clone,Debug)]pub struct MultisigConfig{pub admins:Vec<Address>,pub threshold:u32}
/// Streak bonus configuration (#368). `bonus_pct` is basis points (0-10000)
/// applied on top of the linear base+per-day bonus, all under checked math.
#[contracttype]#[derive(Clone,Debug)]pub struct StreakBonusConfig{pub base_bonus:i128,pub multiplier_per_day:i128,pub bonus_pct:u32}
/// Pending, timelocked dispute resolution (#364). `execute_after` is the
/// ledger timestamp at/after which `execute_dispute_resolution` may apply it,
/// unless a member challenges it first.
#[contracttype]#[derive(Clone,Debug)]pub struct PendingDisputeResolution{pub resolution:u32,pub proposed_by:Address,pub proposed_at:u64,pub execute_after:u64,pub challenged:bool}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleAnalytics{pub total_contributions:i128,pub contribution_count:u32,pub total_payouts:i128,pub avg_completion_time:u64,pub total_members:u32,pub total_defaults:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct MemberStats{pub member:Address,pub joined_at:u64,pub contribution_count:u32,pub total_contributed:i128,pub total_received:i128,pub defaults:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct JoinAttempt{pub ledger:u32,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct MigrationApplied{pub from_version:u32,pub to_version:u32}


