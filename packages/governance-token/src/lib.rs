#![cfg_attr(not(test), no_std)]

mod contract;
#[cfg(test)]
mod test;
mod types;

use crate::types::{AllowanceData, TokenError};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

#[contract]
pub struct GovernanceToken;

#[contractimpl]
impl GovernanceToken {
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) -> Result<(), TokenError> {
        contract::initialize(&env, &admin, &name, &symbol, decimals)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        contract::transfer(&env, &from, &to, amount)
    }

    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        contract::transfer_from(&env, &spender, &from, &to, amount)
    }

    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), TokenError> {
        contract::approve(&env, &owner, &spender, amount, expiration_ledger)
    }

    pub fn balance(env: Env, account: Address) -> i128 {
        contract::balance(&env, &account)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> AllowanceData {
        contract::allowance(&env, &owner, &spender)
    }

    pub fn total_supply(env: Env) -> i128 {
        contract::total_supply(&env)
    }

    pub fn name(env: Env) -> String {
        contract::name(&env)
    }

    pub fn symbol(env: Env) -> String {
        contract::symbol(&env)
    }

    pub fn decimals(env: Env) -> u32 {
        contract::decimals(&env)
    }

    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        contract::mint(&env, &admin, &to, amount)
    }

    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), TokenError> {
        contract::burn(&env, &from, amount)
    }

    pub fn clawback(
        env: Env,
        admin: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
    /// #518 — Admin-authorized burn function
    pub fn burn_from(env: Env, admin: Address, from: Address, amount: i128) -> Result<(), TokenError> {
        contract::burn_from(&env, &admin, &from, amount)
    }

    pub fn clawback(env: Env, admin: Address, from: Address, amount: i128) -> Result<(), TokenError> {
        contract::clawback(&env, &admin, &from, amount)
    }

    pub fn freeze(env: Env, admin: Address, account: Address) -> Result<(), TokenError> {
        contract::freeze(&env, &admin, &account)
    }

    pub fn unfreeze(env: Env, admin: Address, account: Address) -> Result<(), TokenError> {
        contract::unfreeze(&env, &admin, &account)
    }

    pub fn is_frozen(env: Env, account: Address) -> bool {
        contract::is_frozen(&env, &account)
    }

    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), TokenError> {
        contract::set_admin(&env, &admin, &new_admin)
    }

    pub fn get_admin(env: Env) -> Result<Address, TokenError> {
        contract::get_admin(&env)
    }

    pub fn set_allowlist_mode(env: Env, admin: Address, enabled: bool) -> Result<(), TokenError> {
        contract::set_allowlist_mode(&env, &admin, enabled)
    }

    pub fn is_allowlist_mode_enabled(env: Env) -> bool {
        contract::is_allowlist_mode_enabled(&env)
    }

    pub fn add_to_allowlist(env: Env, admin: Address, account: Address) -> Result<(), TokenError> {
        contract::add_to_allowlist(&env, &admin, &account)
    }

    pub fn remove_from_allowlist(
        env: Env,
        admin: Address,
        account: Address,
    ) -> Result<(), TokenError> {
        contract::remove_from_allowlist(&env, &admin, &account)
    }

    pub fn is_allowlisted(env: Env, account: Address) -> bool {
        contract::is_allowlisted(&env, &account)
    }
}
