# RunenECS semantic benchmark baseline

This baseline measures the ECS semantics that must remain usable at the
standalone extraction boundary: insertion, query iteration, archetype
transitions, serial schedule execution, and deferred command application.

- Measurement commit: `4280814536c2f0262eb4ba16f6bc495ed5907153`
- Accepted Runenwerk C9 transfer input: `b7e3d55c76be0acebf3ce260e7ee282ea1787e29`
- Predecessor baseline reference: the semantic baseline measured at the C9 implementation boundary; this standalone transfer does not include `benchmarks/phase6/**` historical dumps.
- Toolchain: `rustc 1.98.1 (48a229cea 2026-09-01)`
- Target: `aarch64-apple-darwin`
- Host: Apple M3, arm64
- Command: `cargo +stable bench -p runen-ecs --bench semantic_baseline --locked -- --quick`
- Configuration: optimized Criterion benchmark, quick mode

Observed medians from the run above:

| Scenario | Median |
| --- | ---: |
| Entity/component insertion (1000) | 919.08 µs |
| Query iteration (10000) | 695.22 µs |
| Archetype transition (1000) | 1.2747 ms |
| Serial schedule execution (10000) | 711.25 µs |
| Deferred command application | 744.13 µs |
