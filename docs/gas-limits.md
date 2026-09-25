# Contract Gas and Resource Limits

## Proposed initial policy

| Operation | Initial maximum | Status |
|---|---:|---|
| `batch_invite` | 50 members | Provisional; benchmark required |
| `resolve_vote` | 50 votes | Provisional; benchmark required |

These are application-level guards, not a replacement for Soroban's
transaction resource limits.

## Why both validation and measurement are required

The total cost of a batch depends on more than its item count. It can vary
with:

- Number of storage reads and writes.
- Number and size of emitted events.
- Authorization checks.
- Nested contract calls.
- Serialized argument sizes.
- Vote/member record size.
- Any loops or aggregation performed per entry.

The implementation should reject a batch before expensive work begins.
The test suite should measure batches at increasing sizes and record the
resource profile.

## Benchmark procedure

1. Test batches of 1, 5, 10, 25, 50, and the configured maximum.
2. Include realistic and worst-case record sizes.
3. Record CPU instructions, memory, ledger reads/writes, and event sizes where
   supported by the SDK version.
4. Keep a safety margin below the observed resource limit.
5. Test one item above the configured maximum and assert a typed rejection.
6. Re-run the benchmark after changing storage schemas or emitted events.

Soroban resource limits are enforced at transaction execution time. The
contract should not claim that an application-level estimate guarantees the
transaction will fit every network budget.
