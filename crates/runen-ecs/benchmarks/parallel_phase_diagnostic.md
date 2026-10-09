# Parallel cohort phase decomposition (#147)

This is an **observation-only**, ignored release test. It is not a
production executor change, performance gate, or authorization to coalesce
change events.

## Why this exists

Accepted [#145](https://github.com/dornglut/runen-ecs/issues/145)
reduced the cost of concurrent mutation-capacity admissions but, in the
accepted 1m-entity paired run, independent writes at capacity 2 and 4
still took about 331ms and 467ms versus about 299ms serial. The remaining
cost cannot be assigned to a single owner without full-schedule evidence.
The older journal microbenchmark recorded repeated changes to one entity;
this probe uses **distinct dense archetype rows**, with eight independent
mutable component columns and eight real Query<&mut Cn> systems.

## Measurement ownership

A test-only invoker-thread collector accumulates these intervals for each
successful worker cohort:

- `prepare_ns`: create structural-freeze lease and prepare all worker
  query projections/capacity before thread launch.
- `execute_ns`: spawn scoped worker threads, execute every selected
  system and join/drain all workers. Includes per-row mutation admission,
  journal event push, payload updates and thread creation/join overhead.
- `reconcile_ns`: canonical invoker-side journal replay after all workers
  joined; includes change cursor updates and component/index metadata.
- `total_ns`: end-to-end Runtime schedule invocation elapsed wall time,
  including planning and other work beyond the three measured slices.

The total is **not** the sum of CPU times across threads; intervals are
invoker-wall durations. `prepare+execute+reconcile` may be below total.
Serial has no worker cohorts, so its slices are zero, not unavailable
measurements of equivalent serial internals.

For 10k, 100k, and 1m entities, each of serial and worker capacities
1/2/4 executes **three full schedule iterations on its own initialized
World**. One million entities means eight million component visits and
conservative mutation events per schedule. World construction and final
verification of all eight component columns are outside timed invocations.
The diagnostic checks exact final changed component values after three
rounds and emits 36 raw observations, one per size/capacity/round.

Raw lines:
```text
ecs_phase_sample,entities=N,workers=W,round=R,total_ns=...,prepare_ns=...,execute_ns=...,reconcile_ns=...,cohorts=...
```

`workers=0` means the production serial oracle. The workload's worker
projection path uses accepted typed contiguous archetype spans and
prevalidated per-row changed-cursor targets, not its fallback scalar hash
map projections. Do not misinterpret this diagnostic as proof of speedup
on larger core counts or a substitute for the public Criterion benchmark.

## Run

```sh
RUNEN_ECS_PHASE_DIAGNOSTIC_SIZES=all \
  cargo +1.98.1 test --release --locked -p runen-ecs --lib \
  parallel_phase_scaling_diagnostic -- --ignored --nocapture
```

Accepted inputs: `10000`, `100000`, `1000000`, `all`. An
unset input measures only 10k, avoiding accidental large local test
workloads. The read-only GitHub workflow runs all sizes on the immutable
pull-request head and archives every raw sample with CPU topology and
the exact toolchain. No duration becomes a branch protection threshold.

The `#[cfg(test)]` instrumentation exists only in Rust test builds.
Canonical tests compile but do not execute the ignored diagnostic.
Production builds have **no timing collector, environment lookup, or
additional runtime instrumentation**. All existing conformance,
Miri, ASan and TSan proofs remain authoritative for safety.

Interpret the measured phase shares before proposing any further
journal replay, worker projection, scheduling, or storage redesign.
