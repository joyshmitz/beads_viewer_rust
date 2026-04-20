#!/usr/bin/env bash
# triad.sh — run the three bvr robot-projection primitives against a beads
# corpus and cache the JSON envelopes.
#
# Usage:
#   ./triad.sh [beads-file] [overlay-json] [out-dir]
#
# Defaults:
#   beads-file   .beads/issues.jsonl (relative to repo root)
#   overlay-json .bv/economics.json  (copy from examples/contract-consumers/economics.sample.json)
#   out-dir      .bv/runs            (inside .bv/ so cached JSON stays gitignored)
#
# Output files (always overwritten):
#   <out-dir>/overview.json
#   <out-dir>/delivery.json
#   <out-dir>/economics.json
#
# Each file is a stable, machine-readable contract. Lens renderers and
# downstream consumers read from here; nobody re-runs bvr per lens.

set -euo pipefail

BEADS_FILE="${1:-.beads/issues.jsonl}"
OVERLAY="${2:-.bv/economics.json}"
OUT_DIR="${3:-.bv/runs}"

if [[ ! -x ./target/release/bvr ]]; then
  echo "triad: ./target/release/bvr not found; run \`cargo build --release --bin bvr\` first" >&2
  exit 1
fi

if [[ ! -f "$BEADS_FILE" ]]; then
  echo "triad: beads file $BEADS_FILE not found" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

./target/release/bvr --robot-overview --beads-file "$BEADS_FILE" > "$OUT_DIR/overview.json"
./target/release/bvr --robot-delivery --beads-file "$BEADS_FILE" > "$OUT_DIR/delivery.json"

if [[ -f "$OVERLAY" ]]; then
  ./target/release/bvr --robot-economics --economics-overlay "$OVERLAY" --beads-file "$BEADS_FILE" > "$OUT_DIR/economics.json"
else
  echo "triad: overlay $OVERLAY missing; skipping economics" >&2
fi

echo "triad: wrote $OUT_DIR/{overview,delivery,economics}.json"
