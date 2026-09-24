use soroban_sdk::{contracttype, contracterror, Address, String, Vec};
// Re-export canonical CircleConfig from common to avoid duplicate type definitions (#320).
pub use common::types::CircleConfig;
#[contracttype]#[derive(Clone,Debug,PartialEq)]pub struct FeeConfig{pub fee_bps:i128,pub updated_at:u64,pub updated_by:Address}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleEntry{pub circle_id:Address,pub name:String,pub organizer:Address,pub deployed_at:u64,pub status:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleRegistry{pub circles:Vec<CircleEntry>}
#[contracttype]#[derive(Clone)]pub enum DataKey{Admin,FeeConfig,CircleList,CircleCount,WasmHash,CircleConfig(Address),Slug(String)}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]pub enum FactoryError{NotInitialized=1,Unauthorized=2,ContractPaused=3,WasmHashNotSet=4,InvalidFeeBps=5,CircleDeployFailed=6,InvalidConfig=7,DuplicateSlug=8,EmptySlug=9}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleDeployed{pub creator:Address,pub circle_id:Address,pub name:String}
#[contracttype]#[derive(Clone,Debug)]pub struct FeeConfigUpdated{pub old_fee_bps:i128,pub new_fee_bps:i128,pub updated_by:Address}
