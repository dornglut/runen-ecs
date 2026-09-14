# Testing and validation

## Canonical repository baseline

Run:

```bash
cargo validate
```

The command is repository-owned through `.cargo/config.toml` and `xtask`. It validates required repository files, starts from a clean worktree, then runs:

- `cargo fmt --all -- --check`;
- `cargo check --workspace --locked`;
- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`;
- `cargo doc --workspace --no-deps --locked` with rustdoc warnings denied;
- `git diff --check` and `git diff --cached --check`;
- a final worktree-state equality check.

The `RunenECS Validation` workflow is a thin read-only caller of the accepted organization reusable workflow and validates the exact pull-request feature head or accepted-main revision.

## Supplemental safety proofs

The repository also maintains focused exact-head Miri, AddressSanitizer, and ThreadSanitizer workflows. They are path-scoped to changes affecting RunenECS packages, conformance/safety tooling, relevant manifests/toolchain files, or their own workflow definitions. Root-documentation-only changes do not automatically trigger these supplemental proofs.

These safety workflows supplement `cargo validate`; they do not replace the canonical baseline. Their focused scope remains owned by the corresponding checked-in workflow and safety harness.

The additional successor-acceptance gates recorded by completed issue #2 are transfer provenance. They do not remain a second ongoing merge-readiness baseline after the accepted source-authority handoff.

## ThreadSanitizer race evidence

The maintained ThreadSanitizer lane is the exact-head workflow
`.github/workflows/ecs-tsan.yml`. Its focused command is also available from a
checked-out repository with:

```text
bash tools/tsan/run_ecs_tsan.sh
```

The script pins `nightly-2026-08-25`, targets `x86_64-unknown-linux-gnu`,
installs/uses `rust-src` through the workflow, and runs the focused
`tsan_smoke` target with `RUSTFLAGS=-Zsanitizer=thread` and `-Zbuild-std`.
The workflow verifies the exact pull-request head or accepted-main revision
before running the proof. The founding smoke only checks race-free standard
library threading with synchronization and disjoint data; it does not invent
concurrent ECS execution. #38 and later executor slices must extend this same
lane with their actual unsafe-boundary and production-path coverage.

ThreadSanitizer is race-detection evidence, not exhaustive interleaving
exploration or a replacement for Rust's type/access proof, Miri, AddressSanitizer,
semantic conformance, or canonical validation. It must observe synchronization
through instrumented/intercepted operations; Rust's current support does not
cover `std::sync::atomic::fence` or synchronization implemented through inline
assembly. Any later implementation relying materially on such primitives needs
an explicit complementary proof and an issue/PR record of the limitation while
retaining the TSan lane for the remaining coverage.
