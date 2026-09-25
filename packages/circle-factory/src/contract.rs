use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    Address, BytesN, Env, IntoVal, String, Symbol, Vec, symbol_short,
};
use crate::types::*;
use common::{pause, validation};

fn validate_config_address(env: &Env, address: &Address) -> Result<(), FactoryError> {
    validation::validate_address(env, address).map_err(|_| FactoryError::InvalidAddress)
}

fn load_templates(env: &Env) -> Vec<CircleTemplate> {
    env.storage().persistent().get(&DataKey::Templates).unwrap_or_else(|| Vec::new(env))
}

fn save_templates(env: &Env, templates: &Vec<CircleTemplate>) {
    env.storage().persistent().set(&DataKey::Templates, templates);
}

fn find_template(templates: &Vec<CircleTemplate>, template_id: u32) -> Result<CircleTemplate, FactoryError> {
    for i in 0..templates.len() {
        let t = templates.get(i).ok_or(FactoryError::TemplateNotFound)?;
        if t.id == template_id {
            return Ok(t);
        }
    }
    Err(FactoryError::TemplateNotFound)
}

fn validate_template_config(t: &CircleTemplate, config: &CircleConfig) -> Result<(), FactoryError> {
    if config.max_members < t.bounds.min_members || config.max_members > t.bounds.max_members { return Err(FactoryError::TemplateOutOfBounds); }
    if config.contribution_amount < t.bounds.min_amount || config.contribution_amount > t.bounds.max_amount { return Err(FactoryError::TemplateOutOfBounds); }
    if config.total_rounds < t.bounds.min_rounds || config.total_rounds > t.bounds.max_rounds { return Err(FactoryError::TemplateOutOfBounds); }
    if config.payout_type > 3 { return Err(FactoryError::InvalidTemplate); }
    if config.max_members < 2 || config.contribution_amount <= 0 || config.total_rounds == 0 { return Err(FactoryError::InvalidConfig); }
    if config.slug.len() == 0 { return Err(FactoryError::EmptySlug); }
    if config.name.len() == 0 { return Err(FactoryError::InvalidTemplate); }
    Ok(())
}

fn load_factory_config(env: &Env) -> Result<FactoryConfig, FactoryError> {
    let config: FactoryConfig = env
        .storage()
        .instance()
        .get(&DataKey::FactoryConfig)
        .ok_or(FactoryError::FactoryConfigNotSet)?;
    if config.fee_bps > 10_000 {
        return Err(FactoryError::InvalidFeeBps);
    }
    validate_config_address(env, &config.treasury)?;
    validate_config_address(env, &config.reputation_registry)?;
    Ok(config)
}

fn configure_deployed_circle(
    env: &Env,
    circle_id: &Address,
    config: &FactoryConfig,
) -> Result<(), FactoryError> {
    let function_name = Symbol::new(env, "configure_from_factory");
    let args = soroban_sdk::vec![
        env,
        env.current_contract_address().into_val(env),
        config.treasury.clone().into_val(env),
        config.reputation_registry.clone().into_val(env),
        config.fee_bps.into_val(env),
    ];
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: circle_id.clone(),
                fn_name: function_name.clone(),
                args: args.clone(),
            },
            sub_invocations: soroban_sdk::vec![env],
        }),
    ]);
    match env.try_invoke_contract::<(), soroban_sdk::Error>(circle_id, &function_name, args) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(FactoryError::CircleConfigurationFailed),
    }
}

fn deploy_validated(
    env: &Env,
    config: &CircleConfig,
    factory_config: &FactoryConfig,
) -> Result<Address, FactoryError> {
    if env.storage().persistent().has(&DataKey::Slug(config.slug.clone())) { return Err(FactoryError::DuplicateSlug); }
    let wh: BytesN<32> = env.storage().instance().get(&DataKey::WasmHash).ok_or(FactoryError::WasmHashNotSet)?;
    let count: u32 = env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0);
    let mut salt = [0u8; 32];
    salt[28..32].copy_from_slice(&count.to_be_bytes());
    let cid = env.deployer().with_current_contract(BytesN::from_array(env, &salt)).deploy_v2(wh, (config.organizer.clone(), env.current_contract_address(), config.clone()));
    configure_deployed_circle(env, &cid, factory_config)?;
    let now = env.ledger().timestamp();
    let mut circles: Vec<CircleEntry> = env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env));
    circles.push_back(CircleEntry { circle_id: cid.clone(), name: config.name.clone(), organizer: config.organizer.clone(), deployed_at: now, status: 0 });
    env.storage().persistent().set(&DataKey::Slug(config.slug.clone()), &cid);
    env.storage().persistent().set(&DataKey::CircleConfig(cid.clone()), config);
    env.storage().persistent().set(&DataKey::CircleList, &circles);
    let c: u32 = env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0);
    env.storage().instance().set(&DataKey::CircleCount, &c.checked_add(1).ok_or(FactoryError::InvalidConfig)?);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("created")),
        CircleCreated {
            address: cid.clone(),
            admin: config.organizer.clone(),
            token: config.token.clone(),
            timestamp: now,
        },
    );
    env.events().publish((env.current_contract_address(), symbol_short!("deploy")), CircleDeployed { creator: config.organizer.clone(), circle_id: cid.clone(), name: config.name.clone() });
    Ok(cid)
}

/// Initializes the circle factory with admin, fee configuration, WASM hash, and optional deployment fee.
///
/// # Parameters
/// - `env`: Contract execution environment
/// - `admin`: Administrator address with fee update privileges
/// - `fee_bps`: Fee in basis points (0-10000, where 10000 = 100%)
/// - `circle_wasm_hash`: WASM hash of the circle contract to deploy
/// - `deploy_fee`: Optional deployment fee, capped at `MAX_DEPLOY_FEE`
/// - `treasury`: Optional treasury address for receiving deployment fees
///
/// # Returns
/// - `Ok(())` on successful initialization
/// - `Err(FactoryError::InvalidFeeBps)` if fee_bps < 0 or > 10000
/// - `Err(FactoryError::DeployFeeExceedsCap)` if deploy_fee < 0 or > MAX_DEPLOY_FEE
/// - `Err(FactoryError::InvalidTreasury)` if deploy_fee > 0 and treasury is None
pub fn init(
    env: &Env,
    admin: &Address,
    fee_bps: i128,
    circle_wasm_hash: &BytesN<32>,
    deploy_fee: Option<i128>,
    treasury: Option<Address>,
) -> Result<(), FactoryError> {
    admin.require_auth();
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    let fee = deploy_fee.unwrap_or(0);
    if fee < 0 || fee > MAX_DEPLOY_FEE {
        return Err(FactoryError::DeployFeeExceedsCap);
    }
    if fee > 0 && treasury.is_none() {
        return Err(FactoryError::InvalidTreasury);
    }
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    env.storage().instance().set(&DataKey::WasmHash, circle_wasm_hash);
    env.storage().instance().set(&DataKey::CircleCount, &0u32);
    env.storage().instance().set(&DataKey::DeployFee, &fee);
    if let Some(ref t) = treasury {
        env.storage().instance().set(&DataKey::Treasury, t);
    }
    let mut templates: Vec<CircleTemplate> = Vec::new(env);
    templates.push_back(builtin_template(env, 1, "Weekly Starter", 100_0000000, 5, 1, 5));
    templates.push_back(builtin_template(env, 2, "Biweekly Standard", 500_0000000, 8, 1, 8));
    templates.push_back(builtin_template(env, 3, "Monthly Premium", 1000_0000000, 12, 1, 12));
    env.storage().persistent().set(&DataKey::Templates, &templates);
    Ok(())
}

pub fn init_with_config(
    env: &Env,
    admin: &Address,
    fee_bps: i128,
    treasury: &Address,
    reputation_registry: &Address,
    circle_wasm_hash: &BytesN<32>,
) -> Result<(), FactoryError> {
    init(env, admin, fee_bps, circle_wasm_hash, None, None)?;
    validate_config_address(env, treasury)?;
    validate_config_address(env, reputation_registry)?;
    let config = FactoryConfig {
        treasury: treasury.clone(),
        reputation_registry: reputation_registry.clone(),
        fee_bps: fee_bps as u32,
    };
    env.storage().instance().set(&DataKey::FactoryConfig, &config);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("fcfg")),
        FactoryConfigUpdated {
            treasury: treasury.clone(),
            reputation_registry: reputation_registry.clone(),
            fee_bps: fee_bps as u32,
            updated_by: admin.clone(),
        },
    );
    Ok(())
}

fn charge_deploy_fee(env: &Env, organizer: &Address, token: &Address) -> Result<(), FactoryError> {
    let deploy_fee: i128 = env.storage().instance().get(&DataKey::DeployFee).unwrap_or(0);
    if deploy_fee > 0 {
        let treasury: Address = env.storage().instance().get(&DataKey::Treasury).ok_or(FactoryError::InvalidTreasury)?;
        let token_client = soroban_sdk::token::Client::new(env, token);
        token_client.transfer(organizer, &treasury, &deploy_fee);
        env.events().publish(
            (env.current_contract_address(), symbol_short!("fee_paid")),
            DeployFeePaid {
                organizer: organizer.clone(),
                treasury,
                amount: deploy_fee,
                token: token.clone(),
            },
        );
    }
    Ok(())
}

fn builtin_template(env: &Env, id: u32, name: &str, amount: i128, members: u32, payout_type: u32, rounds: u32) -> CircleTemplate {
    CircleTemplate {
        id,
        name: String::from_str(env, name),
        contribution_amount: amount,
        max_members: members,
        payout_type,
        total_rounds: rounds,
        bounds: TemplateBounds { min_members: 2, max_members: 20, min_amount: 1, max_amount: 10000_0000000, min_rounds: 1, max_rounds: 52 },
        defaults: TemplateDefaults { contribution_deadline_seconds: 604800, min_moi_score: 0, collateral_amount: 0, penalty_bps: 500, grace_period_seconds: 86400, max_strikes: 3 },
    }
}

pub fn deploy_circle(env: &Env, config: &CircleConfig) -> Result<Address, FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    config.organizer.require_auth();
    if config.max_members < 2 || config.contribution_amount <= 0 || config.total_rounds == 0 || config.payout_type > 3 { return Err(FactoryError::InvalidConfig); }
    if config.slug.len() == 0 { return Err(FactoryError::EmptySlug); }
    let factory_config = load_factory_config(env)?;
    charge_deploy_fee(env, &config.organizer, &config.token)?;
    deploy_validated(env, config, &factory_config)
}

pub fn get_deploy_fee(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::DeployFee).unwrap_or(0)
}

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Treasury)
}

pub fn set_deploy_fee(env: &Env, admin: &Address, fee: i128, treasury: Option<Address>) -> Result<(), FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    if fee < 0 || fee > MAX_DEPLOY_FEE {
        return Err(FactoryError::DeployFeeExceedsCap);
    }
    if fee > 0 && treasury.is_none() {
        return Err(FactoryError::InvalidTreasury);
    }
    let old_fee = env.storage().instance().get(&DataKey::DeployFee).unwrap_or(0);
    env.storage().instance().set(&DataKey::DeployFee, &fee);
    if let Some(ref t) = treasury {
        env.storage().instance().set(&DataKey::Treasury, t);
    } else {
        env.storage().instance().remove(&DataKey::Treasury);
    }
    env.events().publish(
        (env.current_contract_address(), symbol_short!("dep_fee")),
        DeployFeeConfigUpdated { old_fee, new_fee: fee, treasury },
    );
    Ok(())
}

pub fn get_templates(env: &Env) -> Vec<CircleTemplate> { load_templates(env) }

pub fn get_template(env: &Env, template_id: u32) -> Result<CircleTemplate, FactoryError> {
    find_template(&load_templates(env), template_id)
}

pub fn create_template(env: &Env, admin: &Address, template: &CircleTemplate) -> Result<(), FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    if template.name.len() == 0 || template.contribution_amount <= 0 || template.max_members < 2 || template.total_rounds == 0 || template.payout_type > 3 {
        return Err(FactoryError::InvalidTemplate);
    }
    if template.bounds.min_members < 2 || template.bounds.max_members < template.bounds.min_members || template.bounds.min_amount <= 0 || template.bounds.max_amount < template.bounds.min_amount || template.bounds.min_rounds == 0 || template.bounds.max_rounds < template.bounds.min_rounds {
        return Err(FactoryError::InvalidTemplate);
    }
    let mut templates = load_templates(env);
    for i in 0..templates.len() {
        let t = templates.get(i).ok_or(FactoryError::InvalidTemplate)?;
        if t.id == template.id { return Err(FactoryError::TemplateExists); }
    }
    templates.push_back(template.clone());
    save_templates(env, &templates);
    env.events().publish((env.current_contract_address(), symbol_short!("tmpl_new")), TemplateCreated { id: template.id, name: template.name.clone() });
    Ok(())
}

pub fn deploy_from_template(env: &Env, template_id: u32, organizer: &Address, token: &Address, name: soroban_sdk::String, slug: soroban_sdk::String) -> Result<Address, FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    organizer.require_auth();
    let t = find_template(&load_templates(env), template_id)?;
    let config = CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name,
        contribution_amount: t.contribution_amount,
        max_members: t.max_members,
        payout_type: t.payout_type,
        total_rounds: t.total_rounds,
        contribution_deadline_seconds: t.defaults.contribution_deadline_seconds,
        min_moi_score: t.defaults.min_moi_score,
        collateral_amount: t.defaults.collateral_amount,
        penalty_bps: t.defaults.penalty_bps,
        grace_period_seconds: t.defaults.grace_period_seconds,
        max_strikes: t.defaults.max_strikes,
        slug,
    };
    validate_template_config(&t, &config)?;
    let factory_config = load_factory_config(env)?;
    charge_deploy_fee(env, organizer, token)?;
    let cid = deploy_validated(env, &config, &factory_config)?;
    env.events().publish((env.current_contract_address(), symbol_short!("tmpl_dep")), TemplateDeployed { template_id, circle_id: cid.clone(), creator: organizer.clone() });
    Ok(cid)
}

pub fn deploy_from_template_custom(env: &Env, template_id: u32, config: &CircleConfig) -> Result<Address, FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    config.organizer.require_auth();
    let t = find_template(&load_templates(env), template_id)?;
    validate_template_config(&t, config)?;
    let factory_config = load_factory_config(env)?;
    charge_deploy_fee(env, &config.organizer, &config.token)?;
    let cid = deploy_validated(env, config, &factory_config)?;
    env.events().publish((env.current_contract_address(), symbol_short!("tmpl_dep")), TemplateDeployed { template_id, circle_id: cid.clone(), creator: config.organizer.clone() });
    Ok(cid)
}

pub fn migrate_circles(env: &Env, caller: &Address, start_index: u32, limit: u32) -> Result<u32, FactoryError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if caller != &s { return Err(FactoryError::Unauthorized); }
    caller.require_auth();
    let circles: Vec<CircleEntry> = env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env));
    let end = start_index.saturating_add(limit).min(circles.len());
    let mut migrated: u32 = 0;
    for i in start_index..end {
        let entry = circles.get(i).ok_or(FactoryError::InvalidConfig)?;
        env.authorize_as_current_contract(soroban_sdk::vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: entry.circle_id.clone(),
                    fn_name: symbol_short!("migrate"),
                    args: soroban_sdk::vec![env, env.current_contract_address().into_val(env)],
                },
                sub_invocations: soroban_sdk::vec![env],
            }),
        ]);
        let args: soroban_sdk::Vec<soroban_sdk::Val> = (env.current_contract_address(),).into_val(env);
        if matches!(env.try_invoke_contract::<(), soroban_sdk::Error>(&entry.circle_id, &Symbol::new(env, "migrate"), args), Ok(Ok(()))) {
            migrated = migrated.checked_add(1).ok_or(FactoryError::InvalidConfig)?;
        }
    }
    env.events().publish((env.current_contract_address(), symbol_short!("mig_batch")), MigrationBatchProgress { migrated, start_index });
    Ok(migrated)
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

pub fn get_factory_config(env: &Env) -> Option<FactoryConfig> {
    env.storage().instance().get(&DataKey::FactoryConfig)
}

pub fn set_factory_config(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    reputation_registry: &Address,
    fee_bps: u32,
) -> Result<(), FactoryError> {
    admin.require_auth();
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(FactoryError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(FactoryError::Unauthorized);
    }
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    if fee_bps > 10_000 {
        return Err(FactoryError::InvalidFeeBps);
    }
    validate_config_address(env, treasury)?;
    validate_config_address(env, reputation_registry)?;
    let config = FactoryConfig {
        treasury: treasury.clone(),
        reputation_registry: reputation_registry.clone(),
        fee_bps,
    };
    env.storage().instance().set(&DataKey::FactoryConfig, &config);
    env.storage().instance().set(
        &DataKey::FeeConfig,
        &FeeConfig {
            fee_bps: fee_bps as i128,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        },
    );
    env.events().publish(
        (env.current_contract_address(), symbol_short!("fcfg")),
        FactoryConfigUpdated {
            treasury: treasury.clone(),
            reputation_registry: reputation_registry.clone(),
            fee_bps,
            updated_by: admin.clone(),
        },
    );
    Ok(())
}

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
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    let old: FeeConfig = env.storage().instance().get(&DataKey::FeeConfig).unwrap_or_else(|| FeeConfig { fee_bps:0, updated_at:0, updated_by: env.current_contract_address() });
    let normalized_fee_bps = fee_bps as u32;
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    if let Some(mut factory_config) = env.storage().instance().get::<_, FactoryConfig>(&DataKey::FactoryConfig) {
        factory_config.fee_bps = normalized_fee_bps;
        env.storage().instance().set(&DataKey::FactoryConfig, &factory_config);
    }
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
