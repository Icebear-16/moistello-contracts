#![cfg_attr(not(test), no_std)]
mod contract;
#[cfg(test)]
mod test;
mod types;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
#[contract]
pub struct EscrowSwap;
#[contractimpl]
impl EscrowSwap {
    pub fn __constructor(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::init(&env, &admin)
    }
    pub fn create_swap(
        env: Env,
        initiator: Address,
        responder: Address,
        token_a: Address,
        token_b: Address,
        initiator_amount: i128,
        responder_amount: i128,
        hash_lock: BytesN<32>,
        time_lock: u64,
    ) -> Result<u64, types::EscrowError> {
        contract::create_swap(
            &env,
            &initiator,
            &responder,
            &token_a,
            &token_b,
            initiator_amount,
            responder_amount,
            hash_lock,
            time_lock,
        )
    }
    pub fn accept_swap(
        env: Env,
        id: u64,
        responder: Address,
        secret: BytesN<32>,
    ) -> Result<(), types::EscrowError> {
        contract::accept_swap(&env, id, &responder, secret)
    }
    pub fn complete_swap(env: Env, id: u64, caller: Address) -> Result<(), types::EscrowError> {
        contract::complete_swap(&env, id, &caller)
    }
    pub fn cancel_swap(env: Env, id: u64, caller: Address) -> Result<(), types::EscrowError> {
        contract::cancel_swap(&env, id, &caller)
    }
    pub fn get_swap(env: Env, id: u64) -> Result<types::SwapRequest, types::EscrowError> {
        contract::get_swap(&env, id)
    }
    pub fn get_swaps(env: Env) -> soroban_sdk::Vec<types::SwapRequest> {
        contract::get_swaps(&env)
    }
    pub fn pause(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::pause(&env, &admin)
    }
    pub fn unpause(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::unpause(&env, &admin)
    }
}
