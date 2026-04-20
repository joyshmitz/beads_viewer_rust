#!/usr/bin/env bash
# portfolio/rollup.sh — aggregate economics + delivery across N projects.
#
# Given a list of projects (each a directory with its own .beads/ and
# .bv/economics.json overlay), this collects per-project triads and emits
# a single rollup JSON. The rollup is the canonical primitive for a
# multi-project finance dashboard, PMO overview, or cross-project agent
# coordinator.
#
# Usage:
#   ./rollup.sh project-a-dir project-b-dir [project-c-dir...]
#
# Assumption per project directory:
#   - target/release/bvr exists (or bvr on PATH)
#   - .beads/issues.jsonl
#   - .bv/economics.json (for economics; optional — portfolio shows null if absent)
#
# Output: JSON on stdout with shape
#   {
#     "generated_at": "...",
#     "project_count": N,
#     "totals": {
#       "daily_burn": ...,
#       "forecast_cost_to_complete": ...,
#       "open_items": ...,
#       "closed_items_in_window": ...
#     },
#     "by_project": [
#       { "project": "...", "burn": ..., "cost_to_complete": ..., "open_items": ...,
#         "guards_tripped": [...], "data_hash": ..., "overlay_hash": ... },
#       ...
#     ]
#   }
#
# Composability is the point: contracts aggregate by arithmetic; HTML
# lenses don't.

set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 project-dir [project-dir...]" >&2
  exit 2
fi

GEN="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
PROJECTS=()
SUM_BURN=0
SUM_COST=0
SUM_OPEN=0
SUM_CLOSED_WINDOW=0

for DIR in "$@"; do
  if [[ ! -d "$DIR" ]]; then
    echo "skip: $DIR not a directory" >&2
    continue
  fi
  NAME="$(basename "$DIR")"

  # Prefer a repo-local bvr; fall back to PATH.
  BVR="$DIR/target/release/bvr"
  [[ -x "$BVR" ]] || BVR="$(command -v bvr || true)"
  if [[ -z "$BVR" || ! -x "$BVR" ]]; then
    echo "skip: $NAME has no bvr binary" >&2
    continue
  fi

  BEADS="$DIR/.beads/issues.jsonl"
  OVERLAY="$DIR/.bv/economics.json"
  [[ -f "$BEADS" ]] || { echo "skip: $NAME missing $BEADS" >&2; continue; }

  if [[ -f "$OVERLAY" ]]; then
    ECON_JSON="$("$BVR" --robot-economics --economics-overlay "$OVERLAY" --beads-file "$BEADS")"
  else
    ECON_JSON="null"
  fi

  ENTRY="$(jq -n \
    --arg project "$NAME" \
    --argjson econ "$ECON_JSON" \
    '{
      project: $project,
      burn: ($econ.projections.burn_rate_per_day // null),
      cost_to_complete: ($econ.projections.cost_to_complete // null),
      open_items: ($econ.inputs.open_issues // null),
      closed_items_in_window: ($econ.inputs.closed_in_window // null),
      budget_envelope: ($econ.inputs.budget_envelope // null),
      budget_utilization_pct: ($econ.projections.budget_utilization_pct // null),
      guards_tripped: (
        $econ.guards // {}
        | to_entries
        | map(select(.value == true) | .key)
      ),
      data_hash: ($econ.data_hash // null),
      overlay_hash: ($econ.overlay_hash // null)
    }')"

  PROJECTS+=("$ENTRY")

  BURN=$(echo "$ENTRY" | jq -r '.burn // 0')
  COST=$(echo "$ENTRY" | jq -r '.cost_to_complete // 0')
  OPEN=$(echo "$ENTRY" | jq -r '.open_items // 0')
  CLOSED=$(echo "$ENTRY" | jq -r '.closed_items_in_window // 0')
  SUM_BURN=$(awk -v a="$SUM_BURN" -v b="$BURN" 'BEGIN{printf "%.4f", a+b}')
  SUM_COST=$(awk -v a="$SUM_COST" -v b="$COST" 'BEGIN{printf "%.4f", a+b}')
  SUM_OPEN=$((SUM_OPEN + OPEN))
  SUM_CLOSED_WINDOW=$((SUM_CLOSED_WINDOW + CLOSED))
done

PROJECT_JSON="$(printf '%s\n' "${PROJECTS[@]}" | jq -s '.')"

jq -n \
  --arg gen "$GEN" \
  --argjson count "${#PROJECTS[@]}" \
  --argjson burn "$SUM_BURN" \
  --argjson cost "$SUM_COST" \
  --argjson open "$SUM_OPEN" \
  --argjson closed "$SUM_CLOSED_WINDOW" \
  --argjson projects "$PROJECT_JSON" \
  '{
    generated_at: $gen,
    project_count: $count,
    totals: {
      daily_burn: $burn,
      forecast_cost_to_complete: $cost,
      open_items: $open,
      closed_items_in_window: $closed
    },
    by_project: $projects
  }'
