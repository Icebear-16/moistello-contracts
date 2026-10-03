use soroban_sdk::{contracterror, contractevent, symbol_short, Address, BytesN, Env};

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    NotAuthorized = 1,
    NotInitialized = 2,
    InvalidWasmHash = 3,
}

/// Emitted when the contract implementation address is updated.
#[contractevent(topics = ["upgraded"])]
#[derive(Clone, Debug)]
pub struct Upgraded {
    #[topic]
    pub by: Address,
    pub new_impl: Address,
}

/// Emitted when the contract's own Wasm bytecode is upgraded in place.
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractUpgraded {
    pub by: Address,
    pub new_wasm_hash: BytesN<32>,
}

pub fn get_implementation(env: &Env) -> Option<Address> {
    env.storage().instance().get(&symbol_short!("impl"))
}

/// Updates the contract implementation address.
///
/// This function deliberately does NOT check `when_not_paused` — a contract
/// upgrade may be the only way to fix a bug that caused the pause state,
/// so the admin must be able to upgrade even while paused.
pub fn set_implementation(
    env: &Env,
    admin: &Address,
    new_impl: &Address,
) -> Result<(), UpgradeError> {
    admin.require_auth();
    env.storage()
        .instance()
        .set(&symbol_short!("impl"), new_impl);
    Upgraded {
        by: admin.clone(),
        new_impl: new_impl.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn upgrade_contract(
    env: &Env,
    admin: &Address,
    new_wasm_hash: &BytesN<32>,
) -> Result<(), UpgradeError> {
    admin.require_auth();
    env.deployer()
        .update_current_contract_wasm(new_wasm_hash.clone());
    ContractUpgraded {
        by: admin.clone(),
        new_wasm_hash: new_wasm_hash.clone(),
    }
    .publish(env);
    Ok(())
}
