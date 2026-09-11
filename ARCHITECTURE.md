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

## Schedule semantic layers

RunenECS keeps semantic precedence, access incompatibility, deferred visibility, and physical executor grouping distinct. Explicit ordering creates precedence; access conflicts never invent order; deferred visibility is an observable consequence that must not be represented by exposing physical stages or waves.

Ordering-reference presence and the normalized diagnostic model are owned by [ADR 0001: Normalize Schedule Diagnostics and Ordering References](docs/adr/0001-normalize-schedule-diagnostics-and-ordering-references.md).

## Dependency direction

RunenECS does not depend on Runenwerk. Runenwerk may consume an exact immutable accepted RunenECS revision. Reusable networking and spatial semantics remain owned by RunenNet and RunenSpatial rather than being duplicated here.

## Source-authority invariant

Exactly one writable semantic RunenECS implementation authority exists at a time. The current authority is this standalone repository. Any future transfer must follow Engineering ADR 0008, establish a separately accepted successor, migrate real consumers, and retire the predecessor without leaving a mirror, forwarding path, moving dependency, or second runtime implementation.
