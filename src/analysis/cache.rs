use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::model::Issue;

use super::graph::{AnalysisConfig, GraphMetrics, IssueGraph};

/// Default time-to-live for cached metrics.
const DEFAULT_TTL_SECS: u64 = 300; // 5 minutes

/// A cached set of graph metrics with expiry tracking.
#[derive(Clone)]
struct CacheEntry {
    metrics: GraphMetrics,
    inserted_at: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

/// In-memory cache for graph metrics, keyed by a SHA-256 hash of the graph structure.
///
/// The cache avoids recomputing expensive metrics (PageRank, betweenness, HITS, etc.)
/// when the underlying issue graph has not changed. This is especially valuable for
/// TUI sessions where multiple views read the same metrics repeatedly.
pub struct MetricsCache {
    entries: Mutex<HashMap<[u8; 32], CacheEntry>>,
    ttl: Duration,
    hits: Mutex<u64>,
    misses: Mutex<u64>,
}

/// Cache hit/miss statistics.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
}

impl MetricsCache {
    /// Create a new cache with default TTL (5 minutes).
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
            hits: Mutex::new(0),
            misses: Mutex::new(0),
        }
    }

    /// Create a cache with a custom TTL.
    #[must_use]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            hits: Mutex::new(0),
            misses: Mutex::new(0),
        }
    }

    /// Compute (or retrieve cached) metrics for the given issues and config.
    ///
    /// The cache key is a SHA-256 hash of the graph structure (sorted node IDs,
    /// edges, and issue statuses) plus the analysis config.
    pub fn get_or_compute(&self, issues: &[Issue], config: &AnalysisConfig) -> GraphMetrics {
        let key = compute_cache_key(issues, config);

        // Check cache.
        {
            let entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = entries.get(&key) {
                if !entry.is_expired() {
                    *self.hits.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
                    return entry.metrics.clone();
                }
            }
        }

        // Cache miss — compute metrics.
        *self.misses.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        let graph = IssueGraph::build(issues);
        let metrics = graph.compute_metrics_with_config(config);

        // Store in cache.
        {
            let mut entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            // Evict expired entries on insert to prevent unbounded growth.
            entries.retain(|_, entry| !entry.is_expired());
            entries.insert(
                key,
                CacheEntry {
                    metrics: metrics.clone(),
                    inserted_at: Instant::now(),
                    ttl: self.ttl,
                },
            );
        }

        metrics
    }

    /// Invalidate all cached entries.
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Return cache statistics.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let hits = *self.hits.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let misses = *self.misses.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        CacheStats {
            hits,
            misses,
            entries: entries.len(),
        }
    }
}

impl Default for MetricsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a SHA-256 cache key from the graph structure.
///
/// The key incorporates:
/// - Sorted issue IDs
/// - Issue statuses (since open/closed affects metrics like actionable sets)
/// - Dependency edges (sorted)
/// - AnalysisConfig toggles (since they affect which metrics are computed)
fn compute_cache_key(issues: &[Issue], config: &AnalysisConfig) -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Sort issues by ID for determinism.
    let mut sorted_ids: Vec<(&str, &str)> = issues
        .iter()
        .map(|i| (i.id.as_str(), i.status.as_str()))
        .collect();
    sorted_ids.sort();

    for (id, status) in &sorted_ids {
        hasher.update(id.as_bytes());
        hasher.update(b"\0");
        hasher.update(status.as_bytes());
        hasher.update(b"\n");
    }

    // Include edges.
    let mut edges: Vec<(&str, &str)> = Vec::new();
    for issue in issues {
        for dep in &issue.dependencies {
            if dep.is_blocking() {
                edges.push((issue.id.as_str(), dep.depends_on_id.as_str()));
            }
        }
    }
    edges.sort();
    for (from, to) in &edges {
        hasher.update(from.as_bytes());
        hasher.update(b"->");
        hasher.update(to.as_bytes());
        hasher.update(b"\n");
    }

    // Include config toggles that affect metric computation.
    hasher.update(if config.enable_pagerank {
        b"pr:1"
    } else {
        b"pr:0"
    });
    hasher.update(if config.enable_betweenness {
        b"bt:1"
    } else {
        b"bt:0"
    });
    hasher.update(if config.enable_eigenvector {
        b"ev:1"
    } else {
        b"ev:0"
    });
    hasher.update(if config.enable_hits { b"hi:1" } else { b"hi:0" });
    hasher.update(if config.enable_k_core {
        b"kc:1"
    } else {
        b"kc:0"
    });
    hasher.update(if config.enable_cycles {
        b"cy:1"
    } else {
        b"cy:0"
    });
    hasher.update(if config.enable_critical_path {
        b"cp:1"
    } else {
        b"cp:0"
    });
    hasher.update(if config.enable_articulation {
        b"ap:1"
    } else {
        b"ap:0"
    });
    hasher.update(if config.enable_slack {
        b"sl:1"
    } else {
        b"sl:0"
    });

    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dependency, Issue};
    use std::time::Duration;

    fn make_issue(id: &str, status: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Issue {id}"),
            status: status.to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            ..Issue::default()
        }
    }

    fn make_blocked(id: &str, depends_on: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Issue {id}"),
            status: "blocked".to_string(),
            issue_type: "task".to_string(),
            priority: 2,
            dependencies: vec![Dependency {
                issue_id: id.to_string(),
                depends_on_id: depends_on.to_string(),
                dep_type: "blocks".to_string(),
                ..Dependency::default()
            }],
            ..Issue::default()
        }
    }

    #[test]
    fn cache_hit_returns_same_metrics() {
        let cache = MetricsCache::new();
        let issues = vec![make_issue("A", "open"), make_blocked("B", "A")];
        let config = AnalysisConfig::default();

        let first = cache.get_or_compute(&issues, &config);
        let second = cache.get_or_compute(&issues, &config);

        // Same PageRank values.
        assert_eq!(first.pagerank.get("A"), second.pagerank.get("A"));
        assert_eq!(first.pagerank.get("B"), second.pagerank.get("B"));

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn cache_miss_on_changed_graph() {
        let cache = MetricsCache::new();
        let config = AnalysisConfig::default();

        let issues_v1 = vec![make_issue("A", "open")];
        cache.get_or_compute(&issues_v1, &config);

        let issues_v2 = vec![make_issue("A", "open"), make_issue("B", "open")];
        cache.get_or_compute(&issues_v2, &config);

        let stats = cache.stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn cache_miss_on_status_change() {
        let cache = MetricsCache::new();
        let config = AnalysisConfig::default();

        let issues_open = vec![make_issue("A", "open")];
        cache.get_or_compute(&issues_open, &config);

        let issues_closed = vec![make_issue("A", "closed")];
        cache.get_or_compute(&issues_closed, &config);

        let stats = cache.stats();
        assert_eq!(stats.misses, 2, "status change should invalidate cache");
    }

    #[test]
    fn cache_ttl_expiry() {
        let cache = MetricsCache::with_ttl(Duration::from_millis(1));
        let issues = vec![make_issue("A", "open")];
        let config = AnalysisConfig::default();

        cache.get_or_compute(&issues, &config);
        // Sleep past the TTL.
        std::thread::sleep(Duration::from_millis(10));
        cache.get_or_compute(&issues, &config);

        let stats = cache.stats();
        assert_eq!(stats.misses, 2, "expired entry should cause a miss");
    }

    #[test]
    fn cache_clear() {
        let cache = MetricsCache::new();
        let issues = vec![make_issue("A", "open")];
        let config = AnalysisConfig::default();

        cache.get_or_compute(&issues, &config);
        assert_eq!(cache.stats().entries, 1);

        cache.clear();
        assert_eq!(cache.stats().entries, 0);

        // Should recompute.
        cache.get_or_compute(&issues, &config);
        assert_eq!(cache.stats().misses, 2);
    }

    #[test]
    fn cache_key_deterministic() {
        let issues = vec![make_issue("A", "open"), make_blocked("B", "A")];
        let config = AnalysisConfig::default();

        let key1 = compute_cache_key(&issues, &config);
        let key2 = compute_cache_key(&issues, &config);
        assert_eq!(key1, key2);
    }

    #[test]
    fn cache_key_differs_for_different_config() {
        let issues = vec![make_issue("A", "open")];
        let config1 = AnalysisConfig::default();
        let mut config2 = AnalysisConfig::default();
        config2.enable_pagerank = false;

        let key1 = compute_cache_key(&issues, &config1);
        let key2 = compute_cache_key(&issues, &config2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn cache_empty_issues() {
        let cache = MetricsCache::new();
        let issues: Vec<Issue> = vec![];
        let config = AnalysisConfig::default();

        let metrics = cache.get_or_compute(&issues, &config);
        assert!(metrics.pagerank.is_empty());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn cache_default_impl() {
        let cache = MetricsCache::default();
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.entries, 0);
    }
}
