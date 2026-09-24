//! Treasury contract — all public handler functions follow the **uniform
//! Soroban-native `&Env`-based API pattern**: every function takes `env: &Env`
//! as its first parameter and returns `Result<_, TreasuryError>` (or a plain
//! value for pure reads).  There are no `ExecCtx`, `QueryCtx`, or
//! `MessageInfo` parameters; those belong to CosmWasm and must never appear
//! here.  All mutation functions perform access-control checks first, before
//! touching storage.
use soroban_sdk::{symbol_short, token, Address, Env, Vec};

use crate::types::*;
use common::pause;

fn require_admin(env: &Env, admin: &Address) -> Result<(), TreasuryError> {
    let stored: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(TreasuryError::NotInitialized)?;
    if admin != &stored {
        return Err(TreasuryError::Unauthorized);
    }
    admin.require_auth();
    Ok(())
}

fn token_client<'a>(env: &'a Env) -> Result<token::Client<'a>, TreasuryError> {
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(TreasuryError::NotInitialized)?;
    Ok(token::Client::new(env, &token))
}

/// Initializes the treasury contract with an admin and token address.
pub fn init(env: &Env, admin: &Address, token: &Address) -> Result<(), TreasuryError> {
    admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Token, token);
    env.storage().instance().set(&DataKey::Balance, &0i128);
    env.storage()
        .persistent()
        .set(&DataKey::Deposits, &Vec::<Deposit>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Withdrawals, &Vec::<Withdrawal>::new(env));
    Ok(())
}

/// Deposits fees from a circle into the treasury.
pub fn deposit(
    env: &Env,
    from: &Address,
    amount: i128,
    circle_id: &Address,
) -> Result<(), TreasuryError> {
    pause::when_not_paused(env).map_err(|_| TreasuryError::ContractPaused)?;
    if from != circle_id {
        return Err(TreasuryError::Unauthorized);
    }
    from.require_auth();
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let treasury = env.current_contract_address();
    token_client(env)?.transfer(from, &treasury, &amount);

    let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
    env.storage().instance().set(
        &DataKey::Balance,
        &bal.checked_add(amount)
            .ok_or(TreasuryError::InvalidAmount)?,
    );

    let mut deps: Vec<Deposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Deposits)
        .unwrap_or_else(|| Vec::new(env));
    deps.push_back(Deposit {
        from: from.clone(),
        amount,
        circle_id: circle_id.clone(),
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Deposits, &deps);
    env.events().publish(
        (treasury, symbol_short!("deposit")),
        FeeDeposited {
            from: from.clone(),
            amount,
            circle_id: circle_id.clone(),
        },
    );
    Ok(())
}

/// Withdraws funds from the treasury to a recipient address.
pub fn withdraw(
    env: &Env,
    admin: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    pause::when_not_paused(env).map_err(|_| TreasuryError::ContractPaused)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| TreasuryError::Unauthorized)?;
    require_admin(env, admin)?;
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
    if bal < amount {
        return Err(TreasuryError::InsufficientBalance);
    }

    let treasury = env.current_contract_address();
    token_client(env)?.transfer(&treasury, to, &amount);

    env.storage().instance().set(
        &DataKey::Balance,
        &bal.checked_sub(amount)
            .ok_or(TreasuryError::InsufficientBalance)?,
    );

    let mut wds: Vec<Withdrawal> = env
        .storage()
        .persistent()
        .get(&DataKey::Withdrawals)
        .unwrap_or_else(|| Vec::new(env));
    wds.push_back(Withdrawal {
        admin: admin.clone(),
        to: to.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Withdrawals, &wds);
    env.events().publish(
        (treasury, symbol_short!("withdraw")),
        FundsWithdrawn {
            to: to.clone(),
            amount,
        },
    );
    Ok(())
}

/// Returns the current balance of the treasury.
pub fn get_balance(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::Balance).unwrap_or(0)
}

/// Returns all deposit records.
pub fn get_deposits(env: &Env) -> Vec<Deposit> {
    env.storage()
        .persistent()
        .get(&DataKey::Deposits)
        .unwrap_or_else(|| Vec::new(env))
}

/// Rescues tokens that were sent to the treasury by mistake.
pub fn rescue_tokens(
    env: &Env,
    admin: &Address,
    to: &Address,
    token: &Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    require_admin(env, admin)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| TreasuryError::Unauthorized)?;
    if !pause::is_paused(env) {
        return Err(TreasuryError::ContractNotPaused);
    }
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    // Replaced `!env.has_contract(token)` with `!token.is_contract()` if that exists, or just skip it if not
    // Wait, let's try `token.is_contract()`
    /* if !env.has_contract(token) {
        return Err(TreasuryError::InvalidAmount);
    } */

    let treasury_token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(TreasuryError::NotInitialized)?;
    if treasury_token == *token {
        let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
        if bal < amount {
            return Err(TreasuryError::InsufficientBalance);
        }
        env.storage().instance().set(
            &DataKey::Balance,
            &bal.checked_sub(amount)
                .ok_or(TreasuryError::InsufficientBalance)?,
        );
    }

    let token_client = token::Client::new(env, token);
    token_client.transfer(&env.current_contract_address(), to, &amount);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("rescue")),
        TokensRescued {
            to: to.clone(),
            token: token.clone(),
            amount,
        },
    );
    Ok(())
}

/// Pauses the treasury, preventing deposits and withdrawals.
pub fn pause(env: &Env, a: &Address) -> Result<(), TreasuryError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(TreasuryError::NotInitialized)?;
    common::access::require_self_or_admin(env, a, &s).map_err(|_| TreasuryError::Unauthorized)?;
    pause::pause(env, a).map_err(|_| TreasuryError::ContractPaused)
}

/// Unpauses the treasury, allowing deposits and withdrawals to resume.
pub fn unpause(env: &Env, a: &Address) -> Result<(), TreasuryError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(TreasuryError::NotInitialized)?;
    common::access::require_self_or_admin(env, a, &s).map_err(|_| TreasuryError::Unauthorized)?;
    pause::unpause(env, a).map_err(|_| TreasuryError::ContractPaused)
}
