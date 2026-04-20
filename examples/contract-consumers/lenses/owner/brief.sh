#!/usr/bin/env bash
# Owner / delivery-lead lens — rewrites the triad as a delivery-posture
# briefing: how is capacity distributed, what urgency cohorts are live,
# where is the schedule pressure.
#
# Deliberately skips per-issue IDs. Owners want aggregates, not claim
# commands.
#
# Usage: ./brief.sh [runs-dir]
set -euo pipefail
RUNS_DIR="${1:-.bv/runs}"

DL="$RUNS_DIR/delivery.json"
EC="$RUNS_DIR/economics.json"

if [[ ! -f "$DL" ]]; then
  echo "owner-lens: $DL missing; run .bv/examples/triad.sh first" >&2
  exit 1
fi

printf '== delivery posture — %s ==\n\n' "$(jq -r '.generated_at' "$DL")"

OPEN=$(jq -r '.open_issues' "$DL")
printf 'active work: %s open issue(s)\n\n' "$OPEN"

printf 'flow mix (where capacity is going):\n'
jq -r '.flow_distribution[] | "  \(.category | ascii_upcase):  \(.pct)%  (\(.count) item\(if .count == 1 then "" else "s" end))"' "$DL"

printf '\nurgency cohorts:\n'
jq -r '.urgency_profile[] | "  \(.category | ascii_upcase):  \(.pct)%  (\(.count))"' "$DL"

printf '\nmilestone pressure:\n'
PRESSURE_COUNT=$(jq '.milestone_pressure | length' "$DL" 2>/dev/null || echo 0)
if [[ "$PRESSURE_COUNT" -eq 0 ]]; then
  printf '  none (no due_date items inside window)\n'
else
  jq -r '.milestone_pressure[] | "  \(.id)  \(if .is_overdue then "OVERDUE" else "due soon" end)\(if .is_blocked then " [BLOCKED]" else "" end): \(.title)"' "$DL"
fi

if [[ -f "$EC" ]]; then
  printf '\ndelivery-adjacent economics:\n'
  jq -r '
    .projections |
    "  burn rate:       \($rate)/day"      as $l1 |
    "  throughput:      \(.throughput_issues_per_day | .*100|round/100) items/day" as $l2 |
    "  cost to finish:  \(.cost_to_complete | round) (at current pace)" as $l3 |
    ($l1, $l2, $l3) | .
  ' --argjson rate "$(jq '.projections.burn_rate_per_day' "$EC")" "$EC"
fi
