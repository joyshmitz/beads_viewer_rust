use serde::Serialize;

use crate::model::Issue;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEvent {
    pub kind: String,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueHistory {
    pub id: String,
    pub title: String,
    pub status: String,
    pub events: Vec<HistoryEvent>,
}

fn history_event_order(event: &HistoryEvent) -> (u8, Option<chrono::DateTime<chrono::Utc>>, u8) {
    match event.kind.as_str() {
        "created" => (0, event.timestamp, 0),
        "updated" => (1, event.timestamp, 1),
        "closed" => (1, event.timestamp, 2),
        "dependency" => (2, event.timestamp, 3),
        _ => (1, event.timestamp, 4),
    }
}

#[must_use]
pub fn build_histories(
    issues: &[Issue],
    only_issue_id: Option<&str>,
    limit: usize,
) -> Vec<IssueHistory> {
    let mut histories = Vec::<IssueHistory>::new();

    for issue in issues {
        if only_issue_id.is_some_and(|id| id != issue.id) {
            continue;
        }

        let mut events = Vec::<HistoryEvent>::new();
        events.push(HistoryEvent {
            kind: "created".to_string(),
            timestamp: issue.created_at.clone(),
            details: format!("Issue {} created", issue.id),
        });

        if issue.updated_at.is_some() {
            events.push(HistoryEvent {
                kind: "updated".to_string(),
                timestamp: issue.updated_at.clone(),
                details: format!("Current status: {}", issue.status),
            });
        }

        if issue.closed_at.is_some() || issue.is_closed_like() {
            events.push(HistoryEvent {
                kind: "closed".to_string(),
                timestamp: issue.closed_at.clone().or_else(|| issue.updated_at.clone()),
                details: format!("Issue {} is in closed-like status", issue.id),
            });
        }

        for dep in &issue.dependencies {
            if dep.is_blocking() {
                events.push(HistoryEvent {
                    kind: "dependency".to_string(),
                    timestamp: None,
                    details: format!("Blocked by {}", dep.depends_on_id),
                });
            }
        }

        events.sort_by_key(history_event_order);

        histories.push(IssueHistory {
            id: issue.id.clone(),
            title: issue.title.clone(),
            status: issue.status.clone(),
            events,
        });
    }

    histories.sort_by(|left, right| left.id.cmp(&right.id));
    if limit > 0 {
        histories.truncate(limit);
    }

    histories
}

#[cfg(test)]
mod tests {
    use crate::model::{Dependency, Issue, ts};

    use super::build_histories;

    #[test]
    fn builds_history_for_single_issue() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            created_at: ts("2026-01-01T00:00:00Z"),
            updated_at: ts("2026-01-02T00:00:00Z"),
            ..Issue::default()
        }];

        let histories = build_histories(&issues, Some("A"), 10);
        assert_eq!(histories.len(), 1);
        assert!(histories[0].events.len() >= 2);
    }

    #[test]
    fn includes_dependency_events_for_blocked_issue() {
        let issues = vec![
            Issue {
                id: "bd-3q0".to_string(),
                title: "Primary blocker".to_string(),
                status: "in_progress".to_string(),
                issue_type: "feature".to_string(),
                created_at: ts("2026-02-18T03:00:00Z"),
                updated_at: ts("2026-02-18T03:05:00Z"),
                ..Issue::default()
            },
            Issue {
                id: "bd-3q1".to_string(),
                title: "Follow-on work".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                created_at: ts("2026-02-18T03:01:00Z"),
                updated_at: ts("2026-02-18T03:06:00Z"),
                dependencies: vec![Dependency {
                    issue_id: "bd-3q1".to_string(),
                    depends_on_id: "bd-3q0".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let histories = build_histories(&issues, Some("bd-3q1"), 10);
        assert_eq!(histories.len(), 1);
        assert!(
            histories[0].events.iter().any(|event| {
                event.kind == "dependency" && event.details == "Blocked by bd-3q0"
            })
        );
    }

    #[test]
    fn untimestamped_dependency_events_sort_after_timestamped_events() {
        let issues = vec![Issue {
            id: "bd-4z1".to_string(),
            title: "Blocked follow-on".to_string(),
            status: "blocked".to_string(),
            issue_type: "task".to_string(),
            created_at: ts("2026-02-18T03:01:00Z"),
            updated_at: ts("2026-02-18T03:06:00Z"),
            dependencies: vec![Dependency {
                issue_id: "bd-4z1".to_string(),
                depends_on_id: "bd-4z0".to_string(),
                dep_type: "blocks".to_string(),
                ..Dependency::default()
            }],
            ..Issue::default()
        }];

        let histories = build_histories(&issues, Some("bd-4z1"), 10);
        let events = &histories[0].events;
        assert_eq!(events[0].kind, "created");
        assert_eq!(events[1].kind, "updated");
        assert_eq!(events[2].kind, "dependency");
    }

    #[test]
    fn created_event_stays_first_even_without_created_timestamp() {
        let issues = vec![Issue {
            id: "bd-5a1".to_string(),
            title: "Timestamp gap".to_string(),
            status: "blocked".to_string(),
            issue_type: "task".to_string(),
            created_at: None,
            updated_at: ts("2026-02-18T03:06:00Z"),
            dependencies: vec![Dependency {
                issue_id: "bd-5a1".to_string(),
                depends_on_id: "bd-5a0".to_string(),
                dep_type: "blocks".to_string(),
                ..Dependency::default()
            }],
            ..Issue::default()
        }];

        let histories = build_histories(&issues, Some("bd-5a1"), 10);
        let events = &histories[0].events;
        assert_eq!(events[0].kind, "created");
        assert_eq!(events[1].kind, "updated");
        assert_eq!(events[2].kind, "dependency");
    }

    #[test]
    fn empty_input_returns_empty() {
        let histories = build_histories(&[], None, 10);
        assert!(histories.is_empty());
    }

    #[test]
    fn limit_zero_returns_all() {
        let issues = vec![
            Issue { id: "A".into(), title: "A".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
            Issue { id: "B".into(), title: "B".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
        ];
        let histories = build_histories(&issues, None, 0);
        assert_eq!(histories.len(), 2);
    }

    #[test]
    fn limit_one_truncates() {
        let issues = vec![
            Issue { id: "A".into(), title: "A".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
            Issue { id: "B".into(), title: "B".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
        ];
        let histories = build_histories(&issues, None, 1);
        assert_eq!(histories.len(), 1);
    }

    #[test]
    fn nonexistent_filter_id_returns_empty() {
        let issues = vec![Issue {
            id: "A".into(), title: "A".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default()
        }];
        let histories = build_histories(&issues, Some("Z"), 10);
        assert!(histories.is_empty());
    }

    #[test]
    fn closed_like_issue_without_closed_at_uses_updated_at() {
        let issues = vec![Issue {
            id: "C".into(),
            title: "Closed".into(),
            status: "closed".into(),
            issue_type: "task".into(),
            closed_at: None,
            updated_at: ts("2026-03-01T00:00:00Z"),
            ..Issue::default()
        }];
        let histories = build_histories(&issues, None, 10);
        let closed_event = histories[0].events.iter().find(|e| e.kind == "closed").unwrap();
        assert_eq!(closed_event.timestamp, ts("2026-03-01T00:00:00Z"));
    }

    #[test]
    fn results_sorted_by_id() {
        let issues = vec![
            Issue { id: "C".into(), title: "C".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
            Issue { id: "A".into(), title: "A".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
            Issue { id: "B".into(), title: "B".into(), status: "open".into(), issue_type: "task".into(), ..Issue::default() },
        ];
        let histories = build_histories(&issues, None, 0);
        let ids: Vec<&str> = histories.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }
}
