use crate::storage;
use crate::types::{
    ACTIVITY_COMPLETE, ACTIVITY_CONTRIBUTE, ACTIVITY_DEFAULT, TIER_BRONZE, TIER_DIAMOND, TIER_GOLD,
    TIER_PLATINUM, TIER_SILVER,
};
use soroban_sdk::{Address, Env};

const SCORE_SILVER: u32 = 201;
const SCORE_GOLD: u32 = 401;
const SCORE_PLATINUM: u32 = 601;
const SCORE_DIAMOND: u32 = 801;

/// Returns the numeric tier constant for a given MoiScore value.
///
/// Compare the result with the `TIER_*` constants from `types`:
/// `TIER_BRONZE`, `TIER_SILVER`, `TIER_GOLD`, `TIER_PLATINUM`, `TIER_DIAMOND`.
pub fn get_tier(score: u32) -> u32 {
    if score >= SCORE_DIAMOND {
        TIER_DIAMOND
    } else if score >= SCORE_PLATINUM {
        TIER_PLATINUM
    } else if score >= SCORE_GOLD {
        TIER_GOLD
    } else if score >= SCORE_SILVER {
        TIER_SILVER
    } else {
        TIER_BRONZE
    }
}

/// Calculate collateral requirement in basis points based on MoiScore tier.
/// Lower tiers = higher collateral. Diamond = 0% collateral.
pub fn calculate_collateral(env: &Env, member: &Address) -> u32 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 0,    // 0% — fully trusted
        TIER_PLATINUM => 100, // 1%
        TIER_GOLD => 300,     // 3%
        TIER_SILVER => 500,   // 5%
        _ => 1000,            // 10% — default for bronze/unscored
    }
}

/// Returns the maximum circle size a member can create based on their tier.
pub fn max_circle_size(env: &Env, member: &Address) -> u32 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 100,
        TIER_PLATINUM => 50,
        TIER_GOLD => 20,
        TIER_SILVER => 10,
        _ => 5,
    }
}

/// Returns the maximum contribution amount (in stroops) based on tier.
pub fn max_contribution(env: &Env, member: &Address) -> i128 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 50_000_0000000,  // 50,000 USDC
        TIER_PLATINUM => 10_000_0000000, // 10,000 USDC
        TIER_GOLD => 2_000_0000000,      // 2,000 USDC
        TIER_SILVER => 500_0000000,      // 500 USDC
        _ => 100_0000000,                // 100 USDC
    }
}

/// Checks if a member qualifies for a circle with the given minimum MoiScore.
pub fn qualifies_for_circle(
    env: &Env,
    member: &Address,
    min_score: u32,
    require_completions: bool,
) -> bool {
    let score = storage::get_score(env, member);
    if score < min_score {
        return false;
    }
    if require_completions {
        let completions = storage::get_completions(env, member);
        if completions == 0 {
            return false;
        }
    }
    true
}

/// Record an on-time payment. Returns the new MoiScore.
pub fn record_on_time_payment(
    env: &Env,
    member: &Address,
    circle_id: &Address,
    amount: i128,
    round: u32,
) -> u32 {
    let current = storage::get_score(env, member);
    let mut streak = storage::get_streak(env, member, circle_id);
    let last_round = storage::get_last_round(env, member, circle_id);

    if last_round == 0 || round == last_round + 1 {
        storage::increment_streak(env, member, circle_id);
        streak += 1;
    } else {
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::Streak(member.clone(), circle_id.clone()), &1u32);
        streak = 1;
    }
    storage::set_last_round(env, member, circle_id, round);

    let base: u32 = 10;
    let streak_bonus: u32 = if streak <= 10 { streak * 5 } else { 50 };
    let volume_bonus_raw = (amount / 100_0000000) as u32; // 1 point per 100 USDC
    let volume_bonus: u32 = if volume_bonus_raw > 20 {
        20
    } else {
        volume_bonus_raw
    };

    let new_score = current
        .saturating_add(base)
        .saturating_add(streak_bonus)
        .saturating_add(volume_bonus);
    let capped = if new_score > 1000 { 1000 } else { new_score };

    // Update score
    storage::set_score(env, member, capped);
    // Log activity
    storage::add_activity(env, member, ACTIVITY_CONTRIBUTE, 1);

    capped
}

/// Record a circle completion. +100 points. Cap at 1000.
pub fn record_circle_completion(env: &Env, member: &Address) -> u32 {
    let current = storage::get_score(env, member);
    let bonus: u32 = 100;
    let new_score = current.saturating_add(bonus);
    let capped = if new_score > 1000 { 1000 } else { new_score };

    storage::increment_completions(env, member);
    storage::set_score(env, member, capped);
    storage::add_activity(env, member, ACTIVITY_COMPLETE, 5);

    capped
}

/// Record a default (missed payment/circle). -200 points. Floor at 0.
pub fn record_default(env: &Env, member: &Address) -> u32 {
    let current = storage::get_score(env, member);
    let penalty: u32 = 200;
    let new_score = if current > penalty {
        current - penalty
    } else {
        0
    };

    storage::increment_defaults(env, member);
    storage::set_score(env, member, new_score);
    storage::add_activity(env, member, ACTIVITY_DEFAULT, 0);

    new_score
}

/// Apply inactivity decay. -5 points per 30 days of inactivity. Floor at 0.
pub fn apply_inactivity_decay(env: &Env, member: &Address, days_inactive: u64) -> u32 {
    let current = storage::get_score(env, member);
    let months = days_inactive / 30;
    let decay: u32 = ((months as u64).saturating_mul(5)) as u32;
    let new_score = if current > decay { current - decay } else { 0 };

    storage::set_score(env, member, new_score);
    new_score
}

pub fn get_score(env: &Env, member: &Address) -> u32 {
    storage::get_score(env, member)
}

/// Test and helper utility to set member score directly
pub fn set_score(env: &Env, member: &Address, score: u32) {
    storage::set_score(env, member, score);
}
