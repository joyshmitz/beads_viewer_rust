use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::diff::{FieldChange, detect_changes};
use crate::{BvrError, Result, model::Issue};

#[derive(Debug, Clone)]
pub struct GitCommitRecord {
    pub sha: String,
    pub short_sha: String,
    pub timestamp: String,
    pub author: String,
    pub author_email: String,
    pub message: String,
    pub files: Vec<HistoryFileChangeCompat>,
    pub changed_beads: bool,
    pub changed_non_beads: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HistoryMilestonesCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<HistoryEventCompat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed: Option<HistoryEventCompat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<HistoryEventCompat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened: Option<HistoryEventCompat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEventCompat {
    pub bead_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub author_email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryBeadCompat {
    pub bead_id: String,
    pub title: String,
    pub status: String,
    pub events: Vec<HistoryEventCompat>,
    pub milestones: HistoryMilestonesCompat,
    pub commits: Option<Vec<HistoryCommitCompat>>,
    pub cycle_time: Option<HistoryCycleCompat>,
    pub last_author: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryCommitCompat {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub author_email: String,
    pub timestamp: String,
    pub files: Vec<HistoryFileChangeCompat>,
    pub method: String,
    pub confidence: f64,
    pub reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub field_changes: Vec<FieldChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bead_diff_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryFileChangeCompat {
    pub path: String,
    pub action: String,
    pub insertions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryCycleCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_to_close: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_to_close: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_to_claim: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HistoryStatsCompat {
    pub total_beads: usize,
    pub beads_with_commits: usize,
    pub total_commits: usize,
    pub unique_authors: usize,
    pub avg_commits_per_bead: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_cycle_time_days: Option<f64>,
    pub method_distribution: BTreeMap<String, usize>,
}

pub fn load_git_commits(
    repo_root: &Path,
    limit: usize,
    history_since: Option<&str>,
) -> Result<Vec<GitCommitRecord>> {
    if !is_git_work_tree(repo_root) {
        return Ok(Vec::new());
    }

    let mut command = Command::new("git");
    command.arg("-C").arg(repo_root).arg("log");
    if limit > 0 {
        command.arg(format!("-n{limit}"));
    }
    if let Some(since) = history_since {
        command.arg("--since").arg(since);
    }
    command
        .arg("--name-status")
        .arg("--date=iso-strict")
        .arg("--pretty=format:\u{1e}%H\u{1f}%h\u{1f}%cI\u{1f}%an\u{1f}%ae\u{1f}%s");

    let output = command.output()?;
    if !output.status.success() {
        if let Some(since) = history_since {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BvrError::InvalidArgument(format!(
                "Error parsing --history-since '{since}': {}",
                stderr.trim()
            )));
        }
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::<GitCommitRecord>::new();

    for block in text.split('\u{1e}') {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut lines = block.lines();
        let Some(header) = lines.next() else {
            continue;
        };

        let fields = header.split('\u{1f}').collect::<Vec<_>>();
        if fields.len() < 6 {
            continue;
        }

        let mut files = Vec::<HistoryFileChangeCompat>::new();
        let mut changed_beads = false;
        let mut changed_non_beads = false;

        for raw_line in lines {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.len() < 2 {
                continue;
            }

            let status = parts[0];
            let (action, path) = if status.starts_with('R') && parts.len() >= 3 {
                ("R", parts[2])
            } else {
                (&status[..status.len().min(1)], parts[1])
            };

            let path = path.to_string();
            let is_beads = is_beads_jsonl_path(&path);
            changed_beads |= is_beads;
            changed_non_beads |= !is_beads;

            files.push(HistoryFileChangeCompat {
                path,
                action: action.to_string(),
                insertions: 0,
                deletions: 0,
            });
        }

        files.sort_by(|left, right| left.path.cmp(&right.path));

        commits.push(GitCommitRecord {
            sha: fields[0].to_string(),
            short_sha: fields[1].to_string(),
            timestamp: fields[2].to_string(),
            author: fields[3].to_string(),
            author_email: fields[4].to_string(),
            message: fields[5].to_string(),
            files,
            changed_beads,
            changed_non_beads,
        });
    }

    Ok(commits)
}

fn is_git_work_tree(path: &Path) -> bool {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();

    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .eq_ignore_ascii_case("true")
}

pub fn correlate_histories_with_git(
    repo_root: &Path,
    commits: &[GitCommitRecord],
    histories_map: &mut BTreeMap<String, HistoryBeadCompat>,
    commit_index: &mut BTreeMap<String, Vec<String>>,
    method_distribution: &mut BTreeMap<String, usize>,
) {
    let known_ids = histories_map
        .keys()
        .map(|id| (id.to_ascii_lowercase(), id.clone()))
        .collect::<BTreeMap<_, _>>();

    for commit in commits {
        let mut bead_ids = extract_ids_from_message(&commit.message, &known_ids);
        let mut bead_change_details = BTreeMap::<String, (Vec<FieldChange>, Vec<String>)>::new();
        if bead_ids.is_empty() && commit.changed_beads {
            let from_diff = extract_ids_from_beads_diffs(repo_root, commit, &known_ids);
            bead_ids.extend(from_diff);
        }
        if commit.changed_beads {
            bead_change_details = extract_bead_change_details(repo_root, commit, &known_ids);
            bead_ids.extend(bead_change_details.keys().cloned());
        }
        if bead_ids.is_empty() {
            continue;
        }

        let (method, confidence, reason) = if commit.changed_beads && commit.changed_non_beads {
            (
                "co_committed",
                0.95,
                "Commit modified beads metadata and code paths together".to_string(),
            )
        } else if commit.changed_beads {
            (
                "explicit_id",
                0.85,
                "Commit references bead changes explicitly".to_string(),
            )
        } else {
            (
                "explicit_id",
                0.75,
                "Commit message references bead ID".to_string(),
            )
        };

        for bead_id in bead_ids {
            let Some(history) = histories_map.get_mut(&bead_id) else {
                continue;
            };

            let commits = history.commits.get_or_insert_with(Vec::new);
            if commits.iter().any(|entry| entry.sha == commit.sha) {
                continue;
            }

            commits.push(HistoryCommitCompat {
                sha: commit.sha.clone(),
                short_sha: commit.short_sha.clone(),
                message: commit.message.clone(),
                author: commit.author.clone(),
                author_email: commit.author_email.clone(),
                timestamp: commit.timestamp.clone(),
                files: commit.files.clone(),
                method: method.to_string(),
                confidence,
                reason: reason.clone(),
                field_changes: bead_change_details
                    .get(&bead_id)
                    .map(|(changes, _)| changes.clone())
                    .unwrap_or_default(),
                bead_diff_lines: bead_change_details
                    .get(&bead_id)
                    .map(|(_, diff_lines)| diff_lines.clone())
                    .unwrap_or_default(),
            });

            let ids = commit_index.entry(commit.sha.clone()).or_default();
            if !ids.contains(&bead_id) {
                ids.push(bead_id.clone());
            }

            *method_distribution.entry(method.to_string()).or_insert(0) += 1;
        }
    }

    for ids in commit_index.values_mut() {
        ids.sort();
        ids.dedup();
    }
}

pub fn extract_ids_from_message(
    message: &str,
    known_ids: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let message = message.to_ascii_lowercase();
    known_ids
        .iter()
        .filter_map(|(lower, canonical)| {
            if contains_issue_id_token(&message, lower) {
                Some(canonical.clone())
            } else {
                None
            }
        })
        .collect()
}

fn contains_issue_id_token(message: &str, issue_id: &str) -> bool {
    if issue_id.is_empty() {
        return false;
    }

    message.match_indices(issue_id).any(|(start, _)| {
        let left = message[..start].chars().next_back();
        let right = message[start + issue_id.len()..].chars().next();

        let left_boundary = left.is_none_or(|ch| !is_issue_id_char(ch));
        let right_boundary = right.is_none_or(|ch| !is_issue_id_char(ch));

        left_boundary && right_boundary
    })
}

const fn is_issue_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

fn extract_ids_from_beads_diffs(
    repo_root: &Path,
    commit: &GitCommitRecord,
    known_ids: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::<String>::new();

    for file in &commit.files {
        if !is_beads_jsonl_path(&file.path) {
            continue;
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg("--format=")
            .arg("--unified=0")
            .arg(&commit.sha)
            .arg("--")
            .arg(&file.path)
            .output();

        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if !(line.starts_with('+') || line.starts_with('-'))
                || line.starts_with("+++")
                || line.starts_with("---")
            {
                continue;
            }

            let content = line.trim_start_matches(['+', '-']).trim();
            if !(content.starts_with('{') && content.ends_with('}')) {
                continue;
            }

            let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
                continue;
            };
            let Some(raw_id) = value.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if let Some(canonical) = known_ids.get(&raw_id.to_ascii_lowercase()) {
                ids.insert(canonical.clone());
            }
        }
    }

    ids
}

fn extract_bead_change_details(
    repo_root: &Path,
    commit: &GitCommitRecord,
    known_ids: &BTreeMap<String, String>,
) -> BTreeMap<String, (Vec<FieldChange>, Vec<String>)> {
    let mut before = BTreeMap::<String, serde_json::Value>::new();
    let mut after = BTreeMap::<String, serde_json::Value>::new();

    for file in &commit.files {
        if !is_beads_jsonl_path(&file.path) {
            continue;
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg("--format=")
            .arg("--unified=0")
            .arg(&commit.sha)
            .arg("--")
            .arg(&file.path)
            .output();

        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if !(line.starts_with('+') || line.starts_with('-'))
                || line.starts_with("+++")
                || line.starts_with("---")
            {
                continue;
            }

            let content = line.trim_start_matches(['+', '-']).trim();
            if !(content.starts_with('{') && content.ends_with('}')) {
                continue;
            }

            let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
                continue;
            };
            let Some(raw_id) = value.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(canonical) = known_ids.get(&raw_id.to_ascii_lowercase()) else {
                continue;
            };

            if line.starts_with('-') {
                before.insert(canonical.clone(), value);
            } else {
                after.insert(canonical.clone(), value);
            }
        }
    }

    let mut details = BTreeMap::<String, (Vec<FieldChange>, Vec<String>)>::new();
    let bead_ids = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for bead_id in bead_ids {
        let field_changes = match (before.get(&bead_id), after.get(&bead_id)) {
            (Some(old_value), Some(new_value)) => {
                match (
                    serde_json::from_value::<Issue>(old_value.clone()),
                    serde_json::from_value::<Issue>(new_value.clone()),
                ) {
                    (Ok(old_issue), Ok(new_issue)) => detect_changes(&old_issue, &new_issue),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };

        let mut diff_lines = field_changes
            .iter()
            .flat_map(|change| {
                [
                    format!("- {}: {}", change.field, change.old_value),
                    format!("+ {}: {}", change.field, change.new_value),
                ]
            })
            .collect::<Vec<_>>();

        if diff_lines.is_empty() {
            if let Some(old_value) = before.get(&bead_id) {
                diff_lines.push(format!("- issue: {}", summarize_bead_snapshot(old_value)));
            }
            if let Some(new_value) = after.get(&bead_id) {
                diff_lines.push(format!("+ issue: {}", summarize_bead_snapshot(new_value)));
            }
        }

        details.insert(bead_id, (field_changes, diff_lines));
    }

    details
}

fn summarize_bead_snapshot(value: &serde_json::Value) -> String {
    let id = value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if title.is_empty() {
        format!("{id} [{status}]")
    } else {
        format!("{id} [{status}] {title}")
    }
}

fn is_beads_jsonl_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with(".beads/")
        && Path::new(&normalized)
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("jsonl"))
}

fn is_closed_like_status(status: &str) -> bool {
    matches!(status, "closed" | "tombstone")
}

pub fn finalize_history_entries(histories_map: &mut BTreeMap<String, HistoryBeadCompat>) {
    for history in histories_map.values_mut() {
        if let Some(commits) = history.commits.as_mut() {
            commits.sort_by(|left, right| {
                compare_timestamps(&left.timestamp, &right.timestamp)
                    .then_with(|| left.sha.cmp(&right.sha))
            });
        }

        if let Some(commits) = history.commits.as_ref().filter(|c| !c.is_empty()) {
            let mut events = commits
                .iter()
                .enumerate()
                .map(|(index, commit)| HistoryEventCompat {
                    bead_id: history.bead_id.clone(),
                    event_type: infer_event_type_from_commit(index, &commit.message),
                    timestamp: commit.timestamp.clone(),
                    commit_sha: commit.sha.clone(),
                    commit_message: commit.message.clone(),
                    author: commit.author.clone(),
                    author_email: commit.author_email.clone(),
                })
                .collect::<Vec<_>>();

            if !events.iter().any(|entry| entry.event_type == "created")
                && let Some(first) = commits.first()
            {
                events.insert(
                    0,
                    HistoryEventCompat {
                        bead_id: history.bead_id.clone(),
                        event_type: "created".to_string(),
                        timestamp: first.timestamp.clone(),
                        commit_sha: first.sha.clone(),
                        commit_message: first.message.clone(),
                        author: first.author.clone(),
                        author_email: first.author_email.clone(),
                    },
                );
            }

            if is_closed_like_status(&history.status.to_ascii_lowercase())
                && !events.iter().any(|entry| entry.event_type == "closed")
                && let Some(last) = commits.last()
            {
                events.push(HistoryEventCompat {
                    bead_id: history.bead_id.clone(),
                    event_type: "closed".to_string(),
                    timestamp: last.timestamp.clone(),
                    commit_sha: last.sha.clone(),
                    commit_message: last.message.clone(),
                    author: last.author.clone(),
                    author_email: last.author_email.clone(),
                });
            }

            events.sort_by(|left, right| {
                compare_timestamps(&left.timestamp, &right.timestamp)
                    .then_with(|| left.event_type.cmp(&right.event_type))
            });
            history.events = events;
        }

        history.milestones = HistoryMilestonesCompat {
            created: history
                .events
                .iter()
                .find(|event| event.event_type == "created")
                .cloned(),
            claimed: history
                .events
                .iter()
                .find(|event| event.event_type == "claimed")
                .cloned(),
            closed: history
                .events
                .iter()
                .find(|event| event.event_type == "closed")
                .cloned(),
            reopened: history
                .events
                .iter()
                .rev()
                .find(|event| event.event_type == "reopened")
                .cloned(),
        };

        let create_to_close = duration_between(
            history
                .milestones
                .created
                .as_ref()
                .map(|event| event.timestamp.as_str()),
            history
                .milestones
                .closed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
        );
        let claim_to_close = duration_between(
            history
                .milestones
                .claimed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
            history
                .milestones
                .closed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
        );
        let create_to_claim = duration_between(
            history
                .milestones
                .created
                .as_ref()
                .map(|event| event.timestamp.as_str()),
            history
                .milestones
                .claimed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
        );

        if create_to_close.is_some() || claim_to_close.is_some() || create_to_claim.is_some() {
            history.cycle_time = Some(HistoryCycleCompat {
                claim_to_close: claim_to_close.map(format_duration_compact),
                create_to_close: create_to_close.map(format_duration_compact),
                create_to_claim: create_to_claim.map(format_duration_compact),
            });
        }

        history.last_author = history
            .commits
            .as_ref()
            .and_then(|c| c.last())
            .map_or_else(String::new, |commit| commit.author.clone());

        // Normalize: empty Vec -> None (serializes as null, matching legacy)
        if history.commits.as_ref().is_some_and(Vec::is_empty) {
            history.commits = None;
        }
    }
}

fn infer_event_type_from_commit(index: usize, message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if has_word_token(&lower, "reopen") || has_word_token(&lower, "reopened") {
        "reopened".to_string()
    } else if has_word_token(&lower, "close") || has_word_token(&lower, "closed") {
        "closed".to_string()
    } else if has_word_token(&lower, "claim")
        || has_word_token(&lower, "claimed")
        || lower.contains("in_progress")
        || has_word_sequence(&lower, "in progress")
    {
        "claimed".to_string()
    } else if index == 0 {
        "created".to_string()
    } else {
        "modified".to_string()
    }
}

/// Check if `text` contains `token` delimited by non-alphanumeric boundaries.
/// This avoids false positives like "disclose" matching "close" or
/// "closedown" matching "closed".
fn has_word_token(text: &str, token: &str) -> bool {
    text.match_indices(token).any(|(start, _)| {
        let end = start + token.len();
        let left = start == 0 || !text.as_bytes()[start - 1].is_ascii_alphanumeric();
        let right = end == text.len() || !text.as_bytes()[end].is_ascii_alphanumeric();
        left && right
    })
}

/// Check if `text` contains the exact two-word sequence.
fn has_word_sequence(text: &str, sequence: &str) -> bool {
    text.contains(sequence)
}

fn compare_timestamps(left: &str, right: &str) -> std::cmp::Ordering {
    match (parse_rfc3339_utc(left), parse_rfc3339_utc(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn duration_between(start: Option<&str>, end: Option<&str>) -> Option<chrono::Duration> {
    let start = start.and_then(parse_rfc3339_utc)?;
    let end = end.and_then(parse_rfc3339_utc)?;
    let duration = end - start;
    if duration.num_seconds() >= 0 {
        Some(duration)
    } else {
        None
    }
}

fn format_duration_compact(duration: chrono::Duration) -> String {
    let days = duration.num_days();
    let hours = duration.num_hours() - days * 24;
    let minutes = duration.num_minutes() - duration.num_hours() * 60;
    format!("{days}d {hours}h {minutes}m")
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn has_word_token_rejects_substring_match() {
        // "disclose" should NOT match "close"
        assert!(!has_word_token("disclose the issue", "close"));
        // "exclaim" should NOT match "claim"
        assert!(!has_word_token("exclaim loudly", "claim"));
        // "reopen" embedded in "unreopened" should not match at word boundary
        assert!(!has_word_token("unreopened", "reopen"));
        // Suffix continuations should not match either
        assert!(!has_word_token("closedown the task", "closed"));
        assert!(!has_word_token("claimer picked it up", "claim"));
    }

    #[test]
    fn has_word_token_accepts_word_boundary() {
        assert!(has_word_token("close the issue", "close"));
        assert!(has_word_token("closed bd-123", "closed"));
        assert!(has_word_token("claim this task", "claim"));
        assert!(has_word_token("reopen bd-456", "reopen"));
        assert!(has_word_token("reopened bd-456", "reopened"));
        // At start of text
        assert!(has_word_token("close", "close"));
        // After punctuation
        assert!(has_word_token("[close] bd-789", "close"));
    }

    #[test]
    fn infer_event_type_close_vs_disclose() {
        assert_eq!(infer_event_type_from_commit(1, "close bd-123"), "closed");
        assert_eq!(infer_event_type_from_commit(1, "Closed bd-123"), "closed");
        // "disclose" should NOT trigger "closed"
        assert_eq!(
            infer_event_type_from_commit(1, "disclose internal details"),
            "modified"
        );
        assert_eq!(
            infer_event_type_from_commit(1, "closedown remaining tasks"),
            "modified"
        );
    }

    #[test]
    fn infer_event_type_claim_vs_exclaim() {
        assert_eq!(infer_event_type_from_commit(1, "claim bd-abc"), "claimed");
        assert_eq!(
            infer_event_type_from_commit(1, "set status to in_progress"),
            "claimed"
        );
        assert_eq!(
            infer_event_type_from_commit(1, "mark in progress"),
            "claimed"
        );
        // "exclaim" should NOT trigger "claimed"
        assert_eq!(
            infer_event_type_from_commit(1, "exclaim about progress"),
            "modified"
        );
        assert_eq!(
            infer_event_type_from_commit(1, "claimer rotation updated"),
            "modified"
        );
    }

    #[test]
    fn infer_event_type_reopen_vs_embedded() {
        assert_eq!(infer_event_type_from_commit(1, "reopen bd-xyz"), "reopened");
        assert_eq!(
            infer_event_type_from_commit(1, "Reopened the issue"),
            "reopened"
        );
    }

    #[test]
    fn infer_event_type_index_zero_fallback() {
        // Index 0 with no keyword match => "created"
        assert_eq!(infer_event_type_from_commit(0, "initial setup"), "created");
        // Index >0 with no keyword match => "modified"
        assert_eq!(infer_event_type_from_commit(1, "update readme"), "modified");
    }

    #[test]
    fn none_commits_serialize_as_null() {
        let history = HistoryBeadCompat {
            bead_id: "bd-test".to_string(),
            title: "Test".to_string(),
            status: "open".to_string(),
            events: vec![],
            milestones: HistoryMilestonesCompat::default(),
            commits: None,
            cycle_time: None,
            last_author: String::new(),
        };
        let json = serde_json::to_value(&history).unwrap();
        // commits and cycle_time are always present (as null) to match
        // legacy Go output shape.
        assert!(
            json.get("commits").is_some_and(serde_json::Value::is_null),
            "None commits should serialize as null"
        );
        assert!(
            json.get("cycle_time")
                .is_some_and(serde_json::Value::is_null),
            "None cycle_time should serialize as null"
        );
    }

    #[test]
    fn some_commits_serialize_as_array() {
        let history = HistoryBeadCompat {
            bead_id: "bd-test".to_string(),
            title: "Test".to_string(),
            status: "open".to_string(),
            events: vec![],
            milestones: HistoryMilestonesCompat::default(),
            commits: Some(vec![HistoryCommitCompat {
                sha: "abc123".to_string(),
                short_sha: "abc".to_string(),
                message: "test commit".to_string(),
                author: "tester".to_string(),
                author_email: "test@example.com".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                files: vec![],
                method: "log".to_string(),
                confidence: 1.0,
                reason: "test".to_string(),
                field_changes: vec![],
                bead_diff_lines: vec![],
            }]),
            cycle_time: None,
            last_author: String::new(),
        };
        let json = serde_json::to_value(&history).unwrap();
        assert!(
            json["commits"].is_array(),
            "Some commits should serialize as array"
        );
        assert_eq!(json["commits"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn milestones_omit_null_fields() {
        let milestones = HistoryMilestonesCompat {
            created: Some(HistoryEventCompat {
                bead_id: "bd-test".to_string(),
                event_type: "created".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                commit_sha: "abc".to_string(),
                commit_message: "init".to_string(),
                author: "tester".to_string(),
                author_email: "test@example.com".to_string(),
            }),
            claimed: None,
            closed: None,
            reopened: None,
        };
        let json = serde_json::to_value(&milestones).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("created"), "created should be present");
        assert!(
            !obj.contains_key("claimed"),
            "None claimed should be omitted"
        );
        assert!(!obj.contains_key("closed"), "None closed should be omitted");
        assert!(
            !obj.contains_key("reopened"),
            "None reopened should be omitted"
        );
    }

    #[test]
    fn cycle_time_omits_null_fields() {
        let cycle = HistoryCycleCompat {
            create_to_close: Some("2d 3h 0m".to_string()),
            claim_to_close: None,
            create_to_claim: None,
        };
        let json = serde_json::to_value(&cycle).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("create_to_close"));
        assert!(
            !obj.contains_key("claim_to_close"),
            "None should be omitted"
        );
        assert!(
            !obj.contains_key("create_to_claim"),
            "None should be omitted"
        );
    }

    #[test]
    fn history_stats_omit_absent_avg_cycle_time_days() {
        let stats = HistoryStatsCompat {
            total_beads: 2,
            beads_with_commits: 0,
            total_commits: 0,
            unique_authors: 0,
            avg_commits_per_bead: 0.0,
            avg_cycle_time_days: None,
            method_distribution: BTreeMap::new(),
        };

        let json = serde_json::to_value(&stats).unwrap();
        let obj = json.as_object().unwrap();
        assert!(
            !obj.contains_key("avg_cycle_time_days"),
            "None avg_cycle_time_days should be omitted"
        );
    }
}

pub fn compute_history_stats(
    histories_map: &BTreeMap<String, HistoryBeadCompat>,
    commit_index: &BTreeMap<String, Vec<String>>,
    method_distribution: BTreeMap<String, usize>,
) -> HistoryStatsCompat {
    let total_beads = histories_map.len();
    let beads_with_commits = histories_map
        .values()
        .filter(|history| history.commits.as_ref().is_some_and(|c| !c.is_empty()))
        .count();
    let total_commits = commit_index.len();

    let mut authors = BTreeSet::<String>::new();
    let mut claim_to_close_days = Vec::<f64>::new();

    for history in histories_map.values() {
        for commit in history.commits.as_deref().unwrap_or_default() {
            if !commit.author.is_empty() {
                authors.insert(commit.author.clone());
            }
        }
        for event in &history.events {
            if !event.author.is_empty() {
                authors.insert(event.author.clone());
            }
        }

        if let Some(duration) = duration_between(
            history
                .milestones
                .claimed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
            history
                .milestones
                .closed
                .as_ref()
                .map(|event| event.timestamp.as_str()),
        ) {
            let seconds_i32 = i32::try_from(duration.num_seconds()).unwrap_or(i32::MAX);
            claim_to_close_days.push(f64::from(seconds_i32) / 86_400.0);
        }
    }

    let avg_commits_per_bead = if beads_with_commits == 0 {
        0.0
    } else {
        let total_commits_u32 = u32::try_from(total_commits).unwrap_or(u32::MAX);
        let beads_with_commits_u32 = u32::try_from(beads_with_commits).unwrap_or(u32::MAX);
        f64::from(total_commits_u32) / f64::from(beads_with_commits_u32)
    };

    let avg_cycle_time_days = if claim_to_close_days.is_empty() {
        None
    } else {
        let count_u32 = u32::try_from(claim_to_close_days.len()).unwrap_or(u32::MAX);
        Some(claim_to_close_days.iter().sum::<f64>() / f64::from(count_u32))
    };

    HistoryStatsCompat {
        total_beads,
        beads_with_commits,
        total_commits,
        unique_authors: authors.len(),
        avg_commits_per_bead,
        avg_cycle_time_days,
        method_distribution,
    }
}
