use crate::types::*;
use soroban_sdk::{symbol_short, Address, Env, Map, String};

const ADMIN_KEY: soroban_sdk::Symbol = symbol_short!("admin");
const META_KEY: soroban_sdk::Symbol = symbol_short!("meta");
const BALANCES_KEY: soroban_sdk::Symbol = symbol_short!("bal");
const ALLOWANCES_KEY: soroban_sdk::Symbol = symbol_short!("alw");
const TOTAL_KEY: soroban_sdk::Symbol = symbol_short!("total");
const FROZEN_KEY: soroban_sdk::Symbol = symbol_short!("frz");
const ALLOWLIST_MODE_KEY: soroban_sdk::Symbol = symbol_short!("alw_mode");
const ALLOWLIST_KEY: soroban_sdk::Symbol = symbol_short!("alw_list");

pub fn initialize(
    env: &Env,
    admin: &Address,
    name: &String,
    symbol: &String,
    decimals: u32,
) -> Result<(), TokenError> {
    if env.storage().instance().has(&ADMIN_KEY) {
        return Err(TokenError::NotInitialized);
    }
    env.storage().instance().set(&ADMIN_KEY, admin);
    env.storage().instance().set(
        &META_KEY,
        &TokenMetadata {
            name: name.clone(),
            symbol: symbol.clone(),
            decimals,
        },
    );
    env.storage().instance().set(&TOTAL_KEY, &0i128);
    env.storage()
        .persistent()
        .set(&BALANCES_KEY, &Map::<Address, i128>::new(env));
    env.storage()
        .persistent()
        .set(&ALLOWANCES_KEY, &Map::<(Address, Address), AllowanceData>::new(env));
    env.storage()
        .persistent()
        .set(&FROZEN_KEY, &Map::<Address, bool>::new(env));
    Ok(())
}

pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) -> Result<(), TokenError> {
    from.require_auth();
    if to == &env.current_contract_address() {
        return Err(TokenError::CannotTransferToSelf);
    }
    validate_non_frozen(env, from)?;
    validate_non_frozen(env, to)?;
    validate_allowlisted(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let from_balance: i128 = balances.get(from.clone()).unwrap_or(0);
    if from_balance < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(
        from.clone(),
        from_balance
            .checked_sub(amount)
            .ok_or(TokenError::Underflow)?,
    );
    let to_balance: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), to_balance.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    Transfer {
        from: from.clone(),
        to: to.clone(),
        amount,
    }
    .publish(env);
    Ok(())
}

pub fn transfer_from(
    env: &Env,
    spender: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), TokenError> {
    spender.require_auth();
    if to == &env.current_contract_address() {
        return Err(TokenError::CannotTransferToSelf);
    }
    validate_non_frozen(env, from)?;
    validate_non_frozen(env, to)?;
    validate_allowlisted(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let allowances: Map<(Address, Address), AllowanceData> = env
        .storage()
        .persistent()
        .get(&ALLOWANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let key = (from.clone(), spender.clone());
    let allowance: AllowanceData = allowances
        .get(key.clone())
        .ok_or(TokenError::Unauthorized)?;
    let current_ledger: u32 = env.ledger().sequence();
    if allowance.expiration_ledger != 0 && current_ledger > allowance.expiration_ledger {
        return Err(TokenError::AllowanceExpired);
    }
    if allowance.amount < amount {
        return Err(TokenError::AllowanceExceeded);
    }
    let mut allowances_mut = allowances;
    let new_allowance = allowance
        .amount
        .checked_sub(amount)
        .ok_or(TokenError::Underflow)?;
    allowances_mut.set(
        key,
        AllowanceData {
            amount: new_allowance,
            expiration_ledger: allowance.expiration_ledger,
        },
    );
    env.storage()
        .persistent()
        .set(&ALLOWANCES_KEY, &allowances_mut);
    let mut balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let from_balance: i128 = balances.get(from.clone()).unwrap_or(0);
    if from_balance < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(
        from.clone(),
        from_balance
            .checked_sub(amount)
            .ok_or(TokenError::Underflow)?,
    );
    let to_balance: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), to_balance.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    Transfer {
        from: from.clone(),
        to: to.clone(),
        amount,
    }
    .publish(env);
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
    let mut allowances: Map<(Address, Address), AllowanceData> = env
        .storage()
        .persistent()
        .get(&ALLOWANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let key = (owner.clone(), spender.clone());
    allowances.set(
        key,
        AllowanceData {
            amount,
            expiration_ledger,
        },
    );
    env.storage().persistent().set(&ALLOWANCES_KEY, &allowances);
    Approve {
        owner: owner.clone(),
        spender: spender.clone(),
        amount,
        expiration_ledger,
    }
    .publish(env);
    Ok(())
}

pub fn balance(env: &Env, account: &Address) -> i128 {
    let balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .unwrap_or_else(|| Map::new(env));
    balances.get(account.clone()).unwrap_or(0)
}

pub fn allowance(env: &Env, owner: &Address, spender: &Address) -> AllowanceData {
    let allowances: Map<(Address, Address), AllowanceData> = env
        .storage()
        .persistent()
        .get(&ALLOWANCES_KEY)
        .unwrap_or_else(|| Map::new(env));
    let key = (owner.clone(), spender.clone());
    allowances.get(key).unwrap_or(AllowanceData {
        amount: 0,
        expiration_ledger: 0,
    })
}

pub fn total_supply(env: &Env) -> i128 {
    env.storage().instance().get(&TOTAL_KEY).unwrap_or(0)
}

pub fn name(env: &Env) -> String {
    let meta: TokenMetadata = env
        .storage()
        .instance()
        .get(&META_KEY)
        .unwrap_or(TokenMetadata {
            name: String::from_str(env, ""),
            symbol: String::from_str(env, ""),
            decimals: 0,
        });
    meta.name
}

pub fn symbol(env: &Env) -> String {
    let meta: TokenMetadata = env
        .storage()
        .instance()
        .get(&META_KEY)
        .unwrap_or(TokenMetadata {
            name: String::from_str(env, ""),
            symbol: String::from_str(env, ""),
            decimals: 0,
        });
    meta.symbol
}

pub fn decimals(env: &Env) -> u32 {
    let meta: TokenMetadata = env
        .storage()
        .instance()
        .get(&META_KEY)
        .unwrap_or(TokenMetadata {
            name: String::from_str(env, ""),
            symbol: String::from_str(env, ""),
            decimals: 0,
        });
    meta.decimals
}

pub fn mint(env: &Env, admin: &Address, to: &Address, amount: i128) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    if to == &env.current_contract_address() {
        return Err(TokenError::CannotTransferToSelf);
    }
    validate_non_frozen(env, to)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(to.clone()).unwrap_or(0);
    balances.set(to.clone(), current.checked_add(amount).ok_or(TokenError::Overflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    env.storage()
        .instance()
        .set(&TOTAL_KEY, &total.checked_add(amount).ok_or(TokenError::Overflow)?);
    Mint {
        to: to.clone(),
        amount,
    }
    .publish(env);
    Ok(())
}

pub fn burn(env: &Env, from: &Address, amount: i128) -> Result<(), TokenError> {
    from.require_auth();
    validate_non_frozen(env, from)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(from.clone()).unwrap_or(0);
    if current < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(from.clone(), current.checked_sub(amount).ok_or(TokenError::Underflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    env.storage()
        .instance()
        .set(&TOTAL_KEY, &total.checked_sub(amount).ok_or(TokenError::Underflow)?);
    Burn {
        from: from.clone(),
        amount,
    }
    .publish(env);
    Ok(())
}

pub fn clawback(
    env: &Env,
    admin: &Address,
    from: &Address,
    amount: i128,
) -> Result<(), TokenError> {
/// #518 — Admin-authorized burn (distinct from holder burn).
///
/// Allows admin to burn tokens from any account. Emits `Burn` event.
/// Requires admin authorization and validates total supply invariant.
pub fn burn_from(env: &Env, admin: &Address, from: &Address, amount: i128) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
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
    
    // Invariant: total supply decreases by exactly burned amount
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    let new_total = total.checked_sub(amount).ok_or(TokenError::Underflow)?;
    env.storage().instance().set(&TOTAL_KEY, &new_total);
    
    Burn { from: from.clone(), amount }.publish(env);
    Ok(())
}

/// Claws back tokens from an account balance and reduces total supply.
///
/// # Intentional Design: Frozen Account Bypass
/// Clawback is an administrative action intended for regulatory compliance, fraud
/// mitigation, and legal enforcement (aligning with Stellar SEP-0041 asset standards).
/// An administrator is intentionally permitted to claw back tokens from an account even
/// if that account is frozen. Requiring unfreezing prior to clawback would create a race
/// condition allowing unauthorized outbound transfers before the clawback completes.
/// Regular user operations (`transfer`, `burn`) strictly enforce non-frozen checks.
pub fn clawback(env: &Env, admin: &Address, from: &Address, amount: i128) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    if amount <= 0 {
        return Err(TokenError::InvalidAmount);
    }
    let mut balances: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&BALANCES_KEY)
        .ok_or(TokenError::NotInitialized)?;
    let current: i128 = balances.get(from.clone()).unwrap_or(0);
    if current < amount {
        return Err(TokenError::InsufficientBalance);
    }
    balances.set(from.clone(), current.checked_sub(amount).ok_or(TokenError::Underflow)?);
    env.storage().persistent().set(&BALANCES_KEY, &balances);
    let total: i128 = env.storage().instance().get(&TOTAL_KEY).unwrap_or(0);
    env.storage()
        .instance()
        .set(&TOTAL_KEY, &total.checked_sub(amount).ok_or(TokenError::Underflow)?);
    Clawback {
        from: from.clone(),
        amount,
    }
    .publish(env);
    Ok(())
}

pub fn freeze(env: &Env, admin: &Address, account: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut frozen: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&FROZEN_KEY)
        .ok_or(TokenError::NotInitialized)?;
    frozen.set(account.clone(), true);
    env.storage().persistent().set(&FROZEN_KEY, &frozen);
    AccountFrozen {
        account: account.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn unfreeze(env: &Env, admin: &Address, account: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut frozen: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&FROZEN_KEY)
        .ok_or(TokenError::NotInitialized)?;
    frozen.set(account.clone(), false);
    env.storage().persistent().set(&FROZEN_KEY, &frozen);
    AccountUnfrozen {
        account: account.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn is_frozen(env: &Env, account: &Address) -> bool {
    let frozen: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&FROZEN_KEY)
        .unwrap_or_else(|| Map::new(env));
    frozen.get(account.clone()).unwrap_or(false)
}

pub fn set_admin(env: &Env, admin: &Address, new_admin: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    env.storage().instance().set(&ADMIN_KEY, new_admin);
    Ok(())
}

pub fn get_admin(env: &Env) -> Result<Address, TokenError> {
    env.storage()
        .instance()
        .get(&ADMIN_KEY)
        .ok_or(TokenError::NotInitialized)
}

fn require_admin(env: &Env, caller: &Address) -> Result<(), TokenError> {
    let stored: Address = env
        .storage()
        .instance()
        .get(&ADMIN_KEY)
        .ok_or(TokenError::NotInitialized)?;
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

/// Enable or disable allowlist mode for transfer recipients (admin-only).
///
/// When enabled, `transfer` and `transfer_from` reject any transfer whose
/// recipient is not on the allowlist (see `add_to_allowlist`). Default is
/// off. Emits `AllowlistModeChanged`.
///
/// # Authorization
/// Requires `admin.require_auth()` and `admin` must be the stored admin.
pub fn set_allowlist_mode(env: &Env, admin: &Address, enabled: bool) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    env.storage().instance().set(&ALLOWLIST_MODE_KEY, &enabled);
    AllowlistModeChanged { enabled }.publish(env);
    Ok(())
}

/// Returns whether allowlist mode is currently enabled. Defaults to `false`
/// when never toggled.
pub fn is_allowlist_mode_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&ALLOWLIST_MODE_KEY)
        .unwrap_or(false)
}

/// Add `account` to the recipient allowlist (admin-only). Emits `AllowlistAdded`.
///
/// # Authorization
/// Requires `admin.require_auth()` and `admin` must be the stored admin.
pub fn add_to_allowlist(env: &Env, admin: &Address, account: &Address) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut list: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&ALLOWLIST_KEY)
        .unwrap_or_else(|| Map::new(env));
    list.set(account.clone(), true);
    env.storage().persistent().set(&ALLOWLIST_KEY, &list);
    AllowlistAdded {
        account: account.clone(),
    }
    .publish(env);
    Ok(())
}

/// Remove `account` from the recipient allowlist (admin-only). Emits `AllowlistRemoved`.
///
/// # Authorization
/// Requires `admin.require_auth()` and `admin` must be the stored admin.
pub fn remove_from_allowlist(
    env: &Env,
    admin: &Address,
    account: &Address,
) -> Result<(), TokenError> {
    admin.require_auth();
    require_admin(env, admin)?;
    let mut list: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&ALLOWLIST_KEY)
        .unwrap_or_else(|| Map::new(env));
    list.set(account.clone(), false);
    env.storage().persistent().set(&ALLOWLIST_KEY, &list);
    AllowlistRemoved {
        account: account.clone(),
    }
    .publish(env);
    Ok(())
}

/// Returns whether `account` is currently on the recipient allowlist.
pub fn is_allowlisted(env: &Env, account: &Address) -> bool {
    let list: Map<Address, bool> = env
        .storage()
        .persistent()
        .get(&ALLOWLIST_KEY)
        .unwrap_or_else(|| Map::new(env));
    list.get(account.clone()).unwrap_or(false)
}

/// When allowlist mode is enabled, reject transfers to a non-allowlisted `to`.
fn validate_allowlisted(env: &Env, to: &Address) -> Result<(), TokenError> {
    if is_allowlist_mode_enabled(env) && !is_allowlisted(env, to) {
        return Err(TokenError::RecipientNotAllowlisted);
    }
    Ok(())
}
