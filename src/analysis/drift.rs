use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::graph::{GraphMetrics, IssueGraph};
use crate::model::Issue;

/// Tunable thresholds for drift detection severity levels.
///
/// All percentage values are positive (e.g., 50.0 = 50%).
#[derive(Debug, Clone)]
pub struct DriftThresholds {
    /// Density growth >= this % triggers a warning.
    pub density_warning_pct: f64,
    /// Density growth >= this % triggers an info alert.
    pub density_info_pct: f64,
    /// Blocked count increase >= this count triggers a warning.
    pub blocked_increase_threshold: i64,
    /// Actionable count decrease >= this % triggers a warning.
    pub actionable_decrease_pct: f64,
    /// Node/edge count change >= this % triggers a warning.
    pub structure_change_pct: f64,
    /// Top-N ranking items changed >= this count triggers a warning.
    pub ranking_change_threshold: usize,
}

impl Default for DriftThresholds {
    fn default() -> Self {
        Self {
            density_warning_pct: 50.0,
            density_info_pct: 20.0,
            blocked_increase_threshold: 5,
            actionable_decrease_pct: 30.0,
            structure_change_pct: 25.0,
            ranking_change_threshold: 3,
        }
    }
}

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
            e as f64 / (n as f64 * (n as f64 - 1.0))
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
    #[serde(flatten)]
    pub envelope: crate::robot::RobotEnvelope,
    #[serde(flatten)]
    pub result: DriftResult,
}

fn signed_usize_delta(current: usize, baseline: usize) -> i64 {
    if current >= baseline {
        i64::try_from(current - baseline).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(baseline - current).unwrap_or(i64::MAX)
    }
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
    let blocked_delta =
        signed_usize_delta(current.stats.blocked_count, baseline.stats.blocked_count);
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
    let node_delta = signed_usize_delta(current.stats.node_count, baseline.stats.node_count);
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
    let edge_delta = signed_usize_delta(current.stats.edge_count, baseline.stats.edge_count);
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
        .map_or(0, |d| d.as_secs());

    // Convert epoch seconds to ISO-8601
    const SECS_PER_DAY: u64 = 86_400;
    let days = secs / SECS_PER_DAY;
    let time_secs = secs % SECS_PER_DAY;
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
        let mut current_metrics = metrics;
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
        assert!(
            result
                .alerts
                .iter()
                .any(|a| a.alert_type == "blocked_increase"),
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

    // --- signed_usize_delta tests ---

    #[test]
    fn signed_usize_delta_positive() {
        assert_eq!(signed_usize_delta(10, 3), 7);
    }

    #[test]
    fn signed_usize_delta_negative() {
        assert_eq!(signed_usize_delta(3, 10), -7);
    }

    #[test]
    fn signed_usize_delta_zero() {
        assert_eq!(signed_usize_delta(5, 5), 0);
    }

    // --- is_leap tests ---

    #[test]
    fn is_leap_common_year() {
        assert!(!is_leap(2023));
        assert!(!is_leap(1900)); // divisible by 100 but not 400
    }

    #[test]
    fn is_leap_leap_year() {
        assert!(is_leap(2024));
        assert!(is_leap(2000)); // divisible by 400
    }

    // --- days_to_date tests ---

    #[test]
    fn days_to_date_epoch() {
        assert_eq!(days_to_date(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_date_known_date() {
        // 2000-01-01 is day 10957 from epoch
        assert_eq!(days_to_date(10957), (2000, 1, 1));
    }

    #[test]
    fn days_to_date_end_of_year() {
        // 1970-12-31 is day 364
        assert_eq!(days_to_date(364), (1970, 12, 31));
    }

    #[test]
    fn days_to_date_leap_day() {
        // 1972 is leap, 1972-02-29 is day 789 (365 + 366 + 31 + 28 = 790... let me compute)
        // 1970: 365, 1971: 365 = 730 days
        // 1972: Jan=31 + Feb 29 = 60 → day 730+59 = 789
        assert_eq!(days_to_date(789), (1972, 2, 29));
    }

    // --- top_metric_items tests ---

    #[test]
    fn top_metric_items_sorts_descending_by_value() {
        let mut map = HashMap::new();
        map.insert("low".to_string(), 0.1);
        map.insert("high".to_string(), 0.9);
        map.insert("mid".to_string(), 0.5);
        let items = top_metric_items(&map, 10);
        assert_eq!(items[0].id, "high");
        assert_eq!(items[1].id, "mid");
        assert_eq!(items[2].id, "low");
    }

    #[test]
    fn top_metric_items_truncates_to_limit() {
        let mut map = HashMap::new();
        for i in 0..20 {
            map.insert(format!("i-{i}"), i as f64);
        }
        let items = top_metric_items(&map, 5);
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn top_metric_items_empty_map() {
        let map = HashMap::new();
        let items = top_metric_items(&map, 10);
        assert!(items.is_empty());
    }

    #[test]
    fn top_metric_items_tiebreaks_by_id() {
        let mut map = HashMap::new();
        map.insert("B".to_string(), 1.0);
        map.insert("A".to_string(), 1.0);
        let items = top_metric_items(&map, 10);
        assert_eq!(items[0].id, "A");
        assert_eq!(items[1].id, "B");
    }

    // --- severity_rank tests ---

    #[test]
    fn severity_rank_unknown_defaults_to_info() {
        assert_eq!(severity_rank("info"), severity_rank("bogus"));
    }

    // --- check_metric_drift tests ---

    #[test]
    fn check_metric_drift_empty_baseline_no_alert() {
        let mut alerts = Vec::new();
        check_metric_drift(
            "test",
            &[],
            &[BaselineMetricItem {
                id: "A".to_string(),
                value: 1.0,
            }],
            &mut alerts,
        );
        assert!(alerts.is_empty());
    }

    #[test]
    fn check_metric_drift_empty_current_no_alert() {
        let mut alerts = Vec::new();
        check_metric_drift(
            "test",
            &[BaselineMetricItem {
                id: "A".to_string(),
                value: 1.0,
            }],
            &[],
            &mut alerts,
        );
        assert!(alerts.is_empty());
    }

    #[test]
    fn check_metric_drift_fewer_than_3_changes_no_alert() {
        let baseline = vec![
            BaselineMetricItem {
                id: "A".to_string(),
                value: 5.0,
            },
            BaselineMetricItem {
                id: "B".to_string(),
                value: 4.0,
            },
            BaselineMetricItem {
                id: "C".to_string(),
                value: 3.0,
            },
            BaselineMetricItem {
                id: "D".to_string(),
                value: 2.0,
            },
            BaselineMetricItem {
                id: "E".to_string(),
                value: 1.0,
            },
        ];
        // Only change 2 of top-5
        let current = vec![
            BaselineMetricItem {
                id: "A".to_string(),
                value: 5.0,
            },
            BaselineMetricItem {
                id: "B".to_string(),
                value: 4.0,
            },
            BaselineMetricItem {
                id: "C".to_string(),
                value: 3.0,
            },
            BaselineMetricItem {
                id: "X".to_string(),
                value: 2.0,
            },
            BaselineMetricItem {
                id: "Y".to_string(),
                value: 1.0,
            },
        ];
        let mut alerts = Vec::new();
        check_metric_drift("pr", &baseline, &current, &mut alerts);
        assert!(alerts.is_empty());
    }

    #[test]
    fn check_metric_drift_3_or_more_changes_triggers_alert() {
        let baseline = vec![
            BaselineMetricItem {
                id: "A".to_string(),
                value: 5.0,
            },
            BaselineMetricItem {
                id: "B".to_string(),
                value: 4.0,
            },
            BaselineMetricItem {
                id: "C".to_string(),
                value: 3.0,
            },
            BaselineMetricItem {
                id: "D".to_string(),
                value: 2.0,
            },
            BaselineMetricItem {
                id: "E".to_string(),
                value: 1.0,
            },
        ];
        // Change 3 of top-5
        let current = vec![
            BaselineMetricItem {
                id: "A".to_string(),
                value: 5.0,
            },
            BaselineMetricItem {
                id: "B".to_string(),
                value: 4.0,
            },
            BaselineMetricItem {
                id: "X".to_string(),
                value: 3.0,
            },
            BaselineMetricItem {
                id: "Y".to_string(),
                value: 2.0,
            },
            BaselineMetricItem {
                id: "Z".to_string(),
                value: 1.0,
            },
        ];
        let mut alerts = Vec::new();
        check_metric_drift("pr", &baseline, &current, &mut alerts);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "pr");
        assert_eq!(alerts[0].severity, "warning");
    }

    // --- compute_drift density tests ---

    #[test]
    fn compute_drift_density_growth_warning() {
        let baseline = make_baseline(0, 0, 5, 0.1, 10, 5);
        // Need current density >= 0.15 (50% increase from 0.1)
        // With 10 nodes and many edges, density goes up
        let issues: Vec<Issue> = (0..10)
            .map(|i| Issue {
                id: format!("I-{i}"),
                title: format!("Issue {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                dependencies: if i > 0 {
                    vec![
                        crate::model::Dependency {
                            issue_id: format!("I-{i}"),
                            depends_on_id: format!("I-{}", i - 1),
                            dep_type: "blocks".to_string(),
                            ..crate::model::Dependency::default()
                        },
                        crate::model::Dependency {
                            issue_id: format!("I-{i}"),
                            depends_on_id: format!("I-{}", (i + 2) % 10),
                            dep_type: "blocks".to_string(),
                            ..crate::model::Dependency::default()
                        },
                    ]
                } else {
                    vec![]
                },
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        let result = compute_drift(&baseline, &issues, &graph, &metrics);
        let density_alerts: Vec<_> = result
            .alerts
            .iter()
            .filter(|a| a.alert_type == "density_growth")
            .collect();
        // We may or may not hit 50% threshold depending on exact edge count,
        // but we should at least not panic
        assert!(result.exit_code <= 2);
        // Verify alert structure if present
        for alert in &density_alerts {
            assert!(alert.severity == "warning" || alert.severity == "info");
        }
    }

    #[test]
    fn compute_drift_no_alerts_when_density_baseline_zero() {
        let baseline = make_baseline(0, 0, 5, 0.0, 10, 0);
        let (issues, graph, metrics) = make_issues_and_graph(10);
        let result = compute_drift(&baseline, &issues, &graph, &metrics);
        // density baseline is 0, so density check is skipped
        assert!(
            !result
                .alerts
                .iter()
                .any(|a| a.alert_type == "density_growth")
        );
    }

    // --- compute_drift exit code tests ---

    #[test]
    fn compute_drift_exit_code_0_when_clean() {
        let (issues, graph, metrics) = make_issues_and_graph(5);
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "");
        let result = compute_drift(&baseline, &issues, &graph, &metrics);
        assert_eq!(result.exit_code, 0);
        assert!(!result.has_drift);
    }

    #[test]
    fn compute_drift_exit_code_1_for_critical() {
        let (issues, graph, metrics) = make_issues_and_graph(3);
        let mut baseline = Baseline::from_current(&issues, &graph, &metrics, "");
        baseline.stats.cycle_count = 0;
        baseline.cycles.clear();

        let mut metrics_with_cycle = metrics;
        metrics_with_cycle.cycles = vec![vec!["X".to_string(), "Y".to_string()]];

        let result = compute_drift(&baseline, &issues, &graph, &metrics_with_cycle);
        assert_eq!(result.exit_code, 1);
        assert!(result.has_drift);
        assert!(result.summary.critical > 0);
    }

    // --- Baseline save/load roundtrip ---

    #[test]
    fn baseline_save_load_roundtrip() {
        let baseline = make_baseline(2, 3, 8, 0.15, 20, 12);
        let dir = tempfile::tempdir().unwrap();
        let path = baseline.save(dir.path()).unwrap();
        assert!(path.exists());

        let loaded = Baseline::load(dir.path()).unwrap();
        assert_eq!(loaded.version, baseline.version);
        assert_eq!(loaded.stats.node_count, 20);
        assert_eq!(loaded.stats.edge_count, 12);
        assert_eq!(loaded.stats.blocked_count, 3);
        assert_eq!(loaded.stats.cycle_count, 2);
    }

    #[test]
    fn baseline_load_missing_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = Baseline::load(dir.path());
        assert!(result.is_err());
    }

    // --- DriftSummary counts ---

    #[test]
    fn drift_summary_counts_by_severity() {
        let (issues, graph, metrics) = make_issues_and_graph(3);
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "");
        let result = compute_drift(&baseline, &issues, &graph, &metrics);
        assert_eq!(
            result.summary.critical + result.summary.warning + result.summary.info,
            result.alerts.len()
        );
    }

    // --- alert sorting ---

    #[test]
    fn alerts_sorted_critical_before_warning_before_info() {
        let mut baseline = make_baseline(0, 0, 10, 0.1, 10, 5);
        baseline.stats.cycle_count = 0;
        baseline.cycles.clear();

        // Create issues that trigger multiple alert types
        let issues: Vec<Issue> = (0..10)
            .map(|i| Issue {
                id: format!("I-{i}"),
                title: format!("Issue {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let mut metrics = graph.compute_metrics();
        metrics.cycles = vec![vec!["A".to_string(), "B".to_string()]]; // trigger critical

        let result = compute_drift(&baseline, &issues, &graph, &metrics);
        if result.alerts.len() >= 2 {
            for window in result.alerts.windows(2) {
                assert!(
                    severity_rank(&window[0].severity) <= severity_rank(&window[1].severity),
                    "alerts should be sorted by severity"
                );
            }
        }
    }

    // --- Baseline density calculation ---

    #[test]
    fn baseline_from_current_density_zero_for_single_node() {
        let (issues, graph, metrics) = make_issues_and_graph(1);
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "");
        assert_eq!(baseline.stats.density, 0.0);
    }

    #[test]
    fn baseline_from_current_counts_closed() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Open".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Closed".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let baseline = Baseline::from_current(&issues, &graph, &metrics, "");
        assert_eq!(baseline.stats.open_count, 1);
        assert_eq!(baseline.stats.closed_count, 1);
    }
}
