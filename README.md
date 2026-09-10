# RunenECS

RunenECS is Dornglut's standalone reusable entity-component-system framework.

## Current state

The source transfer from the accepted Runenwerk C9 boundary is governed by
[issue #2](https://github.com/dornglut/runen-ecs/issues/2) and Engineering ADR
0008. An unmerged candidate branch is staging only; the accepted revision on
`runen-ecs/main` is the standalone semantic authority.

## Package topology

The standalone physical layout is:

```text
crates/runen-ecs
crates/runen-ecs-macros
```

The runtime and proc-macro packages retain the accepted C9 public identities.

## Boundary

RunenECS owns reusable ECS entity/component/resource/world lifecycle, storage/query semantics, deferred structural mutation, explicit reflection, ECS system identity/access/order/set/schedule validation, and deterministic serial reference execution.

Runenwerk retains application/frame/fixed/render/startup/shutdown policy and product integration. RunenNet and RunenSpatial retain their own reusable networking and spatial semantics.

## Validation

Run the repository-owned canonical baseline with:

```bash
cargo validate
```

See `TESTING.md` for the validation contract and transfer-specific additional gates.

## Provenance

The accepted transfer input is Runenwerk revision
`b7e3d55c76be0acebf3ce260e7ee282ea1787e29`. The source was physically
transferred; Git history remains in the predecessor repository.
