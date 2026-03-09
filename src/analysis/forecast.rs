use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::analysis::graph::{GraphMetrics, IssueGraph};
use crate::model::Issue;

const DEFAULT_ESTIMATED_MINUTES: i64 = 60;
const I64_MAX_F64: f64 = 9_223_372_036_854_775_807.0;
const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;

#[derive(Debug, Clone, Serialize)]
pub struct ForecastItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub confidence: f64,
    pub eta_minutes: i64,
    pub estimated_days: f64,
    pub eta_date: String,
    pub eta_date_low: String,
    pub eta_date_high: String,
    pub velocity_minutes_per_day: f64,
    pub agents: usize,
    pub factors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForecastSummary {
    pub generated_at: String,
    pub count: usize,
    pub avg_eta_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForecastOutput {
    pub summary: ForecastSummary,
    pub forecasts: Vec<ForecastItem>,
}

#[derive(Debug, Clone)]
pub struct EtaEstimate {
    pub estimated_minutes: i64,
    pub estimated_days: f64,
    pub eta_date: String,
    pub eta_date_low: String,
    pub eta_date_high: String,
    pub confidence: f64,
    pub velocity_minutes_per_day: f64,
    pub agents: usize,
    pub factors: Vec<String>,
}

#[must_use]
pub fn estimate_forecast(
    issues: &[Issue],
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    issue_id_or_all: &str,
    label_filter: Option<&str>,
    agents: usize,
) -> ForecastOutput {
    let now = Utc::now();
    let mut forecasts = Vec::<ForecastItem>::new();

    let target_all = issue_id_or_all.eq_ignore_ascii_case("all");

    for issue in issues {
        if !issue.is_open_like() {
            continue;
        }
        if !target_all && issue.id != issue_id_or_all {
            continue;
        }
        if label_filter.is_some_and(|label| !has_label(&issue.labels, label)) {
            continue;
        }

        let Some(eta) = estimate_eta_for_issue(issues, graph, metrics, &issue.id, agents, now)
        else {
            continue;
        };

        forecasts.push(ForecastItem {
            id: issue.id.clone(),
            title: issue.title.clone(),
            status: issue.status.clone(),
            confidence: eta.confidence,
            eta_minutes: eta.estimated_minutes,
            estimated_days: eta.estimated_days,
            eta_date: eta.eta_date,
            eta_date_low: eta.eta_date_low,
            eta_date_high: eta.eta_date_high,
            velocity_minutes_per_day: eta.velocity_minutes_per_day,
            agents: eta.agents,
            factors: eta.factors,
        });
    }

    let avg_eta_minutes = if forecasts.is_empty() {
        0
    } else {
        forecasts.iter().map(|item| item.eta_minutes).sum::<i64>()
            / i64::try_from(forecasts.len()).unwrap_or(1)
    };

    ForecastOutput {
        summary: ForecastSummary {
            generated_at: now.to_rfc3339(),
            count: forecasts.len(),
            avg_eta_minutes,
        },
        forecasts,
    }
}

#[must_use]
pub fn estimate_eta_for_issue(
    issues: &[Issue],
    _graph: &IssueGraph,
    metrics: &GraphMetrics,
    issue_id: &str,
    agents: usize,
    now: DateTime<Utc>,
) -> Option<EtaEstimate> {
    let issue = issues.iter().find(|issue| issue.id == issue_id)?;
    let agents = agents.max(1);

    let median_minutes = compute_median_estimated_minutes(issues);
    let (complexity_minutes, mut factors) =
        estimate_complexity_minutes(issue, metrics, median_minutes);
    let (mut velocity_per_day, velocity_samples, velocity_factors) =
        estimate_velocity_minutes_per_day(issues, issue, now, median_minutes);

    if velocity_per_day <= 0.0 {
        velocity_per_day = (median_minutes as f64) / 5.0;
        if velocity_per_day <= 0.0 {
            velocity_per_day = 60.0;
        }
        factors.extend(velocity_factors);
        factors.push("velocity: no recent closures; using default".to_string());
    } else {
        factors.extend(velocity_factors);
    }

    let capacity_per_day = velocity_per_day * (agents as f64);
    let mut estimated_days = if capacity_per_day > 0.0 {
        (complexity_minutes as f64) / capacity_per_day
    } else {
        0.0
    };
    if estimated_days.is_sign_negative() {
        estimated_days = 0.0;
    }

    let confidence = estimate_eta_confidence(issue, velocity_samples);
    let delta_days = 0.5_f64.max(estimated_days * (1.0 - confidence) * 0.8);

    let eta = now + duration_days(estimated_days);
    let eta_low = now + duration_days((estimated_days - delta_days).max(0.0));
    let eta_high = now + duration_days(estimated_days + delta_days);

    factors.push(format!("agents: {agents}"));
    if factors.len() > 8 {
        factors.truncate(8);
    }

    Some(EtaEstimate {
        estimated_minutes: complexity_minutes,
        estimated_days,
        eta_date: eta.to_rfc3339(),
        eta_date_low: eta_low.to_rfc3339(),
        eta_date_high: eta_high.to_rfc3339(),
        confidence,
        velocity_minutes_per_day: velocity_per_day,
        agents,
        factors,
    })
}

fn estimate_complexity_minutes(
    issue: &Issue,
    metrics: &GraphMetrics,
    median_minutes: i64,
) -> (i64, Vec<String>) {
    let mut factors = Vec::<String>::new();

    let explicit = issue.estimated_minutes.unwrap_or(0) > 0;
    let mut base_minutes = if explicit {
        i64::from(issue.estimated_minutes.unwrap_or(0))
    } else {
        median_minutes
    };

    let estimate_source = if explicit {
        "explicit"
    } else if base_minutes > 0 {
        "median"
    } else {
        "default"
    };
    if base_minutes <= 0 {
        base_minutes = DEFAULT_ESTIMATED_MINUTES;
    }
    factors.push(format!("estimate: {estimate_source} ({base_minutes}m)"));

    let issue_type = issue.issue_type.trim().to_ascii_lowercase();
    let type_weight = match issue_type.as_str() {
        "chore" => 0.8,
        "feature" => 1.3,
        "epic" => 2.0,
        _ => 1.0,
    };
    factors.push(format!("type: {issue_type}×{type_weight:.1}"));

    let depth = metrics.critical_depth.get(&issue.id).copied().unwrap_or(0) as f64;
    let depth_factor = 1.0 + (depth / 10.0).min(1.0);
    factors.push(format!("depth: {depth:.0}×{depth_factor:.2}"));

    let desc_runes = issue.description.chars().count();
    let desc_factor = 1.0 + ((desc_runes as f64) / 2000.0).min(1.0);
    if desc_runes > 0 {
        factors.push(format!("desc: {desc_runes}r×{desc_factor:.2}"));
    } else {
        factors.push("desc: empty×1.00".to_string());
    }

    let derived =
        truncate_f64_to_i64((base_minutes as f64) * type_weight * depth_factor * desc_factor)
            .unwrap_or(base_minutes);
    (derived.max(1), factors)
}

fn estimate_velocity_minutes_per_day(
    issues: &[Issue],
    issue: &Issue,
    now: DateTime<Utc>,
    median_minutes: i64,
) -> (f64, usize, Vec<String>) {
    let since = now - Duration::days(30);
    if issue.labels.is_empty() {
        let (velocity, samples) =
            velocity_minutes_per_day_for_label(issues, None, since, median_minutes);
        return (
            velocity,
            samples,
            vec![format!("velocity: global ({samples} samples/30d)")],
        );
    }

    let mut best_label = String::new();
    let mut best_velocity = 0.0;
    let mut best_samples = 0usize;

    for label in &issue.labels {
        let (velocity, samples) =
            velocity_minutes_per_day_for_label(issues, Some(label), since, median_minutes);
        if samples == 0 || velocity <= 0.0 {
            continue;
        }

        if best_velocity == 0.0
            || velocity < best_velocity
            || ((velocity - best_velocity).abs() < f64::EPSILON
                && label.to_ascii_lowercase() < best_label.to_ascii_lowercase())
        {
            best_label.clone_from(label);
            best_velocity = velocity;
            best_samples = samples;
        }
    }

    if best_velocity > 0.0 {
        return (
            best_velocity,
            best_samples,
            vec![format!(
                "velocity: label={best_label} ({best_velocity:.0} min/day, {best_samples} samples/30d)"
            )],
        );
    }

    let (velocity, samples) =
        velocity_minutes_per_day_for_label(issues, None, since, median_minutes);
    (
        velocity,
        samples,
        vec![format!("velocity: global ({samples} samples/30d)")],
    )
}

fn velocity_minutes_per_day_for_label(
    issues: &[Issue],
    label: Option<&str>,
    since: DateTime<Utc>,
    median_minutes: i64,
) -> (f64, usize) {
    let mut total_minutes = 0_i64;
    let mut samples = 0usize;

    for issue in issues {
        if !issue.is_closed_like() {
            continue;
        }

        let closed_at = issue.closed_at.or(issue.updated_at);
        let Some(closed_at) = closed_at else {
            continue;
        };
        if closed_at < since {
            continue;
        }

        if label.is_some_and(|needle| !has_label(&issue.labels, needle)) {
            continue;
        }

        let minutes = i64::from(issue.estimated_minutes.unwrap_or(0)).max(0);
        total_minutes += if minutes > 0 {
            minutes
        } else if median_minutes > 0 {
            median_minutes
        } else {
            DEFAULT_ESTIMATED_MINUTES
        };
        samples = samples.saturating_add(1);
    }

    if samples == 0 {
        (0.0, 0)
    } else {
        (total_minutes as f64 / 30.0, samples)
    }
}

fn has_label(labels: &[String], target: &str) -> bool {
    let target = target.to_ascii_lowercase();
    labels
        .iter()
        .any(|label| label.to_ascii_lowercase() == target)
}

fn estimate_eta_confidence(issue: &Issue, velocity_samples: usize) -> f64 {
    let mut confidence = 0.25_f64;

    if issue.estimated_minutes.unwrap_or(0) > 0 {
        confidence += 0.25;
    }

    confidence += if velocity_samples >= 15 {
        0.30
    } else if velocity_samples >= 5 {
        0.20
    } else if velocity_samples >= 1 {
        0.10
    } else {
        -0.05
    };

    if issue.labels.is_empty() {
        confidence -= 0.05;
    }

    clamp(confidence, 0.10, 0.90)
}

fn compute_median_estimated_minutes(issues: &[Issue]) -> i64 {
    let mut estimates = issues
        .iter()
        .filter_map(|issue| issue.estimated_minutes)
        .map(i64::from)
        .filter(|minutes| *minutes > 0)
        .collect::<Vec<_>>();

    if estimates.is_empty() {
        return DEFAULT_ESTIMATED_MINUTES;
    }

    estimates.sort_unstable();
    let mid = estimates.len() / 2;
    if estimates.len() % 2 == 0 {
        (estimates[mid - 1] + estimates[mid]) / 2
    } else {
        estimates[mid]
    }
}

fn duration_days(days: f64) -> Duration {
    if days <= 0.0 || !days.is_finite() {
        return Duration::zero();
    }

    let nanos = truncate_f64_to_i64(days * 86_400.0 * 1_000_000_000.0)
        .unwrap_or(i64::MAX)
        .max(0);
    Duration::nanoseconds(nanos)
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn truncate_f64_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }

    if value >= I64_MAX_F64 {
        return Some(i64::MAX);
    }
    if value <= I64_MIN_F64 {
        return Some(i64::MIN);
    }

    value.trunc().to_string().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::analysis::graph::IssueGraph;
    use crate::model::Issue;

    use super::{estimate_eta_for_issue, estimate_forecast, velocity_minutes_per_day_for_label};

    #[test]
    fn forecast_for_all_open_issues() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                estimated_minutes: Some(90),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                estimated_minutes: Some(30),
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "C".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let output = estimate_forecast(&issues, &graph, &metrics, "all", None, 1);
        assert_eq!(output.summary.count, 2);
        assert_eq!(output.forecasts[0].id, "A");
        assert_eq!(output.forecasts[1].id, "B");
        assert!(output.forecasts[0].estimated_days >= 0.0);
    }

    #[test]
    fn eta_includes_bounds_and_normalizes_agents() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let eta = estimate_eta_for_issue(&issues, &graph, &metrics, "A", 0, Utc::now())
            .expect("eta should be computed");

        assert_eq!(eta.agents, 1);
        assert!(!eta.eta_date.is_empty());
        assert!(!eta.eta_date_low.is_empty());
        assert!(!eta.eta_date_high.is_empty());
    }

    #[test]
    fn velocity_counts_tombstone_as_closed_like() {
        let now = Utc::now();
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                estimated_minutes: Some(120),
                closed_at: Some(now - chrono::Duration::days(1)),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "tombstone".to_string(),
                issue_type: "task".to_string(),
                estimated_minutes: Some(60),
                closed_at: Some(now - chrono::Duration::days(2)),
                ..Issue::default()
            },
        ];

        let (velocity, samples) =
            velocity_minutes_per_day_for_label(&issues, None, now - chrono::Duration::days(30), 60);

        assert_eq!(samples, 2);
        assert!((velocity - 6.0).abs() < 0.001);
    }

    // ── compute_median_estimated_minutes ─────────────────────────────

    #[test]
    fn median_odd_count() {
        let issues = vec![
            Issue {
                estimated_minutes: Some(30),
                ..Issue::default()
            },
            Issue {
                estimated_minutes: Some(60),
                ..Issue::default()
            },
            Issue {
                estimated_minutes: Some(120),
                ..Issue::default()
            },
        ];
        assert_eq!(super::compute_median_estimated_minutes(&issues), 60);
    }

    #[test]
    fn median_even_count() {
        let issues = vec![
            Issue {
                estimated_minutes: Some(30),
                ..Issue::default()
            },
            Issue {
                estimated_minutes: Some(90),
                ..Issue::default()
            },
        ];
        // (30 + 90) / 2 = 60
        assert_eq!(super::compute_median_estimated_minutes(&issues), 60);
    }

    #[test]
    fn median_empty_returns_default() {
        assert_eq!(
            super::compute_median_estimated_minutes(&[]),
            super::DEFAULT_ESTIMATED_MINUTES
        );
    }

    #[test]
    fn median_filters_zero_and_none() {
        let issues = vec![
            Issue {
                estimated_minutes: Some(0),
                ..Issue::default()
            },
            Issue {
                estimated_minutes: None,
                ..Issue::default()
            },
            Issue {
                estimated_minutes: Some(120),
                ..Issue::default()
            },
        ];
        assert_eq!(super::compute_median_estimated_minutes(&issues), 120);
    }

    // ── estimate_complexity_minutes ──────────────────────────────────

    #[test]
    fn complexity_uses_explicit_estimate() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let issue = Issue {
            id: "A".to_string(),
            estimated_minutes: Some(120),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let (minutes, factors) = super::estimate_complexity_minutes(&issue, &metrics, 60);
        // task type_weight=1.0, depth=0 → depth_factor=1.0, empty desc → desc_factor=1.0
        // 120 * 1.0 * 1.0 * 1.0 = 120
        assert_eq!(minutes, 120);
        assert!(factors.iter().any(|f| f.contains("explicit")));
    }

    #[test]
    fn complexity_uses_median_fallback() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let issue = Issue {
            id: "A".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        };
        let (minutes, factors) = super::estimate_complexity_minutes(&issue, &metrics, 90);
        // No explicit estimate → uses median=90, task×1.0, depth=0, empty desc
        assert_eq!(minutes, 90);
        assert!(factors.iter().any(|f| f.contains("median")));
    }

    #[test]
    fn complexity_type_weight_feature() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let issue = Issue {
            id: "A".to_string(),
            estimated_minutes: Some(100),
            issue_type: "feature".to_string(),
            ..Issue::default()
        };
        let (minutes, _) = super::estimate_complexity_minutes(&issue, &metrics, 60);
        // 100 * 1.3 (feature) * 1.0 * 1.0 = 130
        assert_eq!(minutes, 130);
    }

    #[test]
    fn complexity_type_weight_epic() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let issue = Issue {
            id: "A".to_string(),
            estimated_minutes: Some(100),
            issue_type: "epic".to_string(),
            ..Issue::default()
        };
        let (minutes, _) = super::estimate_complexity_minutes(&issue, &metrics, 60);
        // 100 * 2.0 (epic) * 1.0 * 1.0 = 200
        assert_eq!(minutes, 200);
    }

    #[test]
    fn complexity_description_scales_estimate() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let long_desc = "x".repeat(2000);
        let issue = Issue {
            id: "A".to_string(),
            estimated_minutes: Some(100),
            issue_type: "task".to_string(),
            description: long_desc,
            ..Issue::default()
        };
        let (minutes, _) = super::estimate_complexity_minutes(&issue, &metrics, 60);
        // 100 * 1.0 * 1.0 * (1.0 + 2000/2000) = 100 * 2.0 = 200
        assert_eq!(minutes, 200);
    }

    // ── estimate_eta_confidence ──────────────────────────────────────

    #[test]
    fn confidence_base_no_estimate_no_velocity() {
        let issue = Issue { ..Issue::default() };
        let confidence = super::estimate_eta_confidence(&issue, 0);
        // 0.25 (base) + (-0.05) (no velocity) + (-0.05) (no labels) = 0.15
        assert!((confidence - 0.15).abs() < 0.01);
    }

    #[test]
    fn confidence_with_explicit_estimate() {
        let issue = Issue {
            estimated_minutes: Some(60),
            ..Issue::default()
        };
        let confidence = super::estimate_eta_confidence(&issue, 0);
        // 0.25 + 0.25 (estimate) + (-0.05) (no velocity) + (-0.05) (no labels) = 0.40
        assert!((confidence - 0.40).abs() < 0.01);
    }

    #[test]
    fn confidence_high_velocity_samples() {
        let issue = Issue {
            estimated_minutes: Some(60),
            labels: vec!["backend".to_string()],
            ..Issue::default()
        };
        let confidence = super::estimate_eta_confidence(&issue, 20);
        // 0.25 + 0.25 (estimate) + 0.30 (>=15 samples) + 0.0 (has labels) = 0.80
        assert!((confidence - 0.80).abs() < 0.01);
    }

    // ── velocity with label filter ──────────────────────────────────

    #[test]
    fn velocity_label_filter() {
        let now = Utc::now();
        let issues = vec![
            Issue {
                id: "A".to_string(),
                status: "closed".to_string(),
                labels: vec!["backend".to_string()],
                estimated_minutes: Some(120),
                closed_at: Some(now - chrono::Duration::days(5)),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                status: "closed".to_string(),
                labels: vec!["frontend".to_string()],
                estimated_minutes: Some(60),
                closed_at: Some(now - chrono::Duration::days(3)),
                ..Issue::default()
            },
        ];

        let (vel_backend, samples_backend) = velocity_minutes_per_day_for_label(
            &issues,
            Some("backend"),
            now - chrono::Duration::days(30),
            60,
        );
        assert_eq!(samples_backend, 1);
        assert!((vel_backend - 4.0).abs() < 0.01); // 120/30

        let (vel_frontend, samples_frontend) = velocity_minutes_per_day_for_label(
            &issues,
            Some("frontend"),
            now - chrono::Duration::days(30),
            60,
        );
        assert_eq!(samples_frontend, 1);
        assert!((vel_frontend - 2.0).abs() < 0.01); // 60/30
    }

    #[test]
    fn velocity_ignores_old_closures() {
        let now = Utc::now();
        let issues = vec![Issue {
            id: "A".to_string(),
            status: "closed".to_string(),
            estimated_minutes: Some(120),
            closed_at: Some(now - chrono::Duration::days(60)),
            ..Issue::default()
        }];
        let (velocity, samples) =
            velocity_minutes_per_day_for_label(&issues, None, now - chrono::Duration::days(30), 60);
        assert_eq!(samples, 0);
        assert_eq!(velocity, 0.0);
    }

    // ── agents scaling ──────────────────────────────────────────────

    #[test]
    fn more_agents_reduces_eta() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            estimated_minutes: Some(240),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now();

        let eta1 = estimate_eta_for_issue(&issues, &graph, &metrics, "A", 1, now).unwrap();
        let eta3 = estimate_eta_for_issue(&issues, &graph, &metrics, "A", 3, now).unwrap();
        assert!(
            eta3.estimated_days < eta1.estimated_days || eta1.estimated_days == 0.0,
            "3 agents should complete faster than 1"
        );
    }

    // ── forecast label filter ───────────────────────────────────────

    #[test]
    fn forecast_respects_label_filter() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                labels: vec!["backend".to_string()],
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                labels: vec!["frontend".to_string()],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let output = estimate_forecast(&issues, &graph, &metrics, "all", Some("backend"), 1);
        assert_eq!(output.summary.count, 1);
        assert_eq!(output.forecasts[0].id, "A");
    }

    // ── edge case helpers ───────────────────────────────────────────

    #[test]
    fn duration_days_handles_zero_and_negative() {
        assert_eq!(super::duration_days(0.0), chrono::Duration::zero());
        assert_eq!(super::duration_days(-5.0), chrono::Duration::zero());
    }

    #[test]
    fn truncate_f64_to_i64_edge_cases() {
        assert_eq!(super::truncate_f64_to_i64(f64::NAN), None);
        assert_eq!(super::truncate_f64_to_i64(f64::INFINITY), None);
        assert_eq!(super::truncate_f64_to_i64(42.9), Some(42));
        assert_eq!(super::truncate_f64_to_i64(-3.7), Some(-3));
        assert_eq!(super::truncate_f64_to_i64(1e19), Some(i64::MAX));
        assert_eq!(super::truncate_f64_to_i64(-1e19), Some(i64::MIN));
    }

    #[test]
    fn clamp_works_correctly() {
        assert_eq!(super::clamp(0.5, 0.1, 0.9), 0.5);
        assert_eq!(super::clamp(-0.5, 0.1, 0.9), 0.1);
        assert_eq!(super::clamp(1.5, 0.1, 0.9), 0.9);
    }

    #[test]
    fn forecast_single_issue_by_id() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "B".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let output = estimate_forecast(&issues, &graph, &metrics, "B", None, 1);
        assert_eq!(output.summary.count, 1);
        assert_eq!(output.forecasts[0].id, "B");
    }
}
