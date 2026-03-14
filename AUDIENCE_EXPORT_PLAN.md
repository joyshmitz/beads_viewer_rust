# Audience Export — Implementation Plan

Add `--audience` flag to `--export-pages` for stakeholder-facing views with optional project economics.

## What this adds

When `--audience` is set, `--export-pages` generates additional surfaces alongside the existing SPA:

```
output/
├── index.html          ← NEW: executive summary (no-JS, <10KB)
├── dashboard/
│   └── index.html      ← NEW: KPI dashboard (velocity, tracks, projections)
├── app/
│   └── index.html      ← MOVED: full bvr SPA (was root index.html)
├── data/               ← UNCHANGED
└── beads.sqlite3       ← UNCHANGED
```

Without `--audience`, export is identical to current behavior.

## CLI surface

```bash
# Existing (unchanged)
bvr --export-pages ./output

# New
bvr --export-pages ./output --audience owner
bvr --export-pages ./output --audience investor --locale uk
bvr --export-pages ./output --audience owner --overlay .bv/audience.yaml
```

### New flags

| Flag | Type | Default | Purpose |
|------|------|---------|---------|
| `--audience` | `owner\|investor\|engineer` | none | Selects which surfaces to generate |
| `--locale` | string | `en` | UI string localization (e.g. `uk`, `de`) |
| `--overlay` | path | none | YAML config for audience/locale/content/economics |

### Audience matrix

| Surface | (none) | engineer | owner | investor |
|---------|--------|----------|-------|----------|
| Full SPA | root | app/ | app/ | — |
| Executive summary | — | index.html | index.html | index.html |
| KPI dashboard | — | ✓ | ✓ | ✓ |
| Triage detail | — | ✓ | ✓ | — |
| Dependency graph | — | ✓ | ✓ | — |
| Economics cards | — | — | if configured | if configured |

## Data flow

No new data collection. All data comes from already-computed analysis:

```
TriageResult (already computed)
├── QuickRef          → summary counts, progress %
├── Recommendations   → top picks, next steps
├── BlockersToClear   → risks section
└── ProjectHealth     → health indicators

GraphStats (already computed)
├── CriticalPath      → timeline section
├── ArticulationPoints → bottleneck highlights
└── Velocity          → burn rate, projections

                    ↓ (new: thin projection layer)

AudienceView struct
├── audience filter  → which sections to render
├── locale          → which strings to use
├── economics       → optional cost projections
└── content         → titles, thesis, disclaimer

                    ↓ (new: template rendering)

Shell HTML (index.html)
Dashboard HTML (dashboard/index.html)
```

## Overlay config (.bv/audience.yaml)

```yaml
audience: owner
locale: uk

content:
  title: "Project Status"
  subtitle: "Q1 2026"
  thesis: "On track with controlled tech debt."
  disclaimer: "Projections based on trailing 30-day velocity."

economics:  # optional, omit to hide
  hourly_rate: 85
  hours_per_day: 6
  budget_envelope: 50000  # optional cap
```

## Economics projections (when configured)

| Metric | Formula | Source |
|--------|---------|-------|
| Burn rate ($/week) | `weekly_velocity × hours_per_day × hourly_rate` | velocity + config |
| Remaining cost | `(open_count / velocity) × daily_rate` | triage + config |
| Cost of blocked work | `blocked_count × avg_block_days × daily_rate` | triage + config |
| Budget utilization | `estimated_spent / budget_envelope` | computed + config |

Rules:
- Zero velocity → "insufficient data", not NaN
- No hourly_rate → economics section omitted entirely
- All projections display disclaimer from content

## Implementation steps

### Step 1: CLI flags + AudienceView struct

Files: `src/main.rs`

- Add `--audience`, `--locale`, `--overlay` clap flags
- Define `AudienceView` struct that borrows from existing `TriageResult` + `GraphStats`
- Parse overlay YAML (serde_yaml)
- Wire into `--export-pages` code path

### Step 2: Shell template

Files: `src/audience_shell.rs` (new), `src/audience_templates/shell.html` (new)

- Static HTML template (~100 lines)
- No JavaScript, renders from AudienceView
- Sections: header, KPI cards, risks, next steps, deep links
- Locale strings from embedded YAML bundles

### Step 3: Dashboard template

Files: `src/audience_dashboard.rs` (new), `src/audience_templates/dashboard.html` (new)

- Lightweight HTML with optional Chart.js for velocity
- Sections: thesis, KPI grid, track breakdown, bottlenecks, economics (optional)
- Audience filtering (investor sees summary only)

### Step 4: Export integration

Files: `src/export_pages.rs` (modify)

- When `--audience` set: move current index.html → app/index.html
- Generate shell index.html + dashboard/
- Update internal links
- Run existing hooks (shell/dashboard available in $BV_EXPORT_PATH)

### Step 5: Locale bundles

Files: `src/locales/en.yaml`, `src/locales/uk.yaml` (new)

- Key-value pairs for UI strings
- Embedded via include_str!
- Fallback to en if locale not found

### Step 6: Economics module

Files: `src/audience_economics.rs` (new)

- Pure functions: velocity + config → projections
- Zero-velocity guard
- Budget utilization calculation
- Returns structured data for template, not HTML

### Step 7: Tests

- Unit: AudienceView construction, economics math, locale loading
- Conformance: `--export-pages` without `--audience` produces identical output
- E2E: `--export-pages --audience owner` generates expected file tree
- Snapshot: shell.html and dashboard.html content

## Files touched (estimated)

| File | Change | Lines (est.) |
|------|--------|-------------|
| `src/main.rs` | Add flags, wire audience | +50 |
| `src/audience_view.rs` | NEW: AudienceView struct + builder | +120 |
| `src/audience_shell.rs` | NEW: shell HTML renderer | +150 |
| `src/audience_dashboard.rs` | NEW: dashboard HTML renderer | +200 |
| `src/audience_economics.rs` | NEW: projection math | +80 |
| `src/export_pages.rs` | Integrate audience into export | +40 |
| `src/locales/en.yaml` | NEW: English strings | +30 |
| `src/locales/uk.yaml` | NEW: Ukrainian strings | +30 |
| `src/audience_templates/shell.html` | NEW: HTML template | +80 |
| `src/audience_templates/dashboard.html` | NEW: HTML template | +120 |
| `tests/` | Unit + e2e + conformance | +200 |
| **Total** | | **~1100** |

## Constraints

- `unsafe_code = "forbid"` — no unsafe
- All builds via `rch exec -- cargo ...`
- Existing 1318 tests must pass (especially conformance)
- `--export-pages` without `--audience` = zero behavior change
- Release profile: opt-level="z", LTO, stripped
