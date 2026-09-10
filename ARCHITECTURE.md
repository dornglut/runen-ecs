# RunenECS architecture

## Current authority state

This repository is currently a pre-transfer standalone shell. Its accepted bootstrap may own repository policy/tooling, but it does **not** yet own RunenECS implementation semantics. Until the successor source-transfer candidate is accepted on `runen-ecs/main`, corrected accepted Runenwerk C9 remains the sole semantic source authority.

## Standalone package topology

The intended standalone workspace topology is:

```text
crates/runen-ecs
crates/runen-ecs-macros
xtask
```

The runtime and proc-macro packages keep the already accepted public identities. The standalone topology replaces predecessor filesystem placement; it is not a compatibility layer.

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

## Dependency direction

The standalone framework must not depend on Runenwerk. Runenwerk may consume an exact immutable accepted RunenECS revision after the ADR-0008 authority switch. Reusable networking and spatial semantics remain owned by RunenNet and RunenSpatial rather than being duplicated here.

## Handoff invariant

Exactly one writable semantic RunenECS implementation authority exists at a time. An unmerged successor candidate is staging only. Acceptance onto `runen-ecs/main` switches authority; the Runenwerk predecessor then freezes until downstream exact-pin migration deletes it.
