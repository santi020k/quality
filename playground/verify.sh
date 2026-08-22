#!/bin/sh

set -eu

playground_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(CDPATH= cd -- "$playground_dir/.." && pwd)
temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/quality-playground.XXXXXX")
sandbox="$temporary_root/project"

cleanup() {
  rm -rf "$temporary_root"
}
trap cleanup EXIT HUP INT TERM

run_quality() {
  cargo run --quiet --manifest-path "$repository_dir/Cargo.toml" -- \
    --root "$sandbox" "$@"
}

sh "$playground_dir/setup.sh" "$sandbox" >/dev/null
run_quality doctor >/dev/null

if run_quality check >/dev/null 2>&1; then
  echo "Expected the initial lint check to fail." >&2
  exit 1
fi

run_quality check --format json > "$temporary_root/check.json" || true
grep -q 'demo-warning' "$temporary_root/check.json"

# A warning can be reported without failing an error-only policy.
run_quality check --report-level warning --fail-level error >/dev/null

# An error marker exercises the blocking severity path, then the fixture is
# restored before testing baselines and fixes.
printf '\nblocking_example = true # QUALITY_ERROR\n' >> "$sandbox/src/greeting.demo"
if run_quality check --fail-level error >/dev/null 2>&1; then
  echo "Expected an error-level finding to fail the check." >&2
  exit 1
fi
run_quality check --format json > "$temporary_root/error.json" || true
grep -q 'demo-error' "$temporary_root/error.json"
git -C "$sandbox" checkout -- src/greeting.demo

run_quality check --report reports/quality.sarif >/dev/null 2>&1 || true
test -f "$sandbox/reports/quality.sarif"
grep -q 'demo-warning' "$sandbox/reports/quality.sarif"

run_quality baseline create >/dev/null
run_quality check >/dev/null

printf '\nchanged = true\n' >> "$sandbox/src/greeting.demo"
run_quality check --changed >/dev/null

run_quality fix >/dev/null

if run_quality format --check >/dev/null 2>&1; then
  echo "Expected the initial formatting check to fail." >&2
  exit 1
fi

run_quality format >/dev/null
run_quality format --check >/dev/null

echo "Playground verification passed."
