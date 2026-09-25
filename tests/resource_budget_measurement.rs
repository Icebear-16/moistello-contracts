// Integration-test plan.
//
// Use the actual Soroban contract client and test environment:
//
// 1. Build a circle with representative state.
// 2. Invoke batch_invite with 1, 5, 10, 25, and the configured maximum.
// 3. Capture env.cost_estimate().budget() and any available resource metrics.
// 4. Repeat with maximum-size addresses/records and event payloads.
// 5. Repeat for resolve_vote.
// 6. Assert the configured ceiling is accepted and ceiling + 1 is rejected.
//
// Keep this separate from simple unit tests because resource measurements
// depend on the complete contract invocation and host configuration.
