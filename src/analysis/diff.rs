use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;

use super::graph::IssueGraph;
use crate::model::{Comment, Dependency, Issue};

const ZERO_TIME_RFC3339: &str = "0001-01-01T00:00:00Z";

#[derive(Debug, Clone)]
pub struct DiffMetadata {
    pub from_timestamp: String,
    pub to_timestamp: String,
    pub from_revision: Option<String>,
    pub to_revision: Option<String>,
}

impl Default for DiffMetadata {
    fn default() -> Self {
        Self {
            from_timestamp: "0001-01-01T00:00:00Z".to_string(),
            to_timestamp: Utc::now().to_rfc3339(),
            from_revision: None,
            to_revision: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModifiedIssue {
    pub issue_id: String,
    pub title: String,
    pub changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffIssue {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DiffDependency>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<DiffComment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffDependency {
    pub issue_id: String,
    pub depends_on_id: String,
    #[serde(rename = "type")]
    pub dep_type: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffComment {
    pub id: i64,
    pub issue_id: String,
    pub author: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MetricDeltas {
    pub total_issues: i64,
    pub open_issues: i64,
    pub closed_issues: i64,
    pub blocked_issues: i64,
    pub total_edges: i64,
    pub cycle_count: i64,
    pub component_count: i64,
    pub avg_pagerank: f64,
    pub avg_betweenness: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiffSummary {
    pub total_changes: usize,
    pub issues_added: usize,
    pub issues_closed: usize,
    pub issues_removed: usize,
    pub issues_reopened: usize,
    pub issues_modified: usize,
    pub cycles_introduced: usize,
    pub cycles_resolved: usize,
    pub net_issue_change: i64,
    pub health_trend: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDiff {
    pub from_timestamp: String,
    pub to_timestamp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from_revision: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub to_revision: String,
    pub new_issues: Option<Vec<DiffIssue>>,
    pub closed_issues: Option<Vec<DiffIssue>>,
    pub removed_issues: Option<Vec<DiffIssue>>,
    pub reopened_issues: Option<Vec<DiffIssue>>,
    pub modified_issues: Option<Vec<ModifiedIssue>>,
    pub new_cycles: Option<Vec<Vec<String>>>,
    pub resolved_cycles: Option<Vec<Vec<String>>>,
    pub metric_deltas: MetricDeltas,
    pub summary: DiffSummary,
}

impl SnapshotDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.summary.total_changes == 0
            && self.summary.cycles_introduced == 0
            && self.summary.cycles_resolved == 0
    }

    #[must_use]
    pub fn has_significant_changes(&self) -> bool {
        option_len(self.new_issues.as_ref()) > 0
            || option_len(self.closed_issues.as_ref()) > 0
            || option_len(self.reopened_issues.as_ref()) > 0
            || option_len(self.new_cycles.as_ref()) > 0
            || option_len(self.resolved_cycles.as_ref()) > 0
            || self.summary.health_trend == "degrading"
    }
}

#[must_use]
pub fn compare_snapshots(before: &[Issue], after: &[Issue]) -> SnapshotDiff {
    compare_snapshots_with_metadata(before, after, &DiffMetadata::default())
}

#[must_use]
pub fn compare_snapshots_with_metadata(
    before: &[Issue],
    after: &[Issue],
    metadata: &DiffMetadata,
) -> SnapshotDiff {
    let before_map: HashMap<&str, &Issue> = before
        .iter()
        .map(|issue| (issue.id.as_str(), issue))
        .collect();
    let after_map: HashMap<&str, &Issue> = after
        .iter()
        .map(|issue| (issue.id.as_str(), issue))
        .collect();

    let before_ids: HashSet<&str> = before_map.keys().copied().collect();
    let after_ids: HashSet<&str> = after_map.keys().copied().collect();

    let mut new_issues = Vec::<DiffIssue>::new();
    let mut closed_issues = Vec::<DiffIssue>::new();
    let mut removed_issues = Vec::<DiffIssue>::new();
    let mut reopened_issues = Vec::<DiffIssue>::new();
    let mut modified_issues = Vec::<ModifiedIssue>::new();

    for id in after_ids.difference(&before_ids) {
        if let Some(issue) = after_map.get(id) {
            new_issues.push(to_diff_issue(issue));
        }
    }

    for id in before_ids.intersection(&after_ids) {
        let Some(before_issue) = before_map.get(id) else {
            continue;
        };
        let Some(after_issue) = after_map.get(id) else {
            continue;
        };

        let mut changes = detect_changes(before_issue, after_issue);
        let before_closed = before_issue.is_closed_like();
        let after_closed = after_issue.is_closed_like();
        let mut status_transition = false;

        if !before_closed && after_closed {
            status_transition = true;
            closed_issues.push(to_diff_issue(after_issue));
        } else if before_closed && !after_closed {
            status_transition = true;
            reopened_issues.push(to_diff_issue(after_issue));
        }

        if status_transition {
            changes.retain(|change| change.field != "status");
        }

        if !changes.is_empty() {
            modified_issues.push(ModifiedIssue {
                issue_id: after_issue.id.clone(),
                title: after_issue.title.clone(),
                changes,
            });
        }
    }

    for id in before_ids.difference(&after_ids) {
        if let Some(issue) = before_map.get(id) {
            removed_issues.push(to_diff_issue(issue));
        }
    }

    new_issues.sort_by(|left, right| left.id.cmp(&right.id));
    closed_issues.sort_by(|left, right| left.id.cmp(&right.id));
    removed_issues.sort_by(|left, right| left.id.cmp(&right.id));
    reopened_issues.sort_by(|left, right| left.id.cmp(&right.id));
    modified_issues.sort_by(|left, right| left.issue_id.cmp(&right.issue_id));

    let from_graph = IssueGraph::build(before);
    let to_graph = IssueGraph::build(after);
    let from_metrics = from_graph.compute_metrics();
    let to_metrics = to_graph.compute_metrics();
    let (new_cycles, resolved_cycles) = compare_cycles(&from_metrics.cycles, &to_metrics.cycles);

    let from_component_count = from_graph.connected_open_components().len();
    let to_component_count = to_graph.connected_open_components().len();

    let metric_deltas = calculate_metric_deltas(MetricDeltaInputs {
        before,
        after,
        new_cycles_count: option_len(new_cycles.as_ref()),
        resolved_cycles_count: option_len(resolved_cycles.as_ref()),
        from_pagerank: &from_metrics.pagerank,
        to_pagerank: &to_metrics.pagerank,
        from_betweenness: &from_metrics.betweenness,
        to_betweenness: &to_metrics.betweenness,
        from_edge_count: from_graph.edge_count(),
        to_edge_count: to_graph.edge_count(),
        from_component_count,
        to_component_count,
    });

    let summary = calculate_summary(SummaryInputs {
        issues_added: new_issues.len(),
        issues_closed: closed_issues.len(),
        issues_removed: removed_issues.len(),
        issues_reopened: reopened_issues.len(),
        issues_modified: modified_issues.len(),
        cycles_introduced: option_len(new_cycles.as_ref()),
        cycles_resolved: option_len(resolved_cycles.as_ref()),
        blocked_issue_delta: metric_deltas.blocked_issues,
    });

    SnapshotDiff {
        from_timestamp: metadata.from_timestamp.clone(),
        to_timestamp: metadata.to_timestamp.clone(),
        from_revision: metadata.from_revision.clone().unwrap_or_default(),
        to_revision: metadata.to_revision.clone().unwrap_or_default(),
        new_issues: into_option(new_issues),
        closed_issues: into_option(closed_issues),
        removed_issues: into_option(removed_issues),
        reopened_issues: into_option(reopened_issues),
        modified_issues: into_option(modified_issues),
        new_cycles,
        resolved_cycles,
        metric_deltas,
        summary,
    }
}

fn detect_changes(from: &Issue, to: &Issue) -> Vec<FieldChange> {
    let mut changes = Vec::<FieldChange>::new();

    if from.title != to.title {
        changes.push(FieldChange {
            field: "title".to_string(),
            old_value: from.title.clone(),
            new_value: to.title.clone(),
        });
    }

    if from.status != to.status {
        changes.push(FieldChange {
            field: "status".to_string(),
            old_value: from.status.clone(),
            new_value: to.status.clone(),
        });
    }

    if from.priority != to.priority {
        changes.push(FieldChange {
            field: "priority".to_string(),
            old_value: priority_string(from.priority),
            new_value: priority_string(to.priority),
        });
    }

    if from.assignee != to.assignee {
        changes.push(FieldChange {
            field: "assignee".to_string(),
            old_value: from.assignee.clone(),
            new_value: to.assignee.clone(),
        });
    }

    if from.issue_type != to.issue_type {
        changes.push(FieldChange {
            field: "type".to_string(),
            old_value: from.issue_type.clone(),
            new_value: to.issue_type.clone(),
        });
    }

    if from.description != to.description {
        changes.push(FieldChange {
            field: "description".to_string(),
            old_value: "(modified)".to_string(),
            new_value: "(modified)".to_string(),
        });
    }

    if from.design != to.design {
        changes.push(FieldChange {
            field: "design".to_string(),
            old_value: "(modified)".to_string(),
            new_value: "(modified)".to_string(),
        });
    }

    if from.acceptance_criteria != to.acceptance_criteria {
        changes.push(FieldChange {
            field: "acceptance_criteria".to_string(),
            old_value: "(modified)".to_string(),
            new_value: "(modified)".to_string(),
        });
    }

    if from.notes != to.notes {
        changes.push(FieldChange {
            field: "notes".to_string(),
            old_value: "(modified)".to_string(),
            new_value: "(modified)".to_string(),
        });
    }

    let from_deps = dependency_set(&from.dependencies);
    let to_deps = dependency_set(&to.dependencies);
    if from_deps != to_deps {
        changes.push(FieldChange {
            field: "dependencies".to_string(),
            old_value: format_dep_set(&from_deps),
            new_value: format_dep_set(&to_deps),
        });
    }

    let from_labels = string_set(&from.labels);
    let to_labels = string_set(&to.labels);
    if from_labels != to_labels {
        changes.push(FieldChange {
            field: "labels".to_string(),
            old_value: format_string_set(&from_labels),
            new_value: format_string_set(&to_labels),
        });
    }

    changes
}

fn dependency_set(deps: &[Dependency]) -> BTreeSet<String> {
    let mut values = BTreeSet::<String>::new();
    for dep in deps {
        if dep.depends_on_id.trim().is_empty() {
            continue;
        }
        values.insert(format!("{}:{}", dep.depends_on_id, dep.dep_type));
    }
    values
}

fn string_set(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn format_dep_set(values: &BTreeSet<String>) -> String {
    format_string_set(values)
}

fn format_string_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

fn priority_string(priority: i32) -> String {
    format!("P{priority}")
}

fn to_diff_issue(issue: &Issue) -> DiffIssue {
    DiffIssue {
        id: issue.id.clone(),
        title: issue.title.clone(),
        description: issue.description.clone(),
        status: issue.status.clone(),
        priority: issue.priority,
        issue_type: issue.issue_type.clone(),
        assignee: non_empty(&issue.assignee),
        created_at: dt_or_zero(issue.created_at),
        updated_at: dt_or_zero(issue.updated_at),
        closed_at: issue
            .closed_at
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        labels: issue.labels.clone(),
        dependencies: issue.dependencies.iter().map(to_diff_dependency).collect(),
        comments: issue.comments.iter().map(to_diff_comment).collect(),
    }
}

fn to_diff_dependency(dep: &Dependency) -> DiffDependency {
    DiffDependency {
        issue_id: dep.issue_id.clone(),
        depends_on_id: dep.depends_on_id.clone(),
        dep_type: dep.dep_type.clone(),
        created_by: dep.created_by.clone(),
        created_at: dt_or_zero(dep.created_at),
    }
}

fn to_diff_comment(comment: &Comment) -> DiffComment {
    DiffComment {
        id: comment.id,
        issue_id: comment.issue_id.clone(),
        author: comment.author.clone(),
        text: comment.text.clone(),
        created_at: dt_or_zero(comment.created_at),
    }
}

fn dt_or_zero(dt: Option<chrono::DateTime<chrono::Utc>>) -> String {
    dt.map_or_else(
        || ZERO_TIME_RFC3339.to_string(),
        |d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn option_len<T>(values: Option<&Vec<T>>) -> usize {
    values.map_or(0, Vec::len)
}

fn into_option<T>(values: Vec<T>) -> Option<Vec<T>> {
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

type OptionalCycleSets = (Option<Vec<Vec<String>>>, Option<Vec<Vec<String>>>);

fn compare_cycles(from_cycles: &[Vec<String>], to_cycles: &[Vec<String>]) -> OptionalCycleSets {
    let from_cycle_set = from_cycles
        .iter()
        .map(|cycle| (normalize_cycle(cycle), cycle.clone()))
        .collect::<HashMap<_, _>>();
    let to_cycle_set = to_cycles
        .iter()
        .map(|cycle| (normalize_cycle(cycle), cycle.clone()))
        .collect::<HashMap<_, _>>();

    let mut new_cycles = to_cycle_set
        .iter()
        .filter_map(|(key, cycle)| {
            if from_cycle_set.contains_key(key) {
                None
            } else {
                Some(cycle.clone())
            }
        })
        .collect::<Vec<_>>();

    let mut resolved_cycles = from_cycle_set
        .iter()
        .filter_map(|(key, cycle)| {
            if to_cycle_set.contains_key(key) {
                None
            } else {
                Some(cycle.clone())
            }
        })
        .collect::<Vec<_>>();

    new_cycles.sort_by_key(|cycle| normalize_cycle(cycle));
    resolved_cycles.sort_by_key(|cycle| normalize_cycle(cycle));

    (into_option(new_cycles), into_option(resolved_cycles))
}

fn normalize_cycle(cycle: &[String]) -> String {
    if cycle.is_empty() {
        return String::new();
    }

    let mut min_idx = 0usize;
    for (index, id) in cycle.iter().enumerate().skip(1) {
        if id < &cycle[min_idx] {
            min_idx = index;
        }
    }

    (0..cycle.len())
        .map(|offset| cycle[(min_idx + offset) % cycle.len()].clone())
        .collect::<Vec<_>>()
        .join("->")
}

struct MetricDeltaInputs<'a> {
    before: &'a [Issue],
    after: &'a [Issue],
    new_cycles_count: usize,
    resolved_cycles_count: usize,
    from_pagerank: &'a HashMap<String, f64>,
    to_pagerank: &'a HashMap<String, f64>,
    from_betweenness: &'a HashMap<String, f64>,
    to_betweenness: &'a HashMap<String, f64>,
    from_edge_count: usize,
    to_edge_count: usize,
    from_component_count: usize,
    to_component_count: usize,
}

fn calculate_metric_deltas(inputs: MetricDeltaInputs<'_>) -> MetricDeltas {
    let before_counts = snapshot_counts(inputs.before);
    let after_counts = snapshot_counts(inputs.after);

    MetricDeltas {
        total_issues: i64::try_from(after_counts.total).unwrap_or(i64::MAX)
            - i64::try_from(before_counts.total).unwrap_or(i64::MAX),
        open_issues: i64::try_from(after_counts.open).unwrap_or(i64::MAX)
            - i64::try_from(before_counts.open).unwrap_or(i64::MAX),
        closed_issues: i64::try_from(after_counts.terminal()).unwrap_or(i64::MAX)
            - i64::try_from(before_counts.terminal()).unwrap_or(i64::MAX),
        blocked_issues: i64::try_from(after_counts.blocked).unwrap_or(i64::MAX)
            - i64::try_from(before_counts.blocked).unwrap_or(i64::MAX),
        total_edges: i64::try_from(inputs.to_edge_count).unwrap_or(i64::MAX)
            - i64::try_from(inputs.from_edge_count).unwrap_or(i64::MAX),
        cycle_count: i64::try_from(inputs.new_cycles_count).unwrap_or(i64::MAX)
            - i64::try_from(inputs.resolved_cycles_count).unwrap_or(i64::MAX),
        component_count: i64::try_from(inputs.to_component_count).unwrap_or(i64::MAX)
            - i64::try_from(inputs.from_component_count).unwrap_or(i64::MAX),
        avg_pagerank: average_map_value(inputs.to_pagerank)
            - average_map_value(inputs.from_pagerank),
        avg_betweenness: average_map_value(inputs.to_betweenness)
            - average_map_value(inputs.from_betweenness),
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct SnapshotCounts {
    total: usize,
    open: usize,
    closed: usize,
    tombstone: usize,
    blocked: usize,
}

impl SnapshotCounts {
    fn terminal(&self) -> usize {
        self.closed + self.tombstone
    }
}

fn snapshot_counts(issues: &[Issue]) -> SnapshotCounts {
    let mut counts = SnapshotCounts {
        total: issues.len(),
        ..SnapshotCounts::default()
    };

    for issue in issues {
        if issue.is_tombstone() {
            counts.tombstone = counts.tombstone.saturating_add(1);
        } else if issue.is_closed() {
            counts.closed = counts.closed.saturating_add(1);
        } else {
            counts.open = counts.open.saturating_add(1);
            if issue.normalized_status() == "blocked" {
                counts.blocked = counts.blocked.saturating_add(1);
            }
        }
    }

    counts
}

fn average_map_value(values: &HashMap<String, f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut keys = values.keys().cloned().collect::<Vec<_>>();
    keys.sort();

    let sum = keys
        .iter()
        .filter_map(|key| values.get(key))
        .fold(0.0_f64, |acc, value| acc + value);

    sum / (values.len() as f64)
}

struct SummaryInputs {
    issues_added: usize,
    issues_closed: usize,
    issues_removed: usize,
    issues_reopened: usize,
    issues_modified: usize,
    cycles_introduced: usize,
    cycles_resolved: usize,
    blocked_issue_delta: i64,
}

fn calculate_summary(inputs: SummaryInputs) -> DiffSummary {
    let total_changes = inputs.issues_added
        + inputs.issues_closed
        + inputs.issues_removed
        + inputs.issues_reopened
        + inputs.issues_modified;

    let mut score = 0_i64;
    score += i64::try_from(inputs.cycles_resolved.saturating_mul(2)).unwrap_or(i64::MAX);
    score -= i64::try_from(inputs.cycles_introduced.saturating_mul(3)).unwrap_or(i64::MAX);
    score += i64::try_from(inputs.issues_closed).unwrap_or(i64::MAX);
    score -= i64::try_from(inputs.issues_reopened).unwrap_or(i64::MAX);

    if inputs.blocked_issue_delta < 0 {
        score += 2;
    } else if inputs.blocked_issue_delta > 0 {
        score -= 1;
    }

    let health_trend = if score > 1 {
        "improving"
    } else if score < -1 {
        "degrading"
    } else {
        "stable"
    };

    DiffSummary {
        total_changes,
        issues_added: inputs.issues_added,
        issues_closed: inputs.issues_closed,
        issues_removed: inputs.issues_removed,
        issues_reopened: inputs.issues_reopened,
        issues_modified: inputs.issues_modified,
        cycles_introduced: inputs.cycles_introduced,
        cycles_resolved: inputs.cycles_resolved,
        net_issue_change: i64::try_from(inputs.issues_added).unwrap_or(i64::MAX)
            - i64::try_from(inputs.issues_removed).unwrap_or(i64::MAX),
        health_trend: health_trend.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use crate::model::{Dependency, Issue};

    use super::{
        SummaryInputs, average_map_value, calculate_summary, compare_cycles, compare_snapshots,
        detect_changes, format_string_set, into_option, non_empty, normalize_cycle, option_len,
        snapshot_counts,
    };

    #[test]
    fn detects_new_closed_reopened_and_modified() {
        let before = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                ..Issue::default()
            },
        ];

        let after = vec![
            Issue {
                id: "A".to_string(),
                title: "A2".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                dependencies: vec![Dependency {
                    depends_on_id: "C".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "C".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                ..Issue::default()
            },
        ];

        let diff = compare_snapshots(&before, &after);
        assert_eq!(diff.new_issues.as_ref().map_or(0, Vec::len), 1);
        assert_eq!(diff.closed_issues.as_ref().map_or(0, Vec::len), 1);
        assert_eq!(diff.reopened_issues.as_ref().map_or(0, Vec::len), 1);
        assert_eq!(diff.modified_issues.as_ref().map_or(0, Vec::len), 1);
        assert_eq!(
            diff.modified_issues
                .as_ref()
                .and_then(|issues| issues.first())
                .map(|issue| issue.issue_id.as_str()),
            Some("A")
        );
        assert_eq!(diff.summary.issues_added, 1);
        assert_eq!(diff.summary.issues_removed, 0);
    }

    #[test]
    fn empty_before_and_after_produces_empty_diff() {
        let diff = compare_snapshots(&[], &[]);
        assert_eq!(diff.summary.issues_added, 0);
        assert_eq!(diff.summary.issues_removed, 0);
        assert_eq!(diff.summary.issues_modified, 0);
        assert!(diff.new_issues.as_ref().is_none_or(Vec::is_empty));
        assert!(diff.closed_issues.as_ref().is_none_or(Vec::is_empty));
    }

    #[test]
    fn all_new_issues_detected() {
        let after = vec![
            Issue {
                id: "N-1".to_string(),
                title: "New one".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "N-2".to_string(),
                title: "New two".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let diff = compare_snapshots(&[], &after);
        assert_eq!(diff.new_issues.as_ref().map_or(0, Vec::len), 2);
        assert_eq!(diff.summary.issues_added, 2);
    }

    #[test]
    fn removed_issues_tracked() {
        let before = vec![Issue {
            id: "G-1".to_string(),
            title: "Gone".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &[]);
        assert_eq!(diff.summary.issues_removed, 1);
    }

    #[test]
    fn identical_snapshots_produce_no_changes() {
        let issues = vec![Issue {
            id: "S-1".to_string(),
            title: "Stable".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let diff = compare_snapshots(&issues, &issues);
        assert!(diff.new_issues.as_ref().is_none_or(Vec::is_empty));
        assert!(diff.closed_issues.as_ref().is_none_or(Vec::is_empty));
        assert!(diff.reopened_issues.as_ref().is_none_or(Vec::is_empty));
        assert!(diff.modified_issues.as_ref().is_none_or(Vec::is_empty));
        assert_eq!(diff.summary.issues_added, 0);
        assert_eq!(diff.summary.issues_removed, 0);
    }

    #[test]
    fn priority_change_detected_as_modification() {
        let before = vec![Issue {
            id: "P-1".to_string(),
            title: "Same".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let after = vec![Issue {
            id: "P-1".to_string(),
            title: "Same".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 3,
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &after);
        assert_eq!(diff.modified_issues.as_ref().map_or(0, Vec::len), 1);
        let mods = diff.modified_issues.unwrap();
        assert!(mods[0].changes.iter().any(|c| c.field == "priority"));
    }

    #[test]
    fn dependency_change_detected() {
        let before = vec![Issue {
            id: "D-1".to_string(),
            title: "Dep change".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let after = vec![Issue {
            id: "D-1".to_string(),
            title: "Dep change".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            dependencies: vec![Dependency {
                depends_on_id: "D-2".to_string(),
                dep_type: "blocks".to_string(),
                ..Dependency::default()
            }],
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &after);
        assert_eq!(diff.modified_issues.as_ref().map_or(0, Vec::len), 1);
    }

    #[test]
    fn metric_deltas_computed() {
        let before = vec![
            Issue {
                id: "M-1".to_string(),
                title: "Open".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "M-2".to_string(),
                title: "Blocked".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![Dependency {
                    depends_on_id: "M-1".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let after = vec![
            Issue {
                id: "M-1".to_string(),
                title: "Open".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "M-2".to_string(),
                title: "Blocked".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let diff = compare_snapshots(&before, &after);
        // Closing M-1 should change open_issues delta
        assert_ne!(diff.metric_deltas.open_issues, 0);
    }

    #[test]
    fn metric_deltas_treat_review_like_status_as_open() {
        let before = vec![Issue {
            id: "R-1".to_string(),
            title: "In review".to_string(),
            status: "review".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let after = Vec::<Issue>::new();

        let diff = compare_snapshots(&before, &after);
        assert_eq!(
            diff.metric_deltas.open_issues, -1,
            "review status should be counted as open-like in deltas"
        );
    }

    // --- detect_changes tests ---

    #[test]
    fn detect_changes_no_changes_returns_empty() {
        let issue = Issue {
            id: "X".to_string(),
            title: "Same".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            assignee: "alice".to_string(),
            ..Issue::default()
        };
        let changes = detect_changes(&issue, &issue);
        assert!(changes.is_empty());
    }

    #[test]
    fn detect_changes_title_change() {
        let from = Issue {
            id: "X".to_string(),
            title: "Old title".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            title: "New title".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "title");
        assert_eq!(changes[0].old_value, "Old title");
        assert_eq!(changes[0].new_value, "New title");
    }

    #[test]
    fn detect_changes_status_change() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            status: "in_progress".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        assert!(changes.iter().any(|c| c.field == "status"));
    }

    #[test]
    fn detect_changes_priority_formats_as_p_string() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        };
        let to = Issue {
            priority: 3,
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        let pchange = changes.iter().find(|c| c.field == "priority").unwrap();
        assert_eq!(pchange.old_value, "P1");
        assert_eq!(pchange.new_value, "P3");
    }

    #[test]
    fn detect_changes_assignee_change() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            assignee: "alice".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            assignee: "bob".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        let achange = changes.iter().find(|c| c.field == "assignee").unwrap();
        assert_eq!(achange.old_value, "alice");
        assert_eq!(achange.new_value, "bob");
    }

    #[test]
    fn detect_changes_type_change() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            issue_type: "bug".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        assert!(changes.iter().any(|c| c.field == "type"));
    }

    #[test]
    fn detect_changes_description_shows_modified_not_content() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            description: "old desc".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            description: "new desc".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        let dchange = changes.iter().find(|c| c.field == "description").unwrap();
        assert_eq!(dchange.old_value, "(modified)");
        assert_eq!(dchange.new_value, "(modified)");
    }

    #[test]
    fn detect_changes_labels_change() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            labels: vec!["api".to_string()],
            ..Issue::default()
        };
        let to = Issue {
            labels: vec!["api".to_string(), "backend".to_string()],
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        let lchange = changes.iter().find(|c| c.field == "labels").unwrap();
        assert_eq!(lchange.old_value, "api");
        assert_eq!(lchange.new_value, "api, backend");
    }

    #[test]
    fn detect_changes_dependency_added() {
        let from = Issue {
            id: "X".to_string(),
            title: "T".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            dependencies: vec![Dependency {
                depends_on_id: "Y".to_string(),
                dep_type: "blocks".to_string(),
                ..Dependency::default()
            }],
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        let dchange = changes.iter().find(|c| c.field == "dependencies").unwrap();
        assert_eq!(dchange.old_value, "(none)");
        assert!(dchange.new_value.contains("Y:blocks"));
    }

    #[test]
    fn detect_changes_multiple_fields_at_once() {
        let from = Issue {
            id: "X".to_string(),
            title: "Old".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            assignee: "alice".to_string(),
            ..Issue::default()
        };
        let to = Issue {
            title: "New".to_string(),
            priority: 3,
            assignee: "bob".to_string(),
            ..from.clone()
        };
        let changes = detect_changes(&from, &to);
        assert_eq!(changes.len(), 3);
        let fields: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"priority"));
        assert!(fields.contains(&"assignee"));
    }

    // --- normalize_cycle tests ---

    #[test]
    fn normalize_cycle_empty() {
        assert_eq!(normalize_cycle(&[]), "");
    }

    #[test]
    fn normalize_cycle_single_element() {
        let cycle = vec!["A".to_string()];
        assert_eq!(normalize_cycle(&cycle), "A");
    }

    #[test]
    fn normalize_cycle_already_starts_at_min() {
        let cycle = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(normalize_cycle(&cycle), "A->B->C");
    }

    #[test]
    fn normalize_cycle_rotates_to_min() {
        let cycle = vec!["C".to_string(), "A".to_string(), "B".to_string()];
        assert_eq!(normalize_cycle(&cycle), "A->B->C");
    }

    #[test]
    fn normalize_cycle_different_rotations_same_result() {
        let c1 = vec!["B".to_string(), "C".to_string(), "A".to_string()];
        let c2 = vec!["C".to_string(), "A".to_string(), "B".to_string()];
        let c3 = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let norm = normalize_cycle(&c3);
        assert_eq!(normalize_cycle(&c1), norm);
        assert_eq!(normalize_cycle(&c2), norm);
    }

    // --- compare_cycles tests ---

    #[test]
    fn compare_cycles_no_change() {
        let cycles = vec![vec!["A".to_string(), "B".to_string()]];
        let (new, resolved) = compare_cycles(&cycles, &cycles);
        assert!(new.is_none());
        assert!(resolved.is_none());
    }

    #[test]
    fn compare_cycles_new_cycle_introduced() {
        let before: Vec<Vec<String>> = vec![];
        let after = vec![vec!["A".to_string(), "B".to_string()]];
        let (new, resolved) = compare_cycles(&before, &after);
        assert_eq!(option_len(new.as_ref()), 1);
        assert!(resolved.is_none());
    }

    #[test]
    fn compare_cycles_cycle_resolved() {
        let before = vec![vec!["A".to_string(), "B".to_string()]];
        let after: Vec<Vec<String>> = vec![];
        let (new, resolved) = compare_cycles(&before, &after);
        assert!(new.is_none());
        assert_eq!(option_len(resolved.as_ref()), 1);
    }

    #[test]
    fn compare_cycles_rotated_cycle_matches() {
        let before = vec![vec!["A".to_string(), "B".to_string(), "C".to_string()]];
        let after = vec![vec!["B".to_string(), "C".to_string(), "A".to_string()]];
        let (new, resolved) = compare_cycles(&before, &after);
        assert!(new.is_none(), "rotated cycle should match");
        assert!(resolved.is_none(), "rotated cycle should match");
    }

    #[test]
    fn compare_cycles_mixed_new_and_resolved() {
        let before = vec![vec!["A".to_string(), "B".to_string()]];
        let after = vec![vec!["C".to_string(), "D".to_string()]];
        let (new, resolved) = compare_cycles(&before, &after);
        assert_eq!(option_len(new.as_ref()), 1);
        assert_eq!(option_len(resolved.as_ref()), 1);
    }

    // --- calculate_summary tests ---

    #[test]
    fn calculate_summary_zero_inputs() {
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 0,
            issues_removed: 0,
            issues_reopened: 0,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.total_changes, 0);
        assert_eq!(summary.health_trend, "stable");
        assert_eq!(summary.net_issue_change, 0);
    }

    #[test]
    fn calculate_summary_improving_trend() {
        // score: +2 (cycles resolved * 2) + 3 (issues closed) = 5 > 1 → improving
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 3,
            issues_removed: 0,
            issues_reopened: 0,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 1,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.health_trend, "improving");
    }

    #[test]
    fn calculate_summary_degrading_trend() {
        // score: -3 (cycle introduced * 3) - 1 (reopened) = -4 < -1 → degrading
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 0,
            issues_removed: 0,
            issues_reopened: 1,
            issues_modified: 0,
            cycles_introduced: 1,
            cycles_resolved: 0,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.health_trend, "degrading");
    }

    #[test]
    fn calculate_summary_stable_when_score_in_range() {
        // score: +1 (closed) - 1 (reopened) = 0 → stable
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 1,
            issues_removed: 0,
            issues_reopened: 1,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.health_trend, "stable");
    }

    #[test]
    fn calculate_summary_blocked_delta_negative_boosts_score() {
        // score: 0 + 2 (blocked decreased) = 2 > 1 → improving
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 0,
            issues_removed: 0,
            issues_reopened: 0,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: -1,
        });
        assert_eq!(summary.health_trend, "improving");
    }

    #[test]
    fn calculate_summary_blocked_delta_positive_hurts_score() {
        // score: 0 - 1 (blocked increased) = -1, which is NOT < -1 → stable
        let summary = calculate_summary(SummaryInputs {
            issues_added: 0,
            issues_closed: 0,
            issues_removed: 0,
            issues_reopened: 0,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: 1,
        });
        assert_eq!(summary.health_trend, "stable");
    }

    #[test]
    fn calculate_summary_total_changes_is_sum() {
        let summary = calculate_summary(SummaryInputs {
            issues_added: 2,
            issues_closed: 3,
            issues_removed: 1,
            issues_reopened: 1,
            issues_modified: 4,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.total_changes, 2 + 3 + 1 + 1 + 4);
    }

    #[test]
    fn calculate_summary_net_issue_change() {
        let summary = calculate_summary(SummaryInputs {
            issues_added: 5,
            issues_closed: 0,
            issues_removed: 2,
            issues_reopened: 0,
            issues_modified: 0,
            cycles_introduced: 0,
            cycles_resolved: 0,
            blocked_issue_delta: 0,
        });
        assert_eq!(summary.net_issue_change, 3);
    }

    // --- snapshot_counts tests ---

    #[test]
    fn snapshot_counts_empty() {
        let counts = snapshot_counts(&[]);
        assert_eq!(counts.total, 0);
        assert_eq!(counts.open, 0);
        assert_eq!(counts.closed, 0);
        assert_eq!(counts.blocked, 0);
        assert_eq!(counts.terminal(), 0);
    }

    #[test]
    fn snapshot_counts_mixed_statuses() {
        let issues = vec![
            Issue {
                id: "1".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "2".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "3".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let counts = snapshot_counts(&issues);
        assert_eq!(counts.total, 3);
        assert_eq!(counts.open, 2); // open + blocked are both non-closed
        assert_eq!(counts.closed, 1);
        assert_eq!(counts.blocked, 1);
    }

    #[test]
    fn snapshot_counts_terminal_includes_tombstones() {
        let issues = vec![Issue {
            id: "1".to_string(),
            status: "tombstone".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let counts = snapshot_counts(&issues);
        assert_eq!(counts.tombstone, 1);
        assert_eq!(counts.terminal(), 1);
    }

    // --- average_map_value tests ---

    #[test]
    fn average_map_value_empty() {
        let map = HashMap::new();
        assert_eq!(average_map_value(&map), 0.0);
    }

    #[test]
    fn average_map_value_single() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), 10.0);
        assert!((average_map_value(&map) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn average_map_value_multiple() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), 2.0);
        map.insert("b".to_string(), 4.0);
        map.insert("c".to_string(), 6.0);
        assert!((average_map_value(&map) - 4.0).abs() < f64::EPSILON);
    }

    // --- format_string_set tests ---

    #[test]
    fn format_string_set_empty_returns_none_marker() {
        let set = BTreeSet::new();
        assert_eq!(format_string_set(&set), "(none)");
    }

    #[test]
    fn format_string_set_single() {
        let mut set = BTreeSet::new();
        set.insert("api".to_string());
        assert_eq!(format_string_set(&set), "api");
    }

    #[test]
    fn format_string_set_multiple_sorted() {
        let mut set = BTreeSet::new();
        set.insert("beta".to_string());
        set.insert("alpha".to_string());
        set.insert("gamma".to_string());
        assert_eq!(format_string_set(&set), "alpha, beta, gamma");
    }

    // --- non_empty tests ---

    #[test]
    fn non_empty_returns_none_for_empty_string() {
        assert_eq!(non_empty(""), None);
    }

    #[test]
    fn non_empty_returns_none_for_whitespace() {
        assert_eq!(non_empty("   "), None);
    }

    #[test]
    fn non_empty_returns_trimmed_value() {
        assert_eq!(non_empty("  hello  "), Some("hello".to_string()));
    }

    // --- into_option tests ---

    #[test]
    fn into_option_empty_vec_is_none() {
        let v: Vec<i32> = vec![];
        assert!(into_option(v).is_none());
    }

    #[test]
    fn into_option_non_empty_vec_is_some() {
        let v = vec![1, 2, 3];
        let opt = into_option(v);
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().len(), 3);
    }

    // --- option_len tests ---

    #[test]
    fn option_len_none_is_zero() {
        let v: Option<&Vec<i32>> = None;
        assert_eq!(option_len(v), 0);
    }

    #[test]
    fn option_len_some_empty_is_zero() {
        let v: Vec<i32> = vec![];
        assert_eq!(option_len(Some(&v)), 0);
    }

    #[test]
    fn option_len_some_with_items() {
        let v = vec![1, 2, 3];
        assert_eq!(option_len(Some(&v)), 3);
    }

    // --- SnapshotDiff::is_empty / has_significant_changes tests ---

    #[test]
    fn snapshot_diff_is_empty_when_no_changes() {
        let diff = compare_snapshots(&[], &[]);
        assert!(diff.is_empty());
    }

    #[test]
    fn snapshot_diff_is_not_empty_with_changes() {
        let after = vec![Issue {
            id: "A".to_string(),
            title: "New".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let diff = compare_snapshots(&[], &after);
        assert!(!diff.is_empty());
    }

    #[test]
    fn snapshot_diff_has_significant_changes_with_new_issues() {
        let after = vec![Issue {
            id: "A".to_string(),
            title: "New".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let diff = compare_snapshots(&[], &after);
        assert!(diff.has_significant_changes());
    }

    #[test]
    fn snapshot_diff_no_significant_changes_for_modification_only() {
        let before = vec![Issue {
            id: "A".to_string(),
            title: "Old".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let after = vec![Issue {
            id: "A".to_string(),
            title: "New".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &after);
        // Only a title modification — no new/closed/reopened/cycles
        assert!(!diff.has_significant_changes());
    }

    // --- status transition stripping test ---

    #[test]
    fn closed_transition_strips_status_from_modified_changes() {
        let before = vec![Issue {
            id: "A".to_string(),
            title: "Old title".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let after = vec![Issue {
            id: "A".to_string(),
            title: "New title".to_string(),
            status: "closed".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &after);
        assert_eq!(diff.closed_issues.as_ref().map_or(0, Vec::len), 1);
        // modified_issues should have title change but NOT status change
        let mods = diff.modified_issues.as_ref().unwrap();
        assert_eq!(mods.len(), 1);
        assert!(mods[0].changes.iter().any(|c| c.field == "title"));
        assert!(!mods[0].changes.iter().any(|c| c.field == "status"));
    }

    #[test]
    fn reopen_transition_strips_status_from_modified_changes() {
        let before = vec![Issue {
            id: "A".to_string(),
            title: "Old".to_string(),
            status: "closed".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let after = vec![Issue {
            id: "A".to_string(),
            title: "New".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let diff = compare_snapshots(&before, &after);
        assert_eq!(diff.reopened_issues.as_ref().map_or(0, Vec::len), 1);
        let mods = diff.modified_issues.as_ref().unwrap();
        assert!(!mods[0].changes.iter().any(|c| c.field == "status"));
    }
}
