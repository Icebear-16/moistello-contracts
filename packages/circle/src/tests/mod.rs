#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod test_integration;

#[cfg(test)]
mod test_security_hardening;

#[cfg(test)]
mod test_stress;

#[cfg(test)]
mod test_vote;

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_smoke_compile_ok() {
        let env = Env::default();
        let _addr =
            Address::from_str(&env, "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H");
    }

    #[test]
    fn test_errors_have_unique_codes() {
        use crate::CircleError;
        assert_ne!(CircleError::NotActive as u32, CircleError::CircleFull as u32);
        assert_ne!(CircleError::AlreadyMember as u32, CircleError::NotMember as u32);
    }
}
