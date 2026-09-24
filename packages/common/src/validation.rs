//! Address validation helpers shared across Moistello contracts (#356).
//!
//! On-chain, every `Address` argument has already been decoded from its
//! Stellar strkey representation by the host, which enforces the full
//! strkey rules (56 characters, `G`/`C` prefix, base-32 alphabet, CRC-16
//! checksum). There is therefore no way to receive a malformed checksum
//! inside contract code — malformed inputs fail at ingress before any
//! contract logic runs.
//!
//! What contracts *can* and *must* still check (this module):
//!   1. Structural shape — 56 chars, `G` (account) or `C` (contract) prefix,
//!      base-32 alphabet. Defense-in-depth in case host behaviour changes.
//!   2. Zero-address rejection — the all-zero Ed25519 key
//!      (`GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF`, the
//!      strkey encoding of 32 zero bytes) is never a legitimate member,
//!      admin, oracle, token, or treasury.
//!
//! Off-chain clients must additionally validate strkeys *before* submission
//! (G-prefix, 56 chars, checksum) using their Stellar SDK; see
//! `validate_strkey` for the on-chain mirror of those rules.

use soroban_sdk::{contracterror, Address, Env, String};

/// Strkey encoding of 32 zero bytes (version byte `G` + zero payload +
/// CRC-16 checksum). Never a legitimate signer or account.
pub const ZERO_G_STRKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Address fails structural checks (length, prefix, or alphabet).
    InvalidAddress = 1,
    /// Address is the all-zero key — rejected everywhere.
    ZeroAddress = 2,
}

fn is_base32(b: u8) -> bool {
    (b'A'..=b'Z').contains(&b) || (b'2'..=b'7').contains(&b)
}

/// Validate a raw strkey `String` (56 chars, `G`/`C` prefix, base-32,
/// non-zero). Checksum itself is enforced by the host on
/// `Address::from_string`; this is the structural mirror for pre-conversion
/// inputs.
pub fn validate_strkey(_env: &Env, strkey: &String) -> Result<(), ValidationError> {
    if strkey.len() != 56 {
        return Err(ValidationError::InvalidAddress);
    }
    let mut buf = [0u8; 56];
    strkey.copy_into_slice(&mut buf);
    validate_strkey_bytes(&buf)
}

fn validate_strkey_bytes(buf: &[u8; 56]) -> Result<(), ValidationError> {
    if buf[0] != b'G' && buf[0] != b'C' {
        return Err(ValidationError::InvalidAddress);
    }
    for b in buf.iter() {
        if !is_base32(*b) {
            return Err(ValidationError::InvalidAddress);
        }
    }
    if buf == ZERO_G_STRKEY.as_bytes() {
        return Err(ValidationError::ZeroAddress);
    }
    Ok(())
}

/// Validate an `Address` contract input.
///
/// The host guarantees checksum validity; this enforces shape + zero-address
/// rejection. Call it at the top of every mutating entry point, before auth
/// and before touching storage (check → compute → write).
pub fn validate_address(_env: &Env, addr: &Address) -> Result<(), ValidationError> {
    let s = addr.to_string();
    if s.len() != 56 {
        return Err(ValidationError::InvalidAddress);
    }
    let mut buf = [0u8; 56];
    s.copy_into_slice(&mut buf);
    validate_strkey_bytes(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn generated_addresses_pass_validation() {
        let env = Env::default();
        for _ in 0..5 {
            let a = Address::generate(&env);
            assert!(validate_address(&env, &a).is_ok());
        }
    }

    #[test]
    fn zero_address_is_rejected() {
        let env = Env::default();
        let zero_str = String::from_str(&env, ZERO_G_STRKEY);
        // Zero strkey is a *valid* checksum encoding, so the host accepts it —
        // our helper must still reject it as a zero address.
        let zero = Address::from_string(&zero_str);
        assert_eq!(
            validate_address(&env, &zero),
            Err(ValidationError::ZeroAddress)
        );
        assert_eq!(
            validate_strkey(&env, &zero_str),
            Err(ValidationError::ZeroAddress)
        );
    }

    #[test]
    fn bad_strkeys_are_rejected() {
        let env = Env::default();
        // Wrong length.
        let short = String::from_str(&env, "GABC");
        assert_eq!(
            validate_strkey(&env, &short),
            Err(ValidationError::InvalidAddress)
        );
        // Bad prefix (56 chars, starts with X).
        let mut bad_prefix = [b'A'; 56];
        bad_prefix[0] = b'X';
        let bad_prefix_str =
            String::from_bytes(&env, &bad_prefix);
        assert_eq!(
            validate_strkey(&env, &bad_prefix_str),
            Err(ValidationError::InvalidAddress)
        );
        // Non-base32 chars: 56 chars, '0'/'1' are not in the alphabet.
        let mut bad_alpha = [b'0'; 56];
        bad_alpha[0] = b'G';
        let bad_alpha_str = String::from_bytes(&env, &bad_alpha);
        assert_eq!(bad_alpha_str.len(), 56);
        assert_eq!(
            validate_strkey(&env, &bad_alpha_str),
            Err(ValidationError::InvalidAddress)
        );
    }
}
