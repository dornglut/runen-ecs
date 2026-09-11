# RunenECS

RunenECS is Dornglut's standalone reusable entity-component-system framework.

## Current state

Accepted `runen-ecs/main` is the standalone RunenECS semantic implementation authority. Runenwerk consumes an exact accepted RunenECS revision and no longer owns a duplicate predecessor implementation. Cross-repository source-authority transfers follow Dornglut Engineering ADR 0008; the Runenwerk-to-RunenECS transfer is complete.

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

See `ARCHITECTURE.md` for the repository boundary and dependency direction.

## Validation

Run the repository-owned canonical baseline with:

```bash
cargo validate
```

See `TESTING.md` for the canonical baseline, CI relationship, and maintained supplemental safety proofs.

## Provenance

The accepted transfer input is Runenwerk revision
`b7e3d55c76be0acebf3ce260e7ee282ea1787e29`. The source was physically
transferred; Git history remains in the predecessor repository.
