# RunenECS agent instructions

## Authority order

For work in this repository, resolve authority in this order:

1. current owning GitHub issue and accepted Engineering initiative/ADR constraints;
2. `ARCHITECTURE.md` and repository-local policy files;
3. accepted RunenECS source/public tests at the current semantic source authority;
4. current code and tests in this repository after successor acceptance;
5. repository-owned validation evidence.

During the pre-transfer bootstrap, Runenwerk remains the sole semantic RunenECS source authority. Do not treat bootstrap files or an unmerged transfer candidate as accepted framework semantics.

## Working rules

- Identify the semantic owner and invariant before editing.
- Keep one coherent boundary per issue/PR.
- Preserve final package identities `runen-ecs` / `runen_ecs` and `runen-ecs-macros` / `runen_ecs_macros`.
- Do not import Runenwerk application/product/network/render/replay policy into RunenECS.
- Do not create compatibility aliases, forwarding crates/modules, source mirrors, `include!` ownership, submodules, or moving branch dependencies.
- Physical repository adaptation must not widen or redesign the accepted public contract.
- Run the repository-owned canonical validation before merge and report only checks actually observed.
- Any feature-head change invalidates earlier exact-head validation evidence.
- Source-authority switching follows Engineering ADR 0008 and the active RunenECS initiative.

## Current sequencing

- Issue #3 owns repository-profile bootstrap only.
- Issue #4 owns the later source-transfer candidate after #3 is accepted.
- Issue #2 owns successor acceptance and the ADR-0008 authority switch.
- Post-extraction audit #1 remains blocked until successor acceptance and Runenwerk predecessor deletion are both complete.
