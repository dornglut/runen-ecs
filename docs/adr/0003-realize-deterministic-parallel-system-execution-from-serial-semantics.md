# ADR 0003: Realize Deterministic Parallel System Execution from Serial Semantics

> **Category: ADR**
>
> **Status:** Accepted
>
> **Decision date:** 2026-09-11

## Context

RunenECS already owns the semantic facts needed to decide whether system work may overlap:

- explicit semantic precedence and deferred visibility;
- normalized access incompatibility;
- a deterministic serial reference executor;
- fail-stop deferred-command behavior across errors and unwinds;
- World-lineage-scoped change observation;
- ADR 0002 execution mobility (`Transferable` versus `InvokerThreadOnly`).

These facts are intentionally separate. Access incompatibility is not semantic ordering, execution mobility is not parallel eligibility, and physical execution groups are not public schedule meaning.

The present runtime is nevertheless serial-only. It invokes systems one at a time against `&mut World`, records successful `Commands` into one runtime-local queue, advances World change cursors directly from mutable access, and invokes deferred-boundary callbacks after current physical `ExecutionStage`s.

A parallel executor cannot safely be obtained by putting those calls on a thread pool. In particular:

- current direct mutation bookkeeping writes one shared World change cursor and shared change maps;
- current query capabilities contain pointers to shared bookkeeping domains;
- current ordinary `Commands` is invoker-thread-only by ADR 0002;
- current public boundary callbacks are coupled to physical topological stages and must first be normalized by #33;
- completion order must not become command publication order;
- panic/error handling cannot leave unpublished command buffers for a later invocation;
- generic rollback of arbitrary component/resource writes is not available.

Comparison evidence is useful but is not RunenECS authority. Bevy demonstrates access-driven multithreaded schedule execution and explicit deferred application, but documents unordered parallel execution as nondeterministic. Flecs demonstrates isolated per-thread staging queues merged at synchronization points. RunenECS requires its own stronger reproducibility contract: physical concurrency may vary in timing, but accepted ECS-observable successful results remain tied to the deterministic serial oracle rather than wall-clock completion order.

## Decision

### 1. Keep the serial executor as the semantic oracle

RunenECS retains a deterministic serial reference executor.

The parallel executor is a physical realization of the same accepted schedule. It does not define new semantic precedence, does not reinterpret access conflicts as ordering, and does not replace the serial path.

For the supported deterministic profile, a successful parallel schedule run must produce the same ECS-observable result as the serial reference run from the same initial World, schedule configuration, and deterministic ECS inputs, including:

- component/resource values;
- entity/component structural results after deferred publication;
- public change-observation results and final change cursor;
- removed-component observations at accepted publication frontiers;
- accepted deferred-command visibility.

This equivalence does not include timing, worker assignment, task completion order, physical cohort shape, or arbitrary process-global effects.

A workload is outside the deterministic profile when its ECS output depends on unsynchronized external timing/state such as wall-clock reads, unordered atomics, foreign mutable globals, nondeterministic I/O responses, or uncontrolled randomness. `Transferable` proves memory/thread mobility; it does not make such inputs deterministic.

### 2. Define a snapshot-local serial reference rank

Every valid built schedule has one deterministic serial reference sequence produced by the serial oracle.

Each system receives a snapshot-local **reference rank** according to that sequence. Reference rank exists only as an executor tie-break and deterministic publication key. It is not:

- semantic precedence;
- a stable runtime `SystemId`;
- portable identity;
- persistence/network identity;
- a public promise that one unordered system semantically occurs before another.

Two systems can remain semantically unordered while the executor uses their reference ranks to choose one reproducible physical outcome when their effects cannot commute.

The public inspection model from ADR 0001 must not expose reference rank as semantic order.

### 3. Use deterministic execution cohorts, not completion-driven scheduling

The first parallel executor executes deterministic **cohorts**. A cohort is an internal group of systems launched from one stable World/publication state and joined before the executor advances the commit frontier.

Cohort construction uses the serial reference sequence and is independent of wall-clock task completion.

The baseline cohort rule is deliberately conservative:

1. begin at the lowest-reference-rank system not yet completed;
2. that system must be semantically ready under accepted precedence/publication facts;
3. if it is `InvokerThreadOnly`, drain worker activity and execute it alone on the invoker thread;
4. otherwise build a contiguous reference-rank prefix of subsequent systems;
5. every additional member must already be semantically ready at cohort launch, be `Transferable`, be pairwise access-compatible with every selected member, and lie before the next required semantic publication frontier;
6. stop at the first system that does not satisfy those conditions.

A cohort therefore never crosses a semantic predecessor/successor dependency, an access incompatibility, an invoker-thread-only fence, or a required publication frontier.

This prefix rule is an internal correctness baseline, not public scheduling semantics. A future accepted design may broaden physical overlap only if it preserves every public invariant in this ADR, including deterministic change/publication behavior. It may not expose broader physical grouping as semantic order.

### 4. Access incompatibility is a physical exclusion, never a semantic edge

Systems in one cohort must be pairwise compatible according to normalized ECS access facts.

If two semantically unordered systems conflict, the parallel executor physically serializes them in serial-reference order. That physical choice preserves the serial oracle's reproducible outcome but does not add `A before B` to the semantic precedence graph.

Diagnostics continue to report such a pair as unordered/access-incompatible unless explicit semantic precedence exists.

Deferred structural recording is treated separately from immediate World access. Multiple task-local transferable deferred recorders may coexist; deterministic publication order resolves their queued effects. Direct structural mutation of the live World remains forbidden while a worker cohort is active.

### 5. Freeze World structure while a worker cohort is active

Worker execution receives only narrow invocation-scoped capabilities derived from validated access facts. It does not receive a transferable `&mut World`.

While any worker cohort is active:

- entity/archetype/resource-container structure required by active projections is frozen;
- no direct spawn/despawn/component-set migration/resource-container replacement may invalidate projected storage;
- direct component/resource payload mutation is allowed only through proven-disjoint transferable projections;
- `WorldMut` executes only as an invoker-thread-only fence;
- structural work is recorded for later publication through an accepted deferred capability.

The implementation must re-derive unsafe projection/thread-safety proofs for the parallel path. Existing serial raw-pointer capability code is evidence, not automatic permission to mark those capability types `Send`.

### 6. Mutable access uses task-local change journals

Parallel systems may directly mutate disjoint component/resource payloads, but they must not race on shared change bookkeeping.

Each worker invocation therefore owns a **mutation journal**. Mutable ECS access records the same logical change-observation events that the serial path would record, in that system's local execution order, while direct payload mutation remains possible against the proven-disjoint payload projection.

A journal event is the serial path's conservative mutation-observation event; it is not proof that user code changed payload bytes or produced a value inequality. For the current built-in semantics, a mutable component-query event is recorded before the mutable item is yielded/fetched, and a mutable resource event is recorded when `ResMut` mutable dereference exposes `&mut T`. Tuple and derived forms preserve the corresponding child-event trigger points and mutable domains. The parallel implementation must reproduce those accepted trigger points rather than invent exact-write detection, equality comparison, drop-time dirty inference, or another mutation semantic.

Shared World bookkeeping is not mutated by workers for those events. In particular, workers do not concurrently update:

- the World `ChangeCursor`;
- component/resource high-water change maps;
- archetype added/changed metadata;
- removed-component records;
- component-index dirty bookkeeping.

After every successful cohort joins, the invoker commits mutation journals in reference-rank order and each journal's local event order. Cursor positions are assigned during this canonical commit. Wall-clock completion order therefore cannot change public change positions.

The implementation must detect/reserve absolute cursor capacity before permitting an event to alias an existing change position. Absolute cursor exhaustion remains an unrecoverable panic boundary; no implementation may wrap or silently reuse a position. Reservation mechanics are implementation details and do not define ordinary change ordering.

### 7. Change-filter observation uses one committed cohort snapshot

Every system in a cohort begins from the same committed change-observation snapshot.

A query state's prior observation cursor is compared only against change metadata committed before cohort launch. This is sound because cohort members are access-compatible: no peer may concurrently mutate an ECS domain another peer observes in a way that would require intra-cohort visibility.

At successful cohort completion, query/system observation state advances consistently with that committed snapshot and the system's own accepted semantics. Peer completion timing must not influence `Added`/`Changed` results.

The implementation may use task-local logical positions internally, but they are not public `ChangeCursor` values until canonical commit.

### 8. Add a distinct transferable deferred-command capability

Current ordinary `Commands` remains `InvokerThreadOnly` under ADR 0002. Its existing ability to queue arbitrary non-`Send` captured state is preserved; it is not silently tightened merely to enable workers.

RunenECS introduces a distinct transferable deferred-command system parameter, semantically named `TransferableCommands`.

`TransferableCommands` has the same ECS ownership purpose—record structural/deferred World effects for later publication—but its recorded erased effects must themselves be movable back to the invoker/publication executor. At minimum, arbitrary queued closures require `Send + 'static` in addition to the existing callable/result contract. Convenience operations such as deferred spawn/insert derive whatever `Send` bounds their captured payload actually requires.

The recorder itself is invocation-local and transfer-safe. It does not directly mutate World structure from a worker.

`TransferableCommands` is a new capability, not a compatibility alias for `Commands`, and is eligible for the ADR 0002 transferable `SystemParam` proof when its implementation satisfies that proof.

### 9. Deferred buffers are task-local and publication-ordered

Every system invocation records deferred effects into its own buffer. Successful task buffers remain unpublished until the accepted semantic publication frontier. A failed/panicking task's own deferred buffer is abandoned.

When a frontier is reached, pending successful buffers are merged/applied in serial reference-rank order, preserving each system buffer's internal command order. Completion order, worker ID, queue address, and steal order never determine publication.

Deferred command application itself remains ordered fail-stop work, not a transaction. If applying a command returns an error or panics:

- commands already applied earlier in canonical publication order remain committed;
- the remainder of the failing buffer and every later still-unpublished buffer are abandoned;
- no later schedule work launches;
- a user panic resumes unwinding rather than being converted to an ordinary system error;
- an ordinary command error is returned through the accepted runtime error path.

### 10. Publication frontiers come from semantic visibility, not cohorts

A worker cohort boundary is not automatically a deferred-apply boundary.

Deferred publication is driven by the normalized semantic publication-frontier model from ADR 0001 / #33:

- a successor that semantically depends on a predecessor must observe the predecessor's accepted deferred work when the ordering contract requires it;
- final pending deferred work is published before the schedule run completes;
- unrelated physical grouping does not create portable visibility semantics.

The executor must not expose current serial `ExecutionStage` or future worker cohorts as the source of this contract.

Cohort construction may conservatively stop at a publication frontier, but changing worker count or cohort width must not change where semantic deferred visibility occurs.

### 11. Boundary callbacks execute only at semantic publication frontiers

The public schedule boundary callback normalized by #33 runs only after the corresponding semantic publication has committed and while no worker cohort is active.

The callback executes on the schedule-invoking thread with exclusive accepted World access. A worker cohort or task completion does not create an extra callback.

If the callback returns an error or panics:

- no later schedule work launches;
- earlier direct mutation commits and earlier published frontiers remain committed;
- still-unpublished deferred work is abandoned;
- no generic World rollback occurs;
- a panic resumes unwinding with the original payload rather than being converted to an ordinary error.

Any exposed boundary descriptor/index is the schedule-local semantic publication identity from #33, never a cohort/stage index.

### 12. Invoker-thread-only systems are physical fences in the baseline executor

An `InvokerThreadOnly` system is executed on the thread that invoked the schedule.

The baseline executor drains the active worker cohort before invoking such a system and does not overlap worker execution across it. This is intentionally conservative and keeps thread-bound effects, whole-World access, and ordinary `Commands` easy to reason about.

Invoker-thread affinity does not itself create semantic ordering. A future executor may overlap compatible worker work around a thread-bound system only under a separately accepted proof that all public semantics in this ADR remain unchanged.

### 13. User system errors and panics use deterministic cohort fail-stop handling

Parallel execution cannot promise generic transactional rollback of direct component/resource writes. Components/resources are not required to be `Clone`, user code may mutate them arbitrarily, and whole-World transactions are outside this design.

Therefore successful-run serial equivalence and failed-run guarantees are distinct.

When launched cohort members produce ordinary system `Err` results or user panics while framework invariants remain intact:

1. launch no later cohort;
2. do not attempt asynchronous cancellation of already-running user code;
3. join/drain every already-launched member of the cohort;
4. preserve task-local mutation journals even from a panicking task so accepted mutation-observation events already recorded before failure can be reconciled;
5. commit those mutation journals in reference-rank/local-event order so public change bookkeeping reflects every recorded event canonically;
6. discard every still-unpublished deferred-command buffer in the failed semantic publication interval, including buffers from otherwise successful peers, preserving #15 fail-stop command isolation;
7. keep effects already published at an earlier completed semantic frontier committed;
8. choose the primary **user/system failure** by the lowest reference rank among cohort members that returned an ordinary system `Err` or produced a user panic;
9. if that selected user/system failure is a panic, resume unwinding with its original panic payload on the invoker thread rather than converting it to `RuntimeError`; otherwise return that system error.

This reference-rank rule selects among ordinary user/system failures only. A framework/invariant failure detected in any launched task or canonical executor phase is governed by section 14 and cannot be hidden by a lower-reference-rank ordinary system error or user panic.

Direct writes from higher-rank cohort peers may therefore remain visible even when a lower-rank peer fails, because those peers had already run concurrently. That partial failure state is explicitly not required to equal the serial executor's partial failure state.

What remains guaranteed on ordinary user/system failure is memory safety, no later launches, deterministic primary-user-failure selection, truthful canonical change bookkeeping for every accepted mutation-observation event already recorded before failure, no abandoned unpublished commands leaking into later invocations, and preservation of earlier completed publication frontiers. Direct payload writes remain non-transactional, but RunenECS does not claim to detect or enumerate the exact subset of byte writes independently of those accepted observation events.

Arbitrary external side effects remain outside RunenECS rollback guarantees.

### 14. Framework/invariant failures dominate user recovery semantics

Internal capability violations, impossible plan references, change-cursor exhaustion, executor bookkeeping corruption, and equivalent framework invariant failures remain framework panics. They are not ordinary system outcomes and are not participants in section 13's user/system primary-failure ranking.

If any launched task or canonical executor phase detects a framework invariant failure:

- launch no later work;
- satisfy every join/drain or ownership step still required for memory safety when the executor state remains trustworthy enough to do so;
- do not attempt semantic recovery/commit work whose safety proof depends on the violated invariant itself;
- propagate a framework invariant panic after required safe cleanup rather than returning an ordinary `RuntimeError` or resuming a user panic in its place.

A framework invariant failure therefore dominates any simultaneously captured ordinary system `Err` or user panic. This preserves the distinction between caller/user failure and a framework state that must not be presented as recoverable merely because a lower-ranked user system also failed.

When multiple **rank-associated framework invariant panics** are safely captured from launched systems, the deterministic tie-break among those invariant failures is the lowest serial reference rank. The selected invariant panic's original payload/diagnostic is resumed where available. User/system failures are not compared against invariant failures for this tie-break.

An invoker-side invariant encountered at a canonical plan, reconciliation, or publication step has no fictional system rank and propagates at that canonical step after any memory-safety-required drain. The implementation must not manufacture a system identity merely to fit the rank-selection rule.

Cursor-capacity exhaustion is a reachable invariant with an additional journal guarantee: capacity must be secured before a mutation-observation event is admitted, so every already-admitted event remains safely reconcilable before the exhaustion panic propagates. The event that cannot reserve capacity is not admitted and must not expose the corresponding mutable access. Physical reservation order remains non-semantic; final public `ChangeCursor` order is still assigned by canonical reference-rank/local-event commit.

No cursor wrap/reuse is permitted. Runtime-reuse guarantees that apply to ordinary caught system errors or user panics do not make a framework invariant failure a supported recoverable state.

### 15. Executor backend is RunenECS-local and replaceable

The physical worker mechanism is an implementation detail owned by RunenECS. It may use scoped threads, a runtime-owned pool, work stealing, or another internal backend provided the accepted semantics remain unchanged.

This ADR does not create a generic Dornglut task/job scheduler package.

Public API does not expose:

- worker IDs;
- pool topology;
- steal order;
- cohort indices as semantic order;
- task handles that permit bypassing schedule ownership;
- physical stage/wave/batch identity.

Runtime configuration may control whether the serial or parallel executor is selected and may control physical capacity. Those settings must not change successful ECS-observable results in the deterministic profile.

### 16. Diagnostics explain eligibility without promoting execution layout

Schedule inspection may explain why two systems cannot overlap using accepted facts:

- semantic precedence;
- access incompatibility;
- `InvokerThreadOnly` capability;
- required deferred-publication dependency.

It may also state that a pair is not prevented by known schedule/capability facts. It must not promise that the pair actually ran concurrently.

Physical cohort membership and reference rank remain executor diagnostics at most, not normalized schedule semantics and not stable public identity.

### 17. External side effects are outside the deterministic ECS profile

`Transferable` proves thread mobility, not purity.

A transferable system may perform external I/O, mutate unrelated atomics, log, call foreign code, or otherwise produce effects RunenECS does not own. Those effects are not serialized or rolled back unless represented through an accepted ECS-owned capability.

If such external inputs/effects influence later ECS writes, the workload is not in the deterministic serial-versus-parallel profile unless the caller supplies its own deterministic synchronization/order.

Callers that require a specific order for external effects must express an ECS semantic dependency or keep that effect in an invoker-thread/otherwise explicitly serialized system.

### 18. Conformance is executor-permutation based

Parallel-executor acceptance requires one shared conformance corpus executed against both serial and parallel paths.

For successful workloads in the supported deterministic profile, tests must compare the complete relevant ECS-observable result, including public change observation and deferred publication.

The parallel path must additionally be exercised with controlled completion perturbations (delays/yields/permuted worker completion) proving that timing does not change accepted results.

At minimum conformance covers:

- independent read/read and disjoint write systems overlap-capable;
- conflicting unordered systems remain physically serialized without gaining semantic precedence;
- explicit predecessor/successor visibility;
- deterministic merge of multiple `TransferableCommands` buffers;
- ordinary `Commands` / `WorldMut` systems executing on the invoking thread;
- ordinary user panic/error cohort draining, deterministic lowest-reference-rank user-failure selection, and command abandonment;
- a higher-reference-rank framework invariant occurring alongside a lower-reference-rank ordinary `Err` or user panic still propagating as the framework invariant rather than being hidden by user-failure ranking;
- deterministic lowest-reference-rank selection among multiple safely captured rank-associated framework invariant panics;
- deferred-command application error/panic fail-stop behavior;
- semantic boundary callback error/panic behavior;
- earlier completed publication frontiers surviving later ordinary user/system failure;
- task-local mutation journals producing deterministic change cursors/metadata under the accepted conservative mutable-access observation semantics;
- cursor-capacity exhaustion preserving the already-admitted journal prefix without wrap/reuse or conversion to a recoverable error;
- `Added`/`Changed` behavior across parallel cohorts;
- structural freeze and exclusive `WorldMut` behavior;
- randomized completion order not changing successful ECS results.

Miri/AddressSanitizer remain required where applicable, and a race detector such as ThreadSanitizer or an equivalent maintained race-evidence path is required before accepting unsafe concurrent World projection code.

### 19. Performance evidence cannot authorize correctness shortcuts

Issue #17 must repair semantic benchmark fixtures before benchmark numbers are used as optimization gates or performance claims for parallel execution.

Benchmark evidence may choose between semantically equivalent executor implementations. It may not weaken:

- deterministic publication;
- change-observation correctness;
- access safety;
- invoker-thread guarantees;
- panic/error isolation;
- serial-oracle conformance.

The first implementation should prefer a smaller provable executor over a broader speculative scheduler. More aggressive overlap can be added later only with new evidence and without changing the normalized semantic model.

## Consequences

RunenECS gains a deterministic target for parallel system execution without turning physical concurrency into schedule semantics.

Successful worker execution remains reproducible because the serial reference sequence supplies deterministic tie-breaking, mutation-observation bookkeeping is committed canonically, and deferred effects publish in reference order at semantic frontiers rather than completion order.

The design deliberately requires more than a thread pool. Safe implementation needs normalized schedule/publication reasoning (#26), proof-preserving system mobility (#30), narrow concurrent World projections, task-local mutation journals, a distinct `TransferableCommands` capability, and deterministic deferred buffers.

Failure semantics are fail-stop rather than transactional. Already-running peers can leave direct writes, but unpublished commands cannot leak and change bookkeeping must truthfully reproduce the accepted mutation-observation events that were recorded. RunenECS does not claim exact byte-write detection. Ordinary user/system failures use deterministic reference-rank selection; framework invariant failures remain a distinct higher-priority panic class and cannot be masked as recoverable system outcomes. This is the minimum honest contract without imposing generic World transactions.

This ADR does not authorize parallel query iteration, generic task scheduling, order-only deferred semantics, application lifecycle barriers, or arbitrary external-side-effect determinism.

## Implementation gates

The executor design is accepted independently of implementation readiness. Parallel-executor implementation must not be accepted until:

- #26 is complete, including #27, #33, and #28;
- #30 is complete, including #31, #35, and #32;
- #17 has repaired benchmark fixtures before any performance claim is used;
- the unsafe concurrent World projection/change-journal design has focused Miri/sanitizer/race evidence;
- exact-current Runenwerk lifecycle behavior passes against the candidate;
- canonical `cargo validate`, exact-head CI, and `git diff --check` pass.