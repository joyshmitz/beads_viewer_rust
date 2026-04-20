# Audience Export — Implementation Plan

Multi-dimensional project views for `--export-pages`, with audience-specific
lenses and optional economics projection.

## Why this exists

bvr's `--export-pages` already exposes the structure of project execution: dependency graphs, triage recommendations, progress signals, and other engineering views of the work graph.

What it does not yet expose is how that same project state should be interpreted by different decision-makers.

Engineers need to see execution topology: what is blocked, what unblocks the most downstream work, where the dependency structure is fragile, and what should be worked next. Owners need to see delivery posture: how much progress is real, which risks threaten milestones, where capacity is being consumed, and whether the project is converging or drifting. Financial stakeholders need to see economic pressure: what current delivery is costing, what the remaining work is likely to cost at current throughput, and where blocked or low-leverage work is burning time and budget.

Today these views are often split across different tools. The work graph lives in the issue tracker and in `bvr`, while cost, burn, budget pressure, and completion projections often end up in separate spreadsheets or slideware. Once that happens, the interpretations drift from the underlying project state. A task closes, a blocker clears, a dependency shifts, throughput changes, and the financial view is immediately stale unless someone manually reconciles it.

This feature exists to eliminate that split.

Audience export does not introduce a second source of truth. It keeps the live work graph as the source and applies audience-specific lenses to that same underlying state. The goal is not to build a parallel planning system or a lightweight accounting package. The goal is to make one project state legible to different audiences without forking the data model, duplicating interpretation logic, or maintaining a separate reporting artifact that decays on contact with reality.

The economics layer is one expression of that principle. It is not a standalone financial model detached from execution. It is a projection over metrics `bvr` already computes: throughput, blocked pressure, dependency structure, progress trajectory, and related graph signals. That makes the economic view operationally grounded. Cost is not being inferred from a narrative assembled after the fact; it is being read from the same system state that engineers and operators are already using to manage the work.

For AI-agent workflows, this matters because agents do not just need a ranking of what is important in the abstract. They need to understand what matters for the current lens: what is the next execution move, what most threatens delivery confidence, what blocked subgraph is consuming disproportionate effort, and where additional work is likely to create the highest system-level payoff. That requires a single graph with multiple interpretations, not multiple disconnected summaries.

## Design principle: one source, three lenses

All audiences read from the same underlying project facts. The audience flag controls interpretation, wording, default surface set, and level of aggregation; it does not create audience-specific facts or a second source of truth.

Terminology is strict in this document: a **lens** is a conceptual interpretation of shared project facts; a **mode** is a concrete CLI/audience value such as `engineer`, `owner`, or `investor`; a **surface** is an emitted artifact such as the SPA, `executive.html`, or `dashboard/index.html`.

| Dimension | Engineer lens | Owner lens | Financial lens (`investor` mode) |
|-----------|--------------|------------|---------------|
| **Progress** | Actionable items, tracks, WIP | % complete, milestone ETA, explicit milestone-pressure aggregates | Delivery predictability, schedule variance |
| **Technical Health** | Cycles, bottlenecks, graph detail | Aggregated risk count | Cost impact of technical issues |
| **Economics** | Cost-per-bead, burn rate | Burn rate, cost-to-complete | Budget utilization, projections* |
| **Risk** | Per-bead: staleness, orphans, alerts | "3 blockers threaten Q2 milestone" | "Blocked work costs $4,200/week" |
| **Flow Distribution** | Feature/Bug/Debt/Risk capacity split | Balance assessment via aggregated flow mix | Value delivery mix by count |

*Omitted when `overlay.economics` is absent.

## What this adds

When `--audience` is enabled, `--export-pages` does not generate a different project model. It generates additional audience-specific surfaces derived from the same export state.

The underlying analysis does not fork. The same issue graph, the same derived metrics, and the same export-time project state continue to drive every output. What changes is the presentation layer: which surface is exposed at the top level, how much detail is shown by default, and whether the export includes the full interactive application or only static audience-oriented views.

This distinction matters.

The audience feature is not a second reporting pipeline bolted onto `bvr`. It is a routing and rendering layer on top of the existing export pipeline. The full SPA remains the canonical high-detail surface when the selected audience still benefits from direct access to the live export bundle. Additional static pages are generated to provide audience-specific summaries that are easier to share, easier to print, and easier to consume without requiring the reader to navigate the engineering-oriented application.

That produces two export shapes.

For `engineer` and `owner`, the existing SPA remains at the root because it is still a valid and useful surface for those readers. In these modes, audience export adds a lightweight executive summary and a static dashboard alongside the app. The app remains intact; the audience pages are additive.

For `investor`, the export shifts the default surface. The root page becomes the executive summary and the dashboard remains available as a secondary static view. This mode applies the financial lens to the same shared export state. The full SPA and its supporting runtime artifacts are not emitted because the intended reader is not being handed the engineering console. This is not a change in project truth. It is a deliberate narrowing of presentation surface.

In other words: the same source state can be exported with different default entry points, different detail levels, and different artifact bundles without becoming different reports in the semantic sense. The audience lens changes exposure, not facts.

Concretely, the audience export adds two new surfaces:

- An executive summary page: a compact, self-contained summary optimized for direct reading, sharing, printing, and low-friction review.
- A dashboard page: a static, CSS-only visual summary that presents the same project state through audience-appropriate emphasis.

These surfaces are not substitutes for the underlying export state. They are controlled views over it. In `engineer` and `owner` modes, they sit next to the SPA. In `investor` mode, they become the visible export because that mode is the financial-facing mode and intentionally suppresses the engineering application and its runtime payload.

Without `--audience`, export behavior remains unchanged. The existing `--export-pages` output stays exactly as it is today. Audience export is therefore an additive interpretation layer over the same export machinery, not a replacement for the existing path.

When `--audience` is set, `--export-pages` generates these additional surfaces alongside the existing SPA layout, or promotes them to the top level when the chosen audience does not need the SPA:

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
# (no SPA, no vendor/, no assets/, no data/, no beads.sqlite3 — the
# executive summary + CSS-only dashboard have no runtime data consumer)
```

The SPA is never relocated. Its JS fetches `data/*` and `beads.sqlite3` via relative paths (`fetchJson("data/meta.json")` etc.), so moving it to a subdirectory would break all data loading. Where the app is present, it stays where it already works. Where the app is absent, it is omitted intentionally rather than partially preserved in a broken or misleading form.

That is the construction rule in practice: one export-time project state, multiple audience surfaces, no divergence in underlying meaning.

## CLI surface

The CLI follows the same construction rule as the export itself: it does not select different sources of truth. It selects how one underlying export state is rendered, routed, and optionally enriched with overlay data.

The existing `--export-pages` path remains the base path. Audience export is not a separate command because it is not a separate pipeline. It is an audience-sensitive mode of the same export operation.

The flag surface therefore has three jobs:

- select the lens to apply to the export;
- select optional localization and overlay inputs that influence presentation and economics projection;
- expose the economics projection directly in robot mode when HTML output is not wanted.

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

This split is intentional. `--audience` belongs to the pages export path because it changes which audience-facing surfaces are emitted from the existing export pipeline. `--robot-economics` exists separately because economics is also a machine-consumable projection over the same project state and should be available without requiring an HTML export.

### New flags

| Flag | Type | Default | Purpose |
|------|------|---------|---------|
| `--audience` | `owner\|investor\|engineer` | none | Selects audience lens for export |
| `--locale` | string | `en` | UI string localization for audience export or robot economics (e.g. `uk`, `de`) |
| `--overlay` | path | none | YAML config for content/economics/branding for audience export or robot economics |
| `--robot-economics` | bool | false | Emit EconomicsProjection as JSON envelope |

### Flag validation rules

Flag validation exists to preserve the architectural boundary described above. The CLI should make it difficult to invoke an audience lens in a context where there is no audience surface, and impossible to request an economics projection where no economics inputs exist.

- `--audience` requires `--export-pages` because audience selection is a pages-export concern, not a global rendering toggle.
- `--robot-economics` requires `--overlay` with an `economics` section because there is no defensible economics projection without explicit economics inputs.
- `--robot-economics` does not require `--audience` or `--export-pages` because it exposes the same projection in machine-readable form without going through the HTML path.
- `--locale` and `--overlay` can be used with either `--audience` or `--robot-economics` because both page rendering and robot economics depend on the same shared input discipline.
- `--locale` without `--audience` and without `--robot-economics` is rejected with a clear error because locale selection has no effect on the legacy export path.
- `--overlay` without `--audience` and without `--robot-economics` is rejected with a clear error because overlay data must feed either audience rendering or economics projection; it is not accepted as an ignored side input.
- `--audience` without `--export-pages` fails with a clear error: "use --export-pages with --audience, or --robot-economics for JSON".
- `--audience` with `--export-md` is rejected in v1 with a clear error: "audience export currently supports --export-pages only; markdown audience export is not implemented".

### Audience matrix

The matrix below is a surface-bundle and aggregation matrix, not a data-model matrix. All audiences still read from the same project state. The cells describe which surfaces are emitted and how aggressively the shared facts are aggregated for default presentation, not whether a separate truth exists for that audience.

| Surface | (none) | engineer | owner | investor mode (`financial` lens) |
|---------|--------|----------|-------|----------|
| Full SPA | root `index.html` | root `index.html` | root `index.html` | — (not generated) |
| Executive summary | — | `executive.html` (technical framing) | `executive.html` (delivery framing) | root `index.html` (financial framing) |
| Dashboard | — | low aggregation | medium aggregation | high aggregation |
| Economics depth | — | per-bead cost | burn rate + budget | full projections |
| Flow Distribution | — | capacity split | balance assessment | value delivery mix by count |

## Data flow

No new data collection is introduced for audience export. The audience layer reads from analysis `bvr` already computes at export time and derives presentation-ready views from that shared state.

This section matters because it is where the document either proves or fails to prove its central claim. If the audience feature required a separate extraction path, a separate project model, or audience-specific preprocessing, then "one source, three lenses" would be false. The data flow below shows the opposite: engineer, owner, and financial views all start from the same runtime analysis outputs and diverge only at the projection and rendering stages.

The engineer lens reads this state as execution structure. The owner lens reads the same state as delivery posture, milestone pressure, and balance of effort. The financial lens reads the same state as burn, cost-to-complete, and cost-of-delay. The point is not that each audience gets different facts. The point is that each audience gets a different interpretation of the same facts.

Economics is therefore a projection layer over existing metrics plus overlay configuration, not a new analytical pipeline:

```
TriageResult (already computed)
├── QuickRef            → counts, progress %
├── Recommendations     → candidate pool for representative_items (see below)
├── BlockersToClear     → risks (with dependents_count from graph)
└── ProjectHealth       → counts, graph stats, velocity (weekly closures)

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

(top_k unlock coverage is NOT consumed from Insights directly by
 audience; it flows in via RobotOverviewOutput.unlock_maximizers,
 which upstream builds from `analyzer.top_k_unlock_set(10)`.
 See the Resolved dependency section for details.)

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

Trailing throughput derived at AudienceView construction time: `closed_issues_trailing_window / throughput_window_days`, using `Issue.closed_at`, where `throughput_window_days = min(30.0, project_age_days)` and `project_age_days` is derived from the earliest observed project timestamp available to the export path. Forecast per-item `velocity_minutes_per_day` is explicitly NOT reused for economics; it is an ETA aid for individual issues, not a team or project throughput metric.

                    ↓ (new: projection + classification)

EconomicsProjection (when overlay.economics configured)
├── burn_rate           → daily_rate = hours_per_day × hourly_rate
├── cost_to_complete    → (open_count / throughput_issues_per_day) × burn_rate
├── cost_of_delay[]     → per blocker: dependents_count × daily_rate (rate, not total)
└── budget_utilization  → closed_cost / budget_envelope
                          closed_cost = closed_count × median_estimated_minutes × hourly_rate/60
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
Classification is priority-ordered and each issue is assigned exactly one urgency bucket. Priority order: Expedite > Fixed-Date > Intangible > Standard.
├── expedite            → priority == 1 AND (blocker OR critical alert)
├── fixed_date          → has due_date within planning horizon
├── intangible          → tech debt: cycles, orphans, stale suggestions
└── standard            → normal priority, no blockers, and not classified above

                    ↓ (shared project state interpreted through audience lens)

AudienceView (owned struct, no lifetime parameters)
Constructor takes: &[Issue], &TriageResult, &GraphMetrics,
  &ForecastOutput, &RobotAlertsOutput, &RobotSuggestOutput,
  &OverlayConfig, &Insights
├── dimensions          → progress, health, economics, risk, flow
├── urgency_profile     → classified items per Reinertsen
├── representative_items → diverse selection for "next steps" (see note below)
├── audience            → which lens to emphasize
├── locale              → which strings to use
├── content             → titles, thesis, disclaimer (HTML-escaped)
├── branding            → optional accent_color, footer
├── has_app             → true for engineer/owner, false for investor mode (`financial` lens)
└── meta                → generated_at, data_hash, version

At this stage the underlying state is still shared. `AudienceView` does not create an engineer-only, owner-only, or financial-only dataset. It creates one audience-aware projection object that preserves common facts while controlling emphasis, wording, detail level, and surface routing for the selected reader.

  NOTE on representative_items: upstream `--robot-overview` (issue #4,
  closed 2026-04-06, commits 92e61cd..2abdb95) now merges the two
  candidate pools that matter — triage composite (`top_pick`/`fronts`)
  and greedy-submodular unlock coverage (`unlock_maximizers`) — and
  filters `unlock_maximizers` against ids already surfaced by
  `top_pick`/`fronts`. `representative_items` consumes that payload
  directly: no MMR / submodular selection implemented in audience,
  no triage+insights merge duplicated here. Audience still applies a
  local second-pass de-dup on
  `representative.id` across `fronts` (multi-label recommendations
  can appear as representatives for multiple labels — see Resolved
  dependency section) and enforces an explicit item budget with `MAX_REPRESENTATIVE_ITEMS = 8` (1 `top_pick` + up to 4 unique `fronts` + remaining slots from `unlock_maximizers`, whose upstream pool is capped at 5). The practical range is `0..=8` items because `top_pick`, `fronts`, and `unlock_maximizers` may each be empty. Executive summaries stay in sync with `--robot-orient` automatically.

                    ↓ (template rendering)

Shell HTML    (executive.html or index.html) — self-contained, browser/filesystem-safe
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

The economics dimension is the third lens over the same shared project state. It does not create a separate financial truth for one audience. It gives engineer, owner, and financial readers a cost-oriented interpretation of the same underlying execution graph.

For engineers, that lens answers questions such as: which blocked region of the graph is burning effort with little forward movement, which items appear expensive relative to their graph leverage, and where local execution choices create disproportionate downstream drag. For owners, it translates delivery posture into operating pressure: what the current throughput implies for remaining cost, whether budget consumption is tracking expectation, and where blockers threaten plan credibility. For financial stakeholders, it summarizes the same state in cost terms: burn, cost-to-complete, budget pressure, and rate-based cost-of-delay.

**Semantic choice for v1:** economics uses **trailing throughput in issues/day**, derived from `Issue.closed_at` over a trailing window capped at 30 days and normalized by `throughput_window_days = min(30.0, project_age_days)`. Burn rate is treated as a project-level daily spend parameter from overlay config, not as a function of forecast's per-issue ETA velocity. This keeps units coherent and avoids abusing `ForecastItem.velocity_minutes_per_day`, which is an issue-level ETA aid rather than a project throughput primitive.

| Metric | Units | Formula | Source | Audience use |
|--------|-------|---------|--------|--------------|
| Burn rate | `$/day` | `burn_rate = hours_per_day × hourly_rate` | config | Engineer: "what does one staffed day of current work cost?"; Owner: "what is current operating burn?"; Financial: "what daily spend assumption is the export using?" |
| Cost to complete | `$` | `cost_to_complete = (open_count / throughput_issues_per_day) × burn_rate` | issues + config | Engineer: rough cost implication of current queue shape; Owner: remaining cost at current throughput; Financial: completion projection under current operating assumptions |
| Cost-of-delay per blocker | `$/day` | `dependents_count × burn_rate` | graph blocks_count + config | Engineer: blocked graph drag; Owner: blocker pressure against delivery; Financial: daily economic drag from blocked work |
| Budget utilization | `%` | `closed_cost / budget_envelope` | derived + config | Engineer: context only; Owner: are we burning against plan; Financial: how much envelope is already consumed |

These metrics are intentionally framed as projections, not accounting outputs. They are meant to make the work graph economically legible, not to masquerade as general-ledger truth. That distinction matters for all three lenses: engineers should treat the numbers as execution-sensitive economic signals, owners should treat them as planning pressure indicators, and financial readers should treat them as model-based operational projections tied to the graph state rather than audited financial reporting.

**Deferred to v2: Critical path cost.** The current `Insights.critical_path` is nodes sorted by `GraphMetrics.critical_depth` (top 20, truncated). This is NOT a single linear chain through the DAG — it includes nodes from different branches at similar depth. Summing costs across this list does not yield "minimum cost to finish." A meaningful critical-path-cost metric requires a true longest-chain primitive (trace back from max-depth node through predecessors) which `IssueGraph` does not yet provide.

Derived values:
- `throughput_window_days`: `min(30.0, project_age_days)`
- `closed_issues_trailing_window`: count of issues with `closed_at` in the active throughput window
- `throughput_issues_per_day`: `closed_issues_trailing_window / throughput_window_days`
- `closed_count`: all-time count of closed issues in the current project state; this is intentionally distinct from `closed_issues_trailing_window`
- `median_estimated_minutes`: median of `estimated_minutes` across the estimate sample used for economics; if estimate coverage is below 50% of the relevant issue sample, estimate-based metrics are omitted rather than imputed silently
- `closed_cost`: `closed_count × median_estimated_minutes × hourly_rate / 60`
- `burn_rate`: `hours_per_day × hourly_rate`
- Cost-of-delay is expressed as a **rate** ($/day this blocker remains open), not a total — avoids needing `avg_block_days` which has no reliable source
- `hours_per_day` is interpreted as project-level staffed hours/day for the export, not per-issue forecast capacity

Rules:
- Zero throughput → "insufficient data", not NaN/Infinity
- `project_age_days < 1` or missing project-age signal → "insufficient data" for throughput-derived economics
- If estimate coverage is below 50% of the relevant issue sample, `closed_cost` and `budget_utilization` are omitted with an explicit disclaimer instead of being computed from a non-representative sample
- No hourly_rate → economics dimension omitted entirely
- Negative/zero validation → fail-fast at overlay parse
- All projections display disclaimer from content
- Per-blocker cost-of-delay uses already-computed `blocks_count` from `GraphMetrics` — no additional graph traversal needed

## Flow Distribution (from existing data)

Flow Distribution is another shared projection over the same issue set. It does not create an audience-specific taxonomy. It takes one pool of open work and classifies it into a stable set of flow categories so that engineer, owner, and financial readers can answer different questions about the same allocation of effort.

For engineers, this view shows how current work is distributed across feature delivery, defects, debt reduction, and risk response. For owners, it shows whether effort is balanced in a way that supports delivery credibility rather than merely local busyness. For financial readers, it gives a cost-relevant view of where capacity is being consumed: value delivery, quality correction, sustainability work, or risk containment.

Classification is primary-type-first and each issue is counted exactly once. Priority order prevents overlap and keeps the output interpretable:

| Priority | Flow type | Source | Purpose |
|----------|-----------|--------|---------|
| 1st | **Risk** | Items with critical/warning alerts | Risk mitigation capacity |
| 2nd | **Debt** | Items flagged by `suggest` (cycles, orphans, stale) AND not Risk | Sustainability investment |
| 3rd | **Defects** | `issue_type == "bug"` AND not Risk or Debt | Quality investment |
| 4th | **Features** | Everything else (feature, task, chore, epic, unknown) | Value delivery capacity |

Percentages always sum to 100%. Displayed as CSS percentage bars. No new data collection is required; this is pure classification of existing state.

Audience interpretation:
- Engineer: "40% features, 25% bugs, 20% debt, 15% risk — debt growing"
- Owner: "Balanced allocation, debt under control"
- Financial: "60% value delivery, 20% quality correction, 20% sustainability/risk absorption"

## Urgency Profile Classification (Reinertsen mapping)

Urgency Profiles provide a second shared classification over the same state. The purpose is not to create a competing prioritization system. The purpose is to prevent the audience layer from collapsing into either engineering-local urgency or undifferentiated cost panic.

The same issue set is mapped into Reinertsen-style urgency archetypes so that all three lenses can talk about urgency without pretending that every expensive thing is immediate, or that every technically annoying thing deserves escalation.

For engineers, this separates true interrupt work from standard flow and from long-horizon maintenance burden. For owners, it translates issue state into schedule and milestone pressure. For financial readers, it distinguishes between immediate economic drag, planned operating work, deadline-driven commitment pressure, and latent structural liabilities.

| Profile | bvr mapping | Effect |
|---------|------------|--------|
| **Expedite** | priority == 1 AND (blocker OR critical alert) | Immediate cross-lens attention: execution interruption, delivery threat, economic drag |
| **Fixed-Date** | has `due_date` within planning horizon | Deadline-coupled work: milestone pressure for owner, commitment exposure for financial readers |
| **Intangible** | suggestions: cycles, orphans, stale items | Structural burden: low immediate urgency, high long-run execution and delivery risk |
| **Standard** | normal priority, no blockers, and not classified above | Planned flow work: normal execution queue, expected delivery cadence, planned operating burn |

This is a ~30 LOC classification function, not a new analysis module. Its value is conceptual discipline: it gives tech debt and latent risk a legitimate place in the priority conversation without allowing cost language to flatten every decision into a false emergency.

## Implementation steps

The implementation order must preserve the same invariant described above: one export-time project state, three audience lenses, no divergence in underlying facts. Each step below exists either to protect that invariant or to express it in a new surface.

The sequence is deliberate. Conformance comes first so the existing export path is locked before audience behavior is added. Shared projection and classification logic comes before rendering so templates cannot invent their own semantics. CLI and integration changes come after the projection layer exists so the external surface reflects an already-coherent internal model rather than driving one.

### Step 0: Conformance safety net

Files: `tests/`

- Add test: `--export-pages` without `--audience` produces functionally
  identical output to current behavior:
  - Same file tree structure (exact same paths)
  - Same JSON content (ordering-invariant, using existing test_utils)
  - Same binary assets (viewer JS/CSS/WASM — byte-identical)
  - Excluded field list is explicit: `generated_at` only
  - `data_hash` MUST remain identical across baseline vs audience-disabled
    exports; drift is a regression, not tolerated timestamp noise
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

This step creates the shared projection layer used by all three lenses. It is the point where the audience feature either remains a disciplined interpretation layer or degenerates into audience-specific logic forks. The implementation goal is one `AudienceView` assembled from shared analysis outputs, with economics, flow, and urgency expressed as reusable projections over that common state.

- Throughput source (decided): `closed_issues_trailing_window / throughput_window_days`, derived directly from `Issue.closed_at` and normalized by `throughput_window_days = min(30.0, project_age_days)`. Forecast per-item `velocity_minutes_per_day` is not reused for economics because it is an issue-level ETA aid, not project throughput.
- `EconomicsProjection` struct + `project()` pure function
- All formulas from the economics table above
- Zero-throughput guard, budget-exceeded detection
- `FlowDistribution` struct + `classify()` function (~30 LOC)
  Priority-based classification: Risk > Debt > Defects > Features
  Each issue counted once, percentages sum to 100%
- `UrgencyProfile` struct + `classify()` function (~30 LOC)
  Maps priority + due_date + alerts + suggestions to Reinertsen archetypes with deterministic priority order: Expedite > Fixed-Date > Intangible > Standard
- `AudienceView` owned struct — constructor extracts values from
  `&[Issue]`, `&TriageResult`, `&GraphMetrics`, `&ForecastOutput`,
  `&RobotAlertsOutput`, `&RobotSuggestOutput`, `&RobotOverviewOutput`,
  `&OverlayConfig` (alerts and suggestions are separate
  `analyzer.alerts()` / `analyzer.suggest()` passes — not part of
  `TriageResult`; `RobotOverviewOutput` is built via
  `build_robot_overview_output(issues, analyzer, triage)` which must
  first be moved from `src/main.rs` (the binary crate) into
  `src/robot.rs` alongside `RobotEnvelope` and declared `pub` — a
  `pub` in `main.rs` alone is unreachable from the library crate
  where `src/audience/` lives. See Resolved dependency section.
  `representative_items` is a thin projection of that payload with
  local `representative.id` de-dup across `fronts` plus an explicit
  `MAX_REPRESENTATIVE_ITEMS = 8` budget (`1 + 4 + 3` in the common
  balanced case, practical range `0..=8`) — not a full MMR /
  submodular pass, which upstream already performs)
- `has_app` field: true for engineer/owner, false for investor mode; this controls surface routing for the financial lens only, not analysis inputs or project truth
- Derive `Serialize` for robot-mode JSON output
- Inline `#[cfg(test)]` unit tests: economics math, flow classification,
  urgency mapping, edge cases (zero throughput, no estimates, empty graph,
  all issues one type, budget exceeded)

### Step 3: CLI flags + robot economics

Files: `src/cli.rs`, `src/main.rs`, `src/robot.rs`

This step exposes the projection layer without creating a second pipeline. The CLI must select lens, locale, overlay, and machine-readable economics output while preserving the rule that all of them operate on the same export-time state.

- Add `--audience`, `--locale`, `--overlay` clap flags (Option types)
- Add `--robot-economics` flag
- Flag validation (see "Flag validation rules" above):
  - `--audience` requires `--export-pages`
  - `--audience` with `--export-md` is rejected with a clear error
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

This step turns the shared projection into audience-facing surfaces. Templates must not invent new semantics. Their job is to render different default readings of the same `AudienceView`: execution emphasis for engineers, delivery posture for owners, and economic interpretation for the financial lens.

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
- Lens-specific rendering rules:
  - engineer: keep execution-facing labels, retain per-item specificity in risks/next steps, and prefer graph-local terminology
  - owner: promote milestone-pressure aggregates, risk counts, flow-balance summaries, and deadline pace comparisons over graph-local detail
  - investor mode / financial lens: suppress engineering-local detail, foreground economics and aggregate delivery signals
- **Next steps renders `AudienceView.representative_items`** — NOT
  `TriageResult.recommendations` directly. `representative_items` is
  projected from `RobotOverviewOutput` (triage composite + submodular
  unlock maximizers, de-duplicated), which upstream `--robot-overview`
  / `--robot-orient` already computes. Rendering from raw triage
  would clip high-unlock items like SL-000 on large graphs — see the
  Resolved dependency section for the fixture and regression test.
- Deep links rendered conditionally via `has_app`:
  - engineer/owner: links to `./index.html` (full SPA) and `./dashboard/`
  - investor: links to `./dashboard/` only (no SPA generated)
- `@media print` CSS (~20 lines): hide nav, optimize fonts, page-break control
- Grayscale-safe: status uses text labels + shapes, not just color
- All user content (title, thesis, etc.) injected via html_escape()
- Target: <15KB, opens reliably in browser/filesystem and is copyable into common mail clients such as Gmail web and Outlook without requiring external assets
- Integrity footer: "Generated {date} · v{version} · data:{hash}"

Dashboard (dashboard.html):
- CSS-only visualizations, zero JavaScript
- Progress bars via CSS `width: calc(N%)`
- Flow distribution via CSS percentage bars
- KPI grid via CSS Grid layout
- Track breakdown via HTML `<table>` with styled cells
- Aggregation policy by mode:
  - engineer: expose low-aggregation detail, including track breakdown and direct linkage back to the SPA
  - owner: expose medium aggregation, replacing graph-local detail with milestone pressure, balance-of-effort summaries, and required-vs-actual pace comparisons
  - investor mode / financial lens: expose high aggregation, retaining economics and delivery posture while omitting engineering-local drill-down
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
  `<meta name="bvr:version">` for drift detection and change tracking
- Audience lens applied via locale strings and section emphasis,
  not by hiding sections

### Step 5: Export integration

Files: `src/export_pages.rs` (modify), `src/export_md.rs` (modify),
`src/pages_wizard.rs` (modify), `src/main.rs` (modify)

This step integrates audience export into the existing pages pipeline without breaking the canonical export path. The integration requirement is strict: no new export truth, no relocation hack that changes app semantics, and no silent divergence between direct CLI export, preview, watch mode, and wizard-driven export.

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
- `data/` and `beads.sqlite3` are written only when `has_app: true`
  in v1; investor output prunes them because the executive summary and
  CSS-only dashboard have no runtime data consumer

**Hook environment extension** (in `build_hook_env` via `HookContext`):
- `$BV_AUDIENCE` — "owner" | "investor" | "engineer"
- `$BV_LOCALE` — "en" | "uk" | ...
- `$BV_EXPORT_SHELL` — absolute path to executive summary (executive.html
  or index.html depending on has_app)
- `$BV_EXPORT_DASHBOARD` — absolute path to dashboard/index.html
- `$BV_EXPORT_APP` — absolute path to SPA index.html (empty if investor)

Because `HookContext::new(path, format, count)` is a constructor used by
three existing tests/callers, extending it is API-breaking inside
`export_md.rs`. Budget this as constructor churn plus test updates, or add
an internal builder/helper to avoid a positional-arg explosion.

Preview server (`--preview-pages`) serves audience pages alongside SPA

**Watch mode extension** (in `main.rs` watch loop):

Assumption to verify before implementation: the current watch loop monitors only issue source files (beads JSONL, workspace config). If confirmed in `src/main.rs`, then when `--audience` + `--overlay` are active, the overlay file must also trigger re-export:
- Add `overlay_path` to the set of watched files (alongside issue sources)
- On overlay mtime change: reload overlay config, rebuild AudienceView,
  regenerate audience pages (executive + dashboard)
- If the edited overlay is temporarily invalid YAML / fails
  `deny_unknown_fields` validation, keep serving the last-valid overlay,
  log a warning, and continue the watch loop rather than aborting export
  watch mode
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

The test strategy must verify more than output presence. It must prove that audience export preserves one shared project state while exposing three different readings of it. If the tests only assert file existence and happy-path rendering, they will miss the actual failure mode: semantic drift between engineer, owner, and financial surfaces.

- **Conformance** (Step 0): `--export-pages` without `--audience` =
  functionally identical (file tree + JSON content + binary assets,
  excluding `generated_at` only; `data_hash` must match exactly)
- **Unit** (inline in module):
  - Overlay parsing: valid, invalid, missing fields, unknown fields
  - Economics math: burn_rate, cost_to_complete, cost_of_delay, budget_utilization,
    zero velocity, no estimates, budget exceeded
  - Flow distribution: mixed issue types, empty input, all one type,
    overlapping categories (issue is both bug AND has alert → Risk wins)
  - Urgency profile: all four archetypes, deterministic priority-order edge cases, overlap cases (`priority == 1` + blocker + due_date + stale)
  - Shared-state invariants: engineer/owner/financial `AudienceView` variants preserve the same underlying facts while differing only in emphasis, surface set, aggregation, and wording
    - explicit fact set for invariant tests: `data_hash`, issue identity set, `open_count`, `closed_count`, `blocker_count`, `flow_distribution` raw counts, economics input presence/absence, and recommendation ids before surface-specific budgeting
  - HTML escaping: all five characters, nested, empty string
  - Locale: both bundles complete, interpolation, fallback
  - Placeholder rendering: CSS braces preserved, nested placeholders,
    missing placeholder key
- **E2E**:
  - `--export-pages --audience owner` → SPA at `index.html`, executive at
    `executive.html`, `dashboard/index.html` present
  - `--export-pages --audience investor` → executive at `index.html`,
    no SPA (no `vendor/`, no `assets/`), no `executive.html`, no
    `data/`, no `beads.sqlite3`
  - `--export-pages --audience investor --overlay` → economics present,
    no broken links to SPA
  - `--export-pages --audience engineer` → low-aggregation surface set
  - `--robot-economics --overlay` → valid JSON envelope
  - `--robot-economics` without overlay → clear error message
  - `--audience` without `--export-pages` → clear error message
  - `--audience` with `--export-md` → clear error message
  - `--locale` without `--audience` and without `--robot-economics` → clear error message
  - `--overlay` without `--audience` and without `--robot-economics` → clear error message
  - Cross-lens semantic consistency: the same export input yields stable shared facts across engineer, owner, and financial outputs even when top-level surface, emphasis, and bundle shape differ
  - Owner-lens coverage: owner export explicitly surfaces delivery posture, milestone pressure, balance signals, and owner-specific aggregates instead of collapsing into either engineer detail or financial summary
  - `--pages` non-TTY help / operator guidance is audience-aware enough
    that it does not imply audience export is unavailable or silently
    wizard-configured when it is not
  - Interactive wizard path passes audience config through to export,
    or explicitly surfaces v1 defaults / manual-only inputs in config
    preview and operator messaging
- **Snapshot** (insta):
  - shell.html content for each audience × locale combination
  - dashboard.html content for each audience
  - Cross-lens snapshots confirm that wording and emphasis change by audience without changing the underlying fact pattern represented in the pages
- **Shell E2E / operator logging**:
  - Extend `scripts/e2e_preview_pages.sh` or add an adjacent audience
    script that captures stdout/stderr, preview status JSON, watch
    regeneration events, wizard/operator transcript, and preserved
    artifact paths on failure
  - Logs should identify which stage failed (export, preview, watch,
    wizard/help, robot-economics) without requiring a future agent to
    reread implementation code

## Module structure

New submodule justified per AGENTS.md: audience export introduces a distinct projection-and-rendering boundary over existing analysis state. That boundary is the real reason to isolate the code. The audience layer is not core graph analysis, not generic export plumbing, and not a one-off template helper. It is the place where shared project facts are transformed into audience-aware views without changing their meaning.

This structure keeps the responsibilities clean:
- `mod.rs` owns shared audience projections, configuration, validation, and lens-aware view assembly.
- `templates.rs` owns rendering mechanics only; it should not become a second semantic layer.
- Embedded templates and locale assets stay close to the audience module because they are surface definitions over the same projection boundary.

Locale logic (bundle loading, interpolation, fallback) lives in `mod.rs` rather than a separate file — at ~80 LOC it does not clear the "incredibly high bar" for new files (AGENTS.md), and separating it further would weaken cohesion inside the shared projection layer.

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

The touched-file list below should be read by role, not as an undifferentiated churn inventory. The changes fall into five buckets: shared projection logic, surface rendering, CLI exposure, pipeline integration, and invariant enforcement in tests. That mapping matters because it shows the implementation is extending one coherent audience layer rather than scattering audience semantics arbitrarily through the codebase.

| File | Change | Lines (est.) |
|------|--------|-------------|
| `src/audience/mod.rs` | NEW: shared audience projection layer — structs, economics, flow, urgency, validation, locales, representative_items projection from RobotOverviewOutput | +350 |
| `src/audience/templates.rs` | NEW: audience surface rendering layer — placeholder engine + shell/dashboard renderers | +270 |
| `src/audience/shell.html` | NEW: executive summary template | +100 |
| `src/audience/dashboard.html` | NEW: dashboard template | +150 |
| `src/audience/en.yaml` | NEW: English strings | +40 |
| `src/audience/uk.yaml` | NEW: Ukrainian strings | +40 |
| `src/cli.rs` | CLI exposure for lens selection and economics projection flags + validation rules | +30 |
| `src/main.rs` | Pipeline wiring for audience + robot-economics dispatch; remove or re-export moved overview types/builder; update overview regression test/reference site(s) during refactor churn | ~0 net, moderate churn |
| `src/robot.rs` | Shared machine-readable exposure: `implemented_robot_command_names`, docs, schema entries (+40) + relocated overview types & builder from `main.rs` (~+200) | +240 |
| *(prerequisite refactor)* | Move `RobotOverviewOutput`, `RobotOverviewPick`, `RobotOverviewFront`, `RobotOverviewUnlockMaximizer`, `RobotOverviewLabelCount`, `RobotOverviewCommands`, `RobotOverviewSummary`, `RobotOverviewBlocker`, and `build_robot_overview_output()` from `src/main.rs` to `src/robot.rs`, declare `pub`, and update the existing regression test at `main.rs:8087` (or keep a `pub` re-export). "Net LOC: ~0" is only nominal — the move still creates real edit churn (moderate churn in regression-test/imports). Counted already in `main.rs` / `robot.rs` rows above. | n/a |
| `src/export_pages.rs` | Export pipeline integration: 4th param to bundle fn, audience branch, routing | +100 |
| `src/export_md.rs` | Hook/pipeline integration: extend `HookContext` + `build_hook_env` for audience vars; constructor/builder changes + 3 existing tests updated | +50 |
| `src/pages_wizard.rs` | Operator-surface integration: audience awareness in config summary + operator messaging | +30 |
| `scripts/e2e_preview_pages.sh` | Invariant/operability enforcement: extend preview/watch/pages operator script with audience scenarios + artifact logging | +80 |
| `src/lib.rs` | `pub mod audience;` | +1 |
| `tests/` | Invariant enforcement: conformance + e2e + snapshot + mechanical `, None` additions | +280 |
| **Total** | (net project growth) | **~1600** |
| *(diff-insertions incl. move)* | Add ~200 LOC of relocated code (diff counts move both as + in `robot.rs` and − in `main.rs`) | ~1800 |

## Resolved dependency: upstream --robot-overview (#4, closed 2026-04-06)

Dicklesworthstone/beads_viewer_rust#4 shipped and closed the coverage gap we raised. This matters to audience export because the feature needs a stable, shared "next steps" source that already combines local triage quality with unlock coverage breadth. Without that upstream resolution, the audience layer would be forced either to duplicate merge logic or to present a narrower and less defensible view of recommended work.

The issue was closed 2026-04-06; the maintainer's closing rationale and SL-000 walkthrough landed as a follow-up comment on 2026-04-17 (comment `4266752765`). The merged implementation draws from both candidate sources — exactly the condition we called out as required. Relevant commits now in `main`:

- `92e61cd` — Add robot overview output
- `a357d9d` — fronts coverage tests
- `b7d8316` — README docs
- **`2abdb95`** — `feat(robot_overview): surface unlock-maximizers
  alongside triage picks` (the fix)
- `d38b9ba` — harden regression test when `UNLOCK_KING` lands in `top_pick`
- `e9f4078` — JSON-shape symmetry with `skip_serializing_if`
- `e05e83e` — `--robot-orient` as visible alias for `--robot-overview`
- `b77a7f0` / `4a800e7` / `84dbfe9` — follow-up cleanup, direct
  `top_k_unlock_set` call, `compute_top_k_set` promoted to `pub`

**Verified on the fixture we flagged.** Regression test `robot_overview_surfaces_unlock_maximizer_triage_would_miss` in `src/main.rs` asserts that `SL-000` (49 unlocks) appears in `unlock_maximizers` on `tests/testdata/stress_large_500.jsonl`. The SL-000 invisibility caveat no longer applies.

**Upstream shape (consumed by audience export):** `build_robot_overview_output()` in `src/main.rs` returns `RobotOverviewOutput` with three objective-distinct candidate pools plus graph summary. This is important because it gives the audience layer one upstream recommendation substrate that can be read by all three lenses rather than forcing each surface to assemble its own recommendation logic:

- `top_pick: Option<RobotOverviewPick>` — triage composite winner
- `fronts: Vec<RobotOverviewFront>` — per-label triage representatives from `triage.recommendations_by_label`. **Caveat:** multi-label recommendations are inserted into every matching label group (`src/analysis/triage.rs:1108`), so a single issue can appear as representative of multiple fronts. `representative_items` must apply a local de-dup pass on `representative.id` after flattening fronts.
- `unlock_maximizers: Vec<RobotOverviewUnlockMaximizer>` — greedy submodular unlock coverage from `analyzer.top_k_unlock_set(10)` (the single-purpose helper, not full `advanced_insights()`, per `4a800e7`), filtered to exclude whatever `top_pick`/`fronts` already surfaced (`src/main.rs:5988-6005`) and then capped with `.take(5)`
- plus `summary`, `top_blocker`, `top_labels`, `commands`, `usage_hints`

**Integration baseline (simplified):** Audience export consumes this function's output directly. The internal triage+insights merge path is no longer needed — upstream already does it, and duplicating the merge would drift. That keeps recommendation semantics shared across engineer, owner, and financial surfaces instead of allowing each one to fork into its own "next steps" logic.

**Required upstream refactor: move overview types from `src/main.rs` to `src/robot.rs`.** `main.rs` is the `[[bin]]` target (see `Cargo.toml`); the audience module will live in the library crate (`src/audience/`, declared in `src/lib.rs`). Library code cannot import symbols from the binary target, so a simple `pub` on the types in `main.rs` does not make them reachable from `src/audience/`. The fix is a prerequisite refactor: move `RobotOverviewOutput`, `RobotOverviewPick`, `RobotOverviewFront`, `RobotOverviewUnlockMaximizer`, `RobotOverviewLabelCount`, `RobotOverviewCommands`, `RobotOverviewSummary`, `RobotOverviewBlocker`, and `build_robot_overview_output()` from `src/main.rs` to `src/robot.rs` (alongside `RobotEnvelope`), declare them `pub`, and update the call site in `main.rs` to use the new module path. Net LOC: ~0 (code moves, does not duplicate). This refactor is a hard prerequisite, not an optimization. It preserves a single shared recommendation source that can be consumed by library code, page rendering, and robot outputs without semantic duplication.

**Item-budget policy for `representative_items` (explicit):** `MAX_REPRESENTATIVE_ITEMS = 8`. Always include `top_pick` if present (1 slot), then up to 4 `fronts` entries de-duplicated by `representative.id` (catches the multi-label caveat above), then fill remaining slots from `unlock_maximizers`. Because upstream already applies `.take(5)` to the unlock pool, the audience layer cannot assume infinite backfill. The practical range is `0..=8` items because `top_pick`, `fronts`, and `unlock_maximizers` may each be empty. Common balanced case remains `1 + 4 + 3 = 8`. Preserves the orient composite winner, shows breadth across labels, and guarantees at least some pure unlock coverage. Unlock items do not carry `score`/`reasons`; the executive summary renders them as "unblocks N downstream issues" cards, not as pretended triage picks.

For multi-agent swarms: the same payload also works as a work distribution primitive — a coordinator partitions agents across `fronts` + `unlock_maximizers` regions from the first assignment cycle instead of sending all agents to the same triage hotspot. That reuse reinforces the same design rule as the rest of the document: one analyzed state, multiple operational readings.

## Constraints

These constraints are not incidental implementation preferences. They are the guardrails that keep the audience feature honest: additive over the existing export path, grounded in shared analysis state, and narrow enough not to create a second reporting system by accident.

- `unsafe_code = "forbid"` — no unsafe
- All builds via `rch exec -- cargo ...` with `export TMPDIR=/data/tmp`
- The existing test suite at implementation time must pass unchanged (especially conformance)
- `--export-pages` without `--audience` = zero behavior change
- The existing export test suite must remain stable — `ExportPagesOptions` struct must not gain new fields; audience config passed as separate parameter
- Release profile: opt-level="z", LTO, stripped
- No new dependencies (serde_yaml already present)
- New files only for genuinely new functionality (AGENTS.md rule); locale logic (80 LOC) merged into `mod.rs`, not a separate file
- Code changes manually, no script-based edits (AGENTS.md rule)
- Every module includes inline `#[cfg(test)]` unit tests (AGENTS.md rule)
