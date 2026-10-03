use crate::types::*;
use common::pause;
use soroban_sdk::{token::Client as TokenClient, Address, BytesN, Env, Vec};

pub fn init(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    admin.require_auth();
    if env.storage().instance().has(&DataKey::Admin) {
        return Err(EscrowError::AlreadyInitialized);
    }
    if *admin == env.current_contract_address() {
        return Err(EscrowError::InvalidAdmin);
    }
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::NextSwapId, &0u64);
    env.storage()
        .persistent()
        .set(&DataKey::SwapRequests, &Vec::<SwapRequest>::new(env));
    Ok(())
}

/// Creates a new escrow swap between two parties.
///
/// # Arguments
/// * `env` - The contract environment
/// * `initiator` - The address creating the swap
/// * `responder` - The address expected to accept the swap
/// * `token_a` - The token address deposited by the initiator
/// * `token_b` - The token address to be deposited by the responder
/// * `initiator_amount` - Amount of token_a to deposit (must be positive)
/// * `responder_amount` - Amount of token_b the responder must deposit (must be positive)
/// * `hash_lock` - SHA-256 hash of the secret that will be required to accept the swap
/// * `time_lock` - Unix timestamp deadline for accepting the swap (must be in the future)
///
/// # Returns
/// * `Ok(u64)` with the new swap ID if successful
/// * `Err(EscrowError)` if any validation fails
///
/// # Validation
/// * initiator and responder must be different addresses
/// * initiator_amount and responder_amount must be positive
/// * time_lock must be strictly greater than current timestamp (deadline is exclusive for creation)
/// * The contract must not be paused
///
/// # Deadline Semantics
/// The deadline for creation is exclusive: `time_lock <= now` is rejected.
/// This means the swap must have a future deadline at creation time.
pub fn create_swap(
    env: &Env,
    initiator: &Address,
    responder: &Address,
    token_a: &Address,
    token_b: &Address,
    initiator_amount: i128,
    responder_amount: i128,
    hash_lock: BytesN<32>,
    time_lock: u64,
) -> Result<u64, EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard =
        common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    initiator.require_auth();

    if initiator == responder {
        return Err(EscrowError::InvalidSwap);
    }
    if initiator_amount <= 0 || responder_amount <= 0 {
        return Err(EscrowError::InvalidAmount);
    }

    let now = env.ledger().timestamp();
    if time_lock <= now {
        return Err(EscrowError::TimeLockExpired);
    }

    let next_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::NextSwapId)
        .unwrap_or(0);
    let swap = SwapRequest {
        id: next_id,
        initiator: initiator.clone(),
        responder: responder.clone(),
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        initiator_amount,
        responder_amount,
        hash_lock,
        time_lock,
        status: STATUS_PENDING,
        created_at: now,
    };

    let mut swaps: Vec<SwapRequest> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .unwrap_or_else(|| Vec::new(env));
    swaps.push_back(swap);
    env.storage()
        .persistent()
        .set(&DataKey::SwapRequests, &swaps);
    env.storage()
        .instance()
        .set(&DataKey::NextSwapId, &next_id.checked_add(1).ok_or(EscrowError::InvalidAmount)?);

    let token_a_client = TokenClient::new(env, token_a);
    token_a_client.transfer(initiator, &env.current_contract_address(), &initiator_amount);

    SwapCreated {
        id: next_id,
        initiator: initiator.clone(),
        responder: responder.clone(),
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        initiator_amount,
        responder_amount,
    }
    .publish(env);

    Ok(next_id)
}

/// Accepts a pending swap by providing the secret that matches the hash lock.
///
/// # Arguments
/// * `env` - The contract environment
/// * `id` - The swap ID to accept
/// * `responder` - The address of the responder accepting the swap (must match the swap's responder)
/// * `secret` - The secret that produces the hash lock when hashed with SHA-256
///
/// # Returns
/// * `Ok(())` if the swap was successfully accepted
/// * `Err(EscrowError)` if any validation fails
///
/// # Validation
/// * The swap must be in PENDING status
/// * The caller must be the swap's responder
/// * The provided secret must match the swap's hash lock
/// * The current timestamp must be strictly before the swap's time_lock (deadline is inclusive)
/// * The contract must not be paused
///
/// # Deadline Semantics
/// The deadline is inclusive: acceptance is rejected if `now >= time_lock`.
/// This means the swap cannot be accepted at or after the exact deadline timestamp.
pub fn accept_swap(
    env: &Env,
    id: u64,
    responder: &Address,
    secret: BytesN<32>,
) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard =
        common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    responder.require_auth();

    let mut swaps: Vec<SwapRequest> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .ok_or(EscrowError::NotInitialized)?;
    let mut found = false;

    for i in 0..swaps.len() {
        let mut s = swaps.get(i).ok_or(EscrowError::VecAccessError)?;
        if s.id == id {
            if s.status != STATUS_PENDING {
                return Err(EscrowError::SwapNotActive);
            }
            if s.responder != *responder {
                return Err(EscrowError::Unauthorized);
            }
            let secret_bytes = soroban_sdk::Bytes::from_array(env, &secret.to_array());
            let computed_hash: BytesN<32> = env.crypto().sha256(&secret_bytes).into();
            if computed_hash != s.hash_lock {
                return Err(EscrowError::HashLockMismatch);
            }
            let now = env.ledger().timestamp();
            if now >= s.time_lock {
                return Err(EscrowError::TimeLockExpired);
            }

            s.status = STATUS_ACTIVE;
            swaps.set(i, s.clone());
            found = true;

            let token_b_client = TokenClient::new(env, &s.token_b);
            token_b_client.transfer(
                responder,
                &env.current_contract_address(),
                &s.responder_amount,
            );

            SwapAccepted {
                id,
                responder: responder.clone(),
            }
            .publish(env);
            break;
        }
    }

    if !found {
        return Err(EscrowError::SwapNotFound);
    }
    env.storage()
        .persistent()
        .set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn complete_swap(env: &Env, id: u64, caller: &Address) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard =
        common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    caller.require_auth();

    let mut swaps: Vec<SwapRequest> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .ok_or(EscrowError::NotInitialized)?;
    let mut found = false;

    for i in 0..swaps.len() {
        let mut s = swaps.get(i).ok_or(EscrowError::VecAccessError)?;
        if s.id == id {
            if s.status != STATUS_ACTIVE {
                return Err(EscrowError::SwapNotActive);
            }
            if *caller != s.initiator && *caller != s.responder {
                return Err(EscrowError::Unauthorized);
            }
            s.status = STATUS_COMPLETED;
            swaps.set(i, s.clone());
            found = true;

            let token_a_client = TokenClient::new(env, &s.token_a);
            token_a_client.transfer(
                &env.current_contract_address(),
                &s.responder,
                &s.initiator_amount,
            );

            let token_b_client = TokenClient::new(env, &s.token_b);
            token_b_client.transfer(
                &env.current_contract_address(),
                &s.initiator,
                &s.responder_amount,
            );

            SwapCompleted {
                id,
                initiator: s.initiator.clone(),
                responder: s.responder.clone(),
            }
            .publish(env);
            break;
        }
    }

    if !found {
        return Err(EscrowError::SwapNotFound);
    }
    env.storage()
        .persistent()
        .set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn cancel_swap(env: &Env, id: u64, caller: &Address) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard =
        common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    caller.require_auth();

    let mut swaps: Vec<SwapRequest> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .ok_or(EscrowError::NotInitialized)?;
    let mut found = false;

    for i in 0..swaps.len() {
        let mut s = swaps.get(i).ok_or(EscrowError::VecAccessError)?;
        if s.id == id {
            if s.status == STATUS_COMPLETED || s.status == STATUS_CANCELLED {
                return Err(EscrowError::SwapNotActive);
            }
            if *caller != s.initiator && *caller != s.responder {
                return Err(EscrowError::Unauthorized);
            }

            let previous_status = s.status;
            s.status = STATUS_CANCELLED;
            swaps.set(i, s.clone());
            found = true;

            let token_a_client = TokenClient::new(env, &s.token_a);
            token_a_client.transfer(
                &env.current_contract_address(),
                &s.initiator,
                &s.initiator_amount,
            );

            if previous_status == STATUS_ACTIVE {
                let token_b_client = TokenClient::new(env, &s.token_b);
                token_b_client.transfer(
                    &env.current_contract_address(),
                    &s.responder,
                    &s.responder_amount,
                );
            }

            SwapCancelled { id }.publish(env);
            break;
        }
    }

    if !found {
        return Err(EscrowError::SwapNotFound);
    }
    env.storage()
        .persistent()
        .set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn get_swap(env: &Env, id: u64) -> Result<SwapRequest, EscrowError> {
    let swaps: Vec<SwapRequest> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .ok_or(EscrowError::NotInitialized)?;
    for i in 0..swaps.len() {
        if let Some(s) = swaps.get(i) {
            if s.id == id {
                return Ok(s);
            }
        }
    }
    Err(EscrowError::SwapNotFound)
}

pub fn get_swaps(env: &Env) -> Vec<SwapRequest> {
    env.storage()
        .persistent()
        .get(&DataKey::SwapRequests)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(EscrowError::NotInitialized)?;
    if admin != &s {
        return Err(EscrowError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| EscrowError::ContractPaused)
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(EscrowError::NotInitialized)?;
    if admin != &s {
        return Err(EscrowError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| EscrowError::ContractPaused)
}
