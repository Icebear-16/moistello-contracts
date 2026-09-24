use soroban_sdk::{symbol_short, Address, Env, Map, String};
use crate::types::*;

const ADMIN_KEY: soroban_sdk::Symbol = symbol_short!("admin");
const META_KEY: soroban_sdk::Symbol = symbol_short!("meta");
const BALANCES_KEY: soroban_sdk::Symbol = symbol_short!("bal");
const ALLOWANCES_KEY: soroban_sdk::Symbol = symbol_short!("alw");
const TOTAL_KEY: soroban_sdk::Symbol = symbol_short!("total");
const FROZEN_KEY: soroban_sdk::Symbol = symbol_short!("frz");

pub fn initialize(
    env: &Env,
    admin: &Address,
    name: &String,
    symbol: &String,
    decimals: u32,
) -> Result<(), TokenError> {
    if env.storage().instance().has(&ADMIN_KEY) {
        return Err(TokenError::AlreadyInitialized);
    }
    env.storage().instance().set(&ADMIN_KEY, admin);
    env.storage().instance().set(&META_KEY, &TokenMetadata {
        name: name.clone(),
        symbol: symbol.clone(),
        decimals,
    });
    env.storage().instance().set(&TOTAL_KEY, &0i128);
    env.storage().persistent().set(&BALANCES_KEY, &Map::<Address, i128>::new(env));
    env.storage().persistent().set(&ALLOWANCES_KEY, &Map::<(Address, Address), AllowanceData>::new(env));
    env.storage().persistent().set(&FROZEN_KEY, &Map::<Address, bool>::new(env));
    Ok(())
}

pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) -> Result<(), TokenError> {
    validate_not_paused(env)?;
    from.require_auth();
    validate_non_frozen(env, from)?;
    validate_non_frozen(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let from_balance: i128 = balances.get(from.clone()).unwrap_or(0);
    if from_balance < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(from.clone(), from_balance.checked_sub(amount).ok_or(TokenError::Underflow)?);
    let to_balance: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), to_balance.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    Transfer { from: from.clone(), to: to.clone(), amount }.publish(env);
    Ok(())
}

fn validate_not_paused(env: &Env) -> Result<(), TokenError> {
    let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
    if paused {
        return Err(TokenError::Paused);
    }
    Ok(())
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    env.storage().instance().set(&DataKey::Paused, &true);
    Ok(())
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    env.storage().instance().set(&DataKey::Paused, &false);
    Ok(())
}

pub fn transfer_from(
    env: &Env,
    spender: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), TokenError> {
    validate_not_paused(env)?;
    spender.require_auth();
    validate_non_frozen(env, from)?;
    validate_non_frozen(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let allowances: Map<(Address, Address), AllowanceData> = env.storage().persistent().get(&ALLOWANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let key = (from.clone(), spender.clone());
    let allowance: AllowanceData = allowances.get(key.clone()).ok_or(TokenError::Unauthorized)?;
    let current_ledger: u32 = env.ledger().sequence();
    if allowance.expiration_ledger != 0 && current_ledger > allowance.expiration_ledger {
        return Err(TokenError::AllowanceExpired);
    }
    if allowance.amount < amount {
        return Err(TokenError::AllowanceExceeded);
    }
    let mut balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let from_balance: i128 = balances.get(from.clone()).unwrap_or(0);
    if from_balance < amount {
        return Err(TokenError::InsufficientBalance);
    }
    let mut allowances_mut = allowances;
    let new_allowance = allowance.amount.checked_sub(amount).ok_or(TokenError::Underflow)?;
    allowances_mut.set(key, AllowanceData {
        amount: new_allowance,
        expiration_ledger: allowance.expiration_ledger,
    });
    env.storage().persistent().set(&ALLOWANCES_KEY, &allowances_mut);
    balances.set(from.clone(), from_balance.checked_sub(amount).ok_or(TokenError::Underflow)?);
    let to_balance: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), to_balance.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    Transfer { from: from.clone(), to: to.clone(), amount }.publish(env);
    Ok(())
}

pub fn approve(
    env: &Env,
    owner: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) -> Result<(), TokenError> {
    owner.require_auth();
    // Reject negative amounts and zero amounts (#257)
    // Zero-approval wastes storage; use it only to revoke allowances
    if amount < 0 {
        return Err(TokenError::InvalidAmount);
    }
    if amount == 0 && expiration_ledger > 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut allowances: Map<(Address, Address), AllowanceData> = env.storage().persistent().get(&ALLOWANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let key = (owner.clone(), spender.clone());
    allowances.set(key, AllowanceData { amount, expiration_ledger });
    env.storage().persistent().set(&ALLOWANCES_KEY, &allowances);
    Approve { owner: owner.clone(), spender: spender.clone(), amount, expiration_ledger }.publish(env);
    Ok(())
}

pub fn balance(env: &Env, account: &Address) -> i128 {
    let balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).unwrap_or_else(|| Map::new(env));
    balances.get(account.clone()).unwrap_or(0)
}

pub fn allowance(env: &Env, owner: &Address, spender: &Address) -> AllowanceData {
    let allowances: Map<(Address, Address), AllowanceData> = env.storage().persistent().get(&ALLOWANCES_KEY).unwrap_or_else(|| Map::new(env));
    let key = (owner.clone(), spender.clone());
    allowances.get(key).unwrap_or(AllowanceData { amount: 0, expiration_ledger: 0 })
}

pub fn total_supply(env: &Env) -> i128 {
    env.storage().instance().get(&TOTAL_KEY).unwrap_or(0)
}

pub fn name(env: &Env) -> String {
    let meta: TokenMetadata = env.storage().instance().get(&META_KEY).unwrap_or(TokenMetadata {
        name: String::from_str(env, ""),
        symbol: String::from_str(env, ""),
        decimals: 0,
    });
    meta.name
}

pub fn symbol(env: &Env) -> String {
    let meta: TokenMetadata = env.storage().instance().get(&META_KEY).unwrap_or(TokenMetadata {
        name: String::from_str(env, ""),
        symbol: String::from_str(env, ""),
        decimals: 0,
    });
    meta.symbol
}

pub fn decimals(env: &Env) -> u32 {
    let meta: TokenMetadata = env.storage().instance().get(&META_KEY).unwrap_or(TokenMetadata {
        name: String::from_str(env, ""),
        symbol: String::from_str(env, ""),
        decimals: 0,
    });
    meta.decimals
}

pub fn mint(env: &Env, admin: &Address, to: &Address, amount: i128) -> Result<(), TokenError> {
    validate_not_paused(env)?;
    admin.require_auth();
    require_admin(env, admin)?;
    validate_non_frozen(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), current.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    let new_total = total.checked_add(amount).ok_or(TokenError::Overflow)?;
    if new_total > 1_000_000_000 {
        return Err(TokenError::MaxSupplyExceeded);
    }
    env.storage().instance().set(&TOTAL_KEY, &total.checked_add(amount).ok_or(TokenError::Overflow)?);
    Mint { to: to.clone(), amount }.publish(env);
    Ok(())
}

pub fn burn(env: &Env, from: &Address, amount: i128) -> Result<(), TokenError> {
    validate_not_paused(env)?;
    from.require_auth();
    validate_non_frozen(env, from)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(from.clone()).unwrap_or(0);
    if current < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(from.clone(), current.checked_sub(amount).ok_or(TokenError::Underflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    let new_total = total.checked_add(amount).ok_or(TokenError::Overflow)?;
    if new_total > 1_000_000_000 {
        return Err(TokenError::MaxSupplyExceeded);
    }
    env.storage().instance().set(&TOTAL_KEY, &total.checked_sub(amount).ok_or(TokenError::Underflow)?);
    Burn { from: from.clone(), amount }.publish(env);
    Ok(())
}

pub fn clawback(env: &Env, admin: &Address, from: &Address, amount: i128) -> Result<(), TokenError> {
    validate_not_paused(env)?;
    admin.require_auth();
    require_admin(env, admin)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env.storage().persistent().get(&BALANCES_KEY).ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(from.clone()).unwrap_or(0);
    if current < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(from.clone(), current.checked_sub(amount).ok_or(TokenError::Underflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    let new_total = total.checked_add(amount).ok_or(TokenError::Overflow)?;
    if new_total > 1_000_000_000 {
        return Err(TokenError::MaxSupplyExceeded);
    }
    env.storage().instance().set(&TOTAL_KEY, &total.checked_sub(amount).ok_or(TokenError::Underflow)?);
    Clawback { from: from.clone(), amount }.publish(env);
    Ok(())
}

pub fn freeze(env: &Env, admin: &Address, account: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut frozen: Map<Address, bool> = env.storage().persistent().get(&FROZEN_KEY).ok_or(TokenError::NotInitialized)?;
    frozen.set(account.clone(), true);
    env.storage().persistent().set(&FROZEN_KEY, &frozen);
    Freeze { account: account.clone() }.publish(env);
    Ok(())
}

pub fn unfreeze(env: &Env, admin: &Address, account: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut frozen: Map<Address, bool> = env.storage().persistent().get(&FROZEN_KEY).ok_or(TokenError::NotInitialized)?;
    frozen.set(account.clone(), false);
    env.storage().persistent().set(&FROZEN_KEY, &frozen);
    Unfreeze { account: account.clone() }.publish(env);
    Ok(())
}

pub fn is_frozen(env: &Env, account: &Address) -> bool {
    let frozen: Map<Address, bool> = env.storage().persistent().get(&FROZEN_KEY).unwrap_or_else(|| Map::new(env));
    frozen.get(account.clone()).unwrap_or(false)
}

pub fn set_admin(env: &Env, admin: &Address, new_admin: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let old_admin = env.storage().instance().get(&ADMIN_KEY).unwrap_or(admin.clone());
    env.storage().instance().set(&ADMIN_KEY, new_admin);
    AdminChanged { old_admin, new_admin: new_admin.clone() }.publish(env);
    Ok(())
}

pub fn get_admin(env: &Env) -> Result<Address, TokenError> {
    env.storage().instance().get(&ADMIN_KEY).ok_or(TokenError::NotInitialized)
}

fn require_admin(env: &Env, caller: &Address) -> Result<(), TokenError> {
    let stored: Address = env.storage().instance().get(&ADMIN_KEY).ok_or(TokenError::NotInitialized)?;
    if caller != &stored {
        return Err(TokenError::Unauthorized);
    }
    Ok(())
}

fn validate_non_frozen(env: &Env, account: &Address) -> Result<(), TokenError> {
    if is_frozen(env, account) {
        return Err(TokenError::Frozen);
    }
    Ok(())
}
