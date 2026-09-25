use soroban_sdk::{contracttype, contracterror, Address, String, Vec};
// Re-export canonical CircleConfig from common to avoid duplicate type definitions (#320).
pub use common::types::CircleConfig;
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct FeeConfig{pub fee_bps:i128,pub updated_at:u64,pub updated_by:Address}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleEntry{pub circle_id:Address,pub name:String,pub organizer:Address,pub deployed_at:u64,pub status:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleRegistry{pub circles:Vec<CircleEntry>}
pub const MAX_DEPLOY_FEE: i128 = 10_000_0000000;
#[contracttype]#[derive(Clone)]pub enum DataKey{Admin,FeeConfig,CircleList,CircleCount,WasmHash,CircleConfig(Address),Slug(String),Templates,DeployFee,Treasury}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]pub enum FactoryError{NotInitialized=1,Unauthorized=2,ContractPaused=3,WasmHashNotSet=4,InvalidFeeBps=5,CircleDeployFailed=6,InvalidConfig=7,DuplicateSlug=8,EmptySlug=9,TemplateNotFound=10,TemplateExists=11,TemplateOutOfBounds=12,InvalidTemplate=13,DeployFeeExceedsCap=14,InvalidTreasury=15}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleDeployed{pub creator:Address,pub circle_id:Address,pub name:String}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct CircleCreated{pub address:Address,pub admin:Address,pub token:Address,pub timestamp:u64}
#[contracttype]#[derive(Clone,Debug)]pub struct FeeConfigUpdated{pub old_fee_bps:i128,pub new_fee_bps:i128,pub updated_by:Address}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct DeployFeePaid{pub organizer:Address,pub treasury:Address,pub amount:i128,pub token:Address}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct DeployFeeConfigUpdated{pub old_fee:i128,pub new_fee:i128,pub treasury:Option<Address>}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct TemplateBounds{pub min_members:u32,pub max_members:u32,pub min_amount:i128,pub max_amount:i128,pub min_rounds:u32,pub max_rounds:u32}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct TemplateDefaults{pub contribution_deadline_seconds:u64,pub min_moi_score:u32,pub collateral_amount:i128,pub penalty_bps:u32,pub grace_period_seconds:u64,pub max_strikes:u32}
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct CircleTemplate{pub id:u32,pub name:String,pub contribution_amount:i128,pub max_members:u32,pub payout_type:u32,pub total_rounds:u32,pub bounds:TemplateBounds,pub defaults:TemplateDefaults}
#[contracttype]#[derive(Clone,Debug)]pub struct TemplateCreated{pub id:u32,pub name:String}
#[contracttype]#[derive(Clone,Debug)]pub struct TemplateDeployed{pub template_id:u32,pub circle_id:Address,pub creator:Address}
#[contracttype]#[derive(Clone,Debug)]pub struct MigrationBatchProgress{pub migrated:u32,pub start_index:u32}

