use soroban_sdk::{contracterror, Address, Env};

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessError {
    NotInitialized = 1,
    Unauthorized = 2,
}

pub fn require_self_or_admin(
    env: &Env,
    addr: &Address,
    stored_admin: &Address,
) -> Result<(), AccessError> {
    let caller = env.current_contract_address();
    if &caller == addr {
        return Ok(());
    }
    if addr == stored_admin {
        Ok(())
    } else {
        Err(AccessError::Unauthorized)
    }
}
