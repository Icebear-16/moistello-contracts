//! N-of-M multi-signature helpers shared across Moistello contracts (#359).
//!
//! High-value circles should not be controlled by a single admin key.
//! This module provides the reusable verification primitive; each contract
//! stores its own admin set + threshold and calls [`check_approval`] from
//! its critical entry points.
//!
//! Semantics (single-transaction co-signing):
//!   * The caller passes the full `signers` set for this call.
//!   * Every signer must be a member of the stored admin set and must have
//!     authorized this invocation (`require_auth` — verified by the host).
//!   * Duplicate signers do not count twice.
//!   * The call succeeds iff unique valid signers `>= threshold`.
//!
//! When no multisig is configured, contracts keep their existing single-admin
//! path unchanged. Once configured, single-sig critical paths must be
//! disabled by the contract (fail closed) so the threshold cannot be
//! bypassed.

use soroban_sdk::{contracterror, Address, Vec};

/// Hard cap on the admin set size — keeps membership checks O(n²) cheap.
pub const MAX_ADMINS: u32 = 10;

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultisigError {
    /// Threshold/admin-set combination is invalid.
    InvalidConfig = 1,
    /// A signer is not in the stored admin set.
    NotAdmin = 2,
    /// Fewer unique admin signatures than the threshold.
    ThresholdNotMet = 3,
    /// Same signer listed twice.
    DuplicateSigner = 4,
}

/// Validate a prospective `(admins, threshold)` configuration.
pub fn validate_config(admins: &Vec<Address>, threshold: u32) -> Result<(), MultisigError> {
    let n = admins.len();
    if n == 0 || n > MAX_ADMINS {
        return Err(MultisigError::InvalidConfig);
    }
    if threshold == 0 || threshold > n {
        return Err(MultisigError::InvalidConfig);
    }
    for i in 0..n {
        let a = admins.get(i).ok_or(MultisigError::InvalidConfig)?;
        for j in (i + 1)..n {
            let b = admins.get(j).ok_or(MultisigError::InvalidConfig)?;
            if a == b {
                return Err(MultisigError::InvalidConfig);
            }
        }
    }
    Ok(())
}

/// True when `who` is in the stored admin set.
pub fn is_admin(admins: &Vec<Address>, who: &Address) -> bool {
    for i in 0..admins.len() {
        if let Some(a) = admins.get(i) {
            if &a == who {
                return true;
            }
        }
    }
    false
}

/// Verify N-of-M approval: every signer must be an admin, must be unique,
/// and unique count must reach `threshold`.
///
/// Pure membership/count check with no auth side effects, so it works both
/// inside contract calls and in unit tests. Callers perform `require_auth`
/// per approver in their own transaction (each approver authorizes their own
/// `approve` call as invoker) — never nest `require_auth` for non-invokers.
pub fn verify_signers(
    admins: &Vec<Address>,
    threshold: u32,
    signers: &Vec<Address>,
) -> Result<(), MultisigError> {
    validate_config(admins, threshold)?;
    if signers.len() == 0 || signers.len() > admins.len() {
        return Err(MultisigError::ThresholdNotMet);
    }
    // Uniqueness + membership (cheap, no host calls).
    for i in 0..signers.len() {
        let s = signers.get(i).ok_or(MultisigError::ThresholdNotMet)?;
        for j in (i + 1)..signers.len() {
            let t = signers.get(j).ok_or(MultisigError::ThresholdNotMet)?;
            if s == t {
                return Err(MultisigError::DuplicateSigner);
            }
        }
        if !is_admin(admins, &s) {
            return Err(MultisigError::NotAdmin);
        }
    }
    if signers.len() < threshold {
        return Err(MultisigError::ThresholdNotMet);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;
    use soroban_sdk::testutils::Address as _;

    fn admins(env: &Env, n: u32) -> Vec<Address> {
        let mut v = Vec::new(env);
        for _ in 0..n {
            v.push_back(Address::generate(env));
        }
        v
    }

    #[test]
    fn config_validation() {
        let env = Env::default();
        let a = admins(&env, 3);
        assert!(validate_config(&a, 1).is_ok());
        assert!(validate_config(&a, 2).is_ok());
        assert!(validate_config(&a, 3).is_ok());
        assert_eq!(validate_config(&a, 0), Err(MultisigError::InvalidConfig));
        assert_eq!(validate_config(&a, 4), Err(MultisigError::InvalidConfig));
        let empty = Vec::new(&env);
        assert_eq!(
            validate_config(&empty, 1),
            Err(MultisigError::InvalidConfig)
        );
        // Duplicate admin rejected.
        let mut dup = Vec::new(&env);
        let one = Address::generate(&env);
        dup.push_back(one.clone());
        dup.push_back(one.clone());
        assert_eq!(
            validate_config(&dup, 1),
            Err(MultisigError::InvalidConfig)
        );
    }

    #[test]
    fn single_sig_threshold_passes() {
        let env = Env::default();
        let a = admins(&env, 1);
        let mut signers = Vec::new(&env);
        signers.push_back(a.get(0).unwrap());
        assert!(verify_signers(&a, 1, &signers).is_ok());
    }

    #[test]
    fn threshold_enforced_and_duplicates_rejected() {
        let env = Env::default();
        let a = admins(&env, 3);
        // 1-of-3 with one signer: ok.
        let mut one = Vec::new(&env);
        one.push_back(a.get(0).unwrap());
        assert!(verify_signers(&a, 1, &one).is_ok());
        // 2-of-3 with one signer: threshold not met.
        assert_eq!(
            verify_signers(&a, 2, &one),
            Err(MultisigError::ThresholdNotMet)
        );
        // 2-of-3 with two distinct signers: ok.
        let mut two = Vec::new(&env);
        two.push_back(a.get(0).unwrap());
        two.push_back(a.get(1).unwrap());
        assert!(verify_signers(&a, 2, &two).is_ok());
        // Duplicate signer: rejected even though len >= threshold.
        let mut dup = Vec::new(&env);
        dup.push_back(a.get(0).unwrap());
        dup.push_back(a.get(0).unwrap());
        assert_eq!(
            verify_signers(&a, 2, &dup),
            Err(MultisigError::DuplicateSigner)
        );
        // Non-admin signer: rejected.
        let mut outsider = Vec::new(&env);
        outsider.push_back(Address::generate(&env));
        outsider.push_back(a.get(0).unwrap());
        assert_eq!(
            verify_signers(&a, 2, &outsider),
            Err(MultisigError::NotAdmin)
        );
    }
}
