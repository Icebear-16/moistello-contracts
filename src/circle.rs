use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberStatus {
    Active = 1,
    Exited = 2,
    Defaulted = 3,
}

#[contracttype]
pub enum DataKey {
    MemberStatus(Address),
    Vote(u32, Address), // round, member
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn vote_payout(env: Env, voter: Address, round: u32, support: bool) {
        voter.require_auth();

        // FIX: Verify voter membership status is strictly ACTIVE before accepting vote
        let status: MemberStatus = env
            .storage()
            .persistent()
            .get(&DataKey::MemberStatus(voter.clone()))
            .unwrap_or_else(|| panic!("Voter is not a registered member"));

        if status != MemberStatus::Active {
            panic!("Only active members are permitted to vote on payouts; exited or defaulted members cannot vote");
        }

        // Check if member already voted for this round
        if env.storage().persistent().has(&DataKey::Vote(round, voter.clone())) {
            panic!("Member has already cast a vote for this round");
        }

        // Record vote state
        env.storage().persistent().set(&DataKey::Vote(round, voter.clone()), &support);

        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "VoteCast"), round, voter),
            support,
        );
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleConfig {
    pub base_amount: i128,
    pub dynamic_amount: bool,
    pub min_amount: i128,
    pub max_amount: i128,
}

#[contracttype]
pub enum DataKey {
    Config,
    Oracle,
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn get_yield_rate(env: Env) -> i128 {
        // Retrieve current yield rate from oracle (expressed in basis points, e.g., 1050 = 10.5% multiplier or scaled)
        let oracle_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .unwrap_or_else(|| panic!("Oracle not configured"));
        
        // Call oracle contract or retrieve stored oracle rate
        // For demonstration, querying oracle client or returning normalized rate
        1050 // Default 1.05x multiplier in basis points (base 1000)
    }

    pub fn calculate_dynamic_contribution(env: Env) -> i128 {
        let config: CircleConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("Circle config not found"));

        if !config.dynamic_amount {
            return config.base_amount;
        }

        let yield_rate = Self::get_yield_rate(env);

        // Calculate dynamic amount based on yield rate (yield_rate scaled by 1000)
        let calculated = (config.base_amount * yield_rate) / 1000;

        // Enforce minimum and maximum caps
        if calculated < config.min_amount {
            config.min_amount
        } else if calculated > config.max_amount {
            config.max_amount
        } else {
            calculated
        }
    }
}