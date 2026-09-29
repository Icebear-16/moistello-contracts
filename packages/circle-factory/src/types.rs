use soroban_sdk::{contracterror, contracttype, Address, BytesN, String, Vec};
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
#[derive(Clone, Debug, PartialEq)]
pub struct FeeConfig {
    pub fee_bps: i128,
    pub updated_at: u64,
    pub updated_by: Address,
}
/// Protocol-wide configuration propagated into every circle this factory deploys.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FactoryConfig {
    pub treasury: Address,
    pub reputation_registry: Address,
    pub fee_bps: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleEntry {
    pub circle_id: Address,
    pub name: String,
    pub organizer: Address,
    pub deployed_at: u64,
    pub status: u32,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleRegistry {
    pub circles: Vec<CircleEntry>,
}
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    FeeConfig,
    CircleList,
    CircleCount,
    WasmHash,
    CircleConfig(Address),
    CanonicalDeployment(BytesN<32>),
    RateLimitConfig,
    OrganizerPeriodCount(Address, u64),
    FactoryConfig,
}
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitConfig {
    pub limit: u32,
    pub period_secs: u64,
}
#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactoryError {
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    WasmHashNotSet = 4,
    InvalidFeeBps = 5,
    CircleDeployFailed = 6,
    InvalidConfig = 7,
    RateLimitExceeded = 8,
    /// `FactoryConfig` has not been set, so circles cannot be configured on deploy.
    FactoryConfigNotSet = 9,
    /// The freshly deployed circle rejected `configure_from_factory`.
    CircleConfigurationFailed = 10,
    /// `treasury` / `reputation_registry` is not a usable contract address.
    InvalidAddress = 11,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct FactoryConfigUpdated {
    pub treasury: Address,
    pub reputation_registry: Address,
    pub fee_bps: u32,
    pub updated_by: Address,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleDeployed {
    pub creator: Address,
    pub circle_id: Address,
    pub name: String,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeConfigUpdated {
    pub old_fee_bps: i128,
    pub new_fee_bps: i128,
    pub updated_by: Address,
}
