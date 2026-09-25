use soroban_sdk::{contracterror, contractevent, contracttype, Address};

/// Staking period options with corresponding voting power multipliers
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum StakingPeriod {
    OneMonth = 1,   // 1 month = 1x multiplier
    ThreeMonths = 3, // 3 months = 2x multiplier
    SixMonths = 6,   // 6 months = 3x multiplier
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

/// Storage keys for the staking contract
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Paused,
    /// Map<Address, StakePosition> - user's active stake
    Stake(Address),
    /// Map<Address, UnbondingPosition> - user's unstaking position
    Unbonding(Address),
    /// Total staked amount across all users
    TotalStaked,
    /// Ordered list of addresses with an active stake position.
    /// Updated on stake() and unstake(). Used by get_all_stakers().
    StakerList,
    /// RewardConfig - global APY configuration
    RewardConfig,
    /// Map<Address, AccruedRewardState> - user's accrued reward state
    RewardState(Address),
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
pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;

/// Global staking reward configuration
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardConfig {
    pub apy_bps: u32,
    pub updated_at: u64,
}

/// User's accrued reward state tracking piecewise intervals
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccruedRewardState {
    pub accrued_amount: i128,
    pub last_accrual_time: u64,
}

/// Event emitted when reward config (APY) is updated
#[contractevent(topics = ["reward_cfg"])]
#[derive(Clone, Debug)]
pub struct RewardConfigUpdated {
    pub old_apy_bps: u32,
    pub new_apy_bps: u32,
    pub updated_at: u64,
}

/// Event emitted when staking rewards are distributed or claimed
#[contractevent(topics = ["reward_dist"])]
#[derive(Clone, Debug)]
pub struct RewardsDistributed {
    #[topic]
    pub user: Address,
    pub amount: i128,
}
