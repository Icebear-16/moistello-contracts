use soroban_sdk::{contractevent, contracterror, symbol_short, Address, Env};

const PAUSED_KEY: soroban_sdk::Symbol = symbol_short!("paused");
const PAUSE_STARTED_AT_KEY: soroban_sdk::Symbol = symbol_short!("pause_at");
const PAUSE_DURATION_KEY: soroban_sdk::Symbol = symbol_short!("pause_dur");

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimedPauseError {
    InvalidDuration = 1,
    ContractPaused = 2,
}

#[contractevent(topics = ["paused"])]
#[derive(Clone, Debug)]
pub struct TimedPaused {
    #[topic]
    pub by: Address,
    pub duration_seconds: Option<u64>,
}

#[contractevent(topics = ["unpaused"])]
#[derive(Clone, Debug)]
pub struct TimedUnpaused {
    #[topic]
    pub by: Address,
    pub automatic: bool,
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&PAUSED_KEY)
        .unwrap_or(false)
}

pub fn pause(
    env: &Env,
    admin: &Address,
    duration_seconds: Option<u64>,
) -> Result<(), TimedPauseError> {
    admin.require_auth();

    if matches!(duration_seconds, Some(0)) {
        return Err(TimedPauseError::InvalidDuration);
    }

    let now = env.ledger().timestamp();

    env.storage().instance().set(&PAUSED_KEY, &true);
    env.storage().instance().set(&PAUSE_STARTED_AT_KEY, &now);

    match duration_seconds {
        Some(duration) => {
            env.storage()
                .instance()
                .set(&PAUSE_DURATION_KEY, &duration);
        }
        None => {
            env.storage().instance().remove(&PAUSE_DURATION_KEY);
        }
    }

    TimedPaused {
        by: admin.clone(),
        duration_seconds,
    }
    .publish(env);

    Ok(())
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), TimedPauseError> {
    admin.require_auth();

    clear_pause(env);

    TimedUnpaused {
        by: admin.clone(),
        automatic: false,
    }
    .publish(env);

    Ok(())
}

pub fn clear_pause(env: &Env) {
    env.storage().instance().set(&PAUSED_KEY, &false);
    env.storage().instance().remove(&PAUSE_STARTED_AT_KEY);
    env.storage().instance().remove(&PAUSE_DURATION_KEY);
}

pub fn has_expired(env: &Env) -> bool {
    if !is_paused(env) {
        return false;
    }

    let started_at: u64 = match env
        .storage()
        .instance()
        .get(&PAUSE_STARTED_AT_KEY)
    {
        Some(value) => value,
        None => return false,
    };

    let duration: u64 = match env
        .storage()
        .instance()
        .get(&PAUSE_DURATION_KEY)
    {
        Some(value) => value,
        None => return false,
    };

    env.ledger()
        .timestamp()
        .saturating_sub(started_at)
        >= duration
}

pub fn when_not_paused(env: &Env) -> Result<(), TimedPauseError> {
    if has_expired(env) {
        clear_pause(env);
        return Ok(());
    }

    if is_paused(env) {
        return Err(TimedPauseError::ContractPaused);
    }

    Ok(())
}
