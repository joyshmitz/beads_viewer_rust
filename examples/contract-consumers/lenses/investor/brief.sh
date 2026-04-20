#!/usr/bin/env bash
# Investor / finance lens — financial view derived from the same triad.
#
# Answers: "how much is this costing, how much is it likely to cost to
# finish, where is budget being consumed, are there economic drags?"
#
# Deliberately abstains from throwing around status labels (no "on track",
# no "at risk"). Downstream decides thresholds; this renderer just shows
# the numbers and the provenance so a finance reviewer can interpret them.
#
# Usage: ./brief.sh [runs-dir]
set -euo pipefail
RUNS_DIR="${1:-.bv/runs}"

EC="$RUNS_DIR/economics.json"
DL="$RUNS_DIR/delivery.json"

if [[ ! -f "$EC" ]]; then
  echo "investor-lens: $EC missing; run .bv/examples/triad.sh first" >&2
  exit 1
fi

printf '== financial summary — %s ==\n\n' "$(jq -r '.generated_at' "$EC")"

printf 'inputs (what this projection assumes):\n'
jq -r '
  .inputs |
  "  hourly rate:              \(.hourly_rate) \(.currency // "USD")/hour",
  "  staffed hours per day:    \(.hours_per_day)",
  "  budget envelope:          \(.budget_envelope // "—") \(.currency // "USD")",
  "  throughput window:        \(.throughput_window_days) days",
  "  project age:              \(.project_age_days) days",
  "  estimate coverage:        \(.estimate_coverage_pct * 100 | round)%",
  "  open items / closed in window: \(.open_issues) / \(.closed_in_window)"
' "$EC"

printf '\nprojections:\n'
jq -r '
  .projections |
  "  burn rate:           \(.burn_rate_per_day) /day",
  "  throughput:          \(.throughput_issues_per_day * 100 | round / 100) items/day",
  "  cost to complete:    \(.cost_to_complete | if . == null then "—" else round end)",
  "  budget utilization:  \(.budget_utilization_pct | if . == null then "—" else (. * 100 | .*100|round/100 | tostring) + "%" end)"
' "$EC"

printf '\ntop sources of economic drag (cost-of-delay by downstream reach):\n'
jq -r '.projections.cost_of_delay[:5][] | "  \(.id)  \(.rate_per_day)/day × \(.dependents_count) dependents"' "$EC"

printf '\nguards (data-quality flags; downstream decides severity):\n'
jq -r '.guards | to_entries[] | "  \(.key): \(if .value then "TRIPPED" else "ok" end)"' "$EC"

if [[ -f "$DL" ]]; then
  printf '\ncapacity mix (what the burn is funding):\n'
  jq -r '.flow_distribution[] | "  \(.category): \(.pct)%"' "$DL"
fi

printf '\nprovenance:\n'
jq -r '
  "  data_hash:     \(.data_hash)",
  "  overlay_hash:  \(.overlay_hash)",
  "  schema_ver:    \(.schema_version)",
  "  bvr:           \(.version)"
' "$EC"
