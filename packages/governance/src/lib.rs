#![cfg_attr(not(test), no_std)]
mod contract;
#[cfg(test)]
mod test;
mod types;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

#[contract]
pub struct Governance;
#[contractimpl]
impl Governance {
    pub fn init(
        env: Env,
        admin: Address,
        config: types::GovernanceConfig,
    ) -> Result<(), types::GovernanceError> {
        contract::init(&env, &admin, &config)
    }
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        deposit_amount: i128,
        action: types::ProposalAction,
        description: BytesN<32>,
    ) -> Result<u64, types::GovernanceError> {
        contract::create_proposal(&env, &proposer, deposit_amount, action, description)
    }
    pub fn cast_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        vote: types::VoteType,
    ) -> Result<(), types::GovernanceError> {
        contract::cast_vote(&env, &voter, proposal_id, vote)
    }
    pub fn finalize_proposal(env: Env, proposal_id: u64) -> Result<(), types::GovernanceError> {
        contract::finalize_proposal(&env, proposal_id)
    }
    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), types::GovernanceError> {
        contract::execute_proposal(&env, proposal_id)
    }
    pub fn cancel_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) -> Result<(), types::GovernanceError> {
        contract::cancel_proposal(&env, &caller, proposal_id)
    }
    pub fn expire_proposal(env: Env, proposal_id: u64) -> Result<(), types::GovernanceError> {
        contract::expire_proposal(&env, proposal_id)
    }
    pub fn queue_config_update(
        env: Env,
        admin: Address,
        new_config: types::GovernanceConfig,
    ) -> Result<(), types::GovernanceError> {
        contract::queue_config_update(&env, &admin, new_config)
    }
    pub fn execute_config_update(env: Env) -> Result<(), types::GovernanceError> {
        contract::execute_config_update(&env)
    }
    pub fn cancel_config_update(env: Env, admin: Address) -> Result<(), types::GovernanceError> {
        contract::cancel_config_update(&env, &admin)
    }
    pub fn get_pending_config_update(env: Env) -> Option<types::PendingConfigUpdate> {
        contract::get_pending_config_update(&env)
    }
    pub fn get_proposal(env: Env, id: u64) -> Result<types::Proposal, types::GovernanceError> {
        contract::get_proposal(&env, id)
    }
    pub fn get_proposals(
        env: Env,
        status: types::ProposalStatus,
        limit: u32,
    ) -> Vec<types::Proposal> {
        contract::get_proposals(&env, status, limit)
    }
    pub fn get_proposal_metadata_page(
        env: Env,
        cursor: u64,
        limit: u32,
    ) -> types::ProposalMetadataPage {
        contract::get_proposal_metadata_page(&env, cursor, limit)
    }
    pub fn get_vote(env: Env, proposal_id: u64, voter: Address) -> Option<types::VoteRecord> {
        contract::get_vote(&env, proposal_id, &voter)
    }
    pub fn get_vote_power(env: Env, voter: Address) -> i128 {
        contract::get_vote_power(&env, &voter)
    }
    pub fn set_staking_contract(
        env: Env,
        admin: Address,
        staking: Address,
    ) -> Result<(), types::GovernanceError> {
        contract::set_staking_contract(&env, &admin, &staking)
    }
    pub fn get_config(env: Env) -> Result<types::GovernanceConfig, types::GovernanceError> {
        contract::get_config(&env)
    }
    pub fn pause(env: Env, admin: Address) -> Result<(), types::GovernanceError> {
        contract::pause(&env, &admin)
    }
    pub fn unpause(env: Env, admin: Address) -> Result<(), types::GovernanceError> {
        contract::unpause(&env, &admin)
    }
    pub fn delegate(
        env: Env,
        delegator: Address,
        delegatee: Address,
    ) -> Result<(), types::GovernanceError> {
        contract::delegate(&env, &delegator, &delegatee)
    }
    pub fn revoke_delegation(env: Env, delegator: Address) -> Result<(), types::GovernanceError> {
        contract::revoke_delegation(&env, &delegator)
    }
    pub fn get_deposit(env: Env, id: u64) -> Option<i128> {
        contract::get_deposit(&env, id)
    }
