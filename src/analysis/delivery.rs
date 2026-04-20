//! `--robot-delivery` — classification-only projection of the *delivery posture*
//! of the open work graph. No overlay, no graph traversal beyond what the
//! analyzer already computed.
//!
//! Two classifications, both priority-ordered (first match wins — so
//! percentages sum to 100 without double-counting):
//!
//! - **flow_distribution**: Risk > Debt > Defects > Features
//!   The Reinertsen capacity-split vocabulary already used in delivery-team
//!   analytics. Operators want to know whether the graph is mostly reactive
//!   (defects + risk) or investment (debt + features).
//!
//! - **urgency_profile**: Expedite > Fixed-Date > Intangible > Standard
//!   Reinertsen urgency cohorts, keyed off priority, due dates, and labels.
//!
//! Plus a `milestone_pressure` list built from issues that carry a
//! `due_date`, keeping cross-surface coherence with `--robot-alerts`.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::model::Issue;

/// Schema version for the robot-delivery payload. See
/// [`crate::analysis::economics::ECONOMICS_SCHEMA_VERSION`] for the bump
/// rules; same contract here.
pub const DELIVERY_SCHEMA_VERSION: &str = "1";

/// Labels that, when present on an issue, classify it as `Risk` for the
/// flow-distribution mix. Matched case-insensitively on the trimmed label.
/// The list is small on purpose — operators can standardize on these four
/// without forking the tool; anything exotic falls through to Features.
const RISK_LABEL_TOKENS: &[&str] = &["risk", "security", "compliance", "safety"];

/// Labels that classify as `Debt`. `refactor` is intentionally in here —
/// it's the one category where the label and the issue_type both carry
/// meaningful signal.
const DEBT_LABEL_TOKENS: &[&str] = &["debt", "tech-debt", "techdebt", "refactor", "cleanup"];

/// Labels that upgrade an otherwise-Standard issue to Expedite. `critical`
/// mirrors the `--robot-alerts` severity vocabulary.
const EXPEDITE_LABEL_TOKENS: &[&str] = &["expedite", "critical", "hotfix", "p0"];

/// Labels that classify an issue as Intangible urgency (research, spikes).
/// These are deliberately not counted as Standard so the "how much Standard
/// work is shippable" number stays honest.
const INTANGIBLE_LABEL_TOKENS: &[&str] = &["intangible", "research", "spike", "explore"];

/// Fixed-Date window: due dates within this many days of `now` qualify
/// for Fixed-Date urgency. Due dates further out still count as Fixed-Date
/// (they have a committed date); the window just drives the
/// `milestone_pressure` surface below.
const FIXED_DATE_PRESSURE_WINDOW_DAYS: i64 = 14;

/// Flow category. Ordered by `classify_flow` precedence — not by rendering
/// order in the output (which is alphabetical-by-enum for stability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowCategory {
    Risk,
    Debt,
    Defects,
    Features,
}

impl FlowCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Risk => "risk",
            Self::Debt => "debt",
            Self::Defects => "defects",
            Self::Features => "features",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UrgencyCategory {
    Expedite,
    FixedDate,
    Intangible,
    Standard,
}

impl UrgencyCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expedite => "expedite",
            Self::FixedDate => "fixed_date",
            Self::Intangible => "intangible",
            Self::Standard => "standard",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowBucket {
    pub category: FlowCategory,
    pub count: usize,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UrgencyBucket {
    pub category: UrgencyCategory,
    pub count: usize,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneSignal {
    pub id: String,
    pub title: String,
    pub due_date: DateTime<Utc>,
    pub days_until_due: i64,
    pub is_overdue: bool,
    pub is_blocked: bool,
}

/// Flattens [`crate::robot::RobotEnvelope`] at top level, mirroring every
/// other `--robot-*` output. Adds `schema_version` as an explicit payload
/// field for downstream pinning (see GH#12).
#[derive(Debug, Clone, Serialize)]
pub struct RobotDeliveryOutput {
    #[serde(flatten)]
    pub envelope: crate::robot::RobotEnvelope,
    pub schema_version: &'static str,
    pub open_issues: usize,
    pub flow_distribution: Vec<FlowBucket>,
    pub urgency_profile: Vec<UrgencyBucket>,
    pub milestone_pressure: Vec<MilestoneSignal>,
    pub window_days: i64,
}

/// Inputs to `compute_delivery`. `blocked_ids` is passed in so this module
/// stays free of a direct dependency on the `Analyzer`/`IssueGraph` surface;
/// the caller supplies the set of issue IDs that have at least one open
/// blocker.
pub struct DeliveryComputation<'a> {
    pub issues: &'a [Issue],
    pub blocked_ids: &'a std::collections::HashSet<String>,
    pub now: DateTime<Utc>,
    /// Cap on the `milestone_pressure` list. Matches the existing
    /// `--insight-limit` default so callers can reuse the same ceiling.
    pub milestone_pressure_limit: usize,
}

pub fn compute_delivery(computation: DeliveryComputation<'_>) -> RobotDeliveryOutput {
    let DeliveryComputation {
        issues,
        blocked_ids,
        now,
        milestone_pressure_limit,
    } = computation;

    let open_issues: Vec<&Issue> = issues.iter().filter(|issue| issue.is_open_like()).collect();
    let open_count = open_issues.len();

    // Tally flow categories by first-match-wins precedence so percentages
    // sum to exactly 100 (modulo rounding).
    let mut flow_counts: [usize; 4] = [0; 4];
    let mut urgency_counts: [usize; 4] = [0; 4];
    for issue in &open_issues {
        flow_counts[flow_index(classify_flow(issue))] += 1;
        urgency_counts[urgency_index(classify_urgency(issue))] += 1;
    }

    let flow_distribution = [
        FlowCategory::Risk,
        FlowCategory::Debt,
        FlowCategory::Defects,
        FlowCategory::Features,
    ]
    .into_iter()
    .map(|category| FlowBucket {
        category,
        count: flow_counts[flow_index(category)],
        pct: pct(flow_counts[flow_index(category)], open_count),
    })
    .collect::<Vec<_>>();

    let urgency_profile = [
        UrgencyCategory::Expedite,
        UrgencyCategory::FixedDate,
        UrgencyCategory::Intangible,
        UrgencyCategory::Standard,
    ]
    .into_iter()
    .map(|category| UrgencyBucket {
        category,
        count: urgency_counts[urgency_index(category)],
        pct: pct(urgency_counts[urgency_index(category)], open_count),
    })
    .collect::<Vec<_>>();

    // Milestone pressure: open issues with a due_date. Anchor the ordering on
    // soonest-due (overdue items land first, positive days_until_due ascend
    // from there) with id as tiebreaker so repeated runs produce byte-stable
    // output for identical input.
    let mut milestone_pressure: Vec<MilestoneSignal> = open_issues
        .iter()
        .filter_map(|issue| {
            let due_date = issue.due_date?;
            let days_until_due = (due_date - now).num_days();
            Some(MilestoneSignal {
                id: issue.id.clone(),
                title: issue.title.clone(),
                due_date,
                days_until_due,
                is_overdue: due_date < now,
                is_blocked: blocked_ids.contains(&issue.id),
            })
        })
        .collect();
    milestone_pressure.sort_by(|left, right| {
        left.due_date
            .cmp(&right.due_date)
            .then_with(|| left.id.cmp(&right.id))
    });
    milestone_pressure.truncate(milestone_pressure_limit);

    RobotDeliveryOutput {
        envelope: crate::robot::envelope(issues),
        schema_version: DELIVERY_SCHEMA_VERSION,
        open_issues: open_count,
        flow_distribution,
        urgency_profile,
        milestone_pressure,
        window_days: FIXED_DATE_PRESSURE_WINDOW_DAYS,
    }
}

fn classify_flow(issue: &Issue) -> FlowCategory {
    // Risk first: security/risk labels outrank the issue_type because a
    // security-labelled bug is more usefully categorized as Risk than Defects.
    if labels_match_any(&issue.labels, RISK_LABEL_TOKENS)
        || matches_token(&issue.issue_type, "risk")
    {
        return FlowCategory::Risk;
    }
    // Debt next: covers both the tech-debt labels and the refactor/cleanup
    // issue_types that some teams use instead of labels.
    if labels_match_any(&issue.labels, DEBT_LABEL_TOKENS)
        || matches_any_token(&issue.issue_type, DEBT_LABEL_TOKENS)
    {
        return FlowCategory::Debt;
    }
    // Defects: issue_type == "bug" is the primary signal because beads spec
    // uses that type name; the bug/defect labels are a fallback for teams
    // that don't set issue_type.
    if matches_token(&issue.issue_type, "bug")
        || matches_token(&issue.issue_type, "defect")
        || labels_match_any(&issue.labels, &["bug", "defect"])
    {
        return FlowCategory::Defects;
    }
    FlowCategory::Features
}

fn classify_urgency(issue: &Issue) -> UrgencyCategory {
    // Expedite: P0 or explicit hotfix label. P0 is the strongest signal
    // beads has; hotfix/critical are the vocabulary --robot-alerts uses.
    if issue.priority == 0 || labels_match_any(&issue.labels, EXPEDITE_LABEL_TOKENS) {
        return UrgencyCategory::Expedite;
    }
    // Fixed-Date: any open issue with a due_date (future or past). Overdue
    // items still count as fixed-date — they have a commitment attached,
    // which is what distinguishes them from Standard. The milestone_pressure
    // surface below is where `now` becomes relevant (overdue flagging).
    if issue.due_date.is_some() {
        return UrgencyCategory::FixedDate;
    }
    if labels_match_any(&issue.labels, INTANGIBLE_LABEL_TOKENS) {
        return UrgencyCategory::Intangible;
    }
    UrgencyCategory::Standard
}

const fn flow_index(category: FlowCategory) -> usize {
    match category {
        FlowCategory::Risk => 0,
        FlowCategory::Debt => 1,
        FlowCategory::Defects => 2,
        FlowCategory::Features => 3,
    }
}

const fn urgency_index(category: UrgencyCategory) -> usize {
    match category {
        UrgencyCategory::Expedite => 0,
        UrgencyCategory::FixedDate => 1,
        UrgencyCategory::Intangible => 2,
        UrgencyCategory::Standard => 3,
    }
}

fn labels_match_any(labels: &[String], tokens: &[&str]) -> bool {
    labels
        .iter()
        .any(|label| matches_any_token(label.trim(), tokens))
}

fn matches_any_token(raw: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| matches_token(raw, token))
}

fn matches_token(raw: &str, token: &str) -> bool {
    raw.trim().eq_ignore_ascii_case(token)
}

fn pct(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (count as f64 / total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use std::collections::HashSet;

    fn open(id: &str, issue_type: &str, priority: i32, labels: &[&str]) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("title of {id}"),
            status: "open".to_string(),
            priority,
            issue_type: issue_type.to_string(),
            labels: labels.iter().map(|l| (*l).to_string()).collect(),
            ..Issue::default()
        }
    }

    fn now_fixture() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 20, 0, 0, 0).unwrap()
    }

    fn empty_blocked() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn flow_distribution_is_priority_ordered_each_issue_counted_once() {
        // A single issue tagged with BOTH security and tech-debt must count
        // as Risk only (priority order wins), so percentages sum to 100%.
        let issues = vec![
            open("A-1", "task", 1, &["security", "tech-debt"]),
            open("A-2", "bug", 1, &[]),
            open("A-3", "task", 1, &["refactor"]),
            open("A-4", "task", 1, &[]),
        ];
        let output = compute_delivery(DeliveryComputation {
            issues: &issues,
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let count_for = |cat: FlowCategory| -> usize {
            output
                .flow_distribution
                .iter()
                .find(|b| b.category == cat)
                .map(|b| b.count)
                .unwrap_or(0)
        };
        assert_eq!(count_for(FlowCategory::Risk), 1);
        assert_eq!(count_for(FlowCategory::Debt), 1);
        assert_eq!(count_for(FlowCategory::Defects), 1);
        assert_eq!(count_for(FlowCategory::Features), 1);
        let total_pct: f64 = output.flow_distribution.iter().map(|b| b.pct).sum();
        assert!((total_pct - 100.0).abs() < 1e-9, "got {total_pct}");
    }

    #[test]
    fn flow_distribution_sums_to_100_across_arbitrary_mixes() {
        // Stress: many issues with overlapping labels. Percentages must
        // still sum to 100 because first-match-wins ensures no double count.
        let labels = [
            vec!["security", "bug"],
            vec!["tech-debt"],
            vec!["refactor", "security"],
            vec!["feature"],
            vec!["bug", "refactor"],
            vec![],
            vec!["risk"],
            vec!["compliance"],
        ];
        let issues: Vec<Issue> = labels
            .iter()
            .enumerate()
            .map(|(i, ls)| open(&format!("X-{i}"), "task", 1, ls))
            .collect();
        let output = compute_delivery(DeliveryComputation {
            issues: &issues,
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let total_count: usize = output.flow_distribution.iter().map(|b| b.count).sum();
        assert_eq!(total_count, issues.len());
        let total_pct: f64 = output.flow_distribution.iter().map(|b| b.pct).sum();
        assert!((total_pct - 100.0).abs() < 1e-9, "got {total_pct}");
    }

    #[test]
    fn urgency_profile_expedite_beats_fixed_date() {
        // A P0 issue with a due_date must land in Expedite, not Fixed-Date
        // (priority ordering) — otherwise the "Expedite" cohort underrepresents
        // the work that actually needs immediate attention.
        let mut p0_with_due = open("A-1", "task", 0, &[]);
        p0_with_due.due_date = Some(now_fixture() + Duration::days(3));
        let issues = vec![p0_with_due];
        let output = compute_delivery(DeliveryComputation {
            issues: &issues,
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let get = |cat: UrgencyCategory| -> usize {
            output
                .urgency_profile
                .iter()
                .find(|b| b.category == cat)
                .map(|b| b.count)
                .unwrap_or(0)
        };
        assert_eq!(get(UrgencyCategory::Expedite), 1);
        assert_eq!(get(UrgencyCategory::FixedDate), 0);
    }

    #[test]
    fn urgency_profile_intangible_does_not_swallow_fixed_date() {
        // An issue with both a "research" label and a due date is Fixed-Date
        // (date commitment beats classification label).
        let mut issue = open("A-1", "task", 1, &["research"]);
        issue.due_date = Some(now_fixture() + Duration::days(10));
        let output = compute_delivery(DeliveryComputation {
            issues: &[issue],
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let get = |cat: UrgencyCategory| -> usize {
            output
                .urgency_profile
                .iter()
                .find(|b| b.category == cat)
                .map(|b| b.count)
                .unwrap_or(0)
        };
        assert_eq!(get(UrgencyCategory::FixedDate), 1);
        assert_eq!(get(UrgencyCategory::Intangible), 0);
    }

    #[test]
    fn urgency_profile_sums_to_100_when_open_issues_exist() {
        let mut p0 = open("A-1", "task", 0, &[]);
        p0.due_date = Some(now_fixture() + Duration::days(1));
        let mut due = open("A-2", "task", 1, &[]);
        due.due_date = Some(now_fixture() + Duration::days(20));
        let intangible = open("A-3", "task", 2, &["research"]);
        let standard_a = open("A-4", "task", 2, &[]);
        let standard_b = open("A-5", "feature", 2, &[]);
        let output = compute_delivery(DeliveryComputation {
            issues: &[p0, due, intangible, standard_a, standard_b],
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let total_pct: f64 = output.urgency_profile.iter().map(|b| b.pct).sum();
        assert!((total_pct - 100.0).abs() < 1e-9, "got {total_pct}");
    }

    #[test]
    fn closed_issues_are_excluded_from_every_bucket() {
        let mut closed = open("A-1", "bug", 0, &["security"]);
        closed.status = "closed".to_string();
        let output = compute_delivery(DeliveryComputation {
            issues: &[closed],
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        assert_eq!(output.open_issues, 0);
        assert!(output.flow_distribution.iter().all(|b| b.count == 0));
        assert!(output.urgency_profile.iter().all(|b| b.count == 0));
        assert!(output.milestone_pressure.is_empty());
    }

    #[test]
    fn milestone_pressure_sorted_by_due_date_then_id() {
        let now = now_fixture();
        let issue = |id: &str, days: i64| -> Issue {
            let mut i = open(id, "task", 1, &[]);
            i.due_date = Some(now + Duration::days(days));
            i
        };
        let output = compute_delivery(DeliveryComputation {
            issues: &[issue("Z-1", 10), issue("A-1", 5), issue("B-1", 5)],
            blocked_ids: &empty_blocked(),
            now,
            milestone_pressure_limit: 20,
        });
        let ids: Vec<&str> = output
            .milestone_pressure
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(ids, vec!["A-1", "B-1", "Z-1"]);
    }

    #[test]
    fn milestone_pressure_marks_overdue_and_blocked() {
        let now = now_fixture();
        let mut overdue = open("A-1", "task", 1, &[]);
        overdue.due_date = Some(now - Duration::days(3));
        let mut future_blocked = open("A-2", "task", 1, &[]);
        future_blocked.due_date = Some(now + Duration::days(7));

        let mut blocked_ids = HashSet::new();
        blocked_ids.insert("A-2".to_string());

        let output = compute_delivery(DeliveryComputation {
            issues: &[overdue, future_blocked],
            blocked_ids: &blocked_ids,
            now,
            milestone_pressure_limit: 20,
        });
        assert!(output.milestone_pressure[0].is_overdue);
        assert!(!output.milestone_pressure[0].is_blocked);
        assert!(!output.milestone_pressure[1].is_overdue);
        assert!(output.milestone_pressure[1].is_blocked);
        assert_eq!(output.milestone_pressure[0].days_until_due, -3);
        assert_eq!(output.milestone_pressure[1].days_until_due, 7);
    }

    #[test]
    fn milestone_pressure_respects_limit() {
        let now = now_fixture();
        let issues: Vec<Issue> = (0..10)
            .map(|i| {
                let mut issue = open(&format!("A-{i}"), "task", 1, &[]);
                issue.due_date = Some(now + Duration::days(i));
                issue
            })
            .collect();
        let output = compute_delivery(DeliveryComputation {
            issues: &issues,
            blocked_ids: &empty_blocked(),
            now,
            milestone_pressure_limit: 3,
        });
        assert_eq!(output.milestone_pressure.len(), 3);
    }

    #[test]
    fn label_matching_is_case_insensitive_and_trim_safe() {
        // Freeze the label token contract so classification does not quietly
        // start missing work labelled "SECURITY " or "Tech-Debt".
        let issues = vec![
            open("A-1", "task", 1, &["  SECURITY  "]),
            open("A-2", "task", 1, &["Tech-Debt"]),
        ];
        let output = compute_delivery(DeliveryComputation {
            issues: &issues,
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        let get = |cat: FlowCategory| -> usize {
            output
                .flow_distribution
                .iter()
                .find(|b| b.category == cat)
                .map(|b| b.count)
                .unwrap_or(0)
        };
        assert_eq!(get(FlowCategory::Risk), 1);
        assert_eq!(get(FlowCategory::Debt), 1);
    }

    #[test]
    fn zero_open_issues_yields_zero_counts_without_panics() {
        let output = compute_delivery(DeliveryComputation {
            issues: &[],
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        assert_eq!(output.open_issues, 0);
        assert!(output.flow_distribution.iter().all(|b| b.pct == 0.0));
        assert!(output.urgency_profile.iter().all(|b| b.pct == 0.0));
    }

    #[test]
    fn schema_version_is_pinned_to_v1() {
        // Bumping this value is a schema contract change. Any renamed or
        // removed field must bump the schema. Adding new optional fields
        // does not.
        let output = compute_delivery(DeliveryComputation {
            issues: &[],
            blocked_ids: &empty_blocked(),
            now: now_fixture(),
            milestone_pressure_limit: 20,
        });
        assert_eq!(output.schema_version, "1");
    }
}
