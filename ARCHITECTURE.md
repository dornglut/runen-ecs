# RunenECS architecture

## Current authority state

Accepted `runen-ecs/main` is the sole reusable RunenECS semantic implementation authority. Runenwerk consumes an exact accepted standalone revision and no longer contains a writable predecessor implementation.

Cross-repository source-authority transfers follow Engineering ADR 0008. The completed Runenwerk-to-RunenECS handoff is historical provenance; the current architecture has one standalone implementation authority and no transfer overlap.

## Standalone package topology

The standalone workspace topology is:

```text
crates/runen-ecs
crates/runen-ecs-macros
xtask
```

The runtime and proc-macro packages keep the accepted public identities. The standalone topology replaces predecessor filesystem placement; it is not a compatibility layer.

## Semantic ownership

RunenECS owns reusable ECS semantics:

- entity, component, resource, and world lifecycle;
- storage and query behavior;
- deferred structural mutation;
- explicit reflection;
- ECS system identity and access facts;
- explicit ordering, sets, schedule validation, and deferred-command boundaries;
- deterministic serial reference execution.

RunenECS does not own application/frame/render lifecycle, product publication policy, networking protocols, spatial indexing semantics, editor behavior, or Runenwerk integration adapters.

## Dense storage and contiguous query capability

RunenECS distinguishes logical archetype membership and row alignment from physical contiguous storage segments, ordinary query order, vectorized processing, and future parallel partitioning. Dense component payloads may relocate between structural epochs; safe references and expert contiguous spans are valid only for the active World borrow or runtime structural-freeze scope.

The normalized dense-storage direction, row-migration invariants, reflection boundary, and sealed fallible contiguous-segment query capability are owned by [ADR 0004: Normalize Dense Storage Contiguity and Expert Query Segments](docs/adr/0004-normalize-dense-storage-contiguity-and-expert-query-segments.md).

## Typed relation domain

RunenECS owns typed ECS relations as a first-class semantic domain:

```text
(source Entity, relation type, target Entity)
```

`World` remains the sole entity-liveness and lifecycle authority. Each relation type has one private authoritative edge store keyed by its Rust `TypeId`; directed inverse access and symmetric neighbor access are observations over that same authority rather than separately writable collections. Successful entity despawn removes the entity from every registered relation store before its identity can be reused.

The public boundary is `Relations<R>` / `RelationsMut<R>`. Relation edges are not ordinary components, are not mirrored into archetype storage, and do not introduce a second public node or edge identity. `Relation::name()` is diagnostic text only.

The exact accepted RunenGraph R0 revision is private structural machinery beneath this ECS-owned domain. RunenGraph graph types, membership state, mutation outcomes, and errors are not RunenECS public semantics.

The same `Relations<R>` / `RelationsMut<R>` capability types also participate in ordinary system extraction. Scheduler access is keyed by relation type: shared access may overlap shared access to the same relation; any same-type writer conflicts; distinct relation types are independent from one another and from component/resource domains. These access conflicts constrain physical overlap only and do not create semantic precedence.

Transferable relation parameters are prepared through narrow relation-store projections plus a frozen entity-validation snapshot under the parallel structural lease. Workers never receive a whole-World relation handle or a second writable graph. Relation stores are boxed privately so prepared store addresses remain stable even if the relation registry grows while a cohort is prepared. Entity allocation/liveness remains World-owned; the worker snapshot carries only the validation facts required to preserve the direct-World `EntityError` classification for the frozen invocation.

Stronger relation policies such as cardinality, hierarchy, ordering, payloads, change observation, traversal, reflection, serialization, and relation-aware entity query/filter semantics remain separately accepted future capabilities built around the same typed relation views.

## Schedule semantic layers

RunenECS keeps semantic precedence, access incompatibility, deferred visibility, and physical executor grouping distinct. Explicit ordering creates precedence; access conflicts never invent order; deferred visibility is an observable consequence that must not be represented by exposing physical stages or waves.

Ordering-reference presence and the normalized diagnostic model are owned by [ADR 0001: Normalize Schedule Diagnostics and Ordering References](docs/adr/0001-normalize-schedule-diagnostics-and-ordering-references.md).

## System execution mobility

RunenECS keeps execution mobility separate from semantic scheduling. Components and resources remain `'static` rather than globally `Send + Sync`; transfer safety is proven from the exact callable and parameter access facts. Normal system registration is the proven-transferable path, while thread-bound behavior is represented explicitly as invoker-thread-only without introducing an application-level "main thread" concept.

The normalized capability and proof boundary are owned by [ADR 0002: Model System Execution Mobility as a Proven Capability](docs/adr/0002-model-system-execution-mobility-as-a-proven-capability.md).

## Deterministic parallel execution

RunenECS treats parallel system execution as a physical realization of the deterministic serial reference semantics. Access conflicts may prevent overlap but never become semantic precedence; worker completion order never determines change positions or deferred publication; invoker-thread-only systems remain explicit physical fences in the baseline executor.

The executor, mutation-journal, deferred-publication, and fail-stop rules are owned by [ADR 0003: Realize Deterministic Parallel System Execution from Serial Semantics](docs/adr/0003-realize-deterministic-parallel-system-execution-from-serial-semantics.md).

`Runtime::run_schedule_parallel` and its deferred-publication-frontier callback
form are supported execution selectors. They keep worker capacity, cohort shape,
completion order, reservation order, and worker identity private. The serial
`Runtime::run_schedule` path remains the independent correctness oracle; callers
choose whether and when to use parallel execution as product policy.

## Dependency direction

RunenECS does not depend on Runenwerk. Runenwerk may consume an exact immutable accepted RunenECS revision. Reusable networking and spatial semantics remain owned by RunenNet and RunenSpatial rather than being duplicated here.

## Source-authority invariant

Exactly one writable semantic RunenECS implementation authority exists at a time. The current authority is this standalone repository. Any future transfer must follow Engineering ADR 0008, establish a separately accepted successor, migrate real consumers, and retire the predecessor without leaving a mirror, forwarding path, moving dependency, or second runtime implementation.
