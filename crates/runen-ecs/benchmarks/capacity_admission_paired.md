# Journal admission optimization evidence (#145)

Accepted source base: `cb45dd8a0f56a86e7064076dfedc83434ff81bb5`.

The private candidate replaces contended per-event mutex admission with a
saturating `AtomicU64` fast tier only on targets that support 64-bit atomics.
It MUST admit at most `u128::MAX - ((epoch << 64) | tick)` events. Once fast
capacity reaches `u64::MAX`, the exact `Mutex<u128>` tail admits additional
positions only if they exist. Targets without 64-bit atomics retain the full
mutex quota. No wrap, falsely exhausted capacity or retroactive admission
is permitted.

Atomic `Relaxed` ordering ensures unique quota claims only. Worker join
synchronizes payload and journals before invoker-side deterministic replay.
Public serial-oracle mutation semantics, repeated access events, change
positions, worker panic/error prefix, World lineage and invariant priority
remain unchanged. Tests cover terminal remaining slots 0/1/2, a concurrent
last-position race, fast-tier saturation and fast-to-u128 boundary.

The `ecs-capacity-admission-paired.yml` proof workflow runs the unchanged
accepted `parallel_scaling` benchmark at 1,000,000 entities, same toolchain,
one runner, in **base/head/head/base** order, archiving raw Criterion samples
and host topology. No performance threshold is encoded. A measurable
microbenchmark speedup is NOT sufficient for merge: independent-writer
whole-schedule gains and no substantial serial/read-only/conflicting-writer
regressions must be demonstrated alongside canonical Validation, Miri,
AddressSanitizer and ThreadSanitizer on the exact reviewed candidate.
Stop and do not merge when results do not justify it.

This file describes the experimental proof; outcomes belong to the issue
and PR exact-head evidence, not to unverified projections.
