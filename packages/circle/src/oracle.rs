//! Oracle integration for the Circle contract.
//!
//! The circle reads a `yield_rate` (in basis-points) from a configurable
//! oracle contract before each round resolves.  If the **primary** oracle
//! call fails (contract migration, network outage, panic), the code
//! transparently retries against a **fallback** oracle address and emits
//! an `OracleFallbackUsed` event so the indexer can flag the degraded
//! path.
//!
//! Storage keys:
//!   `DataKey::OracleContract`  — primary oracle (optional, set by admin)
//!   `DataKey::FallbackOracle`  — fallback oracle (optional, set by admin)
//!   `DataKey::OraclePubkey`    — Ed25519 pubkey for signed rates (#360)
//!   `DataKey::OracleCache`     — last verified rate + ledger + round (#360)
//!
//! If neither oracle is configured `get_yield_rate` returns `Ok(0)` so
//! that rounds can still complete without yield adjustment.
//!
//! ## Manipulation resistance (#360)
//!
//! When an oracle pubkey is configured (via `set_oracle_pubkey`), unsigned
//! rates are rejected: the oracle must expose `yld_sig(round: u32) ->
//! (rate: i128, sig: BytesN<64>)` where `sig` is an Ed25519 signature over
//! [`build_oracle_message`] (`rate_be16 || round_be4`) by the stored pubkey.
//! The circle verifies the signature with `env.crypto().ed25519_verify`
//! (which traps the transaction on failure — fail-closed) and checks the
//! responding oracle is the stored primary/fallback address (`WrongOracle`
//! otherwise). Verified rates are cached for [`ORACLE_CACHE_TTL`] ledgers;
//! cache hits skip the cross-contract call entirely.

use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Error, IntoVal};

use crate::types::{CircleError, DataKey, OracleCache, OracleFallbackUsed};

/// How long a verified oracle rate may be served from cache, in ledgers
/// (~4 minutes at ~5s/ledger). After the TTL the next read refetches.
pub const ORACLE_CACHE_TTL: u32 = 50;

/// Attempt to call `yield_rate(round: u32) -> i128` on `oracle`.
///
/// Soroban's `invoke_contract` panics on any guest-side error (auth failure,
/// contract not found, trap, etc.).  We wrap it in `try_invoke_contract` which
/// returns a `Result` so we can handle oracle unavailability gracefully.
fn call_oracle(env: &Env, oracle: &Address, round: u32) -> Result<i128, ()> {
    let args = (round,).into_val(env);
    env.try_invoke_contract::<i128, Error>(oracle, &symbol_short!("yld_rate"), args)
        .map_err(|_| ())
        .and_then(|res| res.map_err(|_| ()))
}

/// Attempt to call `yld_sig(round: u32) -> (i128, BytesN<64>)` on `oracle`.
fn call_signed_oracle(
    env: &Env,
    oracle: &Address,
    round: u32,
) -> Result<(i128, BytesN<64>), ()> {
    let args = (round,).into_val(env);
    env.try_invoke_contract::<(i128, BytesN<64>), Error>(oracle, &symbol_short!("yld_sig"), args)
        .map_err(|_| ())
        .and_then(|res| res.map_err(|_| ()))
}

/// Integer square root via Newton's method (no floating point).
///
/// # Truncation behaviour
/// This returns the floor of the true square root. For a value `n` the result
/// satisfies `result² ≤ n < (result+1)²`. The truncation can be up to
/// `2 * sqrt(n)` units in absolute terms, which for large inputs represents
/// a small *relative* error but can exceed a tight 1% acceptance window (see
/// `check_variance`). The acceptance window below is deliberately wider than
/// 1% to account for this.
fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Check whether `reported_value` is within an acceptable variance of the
/// oracle's internal estimate derived from `amount`, `pool_balance`, and
/// `deposit_count`.
///
/// # Formula
/// ```text
/// estimate = isqrt(amount * pool_balance * 100 / deposit_count)
/// ```
///
/// # Acceptance window — 5% (±5%)
/// The window is intentionally **5%** rather than the naive 1% for two
/// reasons:
///
/// 1. **Integer sqrt truncation**: `isqrt` returns the floor of the true
///    square root. For large inputs the truncated value can deviate from the
///    true value by up to ~1 ULP of the root, which can easily exceed 1% of
///    the estimate for big pools.  Concrete example:
///    `amount=999_999, pool_balance=500_000, deposit_count=1`
///    → true sqrt ≈ 7_071_067, isqrt = 7_071_063 (4-unit truncation, ~0.00006%)
///    In general the relative truncation error is `O(1/isqrt(n))`, which stays
///    small for large pools but can spike for small ones — hence the 5% guard.
///
/// 2. **Price feed lag**: oracle price feeds may update slightly behind
///    on-chain settlement, and a strict 1% window would cause spurious
///    rejections during normal market movement.
///
/// # Returns
/// `true` if `reported_value` falls within the 5% tolerance window around the
/// estimate, `false` otherwise.
pub fn check_variance(amount: i128, pool_balance: i128, deposit_count: i128, reported_value: i128) -> bool {
    if deposit_count <= 0 || amount <= 0 || pool_balance <= 0 || reported_value <= 0 {
        return false;
    }
    // Compute the internal estimate using integer arithmetic.
    // Multiply by 100 before taking the root to improve precision by one
    // decimal digit (equivalent to scaling the radicand up).
    let scaled = match amount.checked_mul(pool_balance) {
        Some(v) => v,
        None => return false,
    };
    let scaled = match scaled.checked_mul(100) {
        Some(v) => v,
        None => return false,
    };
    let estimate = isqrt(scaled / deposit_count);
    if estimate <= 0 {
        return false;
    }
    // 5% tolerance window: estimate * 95/100 ≤ reported_value ≤ estimate * 105/100
    // Use integer arithmetic throughout — no floating point.
    let lower = estimate * 95 / 100;
    let upper = estimate * 105 / 100;
    reported_value >= lower && reported_value <= upper
}

// ---------------------------------------------------------------------------
// Signed-rate verification + caching (#360)
// ---------------------------------------------------------------------------

/// Build the exact message the oracle must sign: `rate_be16 || round_be4`.
///
/// Both sides (off-chain signer and this contract) must use this encoding.
/// Signed with raw Ed25519 over these bytes (no extra hashing — the host
/// hashes internally as part of verification).
pub fn build_oracle_message(env: &Env, rate: i128, round: u32) -> Bytes {
    let mut msg = Bytes::new(env);
    msg.extend_from_array(&rate.to_be_bytes());
    msg.extend_from_array(&round.to_be_bytes());
    msg
}

/// Verify `signature` over [`build_oracle_message`] with `pubkey`.
///
/// Returns `Ok(())` on success. On failure the host traps the transaction
/// (fail-closed): `ed25519_verify` panics rather than returning false, so an
/// invalid signature can never be mistaken for a valid zero rate.
pub fn verify_oracle_signature(
    env: &Env,
    pubkey: &BytesN<32>,
    rate: i128,
    round: u32,
    signature: &BytesN<64>,
) -> Result<(), CircleError> {
    let msg = build_oracle_message(env, rate, round);
    env.crypto().ed25519_verify(pubkey, &msg, signature);
    Ok(())
}

/// Reject oracle responses that did not come from the stored primary or
/// fallback address. `get_yield_rate` only ever calls stored addresses, so a
/// mismatch here means storage was tampered with or the caller is probing an
/// arbitrary oracle.
pub fn validate_oracle_source(env: &Env, responding_oracle: &Address) -> Result<(), CircleError> {
    let primary: Option<Address> = env.storage().instance().get(&DataKey::OracleContract);
    let fallback: Option<Address> = env.storage().instance().get(&DataKey::FallbackOracle);
    if primary.as_ref() == Some(responding_oracle) || fallback.as_ref() == Some(responding_oracle) {
        Ok(())
    } else {
        Err(CircleError::WrongOracle)
    }
}

/// Return the cached rate for `round` when it is still within TTL.
pub fn get_cached_rate(env: &Env, round: u32) -> Option<i128> {
    let cache: OracleCache = env.storage().instance().get(&DataKey::OracleCache)?;
    if cache.round != round {
        return None;
    }
    let current = env.ledger().sequence();
    let age = current.checked_sub(cache.ledger)?;
    if age <= ORACLE_CACHE_TTL {
        Some(cache.rate)
    } else {
        None
    }
}

fn write_cache(env: &Env, rate: i128, round: u32) {
    env.storage().instance().set(
        &DataKey::OracleCache,
        &OracleCache {
            rate,
            ledger: env.ledger().sequence(),
            round,
        },
    );
}

/// Fetch a rate from `oracle`, requiring an Ed25519 signature when a pubkey
/// is configured. Unsigned legacy path (`yld_rate`) is used only while no
/// pubkey is set.
fn fetch_verified_rate(env: &Env, oracle: &Address, round: u32) -> Result<i128, CircleError> {
    validate_oracle_source(env, oracle)?;
    let pubkey: Option<BytesN<32>> = env.storage().instance().get(&DataKey::OraclePubkey);
    match pubkey {
        Some(pk) => {
            let (rate, sig) =
                call_signed_oracle(env, oracle, round).map_err(|_| CircleError::InvalidOracleSignature)?;
            verify_oracle_signature(env, &pk, rate, round, &sig)
                .map_err(|_| CircleError::InvalidOracleSignature)?;
            Ok(rate)
        }
        None => call_oracle(env, oracle, round).map_err(|_| CircleError::OracleUnavailable),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve the yield rate for `round`.
///
/// Resolution order:
///   1. Fresh cache hit (same round, within TTL) → cached rate, no oracle call
///   2. No oracle configured  → `Ok(0)` (zero yield, round proceeds normally)
///   3. Primary oracle call succeeds (signature-checked when a pubkey is set)
///      → `Ok(rate_bps)` + cache write
///   4. Primary fails, fallback configured and succeeds → `Ok(rate_bps)` +
///      emits `OracleFallbackUsed` event + cache write
///   5. Both fail / fallback absent → `Err(CircleError::OracleUnavailable)`
pub fn get_yield_rate(env: &Env, round: u32) -> Result<i128, CircleError> {
    if let Some(cached) = get_cached_rate(env, round) {
        return Ok(cached);
    }
    let primary: Option<Address> = env.storage().instance().get(&DataKey::OracleContract);

    let primary_addr = match primary {
        None => return Ok(0), // No oracle configured — proceed with zero yield.
        Some(addr) => addr,
    };

    // Try primary oracle.
    if let Ok(rate) = fetch_verified_rate(env, &primary_addr, round) {
        write_cache(env, rate, round);
        return Ok(rate);
    }
    // Distinguish "signature present but invalid" (fail closed, no fallback)
    // from "oracle unreachable" (fallback allowed): only fall back when the
    // primary is unsigned-or-unreachable. When a pubkey is set, any primary
    // response at all must verify — a bad signature must not silently
    // degrade to the fallback oracle.
    let pubkey_set: bool = env.storage().instance().has(&DataKey::OraclePubkey);
    if pubkey_set {
        return Err(CircleError::InvalidOracleSignature);
    }

    // Primary failed — attempt fallback.
    let fallback: Option<Address> = env.storage().instance().get(&DataKey::FallbackOracle);

    let fallback_addr = match fallback {
        None => return Err(CircleError::OracleUnavailable),
        Some(addr) => addr,
    };

    match fetch_verified_rate(env, &fallback_addr, round) {
        Ok(rate) => {
            // Emit degraded-path event for indexer observability.
            env.events().publish(
                (env.current_contract_address(), symbol_short!("orc_fall")),
                OracleFallbackUsed {
                    round,
                    primary_oracle: primary_addr,
                    fallback_oracle: fallback_addr,
                },
            );
            write_cache(env, rate, round);
            Ok(rate)
        }
        Err(_) => Err(CircleError::OracleUnavailable),
    }
}

/// Store the primary oracle address.  Caller must enforce admin auth.
pub fn set_primary_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&DataKey::OracleContract, oracle);
}

/// Store the fallback oracle address.  Caller must enforce admin auth.
pub fn set_fallback_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&DataKey::FallbackOracle, oracle);
}

/// Retrieve the currently configured primary oracle, if any.
pub fn get_primary_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::OracleContract)
}

/// Retrieve the currently configured fallback oracle, if any.
pub fn get_fallback_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::FallbackOracle)
}

/// Store the Ed25519 pubkey that signs oracle rates. Caller must enforce admin auth.
pub fn set_oracle_pubkey(env: &Env, pubkey: &BytesN<32>) {
    env.storage().instance().set(&DataKey::OraclePubkey, pubkey);
}

/// Retrieve the configured oracle signing pubkey, if any.
pub fn get_oracle_pubkey(env: &Env) -> Option<BytesN<32>> {
    env.storage().instance().get(&DataKey::OraclePubkey)
}

/// Retrieve the current oracle rate cache, if any.
pub fn get_oracle_cache(env: &Env) -> Option<OracleCache> {
    env.storage().instance().get(&DataKey::OracleCache)
}
