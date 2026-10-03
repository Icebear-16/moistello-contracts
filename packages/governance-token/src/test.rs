#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use crate as governance_token;
    use governance_token::{GovernanceToken, GovernanceTokenClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String};

    fn setup(env: &Env) -> (Address, GovernanceTokenClient) {
        let admin = Address::generate(env);
        let contract_id = env.register(GovernanceToken, ());
        let client = GovernanceTokenClient::new(env, &contract_id);
        client.initialize(
            &admin,
            &String::from_str(env, "Governance Token"),
            &String::from_str(env, "GOV"),
            &7u32,
        );
        (admin, client)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        assert_eq!(client.name(), String::from_str(&env, "Governance Token"));
        assert_eq!(client.symbol(), String::from_str(&env, "GOV"));
        assert_eq!(client.decimals(), 7u32);
        assert_eq!(client.total_supply(), 0i128);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_initialize_double_init_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(GovernanceToken, ());
        let client = GovernanceTokenClient::new(&env, &contract_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7u32,
        );
        let result = client.try_initialize(
            &admin,
            &String::from_str(&env, "Token2"),
            &String::from_str(&env, "TK2"),
            &7u32,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_mint() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &recipient, &1_000_0000000i128);
        assert_eq!(client.balance(&recipient), 1_000_0000000i128);
        assert_eq!(client.total_supply(), 1_000_0000000i128);
    }

    #[test]
    fn test_mint_unauthorized() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let not_admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_mint(&not_admin, &recipient, &1_000_0000000i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_mint_zero_fails() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_mint(&admin, &recipient, &0i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_mint_negative_fails() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_mint(&admin, &recipient, &-1i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_burn() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &1_000_0000000i128);
        client.burn(&holder, &500_0000000i128);
        assert_eq!(client.balance(&holder), 500_0000000i128);
        assert_eq!(client.total_supply(), 500_0000000i128);
    }

    #[test]
    fn test_burn_insufficient_balance() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &100i128);
        let result = client.try_burn(&holder, &200i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_burn_unauthorized() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        let other = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &1000i128);
        let result = client.try_burn(&other, &500i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);
        client.transfer(&alice, &bob, &300_0000000i128);
        assert_eq!(client.balance(&alice), 700_0000000i128);
        assert_eq!(client.balance(&bob), 300_0000000i128);
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &100i128);
        let result = client.try_transfer(&alice, &bob, &200i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_zero_fails() {
        let env = Env::default();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let (_, client) = setup(&env);
        env.mock_all_auths();
        let result = client.try_transfer(&alice, &bob, &0i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_approve_and_transfer_from() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &owner, &1_000_0000000i128);
        client.approve(&owner, &spender, &500_0000000i128, &0u32);
        let allowance = client.allowance(&owner, &spender);
        assert_eq!(allowance.amount, 500_0000000i128);
        client.transfer_from(&spender, &owner, &recipient, &200_0000000i128);
        assert_eq!(client.balance(&owner), 800_0000000i128);
        assert_eq!(client.balance(&recipient), 200_0000000i128);
        let remaining = client.allowance(&owner, &spender);
        assert_eq!(remaining.amount, 300_0000000i128);
    }

    #[test]
    fn test_transfer_from_exceeds_allowance() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &owner, &1_000_0000000i128);
        client.approve(&owner, &spender, &100i128, &0u32);
        let result = client.try_transfer_from(&spender, &owner, &recipient, &200i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_from_no_allowance() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_transfer_from(&spender, &owner, &recipient, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_clawback() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &1_000_0000000i128);
        client.clawback(&admin, &holder, &400_0000000i128);
        assert_eq!(client.balance(&holder), 600_0000000i128);
        assert_eq!(client.total_supply(), 600_0000000i128);
    }

    #[test]
    fn test_clawback_from_frozen_account_permitted() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &1_000_0000000i128);
        client.freeze(&admin, &holder);
        assert!(client.is_frozen(&holder));
        // Admin clawback succeeds even on frozen accounts (intentional regulatory override)
        client.clawback(&admin, &holder, &400_0000000i128);
        assert_eq!(client.balance(&holder), 600_0000000i128);
        assert_eq!(client.total_supply(), 600_0000000i128);
    }

    #[test]
    fn test_clawback_unauthorized() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        let not_admin = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &holder, &1000i128);
        let result = client.try_clawback(&not_admin, &holder, &500i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_freeze_blocks_transfer() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);
        client.freeze(&admin, &alice);
        assert!(client.is_frozen(&alice));
        let result = client.try_transfer(&alice, &bob, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_freeze_blocks_receive() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);
        client.freeze(&admin, &bob);
        let result = client.try_transfer(&alice, &bob, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_unfreeze() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);
        client.freeze(&admin, &alice);
        assert!(client.is_frozen(&alice));
        client.unfreeze(&admin, &alice);
        assert!(!client.is_frozen(&alice));
        assert!(client.try_transfer(&alice, &bob, &100i128).is_ok());
    }

    #[test]
    fn test_freeze_blocks_mint() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let holder = Address::generate(&env);
        env.mock_all_auths();
        client.freeze(&admin, &holder);
        let result = client.try_mint(&admin, &holder, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_admin() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let new_admin = Address::generate(&env);
        env.mock_all_auths();
        client.set_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_set_admin_unauthorized() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let not_admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_set_admin(&not_admin, &new_admin);
        assert!(result.is_err());
    }

    #[test]
    fn test_balance_nonexistent_account() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let ghost = Address::generate(&env);
        assert_eq!(client.balance(&ghost), 0i128);
    }

    #[test]
    fn test_approve_zero_clears_allowance() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &owner, &1000i128);
        client.approve(&owner, &spender, &500i128, &0u32);
        assert_eq!(client.allowance(&owner, &spender).amount, 500i128);
        client.approve(&owner, &spender, &0i128, &0u32);
        assert_eq!(client.allowance(&owner, &spender).amount, 0i128);
    }

    #[test]
    fn test_full_lifecycle() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);
        env.mock_all_auths();

        // Mint to alice
        client.mint(&admin, &alice, &10_000_0000000i128);
        assert_eq!(client.total_supply(), 10_000_0000000i128);

        // Transfer alice -> bob
        client.transfer(&alice, &bob, &3_000_0000000i128);
        assert_eq!(client.balance(&alice), 7_000_0000000i128);
        assert_eq!(client.balance(&bob), 3_000_0000000i128);

        // Approve bob to spend alice's tokens, transfer to charlie
        client.approve(&alice, &bob, &2_000_0000000i128, &0u32);
        client.transfer_from(&bob, &alice, &charlie, &1_000_0000000i128);
        assert_eq!(client.balance(&alice), 6_000_0000000i128);
        assert_eq!(client.balance(&charlie), 1_000_0000000i128);
        assert_eq!(client.allowance(&alice, &bob).amount, 1_000_0000000i128);

        // Burn some of bob's tokens
        client.burn(&bob, &500_0000000i128);
        assert_eq!(client.balance(&bob), 2_500_0000000i128);
        assert_eq!(client.total_supply(), 9_500_0000000i128);

        // Clawback 100 from charlie
        client.clawback(&admin, &charlie, &100_0000000i128);
        assert_eq!(client.balance(&charlie), 900_0000000i128);
        assert_eq!(client.total_supply(), 9_400_0000000i128);
    }

    #[test]
    fn test_transfer_to_contract_address_rejected() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);

        let result = client.try_transfer(&alice, &client.address, &500_0000000i128);
        assert_eq!(result, Err(Ok(crate::types::TokenError::CannotTransferToSelf)));
    }

    #[test]
    fn test_transfer_from_to_contract_address_rejected() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);
        client.approve(&alice, &bob, &1_000_0000000i128, &0u32);

        let result = client.try_transfer_from(&bob, &alice, &client.address, &500_0000000i128);
        assert_eq!(result, Err(Ok(crate::types::TokenError::CannotTransferToSelf)));
    }

    #[test]
    fn test_mint_to_contract_address_rejected() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        env.mock_all_auths();

        let result = client.try_mint(&admin, &client.address, &1_000_0000000i128);
        assert_eq!(result, Err(Ok(crate::types::TokenError::CannotTransferToSelf)));
    }

    #[test]
    fn test_allowlist_mode_default_off() {
        let env = Env::default();
        let (_, client) = setup(&env);
        assert!(!client.is_allowlist_mode_enabled());
    }

    #[test]
    fn test_set_allowlist_mode_toggles_and_takes_effect() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &alice, &1_000_0000000i128);

        client.set_allowlist_mode(&admin, &true);
        assert!(client.is_allowlist_mode_enabled());

        // bob not allowlisted -> transfer fails
        let result = client.try_transfer(&alice, &bob, &100i128);
        assert!(result.is_err());

        // allowlist bob -> transfer succeeds
        client.add_to_allowlist(&admin, &bob);
        assert!(client.is_allowlisted(&bob));
        client.transfer(&alice, &bob, &100i128);
        assert_eq!(client.balance(&bob), 100i128);

        // remove bob -> transfer fails again
        client.remove_from_allowlist(&admin, &bob);
        assert!(!client.is_allowlisted(&bob));
        let result = client.try_transfer(&alice, &bob, &100i128);
        assert!(result.is_err());

        // disable mode -> transfer succeeds normally
        client.set_allowlist_mode(&admin, &false);
        assert!(!client.is_allowlist_mode_enabled());
        client.transfer(&alice, &bob, &100i128);
        assert_eq!(client.balance(&bob), 200i128);
    }

    #[test]
    fn test_allowlist_mode_transfer_from_gated() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);
        env.mock_all_auths();
        client.mint(&admin, &owner, &1_000_0000000i128);
        client.approve(&owner, &spender, &500_0000000i128, &0u32);
        client.set_allowlist_mode(&admin, &true);

        let result = client.try_transfer_from(&spender, &owner, &recipient, &100i128);
        assert!(result.is_err());

        client.add_to_allowlist(&admin, &recipient);
        client.transfer_from(&spender, &owner, &recipient, &100i128);
        assert_eq!(client.balance(&recipient), 100i128);
    }

    #[test]
    fn test_allowlist_management_unauthorized() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let not_admin = Address::generate(&env);
        let account = Address::generate(&env);
        env.mock_all_auths();

        assert!(client.try_set_allowlist_mode(&not_admin, &true).is_err());
        assert!(client.try_add_to_allowlist(&not_admin, &account).is_err());
        assert!(client
            .try_remove_from_allowlist(&not_admin, &account)
            .is_err());
    }
}
