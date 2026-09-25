#![cfg(test)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, BytesN, Env, String};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;
    use crate as circle;
    use circle::{Circle, CircleArgs, CircleError};


    fn setup_test_env<'a>(env: &'a Env, config: &mut circle::types::CircleConfig) -> (Address, circle::CircleClient<'a>) {
        let admin = config.organizer.clone();
        let factory = Address::generate(env);
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let token = token_contract.address();
        config.token = token.clone();
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, config));
        let client = circle::CircleClient::new(env, &contract_id);
        (token, client)
    }

    fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
        let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
        token_client.mint(to, &amount);
    }

    fn create_config(env: &Env) -> circle::types::CircleConfig {
        circle::types::CircleConfig {
            organizer: Address::generate(env),
            token: Address::generate(env),
            name: String::from_str(env, "Test Circle"),
            contribution_amount: 100_0000000i128,
            max_members: 5u32,
            payout_type: 0u32,
            total_rounds: 5u32,
            contribution_deadline_seconds: 604800u64,
            min_moi_score: 0u32,
            collateral_amount: 0i128,
            penalty_bps: 500u32,
            grace_period_seconds: 86400u64,
            max_strikes: 3u32,
            slug: String::from_str(env, "test-circle"),

        }
    }

    // ── Bonus test helpers ────────────────────────────────────────────────────

    /// Deploy a 2-member active circle and return (env, client, admin, token_id, treasury)
    fn setup_active_circle_with_token(
        env: &Env,
    ) -> (circle::CircleClient, Address, Address, Address, Address) {
        let mut config = create_config(env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(env);
        let contract_id =
            env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(env, &contract_id);

        // Deploy a mock SEP-41 token using the stellar asset contract helper
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = token_contract.address();

        // Use a separate address as the treasury (holds bonus funds)
        let treasury = Address::generate(env);

        env.mock_all_auths();

        // Wire token + treasury into the circle contract
        client.set_token(&admin, &token_id);
        client.set_treasury(&admin, &treasury);

        // Activate the circle (needs max_members joined)
        let m1 = Address::generate(env);
        let m2 = Address::generate(env);
        client.join(&m1);
        client.join(&m2);

        (client, admin, token_id, treasury, m1)
    }
    #[test]
    fn test_initialize() {
        let env = Env::default();
        let mut config = create_config(&env);
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        
        let status = client.get_status();
        assert_eq!(status.status, 0u32);
    }

    #[test]
    fn test_join() {
        let env = Env::default();
        let mut config = create_config(&env);
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);

        env.mock_all_auths();
        assert!({ mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member) }.is_ok());
        assert_eq!(client.get_members().len(), 1);
    }

    #[test]
    fn test_join_full() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        

        env.mock_all_auths();
        client.try_join(&Address::generate(&env)).unwrap().unwrap();
        client.try_join(&Address::generate(&env)).unwrap().unwrap();
        assert!(client.try_join(&Address::generate(&env)).is_err());
    }

    #[test]
    fn test_duplicate_join() {
        let env = Env::default();
        let mut config = create_config(&env);
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);

        env.mock_all_auths();
        assert!({ mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member) }.is_ok());
        assert!(client.try_join(&member).is_err());
    }

    #[test]
    fn test_contribute() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        assert!(client.try_contribute(&member, &config.contribution_amount, &0u32).is_ok());
    }

    #[test]
    fn test_contribute_wrong_amount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        assert!(client.try_contribute(&member, &50_0000000i128, &0u32).is_err());
    }

    #[test]
    fn test_contribute_not_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let outsider = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &outsider, 100000_0000000); client.try_join(&outsider).unwrap().unwrap();
        let non_member = Address::generate(&env);
        assert!(client.try_contribute(&non_member, &config.contribution_amount, &0u32).is_err());
    }

    #[test]
    fn test_exit() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_contribute(&member, &config.contribution_amount, &0u32).unwrap().unwrap();
        assert!(client.try_exit_circle(&member).is_ok());
    }

    #[test]
    fn test_pause_unpause() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        

        env.mock_all_auths();
        assert!(client.try_pause_circle(&admin).is_ok());
        let member = Address::generate(&env);
        assert!(client.try_join(&member).is_err());
        assert!(client.try_unpause_circle(&admin).is_ok());
        assert!({ mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member) }.is_ok());
    }

    #[test]
    fn test_empty_circle_get_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_token, client) = setup_test_env(&env, &mut config);
        let members = client.get_members();
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn test_empty_circle_get_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_token, client) = setup_test_env(&env, &mut config);
        let member = Address::generate(&env);
        let contributions = client.get_contributions(&member, &0, &100);
        assert_eq!(contributions.len(), 0);
    }

    #[test]
    fn test_trigger_payout_not_active_when_pending() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        // Only 1 member joined — circle stays PENDING (not full)
        let m1 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::NotActive)));
    }

    #[test]
    fn test_trigger_payout_after_all_members_exit() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 3u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();

        // Both exit — 0 active members remain
        client.try_exit_circle(&m1).unwrap().unwrap();
        client.try_exit_circle(&m2).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::PayoutAlreadyExecuted)));
    }

    #[test]
    fn test_resolve_dispute_no_dispute() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let result = client.try_resolve_dispute(&admin, &1u32);
        assert_eq!(result, Err(Ok(CircleError::NoActiveDispute)));
    }

    #[test]
    fn test_exit_from_empty_circle() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let stranger = Address::generate(&env);
        // Exiting from a circle with no members and no collateral succeeds
        // with no state change (no active member found, no-op)
        let result = client.try_exit_circle(&stranger);
        assert!(result.is_ok());
        assert_eq!(client.get_members().len(), 0);
    }

    #[test]
    fn test_exit_non_member_does_not_affect_state() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let real_member = Address::generate(&env);
        mint_tokens(&env, &token, &real_member, 100000_0000000); client.try_join(&real_member).unwrap().unwrap();

        let stranger = Address::generate(&env);
        // Stranger exits — succeeds as no-op since stranger isn't in members list
        let result = client.try_exit_circle(&stranger);
        assert!(result.is_ok());
        // Real member should still be in the circle
        assert_eq!(client.get_members().len(), 1);
    }

    #[test]
    fn test_trigger_payout_fixed_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 1u32; // PAYOUT_FIXED
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();

        client.try_exit_circle(&m1).unwrap().unwrap();
        client.try_exit_circle(&m2).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_fixed builds pos_to_addr from active members only,
        // finds no active member at the target position → NotMember
        assert_eq!(result, Err(Ok(CircleError::NotMember)));
    }

    #[test]
    fn test_trigger_payout_auction_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();

        client.try_exit_circle(&m1).unwrap().unwrap();
        client.try_exit_circle(&m2).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_auction finds no bids → VoteQuorumNotMet
        assert_eq!(result, Err(Ok(CircleError::VoteQuorumNotMet)));
    }

    #[test]
    fn test_trigger_payout_vote_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();

        client.try_exit_circle(&m1).unwrap().unwrap();
        client.try_exit_circle(&m2).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_vote finds 0 active members, quorum = 1, no votes → VoteQuorumNotMet
        assert_eq!(result, Err(Ok(CircleError::VoteQuorumNotMet)));
    }

    #[test]
    fn test_raise_dispute_on_empty_circle() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let member = Address::generate(&env);
        let evidence = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_raise_dispute(&member, &evidence);

        // Circle is PENDING until full — raise_dispute requires STATUS_ACTIVE
        assert_eq!(result, Err(Ok(CircleError::NotActive)));    }

    #[test]
    fn test_contribute_fails_on_empty_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();

        mint_tokens(&env, &token, &m1, config.contribution_amount);
        let result = client.try_contribute(&m1, &config.contribution_amount, &0u32);
        // Should succeed since m1 is a member and contributions is empty
        assert!(result.is_ok());
        let contributions = client.get_contributions(&m1, &0, &100);
        assert_eq!(contributions.len(), 1);
    }

    #[test]
    fn test_unauthorized() {
        let env = Env::default();
        let mut config = create_config(&env);
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);

        // No env.mock_all_auths() — should fail authorization
        assert!(client.try_join(&member).is_err());
    }


    #[test]
    fn test_full_lifecycle() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 3u32;
        config.total_rounds = 3u32;
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        

        env.mock_all_auths();

        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let m3 = Address::generate(&env);

        // Join
        assert!({ mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1) }.is_ok());
        assert!({ mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2) }.is_ok());
        assert!({ mint_tokens(&env, &token, &m3, 100000_0000000); client.try_join(&m3) }.is_ok());
        assert_eq!(client.get_members().len(), 3);

        // Round 0
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap().unwrap();
        assert_eq!(client.get_status().current_round, 1u32);

        // Round 1
        client.try_contribute(&m1, &config.contribution_amount, &1u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &1u32).unwrap().unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &1u32).unwrap().unwrap();
        client.try_trigger_payout(&admin, &1u32).unwrap().unwrap();

        // Round 2
        client.try_contribute(&m1, &config.contribution_amount, &2u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &2u32).unwrap().unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &2u32).unwrap().unwrap();
        client.try_trigger_payout(&admin, &2u32).unwrap().unwrap();

        // Should be completed
        assert_eq!(client.get_status().status, 2u32);
    }

    #[test]
    fn test_get_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_contribute(&member, &config.contribution_amount, &0u32).unwrap().unwrap();

        let contributions = client.get_contributions(&member, &0, &100);
        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions.get(0).unwrap().member, member);
        assert_eq!(contributions.get(0).unwrap().round, 0u32);
    }

    #[test]
    fn test_auction_bid_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();

        assert!(client.try_auction_bid(&bidder, &500u32, &0u32).is_ok());
    }

    #[test]
    fn test_auction_bid_duplicate() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();

        client.try_auction_bid(&bidder, &500u32, &0u32).unwrap().unwrap();
        assert!(client.try_auction_bid(&bidder, &600u32, &0u32).is_err());
    }

    #[test]
    fn test_auction_bid_invalid_discount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();

        assert!(client.try_auction_bid(&bidder, &10001u32, &0u32).is_err());
    }

    #[test]
    fn test_vote_payout_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        
        let voter = Address::generate(&env);
        let nominee = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&voter).unwrap().unwrap();
        client.try_join(&nominee).unwrap().unwrap();

        assert!(client.try_vote_payout(&voter, &nominee, &0u32).is_ok());
    }

    #[test]
    fn test_vote_payout_duplicate() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        
        let voter = Address::generate(&env);
        let nominee = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&voter).unwrap().unwrap();
        client.try_join(&nominee).unwrap().unwrap();

        client.try_vote_payout(&voter, &nominee, &0u32).unwrap().unwrap();
        assert!(client.try_vote_payout(&voter, &nominee, &0u32).is_err());
    }

    #[test]
    fn test_report_late_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.contribution_deadline_seconds = 1u64; // Very short deadline
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let late_member = Address::generate(&env);
        let reporter = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &late_member, 100000_0000000); client.try_join(&late_member).unwrap().unwrap();
        mint_tokens(&env, &token, &reporter, 100000_0000000); client.try_join(&reporter).unwrap().unwrap();

        // Manually simulate a late contribution by advancing ledger time
        // For this test, we just check the function doesn't error on non-existent contribution
        // A more complete test would require mocking the contribution as late
        env.ledger().with_mut(|l| {
            l.timestamp = 1000; // Far in future, after deadline
        });

        // Try to report as late (should fail since no late contribution recorded)
        // But let's first contribute late
        client.try_contribute(&late_member, &config.contribution_amount, &0u32).unwrap().unwrap();

        // Now report should succeed — contribution was made at timestamp=1000,
        // well past the 1-second deadline, so on_time=false
        assert!(client.try_report_late(&reporter, &late_member, &0u32).is_ok());
    }

    #[test]
    fn test_raise_dispute_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();

        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        assert!(client.try_raise_dispute(&member, &evidence_hash).is_ok());
        assert_eq!(client.get_status().status, 4u32);
    }

    #[test]
    fn test_raise_dispute_duplicate() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();

        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();
        assert!(client.try_raise_dispute(&member, &evidence_hash).is_err());
    }

    #[test]
    fn test_resolve_dispute_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();

        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();

        assert!(client.try_resolve_dispute(&admin, &1u32).is_ok()); // RESOLVE_DISMISS = 1
        env.ledger().set_timestamp(env.ledger().timestamp() + 24 * 60 * 60);
        client.execute_dispute_resolution();
        assert_eq!(client.get_status().status, 1u32);
    }

    #[test]
    fn test_extend_dispute_window_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();

        let t0 = env.ledger().timestamp();
        assert!(client.try_resolve_dispute(&admin, &1u32).is_ok());

        // Challenger extends dispute window by 12 hours (43200 seconds)
        let ext_seconds: u64 = 12 * 3600;
        let new_exec = client.extend_dispute_window(&member, &ext_seconds);
        assert_eq!(new_exec, t0 + 24 * 3600 + ext_seconds);

        // At 25 hours (which would have been elapsed originally), execution is still blocked
        env.ledger().set_timestamp(t0 + 25 * 3600);
        assert_eq!(client.try_execute_dispute_resolution(), Err(Ok(CircleError::ResolutionTimelockActive)));

        // At 37 hours, new extended window has elapsed, execution succeeds
        env.ledger().set_timestamp(t0 + 37 * 3600);
        assert!(client.try_execute_dispute_resolution().is_ok());
        assert_eq!(client.get_status().status, 1u32);
    }

    #[test]
    fn test_extend_dispute_window_rejects_non_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let outsider = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();
        client.try_resolve_dispute(&admin, &1u32).unwrap().unwrap();

        let res = client.try_extend_dispute_window(&outsider, &3600);
        assert_eq!(res, Err(Ok(CircleError::NotMember)));
    }

    #[test]
    fn test_extend_dispute_window_rejects_invalid_amount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();
        client.try_resolve_dispute(&admin, &1u32).unwrap().unwrap();

        // 0 seconds rejected
        let res_zero = client.try_extend_dispute_window(&member, &0);
        assert_eq!(res_zero, Err(Ok(CircleError::InvalidAmount)));

        // > 7 days (8 days = 8 * 86400) rejected
        let res_too_large = client.try_extend_dispute_window(&member, &(8 * 86400));
        assert_eq!(res_too_large, Err(Ok(CircleError::InvalidAmount)));
    }

    #[test]
    fn test_extend_dispute_window_rejects_after_expiry() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let other = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other).unwrap().unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap().unwrap();
        let t0 = env.ledger().timestamp();
        client.try_resolve_dispute(&admin, &1u32).unwrap().unwrap();

        // Advance past window expiry
        env.ledger().set_timestamp(t0 + 25 * 3600);
        let res = client.try_extend_dispute_window(&member, &3600);
        assert_eq!(res, Err(Ok(CircleError::ResolutionTimelockActive)));
    }

    #[test]
    fn test_trigger_payout_random_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 0u32; // PAYOUT_RANDOM
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap().unwrap();

        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
        assert_eq!(client.get_status().current_round, 1u32);
    }

    #[test]
    fn test_trigger_payout_fixed_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 1u32; // PAYOUT_FIXED
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap().unwrap();

        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
    }

    #[test]
    fn test_trigger_payout_unauthorized() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000); client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000); client.try_join(&m2).unwrap().unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap().unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap().unwrap();

        assert!(client.try_trigger_payout(&unauthorized, &0u32).is_err());
    }

    #[test]
    fn test_pause_unpause_extended() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        
        let (token, client) = setup_test_env(&env, &mut config);
        
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();

        // Initially can join
        assert!({ mint_tokens(&env, &token, &member, 100000_0000000); client.try_join(&member) }.is_ok());

        // Pause circle
        assert!(client.try_pause_circle(&admin).is_ok());

        // Cannot join while paused
        assert!({ mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other) }.is_err());

        // Cannot contribute while paused
        assert!(client.try_contribute(&member, &config.contribution_amount, &0u32).is_err());

        // Unpause
        assert!(client.try_unpause_circle(&admin).is_ok());

        // Can join again
        assert!({ mint_tokens(&env, &token, &other, 100000_0000000); client.try_join(&other) }.is_ok());
    }

    #[test]
    fn test_pause_unpause_unauthorized() {
        let env = Env::default();
        let mut config = create_config(&env);
        let _admin = config.organizer.clone();
        
        let (_token, client) = setup_test_env(&env, &mut config);
        
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();

        assert!(client.try_pause_circle(&unauthorized).is_err());
        assert!(client.try_unpause_circle(&unauthorized).is_err());
    }

    #[test]
    fn test_cancel_circle_blocked_after_contribution() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000);
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        client.try_join(&m2).unwrap().unwrap();

        // Both members have joined — circle transitions to ACTIVE. Record a contribution.
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap().unwrap();

        // Organizer must not be able to cancel once the circle is underway.
        let result = client.try_cancel_circle(&admin);
        assert_eq!(result, Err(Ok(CircleError::NotActive)));
    }

    #[test]
    fn test_cancel_circle_allowed_before_contribution() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 5u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();

        // Circle is still PENDING (not full) and no contributions — cancel must succeed.
        let result = client.try_cancel_circle(&admin);
        assert!(result.is_ok());
        assert_eq!(client.get_status().status, 3u32); // STATUS_CANCELLED
    }

    // ===== Issue 1: Allowlist Tests =====


}
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::{Circle, CircleArgs, CircleClient, CircleError};

fn mint_tokens(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    token_client.mint(recipient, &amount);
}

fn create_config(env: &Env, token: &Address) -> crate::types::CircleConfig {
    crate::types::CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Test Circle"),
        contribution_amount: 100_i128,
        max_members: 2,
        payout_type: crate::types::PAYOUT_FIXED,
        total_rounds: 2,
        contribution_deadline_seconds: 60,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(env, "test-circle"),
    }
}

fn setup_circle(env: &Env) -> (CircleClient<'_>, Address, Address) {
    env.mock_all_auths();
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    let config = create_config(env, &token);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin, token)
}

#[test]
fn test_contribute_rejects_amount_above_circle_max() {
    let env = Env::default();
    let (client, _admin, token) = setup_circle(&env);
    let member = Address::generate(&env);
    let other = Address::generate(&env);

    client.join(&member);
    client.join(&other);

    mint_tokens(&env, &token, &member, 200);
    let result = client.try_contribute(&member, &101_i128, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::ContributionMismatch)));
}

#[test]
fn test_batch_payout_rejects_more_than_ten_recipients() {
    let env = Env::default();
    let (client, admin, _token) = setup_circle(&env);
    let member = Address::generate(&env);
    let other = Address::generate(&env);

    client.join(&member);
    client.join(&other);

    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..11 {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1_i128);
    }

    let result = client.try_batch_payout(&admin, &recipients, &amounts, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::InvalidAmount)));
}

#[test]
fn test_batch_payout_happy_path() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);

    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token, &member_one, 100);
    mint_tokens(&env, &token, &member_two, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.contribute(&member_two, &100_i128, &0_u32);

    let mut recipients = Vec::new(&env);
    recipients.push_back(member_one.clone());
    recipients.push_back(member_two.clone());

    let mut amounts = Vec::new(&env);
    amounts.push_back(80_i128);
    amounts.push_back(120_i128);

    client.batch_payout(&admin, &recipients, &amounts, &0_u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&member_one), 80_i128);
    assert_eq!(token_client.balance(&member_two), 120_i128);
}

#[test]
fn test_auth_trigger_payout_and_admin_setters() {
    let env = Env::default();
    let (client, admin, _token) = setup_circle(&env);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    client.join(&member_one);
    client.join(&member_two);

    let stranger = Address::generate(&env);
    let result = client.try_trigger_payout(&stranger, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::Unauthorized)));

    assert_eq!(
        client.try_set_reputation_registry(&stranger, &Address::generate(&env)),
        Err(Ok(CircleError::Unauthorized))
    );
    assert_eq!(
        client.try_set_treasury(&stranger, &Address::generate(&env)),
        Err(Ok(CircleError::Unauthorized))
    );
    assert_eq!(
        client.try_set_token(&stranger, &Address::generate(&env)),
        Err(Ok(CircleError::Unauthorized))
    );
    assert_eq!(
        client.try_set_fee_bps(&stranger, &500u32),
        Err(Ok(CircleError::Unauthorized))
    );
    assert_eq!(
        client.try_set_oracle(&stranger, &Address::generate(&env)),
        Err(Ok(CircleError::Unauthorized))
    );

    let new_reg = Address::generate(&env);
    assert!(client.try_set_reputation_registry(&admin, &new_reg).is_ok());
    assert_eq!(client.get_reputation_registry(), Some(new_reg));
}

#[test]
fn test_resolve_dispute_unauthorized() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    let member = Address::generate(&env);
    let other = Address::generate(&env);
    let stranger = Address::generate(&env);
    let other = Address::generate(&env);

    client.join(&member);
    client.join(&other);
    client.raise_dispute(&member, &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]));

    let result = client.try_resolve_dispute(&stranger, &1u32);
    assert_eq!(result, Err(Ok(CircleError::Unauthorized)));
}

#[test]
fn test_trigger_payout_transfers_tokens_and_deposits_fee() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);

    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token);

    client.set_treasury(&admin, &treasury_id);
    client.set_fee_bps(&admin, &500u32);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token, &member_one, 100);
    mint_tokens(&env, &token, &member_two, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.contribute(&member_two, &100_i128, &0_u32);

    env.ledger().set_timestamp(env.ledger().timestamp() + 60);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&client.address), 200_i128);

    client.trigger_payout(&admin, &0_u32);

    // pool = 200, fee_bps = 500 => fee = 10, net = 190
    assert_eq!(treasury_client.get_balance(), 10_i128);
    assert_eq!(token_client.balance(&treasury_id), 10_i128);
    assert_eq!(token_client.balance(&client.address), 0_i128);
    assert_eq!(
        token_client.balance(&member_one) + token_client.balance(&member_two),
        190_i128
    );
}


    #[test]
    fn test_cancel_circle_blocked_after_contribution() {
        let env = Env::default();
        let (client, admin, token) = setup_circle(&env);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        client.join(&m1);
        client.join(&m2);
        mint_tokens(&env, &token, &m1, 200);
        client.contribute(&m1, &100_i128, &0_u32);

        let result = client.try_cancel_circle(&admin);
        assert_eq!(result, Err(Ok(CircleError::NotActive)));
    }

    #[test]
    fn test_cancel_circle_allowed_before_contribution() {
        let env = Env::default();
        let (client, admin, _token) = setup_circle(&env);
        let m1 = Address::generate(&env);
        client.join(&m1);

        let result = client.try_cancel_circle(&admin);
        assert!(result.is_ok());
        assert_eq!(client.get_status().status, 3u32); // STATUS_CANCELLED
    }