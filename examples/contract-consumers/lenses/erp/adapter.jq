# ERP / finance-system adapter.
#
# Transforms a `bvr --robot-economics` envelope into a minimal
# period-end reporting shape that a typical finance ingestion pipeline
# can consume without knowing anything about beads or bvr.
#
# Usage:
#   jq -f .bv/examples/lenses/erp/adapter.jq --arg project bvr \
#      .bv/examples/runs/economics.json
#
# Design notes:
#   - Field names use `snake_case` plus `_usd` suffix for monetary values;
#     currency is promoted to a top-level field so downstream doesn't have
#     to parse strings.
#   - `data_quality.guards_tripped` is a list of strings (not a map) so
#     ERP schema stays flat.
#   - Nothing here interprets thresholds. "budget_utilization_fraction":
#     0.95 is a number, not "warning". The receiving system decides.

{
  project: ($project // "unknown"),
  period_end: .generated_at,
  currency: (.inputs.currency // "USD"),
  metrics: {
    daily_burn: .projections.burn_rate_per_day,
    forecast_cost_to_complete: .projections.cost_to_complete,
    budget_envelope: .inputs.budget_envelope,
    budget_utilization_fraction: .projections.budget_utilization_pct,
    throughput_items_per_day: .projections.throughput_issues_per_day,
    open_items: .inputs.open_issues,
    closed_items_in_window: .inputs.closed_in_window,
    throughput_window_days: .inputs.throughput_window_days,
    project_age_days: .inputs.project_age_days
  },
  cost_of_delay: [
    .projections.cost_of_delay[] | {
      item_id: .id,
      dependents_count: .dependents_count,
      rate_per_day: .rate_per_day
    }
  ],
  data_quality: {
    estimate_coverage_fraction: .inputs.estimate_coverage_pct,
    guards_tripped: [
      .guards | to_entries[] | select(.value == true) | .key
    ]
  },
  provenance: {
    data_hash: .data_hash,
    overlay_hash: .overlay_hash,
    schema_version: .schema_version,
    source_tool: "bvr",
    source_version: .version
  }
}
