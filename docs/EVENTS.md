# Circle Contract Event Schema

This document describes the events emitted by the `circle` contract and their
topic layout, so off-chain indexers can subscribe to the fields they need
without decoding every event body.

Soroban events are published as `(topics, data)`. The first topic is always
the emitting contract address. The remaining topics are indexed fields chosen
for the query patterns indexers commonly need (e.g. "all contributions for a
member", "all payouts for a round"). The event body (`data`) carries the full
struct.

| Event | Topics `(contract, symbol, ...)` | Data type | Common query |
|---|---|---|---|
| Member joined | `symbol_short!("joined")`, `member` | `MemberJoined { member, position }` | All join events for a member |
| Contribution recorded | `symbol_short!("contrib")`, `member`, `round` | `ContributionRecorded { member, round, amount, on_time }` | All contributions for a member; all contributions in a round |
| Payout executed | `symbol_short!("payout")`, `recipient`, `round`, `payout_type` | `PayoutExecuted { recipient, round, amount, fee, payout_type }` | All payouts for a round; all payouts of a given type; all payouts to a member |
| Auction bid placed | `symbol_short!("bid")`, `bidder`, `round` | `AuctionBidPlaced { bidder, discount_bips, round }` | All bids for a round |
| Vote cast | `symbol_short!("vote")`, `voter`, `round` | `VoteCast { voter, vote_for, round }` | All votes for a round |
| Member exited | `symbol_short!("exited")`, `member` | `MemberExited { member, penalty }` | All exits for a member |
| Member defaulted | `symbol_short!("default")`, `member`, `round` | `MemberDefaulted { member, strikes }` | All defaults for a member/round |
| Circle cancelled | `symbol_short!("cancel")` | `CircleCancelled { circle_id, cancelled_by, cancelled_at }` | Cancellation for a specific circle contract |
| Dispute raised | `symbol_short!("disputed")`, `member` | `DisputeRaised { member, evidence_hash }` | All disputes raised by a member |
| Dispute resolution proposed | `symbol_short!("dis_prop")`, `admin` | `DisputeResolutionProposed { admin, resolution, execute_after }` | Pending resolutions awaiting timelock |
| Dispute resolution challenged | `symbol_short!("dis_chal")`, `member` | `DisputeResolutionChallenged { member }` | Challenges raised against a pending resolution |
| Dispute window extended | `symbol_short!("dis_ext")`, `challenger` | `DisputeWindowExtended { challenger, extension_seconds, new_execute_after }` | Window extensions requested by a challenger |
| Dispute resolved | `symbol_short!("resolved")`, `admin` | `DisputeResolved { admin, resolution }` | Finalized dispute resolutions |
| Organizer replaced | `symbol_short!("org_rep")`, `new_organizer` | `OrganizerReplaced { old_organizer, new_organizer }` | Supermajority organizer replacement |
| Referral registered | `symbol_short!("referral")`, `referrer` | `ReferralRegistered { referrer, referred, bonus_pct }` | All referrals by a referrer |
| Batch exit executed | `symbol_short!("bat_exit")`, `round` | `BatchExitExecuted { round, exited_count, failed_count }` | Batch exit outcomes per round |

## Notes for indexers

- `member` / `recipient` / `bidder` / `voter` / `challenger` topics are `Address` values and
  can be filtered directly by Soroban RPC `getEvents` topic filters.
- `round` is a `u32` topic present on all per-round events, enabling a single
  filter to reconstruct a full round's activity.
- `payout_type` mirrors `Circle.payout_type` (`0` = Random, `1` = Fixed,
  `2` = Auction, `3` = Vote) and is included on payout events so indexers can
  aggregate payouts by strategy without loading circle state.
- Every event still carries the full data struct in the event body; topics
  are additive indexing hints, not a replacement for decoding the body.

---

# Factory Contract Event Schema

This document describes the events emitted by the `circle-factory` contract and their
topic layout for off-chain indexers.

| Event | Topics `(contract, symbol, ...)` | Data type | Common query |
|---|---|---|---|
| Circle created | `symbol_short!("created")` | `CircleCreated { address, admin, token, timestamp }` | All created circles with canonical address, initial admin, and token |
| Circle deployed | `symbol_short!("deploy")` | `CircleDeployed { creator, circle_id, name }` | Deployed circles by creator and name |
| Template created | `symbol_short!("tmpl_new")` | `TemplateCreated { id, name }` | New circle templates |
| Template deployed | `symbol_short!("tmpl_dep")` | `TemplateDeployed { template_id, circle_id, creator }` | Deployments from templates |
| Fee config updated | `symbol_short!("fee_cfg")` | `FeeConfigUpdated { old_fee_bps, new_fee_bps, updated_by }` | Factory fee updates |
| Deploy fee paid | `symbol_short!("fee_paid")` | `DeployFeePaid { organizer, treasury, amount, token }` | Deployment fee payments to treasury |
| Deploy fee config updated | `symbol_short!("dep_fee")` | `DeployFeeConfigUpdated { old_fee, new_fee, treasury }` | Factory deploy fee configuration updates |
| Migration batch progress | `symbol_short!("mig_batch")` | `MigrationBatchProgress { migrated, start_index }` | Batch circle migration progress |

## Notes for factory indexers

- `CircleCreated` carries canonical deployed `address`, initial `admin`, payment `token`, and block `timestamp`, allowing indexers to correlate circles without relying on transaction sequence numbers.

---

# Staking Contract Event Schema

This document describes the events emitted by the `staking` contract and their
topic layout for off-chain indexers.

| Event | Topics | Data type | Common query |
|---|---|---|---|
| Tokens staked | `staked, user` | `Staked { user, amount, period, multiplier, voting_power }` | New stake positions and voting power |
| Unstake initiated | `unstake, user` | `UnstakeInitiated { user, amount, claimable_time }` | Unbonding positions and unlock schedule |
| Tokens claimed | `claimed, user` | `Claimed { user, amount }` | Principal withdrawals after unbonding |
| Reward config updated | `reward_cfg` | `RewardConfigUpdated { old_apy_bps, new_apy_bps, updated_at }` | APY rate adjustments |
| Rewards distributed | `reward_dist, user` | `RewardsDistributed { user, amount }` | Claimed or distributed staking rewards |

