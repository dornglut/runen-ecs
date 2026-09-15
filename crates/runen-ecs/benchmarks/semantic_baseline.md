# RunenECS semantic benchmark baseline

This baseline measures the ECS semantics that must remain usable at the
standalone extraction boundary: insertion, query iteration, archetype
transitions, serial schedule execution, and deferred command application.

## Historical predecessor evidence

The following C9 measurements are retained as provenance from the predecessor
harness. They are not equivalent regression thresholds for the corrected
harness below.

- Measurement commit: `57f478bf4b9c533ec435080726358f89a002ec69`
- Predecessor C9 measurement: `4280814536c2f0262eb4ba16f6bc495ed5907153`
- Accepted Runenwerk C9 transfer input: `b7e3d55c76be0acebf3ce260e7ee282ea1787e29`
- Predecessor baseline reference: the semantic baseline measured at the C9 implementation boundary; this standalone transfer does not include `benchmarks/phase6/**` historical dumps.
- Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`
- Target: `aarch64-apple-darwin`
- Host: Apple M3, arm64
- Command: `cargo +stable bench -p runen-ecs --bench semantic_baseline --locked -- --quick`
- Configuration: optimized Criterion benchmark, quick mode

The predecessor rows had these semantics:

- Entity/component insertion included fresh-World construction and one
  `black_box` observation per inserted entity.
- Archetype transition actually replaced `Position`, did not change the
  component set, and included World setup plus target-entity discovery in the
  timed closure.
- Deferred command application reused a growing World and included unrelated
  query/count resource work.

Observed predecessor medians:

| Scenario | Median |
| --- | ---: |
| Entity/component insertion (1000) | 943.31 µs |
| Query iteration (10000) | 522.83 µs |
| Archetype transition (1000) | 1.2620 ms |
| Serial schedule execution (10000) | 716.34 µs |
| Deferred command application | 695.53 µs |

These numbers remain historical provenance only and must not be compared as
corrected workload thresholds.

## Superseded first corrected measurement

The first corrected harness and its evidence commit are retained as review
provenance:

- Harness H: `a8adcdae27b5dfb41fc4c5cd8021ad58b30d8819`
- Evidence E: `97949a31cc490e545cd79e9d01b78e21e467406d`

Those measurements are superseded for acceptance because independent review
found that `build_transition_fixture()` performed the full marker-absence,
query-count and per-entity metadata scan during every Criterion setup. That
work was untimed but duplicated the dedicated one-off sanity proof and
pre-touched the transition fixture immediately before measurement.

## Corrected semantic baseline

Pending measurement of the new immutable harness H2. H2 removes only that
per-sample transition assertion; the dedicated `prove_transition_fixture()`
sanity path remains. The deferred fixture still prepares and caches its
one-system schedule plan through the World-independent `Runtime::validate()`
contract before measurement, without executing user code on a throwaway World.

The H2 measurement will record the exact harness SHA, compiler/toolchain,
target, host, command, Criterion configuration and effective batch choice,
together with all five observed medians and semantic cardinalities.
