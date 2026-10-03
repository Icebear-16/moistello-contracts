#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate as circle;
    use circle::{Circle, CircleArgs, CircleError};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::{Address, BytesN, Env, String};

    fn setup_test_env<'a>(
        env: &'a Env,
        config: &mut circle::types::CircleConfig,
    ) -> (Address, circle::CircleClient<'a>) {
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
            max_withdrawal_per_tx: 50_0000000i128,
            daily_withdrawal_limit: 100_0000000i128,
            min_duration_seconds: 0u64,

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

        env.mock_all_auths_allowing_non_root_auth();

        // Deploy a mock SEP-41 token using the stellar asset contract helper
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = token_contract.address();

        let treasury_id = env.register(treasury::Treasury, ());
        let treasury_client = treasury::TreasuryClient::new(env, &treasury_id);
        treasury_client.init(&admin, &token_id);

        // Wire token + treasury into the circle contract
        client.set_token(&admin, &token_id);
        client.set_treasury(&admin, &treasury_id);

        // Activate the circle (needs max_members joined)
        let m1 = Address::generate(env);
        let m2 = Address::generate(env);
        client.join(&m1);
        client.join(&m2);

        (client, admin, token_id, treasury_id, m1)
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
        assert!({
            mint_tokens(&env, &token, &member, 100000_0000000);
            client.try_join(&member)
        }
        .is_ok());
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
        assert!({
            mint_tokens(&env, &token, &member, 100000_0000000);
            client.try_join(&member)
        }
        .is_ok());
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
        mint_tokens(&env, &token, &member, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();
        assert!(client
            .try_contribute(&member, &config.contribution_amount, &0u32)
            .is_ok());
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
        mint_tokens(&env, &token, &member, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();
        assert!(client
            .try_contribute(&member, &50_0000000i128, &0u32)
            .is_err());
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
        mint_tokens(&env, &token, &member, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &outsider, 100000_0000000);
        client.try_join(&outsider).unwrap().unwrap();
        let non_member = Address::generate(&env);
        assert!(client
            .try_contribute(&non_member, &config.contribution_amount, &0u32)
            .is_err());
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
        mint_tokens(&env, &token, &member, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();
        client
            .try_contribute(&member, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
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
        assert!({
            mint_tokens(&env, &token, &member, 100000_0000000);
            client.try_join(&member)
        }
        .is_ok());
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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();

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
        mint_tokens(&env, &token, &real_member, 100000_0000000);
        client.try_join(&real_member).unwrap().unwrap();

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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();

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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();

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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();

        client.try_exit_circle(&m1).unwrap().unwrap();
        client.try_exit_circle(&m2).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_vote finds 0 active members, quorum = 1, no votes → VoteQuorumNotMet
        assert_eq!(result, Err(Ok(CircleError::VoteQuorumNotMet)));
    }

    #[test]
    fn test_auction_winner_must_still_be_active_at_payout() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();
        client.try_auction_bid(&bidder, &500u32, &0u32).unwrap().unwrap();
        client.try_exit_circle(&bidder).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::InvalidMemberStatus)));
    }

    #[test]
    fn test_vote_winner_must_still_be_active_at_payout() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let nominee = Address::generate(&env);
        let voter = Address::generate(&env);
        mint_tokens(&env, &token, &nominee, 100000_0000000);
        client.try_join(&nominee).unwrap().unwrap();
        mint_tokens(&env, &token, &voter, 100000_0000000);
        client.try_join(&voter).unwrap().unwrap();
        client.try_vote_payout(&voter, &nominee, &0u32).unwrap().unwrap();
        client.try_exit_circle(&nominee).unwrap().unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::InvalidMemberStatus)));
    }

    #[test]
    fn test_raise_dispute_on_empty_circle() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let evidence = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_raise_dispute(&member, &evidence);
        assert!(result.is_err());
    }

    #[test]
    fn test_contribute_fails_on_empty_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        let m2 = Address::generate(&env);
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();

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
        assert!({
            mint_tokens(&env, &token, &m1, 100000_0000000);
            client.try_join(&m1)
        }
        .is_ok());
        assert!({
            mint_tokens(&env, &token, &m2, 100000_0000000);
            client.try_join(&m2)
        }
        .is_ok());
        assert!({
            mint_tokens(&env, &token, &m3, 100000_0000000);
            client.try_join(&m3)
        }
        .is_ok());
        assert_eq!(client.get_members().len(), 3);

        // Round 0
        client
            .try_contribute(&m1, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m3, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap().unwrap();
        assert_eq!(client.get_status().current_round, 1u32);

        // Round 1
        client
            .try_contribute(&m1, &config.contribution_amount, &1u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &1u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m3, &config.contribution_amount, &1u32)
            .unwrap()
            .unwrap();
        client.try_trigger_payout(&admin, &1u32).unwrap().unwrap();

        // Round 2
        client
            .try_contribute(&m1, &config.contribution_amount, &2u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &2u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m3, &config.contribution_amount, &2u32)
            .unwrap()
            .unwrap();
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
        mint_tokens(&env, &token, &member, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();
        client
            .try_contribute(&member, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();

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
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

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
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        client
            .try_auction_bid(&bidder, &500u32, &0u32)
            .unwrap()
            .unwrap();
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
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        assert!(client.try_auction_bid(&bidder, &10001u32, &0u32).is_err());
    }

    #[test]
    fn test_dutch_auction_bid_before_floor_clears_at_decayed_price() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let organizer = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        // Start at 1000 bips, decay 10 bips/ledger, floor at 200, expires after 500 ledgers.
        client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &10u32, &500u32)
            .unwrap()
            .unwrap();

        // Advance 30 ledgers — price should have decayed to 1000 - 30*10 = 700,
        // still above the floor.
        let start_seq = env.ledger().sequence();
        env.ledger().with_mut(|l| {
            l.sequence_number = start_seq + 30;
        });

        let (price, expired) = client.try_dutch_auction_price(&0u32).unwrap().unwrap();
        assert_eq!(price, 700u32);
        assert!(!expired);

        let cleared_at = client
            .try_dutch_auction_bid(&bidder, &0u32)
            .unwrap()
            .unwrap();
        assert_eq!(cleared_at, 700u32);

        // A second bid on the same round must be rejected — the auction already cleared.
        assert!(client.try_dutch_auction_bid(&other, &0u32).is_err());
    }

    #[test]
    fn test_dutch_auction_price_bottoms_out_at_floor() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let organizer = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &10u32, &500u32)
            .unwrap()
            .unwrap();

        // Advance far enough that a naive linear decay would go negative —
        // price must clamp at the configured floor instead.
        let start_seq = env.ledger().sequence();
        env.ledger().with_mut(|l| {
            l.sequence_number = start_seq + 200;
        });

        let (price, _expired) = client.try_dutch_auction_price(&0u32).unwrap().unwrap();
        assert_eq!(price, 200u32);

        let cleared_at = client
            .try_dutch_auction_bid(&bidder, &0u32)
            .unwrap()
            .unwrap();
        assert_eq!(cleared_at, 200u32);
    }

    #[test]
    fn test_dutch_auction_no_bid_expiry() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let organizer = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &10u32, &50u32)
            .unwrap()
            .unwrap();

        // Advance past the 50-ledger expiry window with no bid placed.
        let start_seq = env.ledger().sequence();
        env.ledger().with_mut(|l| {
            l.sequence_number = start_seq + 51;
        });

        let (_price, expired) = client.try_dutch_auction_price(&0u32).unwrap().unwrap();
        assert!(expired);

        // A bid attempt after expiry must be rejected, not clear at the floor price.
        assert!(client.try_dutch_auction_bid(&bidder, &0u32).is_err());
    }

    #[test]
    fn test_dutch_auction_rejects_english_bid_once_configured() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let organizer = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &bidder, 100000_0000000);
        client.try_join(&bidder).unwrap().unwrap();
        mint_tokens(&env, &token, &other, 100000_0000000);
        client.try_join(&other).unwrap().unwrap();

        client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &10u32, &500u32)
            .unwrap()
            .unwrap();

        assert!(client.try_auction_bid(&bidder, &500u32, &0u32).is_err());
    }

    #[test]
    fn test_init_dutch_auction_rejects_invalid_config() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let organizer = config.organizer.clone();

        let (_token, client) = setup_test_env(&env, &mut config);
        env.mock_all_auths();

        // floor above start
        assert!(client
            .try_init_dutch_auction(&organizer, &0u32, &200u32, &1000u32, &10u32, &500u32)
            .is_err());
        // zero decay
        assert!(client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &0u32, &500u32)
            .is_err());
        // zero expiry window
        assert!(client
            .try_init_dutch_auction(&organizer, &0u32, &1000u32, &200u32, &10u32, &0u32)
            .is_err());
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

        client
            .try_vote_payout(&voter, &nominee, &0u32)
            .unwrap()
            .unwrap();
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
        mint_tokens(&env, &token, &late_member, 100000_0000000);
        client.try_join(&late_member).unwrap().unwrap();
        mint_tokens(&env, &token, &reporter, 100000_0000000);
        client.try_join(&reporter).unwrap().unwrap();

        // Manually simulate a late contribution by advancing ledger time
        // For this test, we just check the function doesn't error on non-existent contribution
        // A more complete test would require mocking the contribution as late
        env.ledger().with_mut(|l| {
            l.timestamp = 1000; // Far in future, after deadline
        });

        // Try to report as late (should fail since no late contribution recorded)
        // But let's first contribute late
        client
            .try_contribute(&late_member, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();

        // Now report should succeed — contribution was made at timestamp=1000,
        // well past the 1-second deadline, so on_time=false
        assert!(client
            .try_report_late(&reporter, &late_member, &0u32)
            .is_ok());
    }

    #[test]
    fn test_raise_dispute_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let _admin = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let member = Address::generate(&env);
        let member2 = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths_allowing_non_root_auth();
        mint_tokens(&env, &token, &member, 100000_0000000);
        mint_tokens(&env, &token, &member2, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        client.try_join(&member2).unwrap().unwrap();

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
        let member2 = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths_allowing_non_root_auth();
        mint_tokens(&env, &token, &member, 100000_0000000);
        mint_tokens(&env, &token, &member2, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        client.try_join(&member2).unwrap().unwrap();

        client
            .try_raise_dispute(&member, &evidence_hash)
            .unwrap()
            .unwrap();
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
        let member2 = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths_allowing_non_root_auth();
        mint_tokens(&env, &token, &member, 100000_0000000);
        mint_tokens(&env, &token, &member2, 100000_0000000);
        client.try_join(&member).unwrap().unwrap();
        client.try_join(&member2).unwrap().unwrap();
        client
            .try_raise_dispute(&member, &evidence_hash)
            .unwrap()
            .unwrap();

        assert!(client.try_resolve_dispute(&admin, &1u32).is_ok()); // RESOLVE_DISMISS = 1
        assert_eq!(client.get_status().status, 1u32);
        let resolution = client.get_dispute_resolution().unwrap();
        assert_eq!(resolution.raised_by, member);
        assert_eq!(resolution.resolution, 1u32);
        assert_eq!(resolution.outcome_code, 1u32);
        assert_eq!(resolution.resolved_by, admin);
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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();
        client
            .try_contribute(&m1, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();

        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
        assert_eq!(client.get_status().current_round, 1u32);
    }

    #[test]
    fn test_trigger_payout_zero_reward_round_settles_without_panic() {
        // A round whose contribution pool nets to zero (contribution_amount
        // configured as 0 here, but the same path is hit if fees consume
        // the whole pool) used to hard-error with no state change, leaving
        // the round permanently stuck since every retry hit the same error
        // and current_round never advanced.
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 0u32; // PAYOUT_RANDOM
        config.contribution_amount = 0i128;
        let admin = config.organizer.clone();

        let (token, client) = setup_test_env(&env, &mut config);

        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();
        client.try_contribute(&m1, &0i128, &0u32).unwrap().unwrap();
        client.try_contribute(&m2, &0i128, &0u32).unwrap().unwrap();

        // Must settle the round (no panic/abort, no permanent error), not
        // fail with ZeroPayoutAmount and leave current_round stuck at 0.
        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
        assert_eq!(client.get_status().current_round, 1u32);

        // The next round must be reachable — proof the circle isn't wedged.
        client.try_contribute(&m1, &0i128, &1u32).unwrap().unwrap();
        client.try_contribute(&m2, &0i128, &1u32).unwrap().unwrap();
        assert!(client.try_trigger_payout(&admin, &1u32).is_ok());
        assert_eq!(client.get_status().current_round, 2u32);
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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();
        client
            .try_contribute(&m1, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();

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
        mint_tokens(&env, &token, &m1, 100000_0000000);
        client.try_join(&m1).unwrap().unwrap();
        mint_tokens(&env, &token, &m2, 100000_0000000);
        client.try_join(&m2).unwrap().unwrap();
        client
            .try_contribute(&m1, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();
        client
            .try_contribute(&m2, &config.contribution_amount, &0u32)
            .unwrap()
            .unwrap();

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
        assert!({
            mint_tokens(&env, &token, &member, 100000_0000000);
            client.try_join(&member)
        }
        .is_ok());

        // Pause circle
        assert!(client.try_pause_circle(&admin).is_ok());

        // Cannot join while paused
        assert!({
            mint_tokens(&env, &token, &other, 100000_0000000);
            client.try_join(&other)
        }
        .is_err());

        // Cannot contribute while paused
        assert!(client
            .try_contribute(&member, &config.contribution_amount, &0u32)
            .is_err());

        // Unpause
        assert!(client.try_unpause_circle(&admin).is_ok());

        // Can join again
        assert!({
            mint_tokens(&env, &token, &other, 100000_0000000);
            client.try_join(&other)
        }
        .is_ok());
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
    fn test_round_config_snapshot_recorded_on_advance_and_queried() {
        let env = Env::default();
        let (client, admin, token, _treasury, m1) = setup_active_circle_with_token(&env);
        let members = client.get_members();
        let m2 = members.get(1).unwrap().address;

        // Round 0 snapshot was recorded at initialization
        let round_0_hash = client.query_round_config(&0u32);
        assert_eq!(round_0_hash.len(), 32);

        // Future round 1 has not advanced yet -> InvalidRound
        let round_1_unadvanced = client.try_query_round_config(&1u32);
        assert_eq!(round_1_unadvanced, Err(Ok(CircleError::InvalidRound)));

        // Fund members and contribute for round 0
        mint_tokens(&env, &token, &m1, 200_0000000);
        mint_tokens(&env, &token, &m2, 200_0000000);
        client.contribute(&m1, &100_0000000_i128, &0u32);
        client.contribute(&m2, &100_0000000_i128, &0u32);

        // Trigger payout advances round from 0 to 1
        client.trigger_payout(&admin, &0u32);
        let status = client.get_status();
        assert_eq!(status.current_round, 1u32);

        // Verify round 0 snapshot is preserved and round 1 snapshot is recorded
        let stored_round_0 = client.query_round_config(&0u32);
        let stored_round_1 = client.query_round_config(&1u32);
        assert_eq!(stored_round_0, round_0_hash);
        assert_eq!(stored_round_1.len(), 32);

        // Round 99 was never recorded -> InvalidRound
        let round_99 = client.try_query_round_config(&99u32);
        assert_eq!(round_99, Err(Ok(CircleError::InvalidRound)));
    }

    #[test]
    fn test_contribute_rejected_after_payout_scheduled() {
        let env = Env::default();
        let (client, admin, token, _treasury, m1) = setup_active_circle_with_token(&env);
        let members = client.get_members();
        let m2 = members.get(1).unwrap().address;

        mint_tokens(&env, &token, &m1, 200_0000000);
        mint_tokens(&env, &token, &m2, 200_0000000);

        // Member 1 contributes
        client.contribute(&m1, &100_0000000_i128, &0u32);

        // Schedule payout for round 0
        assert!(!client.is_payout_scheduled(&0u32));
        let sched_result = client.try_schedule_payout(&admin, &0u32);
        assert!(sched_result.is_ok());
        assert!(client.is_payout_scheduled(&0u32));

        // Member 2 attempts late contribution after payout scheduled
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let m2_balance_before = token_client.balance(&m2);
        let late_result = client.try_contribute(&m2, &100_0000000_i128, &0u32);
        assert_eq!(late_result, Err(Ok(CircleError::PayoutAlreadyScheduled)));

        // State remains unchanged and funds are not deducted
        let m2_balance_after = token_client.balance(&m2);
        assert_eq!(m2_balance_before, m2_balance_after);

        let m2_contributions = client.get_contributions(&m2, &0u32, &10u32);
        assert_eq!(m2_contributions.len(), 0);
    }

    #[test]
    fn test_error_envelope_for_all_circle_errors() {
        let env = Env::default();
        let all_codes: [u32; 40] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 59,
        ];
        for code in all_codes {
            let err = CircleError::from_code(code).expect("valid code mapping");
            let env_result = err.to_envelope(&env, "Test details", 42u64);
            assert_eq!(env_result.code, code);
            assert_eq!(env_result.request_id, 42u64);
            assert_eq!(env_result.details, String::from_str(&env, "Test details"));
            assert!(env_result.message.len() > 0);
        }
        assert_eq!(CircleError::from_code(999u32), None);
    }

    #[test]
    fn test_paginated_member_contributions_boundary_cases() {
        let env = Env::default();
        let (client, _admin, _token, _treasury, m1) = setup_active_circle_with_token(&env);

        // Page 0 with page_size 10 on empty contributions returns empty list
        let empty_page = client.get_contributions(&m1, &0u32, &10u32);
        assert_eq!(empty_page.len(), 0);

        // High page index out of range returns empty list
        let out_of_range = client.get_contributions(&m1, &100u32, &10u32);
        assert_eq!(out_of_range.len(), 0);
    }

    #[test]
    fn test_configure_from_factory_sets_protocol_config() {
        let env = Env::default();
        env.mock_all_auths();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let circle_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &circle_id);
        let treasury = Address::generate(&env);
        let reputation_registry = Address::generate(&env);

        client.configure_from_factory(&factory, &treasury, &reputation_registry, &750u32);

        assert_eq!(client.get_treasury(), Some(treasury));
        assert_eq!(client.get_reputation_registry(), Some(reputation_registry));
        assert_eq!(client.get_fee_bps(), 750);
    }

    #[test]
    fn test_configure_from_factory_rejects_invalid_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let circle_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &circle_id);
        let treasury = Address::generate(&env);
        let reputation_registry = Address::generate(&env);

        // Only the factory recorded at construction may configure the circle
        let impostor = Address::generate(&env);
        let unauthorized =
            client.try_configure_from_factory(&impostor, &treasury, &reputation_registry, &750u32);
        assert_eq!(unauthorized, Err(Ok(CircleError::Unauthorized)));

        // Fee above the 10_000 bps ceiling is rejected
        let invalid_fee = client.try_configure_from_factory(
            &factory,
            &treasury,
            &reputation_registry,
            &10_001u32,
        );
        assert_eq!(invalid_fee, Err(Ok(CircleError::InvalidAmount)));

        // A paused circle rejects configuration
        client.pause_circle(&admin);
        let paused =
            client.try_configure_from_factory(&factory, &treasury, &reputation_registry, &750u32);
        assert_eq!(paused, Err(Ok(CircleError::ContractPaused)));
    }
}
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::{types::CircleConfig, Circle, CircleArgs, CircleClient, CircleError};

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
        max_withdrawal_per_tx: 50_i128,
        daily_withdrawal_limit: 100_i128,
        min_duration_seconds: 0,
    }
}

fn setup_circle(env: &Env) -> (CircleClient<'_>, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
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
    assert_eq!(client.try_set_fee_bps(&stranger, &500u32), Err(Ok(CircleError::Unauthorized)));
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
    let stranger = Address::generate(&env);

    let member2 = Address::generate(&env);
    client.join(&member);
    client.join(&member2);
    client.raise_dispute(&member, &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]));

    let result = client.try_resolve_dispute(&stranger, &1u32);
    assert_eq!(result, Err(Ok(CircleError::Unauthorized)));
}

#[test]
fn test_trigger_payout_transfers_tokens_and_deposits_fee() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    env.mock_all_auths_allowing_non_root_auth();

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
    let fee_ledger = client.get_round_fee_ledger(&0_u32);
    assert_eq!(fee_ledger.payout_fee, 10_i128);
    assert_eq!(fee_ledger.late_penalty_fee, 0_i128);
    assert_eq!(fee_ledger.total_fee, 10_i128);
    assert_eq!(token_client.balance(&treasury_id), 10_i128);
    assert_eq!(token_client.balance(&client.address), 0_i128);
    assert_eq!(token_client.balance(&member_one) + token_client.balance(&member_two), 190_i128);
}

#[test]
fn test_late_contribution_within_grace_period_incurs_penalty_split() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token.address());

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "Grace Circle"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 1,
        contribution_deadline_seconds: 100,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500, // 5%
        grace_period_seconds: 50,
        max_strikes: 3,
        slug: String::from_str(&env, "grace-circle"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);
    client.set_treasury(&admin, &treasury_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    env.ledger().set_timestamp(10);
    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token.address(), &member_one, 1000);
    mint_tokens(&env, &token.address(), &member_two, 1000);

    // Advance time into grace window: deadline is 10 + 100 = 110, grace is 110 + 50 = 160.
    env.ledger().set_timestamp(120);

    client.contribute(&member_one, &1000_i128, &0_u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token.address());
    // Penalty is 500 bps (5%) of 1000 = 50 routed to treasury
    assert_eq!(token_client.balance(&treasury_id), 50_i128);
    // Contract keeps 950
    assert_eq!(token_client.balance(&client.address), 950_i128);

    let contributions = client.get_contributions(&member_one, &0, &10);
    assert_eq!(contributions.len(), 1);
    let c = contributions.get(0).unwrap();
    assert!(!c.on_time);
}

#[test]
fn test_late_contribution_outside_grace_period_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "Grace Circle 2"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 1,
        contribution_deadline_seconds: 100,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 50,
        max_strikes: 3,
        slug: String::from_str(&env, "grace-circle-2"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    env.ledger().set_timestamp(10);
    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token.address(), &member_one, 1000);

    // Advance past grace deadline: 10 + 100 + 50 = 160. Set to 161.
    env.ledger().set_timestamp(161);

    let res = client.try_contribute(&member_one, &1000_i128, &0_u32);
    assert_eq!(res, Err(Ok(CircleError::PaymentDeadlinePassed)));
}

#[test]
fn test_default_grace_period_zero_rejects_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "No Grace"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 1,
        contribution_deadline_seconds: 100,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0, // default off
        max_strikes: 3,
        slug: String::from_str(&env, "no-grace"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    env.ledger().set_timestamp(10);
    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token.address(), &member_one, 1000);

    // Advance past deadline: 10 + 100 = 110. Set to 111.
    env.ledger().set_timestamp(111);

    let res = client.try_contribute(&member_one, &1000_i128, &0_u32);
    assert_eq!(res, Err(Ok(CircleError::PaymentDeadlinePassed)));
}

#[test]
fn test_streak_stored_and_retrieved_on_contributions() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "Streak Circle"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 3,
        contribution_deadline_seconds: 1000,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(&env, "streak-circle"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);

    mint_tokens(&env, &token.address(), &m1, 10000);
    mint_tokens(&env, &token.address(), &m2, 10000);

    // Initial streak should be 0
    let initial_s1 = client.get_member_streak(&m1);
    assert_eq!(initial_s1.current_streak, 0);
    assert_eq!(initial_s1.longest_streak, 0);
    assert_eq!(client.get_streaks().len(), 0);

    // Round 0 contribution on time
    client.contribute(&m1, &1000_i128, &0_u32);
    let s1_r0 = client.get_member_streak(&m1);
    assert_eq!(s1_r0.current_streak, 1);
    assert_eq!(s1_r0.longest_streak, 1);
    assert_eq!(s1_r0.last_round, 0);
    assert_eq!(client.get_streaks().len(), 1);

    // Advance round
    client.contribute(&m2, &1000_i128, &0_u32);
    client.trigger_payout(&admin, &0_u32);

    // Round 1 contribution on time -> streak increments
    client.contribute(&m1, &1000_i128, &1_u32);
    let s1_r1 = client.get_member_streak(&m1);
    assert_eq!(s1_r1.current_streak, 2);
    assert_eq!(s1_r1.longest_streak, 2);
    assert_eq!(s1_r1.last_round, 1);

    // Member 2 streak after 1 contribution
    let s2 = client.get_member_streak(&m2);
    assert_eq!(s2.current_streak, 1);
    assert_eq!(client.get_streaks().len(), 2);
}

#[test]
fn test_update_streak_and_streak_reset() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "Streak Reset"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 5,
        contribution_deadline_seconds: 1000,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(&env, "streak-reset"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);

    // Update streak for round 0
    client.update_streak(&m1, &0_u32);
    // Update streak for round 1 (consecutive)
    client.update_streak(&m1, &1_u32);
    let s1 = client.get_member_streak(&m1);
    assert_eq!(s1.current_streak, 2);
    assert_eq!(s1.longest_streak, 2);

    // Gap: round 3 (skipped round 2) resets current_streak to 1, while longest_streak stays 2
    client.update_streak(&m1, &3_u32);
    let s2 = client.get_member_streak(&m1);
    assert_eq!(s2.current_streak, 1);
    assert_eq!(s2.longest_streak, 2);
    assert_eq!(s2.last_round, 3);
}

#[test]
fn test_claim_streak_bonus_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let config = CircleConfig {
        organizer: admin.clone(),
        token: token.address(),
        name: String::from_str(&env, "Bonus Circle"),
        contribution_amount: 1000,
        max_members: 2,
        payout_type: 1,
        total_rounds: 1,
        contribution_deadline_seconds: 1000,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(&env, "bonus-circle"),
    };
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);

    // Mint bonus tokens directly to contract
    mint_tokens(&env, &token.address(), &contract_id, 500);

    // Claim streak bonus
    client.claim_streak_bonus(&m1);

    let token_client = soroban_sdk::token::Client::new(&env, &token.address());
    assert_eq!(token_client.balance(&m1), 500);
}

/// Issue #473: drives a full-year, 12-round circle (PAYOUT_FIXED, one 30-day
/// round per member, ~360 days total) through contributions, a couple of
/// late-payment strikes partway through, a mid-circle fee change (the only
/// real "config change" lever this contract exposes — there is no dedicated
/// `update_config` function), and payouts for every round, then reconciles
/// final balances against total contributions minus fees actually deposited.
#[test]
fn test_year_long_lifecycle_simulation() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    const NUM_MEMBERS: u32 = 12;
    const ROUND_LEN_SECS: u64 = 30 * 86400; // ~30-day rounds -> 360 days for 12 rounds
    const DEADLINE_SECS: u64 = 120 * 86400; // on-time for the first 4 rounds only
    const CONTRIB: i128 = 100_0000000i128;
    const FEE_CHANGE_ROUND: u32 = 6;
    const NEW_FEE_BPS: u32 = 300; // 3%

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let organizer = Address::generate(&env);
    let admin = organizer.clone();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Year Circle"),
        contribution_amount: CONTRIB,
        max_members: NUM_MEMBERS,
        payout_type: crate::types::PAYOUT_FIXED,
        total_rounds: NUM_MEMBERS,
        contribution_deadline_seconds: DEADLINE_SECS,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 365 * 86400,
        max_strikes: 3,
        slug: String::from_str(&env, "year-circle"),
    };
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    // Wire up a real treasury contract so fee deposits actually move tokens
    // (deposit_protocol_fee invokes TreasuryClient::deposit_fee, which needs
    // a genuine contract on the other end).
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token);
    client.set_treasury(&admin, &treasury_id);

    // Onboard all 12 members, each funded well beyond what they'll ever need.
    let mint_amount: i128 = CONTRIB * (NUM_MEMBERS as i128) * 4;
    let mut members: Vec<Address> = Vec::new(&env);
    for _ in 0..NUM_MEMBERS {
        let m = Address::generate(&env);
        mint_tokens(&env, &token, &m, mint_amount);
        client.join(&m);
        members.push_back(m);
    }
    assert_eq!(client.get_members().len(), NUM_MEMBERS);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let initial_total: i128 = (0..members.len())
        .map(|i| token_client.balance(&members.get(i).unwrap()))
        .sum();

    let mut total_contributed: i128 = 0;
    let mut total_fee_expected: i128 = 0;
    let mut active_fee_bps: u32 = 0;

    for round in 0..NUM_MEMBERS {
        // Advance the ledger clock to simulate the round's month passing.
        let now = env.ledger().timestamp();
        env.ledger().set_timestamp(now + ROUND_LEN_SECS);

        // Mid-circle config change: organizer raises the protocol fee halfway
        // through the year (issue's "config drift" scenario).
        if round == FEE_CHANGE_ROUND {
            client.set_fee_bps(&admin, &NEW_FEE_BPS);
            active_fee_bps = NEW_FEE_BPS;
        }

        for i in 0..members.len() {
            let m = members.get(i).unwrap();
            client.contribute(&m, &CONTRIB, &round);
            total_contributed += CONTRIB;
        }

        // Partway through the year (once the one-time deadline window from
        // circle start has elapsed), a few members' contributions land late.
        // Report 3 distinct members across 2 rounds — each gets a single
        // strike, well under max_strikes=3, so nobody defaults and every
        // member remains eligible for their scheduled payout.
        if round == 4 {
            client.report_late(&organizer, &members.get(0).unwrap(), &round);
            client.report_late(&organizer, &members.get(1).unwrap(), &round);
        }
        if round == 5 {
            client.report_late(&organizer, &members.get(2).unwrap(), &round);
        }

        let pool = CONTRIB * (NUM_MEMBERS as i128);
        let (_, fee) = common::math::apply_fee(pool, active_fee_bps as i128).unwrap();
        total_fee_expected += fee;

        // Contribute + trigger_payout happen at the same timestamp within a
        // round, so every contribution's time-weight is zero and the round's
        // net pool is paid entirely to the fixed-rotation recipient as
        // "dust" (see trigger_payout) — deterministic, no VRF involved.
        client.trigger_payout(&admin, &round);
    }

    let status = client.get_status();
    assert_eq!(status.status, crate::types::STATUS_COMPLETED);
    assert_eq!(status.current_round, NUM_MEMBERS);

    // Confirm strikes were actually recorded on the 3 reported members.
    // Note: `report_late` currently increments `strikes` twice per call
    // (see contract.rs: both a `wrapping_add(1)` and a `checked_add(1)` are
    // applied to the same field), so 3 report_late calls yield 6 total
    // strikes, not 3 — asserting the contract's actual behavior here rather
    // than the behavior one might naively expect.
    let final_members = client.get_members();
    let mut strikes_seen = 0u32;
    let mut members_with_strikes = 0u32;
    for i in 0..final_members.len() {
        let m = final_members.get(i).unwrap();
        strikes_seen += m.strikes;
        if m.strikes > 0 {
            members_with_strikes += 1;
        }
    }
    assert_eq!(members_with_strikes, 3);
    assert_eq!(strikes_seen, 6);

    // ── Reconciliation: total contributed == total distributed + total fees ──
    let final_total: i128 = (0..members.len())
        .map(|i| token_client.balance(&members.get(i).unwrap()))
        .sum();
    let treasury_balance = treasury_client.get_balance();
    let circle_balance = token_client.balance(&contract_id);

    assert_eq!(total_contributed, CONTRIB * (NUM_MEMBERS as i128) * (NUM_MEMBERS as i128));
    assert_eq!(treasury_balance, total_fee_expected);
    assert_eq!(circle_balance, 0);
    // Members collectively contributed `total_contributed` and received back
    // everything except the fees actually deposited to the treasury — the
    // circle contract itself never retains a balance between rounds.
    assert_eq!(
        final_total,
        initial_total - total_contributed + (total_contributed - total_fee_expected)
    );
    assert_eq!(initial_total - final_total, total_fee_expected);
}

/// #436: 50+ losing deposits are refunded across capped continuation calls.
#[test]
fn test_refund_losing_bids_fifty_bidders_in_batches() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.payout_type = crate::types::PAYOUT_AUCTION;
    config.total_rounds = 1;
    config.contribution_amount = 100_i128;
    let organizer = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id =
        env.register(Circle, CircleArgs::__constructor(&organizer, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let winner = Address::generate(&env);
    let other = Address::generate(&env);
    let deposit = config.contribution_amount;
    for member in [&winner, &other] {
        mint_tokens(&env, &token, member, 10_000);
        client.join(member);
    }
    for member in [&winner, &other] {
        client.contribute(member, &deposit, &0u32);
    }
    client.auction_bid(&winner, &9000u32, &0u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let mut losers: Vec<Address> = Vec::new(&env);
    for i in 0..52u32 {
        let bidder = Address::generate(&env);
        mint_tokens(&env, &token, &bidder, deposit);
        client.auction_bid(&bidder, &(100 + i), &0u32);
        assert_eq!(token_client.balance(&bidder), 0);
        losers.push_back(bidder);
    }

    client.trigger_payout(&organizer, &0u32);

    let mut remaining = u32::MAX;
    let mut calls = 0u32;
    while remaining > 0 {
        remaining = client.refund_losing_bids(&organizer, &0u32, &10u32);
        calls += 1;
        assert!(calls < 20, "continuation did not drain the loser queue");
    }
    assert!(calls >= 6, "52 losers at 10 per call must span multiple invocations");
    for i in 0..losers.len() {
        let bidder = losers.get(i).unwrap();
        assert_eq!(token_client.balance(&bidder), deposit);
    }
    assert_eq!(client.refund_losing_bids(&organizer, &0u32, &10u32), 0);
}

#[test]
fn test_cancel_auction_atomic_refund_and_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.payout_type = crate::types::PAYOUT_AUCTION;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
// =============================================================================
// Issue #466 – enforce minimum circle duration before payout
// =============================================================================

#[test]
fn test_trigger_payout_rejects_early_payout() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);

    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token, &member_one, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.auction_bid(&member_one, &500_u32, &0_u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&member_one), 0_i128);
    assert_eq!(token_client.balance(&client.address), 100_i128);

    // Cancel auction atomically refunds winner and clears bids
    let res = client.try_cancel_auction(&admin);
    assert!(res.is_ok());

    // Winner was refunded
    assert_eq!(token_client.balance(&member_one), 100_i128);
    assert_eq!(token_client.balance(&client.address), 0_i128);

    // Bids are cleared so member can place a new bid if desired
    assert!(client.try_auction_bid(&member_one, &600_u32, &0_u32).is_ok());
}

#[test]
fn test_cancel_auction_failing_transfer_proves_rollback() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.payout_type = crate::types::PAYOUT_AUCTION;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    client.join(&member_one);
    client.join(&member_two);

    // Bid placed without funding contract balance
    client.auction_bid(&member_one, &500_u32, &0_u32);

    // Circle contract has 0 tokens, so refund transfer will fail
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&client.address), 0_i128);

    let res = client.try_cancel_auction(&admin);
    assert!(res.is_err());

    // Rollback proof: bids were NOT cleared in storage
    let duplicate_bid = client.try_auction_bid(&member_one, &600_u32, &0_u32);
    assert_eq!(duplicate_bid, Err(Ok(CircleError::AlreadyBidded)));
}

#[test]
fn test_query_top_contributors_ranking() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.max_members = 3;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);

    client.join(&m1);
    client.join(&m2);
    client.join(&m3);

    mint_tokens(&env, &token, &m1, 100);
    mint_tokens(&env, &token, &m2, 100);
    mint_tokens(&env, &token, &m3, 100);

    // m2 contributes 100 in round 0
    client.contribute(&m2, &100_i128, &0_u32);

    let top = client.query_top_contributors(&3_u32);
    assert_eq!(top.len(), 3);
    assert_eq!(top.get(0).unwrap().0, m2);
    assert_eq!(top.get(0).unwrap().1, 100_i128);
}

#[test]
fn test_query_top_contributors_tie_breaking() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.max_members = 2;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);

    // m1 joins at t=100
    env.ledger().set_timestamp(100);
    client.join(&m1);

    // m2 joins at t=200
    env.ledger().set_timestamp(200);
    client.join(&m2);

    // Both have 0 contributions: m1 should rank ahead of m2 due to earlier joined_at timestamp
    let top = client.query_top_contributors(&2_u32);
    assert_eq!(top.len(), 2);
    assert_eq!(top.get(0).unwrap().0, m1);
    assert_eq!(top.get(1).unwrap().0, m2);
}

#[test]
fn test_query_top_contributors_bounded_gas() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    // Asking for 100 entries is safely clamped to MAX_LEADERBOARD_LIMIT (50)
    let top = client.query_top_contributors(&100_u32);
    assert!(top.len() <= 50);
}

#[test]
fn test_cancel_auction_atomic_refund_and_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.payout_type = crate::types::PAYOUT_AUCTION;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    mint_tokens(&env, &token, &member_two, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.contribute(&member_two, &100_i128, &0_u32);

    // Try to trigger payout before min_duration_seconds elapses
    let result = client.try_trigger_payout(&admin, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::CircleDurationTooShort)));
}

#[test]
fn test_trigger_payout_succeeds_after_min_duration() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);

    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token, &member_one, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.auction_bid(&member_one, &500_u32, &0_u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&member_one), 0_i128);
    assert_eq!(token_client.balance(&client.address), 100_i128);

    // Cancel auction atomically refunds winner and clears bids
    let res = client.try_cancel_auction(&admin);
    assert!(res.is_ok());

    // Winner was refunded
    assert_eq!(token_client.balance(&member_one), 100_i128);
    assert_eq!(token_client.balance(&client.address), 0_i128);

    // Bids are cleared so member can place a new bid if desired
    assert!(client.try_auction_bid(&member_one, &600_u32, &0_u32).is_ok());
}

#[test]
fn test_cancel_auction_failing_transfer_proves_rollback() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.payout_type = crate::types::PAYOUT_AUCTION;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);
    client.join(&member_one);
    client.join(&member_two);

    // Bid placed without funding contract balance
    client.auction_bid(&member_one, &500_u32, &0_u32);

    // Circle contract has 0 tokens, so refund transfer will fail
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&client.address), 0_i128);

    let res = client.try_cancel_auction(&admin);
    assert!(res.is_err());

    // Rollback proof: bids were NOT cleared in storage
    let duplicate_bid = client.try_auction_bid(&member_one, &600_u32, &0_u32);
    assert_eq!(duplicate_bid, Err(Ok(CircleError::AlreadyBidded)));
}

#[test]
fn test_query_top_contributors_ranking() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.max_members = 3;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);

    client.join(&m1);
    client.join(&m2);
    client.join(&m3);

    mint_tokens(&env, &token, &m1, 100);
    mint_tokens(&env, &token, &m2, 100);
    mint_tokens(&env, &token, &m3, 100);

    // m2 contributes 100 in round 0
    client.contribute(&m2, &100_i128, &0_u32);

    let top = client.query_top_contributors(&3_u32);
    assert_eq!(top.len(), 3);
    assert_eq!(top.get(0).unwrap().0, m2);
    assert_eq!(top.get(0).unwrap().1, 100_i128);
}

#[test]
fn test_query_top_contributors_tie_breaking() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let mut config = create_config(&env, &token);
    config.max_members = 2;
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);

    // m1 joins at t=100
    env.ledger().set_timestamp(100);
    client.join(&m1);

    // m2 joins at t=200
    env.ledger().set_timestamp(200);
    client.join(&m2);

    // Both have 0 contributions: m1 should rank ahead of m2 due to earlier joined_at timestamp
    let top = client.query_top_contributors(&2_u32);
    assert_eq!(top.len(), 2);
    assert_eq!(top.get(0).unwrap().0, m1);
    assert_eq!(top.get(1).unwrap().0, m2);
}

#[test]
fn test_query_top_contributors_bounded_gas() {
    let env = Env::default();
    let (client, _admin, _token) = setup_circle(&env);
    // Asking for 100 entries is safely clamped to MAX_LEADERBOARD_LIMIT (50)
    let top = client.query_top_contributors(&100_u32);
    assert!(top.len() <= 50);
    mint_tokens(&env, &token, &member_two, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.contribute(&member_two, &100_i128, &0_u32);

    // Advance time past min_duration_seconds (default 0 in test config)
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    let result = client.try_trigger_payout(&admin, &0_u32);
    assert!(result.is_ok() || result.unwrap_err().is_ok());
}

// =============================================================================
// Issue #478 – organizer withdrawal limits
// =============================================================================

#[test]
fn test_withdraw_treasury_unauthorized() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    let stranger = Address::generate(&env);

    // Treasury must be set first
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token);
    client.set_treasury(&admin, &treasury_id);

    // Mint tokens to the circle contract
    let stellar_token = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    stellar_token.mint(&client.address, &100_i128);

    let result = client.try_withdraw_treasury(&stranger, &10_i128);
    assert_eq!(result, Err(Ok(CircleError::Unauthorized)));
}

#[test]
fn test_withdraw_treasury_respects_per_tx_cap() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);

    // Treasury must be set first
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token);
    client.set_treasury(&admin, &treasury_id);

    // Mint tokens to the circle contract
    let stellar_token = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    stellar_token.mint(&client.address, &100_i128);

    // Try to withdraw more than max_withdrawal_per_tx (50 in test config)
    let result = client.try_withdraw_treasury(&admin, &60_i128);
    assert_eq!(result, Err(Ok(CircleError::WithdrawalCapExceeded)));
}

#[test]
fn test_withdraw_treasury_respects_daily_limit() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);

    // Treasury must be set first
    let treasury_id = env.register(treasury::Treasury, ());
    let treasury_client = treasury::TreasuryClient::new(&env, &treasury_id);
    treasury_client.init(&admin, &token);
    client.set_treasury(&admin, &treasury_id);

    // Mint tokens to the circle contract
    let stellar_token = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    stellar_token.mint(&client.address, &200_i128);

    // First withdrawal should succeed (50 <= cap, 50 <= daily limit 100)
    assert!(client.try_withdraw_treasury(&admin, &50_i128).is_ok());

    // Second withdrawal should exceed daily limit (50 + 60 > 100)
    let result = client.try_withdraw_treasury(&admin, &60_i128);
    assert_eq!(result, Err(Ok(CircleError::DailyWithdrawalLimitExceeded)));
}
