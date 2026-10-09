# Parallel schedule scaling benchmark

This diagnostic benchmark complements, rather than replaces, the 10,000-row
`query_execution` decomposition and corrected `semantic_baseline` evidence.

## Questions

- What is the eight-system end-to-end serial/parallel dispatch floor?
- How does read-only work scale across eight nonconflicting systems?
- How does eight-way disjoint component mutation scale, including one
  conservative mutation event and reconciliation per mutable component exposure?
- How does a roster of eight same-component writers behave when the scheduler
  must serialize them despite a larger worker capacity?

The benchmark deliberately defines **eight systems** in each roster. A capacity
of eight may therefore admit eight independent workers in a single cohort; it
must not be interpreted as eight workers on a host with fewer available CPUs.
Access-incompatible systems never become concurrently runnable because capacity
increases.

## Workload model

Each nonempty fixture has exactly `N` entities with eight `u64` components
(`C0..C7`) in a single dense archetype, created with one derived bundle.
The following modes share the same registration and starting World shape:

| Workload | System behavior | Component visits per schedule |
| --- | --- | ---: |
| `empty` | Eight no-op systems on an empty World | 0 |
| `reads` | Eight systems read a distinct component column each | `8 * N` |
| `independent_writes` | Eight systems increment distinct component columns | `8 * N` |
| `conflicting_writes` | Eight systems increment the same `C0` column | `8 * N` |

For each workload, measure the exact same eight registered systems through
`Runtime::run_schedule` and `Runtime::run_schedule_parallel` at worker
capacities **1, 2, 4, 8**. Benchmarks measure full steady-state schedule
invocations, including worker preparation, launch/join, mutation-journal
recording/reconciliation and public schedule overhead. They do **not** time
fixture construction or initial schedule-plan validation.

All mutable operations use wrapping `u64` increments and observable accumulated
values so repeated Criterion iterations preserve cardinality and do real work;
no per-iteration structural growth, unbounded entity allocation, random inputs,
clock reads, or changes in selected archetypes are part of the timed workload.

An untimed preflight compares repeated serial and parallel execution for every
workload and all four capacities against identical 16-row fixtures, and checks
expected component sums. This verifies successful payload behavior; full
change-cursor, panic, deferred-publication, and race conformance remains owned by
the existing canonical and supplemental executor suites.

Criterion's `Throughput::Elements(8 * N)` expresses **component visits per
schedule invocation**, not unique entities or achieved SIMD work. Interpret
`empty` by elapsed time only.

## Selecting scale

`RUNEN_ECS_BENCH_SIZE` is validated **before** any large fixture is built.
Unspecified defaults to 1,000 rows to keep ordinary PR smoke bounded.
The accepted values are `1000`, `10000`, `100000`, `1000000`, `all`.
Selecting `all` runs every count in ascending order. With no `all` override,
the million-row case remains a first-class benchmark variant explicitly
selectable for dedicated measurement.

~~~sh
# Bounded smoke / inexpensive local experiment:
cargo +1.98.1 bench -p runen-ecs --bench parallel_scaling --locked

# Dedicated million-entity workload:
RUNEN_ECS_BENCH_SIZE=1000000 cargo +1.98.1 bench -p runen-ecs --bench parallel_scaling --locked

# Full 1k/10k/100k/1m cardinality sweep:
RUNEN_ECS_BENCH_SIZE=all cargo +1.98.1 bench -p runen-ecs --bench parallel_scaling --locked
~~~

The dedicated `.github/workflows/ecs-parallel-scaling-benchmark.yml`
selects **1,000 rows on Ubuntu x64** for unchanged, bounded PR smoke.
`workflow_dispatch` allows choosing a size and an explicitly allow-listed
standard runner: `ubuntu-24.04` (x64) or `ubuntu-24.04-arm` (ARM64).
The ARM64 option is useful for testing the accepted same-source workloads on
an architecturally different CPU without making expensive million-row runs
a permanent CI requirement. It is **manual opt-in only**: specifying a
runner does not change the entity count, and choosing `1000000` runs a
single million-row cardinality per nonempty workload, not the full sweep.

For the ongoing [residual mutable-scaling investigation](https://github.com/dornglut/runen-ecs/issues/147),
select branch `main`, `size=1000000` and `runner=ubuntu-24.04-arm`.
The accepted public Linux ARM64 standard-runner specification offers four
vCPUs for public repositories; **only the recorded `lscpu` topology can
establish the observed number of physical cores**. Do not treat a four-vCPU
allocation as automatically four physical cores, or compare raw x64 vs ARM64
times as if their hosts were paired. Evaluate serial, 1/2/4/8-worker modes
**within the same immutable ARM64 run**. Capture the exact accepted-main
revision, `uname`, `lscpu`, Rust version, Criterion raw artifacts, and the
runner architecture embedded in the artifact name. If a four-physical-core
host is not established, the evidence gate remains open.

Both architectures retain the same Rust 1.98.1 toolchain and benchmark source,
and collect raw Criterion results with no absolute performance thresholds.
No optimizer, executor policy, or consumer integration change is authorized
solely by this runner-option addition.

## Interpreting evidence

Report serial and 1/2/4/8-worker per-invocation medians together with
per-run host/toolchain information and observed element-throughput values.
Compute speedup only for an equivalent workload within the same controlled
measurement environment. If comparing source changes, use repeated,
alternating exact-base/exact-head pairs on the same host before claiming a
regression or improvement.

A worker capacity of 8 is a *cap*, not an assurance of eight runnable tasks.
In the `conflicting_writes` case, every system accesses `C0` mutably; there
is no meaningful eight-way payload parallelism. Small-entity cases principally
measure scheduling overhead, while million-entity cases may expose memory
bandwidth and mutation-reservation contention. Neither mechanism is diagnosed
solely by its aggregate elapsed time.

This is a controlled **dense-archetype microbenchmark**, not a representative
gameplay-frame claim. Sparse/multiple-archetype distributions, optional and
change-filtered queries, deferred structural publication, `WorldMut` fences,
true integration scenes, memory/high-water allocation, and intra-system
parallel query iteration require separate focused evidence. No changes to
RunenECS execution or consumer schedule-selection policy are authorized here.

No benchmark measurements should be written into this document without
running the exact harness and documenting immutable measurement provenance.
