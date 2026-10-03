use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathError {
    Overflow = 1,
    Underflow = 2,
    DivisionByZero = 3,
}
pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}
pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}
pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

// ── Primitive checked arithmetic ─────────────────────────────────────────────

pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}

pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

pub fn safe_div(a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    a.checked_div(b).ok_or(MathError::Overflow)
}

pub fn calculate_percentage(amount: i128, bps: i128) -> Result<i128, MathError> {
    if bps < 0 || bps > 10_000 {
        return Err(MathError::Overflow);
    }
    safe_div(safe_mul(amount, bps)?, 10_000)
}

pub fn apply_fee(amount: i128, fee_bps: i128) -> Result<(i128, i128), MathError> {
    let fee = calculate_percentage(amount, fee_bps)?;
    Ok((safe_sub(amount, fee)?, fee))
}
pub fn calculate_penalty(amount: i128, penalty_bps: i128) -> Result<i128, MathError> {
    calculate_percentage(amount, penalty_bps)
}

pub fn calculate_penalty(amount: i128, penalty_bps: i128) -> Result<i128, MathError> {
    calculate_percentage(amount, penalty_bps)
}

// ── Fixed-point arithmetic: 7 decimal places ─────────────────────────────────

/// The fixed-point scalar: 10^7.  All financial values are stored and
/// manipulated as integer multiples of this constant so that 1.0 is
/// represented as 10_000_000, matching Stellar's standard 7-decimal
/// precision for stroops-equivalent amounts.
pub const SCALAR: i128 = 10_000_000;

/// Adds two fixed-point values.
///
/// Both `a` and `b` must already be in fixed-point representation
/// (i.e. scaled by `SCALAR`).  Returns `MathError::Overflow` if the result
/// would overflow `i128`.
pub fn fp_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

/// Subtracts `b` from `a` in fixed-point.
///
/// Returns `MathError::Underflow` if the result would underflow `i128`.
pub fn fp_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}

/// Multiplies two fixed-point values and re-scales by `SCALAR`.
///
/// The intermediate product `a * b` is first computed exactly in `i128`; the
/// result is then divided by `SCALAR` to keep the output in the same
/// fixed-point domain.  Returns `MathError::Overflow` if the intermediate
/// product overflows `i128`.
pub fn fp_mul(a: i128, b: i128) -> Result<i128, MathError> {
    let product = a.checked_mul(b).ok_or(MathError::Overflow)?;
    product.checked_div(SCALAR).ok_or(MathError::Overflow)
}

/// Divides `a` by `b` in fixed-point.
///
/// Pre-scales `a` by `SCALAR` before dividing so that the quotient is
/// expressed in the same fixed-point domain.  Returns
/// `MathError::DivisionByZero` when `b == 0` and `MathError::Overflow` if the
/// pre-scaled numerator overflows `i128`.
pub fn fp_div(a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    let scaled = a.checked_mul(SCALAR).ok_or(MathError::Overflow)?;
    scaled.checked_div(b).ok_or(MathError::Overflow)
}

// ── Shares / proportional distribution ───────────────────────────────────────

/// Converts a member's individual shares into a proportional token amount from
/// a pool.
///
/// # Arguments
/// * `member_shares` – the number of shares attributed to the member (must be ≥ 0)
/// * `total_shares`  – the total shares outstanding across all members
/// * `pool_amount`   – the total token amount to be distributed
///
/// # Errors
/// Returns [`MathError::DivisionByZero`] when `total_shares` is zero (pool has
/// no share-holders), preventing a panic inside the contract host.
/// Returns [`MathError::Overflow`] / [`MathError::Underflow`] on arithmetic
/// overflow / underflow in intermediate computations.
pub fn convert_shares(
    member_shares: i128,
    total_shares: i128,
    pool_amount: i128,
) -> Result<i128, MathError> {
    // Guard: total_shares == 0 would cause a division-by-zero panic on-chain.
    // This can happen when a vault or pool loses all members due to a rounding
    // edge case.  Returning a typed error lets the caller handle this gracefully
    // instead of trapping the entire host execution.
    if total_shares == 0 {
        return Err(MathError::DivisionByZero);
    }
    // member_payout = (member_shares * pool_amount) / total_shares
    // Multiplication is performed first to preserve precision; overflow is
    // checked explicitly via safe_mul.
    let numerator = safe_mul(member_shares, pool_amount)?;
    safe_div(numerator, total_shares)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod fp_tests {
    use super::*;

    // ── fp_add ────────────────────────────────────────────────────────────────

    #[test]
    fn fp_add_happy_path() {
        // 1.5000000 + 2.5000000 = 4.0000000
        assert_eq!(fp_add(15_000_000, 25_000_000), Ok(40_000_000));
    }

    #[test]
    fn fp_add_zero_identity() {
        assert_eq!(fp_add(0, 0), Ok(0));
        assert_eq!(fp_add(SCALAR, 0), Ok(SCALAR));
        assert_eq!(fp_add(0, SCALAR), Ok(SCALAR));
    }

    #[test]
    fn fp_add_negative_values() {
        // (-1.0) + 3.0 = 2.0
        assert_eq!(fp_add(-SCALAR, 3 * SCALAR), Ok(2 * SCALAR));
    }

    #[test]
    fn fp_add_overflow_returns_error() {
        assert_eq!(fp_add(i128::MAX, 1), Err(MathError::Overflow));
    }

    #[test]
    fn fp_add_max_negative_overflow() {
        assert_eq!(fp_add(i128::MIN, -1), Err(MathError::Overflow));
    }

    // ── fp_sub ────────────────────────────────────────────────────────────────

    #[test]
    fn fp_sub_happy_path() {
        // 5.0000000 - 3.0000000 = 2.0000000
        assert_eq!(fp_sub(5 * SCALAR, 3 * SCALAR), Ok(2 * SCALAR));
    }

    #[test]
    fn fp_sub_zero_identity() {
        assert_eq!(fp_sub(SCALAR, 0), Ok(SCALAR));
        assert_eq!(fp_sub(0, 0), Ok(0));
    }

    #[test]
    fn fp_sub_result_zero() {
        assert_eq!(fp_sub(SCALAR, SCALAR), Ok(0));
    }

    #[test]
    fn fp_sub_underflow_returns_error() {
        assert_eq!(fp_sub(i128::MIN, 1), Err(MathError::Underflow));
    }

    #[test]
    fn fp_sub_negative_result_allowed() {
        // 1.0 - 3.0 = -2.0 (valid in i128)
        assert_eq!(fp_sub(SCALAR, 3 * SCALAR), Ok(-2 * SCALAR));
    }

    // ── fp_mul ────────────────────────────────────────────────────────────────

    #[test]
    fn fp_mul_happy_path() {
        // 2.0 * 3.0 = 6.0
        assert_eq!(fp_mul(2 * SCALAR, 3 * SCALAR), Ok(6 * SCALAR));
    }

    #[test]
    fn fp_mul_fractional() {
        // 1.5 * 2.0 = 3.0  (15_000_000 * 20_000_000 / SCALAR)
        assert_eq!(fp_mul(15_000_000, 20_000_000), Ok(30_000_000));
    }

    #[test]
    fn fp_mul_by_zero() {
        assert_eq!(fp_mul(SCALAR, 0), Ok(0));
        assert_eq!(fp_mul(0, SCALAR), Ok(0));
    }

    #[test]
    fn fp_mul_by_one_identity() {
        // x * 1.0 = x
        assert_eq!(fp_mul(12_345_678, SCALAR), Ok(12_345_678));
    }

    #[test]
    fn fp_mul_overflow_returns_error() {
        // Both operands near i128::MAX will overflow the intermediate product.
        assert_eq!(fp_mul(i128::MAX, i128::MAX), Err(MathError::Overflow));
    }

    #[test]
    fn fp_mul_large_but_within_bounds() {
        // 1_000.0 * 1_000.0 = 1_000_000.0
        let a = 1_000 * SCALAR;
        let b = 1_000 * SCALAR;
        assert_eq!(fp_mul(a, b), Ok(1_000_000 * SCALAR));
    }

    // ── fp_div ────────────────────────────────────────────────────────────────

    #[test]
    fn fp_div_happy_path() {
        // 6.0 / 2.0 = 3.0
        assert_eq!(fp_div(6 * SCALAR, 2 * SCALAR), Ok(3 * SCALAR));
    }

    #[test]
    fn fp_div_fractional_result() {
        // 1.0 / 4.0 = 0.25  → 2_500_000 in fixed-point
        assert_eq!(fp_div(SCALAR, 4 * SCALAR), Ok(2_500_000));
    }

    #[test]
    fn fp_div_by_one_identity() {
        // x / 1.0 = x
        assert_eq!(fp_div(7 * SCALAR, SCALAR), Ok(7 * SCALAR));
    }

    #[test]
    fn fp_div_by_zero_returns_error() {
        assert_eq!(fp_div(SCALAR, 0), Err(MathError::DivisionByZero));
        assert_eq!(fp_div(0, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn fp_div_zero_numerator() {
        assert_eq!(fp_div(0, SCALAR), Ok(0));
    }

    #[test]
    fn fp_div_overflow_on_prescale() {
        // Scaling i128::MAX by SCALAR would overflow the numerator.
        assert_eq!(fp_div(i128::MAX, SCALAR), Err(MathError::Overflow));
    }

    #[test]
    fn fp_div_negative_dividend() {
        // -6.0 / 2.0 = -3.0
        assert_eq!(fp_div(-6 * SCALAR, 2 * SCALAR), Ok(-3 * SCALAR));
    }

    // ── precision round-trip ──────────────────────────────────────────────────

    #[test]
    fn fp_mul_then_div_round_trips() {
        // (a * b) / b == a when no rounding loss occurs.
        let a = 5 * SCALAR;
        let b = 4 * SCALAR;
        let product = fp_mul(a, b).unwrap(); // 20.0
        let result = fp_div(product, b).unwrap(); // 20.0 / 4.0 = 5.0
        assert_eq!(result, a);
    }

    #[test]
    fn fp_add_sub_inverse() {
        let a = 123_456_789_i128;
        let b = 987_654_321_i128;
        let sum = fp_add(a, b).unwrap();
        let back = fp_sub(sum, b).unwrap();
        assert_eq!(back, a);
    }

    // ── SCALAR constant ───────────────────────────────────────────────────────

    #[test]
    fn scalar_is_ten_million() {
        assert_eq!(SCALAR, 10_000_000);
    }

    #[test]
    fn scalar_represents_one() {
        // Multiplying two "1.0" values must yield "1.0".
        assert_eq!(fp_mul(SCALAR, SCALAR), Ok(SCALAR));
    }
}
