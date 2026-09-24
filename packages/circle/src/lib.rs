#![cfg_attr(not(test), no_std)]
// Deny unused imports at the crate level so stale imports (e.g. a previously
// re-exported `load_round_details` from payout.rs) are caught at compile time
// rather than silently emitting warnings that can be overlooked.
#![deny(unused_imports)]
mod types; mod contract; mod payout; mod oracle; mod analytics; mod migration; #[cfg(test)] mod test; #[cfg(test)] mod tests;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

pub use types::CircleError;

#[contract]pub struct Circle;
#[contractimpl]impl Circle{
    pub fn __constructor(env:Env,admin:Address,factory:Address,config:types::CircleConfig)->Result<(),types::CircleError>{contract::init(&env,&admin,&factory,&config)}
    pub fn join(env:Env,member:Address)->Result<(),types::CircleError>{contract::join(&env,&member)}
    pub fn contribute(env:Env,member:Address,amount:i128,round:u32)->Result<(),types::CircleError>{contract::contribute(&env,&member,amount,round)}
    pub fn trigger_payout(env:Env,caller:Address,round:u32)->Result<(),types::CircleError>{contract::trigger_payout(&env,&caller,round)}
    pub fn auction_bid(env:Env,bidder:Address,discount_bips:u32,round:u32)->Result<(),types::CircleError>{contract::auction_bid(&env,&bidder,discount_bips,round)}
    pub fn vote_payout(env:Env,voter:Address,vote_for:Address,round:u32)->Result<(),types::CircleError>{contract::vote_payout(&env,&voter,&vote_for,round)}
    pub fn exit_circle(env:Env,member:Address)->Result<(),types::CircleError>{contract::exit(&env,&member)}
    pub fn cancel_circle(env:Env,caller:Address)->Result<(),types::CircleError>{contract::cancel_circle(&env,&caller)}
    pub fn cancel(env:Env,caller:Address)->Result<(),types::CircleError>{contract::cancel(&env,&caller)}
    pub fn report_late(env:Env,reporter:Address,late_member:Address,round:u32)->Result<(),types::CircleError>{contract::report_late(&env,&reporter,&late_member,round)}
    pub fn dispute(env:Env,member:Address,evidence_hash:BytesN<32>)->Result<(),types::CircleError>{contract::dispute(&env,&member,&evidence_hash)}
    pub fn raise_dispute(env:Env,member:Address,evidence_hash:BytesN<32>)->Result<(),types::CircleError>{contract::raise_dispute(&env,&member,&evidence_hash)}
    pub fn resolve_dispute(env:Env,admin:Address,resolution:u32)->Result<(),types::CircleError>{contract::resolve_dispute(&env,&admin,resolution)}
    pub fn challenge_dispute_resolution(env:Env,member:Address)->Result<(),types::CircleError>{contract::challenge_dispute_resolution(&env,&member)}
    pub fn execute_dispute_resolution(env:Env)->Result<(),types::CircleError>{contract::execute_dispute_resolution(&env)}
    pub fn get_status(env:Env)->types::Circle{contract::get_status(&env)}
    pub fn get_members(env:Env)->soroban_sdk::Vec<types::Member>{contract::get_members(&env)}
    pub fn get_contributions(env:Env,member:Address,page:u32,page_size:u32)->soroban_sdk::Vec<types::Contribution>{contract::get_contributions(&env,&member,page,page_size)}

    pub fn pause_circle(env:Env,admin:Address)->Result<(),types::CircleError>{contract::pause_circle(&env,&admin)}
    pub fn unpause_circle(env:Env,admin:Address)->Result<(),types::CircleError>{contract::unpause_circle(&env,&admin)}
    pub fn batch_invite(env:Env,caller:Address,members:soroban_sdk::Vec<Address>)->Result<(),types::CircleError>{contract::batch_invite(&env,&caller,&members)}
    pub fn batch_payout(env:Env,caller:Address,recipients:soroban_sdk::Vec<Address>,amounts:soroban_sdk::Vec<i128>,round:u32)->Result<(),types::CircleError>{contract::batch_payout(&env,&caller,&recipients,&amounts,round)}
    pub fn batch_exit(env:Env,caller:Address,members:soroban_sdk::Vec<Address>)->Result<(),types::CircleError>{contract::batch_exit(&env,&caller,&members)}
    pub fn register_referral(env:Env,referrer:Address,referred:Address,bonus_pct:u32)->Result<(),types::CircleError>{contract::register_referral(&env,&referrer,&referred,bonus_pct)}
    pub fn claim_referral_bonus(env:Env,referrer:Address)->Result<(),types::CircleError>{contract::claim_referral_bonus(&env,&referrer)}
    pub fn update_streak(env:Env,member:Address,round:u32)->Result<(),types::CircleError>{contract::update_streak(&env,&member,round)}
    pub fn claim_streak_bonus(env:Env,member:Address)->Result<(),types::CircleError>{contract::claim_streak_bonus(&env,&member)}
    pub fn set_streak_bonus_config(env:Env,admin:Address,config:types::StreakBonusConfig)->Result<(),types::CircleError>{contract::set_streak_bonus_config(&env,&admin,config)}
    pub fn get_referrals(env:Env)->soroban_sdk::Vec<types::Referral>{contract::get_referrals(&env)}
    pub fn get_streaks(env:Env)->soroban_sdk::Vec<types::Streak>{contract::get_streaks(&env)}
    pub fn get_member_streak(env:Env,member:Address)->types::Streak{contract::get_member_streak(&env,&member)}
    pub fn set_reputation_registry(env:Env,admin:Address,registry:Address)->Result<(),types::CircleError>{contract::set_reputation_registry(&env,&admin,&registry)}
    pub fn get_reputation_registry(env:Env)->Option<Address>{contract::get_reputation_registry(&env)}
    pub fn set_treasury(env:Env,admin:Address,treasury:Address)->Result<(),types::CircleError>{contract::set_treasury(&env,&admin,&treasury)}
    pub fn set_token(env:Env,admin:Address,token:Address)->Result<(),types::CircleError>{contract::set_token(&env,&admin,&token)}
    pub fn set_fee_bps(env:Env,admin:Address,fee_bps:u32)->Result<(),types::CircleError>{contract::set_fee_bps(&env,&admin,fee_bps)}
    pub fn set_allowlist(env:Env,admin:Address,allowlist:soroban_sdk::Vec<Address>)->Result<(),types::CircleError>{contract::set_allowlist(&env,&admin,allowlist)}
    pub fn get_allowlist(env:Env)->soroban_sdk::Vec<Address>{contract::get_allowlist(&env)}
    pub fn set_oracle(env:Env,admin:Address,oracle:Address)->Result<(),types::CircleError>{contract::set_oracle(&env,&admin,&oracle)}
    pub fn set_fallback_oracle(env:Env,admin:Address,oracle:Address)->Result<(),types::CircleError>{contract::set_fallback_oracle(&env,&admin,&oracle)}
    pub fn get_oracle(env:Env)->Option<Address>{contract::get_oracle(&env)}
    pub fn get_fallback_oracle(env:Env)->Option<Address>{contract::get_fallback_oracle(&env)}
    pub fn set_oracle_pubkey(env:Env,admin:Address,pubkey:BytesN<32>)->Result<(),types::CircleError>{contract::set_oracle_pubkey(&env,&admin,&pubkey)}
    pub fn get_oracle_pubkey(env:Env)->Option<BytesN<32>>{contract::get_oracle_pubkey(&env)}
    pub fn get_oracle_cache(env:Env)->Option<types::OracleCache>{contract::get_oracle_cache(&env)}
    pub fn get_yield_rate(env:Env,round:u32)->Result<i128,types::CircleError>{contract::get_yield_rate(&env,round)}
    pub fn check_oracle_source(env:Env,oracle:Address)->Result<(),types::CircleError>{contract::check_oracle_source(&env,&oracle)}
    pub fn check_tx_expiry(env:Env,valid_until_ledger:u32)->Result<(),types::CircleError>{contract::check_tx_expiry(&env,valid_until_ledger)}
    pub fn contribute_with_expiry(env:Env,member:Address,amount:i128,round:u32,valid_until_ledger:u32)->Result<(),types::CircleError>{contract::contribute_with_expiry(&env,&member,amount,round,valid_until_ledger)}
    pub fn trigger_payout_with_expiry(env:Env,caller:Address,round:u32,valid_until_ledger:u32)->Result<(),types::CircleError>{contract::trigger_payout_with_expiry(&env,&caller,round,valid_until_ledger)}
    pub fn configure_multisig(env:Env,admin:Address,admins:soroban_sdk::Vec<Address>,threshold:u32)->Result<(),types::CircleError>{contract::configure_multisig(&env,&admin,&admins,threshold)}
    pub fn disable_multisig(env:Env,admin:Address)->Result<(),types::CircleError>{contract::disable_multisig(&env,&admin)}
    pub fn get_multisig_config(env:Env)->Option<types::MultisigConfig>{contract::get_multisig_config(&env)}
    pub fn approve_action(env:Env,approver:Address,action_id:BytesN<32>)->Result<(),types::CircleError>{contract::approve_action(&env,&approver,&action_id)}
    pub fn get_action_approvals(env:Env,action_id:BytesN<32>)->soroban_sdk::Vec<Address>{contract::get_action_approvals(&env,&action_id)}
    pub fn trigger_payout_multisig(env:Env,caller:Address,round:u32,action_id:BytesN<32>)->Result<(),types::CircleError>{contract::trigger_payout_multisig(&env,&caller,round,&action_id)}
    pub fn set_fee_bps_multisig(env:Env,caller:Address,fee_bps:u32,action_id:BytesN<32>)->Result<(),types::CircleError>{contract::set_fee_bps_multisig(&env,&caller,fee_bps,&action_id)}
    pub fn set_treasury_multisig(env:Env,caller:Address,treasury:Address,action_id:BytesN<32>)->Result<(),types::CircleError>{contract::set_treasury_multisig(&env,&caller,&treasury,&action_id)}
    pub fn resolve_dispute_multisig(env:Env,caller:Address,resolution:u32,action_id:BytesN<32>)->Result<(),types::CircleError>{contract::resolve_dispute_multisig(&env,&caller,resolution,&action_id)}
    pub fn migrate(env:Env,caller:Address)->Result<(),types::CircleError>{migration::migrate(&env,&caller)}
    pub fn verify_state(env:Env)->Result<(),types::CircleError>{migration::verify(&env)}
    pub fn get_storage_version(env:Env)->u32{migration::current_version(&env)}
    pub fn get_analytics(env:Env)->types::CircleAnalytics{analytics::load(&env)}
    pub fn get_member_stats(env:Env,member:Address)->types::MemberStats{analytics::get_member_stats(&env,&member)}
    pub fn get_all_member_stats(env:Env)->Vec<types::MemberStats>{analytics::get_all_member_stats(&env)}
    pub fn get_last_join_attempt(env:Env,member:Address)->Option<types::JoinAttempt>{env.storage().persistent().get(&types::DataKey::JoinAttempt(member))}
}

