# Audience Export — Implementation Plan

Multi-dimensional project views for `--export-pages`, with audience-specific
lenses and optional economics projection.

## Why this exists

bvr's `--export-pages` is a powerful engineering tool — SQL.js queries,
dependency graphs, triage recommendations. The Bloomberg terminal of issue
tracking.

But the terminal shows topology, not economics. Every project has a financial
dimension: what did this cost, what will it cost to finish, where is money
being burned on blocked work. Today that analysis happens in spreadsheets
disconnected from the actual work graph.

bvr already computes velocity, critical path, blocked pressure, and scope
trajectory. Adding a cost dimension turns these into burn rate, projected
cost to completion, and cost-of-delay — the metrics that drive funding and
prioritization decisions.

The economics layer is a thin projection on top of existing metrics — not a
new data pipeline. Keeping it inside bvr means economics stay in sync with
the work graph automatically, unlike a spreadsheet that drifts the moment
someone closes a bead.

For AI agent workflows: bvr + economics = project-level cost tracking.
Which beads cost the most relative to their graph impact? What is the
burn rate of a blocked subgraph? This requires the dependency graph +
velocity together — no external tool can compute it. Per-agent ROI is
a future goal contingent on richer attribution data in the model
(currently `assignee` + `estimated_minutes` exist but lack the
labor-cost primitives needed for trustworthy agent-level costing).

## Design principle: one source, three lenses

All audiences see the same five dimensions. The audience flag controls
emphasis and language, not content visibility:

| Dimension | Engineer lens | Owner lens | Investor lens |
|-----------|--------------|------------|---------------|
| **Progress** | Actionable items, tracks, WIP | % complete, milestone ETA, risks | Delivery predictability, schedule variance |
| **Technical Health** | Cycles, bottlenecks, graph detail | Aggregated risk count | Cost impact of technical issues |
| **Economics** | Cost-per-bead, burn rate | Burn rate, cost-to-complete | Budget utilization, projections |
| **Risk** | Per-bead: staleness, orphans, alerts | "3 blockers threaten Q2 milestone" | "Blocked work costs $4,200/week" |
| **Flow Distribution** | Feature/Bug/Debt/Risk capacity split | Balance assessment | Value delivery ratio |

## What this adds

When `--audience` is set, `--export-pages` generates additional surfaces
alongside the existing SPA:

```
# engineer / owner (has_app = true):
output/
├── index.html          ← UNCHANGED: full bvr SPA stays at root
├── executive.html      ← NEW: audience executive summary (<15KB, no-JS)
├── dashboard/
│   └── index.html      ← NEW: multi-dimensional dashboard (CSS-only)
├── data/               ← UNCHANGED
└── beads.sqlite3       ← UNCHANGED

# investor (has_app = false):
output/
├── index.html          ← NEW: executive summary (replaces SPA at root)
├── dashboard/
│   └── index.html      ← NEW: multi-dimensional dashboard (CSS-only)
├── data/               ← UNCHANGED
└── beads.sqlite3       ← UNCHANGED
# (no SPA, no vendor/, no assets/ — viewer not generated)
```

The SPA is never relocated. Its JS fetches `data/*` and `beads.sqlite3`
via relative paths (`fetchJson("data/meta.json")` etc.), so moving it
to a subdirectory would break all data loading. Instead, audience pages
are added alongside the SPA (engineer/owner) or replace it (investor).

Without `--audience`, export is identical to current behavior.

## CLI surface

```bash
# Existing (unchanged)
bvr --export-pages ./output

# New: audience-specific export
bvr --export-pages ./output --audience owner
bvr --export-pages ./output --audience investor --locale uk
bvr --export-pages ./output --audience owner --overlay .bv/audience.yaml

# New: robot mode economics (no HTML, JSON output)
bvr --robot-economics --overlay .bv/audience.yaml
```

### New flags

| Flag | Type | Default | Purpose |
|------|------|---------|---------|
| `--audience` | `owner\|investor\|engineer` | none | Selects audience lens for export |
| `--locale` | string | `en` | UI string localization (e.g. `uk`, `de`) |
| `--overlay` | path | none | YAML config for content/economics/branding |
| `--robot-economics` | bool | false | Emit EconomicsProjection as JSON envelope |

### Flag validation rules

- `--audience` requires `--export-pages` (audience lens = HTML output)
- `--robot-economics` requires `--overlay` with `economics` section
  (no overlay = no hourly_rate = nothing to compute)
- `--robot-economics` does NOT require `--audience` or `--export-pages`
  (it's robot-mode JSON, independent of HTML export)
- `--locale` and `--overlay` can be used with either `--audience` or
  `--robot-economics`
- `--audience` without `--export-pages` → clear error:
  "use --export-pages with --audience, or --robot-economics for JSON"

### Audience matrix

All audiences see all five dimensions. Cells show detail level, not
presence/absence:

| Surface | (none) | engineer | owner | investor |
|---------|--------|----------|-------|----------|
| Full SPA | root `index.html` | root `index.html` | root `index.html` | — (not generated) |
| Executive summary | — | `executive.html` (technical) | `executive.html` (balanced) | root `index.html` (financial) |
| Dashboard | — | full detail | balanced | summary |
| Economics depth | — | per-bead cost | burn rate + budget | full projections |
| Flow Distribution | — | capacity split | balance assessment | value delivery % |

## Data flow

No new data collection. All data comes from already-computed analysis.
Economics is a projection layer using existing metrics + overlay config:

```
TriageResult (already computed)
├── QuickRef            → counts, progress %
├── Recommendations     → top picks, next steps
└── BlockersToClear     → risks (with dependents_count from graph)
    (no ProjectHealth field — that does not exist in TriageResult)

GraphMetrics (already computed)
├── critical_depth: HashMap<String, usize>  → per-node depth in dependency DAG
├── articulation_points: HashSet<String>    → bottleneck highlights
├── blocks_count: HashMap<String, usize>    → per-node dependent count
└── (velocity not here — derived from forecast)

Insights (already computed via analyzer.insights())
├── critical_path: Vec<String>   → nodes sorted by critical_depth, top 20
│     NOTE: this is NOT a single linear chain — it is all high-depth
│     nodes sorted descending. Summing costs across this list is NOT
│     "minimum cost to finish". A true critical-chain primitive does
│     not yet exist in IssueGraph. See Economics section for v1 scope.
├── bottlenecks: Vec<InsightItem> → top 15 by blocks_count + score
└── articulation_points: Vec<String> → same data, Vec form

RobotAlertsOutput (separate analyzer pass: analyzer.alerts())
├── alerts: Vec<Alert>   → alert_type (NewCycle|StaleIssue|BlockingCascade)
└── summary: AlertSummary → total, critical, warning, info counts
    Needed for FlowDistribution (Risk classification) and
    UrgencyProfile (Expedite classification)

RobotSuggestOutput (separate analyzer pass: analyzer.suggest())
└── suggestions: SuggestionSet → types: CycleWarning, StaleCleanup, etc.
    Needed for FlowDistribution (Debt classification) and
    UrgencyProfile (Intangible classification)

ForecastOutput (already computed)
├── summary: ForecastSummary            → count, avg_eta_minutes
└── forecasts: Vec<ForecastItem>        → per-item velocity_minutes_per_day

Global velocity derived at AudienceView construction time:
  median of ForecastItem.velocity_minutes_per_day across open items.
  Velocity is per-issue in forecast (label-aware with fallback chain),
  not stored centrally on Analyzer — median of forecast items is the
  correct derivation.

                    ↓ (new: projection + classification)

EconomicsProjection (when overlay.economics configured)
├── burn_rate           → derived_velocity × hours_per_day × hourly_rate
├── cost_to_complete    → (open_count / derived_velocity) × daily_rate
├── cost_of_delay[]     → per blocker: dependents_count × daily_rate (rate, not total)
└── budget_utilization  → closed_cost / budget_envelope
                          closed_cost = closed_count × median_minutes × hourly_rate/60
    (critical_path_cost deferred to v2 — requires a true longest-chain
    primitive that IssueGraph does not yet provide. See Economics section.)

FlowDistribution (from &[Issue] + &RobotAlertsOutput + &RobotSuggestOutput)
Classification is primary-type-first, each issue counted once.
Priority order: Risk > Debt > Defects > Features.
├── risk                → items with critical/warning alerts
├── debt                → items flagged by suggest (cycles, orphans, stale)
│                         AND not already classified as risk
├── defects             → issue_type == "bug" AND not above
└── features            → everything else (feature, task, chore, epic, unknown)
Percentages always sum to 100%.

UrgencyProfile (from &[Issue] + &RobotAlertsOutput + &RobotSuggestOutput)
├── expedite            → priority == 1 AND (blocker OR critical alert)
├── fixed_date          → has due_date within planning horizon
├── standard            → normal priority, no blockers
└── intangible          → tech debt: cycles, orphans, stale suggestions

                    ↓ (audience lens selection)

AudienceView (owned struct, no lifetime parameters)
Constructor takes: &[Issue], &TriageResult, &GraphMetrics,
  &ForecastOutput, &RobotAlertsOutput, &RobotSuggestOutput,
  &OverlayConfig
├── dimensions          → progress, health, economics, risk, flow
├── urgency_profile     → classified items per Reinertsen
├── audience            → which lens to emphasize
├── locale              → which strings to use
├── content             → titles, thesis, disclaimer (HTML-escaped)
├── branding            → optional accent_color, footer
├── has_app             → true for engineer/owner, false for investor
└── meta                → generated_at, data_hash, version

                    ↓ (template rendering)

Shell HTML    (executive.html or index.html) — self-contained, email-safe
Dashboard HTML (dashboard/)                  — CSS-only visualizations
```

## Overlay config (.bv/audience.yaml)

Parsed into typed `OverlayConfig` struct with `#[serde(deny_unknown_fields)]`.
Unknown keys cause immediate error (catches typos).

```yaml
audience: owner        # can override CLI --audience
locale: uk             # can override CLI --locale

content:
  title: "Project Status"
  subtitle: "Q1 2026"
  thesis: "On track with controlled tech debt."
  disclaimer: "Projections based on trailing 30-day velocity."

branding:              # optional, omit for neutral defaults
  accent_color: "#2563eb"   # applied as CSS --accent custom property
  footer: "Confidential — Acme Corp"

economics:             # optional, omit to hide economics dimension
  hourly_rate: 85
  hours_per_day: 6
  budget_envelope: 50000    # optional cap
```

### Overlay validation (fail-fast with BvrError::InvalidArgument)

- `hourly_rate` must be > 0
- `hours_per_day` must be > 0 and <= 24
- `budget_envelope` must be >= 0 (if provided)
- `audience` must be valid enum variant
- `locale` must be a known bundle or emit fallback warning
- All `content` strings HTML-escaped before template injection

## Economics projections (when configured)

| Metric | Formula | Source | Audience use |
|--------|---------|-------|--------------|
| Burn rate ($/week) | `derived_velocity × hours_per_day × hourly_rate` | forecast-derived + config | All: "how fast are we spending?" |
| Cost to complete | `(open_count / derived_velocity) × daily_rate` | triage + config | Owner/Investor: "how much more?" |
| Cost-of-delay per blocker | `dependents_count × daily_rate` | graph blocks_count + config | All: reframes blockers as $/day |
| Budget utilization | `closed_cost / budget_envelope` | derived + config | Owner/Investor: "are we on budget?" |

**Deferred to v2: Critical path cost.** The current `Insights.critical_path`
is nodes sorted by `GraphMetrics.critical_depth` (top 20, truncated).
This is NOT a single linear chain through the DAG — it includes nodes
from different branches at similar depth. Summing costs across this list
does not yield "minimum cost to finish." A meaningful critical-path-cost
metric requires a true longest-chain primitive (trace back from max-depth
node through predecessors) which `IssueGraph` does not yet provide.

Derived values:
- `derived_velocity`: median of `ForecastItem.velocity_minutes_per_day`
  across open items
- `closed_cost`: `closed_count × median_estimated_minutes × hourly_rate / 60`
- `daily_rate`: `hours_per_day × hourly_rate`
- Cost-of-delay is expressed as a **rate** ($/day this blocker remains open),
  not a total — avoids needing `avg_block_days` which has no reliable source

Rules:
- Zero velocity → "insufficient data", not NaN/Infinity
- No hourly_rate → economics dimension omitted entirely
- Negative/zero validation → fail-fast at overlay parse
- All projections display disclaimer from content
- Per-blocker cost-of-delay uses already-computed `blocks_count` from
  GraphMetrics — no additional graph traversal needed

## Flow Distribution (from existing data)

Classifies all open issues into Kersten's four flow types using data
bvr already has. Classification is primary-type-first: each issue
counted exactly once. Priority order prevents overlap:

| Priority | Flow type | Source | Purpose |
|----------|-----------|--------|---------|
| 1st | **Risk** | Items with critical/warning alerts | Risk mitigation capacity |
| 2nd | **Debt** | Items flagged by `suggest` (cycles, orphans, stale) AND not Risk | Sustainability investment |
| 3rd | **Defects** | `issue_type == "bug"` AND not Risk or Debt | Quality investment |
| 4th | **Features** | Everything else (feature, task, chore, epic, unknown) | Value delivery capacity |

Percentages always sum to 100%. Displayed as CSS percentage bars.
No new data collection — pure classification of existing data.

Audience interpretation:
- Engineer: "40% features, 25% bugs, 20% debt, 15% risk — debt growing"
- Owner: "Balanced allocation, debt under control"
- Investor: "60% value delivery, 20% quality, 20% sustainability"

## Urgency Profiles (Reinertsen mapping)

Maps existing bvr data to Don Reinertsen's four urgency archetypes.
Prevents "everything is urgent when you add dollar signs":

| Profile | bvr mapping | Effect |
|---------|------------|--------|
| **Expedite** | priority == 1 AND (blocker OR critical alert) | Immediate attention across all audiences |
| **Fixed-Date** | has `due_date` within planning horizon | Milestone risk for owner, deadline cost for investor |
| **Standard** | normal priority, no blockers | Scheduled work, planned burn |
| **Intangible** | suggestions: cycles, orphans, stale items | Tech debt — low urgency now, high risk later |

This is a ~30 LOC classification function, not a new analysis module.
It gives tech debt a legitimate place in the priority conversation
alongside financial urgency.

## Implementation steps

### Step 0: Conformance safety net

Files: `tests/`

- Add test: `--export-pages` without `--audience` produces functionally
  identical output to current behavior:
  - Same file tree structure (exact same paths)
  - Same JSON content (ordering-invariant, using existing test_utils)
  - Same binary assets (viewer JS/CSS/WASM — byte-identical)
  - Timestamps excluded from comparison (generated_at varies)
- This runs BEFORE any code changes as a regression guard
- Validates that the entire audience feature is additive-only

### Step 1: Overlay config + typed structs + locale bundles

Files: `src/audience/mod.rs` (new module), `src/audience/en.yaml`,
`src/audience/uk.yaml`

- Define typed structs: `OverlayConfig`, `ContentConfig`, `EconomicsConfig`,
  `BrandingConfig`, `Audience` enum
- `#[serde(deny_unknown_fields)]` on all config structs
- Validation: hourly_rate > 0, hours_per_day in (0, 24], budget >= 0
- `html_escape()` pure function (~10 LOC: & < > " ' replacement)
- Parse overlay YAML via serde_yaml (already a dependency)
- Locale bundles: key-value YAML pairs embedded via `include_str!`,
  deserialized into typed `LocaleBundle` struct (missing key = shape
  error at deserialization, caught by mandatory unit tests — not a
  true compile-time guarantee since `include_str!` + serde_yaml
  deserialization happens at runtime)
- Fallback: unknown locale → use `en` with warning to stderr
- `{placeholder}` interpolation for dynamic values in locale strings
- Inline `#[cfg(test)]` unit tests for parsing, validation, escaping,
  locale completeness, interpolation

### Step 2: Economics projection + Flow Distribution + Urgency

Files: `src/audience/mod.rs` (continues Step 1 module)

- Velocity source (decided): median of `ForecastItem.velocity_minutes_per_day`
  across open items from `analyzer.forecast("all", None, 1)`. Velocity is
  not stored centrally on Analyzer — it is per-issue in forecast (label-aware
  with fallback chain). Median is the correct project-level derivation.
- `EconomicsProjection` struct + `project()` pure function
- All formulas from the economics table above
- Zero-velocity guard, budget-exceeded detection
- `FlowDistribution` struct + `classify()` function (~30 LOC)
  Priority-based classification: Risk > Debt > Defects > Features
  Each issue counted once, percentages sum to 100%
- `UrgencyProfile` struct + `classify()` function (~30 LOC)
  Maps priority + due_date + alerts + suggestions to Reinertsen archetypes
- `AudienceView` owned struct — constructor extracts values from
  `&[Issue]`, `&TriageResult`, `&GraphMetrics`, `&ForecastOutput`,
  `&RobotAlertsOutput`, `&RobotSuggestOutput`, `&OverlayConfig`
  (alerts and suggestions are separate `analyzer.alerts()` /
  `analyzer.suggest()` passes — not part of TriageResult)
- `has_app` field: true for engineer/owner, false for investor
- Derive `Serialize` for robot-mode JSON output
- Inline `#[cfg(test)]` unit tests: economics math, flow classification,
  urgency mapping, edge cases (zero velocity, no estimates, empty graph,
  all issues one type, budget exceeded)

### Step 3: CLI flags + robot economics

Files: `src/cli.rs`, `src/main.rs`, `src/robot.rs`

- Add `--audience`, `--locale`, `--overlay` clap flags (Option types)
- Add `--robot-economics` flag
- Flag validation (see "Flag validation rules" above):
  - `--audience` requires `--export-pages`
  - `--robot-economics` requires `--overlay` with economics section
  - `--robot-economics` independent of `--audience` and `--export-pages`
- `--robot-economics` path: construct EconomicsProjection, serialize as
  RobotEnvelope JSON (follows existing robot.rs pattern)
- Wire `--audience` into `--export-pages` code path
- **Robot command registration** (required for all new `--robot-*` flags):
  - `Cli::is_robot_command()` — add `|| self.robot_economics`
  - `implemented_robot_command_names()` — add `"robot-economics"`
  - `generate_robot_docs()` — add docs entry with description + examples
  - `generate_robot_schemas()` — add JSON schema for output shape
  - Existing tests (`robot_docs_commands_lists_all_robot_commands`,
    `robot_docs_and_schema_command_sets_match`) enforce completeness —
    they will fail if any of these are missed

### Step 4: Templates — shell + dashboard

Files: `src/audience/templates.rs` (new), `src/audience/shell.html`,
`src/audience/dashboard.html`

Template rendering approach:
- Templates loaded via `include_str!()`
- Placeholders use `<!-- BVR:key -->` syntax (HTML comments, invisible
  if template opened raw in browser)
- Simple `render()` function replaces placeholders with values (~15 LOC)
- CSS braces `{}` remain untouched — no escaping conflicts
- `html_escape()` applied to all user-provided values before replacement
- No new dependency

Executive summary (shell.html):
- Self-contained HTML: all CSS inline in `<style>` tag
- No external resources (stylesheets, fonts, images, scripts)
- System font stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`
- Valid HTML5 document with `lang` attribute from locale
- Sections: header, KPI cards (5 dimensions), risks, flow distribution,
  urgency breakdown, next steps
- Deep links rendered conditionally via `has_app`:
  - engineer/owner: links to `./index.html` (full SPA) and `./dashboard/`
  - investor: links to `./dashboard/` only (no SPA generated)
- `@media print` CSS (~20 lines): hide nav, optimize fonts, page-break control
- Grayscale-safe: status uses text labels + shapes, not just color
- All user content (title, thesis, etc.) injected via html_escape()
- Target: <15KB, opens in Gmail/Outlook/browser/filesystem
- Integrity footer: "Generated {date} · v{version} · data:{hash}"

Dashboard (dashboard.html):
- CSS-only visualizations, zero JavaScript
- Progress bars via CSS `width: calc(N%)`
- Flow distribution via CSS percentage bars
- KPI grid via CSS Grid layout
- Track breakdown via HTML `<table>` with styled cells
- Velocity shown as single KPI number with context:
  "3.2 beads/week" with comparison to required pace for deadline
  (e.g. "need 4.1/week to meet due date" — from open_count,
  velocity, nearest due_date). Future: sparkline trend when bvr
  gains snapshot history.
- Economics section (when configured): burn rate, cost-to-complete,
  per-blocker cost-of-delay rate, budget gauge
- `@media print` CSS: single-column flow, economics on own page
- Branding: `--accent` CSS custom property from overlay, optional footer
- Deep links conditional on `has_app`:
  links to SPA (`./index.html`) only when `has_app: true`
- Target: <30KB including inline styles

Both templates:
- `<meta name="bvr:generated-at">`, `<meta name="bvr:data-hash">`,
  `<meta name="bvr:version">` for integrity verification
- Audience lens applied via locale strings and section emphasis,
  not by hiding sections

### Step 5: Export integration

Files: `src/export_pages.rs` (modify), `src/export_md.rs` (modify),
`src/pages_wizard.rs` (modify), `src/main.rs` (modify)

**Integration strategy — preserve existing test surface:**

`ExportPagesOptions` is constructed in 20+ test sites and tested by 35
passing export tests. Adding fields to it would break all construction
sites. Instead:
- Add `audience: Option<&AudienceExportConfig>` as a **separate parameter**
  to `export_pages_bundle()`, not a field on `ExportPagesOptions`
- When `audience` is `None`, the function is byte-identical to current
  behavior — no conditional branches touched
- The 3 call sites in `main.rs` pass `audience_config.as_ref()` or `None`
- Existing test calls add `, None` as 4th argument (mechanical, no logic
  change) — but the `ExportPagesOptions` struct itself stays untouched

**When audience is Some:**
- SPA (`index.html`, `vendor/`, `assets/`) stays at root untouched
  when `has_app: true` (engineer/owner); skipped entirely when
  `has_app: false` (investor)
- Generate executive summary:
  - `executive.html` when `has_app: true` (SPA occupies `index.html`)
  - `index.html` when `has_app: false` (no SPA to conflict with)
- Generate `dashboard/index.html` from `AudienceView` + template
- Deep links in executive/dashboard conditional on `has_app`
- `data/`, `beads.sqlite3` always written regardless of audience

**Hook environment extension** (in `build_hook_env` via `HookContext`):
- `$BV_AUDIENCE` — "owner" | "investor" | "engineer"
- `$BV_LOCALE` — "en" | "uk" | ...
- `$BV_EXPORT_SHELL` — absolute path to executive summary (executive.html
  or index.html depending on has_app)
- `$BV_EXPORT_DASHBOARD` — absolute path to dashboard/index.html
- `$BV_EXPORT_APP` — absolute path to SPA index.html (empty if investor)

Preview server (`--preview-pages`) serves audience pages alongside SPA

**Watch mode extension** (in `main.rs` watch loop):

The current watch loop monitors only issue source files (beads JSONL,
workspace config). When `--audience` + `--overlay` are active, the
overlay file must also trigger re-export:
- Add `overlay_path` to the set of watched files (alongside issue sources)
- On overlay mtime change: reload overlay config, rebuild AudienceView,
  regenerate audience pages (executive + dashboard)
- Issue source changes still regenerate everything (including audience
  pages, since triage/forecast data changed)
- Locale YAML files are embedded via `include_str!` — they are
  compile-time constants and do not need runtime watching

**Pages wizard audience awareness** (`src/pages_wizard.rs`):

The `--pages` wizard is a real user-facing export surface. Without
audience awareness, audience export would be CLI-only. The wizard
should reflect audience mode when active:
- Config summary and operator guidance should show active audience,
  locale, and overlay inputs (or their defaults/absence)
- Export callback should pass audience config through to
  `export_pages_bundle()` (same 4th parameter as CLI path)
  from the interactive `main.rs` wizard flow, not only the direct CLI path
- Non-interactive `--pages` help / guidance output should mention
  audience export support and any wizard limits, so operators are not
  told only the legacy export story
- If advanced audience inputs (overlay, locale) are not wizard-
  configurable in v1, the wizard must say so explicitly rather than
  silently ignoring them

### Step 6: Tests

- **Conformance** (Step 0): `--export-pages` without `--audience` =
  functionally identical (file tree + JSON content + binary assets,
  timestamps excluded)
- **Unit** (inline in module):
  - Overlay parsing: valid, invalid, missing fields, unknown fields
  - Economics math: burn_rate, cost_to_complete, cost_of_delay, budget_utilization,
    zero velocity, no estimates, budget exceeded
  - Flow distribution: mixed issue types, empty input, all one type,
    overlapping categories (issue is both bug AND has alert → Risk wins)
  - Urgency profiles: all four archetypes, edge cases
  - HTML escaping: all five characters, nested, empty string
  - Locale: both bundles complete, interpolation, fallback
  - Placeholder rendering: CSS braces preserved, nested placeholders,
    missing placeholder key
- **E2E**:
  - `--export-pages --audience owner` → SPA at `index.html`, executive at
    `executive.html`, `dashboard/index.html` present
  - `--export-pages --audience investor` → executive at `index.html`,
    no SPA (no `vendor/`, no `assets/`), no `executive.html`
  - `--export-pages --audience investor --overlay` → economics present,
    no broken links to SPA
  - `--export-pages --audience engineer` → full detail level
  - `--robot-economics --overlay` → valid JSON envelope
  - `--robot-economics` without overlay → clear error message
  - `--audience` without `--export-pages` → clear error message
  - `--pages` non-TTY help / operator guidance is audience-aware enough
    that it does not imply audience export is unavailable or silently
    wizard-configured when it is not
  - Interactive wizard path passes audience config through to export,
    or explicitly surfaces v1 defaults / manual-only inputs in config
    preview and operator messaging
- **Snapshot** (insta):
  - shell.html content for each audience × locale combination
  - dashboard.html content for each audience
- **Shell E2E / operator logging**:
  - Extend `scripts/e2e_preview_pages.sh` or add an adjacent audience
    script that captures stdout/stderr, preview status JSON, watch
    regeneration events, wizard/operator transcript, and preserved
    artifact paths on failure
  - Logs should identify which stage failed (export, preview, watch,
    wizard/help, robot-economics) without requiring a future agent to
    reread implementation code

## Module structure

New submodule justified per AGENTS.md: audience export is genuinely new
functionality (economics projections, flow distribution, urgency profiles,
locale handling, HTML template rendering) that has zero overlap with any
existing file. Follows the `src/analysis/` submodule precedent.

Locale logic (bundle loading, interpolation, fallback) lives in `mod.rs`
rather than a separate file — at ~80 LOC it does not clear the "incredibly
high bar" for new files (AGENTS.md).

```
src/audience/
├── mod.rs              # OverlayConfig, AudienceView, EconomicsProjection,
│                       # FlowDistribution, UrgencyProfile, html_escape(),
│                       # LocaleBundle, load/interpolate, validation,
│                       # constructors, inline tests
├── templates.rs        # render(), render_shell(), render_dashboard(),
│                       # placeholder replacement engine, inline tests
├── shell.html          # HTML template with <!-- BVR:key --> placeholders
├── dashboard.html      # HTML template with <!-- BVR:key --> placeholders
├── en.yaml             # English locale strings (include_str!)
└── uk.yaml             # Ukrainian locale strings (include_str!)
```

2 Rust source files + 4 embedded assets. All tests inline (`#[cfg(test)]`).

## Files touched (estimated)

| File | Change | Lines (est.) |
|------|--------|-------------|
| `src/audience/mod.rs` | NEW: structs, economics, flow, urgency, validation, locales | +400 |
| `src/audience/templates.rs` | NEW: placeholder engine + shell/dashboard renderers | +270 |
| `src/audience/shell.html` | NEW: executive summary template | +100 |
| `src/audience/dashboard.html` | NEW: dashboard template | +150 |
| `src/audience/en.yaml` | NEW: English strings | +40 |
| `src/audience/uk.yaml` | NEW: Ukrainian strings | +40 |
| `src/cli.rs` | Add 4 flags + validation rules | +30 |
| `src/main.rs` | Wire audience + robot-economics dispatch | +80 |
| `src/robot.rs` | `implemented_robot_command_names`, docs, schema entries | +40 |
| `src/export_pages.rs` | 4th param to bundle fn, audience branch, routing | +100 |
| `src/export_md.rs` | Extend `HookContext` + `build_hook_env` for audience vars | +20 |
| `src/pages_wizard.rs` | Audience awareness in config summary + operator messaging | +30 |
| `scripts/e2e_preview_pages.sh` | Extend preview/watch/pages operator script with audience scenarios + artifact logging | +80 |
| `src/lib.rs` | `pub mod audience;` | +1 |
| `tests/` | Conformance + e2e + snapshot + mechanical `, None` additions | +280 |
| **Total** | | **~1660** |

## Constraints

- `unsafe_code = "forbid"` — no unsafe
- All builds via `rch exec -- cargo ...` with `export TMPDIR=/data/tmp`
- Existing 1318 tests must pass (especially conformance)
- `--export-pages` without `--audience` = zero behavior change
- **35 existing export tests are stable** — `ExportPagesOptions` struct
  must not gain new fields; audience config passed as separate parameter
- Release profile: opt-level="z", LTO, stripped
- No new dependencies (serde_yaml already present)
- New files only for genuinely new functionality (AGENTS.md rule);
  locale logic (80 LOC) merged into `mod.rs`, not a separate file
- Code changes manually, no script-based edits (AGENTS.md rule)
- Every module includes inline `#[cfg(test)]` unit tests (AGENTS.md rule)
