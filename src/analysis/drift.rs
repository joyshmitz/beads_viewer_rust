use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::graph::{GraphMetrics, IssueGraph};
use crate::model::Issue;

// ---------------------------------------------------------------------------
// Baseline
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineMetricItem {
    pub id: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineTopMetrics {
    #[serde(default)]
    pub pagerank: Vec<BaselineMetricItem>,
    #[serde(default)]
    pub betweenness: Vec<BaselineMetricItem>,
    #[serde(default)]
    pub hubs: Vec<BaselineMetricItem>,
    #[serde(default)]
    pub authorities: Vec<BaselineMetricItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineGraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f64,
    pub open_count: usize,
    pub closed_count: usize,
    pub blocked_count: usize,
    pub cycle_count: usize,
    pub actionable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub version: u32,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub stats: BaselineGraphStats,
    pub top_metrics: BaselineTopMetrics,
    pub cycles: Vec<Vec<String>>,
}

impl Baseline {
    /// Build a baseline snapshot from current issues, graph, and metrics.
    pub fn from_current(
        issues: &[Issue],
        graph: &IssueGraph,
        metrics: &GraphMetrics,
        description: &str,
    ) -> Self {
        let open_count = issues.iter().filter(|i| i.is_open_like()).count();
        let closed_count = issues.len() - open_count;
        let blocked_count = issues
            .iter()
            .filter(|i| i.is_open_like() && !graph.open_blockers(&i.id).is_empty())
            .count();
        let actionable_count = graph.actionable_ids().len();

        let n = graph.node_count();
        let e = graph.edge_count();
        let density = if n > 1 {
            (2.0 * e as f64) / (n as f64 * (n as f64 - 1.0))
        } else {
            0.0
        };

        let top_n = 10;

        Self {
            version: 1,
            created_at: chrono_now(),
            description: description.to_string(),
            stats: BaselineGraphStats {
                node_count: n,
                edge_count: e,
                density,
                open_count,
                closed_count,
                blocked_count,
                cycle_count: metrics.cycles.len(),
                actionable_count,
            },
            top_metrics: BaselineTopMetrics {
                pagerank: top_metric_items(&metrics.pagerank, top_n),
                betweenness: top_metric_items(&metrics.betweenness, top_n),
                hubs: top_metric_items(&metrics.hubs, top_n),
                authorities: top_metric_items(&metrics.authorities, top_n),
            },
            cycles: metrics.cycles.clone(),
        }
    }

    /// Save baseline to `.bv/baseline.json` under the given project directory.
    pub fn save(&self, project_dir: &Path) -> Result<PathBuf, String> {
        let dir = project_dir.join(".bv");
        fs::create_dir_all(&dir).map_err(|e| format!("failed to create .bv dir: {e}"))?;

        let path = dir.join("baseline.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize baseline: {e}"))?;
        fs::write(&path, json).map_err(|e| format!("failed to write baseline: {e}"))?;
        Ok(path)
    }

    /// Load baseline from `.bv/baseline.json` under the given project directory.
    pub fn load(project_dir: &Path) -> Result<Self, String> {
        let path = project_dir.join(".bv").join("baseline.json");
        let content =
            fs::read_to_string(&path).map_err(|e| format!("failed to read baseline: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("failed to parse baseline: {e}"))
    }
}

fn top_metric_items(values: &HashMap<String, f64>, limit: usize) -> Vec<BaselineMetricItem> {
    let mut items: Vec<BaselineMetricItem> = values
        .iter()
        .map(|(id, value)| BaselineMetricItem {
            id: id.clone(),
            value: *value,
        })
        .collect();

    items.sort_by(|a, b| b.value.total_cmp(&a.value).then_with(|| a.id.cmp(&b.id)));
    items.truncate(limit);
    items
}

// ---------------------------------------------------------------------------
// Drift Detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DriftAlert {
    #[serde(rename = "type")]
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub delta: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftSummary {
    pub critical: usize,
    pub warning: usize,
    pub info: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftBaselineInfo {
    pub created_at: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftResult {
    pub has_drift: bool,
    pub exit_code: u8,
    pub summary: DriftSummary,
    pub alerts: Vec<DriftAlert>,
    pub baseline: DriftBaselineInfo,
}

#[derive(Debug, Serialize)]
pub struct RobotDriftOutput {
    pub generated_at: String,
    pub data_hash: String,
    pub output_format: String,
    pub version: String,
    #[serde(flatten)]
    pub result: DriftResult,
}

/// Compare current state against a saved baseline and produce drift alerts.
pub fn compute_drift(
    baseline: &Baseline,
    issues: &[Issue],
    graph: &IssueGraph,
    metrics: &GraphMetrics,
) -> DriftResult {
    let current = Baseline::from_current(issues, graph, metrics, "");
    let mut alerts = Vec::new();

    // 1. New cycles (CRITICAL)
    let new_cycles = current
        .stats
        .cycle_count
        .saturating_sub(baseline.stats.cycle_count);
    if new_cycles > 0 {
        let details: Vec<String> = current
            .cycles
            .iter()
            .skip(baseline.cycles.len())
            .map(|cycle| cycle.join(" -> "))
            .collect();
        alerts.push(DriftAlert {
            alert_type: "new_cycle".to_string(),
            severity: "critical".to_string(),
            message: format!("{new_cycles} new cycle(s) detected"),
            baseline_value: baseline.stats.cycle_count as f64,
            current_value: current.stats.cycle_count as f64,
            delta: new_cycles as f64,
            details,
        });
    }

    // 2. Density growth (WARNING if >= 50% increase)
    if baseline.stats.density > 0.0 {
        let pct_change =
            ((current.stats.density - baseline.stats.density) / baseline.stats.density) * 100.0;
        if pct_change >= 50.0 {
            alerts.push(DriftAlert {
                alert_type: "density_growth".to_string(),
                severity: "warning".to_string(),
                message: format!("Graph density increased by {pct_change:.0}%"),
                baseline_value: baseline.stats.density,
                current_value: current.stats.density,
                delta: pct_change,
                details: Vec::new(),
            });
        } else if pct_change >= 20.0 {
            alerts.push(DriftAlert {
                alert_type: "density_growth".to_string(),
                severity: "info".to_string(),
                message: format!("Graph density increased by {pct_change:.0}%"),
                baseline_value: baseline.stats.density,
                current_value: current.stats.density,
                delta: pct_change,
                details: Vec::new(),
            });
        }
    }

    // 3. Blocked count increase (WARNING if delta >= 5)
    let blocked_delta = current.stats.blocked_count as i64 - baseline.stats.blocked_count as i64;
    if blocked_delta >= 5 {
        alerts.push(DriftAlert {
            alert_type: "blocked_increase".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "Blocked issues increased by {blocked_delta} ({} -> {})",
                baseline.stats.blocked_count, current.stats.blocked_count
            ),
            baseline_value: baseline.stats.blocked_count as f64,
            current_value: current.stats.blocked_count as f64,
            delta: blocked_delta as f64,
            details: Vec::new(),
        });
    }

    // 4. Actionable count changes (WARNING if decrease >= 30%)
    if baseline.stats.actionable_count > 0 {
        let pct_change = ((current.stats.actionable_count as f64
            - baseline.stats.actionable_count as f64)
            / baseline.stats.actionable_count as f64)
            * 100.0;
        if pct_change <= -30.0 {
            alerts.push(DriftAlert {
                alert_type: "actionable_change".to_string(),
                severity: "warning".to_string(),
                message: format!(
                    "Actionable issues decreased by {:.0}% ({} -> {})",
                    -pct_change, baseline.stats.actionable_count, current.stats.actionable_count
                ),
                baseline_value: baseline.stats.actionable_count as f64,
                current_value: current.stats.actionable_count as f64,
                delta: pct_change,
                details: Vec::new(),
            });
        } else if pct_change >= 20.0 {
            alerts.push(DriftAlert {
                alert_type: "actionable_change".to_string(),
                severity: "info".to_string(),
                message: format!(
                    "Actionable issues increased by {pct_change:.0}% ({} -> {})",
                    baseline.stats.actionable_count, current.stats.actionable_count
                ),
                baseline_value: baseline.stats.actionable_count as f64,
                current_value: current.stats.actionable_count as f64,
                delta: pct_change,
                details: Vec::new(),
            });
        }
    }

    // 5. Node count change (INFO)
    let node_delta = current.stats.node_count as i64 - baseline.stats.node_count as i64;
    if node_delta != 0 && baseline.stats.node_count > 0 {
        let pct = (node_delta.unsigned_abs() as f64 / baseline.stats.node_count as f64) * 100.0;
        if pct >= 25.0 {
            let direction = if node_delta > 0 {
                "increased"
            } else {
                "decreased"
            };
            alerts.push(DriftAlert {
                alert_type: "node_count_change".to_string(),
                severity: "info".to_string(),
                message: format!(
                    "Issue count {direction} by {pct:.0}% ({} -> {})",
                    baseline.stats.node_count, current.stats.node_count
                ),
                baseline_value: baseline.stats.node_count as f64,
                current_value: current.stats.node_count as f64,
                delta: node_delta as f64,
                details: Vec::new(),
            });
        }
    }

    // 6. Edge count change (INFO)
    let edge_delta = current.stats.edge_count as i64 - baseline.stats.edge_count as i64;
    if edge_delta != 0 && baseline.stats.edge_count > 0 {
        let pct = (edge_delta.unsigned_abs() as f64 / baseline.stats.edge_count as f64) * 100.0;
        if pct >= 25.0 {
            let direction = if edge_delta > 0 {
                "increased"
            } else {
                "decreased"
            };
            alerts.push(DriftAlert {
                alert_type: "edge_count_change".to_string(),
                severity: "info".to_string(),
                message: format!(
                    "Dependency count {direction} by {pct:.0}% ({} -> {})",
                    baseline.stats.edge_count, current.stats.edge_count
                ),
                baseline_value: baseline.stats.edge_count as f64,
                current_value: current.stats.edge_count as f64,
                delta: edge_delta as f64,
                details: Vec::new(),
            });
        }
    }

    // 7. PageRank ranking shift (WARNING if top IDs changed significantly)
    check_metric_drift(
        "pagerank_change",
        &baseline.top_metrics.pagerank,
        &current.top_metrics.pagerank,
        &mut alerts,
    );

    // Sort alerts by severity (critical > warning > info), then by type
    alerts.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then_with(|| a.alert_type.cmp(&b.alert_type))
    });

    let critical = alerts.iter().filter(|a| a.severity == "critical").count();
    let warning = alerts.iter().filter(|a| a.severity == "warning").count();
    let info = alerts.iter().filter(|a| a.severity == "info").count();

    let has_drift = critical > 0 || warning > 0;
    let exit_code = if critical > 0 {
        1
    } else if warning > 0 {
        2
    } else {
        0
    };

    DriftResult {
        has_drift,
        exit_code,
        summary: DriftSummary {
            critical,
            warning,
            info,
        },
        alerts,
        baseline: DriftBaselineInfo {
            created_at: baseline.created_at.clone(),
            description: baseline.description.clone(),
        },
    }
}

fn check_metric_drift(
    alert_type: &str,
    baseline_items: &[BaselineMetricItem],
    current_items: &[BaselineMetricItem],
    alerts: &mut Vec<DriftAlert>,
) {
    if baseline_items.is_empty() || current_items.is_empty() {
        return;
    }

    // Compare top-5 IDs: how many are different?
    let baseline_top5: Vec<&str> = baseline_items
        .iter()
        .take(5)
        .map(|i| i.id.as_str())
        .collect();
    let current_top5: Vec<&str> = current_items
        .iter()
        .take(5)
        .map(|i| i.id.as_str())
        .collect();

    let changed = baseline_top5
        .iter()
        .filter(|id| !current_top5.contains(id))
        .count();

    if changed >= 3 {
        let details: Vec<String> = baseline_top5
            .iter()
            .filter(|id| !current_top5.contains(id))
            .map(|id| format!("{id} dropped from top-5"))
            .collect();
        alerts.push(DriftAlert {
            alert_type: alert_type.to_string(),
            severity: "warning".to_string(),
            message: format!("{changed} of top-5 rankings changed"),
            baseline_value: 5.0,
            current_value: (5 - changed) as f64,
            delta: changed as f64,
            details,
        });
    }
}

const fn severity_rank(severity: &str) -> u8 {
    match severity.as_bytes() {
        b"critical" => 0,
        b"warning" => 1,
        _ => 2, // info
    }
}

fn chrono_now() -> String {
    // Simple UTC timestamp without chrono dependency
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Convert epoch seconds to ISO-8601
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Calculate year/month/day from days since epoch
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let mut remaining = days_since_epoch;
    let mut year = 1970;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days in &month_days {
        if remaining < *days {
            break;
        }
        remaining -= days;
        month += 1;
    }

    (year, month, remaining + 1)
}

const fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_baseline(
        cycle_count: usize,
        blocked_count: usize,
        actionable_count: usize,
        density: f64,
        node_count: usize,
        edge_count: usize,
    ) -> Baseline {
        Baseline {
            version: 1,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            description: "test baseline".to_string(),
            stats: BaselineGraphStats {
                node_count,
                edge_count,
                density,
                open_count: node_count,
                closed_count: 0,
                blocked_count,
                cycle_count,
                actionable_count,
            },
            top_metrics: BaselineTopMetrics {
                pagerank: vec![
                    BaselineMetricItem {
                        id: "A".to_string(),
                        value: 0.5,
                    },
                    BaselineMetricItem {
                        id: "B".to_string(),
                        value: 0.3,
                    },
                ],
                betweenness: Vec::new(),
                hubs: Vec::new(),
                authorities: Vec::new(),
            },
            cycles: Vec::new(),
        }
    }

    fn make_issues_and_graph(count: usize) -> (Vec<Issue>, IssueGraph, GraphMetrics) {
        let issues: Vec<Issue> = (0..count)
            .map(|i| Issue {
                id: format!("I-{i}"),
                title: format!("Issue {i}"),
                status: "open".to_string(),
                priority: 1,
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        (issues, graph, metrics)
    }

    #[test]
    fn no_drift_when_identical() {
        let (issues, graph, metrics) = make_issues_and_graph(5);
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "test");
        let result = compute_drift(&baseline, &issues, &graph, &metrics);

        assert!(!result.has_drift);
        assert_eq!(result.exit_code, 0);
        assert!(result.alerts.is_empty());
    }

    #[test]
    fn detects_new_cycles() {
        let (issues, graph, metrics) = make_issues_and_graph(3);
        let mut baseline = Baseline::from_current(&issues, &graph, &metrics, "test");
        baseline.stats.cycle_count = 0;
        baseline.cycles.clear();

        // Simulate current having a cycle
        let mut current_metrics = metrics.clone();
        current_metrics.cycles = vec![vec!["A".to_string(), "B".to_string(), "A".to_string()]];

        let result = compute_drift(&baseline, &issues, &graph, &current_metrics);
        assert!(result.has_drift);
        assert_eq!(result.exit_code, 1);
        assert!(result.alerts.iter().any(|a| a.alert_type == "new_cycle"));
    }

    #[test]
    fn detects_blocked_increase() {
        let (issues, graph, metrics) = make_issues_and_graph(10);
        let mut baseline = Baseline::from_current(&issues, &graph, &metrics, "test");
        baseline.stats.blocked_count = 0; // Pretend no blockers in baseline

        // Create current with 6 blocked (delta = 6 >= 5 threshold)
        let issues_with_blockers: Vec<Issue> = (0..10)
            .map(|i| {
                let mut issue = Issue {
                    id: format!("I-{i}"),
                    title: format!("Issue {i}"),
                    status: if i < 6 { "blocked" } else { "open" }.to_string(),
                    priority: 1,
                    ..Issue::default()
                };
                if i < 6 {
                    issue.dependencies = vec![crate::model::Dependency {
                        issue_id: format!("I-{i}"),
                        depends_on_id: format!("I-{}", i + 4),
                        dep_type: "blocks".to_string(),
                        ..crate::model::Dependency::default()
                    }];
                }
                issue
            })
            .collect();
        let graph2 = IssueGraph::build(&issues_with_blockers);
        let metrics2 = graph2.compute_metrics();

        let result = compute_drift(&baseline, &issues_with_blockers, &graph2, &metrics2);
        let blocked_alerts: Vec<&DriftAlert> = result
            .alerts
            .iter()
            .filter(|a| a.alert_type == "blocked_increase")
            .collect();
        assert!(
            !blocked_alerts.is_empty(),
            "Expected blocked_increase alert"
        );
    }

    #[test]
    fn severity_ordering() {
        assert!(severity_rank("critical") < severity_rank("warning"));
        assert!(severity_rank("warning") < severity_rank("info"));
    }

    #[test]
    fn baseline_serialization_roundtrip() {
        let baseline = make_baseline(0, 2, 5, 0.1, 10, 8);
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        let restored: Baseline = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.version, 1);
        assert_eq!(restored.stats.node_count, 10);
        assert_eq!(restored.stats.blocked_count, 2);
        assert_eq!(restored.top_metrics.pagerank.len(), 2);
    }

    #[test]
    fn chrono_now_format() {
        let now = chrono_now();
        assert!(now.contains('T'));
        assert!(now.ends_with('Z'));
        assert_eq!(now.len(), 20);
    }

    #[test]
    fn baseline_from_current_captures_stats() {
        let (issues, graph, metrics) = make_issues_and_graph(5);
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "snapshot");

        assert_eq!(baseline.version, 1);
        assert_eq!(baseline.stats.node_count, 5);
        assert_eq!(baseline.stats.open_count, 5);
        assert_eq!(baseline.stats.closed_count, 0);
        assert_eq!(baseline.description, "snapshot");
    }
}
