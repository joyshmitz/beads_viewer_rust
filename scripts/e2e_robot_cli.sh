#!/usr/bin/env bash
# e2e_robot_cli.sh — shell-level robot CLI verification with replay artifacts.
#
# Usage:
#   scripts/e2e_robot_cli.sh [path-to-bvr-binary]
#
# Notes:
# - This script only runs an existing binary. Build/test with `rch` separately.
# - Artifacts are always preserved and grouped per scenario for later replay.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BVR="${BVR_BIN:-${1:-${PROJECT_ROOT}/target/debug/bvr}}"
FIXTURE="${E2E_FIXTURE:-${PROJECT_ROOT}/tests/testdata/minimal.jsonl}"
DIFF_FIXTURE="${E2E_DIFF_FIXTURE:-${PROJECT_ROOT}/tests/testdata/all_closed.jsonl}"
ARTIFACT_ROOT="${E2E_ARTIFACT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/bvr_e2e_robot_XXXXXX")}"

PASS=0
FAIL=0

log() {
    echo "  [robot-e2e] $*"
}

shell_quote() {
    if [[ -z "${1}" ]]; then
        printf "''"
        return
    fi
    printf "'%s'" "${1//\'/\'\"\'\"\'}"
}

write_artifacts() {
    local scenario="$1"
    shift
    local stdout_file="$1"
    shift
    local stderr_file="$1"
    shift
    local exit_code="$1"
    shift
    local replay_args=()
    local arg
    local scenario_dir="${ARTIFACT_ROOT}/${scenario}"
    mkdir -p "$scenario_dir"
    cp "$stdout_file" "$scenario_dir/stdout.txt"
    cp "$stderr_file" "$scenario_dir/stderr.txt"
    cp "$FIXTURE" "$scenario_dir/fixture.jsonl"
    for arg in "$@"; do
        replay_args+=("$(shell_quote "$arg")")
    done
    cat > "$scenario_dir/replay.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
BVR_BIN="\${BVR_BIN:-$BVR}"
"\$BVR_BIN" ${replay_args[*]} --beads-file "\$(dirname "\$0")/fixture.jsonl"
EOF
    chmod +x "$scenario_dir/replay.sh"
    python3 - "$scenario" "$BVR" "$FIXTURE" "$exit_code" "$@" <<'PY' > "$scenario_dir/meta.json"
import json
import sys
print(json.dumps({
  "scenario": sys.argv[1],
  "binary": sys.argv[2],
  "fixture": sys.argv[3],
  "exit_code": int(sys.argv[4]),
  "args": sys.argv[5:],
}, indent=2))
PY
}

run_capture() {
    local scenario="$1"
    shift
    local out_dir="${ARTIFACT_ROOT}/_tmp/${scenario}"
    mkdir -p "$out_dir"
    local stdout_file="${out_dir}/stdout.txt"
    local stderr_file="${out_dir}/stderr.txt"
    if "$BVR" "$@" --beads-file "$FIXTURE" >"$stdout_file" 2>"$stderr_file"; then
        write_artifacts "$scenario" "$stdout_file" "$stderr_file" 0 "$@"
        return 0
    fi
    local exit_code=$?
    write_artifacts "$scenario" "$stdout_file" "$stderr_file" "$exit_code" "$@"
    return "$exit_code"
}

pass() {
    PASS=$((PASS + 1))
    log "PASS: $1"
}

fail() {
    FAIL=$((FAIL + 1))
    log "FAIL: $1"
}

assert_json_field() {
    local file="$1"
    local field="$2"
    python3 - "$file" "$field" <<'PY'
import json
import sys
path, field = sys.argv[1], sys.argv[2]
value = json.load(open(path))
current = value
for segment in field.split('.'):
    current = current[segment]
print(current if not isinstance(current, (dict, list)) else "ok")
PY
}

if [[ ! -x "$BVR" ]]; then
    echo "error: bvr binary not found at $BVR" >&2
    exit 1
fi

if [[ ! -f "$FIXTURE" ]]; then
    echo "error: fixture not found at $FIXTURE" >&2
    exit 1
fi

log "Binary: $BVR"
log "Fixture: $FIXTURE"
log "Artifacts: $ARTIFACT_ROOT"

if run_capture "triage_json" --robot-triage; then
    assert_json_field "${ARTIFACT_ROOT}/triage_json/stdout.txt" "triage.quick_ref.total_open" >/dev/null
    pass "triage_json"
else
    fail "triage_json"
fi

if TOON_STATS=1 run_capture "next_toon" --robot-next --format toon; then
    if grep -q "savings" "${ARTIFACT_ROOT}/next_toon/stderr.txt"; then
        pass "next_toon"
    else
        fail "next_toon"
    fi
else
    fail "next_toon"
fi

if run_capture "docs_guide" --robot-docs guide; then
    assert_json_field "${ARTIFACT_ROOT}/docs_guide/stdout.txt" "topic" >/dev/null
    pass "docs_guide"
else
    fail "docs_guide"
fi

if run_capture "schema_full" --robot-schema; then
    assert_json_field "${ARTIFACT_ROOT}/schema_full/stdout.txt" "commands" >/dev/null
    pass "schema_full"
else
    fail "schema_full"
fi

if run_capture "graph_dot" --robot-graph --graph-format dot; then
    if grep -q "digraph" "${ARTIFACT_ROOT}/graph_dot/stdout.txt"; then
        pass "graph_dot"
    else
        fail "graph_dot"
    fi
else
    fail "graph_dot"
fi

mkdir -p "${ARTIFACT_ROOT}/robot_diff"
if "$BVR" --robot-diff --diff-since "$DIFF_FIXTURE" --beads-file "$FIXTURE" \
    >"${ARTIFACT_ROOT}/robot_diff/stdout.txt" 2>"${ARTIFACT_ROOT}/robot_diff/stderr.txt"; then
    cp "$FIXTURE" "${ARTIFACT_ROOT}/robot_diff/fixture.jsonl"
    cp "$DIFF_FIXTURE" "${ARTIFACT_ROOT}/robot_diff/diff_since.jsonl"
    cat > "${ARTIFACT_ROOT}/robot_diff/replay.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
BVR_BIN="\${BVR_BIN:-$BVR}"
"\$BVR_BIN" --robot-diff --diff-since "\$(dirname "\$0")/diff_since.jsonl" --beads-file "\$(dirname "\$0")/fixture.jsonl"
EOF
    chmod +x "${ARTIFACT_ROOT}/robot_diff/replay.sh"
    if python3 - "${ARTIFACT_ROOT}/robot_diff/stdout.txt" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1]))
assert "diff" in payload and "from_data_hash" in payload and "to_data_hash" in payload
PY
    then
        pass "robot_diff"
    else
        fail "robot_diff"
    fi
else
    fail "robot_diff"
fi

echo
log "Results: $PASS passed, $FAIL failed"
log "Artifacts preserved at: $ARTIFACT_ROOT"

if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
