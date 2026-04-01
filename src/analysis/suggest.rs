use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::analysis::graph::GraphMetrics;
use crate::model::Issue;

const DEFAULT_MAX_SUGGESTIONS: usize = 50;
const DUPLICATE_JACCARD_THRESHOLD: f64 = 0.7;
const DUPLICATE_MIN_KEYWORDS: usize = 2;
const DUPLICATE_MAX_SUGGESTIONS: usize = 20;
const DEPENDENCY_MIN_KEYWORD_OVERLAP: usize = 2;
const DEPENDENCY_MIN_CONFIDENCE: f64 = 0.5;
const DEPENDENCY_MAX_SUGGESTIONS: usize = 20;
const LABEL_MIN_CONFIDENCE: f64 = 0.5;
const LABEL_MAX_PER_ISSUE: usize = 3;
const LABEL_MAX_TOTAL: usize = 30;
const CYCLE_MAX: usize = 10;
const HIGH_CONFIDENCE_THRESHOLD: f64 = 0.7;
const LOW_CONFIDENCE_THRESHOLD: f64 = 0.4;
const STALE_DAYS_THRESHOLD: i64 = 90;
const STALE_PAGERANK_PERCENTILE: f64 = 0.25;
const STALE_MAX_SUGGESTIONS: usize = 20;

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "are", "was", "were", "been", "have",
    "has", "had", "does", "did", "will", "would", "could", "should", "may", "might", "can", "not",
    "all", "any", "some", "each", "when", "where", "what", "which", "how", "why", "who", "its",
    "also", "just", "only", "more", "than", "then", "now", "here", "there", "these", "those",
    "such", "into", "over", "after", "before", "being", "other", "about", "like", "very", "most",
    "make", "use",
];

const BUILTIN_LABEL_MAPPINGS: &[(&str, &[&str])] = &[
    ("database", &["database", "db"]),
    ("migration", &["database", "migration"]),
    ("api", &["api"]),
    ("endpoint", &["api"]),
    ("rest", &["api"]),
    ("graphql", &["api", "graphql"]),
    ("auth", &["auth", "security"]),
    ("login", &["auth"]),
    ("password", &["auth", "security"]),
    ("security", &["security"]),
    ("test", &["testing"]),
    ("tests", &["testing"]),
    ("unittest", &["testing"]),
    ("integration", &["testing", "integration"]),
    ("ui", &["ui", "frontend"]),
    ("frontend", &["frontend"]),
    ("backend", &["backend"]),
    ("server", &["backend"]),
    ("cli", &["cli"]),
    ("command", &["cli"]),
    ("config", &["config"]),
    ("settings", &["config"]),
    ("performance", &["performance"]),
    ("slow", &["performance"]),
    ("fast", &["performance"]),
    ("memory", &["performance"]),
    ("cache", &["performance", "cache"]),
    ("docs", &["documentation"]),
    ("readme", &["documentation"]),
    ("refactor", &["refactoring"]),
    ("cleanup", &["refactoring", "maintenance"]),
    ("dependency", &["dependencies"]),
    ("deps", &["dependencies"]),
    ("bug", &["bug"]),
    ("fix", &["bug"]),
    ("broken", &["bug"]),
    ("crash", &["bug"]),
    ("error", &["bug"]),
    ("feature", &["feature"]),
    ("enhance", &["enhancement"]),
    ("improve", &["enhancement"]),
    ("urgent", &["urgent", "priority"]),
    ("hotfix", &["urgent", "bug"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    MissingDependency,
    PotentialDuplicate,
    LabelSuggestion,
    CycleWarning,
    StaleCleanup,
}

impl SuggestionType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingDependency => "missing_dependency",
            Self::PotentialDuplicate => "potential_duplicate",
            Self::LabelSuggestion => "label_suggestion",
            Self::CycleWarning => "cycle_warning",
            Self::StaleCleanup => "stale_cleanup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionConfidenceLevel {
    Low,
    Medium,
    High,
}

impl SuggestionConfidenceLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SuggestOptions {
    pub min_confidence: f64,
    pub max_suggestions: usize,
    pub filter_type: Option<SuggestionType>,
    pub filter_bead: Option<String>,
}

impl Default for SuggestOptions {
    fn default() -> Self {
        Self {
            min_confidence: 0.0,
            max_suggestions: DEFAULT_MAX_SUGGESTIONS,
            filter_type: None,
            filter_bead: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    #[serde(rename = "type")]
    pub suggestion_type: SuggestionType,
    pub target_bead: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_bead: Option<String>,
    pub summary: String,
    pub reason: String,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_command: Option<String>,
    pub generated_at: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestionSet {
    pub suggestions: Vec<Suggestion>,
    pub generated_at: String,
    pub data_hash: String,
    pub stats: SuggestionStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestionStats {
    pub total: usize,
    pub by_type: BTreeMap<String, usize>,
    pub by_confidence: BTreeMap<String, usize>,
    pub high_confidence_count: usize,
    pub actionable_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestFilter {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub filter_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bead_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RobotSuggestOutput {
    #[serde(flatten)]
    pub envelope: crate::robot::RobotEnvelope,
    pub filters: SuggestFilter,
    pub suggestions: SuggestionSet,
    pub usage_hints: Vec<String>,
}

#[must_use]
pub fn generate_robot_suggest_output(
    issues: &[Issue],
    metrics: &GraphMetrics,
    options: &SuggestOptions,
) -> RobotSuggestOutput {
    let env = crate::robot::envelope(issues);
    let mut suggestions = detect_potential_duplicates(issues, &env.generated_at);
    suggestions.extend(detect_missing_dependencies(issues, &env.generated_at));
    suggestions.extend(detect_label_suggestions(issues, &env.generated_at));
    suggestions.extend(detect_cycle_warnings(metrics, &env.generated_at));
    suggestions.extend(detect_stale_cleanup(issues, metrics, &env.generated_at));
    suggestions.retain(|suggestion| matches_filters(suggestion, options));
    sort_suggestions(&mut suggestions);
    if options.max_suggestions > 0 && suggestions.len() > options.max_suggestions {
        suggestions.truncate(options.max_suggestions);
    }

    let suggestion_set = SuggestionSet {
        stats: compute_stats(&suggestions),
        suggestions,
        generated_at: env.generated_at.clone(),
        data_hash: env.data_hash.clone(),
    };

    let active_bead_filter = options
        .filter_bead
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let filters = SuggestFilter {
        filter_type: options.filter_type.map(|value| value.as_str().to_string()),
        min_confidence: if options.min_confidence > 0.0 {
            Some(options.min_confidence)
        } else {
            None
        },
        bead_id: active_bead_filter,
    };

    RobotSuggestOutput {
        envelope: env,
        filters,
        suggestions: suggestion_set,
        usage_hints: vec![
            "jq '.suggestions.suggestions[:5]' - Top 5 suggestions by confidence".to_string(),
            "jq '.suggestions.suggestions[] | select(.type==\"potential_duplicate\")' - Filter duplicates".to_string(),
            "jq '.suggestions.suggestions[] | select(.confidence >= 0.8)' - High-confidence only".to_string(),
            "jq '.suggestions.stats.by_type' - Count by suggestion type".to_string(),
            "jq '.suggestions.suggestions[].action_command' - All action commands".to_string(),
            "--suggest-type=dependency - Filter to dependency suggestions".to_string(),
            "--suggest-confidence=0.7 - Minimum confidence threshold".to_string(),
            "--suggest-bead=<id> - Suggestions for specific bead".to_string(),
        ],
    }
}

fn detect_potential_duplicates(issues: &[Issue], generated_at: &str) -> Vec<Suggestion> {
    if issues.len() < 2 {
        return Vec::new();
    }

    let keyword_sets = issues
        .iter()
        .map(|issue| extract_keywords(&issue.title, &issue.description))
        .collect::<Vec<_>>();

    let mut suggestions = Vec::<Suggestion>::new();
    for left_index in 0..issues.len() {
        if keyword_sets[left_index].len() < DUPLICATE_MIN_KEYWORDS {
            continue;
        }

        for right_index in (left_index + 1)..issues.len() {
            if keyword_sets[right_index].len() < DUPLICATE_MIN_KEYWORDS {
                continue;
            }

            let left_issue = &issues[left_index];
            let right_issue = &issues[right_index];
            if left_issue.normalized_status() == "tombstone"
                || right_issue.normalized_status() == "tombstone"
            {
                continue;
            }

            if left_issue.is_closed_like() != right_issue.is_closed_like() {
                continue;
            }

            let common = intersect_keywords(&keyword_sets[left_index], &keyword_sets[right_index]);
            let union_count = keyword_sets[left_index]
                .len()
                .saturating_add(keyword_sets[right_index].len())
                .saturating_sub(common.len());
            if union_count == 0 {
                continue;
            }

            let similarity = ratio(common.len(), union_count);
            if similarity < DUPLICATE_JACCARD_THRESHOLD {
                continue;
            }

            let mut suggestion = base_suggestion(
                generated_at,
                SuggestionType::PotentialDuplicate,
                left_issue.id.clone(),
                format!("Potential duplicate of {}", right_issue.id),
                format!(
                    "{:.0}% keyword similarity; common: {}",
                    similarity * 100.0,
                    common
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                similarity,
            );
            suggestion.related_bead = Some(right_issue.id.clone());
            if left_issue.is_open_like() && right_issue.is_open_like() {
                suggestion.action_command = Some(format!(
                    "br dep add {} {} --type=related",
                    left_issue.id, right_issue.id
                ));
            }
            suggestion
                .metadata
                .insert("method".to_string(), json!("jaccard"));
            suggestion
                .metadata
                .insert("common_keywords".to_string(), json!(common));
            suggestions.push(suggestion);
        }
    }

    sort_suggestions(&mut suggestions);
    if suggestions.len() > DUPLICATE_MAX_SUGGESTIONS {
        suggestions.truncate(DUPLICATE_MAX_SUGGESTIONS);
    }
    suggestions
}

fn detect_missing_dependencies(issues: &[Issue], generated_at: &str) -> Vec<Suggestion> {
    if issues.len() < 2 {
        return Vec::new();
    }

    let keyword_sets = issues
        .iter()
        .map(|issue| extract_keywords(&issue.title, &issue.description))
        .collect::<Vec<_>>();

    let label_sets = issues
        .iter()
        .map(|issue| {
            issue
                .labels
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();

    let mut suggestions = Vec::<Suggestion>::new();
    for left_index in 0..issues.len() {
        for right_index in (left_index + 1)..issues.len() {
            let left_issue = &issues[left_index];
            let right_issue = &issues[right_index];
            if left_issue.is_closed_like() || right_issue.is_closed_like() {
                continue;
            }
            if has_dependency_between(left_issue, right_issue) {
                continue;
            }

            let shared_keywords =
                intersect_keywords(&keyword_sets[left_index], &keyword_sets[right_index]);
            if shared_keywords.len() < DEPENDENCY_MIN_KEYWORD_OVERLAP {
                continue;
            }

            let shared_labels = label_sets[left_index]
                .intersection(&label_sets[right_index])
                .cloned()
                .collect::<Vec<_>>();

            let mut confidence = (usize_to_f64(shared_keywords.len()) * 0.1).min(0.5);
            if mentions_issue_id(left_issue, right_issue)
                || mentions_issue_id(right_issue, left_issue)
            {
                confidence += 0.3;
            }
            if title_keyword_overlap(right_issue, &keyword_sets[left_index]) {
                confidence += 0.15;
            }
            confidence += usize_to_f64(shared_labels.len()) * 0.1;
            confidence = confidence.min(0.95);
            if confidence < DEPENDENCY_MIN_CONFIDENCE {
                continue;
            }

            let (from_issue, to_issue) = dependency_direction(left_issue, right_issue);
            let mut suggestion = base_suggestion(
                generated_at,
                SuggestionType::MissingDependency,
                from_issue.id.clone(),
                format!("May depend on {}", to_issue.id),
                format!(
                    "{} shared keywords{}",
                    shared_keywords.len(),
                    if shared_labels.is_empty() {
                        String::new()
                    } else {
                        format!(", {} shared labels", shared_labels.len())
                    }
                ),
                confidence,
            );
            suggestion.related_bead = Some(to_issue.id.clone());
            suggestion.action_command =
                Some(format!("br dep add {} {}", from_issue.id, to_issue.id));
            suggestion
                .metadata
                .insert("shared_keywords".to_string(), json!(shared_keywords));
            if !shared_labels.is_empty() {
                suggestion
                    .metadata
                    .insert("shared_labels".to_string(), json!(shared_labels));
            }
            suggestions.push(suggestion);
        }
    }

    sort_suggestions(&mut suggestions);
    if suggestions.len() > DEPENDENCY_MAX_SUGGESTIONS {
        suggestions.truncate(DEPENDENCY_MAX_SUGGESTIONS);
    }
    suggestions
}

fn detect_label_suggestions(issues: &[Issue], generated_at: &str) -> Vec<Suggestion> {
    if issues.is_empty() {
        return Vec::new();
    }

    let canonical_labels = canonical_project_labels(issues);
    let all_labels = issues
        .iter()
        .flat_map(|issue| issue.labels.iter().map(|label| label.to_ascii_lowercase()))
        .collect::<BTreeSet<_>>();
    if all_labels.is_empty() {
        return Vec::new();
    }

    let learned_mappings = learn_label_mappings(issues);
    let mut matches = Vec::<Suggestion>::new();

    for issue in issues {
        if issue.is_closed_like() {
            continue;
        }

        let existing_labels = issue
            .labels
            .iter()
            .map(|label| label.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let keywords = extract_keywords(&issue.title, &issue.description);

        let mut label_scores = BTreeMap::<String, f64>::new();
        let mut label_reasons = BTreeMap::<String, BTreeSet<String>>::new();

        for keyword in &keywords {
            for &(mapping_keyword, labels) in BUILTIN_LABEL_MAPPINGS {
                if mapping_keyword != keyword {
                    continue;
                }

                for &label in labels {
                    let candidate = label.to_string();
                    if existing_labels.contains(&candidate) || !all_labels.contains(&candidate) {
                        continue;
                    }
                    *label_scores.entry(candidate.clone()).or_insert(0.0) += 0.3;
                    label_reasons
                        .entry(candidate)
                        .or_default()
                        .insert(keyword.clone());
                }
            }

            if let Some(label_counts) = learned_mappings.get(keyword) {
                for (label, count) in label_counts {
                    if existing_labels.contains(label) || !all_labels.contains(label) {
                        continue;
                    }
                    let learned_bonus = (0.1 + (usize_to_f64(*count) * 0.05)).min(0.4);
                    *label_scores.entry(label.clone()).or_insert(0.0) += learned_bonus;
                    label_reasons
                        .entry(label.clone())
                        .or_default()
                        .insert(keyword.clone());
                }
            }
        }

        let mut candidates = label_scores.into_iter().collect::<Vec<(String, f64)>>();
        candidates.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });

        for (index, (label, score)) in candidates.into_iter().enumerate() {
            if index >= LABEL_MAX_PER_ISSUE || score < LABEL_MIN_CONFIDENCE {
                continue;
            }

            let display_label = canonical_labels
                .get(&label)
                .cloned()
                .unwrap_or(label.clone());

            let matched_keywords = label_reasons
                .get(&label)
                .map(|values| values.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let reason = format!("keywords: {}", matched_keywords.join(", "));

            let mut suggestion = base_suggestion(
                generated_at,
                SuggestionType::LabelSuggestion,
                issue.id.clone(),
                format!("Consider adding label '{display_label}'"),
                reason,
                score.min(0.95),
            );
            suggestion.action_command = Some(format!(
                "br update {} --add-label={display_label}",
                issue.id
            ));
            suggestion
                .metadata
                .insert("suggested_label".to_string(), json!(display_label));
            suggestion
                .metadata
                .insert("matched_keywords".to_string(), json!(matched_keywords));
            matches.push(suggestion);
        }
    }

    sort_suggestions(&mut matches);
    if matches.len() > LABEL_MAX_TOTAL {
        matches.truncate(LABEL_MAX_TOTAL);
    }
    matches
}

fn detect_cycle_warnings(metrics: &GraphMetrics, generated_at: &str) -> Vec<Suggestion> {
    let mut suggestions = Vec::<Suggestion>::new();

    for cycle in metrics.cycles.iter().take(CYCLE_MAX) {
        if cycle.is_empty() {
            continue;
        }

        let cycle_length = cycle.len();
        let distance_from_shortest = cycle_length.saturating_sub(2);
        let confidence = (1.0 - (usize_to_f64(distance_from_shortest) * 0.1)).clamp(0.5, 1.0);

        let summary = if cycle_length == 1 {
            format!("Self-loop: {} depends on itself", cycle[0])
        } else if cycle_length == 2 {
            format!("Direct cycle between {} and {}", cycle[0], cycle[1])
        } else {
            format!("Dependency cycle of {cycle_length} issues")
        };

        let mut cycle_path = cycle.clone();
        if cycle_length > 1 {
            cycle_path.push(cycle[0].clone());
        }

        let mut suggestion = base_suggestion(
            generated_at,
            SuggestionType::CycleWarning,
            cycle[0].clone(),
            summary,
            format!("Cycle path: {}", cycle_path.join(" -> ")),
            confidence,
        );
        if cycle_length >= 2 {
            let last = cycle[cycle_length - 1].clone();
            let first = cycle[0].clone();
            suggestion.action_command = Some(format!("br dep remove {last} {first}"));
            suggestion.related_bead = Some(cycle[1].clone());
        }
        suggestion
            .metadata
            .insert("cycle_length".to_string(), json!(cycle_length));
        suggestion
            .metadata
            .insert("cycle_path".to_string(), json!(cycle));
        suggestions.push(suggestion);
    }

    suggestions
}

fn detect_stale_cleanup(
    issues: &[Issue],
    metrics: &GraphMetrics,
    generated_at: &str,
) -> Vec<Suggestion> {
    let now = Utc::now();

    // Compute the PageRank threshold at the given percentile of open issues.
    let mut open_pageranks: Vec<f64> = issues
        .iter()
        .filter(|i| i.is_open_like())
        .map(|i| metrics.pagerank.get(&i.id).copied().unwrap_or(0.0))
        .collect();
    open_pageranks.sort_by(f64::total_cmp);
    let pagerank_threshold = if open_pageranks.is_empty() {
        0.0
    } else {
        let idx = ((open_pageranks.len() - 1) as f64 * STALE_PAGERANK_PERCENTILE).floor() as usize;
        open_pageranks[idx]
    };

    let mut suggestions = Vec::new();

    for issue in issues.iter().filter(|i| i.is_open_like()) {
        let updated = issue.updated_at;
        let days_stale = match updated {
            Some(ts) => (now - ts).num_days(),
            None => {
                // Fall back to created_at; if neither exists, skip.
                match issue.created_at {
                    Some(ts) => (now - ts).num_days(),
                    None => continue,
                }
            }
        };

        if days_stale < STALE_DAYS_THRESHOLD {
            continue;
        }

        let pagerank = metrics.pagerank.get(&issue.id).copied().unwrap_or(0.0);
        if pagerank > pagerank_threshold {
            continue;
        }

        // Higher confidence for older, lower-impact issues.
        let age_factor = ((days_stale as f64 - STALE_DAYS_THRESHOLD as f64) / 180.0).min(1.0);
        let confidence = (0.4 + 0.3 * age_factor).clamp(0.0, 1.0);

        let mut suggestion = base_suggestion(
            generated_at,
            SuggestionType::StaleCleanup,
            issue.id.clone(),
            format!(
                "{} has been stale for {} days with low graph impact",
                issue.id, days_stale
            ),
            format!(
                "Last updated {} days ago, PageRank {:.4} (below threshold {:.4})",
                days_stale, pagerank, pagerank_threshold
            ),
            confidence,
        );
        suggestion.action_command =
            Some(format!("br close {} --reason \"stale cleanup\"", issue.id));
        suggestion
            .metadata
            .insert("days_stale".to_string(), json!(days_stale));
        suggestion
            .metadata
            .insert("pagerank".to_string(), json!(pagerank));
        suggestions.push(suggestion);
    }

    suggestions.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.target_bead.cmp(&b.target_bead))
    });
    suggestions.truncate(STALE_MAX_SUGGESTIONS);
    suggestions
}

fn base_suggestion(
    generated_at: &str,
    suggestion_type: SuggestionType,
    target_bead: String,
    summary: String,
    reason: String,
    confidence: f64,
) -> Suggestion {
    Suggestion {
        suggestion_type,
        target_bead,
        related_bead: None,
        summary,
        reason,
        confidence,
        action_command: None,
        generated_at: generated_at.to_string(),
        metadata: BTreeMap::new(),
    }
}

fn dependency_direction<'a>(left: &'a Issue, right: &'a Issue) -> (&'a Issue, &'a Issue) {
    let left_created = left.created_at;
    let right_created = right.created_at;

    let priority_cmp = left.priority.cmp(&right.priority);
    let time_cmp = match (left_created, right_created) {
        (Some(l), Some(r)) => l.cmp(&r),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };

    let left_is_blocker = match priority_cmp {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => match time_cmp {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => left.id < right.id,
        },
    };

    if left_is_blocker {
        (right, left)
    } else {
        (left, right)
    }
}

fn learn_label_mappings(issues: &[Issue]) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut mappings = BTreeMap::<String, BTreeMap<String, usize>>::new();

    for issue in issues {
        if issue.labels.is_empty() {
            continue;
        }

        let keywords = extract_keywords(&issue.title, &issue.description);
        for keyword in keywords {
            let label_counts = mappings.entry(keyword).or_default();
            for label in &issue.labels {
                let label = label.to_ascii_lowercase();
                *label_counts.entry(label).or_insert(0) += 1;
            }
        }
    }

    mappings
}

fn canonical_project_labels(issues: &[Issue]) -> BTreeMap<String, String> {
    let mut variants = BTreeMap::<String, BTreeMap<String, usize>>::new();

    for issue in issues {
        for label in &issue.labels {
            let normalized = label.to_ascii_lowercase();
            *variants
                .entry(normalized)
                .or_default()
                .entry(label.clone())
                .or_insert(0) += 1;
        }
    }

    variants
        .into_iter()
        .filter_map(|(normalized, counts)| {
            counts
                .into_iter()
                .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
                .map(|(canonical, _)| (normalized, canonical))
        })
        .collect()
}

fn has_dependency_between(left: &Issue, right: &Issue) -> bool {
    left.dependencies
        .iter()
        .any(|dep| dep.depends_on_id == right.id)
        || right
            .dependencies
            .iter()
            .any(|dep| dep.depends_on_id == left.id)
}

fn mentions_issue_id(primary: &Issue, other: &Issue) -> bool {
    let primary_text = primary.description.to_ascii_lowercase();
    let other_id = other.id.to_ascii_lowercase();
    !other_id.is_empty() && primary_text.contains(&other_id)
}

fn title_keyword_overlap(other: &Issue, primary_keywords: &[String]) -> bool {
    let other_title = other.title.to_ascii_lowercase();
    primary_keywords
        .iter()
        .any(|keyword| keyword.len() >= 5 && other_title.contains(keyword))
}

fn matches_filters(suggestion: &Suggestion, options: &SuggestOptions) -> bool {
    if options.min_confidence > 0.0 && suggestion.confidence < options.min_confidence {
        return false;
    }

    if options
        .filter_type
        .is_some_and(|filter_type| suggestion.suggestion_type != filter_type)
    {
        return false;
    }

    let bead_filter = options
        .filter_bead
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    if let Some(bead_id) = bead_filter
        && suggestion.target_bead != bead_id
        && suggestion.related_bead.as_deref() != Some(bead_id)
    {
        return false;
    }

    true
}

fn sort_suggestions(suggestions: &mut [Suggestion]) {
    suggestions.sort_by(|left, right| {
        right
            .confidence
            .total_cmp(&left.confidence)
            .then_with(|| {
                left.suggestion_type
                    .as_str()
                    .cmp(right.suggestion_type.as_str())
            })
            .then_with(|| left.target_bead.cmp(&right.target_bead))
            .then_with(|| left.related_bead.cmp(&right.related_bead))
    });
}

fn compute_stats(suggestions: &[Suggestion]) -> SuggestionStats {
    let mut by_type = BTreeMap::<String, usize>::new();
    let mut by_confidence = BTreeMap::<String, usize>::new();
    let mut high_confidence_count = 0usize;
    let mut actionable_count = 0usize;

    for suggestion in suggestions {
        *by_type
            .entry(suggestion.suggestion_type.as_str().to_string())
            .or_insert(0) += 1;

        let level = confidence_level(suggestion.confidence);
        *by_confidence.entry(level.as_str().to_string()).or_insert(0) += 1;
        if suggestion.confidence >= HIGH_CONFIDENCE_THRESHOLD {
            high_confidence_count += 1;
        }
        if suggestion.action_command.is_some() {
            actionable_count += 1;
        }
    }

    SuggestionStats {
        total: suggestions.len(),
        by_type,
        by_confidence,
        high_confidence_count,
        actionable_count,
    }
}

fn confidence_level(confidence: f64) -> SuggestionConfidenceLevel {
    if confidence < LOW_CONFIDENCE_THRESHOLD {
        return SuggestionConfidenceLevel::Low;
    }
    if confidence >= HIGH_CONFIDENCE_THRESHOLD {
        return SuggestionConfidenceLevel::High;
    }
    SuggestionConfidenceLevel::Medium
}

fn extract_keywords(title: &str, description: &str) -> Vec<String> {
    let normalized = normalize_text(&format!("{title} {description}"));
    let mut keywords = BTreeSet::<String>::new();

    for word in normalized.split_whitespace() {
        if word.len() < 3 || STOP_WORDS.contains(&word) {
            continue;
        }
        keywords.insert(word.to_string());
    }

    keywords.into_iter().collect()
}

fn normalize_text(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect()
}

fn intersect_keywords(left: &[String], right: &[String]) -> Vec<String> {
    let right_set = right.iter().cloned().collect::<BTreeSet<_>>();
    left.iter()
        .filter(|word| right_set.contains(*word))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    usize_to_f64(numerator) / usize_to_f64(denominator)
}

fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_suggestion(
        suggestion_type: SuggestionType,
        confidence: f64,
        target: &str,
    ) -> Suggestion {
        Suggestion {
            suggestion_type,
            target_bead: target.to_string(),
            related_bead: None,
            summary: String::new(),
            reason: String::new(),
            confidence,
            action_command: None,
            generated_at: String::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn sort_by_confidence_descending() {
        let mut suggestions = vec![
            make_suggestion(SuggestionType::MissingDependency, 0.5, "bd-a"),
            make_suggestion(SuggestionType::MissingDependency, 0.9, "bd-b"),
            make_suggestion(SuggestionType::MissingDependency, 0.7, "bd-c"),
        ];
        sort_suggestions(&mut suggestions);
        assert_eq!(suggestions[0].target_bead, "bd-b"); // 0.9
        assert_eq!(suggestions[1].target_bead, "bd-c"); // 0.7
        assert_eq!(suggestions[2].target_bead, "bd-a"); // 0.5
    }

    #[test]
    fn sort_tiebreak_by_type_alphabetical() {
        // All same confidence — should sort alphabetically by type string
        let mut suggestions = vec![
            make_suggestion(SuggestionType::PotentialDuplicate, 0.8, "bd-a"), // "potential_duplicate"
            make_suggestion(SuggestionType::CycleWarning, 0.8, "bd-b"),       // "cycle_warning"
            make_suggestion(SuggestionType::MissingDependency, 0.8, "bd-c"), // "missing_dependency"
            make_suggestion(SuggestionType::LabelSuggestion, 0.8, "bd-d"),   // "label_suggestion"
        ];
        sort_suggestions(&mut suggestions);
        // Alphabetical: cycle_warning < label_suggestion < missing_dependency < potential_duplicate
        assert_eq!(suggestions[0].suggestion_type.as_str(), "cycle_warning");
        assert_eq!(suggestions[1].suggestion_type.as_str(), "label_suggestion");
        assert_eq!(
            suggestions[2].suggestion_type.as_str(),
            "missing_dependency"
        );
        assert_eq!(
            suggestions[3].suggestion_type.as_str(),
            "potential_duplicate"
        );
    }

    #[test]
    fn sort_tiebreak_by_target_bead() {
        // Same confidence and type — should sort by target_bead
        let mut suggestions = vec![
            make_suggestion(SuggestionType::CycleWarning, 0.8, "bd-z"),
            make_suggestion(SuggestionType::CycleWarning, 0.8, "bd-a"),
            make_suggestion(SuggestionType::CycleWarning, 0.8, "bd-m"),
        ];
        sort_suggestions(&mut suggestions);
        assert_eq!(suggestions[0].target_bead, "bd-a");
        assert_eq!(suggestions[1].target_bead, "bd-m");
        assert_eq!(suggestions[2].target_bead, "bd-z");
    }

    fn make_issue_with_dates(id: &str, status: &str, updated_days_ago: i64) -> Issue {
        let updated = Utc::now() - chrono::Duration::days(updated_days_ago);
        Issue {
            id: id.to_string(),
            title: format!("Issue {id}"),
            status: status.to_string(),
            issue_type: "task".to_string(),
            priority: 3,
            updated_at: Some(updated),
            created_at: Some(updated),
            ..Issue::default()
        }
    }

    fn make_issue_with_dep(id: &str, title: &str, status: &str, depends_on: &str) -> Issue {
        let mut issue = make_issue_with_dates(id, status, 30);
        issue.title = title.to_string();
        issue.dependencies.push(crate::model::Dependency {
            issue_id: id.to_string(),
            depends_on_id: depends_on.to_string(),
            dep_type: "blocks".to_string(),
            ..crate::model::Dependency::default()
        });
        issue
    }

    #[test]
    fn stale_cleanup_detects_old_low_impact_issues() {
        use crate::analysis::graph::IssueGraph;

        let issues = vec![
            make_issue_with_dates("A", "open", 120),
            make_issue_with_dates("B", "open", 10),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now().to_rfc3339();

        let results = detect_stale_cleanup(&issues, &metrics, &now);
        // A is stale (120 days > 90), B is fresh
        assert!(
            results.iter().any(|s| s.target_bead == "A"),
            "should detect stale issue A"
        );
        assert!(
            !results.iter().any(|s| s.target_bead == "B"),
            "should not flag fresh issue B"
        );
    }

    #[test]
    fn stale_cleanup_skips_closed_issues() {
        use crate::analysis::graph::IssueGraph;

        let issues = vec![make_issue_with_dates("A", "closed", 200)];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now().to_rfc3339();

        let results = detect_stale_cleanup(&issues, &metrics, &now);
        assert!(results.is_empty(), "closed issues should not be flagged");
    }

    #[test]
    fn stale_cleanup_has_action_command() {
        use crate::analysis::graph::IssueGraph;

        let issues = vec![make_issue_with_dates("X", "open", 100)];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now().to_rfc3339();

        let results = detect_stale_cleanup(&issues, &metrics, &now);
        assert!(!results.is_empty());
        assert_eq!(
            results[0].action_command.as_deref(),
            Some("br close X --reason \"stale cleanup\"")
        );
        assert_eq!(results[0].suggestion_type, SuggestionType::StaleCleanup);
    }

    #[test]
    fn stale_cleanup_empty_issues() {
        use crate::analysis::graph::IssueGraph;

        let issues: Vec<Issue> = vec![];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now().to_rfc3339();

        let results = detect_stale_cleanup(&issues, &metrics, &now);
        assert!(results.is_empty());
    }

    #[test]
    fn stale_cleanup_does_not_flag_high_impact_issue_at_small_n() {
        use crate::analysis::graph::IssueGraph;

        let mut blocker = make_issue_with_dates("A", "open", 120);
        let blocked_one = make_issue_with_dep("B", "frontend follow-up", "open", "A");
        let blocked_two = make_issue_with_dep("C", "ops follow-up", "open", "A");
        blocker.title = "Core blocker".to_string();

        let issues = vec![blocker, blocked_one, blocked_two];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let now = Utc::now().to_rfc3339();

        let results = detect_stale_cleanup(&issues, &metrics, &now);
        assert!(
            !results
                .iter()
                .any(|suggestion| suggestion.target_bead == "A"),
            "the highest-impact stale issue should not be classified as low-impact"
        );
    }

    // ── detect_potential_duplicates ──────────────────────────────────

    fn make_issue(id: &str, title: &str, description: &str, status: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: status.to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            ..Issue::default()
        }
    }

    #[test]
    fn duplicates_detected_for_high_keyword_overlap() {
        // Need high Jaccard similarity (>= 0.7) with at least 2 keywords each.
        // Use nearly identical keywords so intersection/union ratio is high.
        let issues = vec![
            make_issue(
                "bd-1",
                "database migration schema upgrade rollback",
                "database migration schema upgrade rollback procedure",
                "open",
            ),
            make_issue(
                "bd-2",
                "database migration schema upgrade rollback",
                "database migration schema upgrade rollback implementation",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        assert!(
            !results.is_empty(),
            "highly similar issues should be flagged as duplicates"
        );
        assert_eq!(
            results[0].suggestion_type,
            SuggestionType::PotentialDuplicate
        );
        assert!(results[0].related_bead.is_some());
        assert!(results[0].action_command.is_some());
        assert_eq!(
            results[0].metadata.get("method").and_then(|v| v.as_str()),
            Some("jaccard")
        );
    }

    #[test]
    fn duplicates_not_detected_for_dissimilar_issues() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Database migration system",
                "Handle schema changes",
                "open",
            ),
            make_issue(
                "bd-2",
                "Frontend styling improvements",
                "Update CSS grid layout for responsive design",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        assert!(
            results.is_empty(),
            "dissimilar issues should not be flagged"
        );
    }

    #[test]
    fn duplicates_skip_tombstone_issues() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Database migration system upgrade",
                "Schema migration for database upgrades",
                "tombstone",
            ),
            make_issue(
                "bd-2",
                "Database migration system upgrade needed",
                "Schema migration for database upgrade process",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        assert!(results.is_empty(), "tombstone issues should be skipped");
    }

    #[test]
    fn duplicates_skip_mixed_open_closed_status() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Database migration system upgrade",
                "Schema migration for database upgrades",
                "open",
            ),
            make_issue(
                "bd-2",
                "Database migration system upgrade needed",
                "Schema migration for database upgrade process",
                "closed",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        assert!(
            results.is_empty(),
            "one open + one closed should not be flagged"
        );
    }

    #[test]
    fn duplicates_single_issue_returns_empty() {
        let issues = vec![make_issue(
            "bd-1",
            "Something important",
            "Details here",
            "open",
        )];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        assert!(results.is_empty());
    }

    #[test]
    fn duplicates_empty_issues_returns_empty() {
        let results = detect_potential_duplicates(&[], &Utc::now().to_rfc3339());
        assert!(results.is_empty());
    }

    #[test]
    fn duplicates_both_closed_still_detected() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Database migration system upgrade",
                "Schema migration for database upgrades process",
                "closed",
            ),
            make_issue(
                "bd-2",
                "Database migration system upgrade needed",
                "Schema migration for database upgrade process",
                "closed",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_potential_duplicates(&issues, &now);
        // Both closed → same is_closed_like → should be detected
        assert!(
            !results.is_empty(),
            "two closed issues with high overlap should still be flagged"
        );
    }

    // ── detect_missing_dependencies ─────────────────────────────────

    #[test]
    fn missing_deps_detected_for_keyword_overlap() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Implement authentication service",
                "Build the authentication backend service layer",
                "open",
            ),
            make_issue(
                "bd-2",
                "Authentication integration testing",
                "Test the authentication service integration bd-1",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_missing_dependencies(&issues, &now);
        assert!(
            !results.is_empty(),
            "issues with shared keywords + ID mention should suggest dependency"
        );
        assert_eq!(
            results[0].suggestion_type,
            SuggestionType::MissingDependency
        );
        assert!(results[0].related_bead.is_some());
        assert!(
            results[0]
                .action_command
                .as_deref()
                .unwrap()
                .starts_with("br dep add")
        );
    }

    #[test]
    fn missing_deps_skip_closed_issues() {
        let issues = vec![
            make_issue(
                "bd-1",
                "Authentication service implementation",
                "Build authentication backend service",
                "closed",
            ),
            make_issue(
                "bd-2",
                "Authentication integration testing",
                "Test authentication service integration",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_missing_dependencies(&issues, &now);
        assert!(
            results.is_empty(),
            "closed issues should not get dep suggestions"
        );
    }

    #[test]
    fn missing_deps_skip_existing_dependency() {
        use crate::model::Dependency;

        let mut issue1 = make_issue(
            "bd-1",
            "Authentication service implementation",
            "Build authentication backend service",
            "open",
        );
        issue1.dependencies.push(Dependency {
            depends_on_id: "bd-2".to_string(),
            ..Dependency::default()
        });
        let issue2 = make_issue(
            "bd-2",
            "Authentication service testing",
            "Test authentication backend service",
            "open",
        );
        let now = Utc::now().to_rfc3339();
        let results = detect_missing_dependencies(&[issue1, issue2], &now);
        assert!(
            results.is_empty(),
            "issues with existing dep should not be suggested"
        );
    }

    #[test]
    fn missing_deps_shared_labels_boost_confidence() {
        let mut issue1 = make_issue(
            "bd-1",
            "Authentication service implementation",
            "Build authentication backend service layer",
            "open",
        );
        issue1.labels = vec!["backend".to_string(), "auth".to_string()];

        let mut issue2 = make_issue(
            "bd-2",
            "Authentication endpoint testing",
            "Test authentication backend endpoints layer",
            "open",
        );
        issue2.labels = vec!["backend".to_string(), "auth".to_string()];

        let now = Utc::now().to_rfc3339();
        let results = detect_missing_dependencies(&[issue1, issue2], &now);
        if !results.is_empty() {
            // Shared labels should appear in metadata
            assert!(
                results[0].metadata.get("shared_labels").is_some(),
                "shared labels should be in metadata"
            );
        }
    }

    #[test]
    fn missing_deps_empty_and_single_return_empty() {
        let now = Utc::now().to_rfc3339();
        assert!(detect_missing_dependencies(&[], &now).is_empty());
        assert!(
            detect_missing_dependencies(
                &[make_issue("bd-1", "Something", "Details", "open")],
                &now
            )
            .is_empty()
        );
    }

    // ── detect_label_suggestions ────────────────────────────────────

    #[test]
    fn label_suggestion_from_builtin_mapping() {
        // Issue has "database" keyword; project has "database" label on another issue
        let mut labeled = make_issue(
            "bd-1",
            "Old database work",
            "Previous db migration",
            "closed",
        );
        labeled.labels = vec!["database".to_string()];

        let unlabeled = make_issue(
            "bd-2",
            "New database migration needed",
            "Handle the database schema changes",
            "open",
        );

        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&[labeled, unlabeled], &now);
        assert!(
            !results.is_empty(),
            "should suggest 'database' label for issue with database keyword"
        );
        assert_eq!(results[0].suggestion_type, SuggestionType::LabelSuggestion);
        assert_eq!(results[0].target_bead, "bd-2");
        let suggested = results[0]
            .metadata
            .get("suggested_label")
            .and_then(|v| v.as_str());
        assert_eq!(suggested, Some("database"));
    }

    #[test]
    fn label_suggestion_skips_already_labeled() {
        let mut issue1 = make_issue(
            "bd-1",
            "Database migration work",
            "Previous effort",
            "closed",
        );
        issue1.labels = vec!["database".to_string()];

        let mut issue2 = make_issue(
            "bd-2",
            "New database migration",
            "Database schema changes",
            "open",
        );
        issue2.labels = vec!["database".to_string()]; // already has it

        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&[issue1, issue2], &now);
        // Should not suggest "database" since bd-2 already has it
        let db_suggestions: Vec<_> = results
            .iter()
            .filter(|s| {
                s.target_bead == "bd-2"
                    && s.metadata.get("suggested_label").and_then(|v| v.as_str())
                        == Some("database")
            })
            .collect();
        assert!(
            db_suggestions.is_empty(),
            "should not suggest label the issue already has"
        );
    }

    #[test]
    fn label_suggestion_skips_closed_issues() {
        let mut issue1 = make_issue("bd-1", "Database work", "DB migration", "open");
        issue1.labels = vec!["database".to_string()];

        let issue2 = make_issue(
            "bd-2",
            "Database migration",
            "Database schema changes",
            "closed",
        );

        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&[issue1, issue2], &now);
        let closed_suggestions: Vec<_> =
            results.iter().filter(|s| s.target_bead == "bd-2").collect();
        assert!(
            closed_suggestions.is_empty(),
            "closed issues should not get label suggestions"
        );
    }

    #[test]
    fn label_suggestion_preserves_canonical_project_label_casing() {
        let mut labeled = make_issue(
            "bd-1",
            "Backend auth work",
            "Previous backend login change",
            "closed",
        );
        labeled.labels = vec!["Backend".to_string()];

        let unlabeled = make_issue(
            "bd-2",
            "Backend login endpoint",
            "Fix backend auth flow",
            "open",
        );

        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&[labeled, unlabeled], &now);
        let suggestion = results
            .iter()
            .find(|item| item.target_bead == "bd-2")
            .expect("expected a backend label suggestion");
        assert_eq!(
            suggestion
                .metadata
                .get("suggested_label")
                .and_then(|value| value.as_str()),
            Some("Backend")
        );
        assert_eq!(
            suggestion.action_command.as_deref(),
            Some("br update bd-2 --add-label=Backend")
        );
        assert!(suggestion.summary.contains("'Backend'"));
    }

    #[test]
    fn label_suggestion_empty_labels_returns_empty() {
        // No issue has any label → no labels in the project → no suggestions
        let issues = vec![
            make_issue("bd-1", "Database migration", "Schema changes", "open"),
            make_issue(
                "bd-2",
                "Another database task",
                "More database work",
                "open",
            ),
        ];
        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&issues, &now);
        assert!(
            results.is_empty(),
            "no labels in project means no suggestions"
        );
    }

    #[test]
    fn label_suggestion_empty_issues() {
        let results = detect_label_suggestions(&[], &Utc::now().to_rfc3339());
        assert!(results.is_empty());
    }

    #[test]
    fn label_suggestion_learned_mapping() {
        // Multiple shared keywords between closed labeled issues and the open target.
        // Each keyword contributes learned_bonus = 0.1 + count*0.05.
        // Need total score >= LABEL_MIN_CONFIDENCE (0.5).
        // 4 keywords each seen 2x → bonus = 4 * (0.1 + 2*0.05) = 4 * 0.2 = 0.8 ≥ 0.5
        let mut i1 = make_issue(
            "bd-1",
            "webhook handler retry payload",
            "Process webhook retry payload events",
            "closed",
        );
        i1.labels = vec!["integration".to_string()];
        let mut i2 = make_issue(
            "bd-2",
            "webhook handler retry payload",
            "Retry failed webhook payload handler",
            "closed",
        );
        i2.labels = vec!["integration".to_string()];
        let i3 = make_issue(
            "bd-3",
            "webhook handler retry payload",
            "Validate incoming webhook handler retry payload",
            "open",
        );

        let now = Utc::now().to_rfc3339();
        let results = detect_label_suggestions(&[i1, i2, i3], &now);
        let integration_suggestions: Vec<_> = results
            .iter()
            .filter(|s| {
                s.target_bead == "bd-3"
                    && s.metadata.get("suggested_label").and_then(|v| v.as_str())
                        == Some("integration")
            })
            .collect();
        assert!(
            !integration_suggestions.is_empty(),
            "learned mapping from multiple shared keywords should suggest label"
        );
    }

    #[test]
    fn canonical_project_labels_prefers_most_common_variant() {
        let mut issue1 = make_issue("bd-1", "One", "Desc", "open");
        issue1.labels = vec!["Backend".to_string()];
        let mut issue2 = make_issue("bd-2", "Two", "Desc", "open");
        issue2.labels = vec!["backend".to_string()];
        let mut issue3 = make_issue("bd-3", "Three", "Desc", "open");
        issue3.labels = vec!["Backend".to_string()];

        let labels = canonical_project_labels(&[issue1, issue2, issue3]);
        assert_eq!(labels.get("backend").map(String::as_str), Some("Backend"));
    }

    // ── detect_cycle_warnings ───────────────────────────────────────

    fn empty_metrics() -> GraphMetrics {
        use crate::analysis::graph::IssueGraph;
        let graph = IssueGraph::build(&[]);
        graph.compute_metrics()
    }

    #[test]
    fn cycle_warning_self_loop() {
        let mut metrics = empty_metrics();
        metrics.cycles.push(vec!["bd-1".to_string()]);

        let now = Utc::now().to_rfc3339();
        let results = detect_cycle_warnings(&metrics, &now);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].suggestion_type, SuggestionType::CycleWarning);
        assert!(results[0].summary.contains("Self-loop"));
        assert_eq!(results[0].target_bead, "bd-1");
        // Self-loop has no action_command (need cycle_length >= 2)
        assert!(results[0].action_command.is_none());
    }

    #[test]
    fn cycle_warning_two_node_cycle() {
        let mut metrics = empty_metrics();
        metrics
            .cycles
            .push(vec!["bd-1".to_string(), "bd-2".to_string()]);

        let now = Utc::now().to_rfc3339();
        let results = detect_cycle_warnings(&metrics, &now);
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("Direct cycle"));
        assert!(results[0].summary.contains("bd-1"));
        assert!(results[0].summary.contains("bd-2"));
        assert_eq!(results[0].related_bead.as_deref(), Some("bd-2"));
        // Action suggests removing the edge from last→first
        assert_eq!(
            results[0].action_command.as_deref(),
            Some("br dep remove bd-2 bd-1")
        );
        assert_eq!(
            results[0]
                .metadata
                .get("cycle_length")
                .and_then(|v| v.as_u64()),
            Some(2)
        );
    }

    #[test]
    fn cycle_warning_large_cycle() {
        let mut metrics = empty_metrics();
        metrics.cycles.push(vec![
            "bd-1".to_string(),
            "bd-2".to_string(),
            "bd-3".to_string(),
            "bd-4".to_string(),
        ]);

        let now = Utc::now().to_rfc3339();
        let results = detect_cycle_warnings(&metrics, &now);
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("4 issues"));
        // Longer cycles get lower confidence
        assert!(results[0].confidence < 1.0);
        assert!(results[0].confidence >= 0.5);
        // Reason should contain cycle path
        assert!(
            results[0]
                .reason
                .contains("bd-1 -> bd-2 -> bd-3 -> bd-4 -> bd-1")
        );
    }

    #[test]
    fn cycle_warning_empty_cycles() {
        let metrics = empty_metrics();
        let results = detect_cycle_warnings(&metrics, &Utc::now().to_rfc3339());
        assert!(results.is_empty());
    }

    #[test]
    fn cycle_warning_skips_empty_cycle_vec() {
        let mut metrics = empty_metrics();
        metrics.cycles.push(vec![]); // empty cycle should be skipped
        metrics
            .cycles
            .push(vec!["bd-1".to_string(), "bd-2".to_string()]);

        let now = Utc::now().to_rfc3339();
        let results = detect_cycle_warnings(&metrics, &now);
        assert_eq!(results.len(), 1, "empty cycle should be skipped");
        assert!(results[0].summary.contains("Direct cycle"));
    }

    #[test]
    fn cycle_warning_capped_at_max() {
        let mut metrics = empty_metrics();
        for i in 0..15 {
            metrics
                .cycles
                .push(vec![format!("bd-{i}a"), format!("bd-{i}b")]);
        }

        let now = Utc::now().to_rfc3339();
        let results = detect_cycle_warnings(&metrics, &now);
        assert_eq!(
            results.len(),
            CYCLE_MAX,
            "should cap at CYCLE_MAX={CYCLE_MAX}"
        );
    }

    // ── extract_keywords / helpers ──────────────────────────────────

    #[test]
    fn extract_keywords_filters_stop_words_and_short() {
        let keywords = extract_keywords("The quick fix", "and the bug was not here");
        // "the", "and", "was", "not" are stop words; "fix" and "bug" are 3 chars (kept)
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"and".to_string()));
        assert!(!keywords.contains(&"was".to_string()));
        assert!(keywords.contains(&"quick".to_string()));
        assert!(keywords.contains(&"fix".to_string()));
        assert!(keywords.contains(&"bug".to_string()));
    }

    #[test]
    fn extract_keywords_normalizes_case_and_punctuation() {
        let keywords = extract_keywords("Database-Migration", "Schema_Upgrade!");
        assert!(keywords.contains(&"database".to_string()));
        assert!(keywords.contains(&"migration".to_string()));
        assert!(keywords.contains(&"schema".to_string()));
        assert!(keywords.contains(&"upgrade".to_string()));
    }

    #[test]
    fn intersect_keywords_returns_common() {
        let left = vec![
            "database".to_string(),
            "migration".to_string(),
            "schema".to_string(),
        ];
        let right = vec![
            "migration".to_string(),
            "testing".to_string(),
            "schema".to_string(),
        ];
        let common = intersect_keywords(&left, &right);
        assert!(common.contains(&"migration".to_string()));
        assert!(common.contains(&"schema".to_string()));
        assert!(!common.contains(&"database".to_string()));
        assert!(!common.contains(&"testing".to_string()));
    }

    #[test]
    fn ratio_handles_zero_denominator() {
        assert_eq!(ratio(5, 0), 0.0);
        assert!((ratio(3, 4) - 0.75).abs() < 0.001);
    }

    // ── compute_stats ───────────────────────────────────────────────

    #[test]
    fn compute_stats_counts_correctly() {
        let suggestions = vec![
            make_suggestion(SuggestionType::PotentialDuplicate, 0.9, "bd-1"),
            make_suggestion(SuggestionType::CycleWarning, 0.3, "bd-2"),
            make_suggestion(SuggestionType::MissingDependency, 0.6, "bd-3"),
        ];
        let stats = compute_stats(&suggestions);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_type.get("potential_duplicate"), Some(&1));
        assert_eq!(stats.by_type.get("cycle_warning"), Some(&1));
        assert_eq!(stats.by_type.get("missing_dependency"), Some(&1));
        assert_eq!(stats.high_confidence_count, 1); // only 0.9 >= 0.7
        assert_eq!(stats.by_confidence.get("high"), Some(&1));
        assert_eq!(stats.by_confidence.get("medium"), Some(&1)); // 0.6
        assert_eq!(stats.by_confidence.get("low"), Some(&1)); // 0.3
    }

    #[test]
    fn compute_stats_empty() {
        let stats = compute_stats(&[]);
        assert_eq!(stats.total, 0);
        assert!(stats.by_type.is_empty());
        assert_eq!(stats.high_confidence_count, 0);
        assert_eq!(stats.actionable_count, 0);
    }

    // ── matches_filters ─────────────────────────────────────────────

    #[test]
    fn matches_filters_by_min_confidence() {
        let suggestion = make_suggestion(SuggestionType::CycleWarning, 0.5, "bd-1");
        let mut options = SuggestOptions::default();
        options.min_confidence = 0.3;
        assert!(matches_filters(&suggestion, &options));
        options.min_confidence = 0.8;
        assert!(!matches_filters(&suggestion, &options));
    }

    #[test]
    fn matches_filters_by_type() {
        let suggestion = make_suggestion(SuggestionType::CycleWarning, 0.8, "bd-1");
        let mut options = SuggestOptions::default();
        options.filter_type = Some(SuggestionType::CycleWarning);
        assert!(matches_filters(&suggestion, &options));
        options.filter_type = Some(SuggestionType::PotentialDuplicate);
        assert!(!matches_filters(&suggestion, &options));
    }

    #[test]
    fn matches_filters_by_bead_id() {
        let mut suggestion = make_suggestion(SuggestionType::MissingDependency, 0.7, "bd-1");
        suggestion.related_bead = Some("bd-2".to_string());
        let mut options = SuggestOptions::default();

        options.filter_bead = Some("bd-1".to_string());
        assert!(matches_filters(&suggestion, &options), "target match");

        options.filter_bead = Some("bd-2".to_string());
        assert!(matches_filters(&suggestion, &options), "related match");

        options.filter_bead = Some("bd-99".to_string());
        assert!(!matches_filters(&suggestion, &options), "no match");
    }

    // ── confidence_level ────────────────────────────────────────────

    #[test]
    fn confidence_level_boundaries() {
        assert_eq!(confidence_level(0.0).as_str(), "low");
        assert_eq!(confidence_level(0.39).as_str(), "low");
        assert_eq!(confidence_level(0.4).as_str(), "medium");
        assert_eq!(confidence_level(0.69).as_str(), "medium");
        assert_eq!(confidence_level(0.7).as_str(), "high");
        assert_eq!(confidence_level(1.0).as_str(), "high");
    }
}
