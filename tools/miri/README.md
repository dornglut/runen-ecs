# RunenECS focused Miri validation

The RunenECS package owns the retained unsafe query, capability, and extraction
proofs. Run the permanent focused path from the repository root with:

```text
bash tools/miri/run_ecs_c3.sh
```

The script requires the pinned `nightly-2026-08-25` toolchain with its Miri
component and runs `crates/runen-ecs/tests/miri_c3.rs`. The same script is invoked by
`.github/workflows/ecs-miri.yml`; stable `cargo validate` remains independent
of this nightly safety check.

The focused test covers retained mutable query items, mutable tuples, disjoint
query/resource extraction, stable resource payloads during mutation tracking,
and queries after structural migration.
