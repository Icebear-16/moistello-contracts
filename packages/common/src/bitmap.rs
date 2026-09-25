use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BitmapError {
    IndexOutOfBounds = 1,
}

pub const MAX_BITMAP_CAPACITY: u32 = 128;

/// Validates that the bit position is within bounds for a u128 bitmap (< 128).
pub fn validate_index(index: u32) -> Result<(), BitmapError> {
    if index >= MAX_BITMAP_CAPACITY {
        Err(BitmapError::IndexOutOfBounds)
    } else {
        Ok(())
    }
}

/// Encodes a single bitmask corresponding to the specified index.
/// Returns `Err(BitmapError::IndexOutOfBounds)` if index >= 128.
pub fn encode_bit(index: u32) -> Result<u128, BitmapError> {
    validate_index(index)?;
    Ok(1u128 << index)
}

/// Checks if the bit at the given index is set in the bitmap.
/// Returns `Err(BitmapError::IndexOutOfBounds)` if index >= 128.
pub fn is_set(bitmap: u128, index: u32) -> Result<bool, BitmapError> {
    let mask = encode_bit(index)?;
    Ok((bitmap & mask) != 0)
}

/// Sets the bit at the given index in the bitmap.
/// Returns `Err(BitmapError::IndexOutOfBounds)` if index >= 128.
pub fn set_bit(bitmap: &mut u128, index: u32) -> Result<(), BitmapError> {
    let mask = encode_bit(index)?;
    *bitmap |= mask;
    Ok(())
}

/// Clears the bit at the given index in the bitmap.
/// Returns `Err(BitmapError::IndexOutOfBounds)` if index >= 128.
pub fn clear_bit(bitmap: &mut u128, index: u32) -> Result<(), BitmapError> {
    let mask = encode_bit(index)?;
    *bitmap &= !mask;
    Ok(())
}

/// Counts total set bits in the bitmap.
pub fn count_set_bits(bitmap: u128) -> u32 {
    bitmap.count_ones()
}

/// Finds the first unset bit within [0, bound).
/// Returns `None` if all bits in the range are set.
pub fn first_unset_bit(bitmap: u128, bound: u32) -> Result<Option<u32>, BitmapError> {
    if bound > MAX_BITMAP_CAPACITY {
        return Err(BitmapError::IndexOutOfBounds);
    }
    for i in 0..bound {
        if !is_set(bitmap, i)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}
