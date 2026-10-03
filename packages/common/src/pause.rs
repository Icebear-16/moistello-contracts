use soroban_sdk::{contracterror, contractevent, symbol_short, Address, Env};

#[contracterror]
#[derive(Debug)]
pub enum PauseError {
    ContractPaused = 1,
}

/// Emitted when the contract is paused.
#[contractevent(topics = ["paused"])]
#[derive(Clone, Debug)]
pub struct Paused {
    #[topic]
    pub by: Address,
}

/// Emitted when the contract is unpaused.
#[contractevent(topics = ["unpaused"])]
#[derive(Clone, Debug)]
pub struct Unpaused {
    #[topic]
    pub by: Address,
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&symbol_short!("paused"))
        .unwrap_or(false)
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), PauseError> {
    admin.require_auth();
    if is_paused(env) {
        return Ok(()); // Idempotent: already paused, no-op.
    }
    env.storage()
        .instance()
        .set(&symbol_short!("paused"), &true);
    Paused { by: admin.clone() }.publish(env);
    Ok(())
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), PauseError> {
    admin.require_auth();
    if !is_paused(env) {
        return Ok(()); // Idempotent: already unpaused, no-op.
    }
    env.storage()
        .instance()
        .set(&symbol_short!("paused"), &false);
    Unpaused { by: admin.clone() }.publish(env);
    Ok(())
}

pub fn when_not_paused(env: &Env) -> Result<(), PauseError> {
    if is_paused(env) {
        Err(PauseError::ContractPaused)
    } else {
        Ok(())
    }
}
