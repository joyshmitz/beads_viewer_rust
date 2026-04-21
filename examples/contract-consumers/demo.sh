#!/usr/bin/env bash
# demo.sh — end-to-end walkthrough of the contract-consumer examples.
#
# Regenerates the triad and runs every lens against the current project's
# beads corpus, printing each section under a labeled header. Nothing is
# written to disk beyond the triad's cached JSONs under .bv/runs/.
#
# Usage (from repo root):
#   examples/contract-consumers/demo.sh
#
# Prerequisites:
#   - `cargo build --release --bin bvr` has produced ./target/release/bvr
#   - .bv/economics.json exists (see README → Quick Start, step 2)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

if [[ ! -x ./target/release/bvr ]]; then
  echo "demo: ./target/release/bvr not found; run \`cargo build --release --bin bvr\` first" >&2
  exit 1
fi

if [[ ! -f .bv/economics.json ]]; then
  echo "demo: .bv/economics.json missing; copy it with" >&2
  echo "      cp examples/contract-consumers/economics.sample.json .bv/economics.json" >&2
  echo "      then edit the overlay to match your own rates." >&2
  exit 1
fi

section() { printf '\n==== %s ====\n\n' "$1"; }

section "triad — regenerate cached envelopes"
"$SCRIPT_DIR/triad.sh"

section "engineer lens"
"$SCRIPT_DIR/lenses/engineer/brief.sh"

section "owner lens"
"$SCRIPT_DIR/lenses/owner/brief.sh"

section "investor lens"
"$SCRIPT_DIR/lenses/investor/brief.sh"

section "erp adapter (jq)"
jq -f "$SCRIPT_DIR/lenses/erp/adapter.jq" --arg project bvr .bv/runs/economics.json
