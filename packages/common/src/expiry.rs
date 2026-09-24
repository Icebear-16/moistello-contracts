//! Transaction expiry helpers shared across Moistello contracts (#358).
//!
//! ## Expiry policy
//!
//! Stale transactions — signed long ago and executed much later, possibly in
//! different market conditions — must not execute. Every time-sensitive
//! entry point therefore accepts a `valid_until_ledger` bound (a ledger
//! sequence number) and rejects the call when the current ledger has moved
//! past it:
//!
//! ```text
//! require(current_ledger <= valid_until_ledger), else TxExpired
//! ```
//!
//! Bounds:
//!   * `valid_until_ledger == 0` is never valid (`InvalidBound`) — callers
//!     must supply an explicit bound.
//!   * `DEFAULT_MAX_TX_AGE_LEDGERS = 100` documents the recommended maximum
//!     age: a transaction should execute within ~100 ledgers of signing
//!     (~8 minutes at ~5s/ledger). `check_tx_age` enforces this window given
//!     the ledger the transaction was signed at.
//!
//! Client guidance: set `valid_until_ledger = current_ledger + N` with a
//! small `N` (e.g. 20–50) when submitting; the contract enforces the bound
//! on-chain via `env.ledger().sequence()`.

use soroban_sdk::{contracterror, Env};

/// Recommended maximum age of a signed transaction, in ledgers (~8 min).
pub const DEFAULT_MAX_TX_AGE_LEDGERS: u32 = 100;

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpiryError {
    /// Current ledger is past the caller-supplied `valid_until_ledger`.
    Expired = 1,
    /// Bound is unusable (`valid_until_ledger == 0`, or a `submitted_at`
    /// ledger in the future).
    InvalidBound = 2,
}

/// Reject the call when the current ledger exceeds `valid_until_ledger`.
pub fn check_tx_expiry(env: &Env, valid_until_ledger: u32) -> Result<(), ExpiryError> {
    if valid_until_ledger == 0 {
        return Err(ExpiryError::InvalidBound);
    }
    let current = env.ledger().sequence();
    if current > valid_until_ledger {
        return Err(ExpiryError::Expired);
    }
    Ok(())
}

/// Reject when more than `max_age_ledgers` have elapsed since
/// `submitted_at_ledger`.
pub fn check_tx_age(
    env: &Env,
    submitted_at_ledger: u32,
    max_age_ledgers: u32,
) -> Result<(), ExpiryError> {
    let current = env.ledger().sequence();
    if submitted_at_ledger > current {
        return Err(ExpiryError::InvalidBound);
    }
    let age = current - submitted_at_ledger;
    if age > max_age_ledgers {
        return Err(ExpiryError::Expired);
    }
    Ok(())
}

/// Same as [`check_tx_age`] with [`DEFAULT_MAX_TX_AGE_LEDGERS`].
pub fn check_tx_age_default(env: &Env, submitted_at_ledger: u32) -> Result<(), ExpiryError> {
    check_tx_age(env, submitted_at_ledger, DEFAULT_MAX_TX_AGE_LEDGERS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger as _;

    #[test]
    fn fresh_bound_passes() {
        let env = Env::default();
        env.ledger().set_sequence_number(1_000);
        let cur = env.ledger().sequence();
        assert!(check_tx_expiry(&env, cur).is_ok());
        assert!(check_tx_expiry(&env, cur + 50).is_ok());
    }

    #[test]
    fn expired_bound_rejected() {
        let env = Env::default();
        let cur = env.ledger().sequence();
        env.ledger().set_sequence_number(cur + 101);
        assert_eq!(
            check_tx_expiry(&env, cur + 100),
            Err(ExpiryError::Expired)
        );
    }

    #[test]
    fn zero_bound_rejected() {
        let env = Env::default();
        assert_eq!(check_tx_expiry(&env, 0), Err(ExpiryError::InvalidBound));
    }

    #[test]
    fn age_window_enforced() {
        let env = Env::default();
        let start = env.ledger().sequence();
        assert!(check_tx_age_default(&env, start).is_ok());
        env.ledger()
            .set_sequence_number(start + DEFAULT_MAX_TX_AGE_LEDGERS);
        assert!(check_tx_age_default(&env, start).is_ok());
        env.ledger()
            .set_sequence_number(start + DEFAULT_MAX_TX_AGE_LEDGERS + 1);
        assert_eq!(
            check_tx_age_default(&env, start),
            Err(ExpiryError::Expired)
        );
        // Submitted-at in the future is a caller bug, not an expiry.
        assert_eq!(
            check_tx_age(&env, start + 10_000, 100),
            Err(ExpiryError::InvalidBound)
        );
    }
}
