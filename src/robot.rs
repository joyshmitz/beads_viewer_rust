use std::collections::BTreeMap;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::Result;
use crate::cli::OutputFormat;
use crate::model::Issue;

#[derive(Debug, Clone, Serialize)]
pub struct RobotEnvelope {
    pub generated_at: String,
    pub data_hash: String,
}

#[must_use]
pub fn envelope(issues: &[Issue]) -> RobotEnvelope {
    RobotEnvelope {
        generated_at: Utc::now().to_rfc3339(),
        data_hash: compute_data_hash(issues),
    }
}

#[must_use]
pub fn compute_data_hash(issues: &[Issue]) -> String {
    let mut stable = issues
        .iter()
        .map(|issue| {
            (
                issue.id.clone(),
                issue.status.clone(),
                issue.priority,
                issue.updated_at.clone().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    stable.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    for row in stable {
        hasher.update(row.0);
        hasher.update(b"\x1f");
        hasher.update(row.1);
        hasher.update(b"\x1f");
        hasher.update(row.2.to_string());
        hasher.update(b"\x1f");
        hasher.update(row.3);
        hasher.update("\n");
    }

    let digest = hasher.finalize();
    format!("{digest:x}")[..16].to_string()
}

pub fn emit<T: Serialize>(format: OutputFormat, payload: &T) -> Result<()> {
    match format {
        // TODO(port-parity): replace this compatibility behavior with true TOON output.
        OutputFormat::Json | OutputFormat::Toon => {
            let line = serde_json::to_string(payload)?;
            println!("{line}");
        }
    }

    Ok(())
}

pub fn emit_with_stats<T: Serialize>(
    format: OutputFormat,
    payload: &T,
    show_stats: bool,
) -> Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Toon => {
            let line = serde_json::to_string(payload)?;
            println!("{line}");
            if show_stats {
                print_format_stats(&line);
            }
        }
    }

    Ok(())
}

#[must_use]
pub fn default_field_descriptions() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("score", "Composite impact score (0..1)"),
        (
            "confidence",
            "Heuristic confidence for recommendation quality (0..1)",
        ),
        (
            "unblocks",
            "Count of downstream issues immediately unblocked",
        ),
        (
            "claim_command",
            "Suggested br command to claim/start the issue",
        ),
    ])
}

// ---------------------------------------------------------------------------
// --robot-docs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct CmdDoc {
    flag: &'static str,
    description: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    key_fields: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    params: Vec<&'static str>,
    needs_issues: bool,
}

fn robot_command_docs() -> BTreeMap<&'static str, CmdDoc> {
    BTreeMap::from([
        (
            "robot-triage",
            CmdDoc {
                flag: "--robot-triage",
                description: "Unified triage: top picks, recommendations, quick wins, blockers, project health, velocity.",
                key_fields: vec![
                    "triage.quick_ref.top_picks",
                    "triage.recommendations",
                    "triage.quick_wins",
                    "triage.blockers_to_clear",
                    "triage.project_health",
                ],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-next",
            CmdDoc {
                flag: "--robot-next",
                description: "Single top recommendation with claim/show commands.",
                key_fields: vec![
                    "id",
                    "title",
                    "score",
                    "reasons",
                    "unblocks",
                    "claim_command",
                    "show_command",
                ],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-plan",
            CmdDoc {
                flag: "--robot-plan",
                description: "Dependency-respecting execution plan with parallel tracks.",
                key_fields: vec!["tracks", "items", "unblocks", "summary"],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-insights",
            CmdDoc {
                flag: "--robot-insights",
                description: "Deep graph analysis: PageRank, betweenness, HITS, eigenvector, k-core, cycle detection.",
                key_fields: vec![
                    "pagerank",
                    "betweenness",
                    "hits",
                    "eigenvector",
                    "k_core",
                    "cycles",
                ],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-priority",
            CmdDoc {
                flag: "--robot-priority",
                description: "Priority misalignment detection: items whose graph importance differs from assigned priority.",
                key_fields: vec!["misalignments", "suggestions"],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-triage-by-track",
            CmdDoc {
                flag: "--robot-triage-by-track",
                description: "Triage grouped by independent parallel execution tracks.",
                key_fields: vec!["tracks[].track_id", "tracks[].top_pick", "tracks[].items"],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-triage-by-label",
            CmdDoc {
                flag: "--robot-triage-by-label",
                description: "Triage grouped by label for area-focused agents.",
                key_fields: vec!["labels[].label", "labels[].top_pick", "labels[].items"],
                params: vec![],
                needs_issues: true,
            },
        ),
        (
            "robot-alerts",
            CmdDoc {
                flag: "--robot-alerts",
                description: "Stale issues, blocking cascades, priority mismatches.",
                key_fields: vec!["alerts", "severity", "affected_issues"],
                params: vec![
                    "--severity info|warning|critical",
                    "--alert-type <type>",
                    "--alert-label <label>",
                ],
                needs_issues: true,
            },
        ),
        (
            "robot-suggest",
            CmdDoc {
                flag: "--robot-suggest",
                description: "Smart suggestions: potential duplicates, missing dependencies, label assignments, cycle warnings.",
                key_fields: vec!["suggestions", "type", "confidence"],
                params: vec![
                    "--suggest-type duplicate|dependency|label|cycle",
                    "--suggest-confidence 0.0-1.0",
                    "--suggest-bead <id>",
                ],
                needs_issues: true,
            },
        ),
        (
            "robot-schema",
            CmdDoc {
                flag: "--robot-schema",
                description: "JSON Schema definitions for all robot command outputs.",
                key_fields: vec!["schema_version", "envelope", "commands"],
                params: vec!["--schema-command <cmd>"],
                needs_issues: false,
            },
        ),
        (
            "robot-docs",
            CmdDoc {
                flag: "--robot-docs <topic>",
                description: "Machine-readable JSON documentation. Topics: guide, commands, examples, env, exit-codes, all.",
                key_fields: vec![],
                params: vec![],
                needs_issues: false,
            },
        ),
        (
            "robot-history",
            CmdDoc {
                flag: "--robot-history",
                description: "Bead-to-commit correlations from git history.",
                key_fields: vec!["correlations", "confidence", "commit_sha", "bead_id"],
                params: vec![
                    "--bead-history <id>",
                    "--history-since <date>",
                    "--history-limit <n>",
                    "--min-confidence 0.0-1.0",
                ],
                needs_issues: true,
            },
        ),
        (
            "robot-diff",
            CmdDoc {
                flag: "--robot-diff",
                description: "Changes since a historical point (commit, branch, tag, or date).",
                key_fields: vec![],
                params: vec!["--diff-since <ref>"],
                needs_issues: true,
            },
        ),
        (
            "robot-graph",
            CmdDoc {
                flag: "--robot-graph",
                description: "Dependency graph export in JSON, DOT, or Mermaid format.",
                key_fields: vec![],
                params: vec![
                    "--graph-format json|dot|mermaid",
                    "--graph-root <id>",
                    "--graph-depth <n>",
                ],
                needs_issues: true,
            },
        ),
        (
            "robot-forecast",
            CmdDoc {
                flag: "--robot-forecast <id|all>",
                description: "ETA predictions for bead completion.",
                key_fields: vec![],
                params: vec![
                    "--forecast-label <label>",
                    "--forecast-sprint <id>",
                    "--forecast-agents <n>",
                ],
                needs_issues: true,
            },
        ),
        (
            "robot-capacity",
            CmdDoc {
                flag: "--robot-capacity",
                description: "Capacity simulation and completion projections.",
                key_fields: vec![],
                params: vec!["--agents <n>", "--capacity-label <label>"],
                needs_issues: true,
            },
        ),
        (
            "robot-burndown",
            CmdDoc {
                flag: "--robot-burndown <sprint|current>",
                description: "Sprint burndown data.",
                key_fields: vec![],
                params: vec![],
                needs_issues: true,
            },
        ),
    ])
}

#[must_use]
pub fn generate_robot_docs(topic: &str) -> Value {
    let now = Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION");

    let mut result = serde_json::json!({
        "generated_at": now,
        "output_format": "json",
        "version": version,
        "topic": topic,
    });

    let guide = serde_json::json!({
        "description": "bvr (Beads Viewer Rust) provides structural analysis of the beads issue tracker DAG. It is the primary interface for AI agents to understand project state, plan work, and discover high-impact tasks.",
        "quickstart": [
            "bvr --robot-triage               # Full triage with recommendations",
            "bvr --robot-next                  # Single top pick for immediate work",
            "bvr --robot-plan                  # Dependency-respecting execution plan",
            "bvr --robot-insights              # Deep graph analysis (PageRank, betweenness, etc.)",
            "bvr --robot-triage-by-track       # Parallel work streams for multi-agent coordination",
            "bvr --robot-schema                # JSON Schema definitions for all commands",
        ],
        "data_source": ".beads/issues.jsonl and git history (correlations)",
        "output_modes": {
            "json": "Default structured output",
            "toon": "Token-optimized notation (saves ~30-50% tokens)",
        },
    });

    let commands =
        serde_json::to_value(robot_command_docs()).unwrap_or_else(|_| serde_json::json!({}));

    let examples = serde_json::json!([
        {"description": "Get top 3 picks for immediate work", "command": "bvr --robot-triage | jq '.triage.quick_ref.top_picks[:3]'"},
        {"description": "Claim the top recommendation", "command": "bvr --robot-next | jq -r '.claim_command' | sh"},
        {"description": "Find high-impact blockers to clear", "command": "bvr --robot-triage | jq '.triage.blockers_to_clear | map(.id)'"},
        {"description": "Get bug-only recommendations", "command": "bvr --robot-triage | jq '.triage.recommendations[] | select(.type == \"bug\")'"},
        {"description": "Multi-agent: top pick per parallel track", "command": "bvr --robot-triage-by-track | jq '.triage.recommendations_by_track[].top_pick'"},
        {"description": "Get TOON output (saves tokens)", "command": "bvr --robot-triage --format toon"},
        {"description": "Use env for default format", "command": "BV_OUTPUT_FORMAT=toon bvr --robot-triage"},
        {"description": "Show token savings estimate", "command": "bvr --robot-triage --format toon --stats"},
    ]);

    let env_vars = serde_json::json!({
        "BV_OUTPUT_FORMAT": "Default output format: json or toon (overridden by --format)",
        "TOON_DEFAULT_FORMAT": "Fallback format if BV_OUTPUT_FORMAT not set",
        "TOON_STATS": "Set to 1 to show JSON vs TOON token estimates on stderr",
        "TOON_KEY_FOLDING": "TOON key folding mode",
        "TOON_INDENT": "TOON indentation level (0-16)",
        "BV_PRETTY_JSON": "Set to 1 for indented JSON output",
        "BV_ROBOT": "Set to 1 to force robot mode (clean stdout)",
        "BV_SEARCH_MODE": "Search mode: text or hybrid",
        "BV_SEARCH_PRESET": "Hybrid search preset name",
    });

    let exit_codes = serde_json::json!({
        "0": "Success",
        "1": "Error (general failure, drift critical)",
        "2": "Invalid arguments or drift warning",
    });

    match topic {
        "guide" => {
            result["guide"] = guide;
        }
        "commands" => {
            result["commands"] = commands;
        }
        "examples" => {
            result["examples"] = examples;
        }
        "env" => {
            result["environment_variables"] = env_vars;
        }
        "exit-codes" => {
            result["exit_codes"] = exit_codes;
        }
        "all" => {
            result["guide"] = guide;
            result["commands"] = commands;
            result["examples"] = examples;
            result["environment_variables"] = env_vars;
            result["exit_codes"] = exit_codes;
        }
        _ => {
            result["error"] = Value::String(format!("Unknown topic: {topic}"));
            result["available_topics"] =
                serde_json::json!(["guide", "commands", "examples", "env", "exit-codes", "all"]);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// --robot-schema
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RobotSchemas {
    pub schema_version: String,
    pub generated_at: String,
    pub envelope: Value,
    pub commands: BTreeMap<String, Value>,
}

fn schema_prop(type_str: &str) -> Value {
    serde_json::json!({"type": type_str})
}

fn schema_prop_dt() -> Value {
    serde_json::json!({"type": "string", "format": "date-time"})
}

#[must_use]
pub fn generate_robot_schemas() -> RobotSchemas {
    let now = Utc::now().to_rfc3339();

    let envelope = serde_json::json!({
        "type": "object",
        "properties": {
            "generated_at": {
                "type": "string",
                "format": "date-time",
                "description": "ISO 8601 timestamp when output was generated",
            },
            "data_hash": {
                "type": "string",
                "description": "Fingerprint of source beads.jsonl for cache validation",
            },
            "output_format": {
                "type": "string",
                "enum": ["json", "toon"],
                "description": "Output format used (json or toon)",
            },
            "version": {
                "type": "string",
                "description": "bvr version that generated this output",
            },
        },
        "required": ["generated_at", "data_hash"],
    });

    let mut commands = BTreeMap::new();

    commands.insert("robot-triage".to_string(), serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Robot Triage Output",
        "description": "Unified triage recommendations with quick picks, blockers, and project health",
        "type": "object",
        "properties": {
            "generated_at": schema_prop_dt(),
            "data_hash": schema_prop("string"),
            "triage": {
                "type": "object",
                "properties": {
                    "meta": {
                        "type": "object",
                        "properties": {
                            "version": schema_prop("string"),
                            "generated_at": schema_prop("string"),
                            "phase2_ready": schema_prop("boolean"),
                            "issue_count": schema_prop("integer"),
                        }
                    },
                    "quick_ref": {
                        "type": "object",
                        "properties": {
                            "open_count": schema_prop("integer"),
                            "actionable_count": schema_prop("integer"),
                            "blocked_count": schema_prop("integer"),
                            "in_progress_count": schema_prop("integer"),
                            "top_picks": {
                                "type": "array",
                                "items": {"$ref": "#/$defs/recommendation"}
                            }
                        }
                    },
                    "recommendations": {"type": "array", "items": {"$ref": "#/$defs/recommendation"}},
                    "quick_wins": {"type": "array"},
                    "blockers_to_clear": {"type": "array"},
                    "project_health": {"type": "object"},
                    "commands": {"type": "object"},
                }
            },
            "usage_hints": {"type": "array", "items": schema_prop("string")},
        },
        "$defs": {
            "recommendation": {
                "type": "object",
                "properties": {
                    "id": schema_prop("string"),
                    "title": schema_prop("string"),
                    "type": schema_prop("string"),
                    "status": schema_prop("string"),
                    "priority": schema_prop("integer"),
                    "labels": {"type": "array", "items": schema_prop("string")},
                    "score": schema_prop("number"),
                    "reasons": {"type": "array", "items": schema_prop("string")},
                    "unblocks": schema_prop("integer"),
                },
                "required": ["id", "title", "score"],
            }
        }
    }));

    commands.insert(
        "robot-next".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Next Output",
            "description": "Single top pick recommendation with claim command",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "id": schema_prop("string"),
                "title": schema_prop("string"),
                "score": schema_prop("number"),
                "reasons": {"type": "array", "items": schema_prop("string")},
                "unblocks": schema_prop("integer"),
                "claim_command": schema_prop("string"),
                "show_command": schema_prop("string"),
            },
            "required": ["generated_at", "data_hash", "id", "title", "score"],
        }),
    );

    commands.insert(
        "robot-plan".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Plan Output",
            "description": "Dependency-respecting execution plan with parallel tracks",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "plan": {
                    "type": "object",
                    "properties": {
                        "phases": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "phase": schema_prop("integer"),
                                    "issues": {"type": "array"},
                                }
                            }
                        },
                        "summary": {"type": "object"},
                    }
                },
                "status": {"type": "object"},
                "usage_hints": {"type": "array"},
            },
        }),
    );

    commands.insert("robot-insights".to_string(), serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Robot Insights Output",
        "description": "Full graph analysis metrics including PageRank, betweenness, HITS, cycles",
        "type": "object",
        "properties": {
            "generated_at": schema_prop_dt(),
            "data_hash": schema_prop("string"),
            "Stats": {"type": "object"},
            "Cycles": {"type": "array"},
            "Keystones": {"type": "array"},
            "Bottlenecks": {"type": "array"},
            "Influencers": {"type": "array"},
            "Hubs": {"type": "array"},
            "Authorities": {"type": "array"},
            "Orphans": {"type": "array"},
            "Cores": {"type": "object"},
            "Articulation": {"type": "array"},
            "Slack": {"type": "object"},
            "Velocity": {"type": "object"},
            "status": {"type": "object"},
            "advanced_insights": {"type": "object"},
            "usage_hints": {"type": "array"},
        },
    }));

    commands.insert(
        "robot-priority".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Priority Output",
            "description": "Priority misalignment detection with recommendations",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "recommendations": {"type": "array"},
                "status": {"type": "object"},
                "usage_hints": {"type": "array"},
            },
        }),
    );

    commands.insert(
        "robot-graph".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Graph Output",
            "description": "Dependency graph in JSON/DOT/Mermaid format",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "format": {"type": "string", "enum": ["json", "dot", "mermaid"]},
                "nodes": {"type": "array"},
                "edges": {"type": "array"},
                "stats": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-diff".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Diff Output",
            "description": "Changes since a historical point (commit, branch, date)",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "since": schema_prop("string"),
                "since_commit": schema_prop("string"),
                "new": {"type": "array"},
                "closed": {"type": "array"},
                "modified": {"type": "array"},
                "cycles": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-alerts".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Alerts Output",
            "description": "Stale issues, blocking cascades, priority mismatches",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "alerts": {"type": "array"},
                "summary": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-suggest".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Suggest Output",
            "description": "Smart suggestions for duplicates, dependencies, labels, cycle breaks",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "suggestions": {"type": "array"},
                "counts": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-burndown".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Burndown Output",
            "description": "Sprint burndown data with scope changes and at-risk items",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "sprint_id": schema_prop("string"),
                "burndown": {"type": "array"},
                "scope_changes": {"type": "array"},
                "at_risk": {"type": "array"},
            },
        }),
    );

    commands.insert(
        "robot-forecast".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Forecast Output",
            "description": "ETA predictions with dependency-aware scheduling",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "forecasts": {"type": "array"},
                "methodology": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-history".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot History Output",
            "description": "Bead-to-commit correlations from git history",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "beads": {"type": "array"},
                "stats": {"type": "object"},
            },
        }),
    );

    commands.insert(
        "robot-capacity".to_string(),
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Robot Capacity Output",
            "description": "Capacity simulation and completion projections",
            "type": "object",
            "properties": {
                "generated_at": schema_prop_dt(),
                "data_hash": schema_prop("string"),
                "capacity": {"type": "object"},
                "projections": {"type": "array"},
            },
        }),
    );

    RobotSchemas {
        schema_version: "1.0.0".to_string(),
        generated_at: now,
        envelope,
        commands,
    }
}

// ---------------------------------------------------------------------------
// --stats (format token estimation)
// ---------------------------------------------------------------------------

#[must_use]
pub fn estimate_tokens(s: &str) -> usize {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed.len().div_ceil(4)
}

pub fn print_format_stats(json_output: &str) {
    let json_tokens = estimate_tokens(json_output);
    eprintln!("Format stats:");
    eprintln!(
        "  JSON: ~{json_tokens} tokens ({} bytes)",
        json_output.len()
    );
    eprintln!("  (TOON format not yet implemented; will show savings when available)");
}
