#!/usr/bin/env python3
"""Render static HTML audience lenses from bvr robot-contract JSON.

The renderer intentionally lives outside core bvr. It consumes the same cached
triad used by the shell examples:

    .bv/runs/overview.json
    .bv/runs/delivery.json
    .bv/runs/economics.json  (optional)
"""

from __future__ import annotations

import argparse
import html
import json
import os
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_RUNS_DIR = Path(".bv/runs")
DEFAULT_OUT_DIR = Path(".bv/audience-html")
EMPTY_VALUE = "—"
PROJECTION_KEYS = [
    "burn_rate_per_day",
    "throughput_issues_per_day",
    "cost_to_complete",
    "budget_utilization_pct",
]


def esc(value: Any) -> str:
    if value is None:
        return EMPTY_VALUE
    return html.escape(str(value), quote=True)


def load_json(path: Path, required: bool) -> dict[str, Any] | None:
    if not path.exists():
        if required:
            raise SystemExit(f"missing required input: {path}")
        return None
    with path.open("r", encoding="utf-8") as fh:
        value = json.load(fh)
    if not isinstance(value, dict):
        raise SystemExit(f"input must be a JSON object: {path}")
    return value


def fmt_number(value: Any, digits: int = 2) -> str:
    if value is None:
        return EMPTY_VALUE
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if value.is_integer():
            return str(int(value))
        return f"{value:.{digits}f}".rstrip("0").rstrip(".")
    return str(value)


def currency_label(economics: dict[str, Any] | None) -> str:
    if not economics:
        return "USD"
    inputs = economics.get("inputs", {})
    if isinstance(inputs, dict):
        return str(inputs.get("currency") or "USD")
    return "USD"


def fmt_money(value: Any, economics: dict[str, Any] | None) -> str:
    if value is None:
        return EMPTY_VALUE
    return f"{fmt_number(value)} {currency_label(economics)}"


def fmt_percent_fraction(value: Any, digits: int = 2) -> str:
    if value is None:
        return EMPTY_VALUE
    if isinstance(value, bool):
        return "true" if value else "false"
    try:
        pct = float(value) * 100
    except (TypeError, ValueError):
        return str(value)
    return f"{fmt_number(pct, digits)}%"


def fmt_economic_value(key: str, value: Any, economics: dict[str, Any] | None) -> str:
    if key.endswith("_pct"):
        return fmt_percent_fraction(value)
    if "cost" in key or "burn" in key or "budget" in key:
        return fmt_money(value, economics)
    return fmt_number(value)


def pct_width(value: Any) -> str:
    try:
        pct = float(value)
    except (TypeError, ValueError):
        pct = 0.0
    pct = max(0.0, min(100.0, pct))
    return f"{pct:.2f}%"


def summary_value(overview: dict[str, Any], key: str) -> Any:
    summary = overview.get("summary", {})
    if isinstance(summary, dict):
        return summary.get(key)
    return None


def tripped_guards(economics: dict[str, Any] | None) -> list[str]:
    if not economics:
        return []
    guards = economics.get("guards", {})
    if not isinstance(guards, dict):
        return []
    return [name for name, tripped in guards.items() if tripped is True]


def data_hash_warning(docs: list[dict[str, Any] | None]) -> str:
    # Missing hashes are ignored; this banner flags known snapshot divergence.
    hashes = sorted({str(doc.get("data_hash")) for doc in docs if doc and doc.get("data_hash")})
    if len(hashes) <= 1:
        return ""
    return (
        '<div class="warning"><strong>data_hash mismatch:</strong> '
        + esc(", ".join(hashes))
        + "</div>"
    )


def provenance(overview: dict[str, Any], delivery: dict[str, Any], economics: dict[str, Any] | None) -> str:
    doc = economics or delivery or overview
    generated = doc.get("generated_at") or overview.get("generated_at") or "-"
    version = doc.get("version") or overview.get("version") or "-"
    data_hash = doc.get("data_hash") or overview.get("data_hash") or "-"
    overlay = ""
    if economics and economics.get("overlay_hash"):
        overlay = f" | overlay:{esc(economics.get('overlay_hash'))}"
    return (
        '<div class="provenance">'
        f"project: bvr | data:{esc(data_hash)}{overlay} | "
        f"generated: {esc(generated)} | bvr {esc(version)}"
        "</div>"
    )


def page(title: str, subtitle: str, body: str, overview: dict[str, Any], delivery: dict[str, Any], economics: dict[str, Any] | None, css: str) -> str:
    nav = (
        '<nav class="nav">'
        '<a href="index.html">Index</a>'
        '<a href="engineer.html">Engineer</a>'
        '<a href="owner.html">Owner</a>'
        '<a href="investor.html">Investor</a>'
        "</nav>"
    )
    return "\n".join(
        [
            "<!doctype html>",
            '<html lang="en">',
            "<head>",
            '<meta charset="utf-8">',
            '<meta name="viewport" content="width=device-width, initial-scale=1">',
            f"<title>{esc(title)}</title>",
            "<style>",
            css,
            "</style>",
            "</head>",
            "<body>",
            '<div class="shell">',
            '<header class="top">',
            f"<div><h1>{esc(title)}</h1><p>{esc(subtitle)}</p></div>",
            nav,
            "</header>",
            provenance(overview, delivery, economics),
            data_hash_warning([overview, delivery, economics]),
            body,
            "</div>",
            "</body>",
            "</html>",
            "",
        ]
    )


def metric_grid(metrics: list[tuple[str, Any]]) -> str:
    cards = []
    for label, value in metrics:
        cards.append(
            '<div class="card">'
            f'<span class="metric">{esc(fmt_number(value))}</span>'
            f'<span class="muted">{esc(label)}</span>'
            "</div>"
        )
    return '<div class="grid">' + "".join(cards) + "</div>"


def bars(title: str, rows: list[dict[str, Any]]) -> str:
    parts = [f"<section><h2>{esc(title)}</h2>"]
    if not rows:
        parts.append('<p class="empty">No data.</p>')
    for row in rows:
        label = row.get("category", "-")
        count = row.get("count", 0)
        pct = row.get("pct", 0)
        parts.append(
            '<div class="bar-row">'
            f"<span>{esc(label)}</span>"
            '<div class="bar-track">'
            f'<div class="bar-fill" style="width: {esc(pct_width(pct))}"></div>'
            "</div>"
            f"<span>{esc(fmt_number(count))} | {esc(fmt_number(pct))}%</span>"
            "</div>"
        )
    parts.append("</section>")
    return "".join(parts)


def table(headers: list[str], rows: list[list[Any]], empty: str) -> str:
    if not rows:
        return f'<p class="empty">{esc(empty)}</p>'
    head = "".join(f"<th>{esc(h)}</th>" for h in headers)
    body_rows = []
    for row in rows:
        body_rows.append("<tr>" + "".join(f"<td>{cell}</td>" for cell in row) + "</tr>")
    return "<table><thead><tr>" + head + "</tr></thead><tbody>" + "".join(body_rows) + "</tbody></table>"


def render_index(overview: dict[str, Any], delivery: dict[str, Any], economics: dict[str, Any] | None, css: str) -> str:
    body = [
        metric_grid(
            [
                ("open", summary_value(overview, "open_issues")),
                ("in progress", summary_value(overview, "in_progress_issues")),
                ("blocked", summary_value(overview, "blocked_issues")),
                ("closed", summary_value(overview, "closed_issues")),
                ("cycles", summary_value(overview, "cycle_count")),
            ]
        ),
        '<section><h2>Audience lenses</h2><div class="grid">',
        '<a class="card" href="engineer.html"><h2>Engineer</h2><p>Execution topology, unlocks, blockers, and claim commands.</p></a>',
        '<a class="card" href="owner.html"><h2>Owner</h2><p>Delivery posture, flow mix, urgency, and milestone pressure.</p></a>',
        '<a class="card" href="investor.html"><h2>Investor</h2><p>Inputs, burn, cost-to-complete, guards, and provenance.</p></a>',
        "</div></section>",
    ]
    return page("bvr audience report", "External HTML lenses rendered from bvr robot contracts.", "".join(body), overview, delivery, economics, css)


def render_engineer(overview: dict[str, Any], delivery: dict[str, Any], economics: dict[str, Any] | None, css: str) -> str:
    rows = []
    for item in overview.get("unlock_maximizers", []) or []:
        rows.append(
            [
                f'<span class="mono">{esc(item.get("id"))}</span>',
                esc(fmt_number(item.get("marginal_unlocks"))),
                esc(item.get("title")),
                f'<code>{esc(item.get("claim_command"))}</code>',
            ]
        )
    blocker_rows = []
    if economics:
        for item in (economics.get("projections", {}).get("cost_of_delay", []) or [])[:10]:
            blocker_rows.append(
                [
                    f'<span class="mono">{esc(item.get("id"))}</span>',
                    esc(fmt_number(item.get("dependents_count"))),
                    esc(item.get("title")),
                    esc(fmt_money(item.get("rate_per_day"), economics)) + "/day",
                ]
            )
    guards = tripped_guards(economics)
    body = [
        metric_grid(
            [
                ("open", summary_value(overview, "open_issues")),
                ("actionable", summary_value(overview, "actionable_issues")),
                ("blocked", summary_value(overview, "blocked_issues")),
                ("in progress", summary_value(overview, "in_progress_issues")),
                ("cycles", summary_value(overview, "cycle_count")),
            ]
        ),
        "<section><h2>Next moves</h2>",
        table(["ID", "+ unlocks", "Title", "Claim"], rows, "No actionable unlock maximizers."),
        "</section>",
        "<section><h2>Top blockers by downstream reach</h2>",
        table(["ID", "Dependents", "Title", "Rate"], blocker_rows, "No cost-of-delay data."),
        "</section>",
        bars("Flow sanity check", delivery.get("flow_distribution", []) or []),
        "<section><h2>Guards</h2>",
        "<p>" + (", ".join(esc(g) for g in guards) if guards else "No guards tripped.") + "</p>",
        "</section>",
    ]
    return page("Engineer lens", "Actionable work, unlock coverage, and graph pressure.", "".join(body), overview, delivery, economics, css)


def render_owner(overview: dict[str, Any], delivery: dict[str, Any], economics: dict[str, Any] | None, css: str) -> str:
    milestone_rows = []
    for item in delivery.get("milestone_pressure", []) or []:
        state = "overdue" if item.get("is_overdue") else "due soon"
        if item.get("is_blocked"):
            state += " / blocked"
        milestone_rows.append(
            [
                f'<span class="mono">{esc(item.get("id"))}</span>',
                esc(item.get("title")),
                esc(item.get("due_date")),
                esc(state),
            ]
        )
    econ_cards = ""
    if economics:
        projections = economics.get("projections", {})
        econ_cards = metric_grid(
            [
                ("burn / day", fmt_money(projections.get("burn_rate_per_day"), economics)),
                ("throughput / day", projections.get("throughput_issues_per_day")),
                ("cost to finish", fmt_money(projections.get("cost_to_complete"), economics)),
            ]
        )
    else:
        econ_cards = '<p class="empty">No economics.json; delivery lens rendered without economics.</p>'
    body = [
        metric_grid(
            [
                ("open", summary_value(overview, "open_issues")),
                ("in progress", summary_value(overview, "in_progress_issues")),
                ("blocked", summary_value(overview, "blocked_issues")),
                ("closed", summary_value(overview, "closed_issues")),
            ]
        ),
        bars("Flow distribution", delivery.get("flow_distribution", []) or []),
        bars("Urgency profile", delivery.get("urgency_profile", []) or []),
        "<section><h2>Milestone pressure</h2>",
        table(["ID", "Title", "Due", "State"], milestone_rows, "No due-date items inside the window."),
        "</section>",
        "<section><h2>Delivery-adjacent economics</h2>",
        econ_cards,
        "</section>",
    ]
    return page("Owner lens", "Delivery posture, capacity mix, urgency, and milestone pressure.", "".join(body), overview, delivery, economics, css)


def render_investor(
    overview: dict[str, Any],
    delivery: dict[str, Any],
    economics: dict[str, Any] | None,
    css: str,
    economics_path: Path,
) -> str:
    if not economics:
        economics_path_text = esc(str(economics_path))
        body = (
            '<section><h2>No economics overlay</h2>'
            f'<p class="empty">Missing {economics_path_text}. Create .bv/economics.json and run '
            '<code>bvr --robot-economics --economics-overlay .bv/economics.json &gt; '
            f'{economics_path_text}</code>.</p>'
            "</section>"
            + bars("Capacity mix", delivery.get("flow_distribution", []) or [])
        )
        return page("Investor lens", "Financial view unavailable until economics.json exists.", body, overview, delivery, economics, css)

    inputs = economics.get("inputs", {})
    projections = economics.get("projections", {})
    input_rows = [[esc(key), esc(fmt_economic_value(key, value, economics))] for key, value in inputs.items()]
    projection_rows = []
    projection_keys = PROJECTION_KEYS + [
        key for key in projections if key not in PROJECTION_KEYS and key != "cost_of_delay"
    ]
    for key in projection_keys:
        value = projections.get(key)
        projection_rows.append([esc(key), esc(fmt_economic_value(key, value, economics))])
    blocker_rows = []
    for item in (projections.get("cost_of_delay", []) or [])[:20]:
        blocker_rows.append(
            [
                f'<span class="mono">{esc(item.get("id"))}</span>',
                esc(fmt_number(item.get("dependents_count"))),
                esc(fmt_money(item.get("rate_per_day"), economics)) + "/day",
                esc(item.get("title")),
            ]
        )
    guard_rows = []
    for name, value in (economics.get("guards", {}) or {}).items():
        status = "TRIPPED" if value else "ok"
        cls = "status-trip" if value else "status-ok"
        guard_rows.append([esc(name), f'<span class="{cls}">{esc(status)}</span>'])
    provenance_rows = [
        ["data_hash", esc(economics.get("data_hash"))],
        ["overlay_hash", esc(economics.get("overlay_hash"))],
        ["schema_version", esc(economics.get("schema_version"))],
        ["version", esc(economics.get("version"))],
    ]
    body = [
        "<section><h2>Inputs</h2>",
        table(["Field", "Value"], input_rows, "No input data."),
        "</section>",
        "<section><h2>Projections</h2>",
        table(["Metric", "Value"], projection_rows, "No projections."),
        "</section>",
        "<section><h2>Top sources of economic drag</h2>",
        table(["ID", "Dependents", "Rate", "Title"], blocker_rows, "No cost-of-delay entries."),
        "</section>",
        "<section><h2>Guards</h2>",
        table(["Guard", "State"], guard_rows, "No guards."),
        "</section>",
        bars("Capacity mix", delivery.get("flow_distribution", []) or []),
        "<section><h2>Provenance</h2>",
        table(["Field", "Value"], provenance_rows, "No provenance."),
        "</section>",
    ]
    return page("Investor lens", "Economic projection, data-quality guards, and provenance.", "".join(body), overview, delivery, economics, css)


def write_atomic(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(contents, encoding="utf-8")
    os.replace(tmp, path)


def render(runs_dir: Path, out_dir: Path, css_path: Path) -> list[Path]:
    overview = load_json(runs_dir / "overview.json", required=True)
    delivery = load_json(runs_dir / "delivery.json", required=True)
    economics_path = runs_dir / "economics.json"
    economics = load_json(economics_path, required=False)
    assert overview is not None
    assert delivery is not None
    css = css_path.read_text(encoding="utf-8")
    pages = {
        "index.html": render_index(overview, delivery, economics, css),
        "engineer.html": render_engineer(overview, delivery, economics, css),
        "owner.html": render_owner(overview, delivery, economics, css),
        "investor.html": render_investor(overview, delivery, economics, css, economics_path),
    }
    written = []
    for name, contents in pages.items():
        target = out_dir / name
        write_atomic(target, contents)
        written.append(target)
    return written


def main() -> int:
    parser = argparse.ArgumentParser(description="Render bvr robot-contract JSON as static HTML audience lenses.")
    parser.add_argument("--runs-dir", type=Path, default=DEFAULT_RUNS_DIR)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--css", type=Path, default=SCRIPT_DIR / "assets/style.css")
    args = parser.parse_args()
    written = render(args.runs_dir, args.out_dir, args.css)
    for path in written:
        print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
