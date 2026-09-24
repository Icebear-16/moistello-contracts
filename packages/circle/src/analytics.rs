use crate::types::*;
use common::math;
use soroban_sdk::{Address, Env, Map, Vec};

fn default_analytics() -> CircleAnalytics {
    CircleAnalytics {
        total_contributions: 0,
        contribution_count: 0,
        total_payouts: 0,
        avg_completion_time: 0,
        total_members: 0,
        total_defaults: 0,
    }
}

fn default_stats(member: &Address) -> MemberStats {
    MemberStats {
        member: member.clone(),
        joined_at: 0,
        contribution_count: 0,
        total_contributed: 0,
        total_received: 0,
        defaults: 0,
    }
}

pub fn load(env: &Env) -> CircleAnalytics {
    env.storage()
        .instance()
        .get(&DataKey::Analytics)
        .unwrap_or_else(default_analytics)
}

pub fn save(env: &Env, analytics: &CircleAnalytics) {
    env.storage()
        .instance()
        .set(&DataKey::Analytics, analytics);
}

fn load_map(env: &Env) -> Map<Address, MemberStats> {
    env.storage()
        .persistent()
        .get(&DataKey::MemberStatsMap)
        .unwrap_or_else(|| Map::new(env))
}

fn save_map(env: &Env, map: &Map<Address, MemberStats>) {
    env.storage()
        .persistent()
        .set(&DataKey::MemberStatsMap, map);
}

pub fn get_member_stats(env: &Env, member: &Address) -> MemberStats {
    load_map(env)
        .get(member.clone())
        .unwrap_or_else(|| default_stats(member))
}

pub fn get_all_member_stats(env: &Env) -> Vec<MemberStats> {
    let map = load_map(env);
    let mut out = Vec::new(env);
    for (_, stats) in map.iter() {
        out.push_back(stats);
    }
    out
}

pub fn record_join(
    env: &Env,
    member: &Address,
    member_count: u32,
    joined_at: u64,
) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_members = member_count;
    save(env, &analytics);
    let mut map = load_map(env);
    if map.get(member.clone()).is_none() {
        map.set(member.clone(), default_stats(member));
        let stats = map.get(member.clone()).ok_or(CircleError::VecAccessError)?;
        let mut updated = stats;
        updated.joined_at = joined_at;
        map.set(member.clone(), updated);
        save_map(env, &map);
    }
    Ok(())
}

pub fn sync_members(
    env: &Env,
    members: &Vec<Member>,
    member_count: u32,
) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_members = member_count;
    save(env, &analytics);
    let mut map = load_map(env);
    for i in 0..members.len() {
        let member = members.get(i).ok_or(CircleError::VecAccessError)?;
        if map.get(member.address.clone()).is_none() {
            let mut stats = default_stats(&member.address);
            stats.joined_at = member.joined_at;
            map.set(member.address.clone(), stats);
        }
    }
    save_map(env, &map);
    Ok(())
}

pub fn record_contribution(
    env: &Env,
    member: &Address,
    amount: i128,
) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_contributions =
        math::safe_add(analytics.total_contributions, amount)
            .map_err(|_| CircleError::InvalidAmount)?;
    analytics.contribution_count = analytics
        .contribution_count
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    save(env, &analytics);
    let mut map = load_map(env);
    let mut stats = map
        .get(member.clone())
        .unwrap_or_else(|| default_stats(member));
    stats.total_contributed = math::safe_add(stats.total_contributed, amount)
        .map_err(|_| CircleError::InvalidAmount)?;
    stats.contribution_count = stats
        .contribution_count
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    map.set(member.clone(), stats);
    save_map(env, &map);
    Ok(())
}

pub fn record_receipt(env: &Env, member: &Address, amount: i128) -> Result<(), CircleError> {
    if amount <= 0 {
        return Ok(());
    }
    let mut map = load_map(env);
    let mut stats = map
        .get(member.clone())
        .unwrap_or_else(|| default_stats(member));
    stats.total_received = math::safe_add(stats.total_received, amount)
        .map_err(|_| CircleError::InvalidAmount)?;
    map.set(member.clone(), stats);
    save_map(env, &map);
    Ok(())
}

pub fn record_payout_total(env: &Env, circle: &Circle) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_payouts = circle.total_payouts;
    save(env, &analytics);
    Ok(())
}

pub fn record_round_completed(env: &Env, circle: &Circle) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_payouts = circle.total_payouts;
    if circle.current_round > 0 && circle.started_at > 0 {
        let now = env.ledger().timestamp();
        analytics.avg_completion_time = now.saturating_sub(circle.started_at)
            / circle.current_round as u64;
    }
    save(env, &analytics);
    Ok(())
}

pub fn record_default(env: &Env, member: &Address) -> Result<(), CircleError> {
    let mut analytics = load(env);
    analytics.total_defaults = analytics
        .total_defaults
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    save(env, &analytics);
    let mut map = load_map(env);
    let mut stats = map
        .get(member.clone())
        .unwrap_or_else(|| default_stats(member));
    stats.defaults = stats
        .defaults
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    map.set(member.clone(), stats);
    save_map(env, &map);
    Ok(())
}

pub fn recompute(env: &Env, circle: &Circle) -> Result<(), CircleError> {
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    let mut analytics = CircleAnalytics {
        total_contributions: 0,
        contribution_count: 0,
        total_payouts: circle.total_payouts,
        avg_completion_time: 0,
        total_members: members.len() as u32,
        total_defaults: 0,
    };
    let mut per_member: Map<Address, (i128, u32)> = Map::new(env);
    for i in 0..contributions.len() {
        let contribution = contributions.get(i).ok_or(CircleError::VecAccessError)?;
        analytics.total_contributions =
            math::safe_add(analytics.total_contributions, contribution.amount)
                .map_err(|_| CircleError::InvalidAmount)?;
        analytics.contribution_count = analytics
            .contribution_count
            .checked_add(1)
            .ok_or(CircleError::InvalidAmount)?;
        let entry = per_member
            .get(contribution.member.clone())
            .unwrap_or((0, 0));
        let total = math::safe_add(entry.0, contribution.amount)
            .map_err(|_| CircleError::InvalidAmount)?;
        let count = entry.1.checked_add(1).ok_or(CircleError::InvalidAmount)?;
        per_member.set(contribution.member.clone(), (total, count));
    }
    let mut map: Map<Address, MemberStats> = Map::new(env);
    for i in 0..members.len() {
        let member = members.get(i).ok_or(CircleError::VecAccessError)?;
        let (total_contributed, contribution_count) = per_member
            .get(member.address.clone())
            .unwrap_or((0, 0));
        let mut defaults: u32 = 0;
        if member.status == MEMBER_DEFAULTED {
            defaults = 1;
            analytics.total_defaults = analytics
                .total_defaults
                .checked_add(1)
                .ok_or(CircleError::InvalidAmount)?;
        }
        map.set(
            member.address.clone(),
            MemberStats {
                member: member.address.clone(),
                joined_at: member.joined_at,
                contribution_count,
                total_contributed,
                total_received: member.total_received,
                defaults,
            },
        );
    }
    let mut last_payout_at: u64 = 0;
    for i in 0..payouts.len() {
        let payout = payouts.get(i).ok_or(CircleError::VecAccessError)?;
        if payout.timestamp > last_payout_at {
            last_payout_at = payout.timestamp;
        }
    }
    if circle.current_round > 0 && circle.started_at > 0 && last_payout_at > circle.started_at {
        analytics.avg_completion_time = last_payout_at.saturating_sub(circle.started_at)
            / circle.current_round as u64;
    }
    save(env, &analytics);
    save_map(env, &map);
    Ok(())
}

pub fn validate(env: &Env) -> Result<(), CircleError> {
    let circle: Circle = env
        .storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)?;
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let analytics: CircleAnalytics = env
        .storage()
        .instance()
        .get(&DataKey::Analytics)
        .ok_or(CircleError::MigrationValidationFailed)?;
    let map = load_map(env);
    if circle.member_count != members.len() as u32 || analytics.total_members != members.len() as u32
    {
        return Err(CircleError::MigrationValidationFailed);
    }
    let mut total: i128 = 0;
    let mut count: u32 = 0;
    for i in 0..contributions.len() {
        let contribution = contributions.get(i).ok_or(CircleError::VecAccessError)?;
        total = math::safe_add(total, contribution.amount)
            .map_err(|_| CircleError::MigrationValidationFailed)?;
        count = count
            .checked_add(1)
            .ok_or(CircleError::MigrationValidationFailed)?;
    }
    if analytics.total_contributions != total || analytics.contribution_count != count {
        return Err(CircleError::MigrationValidationFailed);
    }
    if analytics.total_payouts != circle.total_payouts {
        return Err(CircleError::MigrationValidationFailed);
    }
    for i in 0..members.len() {
        let member = members.get(i).ok_or(CircleError::VecAccessError)?;
        let stats = map
            .get(member.address.clone())
            .ok_or(CircleError::MigrationValidationFailed)?;
        let mut member_total: i128 = 0;
        let mut member_count: u32 = 0;
        for j in 0..contributions.len() {
            let contribution = contributions.get(j).ok_or(CircleError::VecAccessError)?;
            if contribution.member == member.address {
                member_total = math::safe_add(member_total, contribution.amount)
                    .map_err(|_| CircleError::MigrationValidationFailed)?;
                member_count = member_count
                    .checked_add(1)
                    .ok_or(CircleError::MigrationValidationFailed)?;
            }
        }
        if stats.total_contributed != member_total
            || stats.contribution_count != member_count
            || stats.total_received != member.total_received
        {
            return Err(CircleError::MigrationValidationFailed);
        }
    }
    Ok(())
}
