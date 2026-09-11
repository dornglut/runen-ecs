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

The repository also maintains focused exact-head Miri and AddressSanitizer workflows. They are path-scoped to changes affecting RunenECS packages, conformance/safety tooling, relevant manifests/lockfile, or their own workflow definitions. Root-documentation-only changes do not automatically trigger these supplemental proofs.

These safety workflows supplement `cargo validate`; they do not replace the canonical baseline. Their focused scope remains owned by the corresponding checked-in workflow and safety harness.

The additional successor-acceptance gates recorded by completed issue #2 are transfer provenance. They do not remain a second ongoing merge-readiness baseline after the accepted source-authority handoff.
