use soroban_sdk::{contracterror, contractevent, contracttype, Address, Vec};

/// Staking period options with corresponding voting power multipliers
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum StakingPeriod {
    OneMonth = 1,      // 1 month = 1x multiplier
    ThreeMonths = 3,   // 3 months = 2x multiplier
    SixMonths = 6,     // 6 months = 3x multiplier
    TwelveMonths = 12, // 12 months = 5x multiplier
}

impl StakingPeriod {
    /// Returns the voting power multiplier for this staking period
    pub fn multiplier(&self) -> u32 {
        match self {
            StakingPeriod::OneMonth => 1,
            StakingPeriod::ThreeMonths => 2,
            StakingPeriod::SixMonths => 3,
            StakingPeriod::TwelveMonths => 5,
        }
    }

    /// Returns the staking period in seconds
    pub fn as_seconds(&self) -> u64 {
        // 30 days per month approximation
        match self {
            StakingPeriod::OneMonth => 30 * 24 * 60 * 60,
            StakingPeriod::ThreeMonths => 90 * 24 * 60 * 60,
            StakingPeriod::SixMonths => 180 * 24 * 60 * 60,
            StakingPeriod::TwelveMonths => 360 * 24 * 60 * 60,
        }
    }

    /// Validates the period value and returns the corresponding StakingPeriod
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(StakingPeriod::OneMonth),
            3 => Some(StakingPeriod::ThreeMonths),
            6 => Some(StakingPeriod::SixMonths),
            12 => Some(StakingPeriod::TwelveMonths),
            _ => None,
        }
    }
}

/// Unbonding period in seconds (14 days)
pub const UNBONDING_PERIOD_SECONDS: u64 = 14 * 24 * 60 * 60;

/// Minimum stake accepted by the contract, expressed in stroop-like token units.
/// Rejecting smaller "dust" positions keeps voting snapshots and staker pages
/// from being spammed with economically meaningless entries.
pub const MIN_STAKE_AMOUNT: i128 = 1_0000000;

/// Storage keys for the staking contract
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Paused,
    /// Map<Address, StakePosition> - user's active stake
    Stake(Address),
    /// Map<Address, UnbondingPosition> - user's unbonding position
    Unbonding(Address),
    /// Total staked amount across all users
    TotalStaked,
    /// Ordered list of addresses with an active stake position.
    /// Updated on stake() and unstake(). Used by get_all_stakers().
    StakerList,
    /// Active slash notice: (amount, notice_until, executor)
    SlashNotice(Address),
    /// Global slash notice period in seconds (default: 86400 = 24 hours)
    SlashNoticePeriod,
}

/// User's active staking position
#[derive(Clone, Debug)]
#[contracttype]
pub struct StakePosition {
    pub amount: i128,
    pub period: StakingPeriod,
    pub start_time: u64,
    pub unlock_time: u64,
    pub voting_power: i128,
    pub start_ledger: u32,
    pub unlock_ledger: u32,
}

/// Stake query summary for account timing, amount, and accrued rewards
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct StakeInfo {
    pub amount: i128,
    pub start_ledger: u32,
    pub unlock_ledger: u32,
    pub accrued_rewards: i128,
}

/// User's unbonding position (after unstake initiated)
#[derive(Clone, Debug)]
#[contracttype]
pub struct UnbondingPosition {
    pub amount: i128,
    pub unbonding_start_time: u64,
    pub claimable_time: u64,
}

/// Contract-specific errors
#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StakingError {
    NotInitialized = 100,
    Unauthorized = 101,
    ContractPaused = 102,
    InvalidAmount = 103,
    InvalidPeriod = 104,
    TokenError = 105,
    InsufficientBalance = 106,
    NoActiveStake = 107,
    StakeNotUnlocked = 108,
    UnbondingNotComplete = 109,
    NoUnbondingPosition = 110,
    AlreadyStaked = 111,
    Overflow = 112,
    /// The staking contract does not hold enough tokens to fulfil the claim.
    /// Returned as a typed preflight error before attempting the transfer so
    /// callers (and the Go-side error classifier) receive a machine-readable
    /// code rather than an opaque host panic.
    InsufficientContractBalance = 113,
    SlashNoticePeriodActive = 114,
    NoActiveSlashNotice = 115,
    SlashNotAuthorized = 116,
    SlashNoticeExpired = 117,
}

/// Event emitted when tokens are staked
#[contractevent(topics = ["staked"])]
#[derive(Clone, Debug)]
pub struct Staked {
    #[topic]
    pub user: Address,
    pub amount: i128,
    pub period: u32,
    pub multiplier: u32,
    pub voting_power: i128,
}

/// Event emitted when unstake is initiated
#[contractevent(topics = ["unstake"])]
#[derive(Clone, Debug)]
pub struct UnstakeInitiated {
    #[topic]
    pub user: Address,
    pub amount: i128,
    pub claimable_time: u64,
}

/// Event emitted when tokens are claimed after unbonding
#[contractevent(topics = ["claimed"])]
#[derive(Clone, Debug)]
pub struct Claimed {
    #[topic]
    pub user: Address,
    pub amount: i128,
}

/// Event emitted when voting power is queried (for governance integration)
#[contractevent(topics = ["vp_query"])]
#[derive(Clone, Debug)]
pub struct VotingPowerQueried {
    #[topic]
    pub user: Address,
    pub voting_power: i128,
}

/// Event emitted when a slash notice is created
#[contractevent(topics = ["slash_notice"])]
#[derive(Clone, Debug)]
pub struct SlashNoticeCreated {
    #[topic]
    pub executor: Address,
    pub user: Address,
    pub amount: i128,
    pub notice_until: u64,
}

/// Event emitted when a slash is cancelled during the notice period
#[contractevent(topics = ["slash_cancelled"])]
#[derive(Clone, Debug)]
pub struct SlashCancelled {
    #[topic]
    pub executor: Address,
    pub user: Address,
    pub amount: i128,
}

/// Event emitted when a slash is executed after the notice period
#[contractevent(topics = ["slash_executed"])]
#[derive(Clone, Debug)]
pub struct SlashExecuted {
    #[topic]
    pub executor: Address,
    pub user: Address,
    pub amount: i128,
/// Event emitted when stake is slashed (#523)
#[contractevent(topics = ["slashed"])]
#[derive(Clone, Debug)]
pub struct Slashed {
    #[topic]
    pub user: Address,
    pub amount: i128,
    pub shortfall: i128,
}

// ── #445 — Paginated staker query ────────────────────────────────────────────

/// Maximum number of entries returned by a single `query_stakers_page` call.
/// Callers that pass a higher `limit` will silently receive at most this many.
pub const MAX_STAKERS_PAGE_SIZE: u32 = 50;

/// One entry in a `StakersPage`: the staker's address and their current staked amount.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StakerEntry {
    pub address: Address,
    pub amount: i128,
}

/// Result of `query_stakers_page`.
///
/// - `entries`     — up to `MAX_STAKERS_PAGE_SIZE` entries starting at the requested cursor.
/// - `next_cursor` — pass this as `cursor` in the next call; equals `total` when exhausted.
/// - `total`       — total number of active stakers at query time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StakersPage {
    pub entries: Vec<StakerEntry>,
    pub next_cursor: u32,
    pub total: u32,
}
