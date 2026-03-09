use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;

use crate::analysis::graph::{GraphMetrics, IssueGraph};
use crate::model::Issue;

// ---------------------------------------------------------------------------
// ImpactScore – 8-component transparent scoring (matches Go's priority.go)
// ---------------------------------------------------------------------------

/// Weight constants for ImpactScore components (must sum to 1.0).
const W_PAGERANK: f64 = 0.22;
const W_BETWEENNESS: f64 = 0.20;
const W_BLOCKER_RATIO: f64 = 0.13;
const W_STALENESS: f64 = 0.05;
const W_PRIORITY_BOOST: f64 = 0.10;
const W_TIME_TO_IMPACT: f64 = 0.10;
const W_URGENCY: f64 = 0.10;
const W_RISK: f64 = 0.10;

/// One component of the impact score breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct ScoreComponent {
    pub name: &'static str,
    pub weight: f64,
    pub raw: f64,
    pub normalized: f64,
    pub weighted: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// Full impact score with transparent 8-component breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct ImpactScore {
    pub issue_id: String,
    pub score: f64,
    pub breakdown: Vec<ScoreComponent>,
}

/// Context needed to compute ImpactScores across a set of issues.
struct ScoringContext {
    max_pagerank: f64,
    max_betweenness: f64,
    max_blocks: f64,
    total_open: usize,
    now_ts: i64,
}

impl ScoringContext {
    fn from_metrics(metrics: &GraphMetrics, total_open: usize) -> Self {
        Self {
            max_pagerank: metrics
                .pagerank
                .values()
                .copied()
                .fold(0.0_f64, f64::max)
                .max(1e-9),
            max_betweenness: metrics
                .betweenness
                .values()
                .copied()
                .fold(0.0_f64, f64::max)
                .max(1e-9),
            max_blocks: metrics
                .blocks_count
                .values()
                .copied()
                .max()
                .unwrap_or(1)
                .max(1) as f64,
            total_open,
            now_ts: Utc::now().timestamp(),
        }
    }
}

/// Compute the full 8-component ImpactScore for a single issue.
fn compute_impact_score(
    issue: &Issue,
    metrics: &GraphMetrics,
    graph: &IssueGraph,
    ctx: &ScoringContext,
    weight_adjustments: &HashMap<String, f64>,
) -> ImpactScore {
    // 1. PageRank (graph centrality)
    let pr_raw = metrics.pagerank.get(&issue.id).copied().unwrap_or_default();
    let pr_norm = pr_raw / ctx.max_pagerank;

    // 2. Betweenness (bridge importance)
    let bt_raw = metrics
        .betweenness
        .get(&issue.id)
        .copied()
        .unwrap_or_default();
    let bt_norm = bt_raw / ctx.max_betweenness;

    // 3. BlockerRatio (fraction of open issues this blocks)
    let blocks = metrics
        .blocks_count
        .get(&issue.id)
        .copied()
        .unwrap_or_default();
    let br_raw = blocks as f64;
    let br_norm = if ctx.total_open > 1 {
        br_raw / ctx.max_blocks
    } else {
        0.0
    };

    // 4. Staleness (how long since last update — stale items get deprioritized)
    let updated_ts = issue
        .updated_at
        .map(|dt| dt.timestamp())
        .unwrap_or(ctx.now_ts);
    let days_stale = ((ctx.now_ts - updated_ts) as f64 / 86400.0).max(0.0);
    // Inverse staleness: recently updated items score higher.  Cap at 90 days.
    let staleness_norm = 1.0 - (days_stale / 90.0).min(1.0);

    // 5. PriorityBoost (declared priority)
    let priority_norm = issue.priority_normalized();

    // 6. TimeToImpact (how quickly completion propagates value)
    let depth = metrics.critical_depth.get(&issue.id).copied().unwrap_or(0);
    let max_depth = metrics
        .critical_depth
        .values()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    // Higher depth = root blocker = faster propagation of value.
    let tti_raw = depth as f64;
    let tti_norm = tti_raw / max_depth as f64;
    let tti_explanation = if depth > 0 {
        Some(format!(
            "critical depth {depth}/{max_depth} — completing this unlocks a chain of {blocks} issue(s)"
        ))
    } else {
        None
    };

    // 7. Urgency (status-based urgency signal)
    let (urgency_norm, urgency_explanation) = match issue.normalized_status().as_str() {
        "in_progress" => (1.0, Some("actively being worked on".to_string())),
        "open" => (0.8, None),
        "review" => (0.7, Some("awaiting review".to_string())),
        "blocked" => (0.5, Some("blocked — resolve blockers first".to_string())),
        "deferred" | "pinned" => (0.3, Some("deferred/pinned — lower urgency".to_string())),
        _ => (0.6, None),
    };

    // 8. Risk (signals that increase execution risk)
    let open_blockers = graph.open_blockers(&issue.id).len();
    let is_in_cycle = metrics.cycles.iter().any(|cycle| cycle.contains(&issue.id));
    let is_articulation = metrics.articulation_points.contains(&issue.id);
    let mut risk_signals = Vec::new();
    let mut risk_raw = 0.0_f64;
    if open_blockers > 0 {
        risk_raw += 0.3;
        risk_signals.push(format!("{open_blockers} open blocker(s)"));
    }
    if is_in_cycle {
        risk_raw += 0.4;
        risk_signals.push("part of a dependency cycle".to_string());
    }
    if is_articulation {
        risk_raw += 0.3;
        risk_signals.push("articulation point (removing breaks graph)".to_string());
    }
    let risk_norm = risk_raw.min(1.0);
    // For scoring, LOWER risk is better — invert so low-risk items score higher.
    let risk_benefit = 1.0 - risk_norm;
    let risk_explanation = if risk_signals.is_empty() {
        None
    } else {
        Some(risk_signals.join("; "))
    };

    // Helper: look up feedback adjustment for a component (default 1.0 = no change).
    let adj = |name: &str| -> f64 {
        weight_adjustments
            .get(name)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.5, 2.0)
    };

    // Assemble components with feedback-adjusted weights.
    let w_pr = W_PAGERANK * adj("PageRank");
    let w_bt = W_BETWEENNESS * adj("Betweenness");
    let w_br = W_BLOCKER_RATIO * adj("BlockerRatio");
    let w_st = W_STALENESS * adj("Staleness");
    let w_pb = W_PRIORITY_BOOST * adj("PriorityBoost");
    let w_tti = W_TIME_TO_IMPACT * adj("TimeToImpact");
    let w_urg = W_URGENCY * adj("Urgency");
    let w_risk = W_RISK * adj("Risk");

    // Renormalize so adjusted weights still sum to 1.0.
    let w_sum = w_pr + w_bt + w_br + w_st + w_pb + w_tti + w_urg + w_risk;
    let norm = if w_sum > 0.0 { 1.0 / w_sum } else { 1.0 };

    let components = vec![
        ScoreComponent {
            name: "PageRank",
            weight: w_pr * norm,
            raw: pr_raw,
            normalized: pr_norm,
            weighted: w_pr * norm * pr_norm,
            explanation: None,
        },
        ScoreComponent {
            name: "Betweenness",
            weight: w_bt * norm,
            raw: bt_raw,
            normalized: bt_norm,
            weighted: w_bt * norm * bt_norm,
            explanation: None,
        },
        ScoreComponent {
            name: "BlockerRatio",
            weight: w_br * norm,
            raw: br_raw,
            normalized: br_norm,
            weighted: w_br * norm * br_norm,
            explanation: if blocks > 0 {
                Some(format!("blocks {blocks} issue(s)"))
            } else {
                None
            },
        },
        ScoreComponent {
            name: "Staleness",
            weight: w_st * norm,
            raw: days_stale,
            normalized: staleness_norm,
            weighted: w_st * norm * staleness_norm,
            explanation: if days_stale > 30.0 {
                Some(format!("stale: {days_stale:.0} days since last update"))
            } else {
                None
            },
        },
        ScoreComponent {
            name: "PriorityBoost",
            weight: w_pb * norm,
            raw: issue.priority as f64,
            normalized: priority_norm,
            weighted: w_pb * norm * priority_norm,
            explanation: None,
        },
        ScoreComponent {
            name: "TimeToImpact",
            weight: w_tti * norm,
            raw: tti_raw,
            normalized: tti_norm,
            weighted: w_tti * norm * tti_norm,
            explanation: tti_explanation,
        },
        ScoreComponent {
            name: "Urgency",
            weight: w_urg * norm,
            raw: urgency_norm, // urgency is already a normalized signal
            normalized: urgency_norm,
            weighted: w_urg * norm * urgency_norm,
            explanation: urgency_explanation,
        },
        ScoreComponent {
            name: "Risk",
            weight: w_risk * norm,
            raw: risk_norm,
            normalized: risk_benefit,
            weighted: w_risk * norm * risk_benefit,
            explanation: risk_explanation,
        },
    ];

    let score: f64 = components.iter().map(|c| c.weighted).sum();

    ImpactScore {
        issue_id: issue.id.clone(),
        score: score.clamp(0.0, 1.0),
        breakdown: components,
    }
}

// ---------------------------------------------------------------------------
// Original triage types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct QuickPick {
    pub id: String,
    pub title: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub unblocks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub id: String,
    pub title: String,
    pub score: f64,
    pub impact_score: f64,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub unblocks: usize,
    pub status: String,
    pub priority: i32,
    pub issue_type: String,
    pub labels: Vec<String>,
    pub assignee: String,
    pub claim_command: String,
    pub show_command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakdown: Option<Vec<ScoreComponent>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockerToClear {
    pub id: String,
    pub title: String,
    pub status: String,
    pub unblocks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationsByTrack {
    pub track_id: String,
    pub top_pick: Option<Recommendation>,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationsByLabel {
    pub label: String,
    pub top_pick: Option<Recommendation>,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuickRef {
    pub total_open: usize,
    pub total_actionable: usize,
    pub top_picks: Vec<QuickPick>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriageResult {
    pub quick_ref: QuickRef,
    pub recommendations: Vec<Recommendation>,
    pub quick_wins: Vec<Recommendation>,
    pub blockers_to_clear: Vec<BlockerToClear>,
    pub recommendations_by_track: Vec<RecommendationsByTrack>,
    pub recommendations_by_label: Vec<RecommendationsByLabel>,
}

/// Configurable scoring weights and thresholds for triage (matches Go's TriageScoringOptions).
#[derive(Debug, Clone, Serialize)]
pub struct TriageScoringOptions {
    /// Weight for the base ImpactScore (0.0–1.0).
    pub base_score_weight: f64,
    /// Weight for the unblock boost (0.0–1.0).
    pub unblock_boost_weight: f64,
    /// Weight for the quick-win bonus (0.0–1.0).
    pub quick_win_weight: f64,
    /// Issues that unblock >= this many others get the full unblock boost.
    pub unblock_threshold: usize,
    /// Issues with critical_depth <= this are considered quick wins.
    pub quick_win_max_depth: usize,
    /// Phase 2: incorporate label health into scoring.
    pub enable_label_health: bool,
    /// Phase 3: penalize items already claimed by another agent.
    pub enable_claim_penalty: bool,
    /// Phase 4: boost items in high-attention labels.
    pub enable_attention_score: bool,
    /// Identity of the agent computing triage (for claim penalty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_by_agent: Option<String>,
    /// Per-component weight multipliers from feedback (e.g. "PageRank" -> 1.1).
    /// Applied multiplicatively to the default component weights during scoring.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub weight_adjustments: HashMap<String, f64>,
}

impl Default for TriageScoringOptions {
    fn default() -> Self {
        Self {
            base_score_weight: 0.70,
            unblock_boost_weight: 0.15,
            quick_win_weight: 0.15,
            unblock_threshold: 5,
            quick_win_max_depth: 2,
            enable_label_health: false,
            enable_claim_penalty: false,
            enable_attention_score: false,
            claimed_by_agent: None,
            weight_adjustments: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TriageOptions {
    pub group_by_track: bool,
    pub group_by_label: bool,
    pub max_recommendations: usize,
    pub scoring: TriageScoringOptions,
}

#[derive(Debug, Clone)]
pub struct TriageComputation {
    pub result: TriageResult,
    pub score_by_id: HashMap<String, f64>,
}

pub fn compute_triage(
    issues: &[Issue],
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    options: &TriageOptions,
) -> TriageComputation {
    let max_recommendations = if options.max_recommendations == 0 {
        50
    } else {
        options.max_recommendations
    };

    let actionable: HashSet<String> = graph.actionable_ids().into_iter().collect();
    let total_open = issues.iter().filter(|issue| issue.is_open_like()).count();

    let ctx = ScoringContext::from_metrics(metrics, total_open);

    let mut recommendations = Vec::<Recommendation>::new();
    let mut score_by_id = HashMap::<String, f64>::new();

    let scoring = &options.scoring;

    for issue in issues.iter().filter(|issue| actionable.contains(&issue.id)) {
        let impact = compute_impact_score(issue, metrics, graph, &ctx, &scoring.weight_adjustments);
        let base_score = impact.score;

        let unblocks = metrics
            .blocks_count
            .get(&issue.id)
            .copied()
            .unwrap_or_default();

        // Unblock boost: scales linearly up to the threshold then caps at 1.0.
        let unblock_boost = if scoring.unblock_threshold > 0 && unblocks > 0 {
            (unblocks as f64 / scoring.unblock_threshold as f64).min(1.0)
        } else {
            0.0
        };

        // Quick-win bonus: low-depth, high-priority items that can be done fast.
        let depth = metrics.critical_depth.get(&issue.id).copied().unwrap_or(0);
        let quick_win_bonus = if depth <= scoring.quick_win_max_depth
            && issue.priority <= 2
            && issue.estimated_minutes.is_none_or(|m| m <= 120)
        {
            1.0
        } else {
            0.0
        };

        // Composite triage score.
        let score = (scoring.base_score_weight * base_score
            + scoring.unblock_boost_weight * unblock_boost
            + scoring.quick_win_weight * quick_win_bonus)
            .clamp(0.0, 1.0);

        // Build human-readable reasons from the top contributing components.
        let mut reasons = Vec::<String>::new();
        let pr_norm = impact.breakdown[0].normalized;
        if pr_norm > 0.6 {
            reasons.push("high graph centrality".to_string());
        }
        if unblocks > 0 {
            reasons.push(format!("unblocks {unblocks} issues"));
        }
        if issue.priority <= 2 {
            reasons.push("high declared priority".to_string());
        }
        if unblock_boost > 0.5 {
            reasons.push(format!("unblock boost {:.0}%", unblock_boost * 100.0));
        }
        if quick_win_bonus > 0.0 {
            reasons.push("quick win candidate".to_string());
        }
        if reasons.is_empty() {
            reasons.push("ready to execute now".to_string());
        }

        score_by_id.insert(issue.id.clone(), score);

        recommendations.push(Recommendation {
            id: issue.id.clone(),
            title: issue.title.clone(),
            score,
            impact_score: base_score,
            confidence: (0.5 + 0.5 * score).clamp(0.0, 1.0),
            reasons,
            unblocks,
            status: issue.status.clone(),
            priority: issue.priority,
            issue_type: issue.issue_type.clone(),
            labels: issue.labels.clone(),
            assignee: issue.assignee.clone(),
            claim_command: format!("br update {} --status=in_progress", issue.id),
            show_command: format!("br show {}", issue.id),
            breakdown: Some(impact.breakdown),
        });
    }

    recommendations.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    recommendations.truncate(max_recommendations);

    let top_picks = recommendations
        .iter()
        .take(3)
        .map(|rec| QuickPick {
            id: rec.id.clone(),
            title: rec.title.clone(),
            score: rec.score,
            reasons: rec.reasons.clone(),
            unblocks: rec.unblocks,
        })
        .collect();

    let quick_wins = recommendations
        .iter()
        .filter(|rec| {
            issues
                .iter()
                .find(|issue| issue.id == rec.id)
                .is_some_and(|issue| {
                    issue.estimated_minutes.is_some_and(|mins| mins <= 120)
                        || (issue.priority <= 2 && rec.unblocks > 0)
                })
        })
        .take(10)
        .cloned()
        .collect();

    let blockers_to_clear = compute_blockers_to_clear(issues, graph, metrics, &actionable);

    let recommendations_by_track = if options.group_by_track {
        compute_recommendations_by_track(graph, &recommendations)
    } else {
        Vec::new()
    };

    let recommendations_by_label = if options.group_by_label {
        compute_recommendations_by_label(&recommendations)
    } else {
        Vec::new()
    };

    let result = TriageResult {
        quick_ref: QuickRef {
            total_open,
            total_actionable: actionable.len(),
            top_picks,
        },
        recommendations,
        quick_wins,
        blockers_to_clear,
        recommendations_by_track,
        recommendations_by_label,
    };

    TriageComputation {
        result,
        score_by_id,
    }
}

fn compute_blockers_to_clear(
    issues: &[Issue],
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    actionable: &HashSet<String>,
) -> Vec<BlockerToClear> {
    let mut blockers = Vec::<BlockerToClear>::new();

    for issue in issues
        .iter()
        .filter(|issue| issue.is_open_like() && !actionable.contains(&issue.id))
    {
        let unblocks = metrics
            .blocks_count
            .get(&issue.id)
            .copied()
            .unwrap_or_default();
        if unblocks == 0 {
            continue;
        }
        if graph.open_blockers(&issue.id).is_empty() {
            continue;
        }

        blockers.push(BlockerToClear {
            id: issue.id.clone(),
            title: issue.title.clone(),
            status: issue.status.clone(),
            unblocks,
        });
    }

    blockers.sort_by(|left, right| {
        right
            .unblocks
            .cmp(&left.unblocks)
            .then_with(|| left.id.cmp(&right.id))
    });
    blockers.truncate(15);
    blockers
}

fn compute_recommendations_by_track(
    graph: &IssueGraph,
    recommendations: &[Recommendation],
) -> Vec<RecommendationsByTrack> {
    let component_lookup = graph.connected_open_components();
    let rec_by_id: HashMap<&str, &Recommendation> = recommendations
        .iter()
        .map(|rec| (rec.id.as_str(), rec))
        .collect();

    let mut by_track = Vec::<RecommendationsByTrack>::new();

    for (index, component) in component_lookup.iter().enumerate() {
        let mut items: Vec<&Recommendation> = component
            .iter()
            .filter_map(|id| rec_by_id.get(id.as_str()).copied())
            .collect();

        items.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });

        if items.is_empty() {
            continue;
        }

        by_track.push(RecommendationsByTrack {
            track_id: format!("track-{}", index + 1),
            top_pick: items.first().map(|item| (*item).clone()),
            item_ids: items.into_iter().map(|item| item.id.clone()).collect(),
        });
    }

    by_track
}

fn compute_recommendations_by_label(
    recommendations: &[Recommendation],
) -> Vec<RecommendationsByLabel> {
    let mut groups: BTreeMap<String, Vec<Recommendation>> = BTreeMap::new();

    for rec in recommendations {
        for label in &rec.labels {
            groups.entry(label.clone()).or_default().push(rec.clone());
        }
    }

    let mut out = Vec::<RecommendationsByLabel>::new();
    for (label, mut recs) in groups {
        recs.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });

        out.push(RecommendationsByLabel {
            label,
            top_pick: recs.first().cloned(),
            item_ids: recs.into_iter().map(|rec| rec.id).collect(),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::analysis::graph::IssueGraph;
    use crate::model::Issue;

    use super::{TriageOptions, TriageScoringOptions, compute_triage};

    #[test]
    fn triage_produces_recommendations() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Root".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Depends on A".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: true,
                group_by_label: true,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );

        assert_eq!(triage.result.quick_ref.total_open, 2);
        assert_eq!(triage.result.quick_ref.total_actionable, 1);
        assert_eq!(triage.result.recommendations.len(), 1);
        assert_eq!(triage.result.recommendations[0].id, "A");
    }

    #[test]
    fn triage_empty_issues_produces_zero_recommendations() {
        let issues: Vec<Issue> = vec![];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: false,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );
        assert_eq!(triage.result.quick_ref.total_open, 0);
        assert_eq!(triage.result.quick_ref.total_actionable, 0);
        assert!(triage.result.recommendations.is_empty());
        assert!(triage.result.blockers_to_clear.is_empty());
    }

    #[test]
    fn triage_all_closed_produces_no_actionable() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Done".to_string(),
                status: "closed".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Also done".to_string(),
                status: "tombstone".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: false,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );
        assert_eq!(triage.result.quick_ref.total_open, 0);
        assert!(triage.result.recommendations.is_empty());
    }

    #[test]
    fn triage_max_recommendations_limits_output() {
        let issues: Vec<Issue> = (0..20)
            .map(|i| Issue {
                id: format!("X-{i}"),
                title: format!("Issue {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: false,
                max_recommendations: 5,
                ..TriageOptions::default()
            },
        );
        assert!(triage.result.recommendations.len() <= 5);
    }

    #[test]
    fn triage_scores_are_sorted_descending() {
        let issues: Vec<Issue> = (0..5)
            .map(|i| Issue {
                id: format!("P-{i}"),
                title: format!("Task {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: i + 1,
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: false,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );
        let scores: Vec<f64> = triage
            .result
            .recommendations
            .iter()
            .map(|r| r.score)
            .collect();
        for w in scores.windows(2) {
            assert!(
                w[0] >= w[1],
                "scores should be descending: {} >= {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn triage_blockers_to_clear_identifies_chained_blockers() {
        // Chain: A (open, actionable) blocks B (open, blocked by A, blocks C+D)
        // B is not actionable (blocked by A) but blocks C+D => should be in blockers_to_clear
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Root".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Middle".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "Leaf 1".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 3,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "B".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "D".to_string(),
                title: "Leaf 2".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 3,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "B".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: false,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );
        // B should be in blockers_to_clear (blocked by A, blocks C+D)
        let b_blocker = triage.result.blockers_to_clear.iter().find(|b| b.id == "B");
        assert!(
            b_blocker.is_some(),
            "B should be in blockers_to_clear (got {:?})",
            triage.result.blockers_to_clear
        );
        assert!(
            b_blocker.unwrap().unblocks >= 2,
            "B should unblock 2+ issues"
        );
    }

    #[test]
    fn triage_group_by_label_produces_label_groups() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "UI fix".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                labels: vec!["ui".to_string()],
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "API fix".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                labels: vec!["api".to_string()],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                group_by_track: false,
                group_by_label: true,
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );
        assert!(
            !triage.result.recommendations_by_label.is_empty(),
            "should group by label"
        );
    }

    // -- ImpactScore tests --

    #[test]
    fn impact_score_has_8_components() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Root".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 1);
        let score =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());

        assert_eq!(score.breakdown.len(), 8);
        assert_eq!(score.breakdown[0].name, "PageRank");
        assert_eq!(score.breakdown[1].name, "Betweenness");
        assert_eq!(score.breakdown[2].name, "BlockerRatio");
        assert_eq!(score.breakdown[3].name, "Staleness");
        assert_eq!(score.breakdown[4].name, "PriorityBoost");
        assert_eq!(score.breakdown[5].name, "TimeToImpact");
        assert_eq!(score.breakdown[6].name, "Urgency");
        assert_eq!(score.breakdown[7].name, "Risk");
    }

    #[test]
    fn impact_score_weights_sum_to_one() {
        let total: f64 = [
            super::W_PAGERANK,
            super::W_BETWEENNESS,
            super::W_BLOCKER_RATIO,
            super::W_STALENESS,
            super::W_PRIORITY_BOOST,
            super::W_TIME_TO_IMPACT,
            super::W_URGENCY,
            super::W_RISK,
        ]
        .iter()
        .sum();
        assert!(
            (total - 1.0).abs() < 1e-9,
            "weights should sum to 1.0, got {total}"
        );
    }

    #[test]
    fn impact_score_bounded_zero_one() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Root".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Leaf".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 5,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 2);

        for issue in &issues {
            let score = super::compute_impact_score(issue, &metrics, &graph, &ctx, &HashMap::new());
            assert!(
                score.score >= 0.0 && score.score <= 1.0,
                "score for {} should be in [0,1], got {}",
                issue.id,
                score.score
            );
            for comp in &score.breakdown {
                assert!(
                    comp.normalized >= 0.0 && comp.normalized <= 1.0,
                    "normalized {} for {} should be in [0,1], got {}",
                    comp.name,
                    issue.id,
                    comp.normalized
                );
            }
        }
    }

    #[test]
    fn impact_score_empty_graph() {
        let issues: Vec<Issue> = Vec::new();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(&issues, &graph, &metrics, &TriageOptions::default());
        assert!(triage.result.recommendations.is_empty());
    }

    #[test]
    fn impact_score_single_node() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Only".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 1);
        let score =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());

        // Single node: PageRank=1.0 (normalized to 1.0), no blockers, no risk.
        assert!(score.score > 0.0);
        assert_eq!(score.breakdown.len(), 8);
    }

    #[test]
    fn impact_score_blocker_scores_higher_than_leaf() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Root blocker".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Blocked leaf".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 4,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 2);

        let score_a =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());
        let score_b =
            super::compute_impact_score(&issues[1], &metrics, &graph, &ctx, &HashMap::new());

        assert!(
            score_a.score > score_b.score,
            "root blocker ({}) should score higher than blocked leaf ({})",
            score_a.score,
            score_b.score
        );
    }

    #[test]
    fn impact_score_breakdown_included_in_recommendations() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Task".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );

        assert_eq!(triage.result.recommendations.len(), 1);
        let rec = &triage.result.recommendations[0];
        assert!(rec.breakdown.is_some(), "breakdown should be present");
        let bd = rec.breakdown.as_ref().unwrap();
        assert_eq!(bd.len(), 8);
    }

    #[test]
    fn impact_score_risk_penalizes_cyclic_issues() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "In cycle".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "B".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "In cycle".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 2);
        let score =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());

        // Risk component should be non-zero (cycle detected).
        let risk = &score.breakdown[7];
        assert_eq!(risk.name, "Risk");
        assert!(
            risk.raw > 0.0,
            "risk raw should be > 0 for cyclic issue, got {}",
            risk.raw
        );
        assert!(risk.explanation.is_some());
    }

    #[test]
    fn impact_score_large_graph_correctness() {
        let mut issues: Vec<Issue> = (0..100)
            .map(|i| Issue {
                id: format!("N-{i}"),
                title: format!("Node {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: (i % 4) + 1,
                ..Issue::default()
            })
            .collect();
        // Add a dependency chain: N-1 depends on N-0, N-2 on N-1, etc. for first 20.
        for i in 1..20 {
            issues[i].dependencies.push(crate::model::Dependency {
                issue_id: format!("N-{i}"),
                depends_on_id: format!("N-{}", i - 1),
                dep_type: "blocks".to_string(),
                ..crate::model::Dependency::default()
            });
        }

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, issues.len());

        for issue in &issues {
            let score = super::compute_impact_score(issue, &metrics, &graph, &ctx, &HashMap::new());
            assert!(
                score.score >= 0.0 && score.score <= 1.0,
                "score for {} out of range: {}",
                issue.id,
                score.score
            );
            assert_eq!(score.breakdown.len(), 8);
            // Weighted sum should match the score (within float tolerance).
            let sum: f64 = score.breakdown.iter().map(|c| c.weighted).sum();
            assert!(
                (sum - score.score).abs() < 1e-9,
                "breakdown sum ({sum}) != score ({}) for {}",
                score.score,
                issue.id,
            );
        }
    }

    // -----------------------------------------------------------------------
    // TriageScoringOptions tests
    // -----------------------------------------------------------------------

    #[test]
    fn scoring_options_defaults_match_go() {
        let opts = TriageScoringOptions::default();
        assert!((opts.base_score_weight - 0.70).abs() < 1e-9);
        assert!((opts.unblock_boost_weight - 0.15).abs() < 1e-9);
        assert!((opts.quick_win_weight - 0.15).abs() < 1e-9);
        assert_eq!(opts.unblock_threshold, 5);
        assert_eq!(opts.quick_win_max_depth, 2);
        assert!(!opts.enable_label_health);
        assert!(!opts.enable_claim_penalty);
        assert!(!opts.enable_attention_score);
        assert!(opts.claimed_by_agent.is_none());
        // Weights must sum to 1.0
        let sum = opts.base_score_weight + opts.unblock_boost_weight + opts.quick_win_weight;
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "weights should sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn scoring_options_serializes_to_json() {
        let opts = TriageScoringOptions::default();
        let json = serde_json::to_value(&opts).unwrap();
        assert_eq!(json["base_score_weight"], 0.7);
        assert_eq!(json["unblock_boost_weight"], 0.15);
        assert_eq!(json["quick_win_weight"], 0.15);
        assert_eq!(json["unblock_threshold"], 5);
        assert_eq!(json["quick_win_max_depth"], 2);
        assert_eq!(json["enable_label_health"], false);
        // claimed_by_agent should be absent (skip_serializing_if)
        assert!(json.get("claimed_by_agent").is_none());
    }

    #[test]
    fn scoring_options_unblock_boost_increases_score() {
        // Issue A blocks 6 issues (above threshold of 5) → should get full unblock boost.
        // Issue B blocks nothing → no unblock boost.
        let mut issues = vec![
            Issue {
                id: "A".to_string(),
                title: "Blocker".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Leaf".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                ..Issue::default()
            },
        ];

        // Create 6 issues that depend on A.
        for i in 0..6 {
            issues.push(Issue {
                id: format!("D{i}"),
                title: format!("Dep {i}"),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![crate::model::Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            });
        }

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );

        let rec_a = triage.result.recommendations.iter().find(|r| r.id == "A");
        let rec_b = triage.result.recommendations.iter().find(|r| r.id == "B");
        assert!(rec_a.is_some(), "A should be recommended");
        assert!(rec_b.is_some(), "B should be recommended");
        assert!(
            rec_a.unwrap().score > rec_b.unwrap().score,
            "A (blocker with unblock boost) should score higher than B"
        );
    }

    #[test]
    fn scoring_options_custom_weights_change_ranking() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "High priority quick win".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                estimated_minutes: Some(60),
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Low priority".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 4,
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        // Heavy quick-win weighting should strongly favor A.
        let triage_qw = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                max_recommendations: 10,
                scoring: TriageScoringOptions {
                    base_score_weight: 0.2,
                    unblock_boost_weight: 0.0,
                    quick_win_weight: 0.8,
                    ..TriageScoringOptions::default()
                },
                ..TriageOptions::default()
            },
        );

        let a_score = triage_qw
            .result
            .recommendations
            .iter()
            .find(|r| r.id == "A")
            .map(|r| r.score)
            .unwrap_or(0.0);
        let b_score = triage_qw
            .result
            .recommendations
            .iter()
            .find(|r| r.id == "B")
            .map(|r| r.score)
            .unwrap_or(0.0);

        assert!(
            a_score > b_score,
            "A (quick win) should score much higher than B with heavy quick-win weight"
        );
    }

    #[test]
    fn scoring_options_empty_graph_no_panic() {
        let issues: Vec<Issue> = vec![];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                scoring: TriageScoringOptions {
                    base_score_weight: 0.5,
                    unblock_boost_weight: 0.3,
                    quick_win_weight: 0.2,
                    ..TriageScoringOptions::default()
                },
                ..TriageOptions::default()
            },
        );
        assert!(triage.result.recommendations.is_empty());
    }

    #[test]
    fn scoring_options_single_node() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Solo".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let triage = compute_triage(
            &issues,
            &graph,
            &metrics,
            &TriageOptions {
                max_recommendations: 10,
                ..TriageOptions::default()
            },
        );

        assert_eq!(triage.result.recommendations.len(), 1);
        let rec = &triage.result.recommendations[0];
        assert!(rec.score >= 0.0 && rec.score <= 1.0);
        // impact_score should still be present.
        assert!(rec.impact_score >= 0.0 && rec.impact_score <= 1.0);
    }

    #[test]
    fn feedback_weight_adjustments_shift_scores() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "High centrality".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "B".to_string(),
                title: "Depends on A".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![crate::model::Dependency {
                    issue_id: "B".to_string(),
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..crate::model::Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 2);

        // Baseline: no adjustments
        let baseline =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());

        // With PageRank boosted to 2x
        let mut adjustments = HashMap::new();
        adjustments.insert("PageRank".to_string(), 2.0);
        let boosted = super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &adjustments);

        // The PageRank component weight should be higher in the boosted version.
        let baseline_pr_weight = baseline.breakdown[0].weight;
        let boosted_pr_weight = boosted.breakdown[0].weight;
        assert!(
            boosted_pr_weight > baseline_pr_weight,
            "boosted PageRank weight {boosted_pr_weight} should exceed baseline {baseline_pr_weight}"
        );

        // Scores remain in valid range.
        assert!(boosted.score >= 0.0 && boosted.score <= 1.0);
    }

    #[test]
    fn feedback_empty_adjustments_match_baseline() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Solo".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 1,
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 1);

        let no_adj =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &HashMap::new());
        let empty_map: HashMap<String, f64> = HashMap::new();
        let with_empty =
            super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &empty_map);

        assert!(
            (no_adj.score - with_empty.score).abs() < 1e-12,
            "empty adjustments should produce identical scores"
        );
    }

    #[test]
    fn feedback_adjustments_renormalize_weights() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "Test".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let ctx = super::ScoringContext::from_metrics(&metrics, 1);

        // All weights doubled → should renormalize to sum=1.0
        let mut adjustments = HashMap::new();
        for name in &[
            "PageRank",
            "Betweenness",
            "BlockerRatio",
            "Staleness",
            "PriorityBoost",
            "TimeToImpact",
            "Urgency",
            "Risk",
        ] {
            adjustments.insert(name.to_string(), 2.0);
        }
        let score = super::compute_impact_score(&issues[0], &metrics, &graph, &ctx, &adjustments);

        let weight_sum: f64 = score.breakdown.iter().map(|c| c.weight).sum();
        assert!(
            (weight_sum - 1.0).abs() < 1e-9,
            "adjusted weights should sum to 1.0, got {weight_sum}"
        );
    }
}
