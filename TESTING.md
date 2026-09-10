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

The CI workflow is a thin read-only caller of the accepted organization reusable workflow and validates the exact feature head or accepted-main revision.

## Transfer-specific gates

Issue #2 owns the additional successor-acceptance gates, including all-features
package validation, public/conformance tests, deterministic
scheduling/deferred-command behavior, focused Miri and AddressSanitizer
evidence, examples/benchmarks, package-identity checks, and
no-mirror/no-forwarder/no-moving-dependency residue checks.

Focused gates supplement the canonical baseline; they do not replace it.
