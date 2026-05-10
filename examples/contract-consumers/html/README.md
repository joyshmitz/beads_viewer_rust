# HTML audience lenses

This renderer turns the existing `bvr v0.2.1` robot-contract triad into a
static HTML bundle. It does not modify `bvr`, does not call `--export-pages`,
and does not require future `--audience` support.

## Inputs

```text
.bv/runs/
|-- overview.json
|-- delivery.json
`-- economics.json   # optional, but required for the full investor lens
```

The JSON files come from:

```bash
bvr --robot-overview > .bv/runs/overview.json
bvr --robot-delivery > .bv/runs/delivery.json
bvr --robot-economics \
  --economics-overlay .bv/economics.json \
  > .bv/runs/economics.json
```

`render.sh` runs the existing `examples/contract-consumers/triad.sh` first,
then renders HTML. Set `BVR_HTML_SKIP_TRIAD=1` to render from existing JSON.

## Usage

```bash
mkdir -p .bv
cp examples/contract-consumers/economics.sample.json .bv/economics.json
examples/contract-consumers/html/render.sh
```

Output:

```text
.bv/audience-html/
|-- index.html
|-- engineer.html
|-- owner.html
`-- investor.html
```

Each HTML file is self-contained: CSS is inlined, no JavaScript is required,
and no remote assets are used.

## Lens mapping

- `engineer.html` reads execution state and unlock coverage from
  `overview.json`, flow sanity from `delivery.json`, and downstream reach /
  guard data from `economics.json` when available.
- `owner.html` reads flow distribution, urgency profile, and milestone
  pressure from `delivery.json`, with adjacent economics when available.
- `investor.html` reads inputs, projections, cost-of-delay, guards, and
  provenance from `economics.json`, plus capacity mix from `delivery.json`.

If the three envelopes have different `data_hash` values, every page renders a
warning banner instead of silently splicing incoherent snapshots.

## Tests

```bash
examples/contract-consumers/html/tests/run.sh
```

The test harness runs a golden fixture diff and a live smoke render against
the current repo. It stays outside the cargo suite because this directory is a
downstream consumer example, not core `bvr`.
