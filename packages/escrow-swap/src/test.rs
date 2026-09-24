#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, BytesN, Env};
    use soroban_sdk::testutils::Address as _;
    use crate as escrow_swap;
    use escrow_swap::{EscrowSwap, EscrowSwapArgs};

    fn create_hash_lock(env: &Env) -> BytesN<32> {
        let secret = soroban_sdk::Bytes::from_array(env, &[1u8; 32]);
        env.crypto().sha256(&secret).into()
    }

    fn setup_token(env: &Env, admin: &Address) -> Address {
        env.register_stellar_asset_contract_v2(admin.clone()).address()
    }

    #[test]
    fn test_create_swap() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(EscrowSwap, EscrowSwapArgs::__constructor(&admin));
        let client = escrow_swap::EscrowSwapClient::new(&env, &contract_id);
        
        let token_a = setup_token(&env, &admin);
        let token_b = setup_token(&env, &admin);
        let token_a_client = soroban_sdk::token::Client::new(&env, &token_a);
        
        let initiator = Address::generate(&env);
        let responder = Address::generate(&env);
        
        let token_a_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_a);
        token_a_admin.mint(&initiator, &100_0000000i128);

        let hash_lock = create_hash_lock(&env);
        let time_lock = env.ledger().timestamp() + 3600;
        assert!(client.try_create_swap(&initiator, &responder, &token_a, &token_b, &100_0000000i128, &200_0000000i128, &hash_lock, &time_lock).is_ok());
    }

    #[test]
    fn test_create_swap_invalid_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(EscrowSwap, EscrowSwapArgs::__constructor(&admin));
        let client = escrow_swap::EscrowSwapClient::new(&env, &contract_id);
        
        let token_a = setup_token(&env, &admin);
        let token_b = setup_token(&env, &admin);

        let initiator = Address::generate(&env);
        let responder = Address::generate(&env);
        let hash_lock = create_hash_lock(&env);
        let time_lock = env.ledger().timestamp() + 3600;
        assert!(client.try_create_swap(&initiator, &responder, &token_a, &token_b, &0i128, &200_0000000i128, &hash_lock, &time_lock).is_err());
    }

    #[test]
    fn test_create_swap_self_referral() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(EscrowSwap, EscrowSwapArgs::__constructor(&admin));
        let client = escrow_swap::EscrowSwapClient::new(&env, &contract_id);
        
        let token_a = setup_token(&env, &admin);
        let token_b = setup_token(&env, &admin);

        let initiator = Address::generate(&env);
        let hash_lock = create_hash_lock(&env);
        let time_lock = env.ledger().timestamp() + 3600;
        assert!(client.try_create_swap(&initiator, &initiator, &token_a, &token_b, &100_0000000i128, &200_0000000i128, &hash_lock, &time_lock).is_err());
    }

    #[test]
    fn test_get_swaps() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(EscrowSwap, EscrowSwapArgs::__constructor(&admin));
        let client = escrow_swap::EscrowSwapClient::new(&env, &contract_id);
        
        let token_a = setup_token(&env, &admin);
        let token_b = setup_token(&env, &admin);

        let initiator = Address::generate(&env);
        let responder = Address::generate(&env);
        
        let token_a_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_a);
        token_a_admin.mint(&initiator, &100_0000000i128);

        let hash_lock = create_hash_lock(&env);
        let time_lock = env.ledger().timestamp() + 3600;
        let _ = client.try_create_swap(&initiator, &responder, &token_a, &token_b, &100_0000000i128, &200_0000000i128, &hash_lock, &time_lock).unwrap();
        let swaps = client.get_swaps();
        assert_eq!(swaps.len(), 1);
    }
}
