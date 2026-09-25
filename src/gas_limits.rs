use soroban_sdk::Env;

/// Conservative application-level ceilings.
///
/// These values are placeholders until measured against the actual contract
/// implementation and deployed network resource limits. Do not treat them as
/// production-tuned values without benchmark evidence.
pub const MAX_BATCH_INVITE_MEMBERS: u32 = 50;
pub const MAX_RESOLVE_VOTE_ENTRIES: u32 = 50;

pub fn validate_batch_size(
    actual: u32,
    maximum: u32,
) -> Result<(), GasLimitError> {
    if actual == 0 {
        return Err(GasLimitError::EmptyBatch);
    }

    if actual > maximum {
        return Err(GasLimitError::BatchTooLarge);
    }

    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GasLimitError {
    EmptyBatch,
    BatchTooLarge,
}

pub fn batch_invite_limit() -> u32 {
    MAX_BATCH_INVITE_MEMBERS
}

pub fn resolve_vote_limit() -> u32 {
    MAX_RESOLVE_VOTE_ENTRIES
}

/// Test-only measurement helper sketch.
///
/// Use the repository's actual test environment and contract invocation to
/// capture CPU/memory/storage metrics. Avoid using this estimate as a
/// production runtime gas oracle.
#[cfg(test)]
pub fn record_budget_snapshot(env: &Env) {
    let _budget = env.cost_estimate().budget();
    // Add repository-specific assertions/reporting here.
}
