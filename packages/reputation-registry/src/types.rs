use soroban_sdk::{contracterror, contracttype, Address};
pub const ACTIVITY_JOIN: u32 = 0;
pub const ACTIVITY_CONTRIBUTE: u32 = 1;
pub const ACTIVITY_COMPLETE: u32 = 2;
pub const ACTIVITY_DEFAULT: u32 = 3;
pub const ACTIVITY_PAYOUT_RECEIVED: u32 = 4;
pub const TIER_BRONZE: u32 = 0;
pub const TIER_SILVER: u32 = 1;
pub const TIER_GOLD: u32 = 2;
pub const TIER_PLATINUM: u32 = 3;
pub const TIER_DIAMOND: u32 = 4;
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MoiScore {
    pub user: Address,
    pub score: u32,
    pub tier: u32,
    pub total_circles: u32,
    pub completed_circles: u32,
    pub defaulted_circles: u32,
    pub streak_count: u32,
    pub last_activity_at: u64,
    pub updated_at: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Activity {
    pub user: Address,
    pub activity_type: u32,
    pub score_impact: u32,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Score(Address),
    UserActivityPage(Address, u32),
    UserActivityCount(Address),
    Streak(Address, Address),
    Completions(Address),
    MemberScore(Address),
    Defaults(Address),
    MemberLog(Address),
    LastRound(Address, Address),
}
#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReputationError {
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidActivityType = 4,
    ScoreNotFound = 5,
    InvalidScoreImpact = 6,
    Overflow = 7,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct ActivityRecorded {
    pub user: Address,
    pub activity_type: u32,
    pub score_impact: u32,
    pub new_score: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct ScoreUpdated {
    pub user: Address,
    pub old_score: u32,
    pub new_score: u32,
    pub tier: u32,
}
