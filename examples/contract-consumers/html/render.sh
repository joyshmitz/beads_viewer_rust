#!/usr/bin/env bash
# Render static HTML audience lenses from bvr robot-contract JSON.
#
# Usage:
#   render.sh [beads-file] [overlay-json] [out-dir]
#
# Set BVR_HTML_SKIP_TRIAD=1 to render from an existing .bv/runs directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"

BEADS_FILE="${1:-.beads/issues.jsonl}"
OVERLAY="${2:-.bv/economics.json}"
OUT_DIR="${3:-.bv/audience-html}"
RUNS_DIR=".bv/runs"

if ! command -v python3 >/dev/null 2>&1; then
  echo "html-render: python3 is required" >&2
  exit 127
fi

cd "$ROOT_DIR"

if [[ "${BVR_HTML_SKIP_TRIAD:-0}" != "1" ]]; then
  examples/contract-consumers/triad.sh "$BEADS_FILE" "$OVERLAY" "$RUNS_DIR"
fi

python3 examples/contract-consumers/html/render.py \
  --runs-dir "$RUNS_DIR" \
  --out-dir "$OUT_DIR"
