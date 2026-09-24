use soroban_sdk::{Address, BytesN, Env, Vec, symbol_short};
use crate::types::*; use common::pause;

/// Initializes the circle factory with admin, fee configuration, and WASM hash.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Administrator address with fee update privileges
/// - `fee_bps`: Fee in basis points (0-10000, where 10000 = 100%)
/// - `circle_wasm_hash`: WASM hash of the circle contract to deploy
///
/// # Returns
/// - `Ok(())` on successful initialization
/// - `Err(FactoryError::InvalidFeeBps)` if fee_bps < 0 or > 10000
///
/// # Authorization
/// Requires authentication from the admin address.
///
/// # Panics
/// Never panics. All errors are returned as typed FactoryError variants.
pub fn init(env: &Env, admin: &Address, fee_bps: i128, circle_wasm_hash: &BytesN<32>) -> Result<(), FactoryError> {
    admin.require_auth();
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    env.storage().instance().set(&DataKey::WasmHash, circle_wasm_hash);
    env.storage().instance().set(&DataKey::CircleCount, &0u32);
    Ok(())
}
/// Deploys a new circle contract with the provided configuration.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `config`: Circle configuration including organizer, token, contribution amount, max members, payout type, and rounds
///
/// # Returns
/// - `Ok(Address)` - Address of the newly deployed circle contract
/// - `Err(FactoryError::ContractPaused)` if factory is paused
/// - `Err(FactoryError::InvalidConfig)` if config validation fails (max_members < 2, contribution_amount <= 0, total_rounds == 0, or payout_type > 3)
/// - `Err(FactoryError::WasmHashNotSet)` if WASM hash not configured
///
/// # Authorization
/// Requires authentication from the organizer address specified in config.
///
/// # Notes
/// - Increments circle_count after successful deployment
/// - Records circle entry in the factory registry
/// - Emits CircleDeployed event with creator, circle_id, and name
///
/// # Panics
/// Never panics. All errors are returned as typed FactoryError variants.
pub fn deploy_circle(env: &Env, config: &CircleConfig) -> Result<Address, FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    config.organizer.require_auth();
    if config.max_members < 2 || config.contribution_amount <= 0 || config.total_rounds == 0 || config.payout_type > 3 { return Err(FactoryError::InvalidConfig); }
    if config.slug.len() == 0 { return Err(FactoryError::EmptySlug); }
    if env.storage().persistent().has(&DataKey::Slug(config.slug.clone())) { return Err(FactoryError::DuplicateSlug); }
    let wh: BytesN<32> = env.storage().instance().get(&DataKey::WasmHash).ok_or(FactoryError::WasmHashNotSet)?;
    let count: u32 = env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0);
    let mut salt = [0u8; 32];
    salt[28..32].copy_from_slice(&count.to_be_bytes());
    let cid = env.deployer().with_current_contract(BytesN::from_array(env, &salt)).deploy_v2(wh, (config.organizer.clone(), env.current_contract_address(), config.clone()));
    let now = env.ledger().timestamp();
    let mut circles: Vec<CircleEntry> = env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env));
    circles.push_back(CircleEntry { circle_id: cid.clone(), name: config.name.clone(), organizer: config.organizer.clone(), deployed_at: now, status: 0 });
    env.storage().persistent().set(&DataKey::Slug(config.slug.clone()), &cid);
    env.storage().persistent().set(&DataKey::CircleConfig(cid.clone()), config);
    env.storage().persistent().set(&DataKey::CircleList, &circles);
    let c: u32 = env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0);
    env.storage().instance().set(&DataKey::CircleCount, &c.checked_add(1).ok_or(FactoryError::InvalidConfig)?);
    env.events().publish((env.current_contract_address(), symbol_short!("deploy")), CircleDeployed { creator: config.organizer.clone(), circle_id: cid.clone(), name: config.name.clone() });
    Ok(cid)
}
/// Returns the registry of all circles deployed by this factory.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// CircleRegistry struct containing a vector of all deployed circle entries.
///
/// # Panics
/// Never panics. Returns empty registry if no circles have been deployed.
pub fn get_circle_config(env: &Env, cid: &Address) -> Result<CircleConfig, FactoryError> {
    env.storage()
        .persistent()
        .get(&DataKey::CircleConfig(cid.clone()))
        .ok_or(FactoryError::InvalidConfig)
}

pub fn get_circles(env: &Env) -> CircleRegistry { CircleRegistry { circles: env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env)) } }
/// Returns the total count of circles deployed by this factory.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// Number of circles deployed, or 0 if none.
///
/// # Panics
/// Never panics.
pub fn get_circle_count(env: &Env) -> u32 { env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0) }
/// Returns the current fee configuration.
///
/// # Parameters
/// - `env`: Contract execution environment
///
/// # Returns
/// FeeConfig struct containing fee_bps, updated_at timestamp, and updated_by address.
/// Returns default FeeConfig with 0 fee if not initialized.
///
/// # Panics
/// Never panics.
pub fn get_fee_config(env: &Env) -> FeeConfig { env.storage().instance().get(&DataKey::FeeConfig).unwrap_or_else(|| FeeConfig { fee_bps:0, updated_at:0, updated_by: env.current_contract_address() }) }
/// Updates the fee configuration for all future circle deployments.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address requesting the fee update
/// - `fee_bps`: New fee in basis points (0-10000)
///
/// # Returns
/// - `Ok(())` on successful fee update
/// - `Err(FactoryError::ContractPaused)` if factory is paused
/// - `Err(FactoryError::Unauthorized)` if caller is not the admin
/// - `Err(FactoryError::InvalidFeeBps)` if fee_bps < 0 or > 10000
/// - `Err(FactoryError::NotInitialized)` if admin not set
///
/// # Authorization
/// Requires authentication from admin and admin must match stored admin.
///
/// # Notes
/// Emits FeeConfigUpdated event with old and new fee_bps values.
///
/// # Panics
/// Never panics. All errors are returned as typed FactoryError variants.
pub fn set_fee_config(env: &Env, admin: &Address, fee_bps: i128) -> Result<(), FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    let old: FeeConfig = env.storage().instance().get(&DataKey::FeeConfig).unwrap_or_else(|| FeeConfig { fee_bps:0, updated_at:0, updated_by: env.current_contract_address() });
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    env.events().publish((env.current_contract_address(), symbol_short!("fee_cfg")), FeeConfigUpdated { old_fee_bps: old.fee_bps, new_fee_bps: fee_bps, updated_by: admin.clone() });
    Ok(())
}
/// Pauses the factory, preventing new circle deployments.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address requesting the pause
///
/// # Returns
/// - `Ok(())` on successful pause
/// - `Err(FactoryError::Unauthorized)` if caller is not the admin
/// - `Err(FactoryError::NotInitialized)` if admin not set
/// - `Err(FactoryError::ContractPaused)` if pause operation fails
///
/// # Authorization
/// Only the stored admin can pause the factory.
///
/// # Panics
/// Never panics. All errors are returned as typed FactoryError variants.
pub fn pause(env: &Env, admin: &Address) -> Result<(), FactoryError> { let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?; if admin != &s { return Err(FactoryError::Unauthorized); } pause::pause(env, admin).map_err(|_| FactoryError::ContractPaused) }
/// Unpauses the factory, allowing new circle deployments.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Admin address requesting the unpause
///
/// # Returns
/// - `Ok(())` on successful unpause
/// - `Err(FactoryError::Unauthorized)` if caller is not the admin
/// - `Err(FactoryError::NotInitialized)` if admin not set
/// - `Err(FactoryError::ContractPaused)` if unpause operation fails
///
/// # Authorization
/// Only the stored admin can unpause the factory.
///
/// # Panics
/// Never panics. All errors are returned as typed FactoryError variants.
pub fn unpause(env: &Env, admin: &Address) -> Result<(), FactoryError> { let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?; if admin != &s { return Err(FactoryError::Unauthorized); } pause::unpause(env, admin).map_err(|_| FactoryError::ContractPaused) }
