#![cfg(test)]

use proptest::prelude::*;

use crate::math::{apply_fee, calculate_penalty, calculate_percentage, convert_shares, MathError};

proptest! {
    #[test]
    fn apply_fee_preserves_total(amount in 0_i128..=(i128::MAX / 10_000), fee_bps in 0_i128..=10_000) {
        let (net, fee) = apply_fee(amount, fee_bps).expect("bounded inputs should not overflow");
        prop_assert_eq!(net + fee, amount);
        prop_assert!(net >= 0);
        prop_assert!(fee >= 0);
        prop_assert!(fee <= amount);
    }

    #[test]
    fn calculate_percentage_stays_within_amount(amount in 0_i128..=(i128::MAX / 10_000), bps in 0_i128..=10_000) {
        let percentage = calculate_percentage(amount, bps).expect("bounded inputs should not overflow");
        prop_assert!(percentage >= 0);
        prop_assert!(percentage <= amount);
    }

    #[test]
    fn calculate_penalty_never_exceeds_total(amount in 0_i128..=(i128::MAX / 10_000), penalty_bps in 0_i128..=10_000) {
        let penalty = calculate_penalty(amount, penalty_bps).expect("bounded inputs should not overflow");
        prop_assert!(penalty >= 0);
        prop_assert!(penalty <= amount);
    }

    /// convert_shares must never panic for any non-zero total_shares.
    #[test]
    fn convert_shares_no_panic_nonzero_total(
        member_shares in 0_i128..=1_000_000_i128,
        total_shares  in 1_i128..=1_000_000_i128,
        pool_amount   in 0_i128..=(i128::MAX / 1_000_000_i128),
    ) {
        let result = convert_shares(member_shares, total_shares, pool_amount);
        // Must never panic — always returns Ok or a typed error.
        match result {
            Ok(v)  => prop_assert!(v >= 0),
            Err(e) => prop_assert_eq!(e, MathError::Overflow),
        }
    }
}

// ── Deterministic unit tests for convert_shares ──────────────────────────────

#[test]
fn convert_shares_zero_total_returns_division_by_zero() {
    // GUARD: total_shares == 0 must return DivisionByZero, not panic.
    let result = convert_shares(100, 0, 1_000);
    assert_eq!(result, Err(MathError::DivisionByZero));
}

#[test]
fn convert_shares_zero_pool_returns_zero() {
    // Pool is empty — every member gets 0 regardless of their share count.
    assert_eq!(convert_shares(50, 100, 0), Ok(0));
}

#[test]
fn convert_shares_equal_shares_splits_evenly() {
    // 5 members each holding 20 out of 100 shares, pool = 1000.
    // Each member should receive 200.
    assert_eq!(convert_shares(20, 100, 1_000), Ok(200));
}

#[test]
fn convert_shares_all_shares_to_one_member() {
    // Single member holds all shares — should receive the entire pool.
    assert_eq!(convert_shares(100, 100, 5_000), Ok(5_000));
}

#[test]
fn convert_shares_zero_member_shares_returns_zero() {
    // Member with no shares gets nothing.
    assert_eq!(convert_shares(0, 100, 1_000), Ok(0));
}

#[test]
fn convert_shares_single_share_of_many() {
    // 1 share out of 1_000_000, pool = 1_000_000 → member gets 1.
    assert_eq!(convert_shares(1, 1_000_000, 1_000_000), Ok(1));
}

#[test]
fn convert_shares_overflow_on_huge_inputs() {
    // member_shares * pool_amount overflows i128 → must return Overflow, not panic.
    let result = convert_shares(i128::MAX, 1, i128::MAX);
    assert_eq!(result, Err(MathError::Overflow));
}

// ── Bitmap encode / decode / bounds unit tests (#438) ───────────────────────

use crate::bitmap::{clear_bit, count_set_bits, encode_bit, first_unset_bit, is_set, set_bit, BitmapError, MAX_BITMAP_CAPACITY};

#[test]
fn bitmap_encode_and_decode_valid_indices() {
    for i in 0..MAX_BITMAP_CAPACITY {
        let mask = encode_bit(i).expect("valid index must encode");
        assert_eq!(mask, 1u128 << i);
        assert!(is_set(mask, i).expect("must be set"));
        assert_eq!(count_set_bits(mask), 1);
    }
}

#[test]
fn bitmap_set_and_clear_bit() {
    let mut bitmap: u128 = 0;
    assert!(!is_set(bitmap, 0).unwrap());
    assert!(!is_set(bitmap, 63).unwrap());
    assert!(!is_set(bitmap, 127).unwrap());

    set_bit(&mut bitmap, 0).unwrap();
    set_bit(&mut bitmap, 63).unwrap();
    set_bit(&mut bitmap, 127).unwrap();

    assert!(is_set(bitmap, 0).unwrap());
    assert!(is_set(bitmap, 63).unwrap());
    assert!(is_set(bitmap, 127).unwrap());
    assert!(!is_set(bitmap, 1).unwrap());
    assert_eq!(count_set_bits(bitmap), 3);

    clear_bit(&mut bitmap, 63).unwrap();
    assert!(!is_set(bitmap, 63).unwrap());
    assert!(is_set(bitmap, 0).unwrap());
    assert!(is_set(bitmap, 127).unwrap());
    assert_eq!(count_set_bits(bitmap), 2);
}

#[test]
fn bitmap_first_unset_bit() {
    let mut bitmap: u128 = 0;
    assert_eq!(first_unset_bit(bitmap, 10).unwrap(), Some(0));

    set_bit(&mut bitmap, 0).unwrap();
    set_bit(&mut bitmap, 1).unwrap();
    assert_eq!(first_unset_bit(bitmap, 10).unwrap(), Some(2));

    for i in 2..10 {
        set_bit(&mut bitmap, i).unwrap();
    }
    assert_eq!(first_unset_bit(bitmap, 10).unwrap(), None);
}

#[test]
fn bitmap_bounds_validation_rejects_greater_than_128_members() {
    // Tests u128 overflow with >128 members.
    // Index 128 is the first out-of-bounds index for a 128-bit bitmap.
    let mut bitmap: u128 = 0;

    assert_eq!(encode_bit(128), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(encode_bit(129), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(encode_bit(200), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(encode_bit(u32::MAX), Err(BitmapError::IndexOutOfBounds));

    assert_eq!(is_set(bitmap, 128), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(is_set(bitmap, 256), Err(BitmapError::IndexOutOfBounds));

    assert_eq!(set_bit(&mut bitmap, 128), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(set_bit(&mut bitmap, 1000), Err(BitmapError::IndexOutOfBounds));

    assert_eq!(clear_bit(&mut bitmap, 128), Err(BitmapError::IndexOutOfBounds));
    assert_eq!(first_unset_bit(bitmap, 129), Err(BitmapError::IndexOutOfBounds));
}

