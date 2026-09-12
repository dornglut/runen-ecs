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

## Schedule semantic layers

RunenECS keeps semantic precedence, access incompatibility, deferred visibility, and physical executor grouping distinct. Explicit ordering creates precedence; access conflicts never invent order; deferred visibility is an observable consequence that must not be represented by exposing physical stages or waves.

Ordering-reference presence and the normalized diagnostic model are owned by [ADR 0001: Normalize Schedule Diagnostics and Ordering References](docs/adr/0001-normalize-schedule-diagnostics-and-ordering-references.md).

## System execution mobility

RunenECS keeps execution mobility separate from semantic scheduling. Components and resources remain `'static` rather than globally `Send + Sync`; transfer safety is proven from the exact callable and parameter access facts. Normal system registration is the proven-transferable path, while thread-bound behavior is represented explicitly as invoker-thread-only without introducing an application-level "main thread" concept.

The normalized capability and proof boundary are owned by [ADR 0002: Model System Execution Mobility as a Proven Capability](docs/adr/0002-model-system-execution-mobility-as-a-proven-capability.md).

## Deterministic parallel execution

RunenECS treats parallel system execution as a physical realization of the deterministic serial reference semantics. Access conflicts may prevent overlap but never become semantic precedence; worker completion order never determines change positions or deferred publication; invoker-thread-only systems remain explicit physical fences in the baseline executor.

The executor, mutation-journal, deferred-publication, and fail-stop rules are owned by [ADR 0003: Realize Deterministic Parallel System Execution from Serial Semantics](docs/adr/0003-realize-deterministic-parallel-system-execution-from-serial-semantics.md).

## Dependency direction

RunenECS does not depend on Runenwerk. Runenwerk may consume an exact immutable accepted RunenECS revision. Reusable networking and spatial semantics remain owned by RunenNet and RunenSpatial rather than being duplicated here.

## Source-authority invariant

Exactly one writable semantic RunenECS implementation authority exists at a time. The current authority is this standalone repository. Any future transfer must follow Engineering ADR 0008, establish a separately accepted successor, migrate real consumers, and retire the predecessor without leaving a mirror, forwarding path, moving dependency, or second runtime implementation.
