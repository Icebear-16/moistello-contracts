use crate::analytics;
use crate::types::*;
use soroban_sdk::{symbol_short, Address, Env};

pub const STORAGE_VERSION: u32 = 2;

pub fn current_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::StorageVersion)
        .unwrap_or(1)
}

pub fn init_current_version(env: &Env) {
    env.storage()
        .instance()
        .set(&DataKey::StorageVersion, &STORAGE_VERSION);
}

pub fn migrate(env: &Env, caller: &Address) -> Result<(), CircleError> {
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    let stored_factory: Address = env
        .storage()
        .instance()
        .get(&DataKey::Factory)
        .ok_or(CircleError::NotInitialized)?;
    if caller != &stored_admin && caller != &stored_factory {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    let from = current_version(env);
    if from > STORAGE_VERSION {
        return Err(CircleError::IncompatibleStorageVersion);
    }
    if from == STORAGE_VERSION {
        return Ok(());
    }
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    analytics::recompute(env, &circle)?;
    env.storage()
        .instance()
        .set(&DataKey::StorageVersion, &STORAGE_VERSION);
    analytics::validate(env)?;
    env.events().publish(
        (env.current_contract_address(), symbol_short!("migrat")),
        MigrationApplied {
            from_version: from,
            to_version: STORAGE_VERSION,
        },
    );
    Ok(())
}

pub fn verify(env: &Env) -> Result<(), CircleError> {
    let version = current_version(env);
    if version < STORAGE_VERSION {
        return Err(CircleError::MigrationPending);
    }
    if version > STORAGE_VERSION {
        return Err(CircleError::IncompatibleStorageVersion);
    }
    analytics::validate(env)
}
