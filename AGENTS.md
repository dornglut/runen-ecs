# RunenECS agent instructions

RunenECS is a standalone Dornglut `rust-framework` repository.

## Start here

Before editing:

1. identify the semantic owner and invariant;
2. read `ARCHITECTURE.md` and `TESTING.md`;
3. inspect the owning source and public tests;
4. read the owning accepted issue and relevant accepted ADR when work is already accepted;
5. for cross-repository source-authority changes, follow Dornglut Engineering governance and ADR 0008.

Current `runen-ecs/main` is the sole reusable RunenECS semantic implementation authority. Runenwerk is a downstream integration consumer and no longer owns a writable predecessor implementation.

## Working rules

- Keep one coherent boundary per issue and pull request.
- Preserve package identities `runen-ecs` / `runen_ecs` and `runen-ecs-macros` / `runen_ecs_macros` unless an accepted public-contract change explicitly owns a rename.
- Do not import Runenwerk application, product, networking, rendering, replay, spatial, or editor policy into RunenECS.
- RunenECS must not depend on Runenwerk; downstream integration belongs in the consumer.
- Do not create compatibility aliases, forwarding crates/modules, source mirrors, `include!` ownership, submodules, or moving branch dependencies.
- Do not treat a read-only investigation or audit as authorization for implementation or public-surface expansion.
- Use focused checks while editing and `cargo validate` as the canonical merge-readiness baseline.
- Acceptance requires repository-owned exact-head CI on the unchanged reviewed head; any feature-head movement invalidates earlier exact-head evidence.
- Report only validation and behavior actually observed.

A future source-authority transfer must preserve exactly one writable semantic implementation authority and follow Engineering ADR 0008. The completed Runenwerk-to-RunenECS transfer is provenance, not an active migration path.
