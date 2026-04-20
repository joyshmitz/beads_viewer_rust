#!/usr/bin/env bash
# Engineer lens — concise terminal briefing from the cached triad.
#
# Answers: "what should I do right now, what unblocks the most, what's
# blocking me?" Leans on overview (next step) + delivery (flow mix as a
# sanity check that we're not drowning in debt) + economics (cost-of-delay
# ordering mirrors blocks_count, useful when picking between equal-priority
# items).
#
# Usage: ./brief.sh [runs-dir]
set -euo pipefail
RUNS_DIR="${1:-.bv/runs}"

for f in overview.json delivery.json economics.json; do
  if [[ ! -f "$RUNS_DIR/$f" ]]; then
    echo "engineer-lens: $RUNS_DIR/$f missing; run .bv/examples/triad.sh first" >&2
    exit 1
  fi
done

OV="$RUNS_DIR/overview.json"
DL="$RUNS_DIR/delivery.json"
EC="$RUNS_DIR/economics.json"

printf '== engineer brief — %s ==\n\n' "$(jq -r '.generated_at' "$OV")"

printf 'graph state:\n'
jq -r '.summary | "  open=\(.open_issues)  in_progress=\(.in_progress_issues)  blocked=\(.blocked_issues)  cycles=\(.cycle_count)"' "$OV"

printf '\nnext moves (unlock coverage):\n'
jq -r '.unlock_maximizers[] | "  \(.id)  +\(.marginal_unlocks) unlocks  — \(.title)\n    claim: \(.claim_command)"' "$OV" | head -20

printf '\nflow mix (sanity check):\n'
jq -r '.flow_distribution[] | select(.count>0) | "  \(.category): \(.count) (\(.pct)%)"' "$DL"

printf '\ntop blockers by downstream reach:\n'
jq -r '.projections.cost_of_delay[:5][] | "  \(.id)  deps=\(.dependents_count)  — \(.title)"' "$EC"

printf '\nguards:\n'
jq -r '.guards | to_entries | map(select(.value==true)) | if length==0 then "  none tripped" else .[] | "  * \(.key)" end' "$EC"
