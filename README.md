# RunenECS

RunenECS is the standalone repository reserved for Dornglut's reusable entity-component-system framework.

## Current state

This repository is in **pre-transfer bootstrap**. Runenwerk remains the sole semantic RunenECS source authority until the successor implementation owned by [issue #2](https://github.com/dornglut/runen-ecs/issues/2) is validated and accepted on `runen-ecs/main` under Engineering ADR 0008.

The accepted bootstrap contains repository policy, validation, licensing, and tooling only. It does not contain RunenECS runtime or proc-macro implementation source and does not switch semantic authority.

## Target package topology

The accepted standalone physical layout for the source-transfer candidate is:

```text
crates/runen-ecs
crates/runen-ecs-macros
```

These paths are repository organization only. They do not redefine the accepted C9 semantic boundary or public package identities.

## Boundary

RunenECS owns reusable ECS entity/component/resource/world lifecycle, storage/query semantics, deferred structural mutation, explicit reflection, ECS system identity/access/order/set/schedule validation, and deterministic serial reference execution.

Runenwerk retains application/frame/fixed/render/startup/shutdown policy and product integration. RunenNet and RunenSpatial retain their own reusable networking and spatial semantics.

## Validation

Run the repository-owned canonical baseline with:

```bash
cargo validate
```

See `TESTING.md` for the validation contract and transfer-specific additional gates.
