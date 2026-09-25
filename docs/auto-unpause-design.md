# Automatic Unpause Design

## Semantics

| Input | Behavior |
|---|---|
| `None` | Indefinite pause |
| `Some(n)` where `n > 0` | Pause expires after `n` seconds |
| `Some(0)` | Rejected as invalid |

The expiration condition is:

```text
current_ledger_timestamp - pause_started_at >= pause_duration
```

Use checked or saturating arithmetic. Ledger timestamps are unsigned values,
and the contract must not panic because of timestamp subtraction.

## Lazy expiry

Soroban contracts do not run a background scheduler. Therefore, the contract
must check expiration at the beginning of each mutation that is protected by
the pause guard.

When the duration has expired:

1. Clear the paused flag.
2. Clear the start timestamp.
3. Clear the duration.
4. Allow the current mutation to continue.
5. Optionally emit an automatic-unpause event if the event policy is approved.

## Security requirements

- Preserve existing admin authentication.
- Preserve existing multisig enforcement.
- Do not allow arbitrary callers to manually unpause.
- Reject zero duration.
- Ensure every pause-protected mutation uses the expiry-aware guard.
- Keep pause metadata in the same storage scope as the pause flag.
- Add tests for timestamp boundaries and indefinite pauses.

## Compatibility

Changing the public `pause_circle` signature may affect generated clients,
bindings, integration tests, and callers. Consider whether a new entry point
or an optional argument is compatible with the deployed contract interface
and migration policy.
