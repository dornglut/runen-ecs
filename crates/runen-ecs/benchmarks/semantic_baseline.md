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

## Corrected semantic baseline

The corrected harness was measured without changing its source after the
following immutable harness commit:

- Measured harness commit (H): `a8adcdae27b5dfb41fc4c5cd8021ad58b30d8819`
- Compiler: `rustc 1.98.1 (48a229cea 2026-09-01)`
- Active toolchain: `stable-aarch64-apple-darwin` (repository toolchain override)
- Target/host: `aarch64-apple-darwin`
- Host architecture/CPU: `arm64`, Apple M3
- Command: `cargo bench -p runen-ecs --bench semantic_baseline --locked`
- Criterion configuration: normal optimized Criterion run, default warm-up and
  sampling configuration; Gnuplot was unavailable, so Criterion used its
  Plotters backend.
- Reset-fixture batch choice: `BatchSize::SmallInput` for insertion, transition
  and deferred application. The normal run showed no excessive memory or
  external-resource pressure, so no escalation was necessary.

The corrected deferred fixture registers only `queue_spawn`, calls
`Runtime::validate()` during untimed setup to validate and cache the schedule
plan, and then runs one schedule invocation against each fresh target World.
It does not execute user code on a throwaway World or bind the Runtime to a
bootstrap World.

Corrected semantic cardinalities and observed medians:

| Scenario | Semantic cardinality | Median |
| --- | --- | ---: |
| Entity/component insertion (1000) | 1000 fresh `(Position, Velocity)` spawns | 916.79 µs |
| Query iteration (10000) | 10,000 matching `(Position, Velocity)` entities | 679.40 µs |
| Archetype transition (1000) | 1000 `{Position, Velocity}` → `{Position, Velocity, TransitionMarker}` additions | 673.64 µs |
| Serial schedule execution (10000) | 10,000 mutable `Position` components per invocation | 797.06 µs |
| Deferred command application | one `Commands` spawn published by one `Update` invocation | 1.6251 µs |

These corrected medians belong only to H and are not comparable with the
historical predecessor rows above. This evidence commit changes documentation
only; `H..E` must contain no benchmark-source or runtime changes.
