# Repository bootstrap

This file records the standalone repository-profile bootstrap performed before RunenECS implementation transfer.

## Purpose

The repository began as an inert namespace shell containing only `LICENSE`. Bootstrap establishes a truthful Rust-framework repository surface before implementation source is introduced, so the later transfer can be reviewed as the actual source-authority candidate rather than mixing repository setup with semantic movement.

## Bootstrap contents

The bootstrap owns:

- Cargo workspace and repository-local `xtask`;
- canonical `cargo validate` command;
- thin read-only exact-head CI caller;
- repository architecture/agent/testing/licensing documentation;
- stable toolchain declaration and bootstrap lockfile.

It intentionally owns no RunenECS runtime or proc-macro implementation source.

## Transfer sequence

1. Accept repository-profile bootstrap under issue #3.
2. Re-resolve the corrected accepted Runenwerk C9 input under issue #2.
3. Record the file-by-file `MOVE/ADAPT`, `STAY`, and `DELETE/DO NOT TRANSFER` census.
4. Build the source-transfer candidate under issue #4 in `crates/runen-ecs` and `crates/runen-ecs-macros`.
5. Run exact-head repository, all-features, conformance, Miri, AddressSanitizer, example, benchmark, and residue checks required by issue #2.
6. Accept the successor candidate only when those gates pass. That merge is the ADR-0008 semantic authority switch.

This bootstrap is provenance for repository setup, not a second semantic source authority.
