#!/usr/bin/env bash
# Golden + smoke tests for the external HTML audience renderer.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HTML_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$HTML_DIR/../../.." && pwd)"
FIXTURES="$SCRIPT_DIR/fixtures"
EXPECTED="$SCRIPT_DIR/expected"
ACTUAL="${TMPDIR:-/tmp}/bvr-audience-html-actual"
NORMALIZED="${TMPDIR:-/tmp}/bvr-audience-html-normalized"
SMOKE="${TMPDIR:-/tmp}/bvr-audience-html-smoke"
NO_ECON="${TMPDIR:-/tmp}/bvr-audience-html-no-econ"
MISMATCH="${TMPDIR:-/tmp}/bvr-audience-html-mismatch"

if ! command -v python3 >/dev/null 2>&1; then
  echo "html-tests: python3 is required" >&2
  exit 127
fi

rm -rf "$ACTUAL" "$NORMALIZED" "$SMOKE" "$NO_ECON" "$MISMATCH"
mkdir -p "$ACTUAL" "$NORMALIZED" "$SMOKE" "$NO_ECON" "$MISMATCH"

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
grep -Fq "No economics overlay" "$NO_ECON/pages/investor.html"
test -s "$NO_ECON/pages/engineer.html"
test -s "$NO_ECON/pages/owner.html"

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
grep -Fq "data_hash mismatch" "$MISMATCH/pages/index.html"

mkdir -p .bv
if [[ ! -f .bv/economics.json ]]; then
  cp examples/contract-consumers/economics.sample.json .bv/economics.json
fi

examples/contract-consumers/html/render.sh .beads/issues.jsonl .bv/economics.json "$SMOKE"

for page in index.html engineer.html owner.html investor.html; do
  test -s "$SMOKE/$page"
  grep -Fq "$(jq -r '.data_hash' .bv/runs/overview.json)" "$SMOKE/$page"
done

echo "html-tests: ok"
