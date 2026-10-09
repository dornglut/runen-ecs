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

## Subsequent replay-publication experiment (#152)

The original #145 comparison proved the private atomic admission change;
its accepted source result remains historical. [Issue #152](https://github.com/dornglut/runen-ecs/issues/152)
owns a **different** proof question: whether reducing repeated *type-level*
publication during private, invoker-owned mutation-journal replay saves
substantial **end-to-end** mutable schedule time while preserving every
individual event's checked cursor and row changed position.

The same existing ABBA proof workflow now runs its **one** read-only job on
`ubuntu-24.04-arm` instead of the previous two-physical-core x64 host.
The accepted [ARM64 1m scaling run](https://github.com/dornglut/runen-ecs/actions/runs/37921720564)
observed an ARM Neoverse-N2 with four actual physical cores. Each proof
job MUST report its own `lscpu`: this is an observed host configuration,
not a permanent promise inferred from its runner label. Raw artifact names
carry their CPU architecture. Never pool its absolute elapsed times with
historical x64 results.

Source revisions are checked before each base/head/head/base run, with the
same eight-system, 1m-entity Criterion suite and all serial/worker modes.
Only a matched same-host result may justify a source-level performance
decision. Exact per-row metadata, secondary-index invalidation, fallback
events, resource interleaving, error/panic prefixes, terminal cursor
overflow and all existing hosted safety suites remain mandatory. The
workflow does not change its scheduling cadence, job count, user-visible
policy or runtime behavior.
