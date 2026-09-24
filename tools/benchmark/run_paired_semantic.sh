#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run_paired_semantic.sh BASELINE_SHA CANDIDATE_SHA [REPETITIONS] [OUTPUT_DIR]

Runs the RunenECS semantic Criterion benchmark for two exact commit SHAs on one
host, alternating order across repetitions. Rust 1.98.1 must be installed.
EOF
}

if [[ $# -lt 2 || $# -gt 4 ]]; then
  usage >&2
  exit 2
fi

baseline_sha="$1"
candidate_sha="$2"
repetitions="${3:-4}"
output_dir="${4:-paired-semantic-evidence}"

sha_pattern='^[0-9a-f]{40}$'
if [[ ! "$baseline_sha" =~ $sha_pattern ]]; then
  echo "baseline revision must be an exact 40-hex commit SHA: $baseline_sha" >&2
  exit 2
fi
if [[ ! "$candidate_sha" =~ $sha_pattern ]]; then
  echo "candidate revision must be an exact 40-hex commit SHA: $candidate_sha" >&2
  exit 2
fi
if [[ ! "$repetitions" =~ ^[1-9][0-9]*$ ]] || (( repetitions < 2 || repetitions > 12 )); then
  echo "repetitions must be an integer in [2, 12]: $repetitions" >&2
  exit 2
fi

for required in git cargo rustc jq; do
  command -v "$required" >/dev/null 2>&1 || {
    echo "required command is unavailable: $required" >&2
    exit 1
  }
done

rustc +1.98.1 -Vv >/dev/null 2>&1 || {
  echo "Rust toolchain 1.98.1 is not installed" >&2
  exit 1
}

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

ensure_commit() {
  local revision="$1"
  if git cat-file -e "$revision^{commit}" 2>/dev/null; then
    return 0
  fi

  echo "Fetching exact measured revision: $revision"
  git fetch --no-tags --depth=1 origin "$revision"

  git cat-file -e "$revision^{commit}" || {
    echo "commit is not available after exact-SHA fetch: $revision" >&2
    exit 1
  }
}

ensure_commit "$baseline_sha"
ensure_commit "$candidate_sha"

output_dir="$(mkdir -p "$output_dir" && cd "$output_dir" && pwd)"
rm -rf "$output_dir/runs"
mkdir -p "$output_dir/runs"

temp_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/runenecs-paired-semantic.XXXXXX")"
baseline_tree="$temp_root/baseline"
candidate_tree="$temp_root/candidate"
baseline_target="$temp_root/target-baseline"
candidate_target="$temp_root/target-candidate"

cleanup() {
  git -C "$repo_root" worktree remove --force "$baseline_tree" >/dev/null 2>&1 || true
  git -C "$repo_root" worktree remove --force "$candidate_tree" >/dev/null 2>&1 || true
  rm -rf "$temp_root"
}
trap cleanup EXIT

git worktree add --detach "$baseline_tree" "$baseline_sha" >/dev/null
git worktree add --detach "$candidate_tree" "$candidate_sha" >/dev/null

actual_baseline="$(git -C "$baseline_tree" rev-parse HEAD)"
actual_candidate="$(git -C "$candidate_tree" rev-parse HEAD)"
[[ "$actual_baseline" == "$baseline_sha" ]]
[[ "$actual_candidate" == "$candidate_sha" ]]

provenance="$output_dir/environment.txt"
{
  echo "Runner source revision: $(git -C "$repo_root" rev-parse HEAD)"
  echo "Baseline revision: $actual_baseline"
  echo "Candidate revision: $actual_candidate"
  echo "Repetitions: $repetitions"
  echo "Benchmark command: cargo +1.98.1 bench -p runen-ecs --bench semantic_baseline --locked"
  echo "Criterion configuration: normal defaults from each measured source; no CLI timing overrides"
  echo
  echo "rustc:"
  rustc +1.98.1 -Vv
  echo
  echo "cargo:"
  cargo +1.98.1 -V
  echo
  echo "uname:"
  uname -a
  echo
  echo "CPU:"
  if command -v lscpu >/dev/null 2>&1; then
    lscpu
  else
    echo "lscpu unavailable"
  fi
  echo
  for label in baseline candidate; do
    if [[ "$label" == baseline ]]; then
      revision="$baseline_sha"
    else
      revision="$candidate_sha"
    fi
    echo "$label Git tree: $(git rev-parse "$revision^{tree}")"
    for path in \
      rust-toolchain.toml \
      Cargo.lock \
      crates/runen-ecs/Cargo.toml \
      crates/runen-ecs/benches/semantic_baseline.rs
    do
      echo "$label blob $path: $(git rev-parse "$revision:$path")"
    done
  done
} >"$provenance"

echo "Prebuilding baseline benchmark..."
(
  cd "$baseline_tree"
  CARGO_TARGET_DIR="$baseline_target" \
    cargo +1.98.1 bench -p runen-ecs --bench semantic_baseline --locked --no-run
)

echo "Prebuilding candidate benchmark..."
(
  cd "$candidate_tree"
  CARGO_TARGET_DIR="$candidate_target" \
    cargo +1.98.1 bench -p runen-ecs --bench semantic_baseline --locked --no-run
)

sleep 10

summary="$output_dir/summary.tsv"
printf 'run_index\trepetition\tslot\trevision_label\trevision_sha\tbenchmark\tmedian_ns\tlower_ns\tupper_ns\n' >"$summary"

run_one() {
  local label="$1"
  local revision="$2"
  local repetition="$3"
  local slot="$4"
  local run_index="$5"
  local worktree target

  if [[ "$label" == baseline ]]; then
    worktree="$baseline_tree"
    target="$baseline_target"
  else
    worktree="$candidate_tree"
    target="$candidate_target"
  fi

  local run_dir="$output_dir/runs/$(printf '%02d' "$run_index")-$label"
  mkdir -p "$run_dir/criterion"
  rm -rf "$target/criterion"

  {
    echo "run_index=$run_index"
    echo "repetition=$repetition"
    echo "slot=$slot"
    echo "label=$label"
    echo "revision=$revision"
  } >"$run_dir/run.txt"

  echo "Measuring run $run_index: repetition=$repetition slot=$slot label=$label revision=$revision"
  (
    cd "$worktree"
    CARGO_TARGET_DIR="$target" \
      cargo +1.98.1 bench -p runen-ecs --bench semantic_baseline --locked
  ) 2>&1 | tee "$run_dir/benchmark.log"

  mapfile -d '' estimates < <(
    find "$target/criterion" -type f -path '*/new/estimates.json' -print0 | sort -z
  )
  if (( ${#estimates[@]} == 0 )); then
    echo "no Criterion estimates were produced for $label run $run_index" >&2
    exit 1
  fi

  for estimate in "${estimates[@]}"; do
    relative="${estimate#"$target/criterion/"}"
    benchmark="${relative%/new/estimates.json}"
    json_dir="$run_dir/criterion/$benchmark/new"
    mkdir -p "$json_dir"
    cp "$estimate" "$json_dir/estimates.json"
    for sibling in sample.json tukey.json benchmark.json; do
      source_json="$(dirname "$estimate")/$sibling"
      if [[ -f "$source_json" ]]; then
        cp "$source_json" "$json_dir/$sibling"
      fi
    done

    read -r median lower upper < <(
      jq -r '.median | [.point_estimate, .confidence_interval.lower_bound, .confidence_interval.upper_bound] | @tsv' "$estimate"
    )
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$run_index" "$repetition" "$slot" "$label" "$revision" "$benchmark" \
      "$median" "$lower" "$upper" >>"$summary"
  done
}

run_index=0
for (( repetition=1; repetition<=repetitions; repetition++ )); do
  if (( repetition % 2 == 1 )); then
    order=(baseline candidate)
  else
    order=(candidate baseline)
  fi

  slot=0
  for label in "${order[@]}"; do
    ((slot += 1))
    ((run_index += 1))
    if [[ "$label" == baseline ]]; then
      revision="$baseline_sha"
    else
      revision="$candidate_sha"
    fi
    run_one "$label" "$revision" "$repetition" "$slot" "$run_index"
  done
done

{
  echo
  echo "Run order:"
  find "$output_dir/runs" -mindepth 2 -maxdepth 2 -name run.txt -print0 \
    | sort -z \
    | xargs -0 -r cat
} >>"$provenance"

echo "Paired benchmark evidence: $output_dir"
echo "Summary: $summary"
