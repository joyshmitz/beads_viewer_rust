#!/usr/bin/env python3
"""Generate large-dataset stress-test fixtures for bvr.

Fixtures created:
1. stress_large_500.jsonl — 500 issues with diverse dependency topologies
2. pathological_deps.jsonl — Extreme dependency patterns (deep chains, wide fan-out, overlapping cycles)
3. malformed_metadata.jsonl — Edge-case metadata values
"""

import json
import sys
from datetime import datetime, timedelta

def jl(obj):
    return json.dumps(obj, separators=(",", ":"))

STATUSES = ["open", "in_progress", "blocked", "closed", "review", "deferred"]
TYPES = ["task", "bug", "feature", "epic", "story", "chore"]
LABELS_POOL = [
    "backend", "frontend", "api", "infra", "ci", "docs", "security",
    "performance", "ux", "data", "auth", "billing", "search", "mobile",
    "analytics", "core", "testing", "migration"
]
ASSIGNEES = ["alice", "bob", "carol", "dave", "eve", "frank", "grace", ""]

def gen_large_500():
    """500 issues with diverse topologies:
    - 0-49:   Linear chain (each depends on previous)
    - 50-99:  Hub-spoke clusters (5 hubs with 10 spokes each)
    - 100-149: Diamond merges (10 diamond patterns)
    - 150-249: Cross-team dependencies (random cross-links)
    - 250-349: Isolated islands (no dependencies)
    - 350-399: Deep fan-out trees (5 roots, each fans to 10)
    - 400-449: Mixed cycles (5 cycles of length 10)
    - 450-499: Sprint-grouped work (5 sprints of 10 issues)
    """
    issues = []
    base = datetime(2024, 1, 1, 9, 0, 0)

    # --- 0-49: Linear chain ---
    for i in range(50):
        deps = []
        if i > 0:
            deps = [{"depends_on_id": f"SL-{i-1:03d}", "type": "blocks"}]
        issues.append({
            "id": f"SL-{i:03d}",
            "title": f"Chain step {i}",
            "description": f"Linear chain node {i}/49",
            "status": STATUSES[i % len(STATUSES)],
            "priority": (i % 5) + 1,
            "issue_type": TYPES[i % len(TYPES)],
            "assignee": ASSIGNEES[i % len(ASSIGNEES)],
            "estimated_minutes": 30 + (i * 7) % 480,
            "created_at": (base + timedelta(hours=i)).isoformat() + "Z",
            "updated_at": (base + timedelta(hours=i, minutes=30)).isoformat() + "Z",
            "labels": [LABELS_POOL[i % len(LABELS_POOL)], LABELS_POOL[(i+3) % len(LABELS_POOL)]],
            "dependencies": deps,
        })

    # --- 50-99: Hub-spoke clusters ---
    for cluster in range(5):
        hub_idx = 50 + cluster * 10
        hub_id = f"SL-{hub_idx:03d}"
        issues.append({
            "id": hub_id,
            "title": f"Hub {cluster} coordinator",
            "status": "open",
            "priority": 1,
            "issue_type": "epic",
            "labels": [f"team-{cluster}", "core"],
            "created_at": (base + timedelta(days=cluster)).isoformat() + "Z",
            "estimated_minutes": 120,
        })
        for spoke in range(1, 10):
            spoke_idx = hub_idx + spoke
            issues.append({
                "id": f"SL-{spoke_idx:03d}",
                "title": f"Hub {cluster} spoke {spoke}",
                "status": "blocked" if spoke % 3 == 0 else "open",
                "priority": (spoke % 5) + 1,
                "issue_type": "task",
                "labels": [f"team-{cluster}"],
                "created_at": (base + timedelta(days=cluster, hours=spoke)).isoformat() + "Z",
                "estimated_minutes": 60 + spoke * 15,
                "dependencies": [{"depends_on_id": hub_id, "type": "blocks"}],
            })

    # --- 100-149: Diamond merges (10 diamonds) ---
    for d in range(10):
        base_idx = 100 + d * 5
        apex = f"SL-{base_idx:03d}"
        left = f"SL-{base_idx+1:03d}"
        right = f"SL-{base_idx+2:03d}"
        merge = f"SL-{base_idx+3:03d}"
        tail = f"SL-{base_idx+4:03d}"
        issues.append({"id": apex, "title": f"Diamond {d} apex", "status": "open", "priority": 1, "issue_type": "task", "labels": ["diamond"], "created_at": (base + timedelta(days=10+d)).isoformat() + "Z", "estimated_minutes": 60})
        issues.append({"id": left, "title": f"Diamond {d} left", "status": "open", "priority": 2, "issue_type": "task", "labels": ["diamond"], "created_at": (base + timedelta(days=10+d, hours=1)).isoformat() + "Z", "estimated_minutes": 90, "dependencies": [{"depends_on_id": apex, "type": "blocks"}]})
        issues.append({"id": right, "title": f"Diamond {d} right", "status": "open", "priority": 2, "issue_type": "task", "labels": ["diamond"], "created_at": (base + timedelta(days=10+d, hours=2)).isoformat() + "Z", "estimated_minutes": 90, "dependencies": [{"depends_on_id": apex, "type": "blocks"}]})
        issues.append({"id": merge, "title": f"Diamond {d} merge", "status": "blocked", "priority": 1, "issue_type": "task", "labels": ["diamond", "merge"], "created_at": (base + timedelta(days=10+d, hours=3)).isoformat() + "Z", "estimated_minutes": 120, "dependencies": [{"depends_on_id": left, "type": "blocks"}, {"depends_on_id": right, "type": "blocks"}]})
        issues.append({"id": tail, "title": f"Diamond {d} tail", "status": "blocked", "priority": 3, "issue_type": "task", "labels": ["diamond"], "created_at": (base + timedelta(days=10+d, hours=4)).isoformat() + "Z", "estimated_minutes": 45, "dependencies": [{"depends_on_id": merge, "type": "blocks"}]})

    # --- 150-249: Cross-team dependencies ---
    for i in range(100):
        idx = 150 + i
        deps = []
        # Each issue depends on 1-3 earlier issues from different ranges
        if i > 5:
            deps.append({"depends_on_id": f"SL-{150 + (i-3):03d}", "type": "blocks"})
        if i > 20 and i % 4 == 0:
            deps.append({"depends_on_id": f"SL-{50 + (i % 50):03d}", "type": "related"})
        if i > 40 and i % 7 == 0:
            deps.append({"depends_on_id": f"SL-{100 + (i % 50):03d}", "type": "blocks"})
        issues.append({
            "id": f"SL-{idx:03d}",
            "title": f"Cross-team work item {i}",
            "status": STATUSES[i % len(STATUSES)],
            "priority": (i % 5) + 1,
            "issue_type": TYPES[i % len(TYPES)],
            "assignee": ASSIGNEES[i % len(ASSIGNEES)],
            "estimated_minutes": 45 + (i * 11) % 360,
            "created_at": (base + timedelta(days=20, hours=i)).isoformat() + "Z",
            "updated_at": (base + timedelta(days=20, hours=i, minutes=45)).isoformat() + "Z",
            "labels": [LABELS_POOL[i % len(LABELS_POOL)], LABELS_POOL[(i+7) % len(LABELS_POOL)]],
            "dependencies": deps,
        })

    # --- 250-349: Isolated islands ---
    for i in range(100):
        idx = 250 + i
        status = "closed" if i < 30 else ("open" if i < 70 else "review")
        issues.append({
            "id": f"SL-{idx:03d}",
            "title": f"Standalone item {i}",
            "status": status,
            "priority": (i % 5) + 1,
            "issue_type": TYPES[i % len(TYPES)],
            "labels": [LABELS_POOL[i % len(LABELS_POOL)]],
            "created_at": (base + timedelta(days=30, hours=i)).isoformat() + "Z",
            "estimated_minutes": 15 + (i * 13) % 300,
        })
        if status == "closed":
            issues[-1]["closed_at"] = (base + timedelta(days=35, hours=i)).isoformat() + "Z"

    # --- 350-399: Deep fan-out trees ---
    for tree in range(5):
        root_idx = 350 + tree * 10
        root_id = f"SL-{root_idx:03d}"
        issues.append({
            "id": root_id,
            "title": f"Fan-out root {tree}",
            "status": "open",
            "priority": 1,
            "issue_type": "epic",
            "labels": ["fanout", f"tree-{tree}"],
            "created_at": (base + timedelta(days=40+tree)).isoformat() + "Z",
            "estimated_minutes": 180,
        })
        for child in range(1, 10):
            child_idx = root_idx + child
            # First 3 children depend on root, rest depend on previous child
            dep_target = root_id if child <= 3 else f"SL-{child_idx-1:03d}"
            issues.append({
                "id": f"SL-{child_idx:03d}",
                "title": f"Fan-out {tree} child {child}",
                "status": "blocked" if child > 3 else "open",
                "priority": (child % 4) + 1,
                "issue_type": "task",
                "labels": [f"tree-{tree}"],
                "created_at": (base + timedelta(days=40+tree, hours=child)).isoformat() + "Z",
                "estimated_minutes": 30 + child * 20,
                "dependencies": [{"depends_on_id": dep_target, "type": "blocks"}],
            })

    # --- 400-449: Mixed cycles (5 cycles of 10) ---
    for cycle in range(5):
        cycle_base = 400 + cycle * 10
        for j in range(10):
            idx = cycle_base + j
            next_idx = cycle_base + ((j + 1) % 10)
            issues.append({
                "id": f"SL-{idx:03d}",
                "title": f"Cycle {cycle} node {j}",
                "status": "blocked",
                "priority": 2,
                "issue_type": "bug",
                "labels": ["cycle", f"cycle-{cycle}"],
                "created_at": (base + timedelta(days=50+cycle, hours=j)).isoformat() + "Z",
                "estimated_minutes": 60,
                "dependencies": [{"depends_on_id": f"SL-{next_idx:03d}", "type": "blocks"}],
            })

    # --- 450-499: Sprint-grouped work ---
    for sprint in range(5):
        for item in range(10):
            idx = 450 + sprint * 10 + item
            deps = []
            if item > 0:
                deps = [{"depends_on_id": f"SL-{idx-1:03d}", "type": "blocks"}]
            issues.append({
                "id": f"SL-{idx:03d}",
                "title": f"Sprint {sprint+1} item {item}",
                "status": "open" if item < 5 else "in_progress",
                "priority": (item % 3) + 1,
                "issue_type": "story",
                "labels": [f"sprint-{sprint+1}", "planned"],
                "created_at": (base + timedelta(days=60+sprint*7, hours=item)).isoformat() + "Z",
                "estimated_minutes": 120 + item * 30,
                "dependencies": deps,
            })

    return issues


def gen_pathological():
    """Pathological dependency patterns:
    - Deep chain of 100 nodes
    - Single node with 50 inbound deps (convergence bottleneck)
    - Single node with 50 outbound deps (divergence hub)
    - Overlapping 3-cycles (A->B->C->A, B->D->E->B, etc.)
    - Self-dependency (should be handled gracefully)
    - Bidirectional dependency (A<->B)
    - Very long cycle (20 nodes)
    """
    issues = []
    base = datetime(2024, 6, 1, 9, 0, 0)

    # --- Deep chain: 100 nodes ---
    for i in range(100):
        deps = []
        if i > 0:
            deps = [{"depends_on_id": f"PD-{i-1:03d}", "type": "blocks"}]
        issues.append({
            "id": f"PD-{i:03d}",
            "title": f"Deep chain node {i}",
            "status": "blocked" if i > 0 else "open",
            "priority": 3,
            "issue_type": "task",
            "labels": ["deep-chain"],
            "created_at": (base + timedelta(hours=i)).isoformat() + "Z",
            "estimated_minutes": 30,
            "dependencies": deps,
        })

    # --- Convergence bottleneck: 50 nodes -> 1 sink ---
    sink_id = "PD-150"
    for i in range(50):
        idx = 100 + i
        issues.append({
            "id": f"PD-{idx:03d}",
            "title": f"Converge feeder {i}",
            "status": "open",
            "priority": (i % 5) + 1,
            "issue_type": "task",
            "labels": ["converge"],
            "created_at": (base + timedelta(days=5, hours=i)).isoformat() + "Z",
            "estimated_minutes": 60,
        })
    # The sink depends on all 50 feeders
    sink_deps = [{"depends_on_id": f"PD-{100+i:03d}", "type": "blocks"} for i in range(50)]
    issues.append({
        "id": sink_id,
        "title": "Convergence bottleneck sink",
        "status": "blocked",
        "priority": 1,
        "issue_type": "epic",
        "labels": ["converge", "bottleneck"],
        "created_at": (base + timedelta(days=7)).isoformat() + "Z",
        "estimated_minutes": 240,
        "dependencies": sink_deps,
    })

    # --- Divergence hub: 1 source -> 50 dependents ---
    source_id = "PD-151"
    issues.append({
        "id": source_id,
        "title": "Divergence hub source",
        "status": "open",
        "priority": 1,
        "issue_type": "epic",
        "labels": ["diverge", "hub"],
        "created_at": (base + timedelta(days=10)).isoformat() + "Z",
        "estimated_minutes": 300,
    })
    for i in range(50):
        idx = 152 + i
        issues.append({
            "id": f"PD-{idx:03d}",
            "title": f"Diverge dependent {i}",
            "status": "blocked",
            "priority": (i % 5) + 1,
            "issue_type": "task",
            "labels": ["diverge"],
            "created_at": (base + timedelta(days=10, hours=i+1)).isoformat() + "Z",
            "estimated_minutes": 45,
            "dependencies": [{"depends_on_id": source_id, "type": "blocks"}],
        })

    # --- Overlapping 3-cycles ---
    # Cycle 1: 202->203->204->202
    # Cycle 2: 203->205->206->203 (overlaps at 203)
    # Cycle 3: 204->207->208->204 (overlaps at 204)
    overlap_issues = [
        {"id": "PD-202", "deps": ["PD-204"], "title": "Overlap cycle 1 node A"},
        {"id": "PD-203", "deps": ["PD-202"], "title": "Overlap cycle 1 node B / cycle 2 node A"},
        {"id": "PD-204", "deps": ["PD-203"], "title": "Overlap cycle 1 node C / cycle 3 node A"},
        {"id": "PD-205", "deps": ["PD-203"], "title": "Overlap cycle 2 node B"},
        {"id": "PD-206", "deps": ["PD-205"], "title": "Overlap cycle 2 node C"},
        {"id": "PD-207", "deps": ["PD-204"], "title": "Overlap cycle 3 node B"},
        {"id": "PD-208", "deps": ["PD-207"], "title": "Overlap cycle 3 node C"},
    ]
    # Close cycle 2: 203 also depends on 206
    # Close cycle 3: 204 also depends on 208
    for oi in overlap_issues:
        deps = [{"depends_on_id": d, "type": "blocks"} for d in oi["deps"]]
        issues.append({
            "id": oi["id"],
            "title": oi["title"],
            "status": "blocked",
            "priority": 2,
            "issue_type": "bug",
            "labels": ["overlap-cycle"],
            "created_at": (base + timedelta(days=15)).isoformat() + "Z",
            "estimated_minutes": 90,
            "dependencies": deps,
        })
    # Add closing edges for cycles 2 and 3
    for issue in issues:
        if issue["id"] == "PD-203":
            issue["dependencies"].append({"depends_on_id": "PD-206", "type": "blocks"})
        if issue["id"] == "PD-204":
            issue["dependencies"].append({"depends_on_id": "PD-208", "type": "blocks"})

    # --- Self-dependency ---
    issues.append({
        "id": "PD-210",
        "title": "Self-dependent issue",
        "status": "blocked",
        "priority": 1,
        "issue_type": "bug",
        "labels": ["self-dep", "edge-case"],
        "created_at": (base + timedelta(days=20)).isoformat() + "Z",
        "estimated_minutes": 60,
        "dependencies": [{"depends_on_id": "PD-210", "type": "blocks"}],
    })

    # --- Bidirectional dependency ---
    issues.append({
        "id": "PD-211",
        "title": "Bidirectional A",
        "status": "blocked",
        "priority": 2,
        "issue_type": "task",
        "labels": ["bidir"],
        "created_at": (base + timedelta(days=21)).isoformat() + "Z",
        "estimated_minutes": 120,
        "dependencies": [{"depends_on_id": "PD-212", "type": "blocks"}],
    })
    issues.append({
        "id": "PD-212",
        "title": "Bidirectional B",
        "status": "blocked",
        "priority": 2,
        "issue_type": "task",
        "labels": ["bidir"],
        "created_at": (base + timedelta(days=21, hours=1)).isoformat() + "Z",
        "estimated_minutes": 120,
        "dependencies": [{"depends_on_id": "PD-211", "type": "blocks"}],
    })

    # --- Very long cycle (20 nodes) ---
    for i in range(20):
        idx = 220 + i
        next_idx = 220 + ((i + 1) % 20)
        issues.append({
            "id": f"PD-{idx:03d}",
            "title": f"Long cycle node {i}",
            "status": "blocked",
            "priority": 3,
            "issue_type": "task",
            "labels": ["long-cycle"],
            "created_at": (base + timedelta(days=25, hours=i)).isoformat() + "Z",
            "estimated_minutes": 45,
            "dependencies": [{"depends_on_id": f"PD-{next_idx:03d}", "type": "blocks"}],
        })

    # --- Dangling dependency (depends on non-existent issue) ---
    issues.append({
        "id": "PD-250",
        "title": "Depends on phantom",
        "status": "blocked",
        "priority": 1,
        "issue_type": "task",
        "labels": ["dangling"],
        "created_at": (base + timedelta(days=30)).isoformat() + "Z",
        "estimated_minutes": 60,
        "dependencies": [{"depends_on_id": "GHOST-999", "type": "blocks"}],
    })

    return issues


def gen_malformed():
    """Edge-case metadata:
    - Empty string fields
    - Zero/negative priority
    - Extremely long title
    - Unicode in all text fields
    - Null-like estimated_minutes
    - Future dates
    - Invalid date formats (should be handled gracefully)
    - Duplicate IDs (second should win or be handled)
    - Empty labels list
    - Extremely large estimated_minutes
    """
    base = datetime(2024, 1, 1, 9, 0, 0)
    issues = []

    # Empty strings everywhere
    issues.append({
        "id": "MF-001",
        "title": "",
        "description": "",
        "status": "",
        "priority": 0,
        "issue_type": "",
        "labels": [],
        "created_at": (base).isoformat() + "Z",
    })

    # Negative priority
    issues.append({
        "id": "MF-002",
        "title": "Negative priority",
        "status": "open",
        "priority": -5,
        "issue_type": "bug",
        "labels": ["edge"],
        "created_at": (base + timedelta(hours=1)).isoformat() + "Z",
    })

    # Very long title (500 chars)
    issues.append({
        "id": "MF-003",
        "title": "A" * 500,
        "status": "open",
        "priority": 3,
        "issue_type": "task",
        "labels": ["long-title"],
        "created_at": (base + timedelta(hours=2)).isoformat() + "Z",
        "estimated_minutes": 60,
    })

    # Unicode in all text fields
    issues.append({
        "id": "MF-004",
        "title": "Ünïcödé tïtlé 日本語 中文 한국어 🚀",
        "description": "Dëscríptîon with émojis 🎉🔥 and spëcial chars: <>&\"'\\n\\t",
        "status": "open",
        "priority": 2,
        "issue_type": "feature",
        "assignee": "ñoño",
        "labels": ["ünïcödé", "日本語", "emoji-🔥"],
        "created_at": (base + timedelta(hours=3)).isoformat() + "Z",
        "estimated_minutes": 120,
    })

    # Zero estimated_minutes
    issues.append({
        "id": "MF-005",
        "title": "Zero estimate",
        "status": "open",
        "priority": 1,
        "issue_type": "task",
        "labels": ["zero"],
        "created_at": (base + timedelta(hours=4)).isoformat() + "Z",
        "estimated_minutes": 0,
    })

    # Extremely large estimated_minutes
    issues.append({
        "id": "MF-006",
        "title": "Huge estimate",
        "status": "open",
        "priority": 1,
        "issue_type": "task",
        "labels": ["huge"],
        "created_at": (base + timedelta(hours=5)).isoformat() + "Z",
        "estimated_minutes": 999999,
    })

    # Future dates
    issues.append({
        "id": "MF-007",
        "title": "Future dated issue",
        "status": "open",
        "priority": 2,
        "issue_type": "task",
        "labels": ["future"],
        "created_at": "2030-12-31T23:59:59Z",
        "updated_at": "2031-01-01T00:00:00Z",
        "due_date": "2035-06-15T00:00:00Z",
        "estimated_minutes": 60,
    })

    # Closed_at before created_at (time paradox)
    issues.append({
        "id": "MF-008",
        "title": "Time paradox issue",
        "status": "closed",
        "priority": 3,
        "issue_type": "bug",
        "labels": ["paradox"],
        "created_at": "2024-06-01T12:00:00Z",
        "closed_at": "2024-01-01T00:00:00Z",
        "estimated_minutes": 30,
    })

    # Duplicate labels
    issues.append({
        "id": "MF-009",
        "title": "Duplicate labels",
        "status": "open",
        "priority": 2,
        "issue_type": "task",
        "labels": ["api", "api", "API", "Api", "backend", "backend"],
        "created_at": (base + timedelta(hours=8)).isoformat() + "Z",
        "estimated_minutes": 90,
    })

    # Many labels (30)
    issues.append({
        "id": "MF-010",
        "title": "Label explosion",
        "status": "open",
        "priority": 1,
        "issue_type": "task",
        "labels": [f"label-{i}" for i in range(30)],
        "created_at": (base + timedelta(hours=9)).isoformat() + "Z",
        "estimated_minutes": 60,
    })

    # Multiple dependencies of different types
    issues.append({
        "id": "MF-011",
        "title": "Multi-dep types",
        "status": "blocked",
        "priority": 2,
        "issue_type": "task",
        "labels": ["multi-dep"],
        "created_at": (base + timedelta(hours=10)).isoformat() + "Z",
        "estimated_minutes": 120,
        "dependencies": [
            {"depends_on_id": "MF-009", "type": "blocks"},
            {"depends_on_id": "MF-010", "type": "related"},
            {"depends_on_id": "MF-005", "type": "blocks"},
            {"depends_on_id": "MF-006", "type": ""},
        ],
    })

    # Empty dependency type
    issues.append({
        "id": "MF-012",
        "title": "Empty dep type",
        "status": "open",
        "priority": 3,
        "issue_type": "task",
        "labels": [],
        "created_at": (base + timedelta(hours=11)).isoformat() + "Z",
        "dependencies": [{"depends_on_id": "MF-001", "type": ""}],
    })

    # Issue with only ID (minimal valid)
    issues.append({"id": "MF-013"})

    # Comments with edge cases
    issues.append({
        "id": "MF-014",
        "title": "Issue with comments",
        "status": "open",
        "priority": 2,
        "issue_type": "task",
        "labels": ["comments"],
        "created_at": (base + timedelta(hours=13)).isoformat() + "Z",
        "comments": [
            {"id": 1, "issue_id": "MF-014", "body": "", "author": ""},
            {"id": 2, "issue_id": "MF-014", "body": "Normal comment", "author": "alice"},
            {"id": 3, "issue_id": "MF-014", "body": "Ünïcödé cömmënt 🚀", "author": "ñoño"},
        ],
    })

    # Very long description
    issues.append({
        "id": "MF-015",
        "title": "Long description",
        "description": "x" * 10000,
        "status": "open",
        "priority": 1,
        "issue_type": "feature",
        "labels": ["verbose"],
        "created_at": (base + timedelta(hours=14)).isoformat() + "Z",
        "estimated_minutes": 480,
    })

    # All known statuses
    for i, status in enumerate(["open", "in_progress", "blocked", "deferred", "pinned", "hooked", "review", "closed", "tombstone"]):
        issues.append({
            "id": f"MF-{100+i:03d}",
            "title": f"Status: {status}",
            "status": status,
            "priority": 3,
            "issue_type": "task",
            "labels": ["status-coverage"],
            "created_at": (base + timedelta(hours=20+i)).isoformat() + "Z",
        })

    return issues


if __name__ == "__main__":
    fixtures = {
        "stress_large_500.jsonl": gen_large_500,
        "pathological_deps.jsonl": gen_pathological,
        "malformed_metadata.jsonl": gen_malformed,
    }

    import os
    script_dir = os.path.dirname(os.path.abspath(__file__))
    manifest_path = os.path.join(script_dir, "fixture_metadata.json")

    fixture_manifest = {}
    if os.path.exists(manifest_path):
        try:
            with open(manifest_path) as f:
                manifest = json.load(f)
            fixture_manifest = {
                item.get("file", ""): item for item in manifest.get("fixtures", [])
            }
            print(
                f"Loaded fixture metadata manifest from {manifest_path} "
                f"({len(fixture_manifest)} entries)"
            )
        except Exception as e:
            print(f"WARNING: could not parse fixture metadata manifest: {e}")
    else:
        print(f"WARNING: fixture metadata manifest missing at {manifest_path}")

    for name, gen_fn in fixtures.items():
        path = os.path.join(script_dir, name)
        issues = gen_fn()
        with open(path, "w") as f:
            for issue in issues:
                f.write(jl(issue) + "\n")
        print(f"[fixture] {name}")
        print(f"  records: {len(issues)}")
        print(f"  output: {path}")

        metadata = fixture_manifest.get(name)
        if metadata:
            print(f"  kind: {metadata.get('kind', 'unknown')}")
            print(f"  origin: {metadata.get('origin', 'unknown')}")
            print(f"  provenance: {metadata.get('provenance', 'unknown')}")
            print(f"  intent: {metadata.get('intent', 'unknown')}")

            categories = metadata.get("categories", [])
            if categories:
                print(f"  categories: {', '.join(categories)}")

            signatures = metadata.get("expected_failure_signatures", [])
            if signatures:
                print("  expected failure signatures:")
                for sig in signatures:
                    print(f"    - {sig}")
        else:
            print("  metadata: MISSING entry in fixture_metadata.json")

    print("Done.")
    print("Validation:")
    print("  cargo test --test conformance stress_fixture_manifest_has_provenance_and_validated_counts")
