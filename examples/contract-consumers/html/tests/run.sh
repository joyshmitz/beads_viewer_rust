#!/usr/bin/env bash
# Golden + smoke tests for the external HTML audience renderer.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HTML_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$HTML_DIR/../../.." && pwd)"
FIXTURES="$SCRIPT_DIR/fixtures"
EXPECTED="$SCRIPT_DIR/expected"

if ! command -v python3 >/dev/null 2>&1; then
  echo "html-tests: python3 is required" >&2
  exit 127
fi

TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bvr-audience-html.XXXXXX")"
ACTUAL="$TEST_ROOT/actual"
NORMALIZED="$TEST_ROOT/normalized"
SMOKE="$TEST_ROOT/smoke"
NO_ECON="$TEST_ROOT/no-econ"
MISMATCH="$TEST_ROOT/mismatch"
NULL_ECON="$TEST_ROOT/null-econ"
MISSING_PROJECTIONS="$TEST_ROOT/missing-projections"
mkdir -p "$ACTUAL" "$NORMALIZED" "$SMOKE" "$NO_ECON" "$MISMATCH" "$NULL_ECON" "$MISSING_PROJECTIONS"

cd "$ROOT_DIR"

python3 examples/contract-consumers/html/render.py \
  --runs-dir "$FIXTURES" \
  --out-dir "$ACTUAL"

for page in index.html engineer.html owner.html investor.html; do
  python3 - "$ACTUAL/$page" "$NORMALIZED/$page" <<'PY'
import re
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
normalized = re.sub(
    r"<style>\n.*?\n</style>",
    "<style>\n/* inlined CSS omitted in golden */\n</style>",
    source,
    flags=re.S,
)
Path(sys.argv[2]).write_text(normalized, encoding="utf-8")
PY
  diff -u "$EXPECTED/$page" "$NORMALIZED/$page"
done

grep -Fq "fixturehash1234" "$NORMALIZED/index.html"
grep -Fq "&lt;script&gt;alert(1)&lt;/script&gt;" "$NORMALIZED/engineer.html"
grep -Fq "budget_utilization_pct</td><td>18%</td>" "$NORMALIZED/investor.html"
grep -Fq "estimate_coverage_pct</td><td>75%</td>" "$NORMALIZED/investor.html"
if grep -Fq "<script>alert(1)</script>" "$NORMALIZED/engineer.html"; then
  echo "html-tests: script title was not escaped" >&2
  exit 1
fi

python3 - "$FIXTURES" "$NO_ECON/runs" <<'PY'
import shutil
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
for name in ("overview.json", "delivery.json"):
    shutil.copy2(source / name, target / name)
PY
python3 examples/contract-consumers/html/render.py \
  --runs-dir "$NO_ECON/runs" \
  --out-dir "$NO_ECON/pages"
grep -Fq "Missing $NO_ECON/runs/economics.json" "$NO_ECON/pages/investor.html"
test -s "$NO_ECON/pages/engineer.html"
test -s "$NO_ECON/pages/owner.html"

python3 - "$FIXTURES" "$NULL_ECON/runs" <<'PY'
import json
import shutil
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
for name in ("overview.json", "delivery.json", "economics.json"):
    shutil.copy2(source / name, target / name)
economics_path = target / "economics.json"
economics = json.loads(economics_path.read_text(encoding="utf-8"))
economics["inputs"]["budget_envelope"] = None
economics["projections"]["cost_to_complete"] = None
economics["projections"]["budget_utilization_pct"] = None
economics_path.write_text(json.dumps(economics, indent=2) + "\n", encoding="utf-8")
PY
python3 examples/contract-consumers/html/render.py \
  --runs-dir "$NULL_ECON/runs" \
  --out-dir "$NULL_ECON/pages"
grep -Fq "budget_envelope</td><td>—</td>" "$NULL_ECON/pages/investor.html"
grep -Fq "cost_to_complete</td><td>—</td>" "$NULL_ECON/pages/investor.html"
grep -Fq "budget_utilization_pct</td><td>—</td>" "$NULL_ECON/pages/investor.html"
if grep -Fq ">null<" "$NULL_ECON/pages/investor.html"; then
  echo "html-tests: null leaked into investor HTML" >&2
  exit 1
fi

python3 - "$FIXTURES" "$MISSING_PROJECTIONS/runs" <<'PY'
import json
import shutil
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
for name in ("overview.json", "delivery.json", "economics.json"):
    shutil.copy2(source / name, target / name)
economics_path = target / "economics.json"
economics = json.loads(economics_path.read_text(encoding="utf-8"))
economics["projections"].pop("cost_to_complete", None)
economics["projections"].pop("budget_utilization_pct", None)
economics_path.write_text(json.dumps(economics, indent=2) + "\n", encoding="utf-8")
PY
python3 examples/contract-consumers/html/render.py \
  --runs-dir "$MISSING_PROJECTIONS/runs" \
  --out-dir "$MISSING_PROJECTIONS/pages"
grep -Fq "cost_to_complete</td><td>—</td>" "$MISSING_PROJECTIONS/pages/investor.html"
grep -Fq "budget_utilization_pct</td><td>—</td>" "$MISSING_PROJECTIONS/pages/investor.html"

python3 - "$FIXTURES" "$MISMATCH/runs" <<'PY'
import json
import shutil
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
for name in ("overview.json", "delivery.json", "economics.json"):
    shutil.copy2(source / name, target / name)
delivery_path = target / "delivery.json"
delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
delivery["data_hash"] = "differenthash9999"
delivery_path.write_text(json.dumps(delivery, indent=2) + "\n", encoding="utf-8")
PY
python3 examples/contract-consumers/html/render.py \
  --runs-dir "$MISMATCH/runs" \
  --out-dir "$MISMATCH/pages"
for page in index.html engineer.html owner.html investor.html; do
  grep -Fq "data_hash mismatch" "$MISMATCH/pages/$page"
done

mkdir -p .bv
if [[ ! -f .bv/economics.json ]]; then
  cp examples/contract-consumers/economics.sample.json .bv/economics.json
fi

examples/contract-consumers/html/render.sh .beads/issues.jsonl .bv/economics.json "$SMOKE"

mapfile -t LIVE_HASHES < <(
  for json in .bv/runs/overview.json .bv/runs/delivery.json .bv/runs/economics.json; do
    if [[ -f "$json" ]]; then
      jq -r '.data_hash // empty' "$json"
    fi
  done | sort -u
)

for page in index.html engineer.html owner.html investor.html; do
  test -s "$SMOKE/$page"
  if [[ "${#LIVE_HASHES[@]}" -eq 1 ]]; then
    grep -Fq "${LIVE_HASHES[0]}" "$SMOKE/$page"
  else
    grep -Fq "data_hash mismatch" "$SMOKE/$page"
  fi
done

echo "html-tests: ok"
