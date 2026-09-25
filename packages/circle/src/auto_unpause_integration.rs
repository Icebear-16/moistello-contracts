// Integration sketch for packages/circle/src/contract.rs.
//
// Replace every mutation guard of this form:
//
//     pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
//
// with:
//
//     pause_auto_unpause::when_not_paused(env)
//         .map_err(|_| CircleError::ContractPaused)?;
//
// The pause_circle entry point should accept:
//
//     duration_seconds: Option<u64>
//
// and call:
//
//     pause_auto_unpause::pause(env, &admin, duration_seconds)
//
// Manual unpause should call:
//
//     pause_auto_unpause::unpause(env, &admin)
//
// Keep the existing admin, multisig, and authorization checks unchanged.
// Do not automatically unpause inside read-only methods unless the product
// explicitly wants reads to mutate storage.
