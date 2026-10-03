use crate::types::*;
use soroban_sdk::{Address, Env, Vec};

/// Get a member's raw MoiScore value
pub fn get_score(env: &Env, member: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::MemberScore(member.clone()))
        .unwrap_or(0u32)
}

/// Set a member's raw MoiScore value
pub fn set_score(env: &Env, member: &Address, score: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::MemberScore(member.clone()), &score);
}

/// Get the current streak count for a member in a circle
pub fn get_streak(env: &Env, member: &Address, circle_id: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Streak(member.clone(), circle_id.clone()))
        .unwrap_or(0u32)
}

/// Increment the streak count
pub fn increment_streak(env: &Env, member: &Address, circle_id: &Address) {
    let current = get_streak(env, member, circle_id);
    env.storage()
        .persistent()
        .set(&DataKey::Streak(member.clone(), circle_id.clone()), &(current + 1));
}

/// Get number of completed circles
pub fn get_completions(env: &Env, member: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Completions(member.clone()))
        .unwrap_or(0u32)
}

/// Increment completions
pub fn increment_completions(env: &Env, member: &Address) {
    let current = get_completions(env, member);
    env.storage()
        .persistent()
        .set(&DataKey::Completions(member.clone()), &(current + 1));
}

/// Get number of defaults
pub fn get_defaults(env: &Env, member: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Defaults(member.clone()))
        .unwrap_or(0u32)
}

/// Increment defaults
pub fn increment_defaults(env: &Env, member: &Address) {
    let current = get_defaults(env, member);
    env.storage()
        .persistent()
        .set(&DataKey::Defaults(member.clone()), &(current + 1));
}

/// Add an activity record
pub fn add_activity(env: &Env, member: &Address, activity_type: u32, impact: u32) {
    let mut activities: Vec<Activity> = env
        .storage()
        .persistent()
        .get(&DataKey::MemberLog(member.clone()))
        .unwrap_or_else(|| Vec::new(env));
    activities.push_back(Activity {
        user: member.clone(),
        activity_type,
        score_impact: impact,
        timestamp: env.ledger().timestamp(),
    });
    env.storage()
        .persistent()
        .set(&DataKey::MemberLog(member.clone()), &activities);
}

pub fn get_last_round(env: &Env, member: &Address, circle_id: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::LastRound(member.clone(), circle_id.clone()))
        .unwrap_or(0u32)
}

pub fn set_last_round(env: &Env, member: &Address, circle_id: &Address, round: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::LastRound(member.clone(), circle_id.clone()), &round);
}
