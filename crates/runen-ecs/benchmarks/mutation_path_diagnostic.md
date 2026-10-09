# Mutation journal path diagnostic (#143)

This is **test-only evidence instrumentation**, not a proposed journal
implementation. The private test is deliberately ignored by normal
`cargo test`, and production journal, cursor and worker behavior is unchanged.

## Accepted motivating evidence

[One-million-entity run #142](https://github.com/dornglut/runen-ecs/actions/runs/37897502635):
eight independent writers took 369.64 ms serial, 635.40 ms at worker
capacity 2, and 737.97 ms at capacity 4 on a two-core/four-logical-CPU
host; eight compatible readers improved from 31.25 ms to 16.14 ms.
The mutable slowdown could result from shared admission, vector growth,
journal replay, memory pressure or preparation. No cause has yet been isolated.

## Separately measured slices

The test constructs true prevalidated row-metadata targets using a real
World and the existing journal machinery. It divides a **fixed total**
of 10k, 100k or 1m events among 1/2/4 scoped workers, with three raw
repetitions of each operation:

| Operation | Time boundary | Interpretation |
| --- | --- | --- |
| `vector_only` | Thread start/join + allocate/populate `Vec<MutationEvent>` | No capacity admission or World replay; **not** a valid production strategy |
| `reservation_only` | Thread start/join + existing shared `reserve_next_event()` | Isolates common admission without journal vectors |
| `concurrent_record` | Thread start/join + actual prevalidated `MutationJournal` record calls | Admission and vector growth; excludes component payload mutation/preparation |
| `concurrent_replay` | Canonical serial replay of completed worker journals | Starts only after worker join; excludes record time |
| `serial_record` / `serial_replay` | Real serial journal recording and replay | Untheaded diagnostic reference |

Every full-journal case asserts exact event count, final cursor position
and row changed metadata. The repeated events intentionally refer to
the same valid entity. This probes journal overhead, **not** cache and
component-update behavior of one million different entities.

Raw output is newline-separated:
```text
mutation_path_sample,events=N,workers=W,round=R,operation=NAME,nanoseconds=VALUE
```

Do not use timings from different hosts, different event totals, or
synthetic vector/reservation controls as equivalent full-schedule
performance comparisons. Record raw runs with host topology, compiler,
and immutable feature-head provenance. A production change requires
independent semantic proof and same-host paired full-schedule evidence.

## Invocation

```sh
RUNEN_ECS_MUTATION_DIAGNOSTIC_SIZES=all \
  cargo +1.98.1 test --release --locked -p runen-ecs --lib \
  mutation_path_scaling_diagnostic -- --ignored --nocapture
```

`RUNEN_ECS_MUTATION_DIAGNOSTIC_SIZES` accepts `10000`,
`100000`, `1000000` or `all`; unset defaults to 10k.
Normal `cargo validate` compiles the ignored test but does not run it.
The read-only diagnostic workflow explicitly runs all three sizes on
a pinned toolchain and saves the raw output.

## Four-physical-core admission attribution (#154)

The accepted full-schedule [ARM64 phase run](https://github.com/dornglut/runen-ecs/actions/runs/37926200890)
on a four-physical-core Neoverse-N2 host measured substantial worker execution
time after the replay improvement in [#152](https://github.com/dornglut/runen-ecs/issues/152).
The original journal path proof uses a two-physical-core x64 runner and cannot
isolate contention on four actual cores. To close that evidence gap, the
**one existing** exact-head mutation-path diagnostic workflow now runs on
`ubuntu-24.04-arm`; it still uses exactly the same private ignored release
test, 10k/100k/1m event counts, three repeats and worker1/2/4 variants.
No new permanent CI lane, required threshold, source instrumentation, cursor
change or production implementation is added.

The job records actual `lscpu` topology, the checked-out immutable source
SHA and pinned toolchain, and includes the architecture in the raw artifact
name. Four vCPUs do not guarantee four *physical* cores without inspecting
the run output. For each fixed total event count, compare `reservation_only`
against `vector_only` and `concurrent_record` within the **same host run**.
The synthetic path targets one row repeatedly, rather than eight million
distinct World-row mutations; it excludes component payload writes and
structural projection preparation. The result can bound a worker-side
admission hypothesis but cannot establish any full-schedule improvement or
authorize cursor batching, admission policy or worker-count changes.

This remains a microbenchmark alongside the accepted public
`parallel_scaling` suite, not permission to weaken absolute cursor
capacity, failure-prefix retention, serial-rank reconciliation, or
unsafe World projection guarantees from ADR 0003.
