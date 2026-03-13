//! Advanced graph insights: `TopKSet`, `CoverageSet`, `KPaths`, `CycleBreak`, `ParallelCut`, `ParallelGain`.
//!
//! Implements six advanced analysis algorithms that build on the core graph metrics.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::analysis::graph::{GraphMetrics, IssueGraph};
use crate::model::Issue;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Aggregates all six advanced insight types.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedInsights {
    pub top_k_set: TopKSetResult,
    pub coverage_set: CoverageSetResult,
    pub k_paths: KPathsResult,
    pub cycle_break: CycleBreakResult,
    pub parallel_cut: ParallelCutResult,
    pub parallel_gain: ParallelGainResult,
    pub config: AdvancedInsightsConfig,
    pub usage_hints: Vec<String>,
}

/// Configuration used to produce the advanced insights.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedInsightsConfig {
    pub top_k: usize,
    pub k_paths_k: usize,
    pub max_cycle_break: usize,
    pub max_parallel_cut: usize,
}

impl Default for AdvancedInsightsConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            k_paths_k: 5,
            max_cycle_break: 10,
            max_parallel_cut: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// 1. TopKSet – greedy submodular set of issues that maximize downstream unlocks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TopKSetResult {
    pub items: Vec<TopKItem>,
    pub total_unlocked: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopKItem {
    pub id: String,
    pub marginal_unlocks: usize,
    pub cumulative_unlocks: usize,
}

/// Greedy selection of up to `k` issues whose completion maximally unlocks downstream work.
///
/// At each step we pick the open issue whose completion would unblock the most new downstream
/// issues (that are not already unblocked by previously selected items).
fn compute_top_k_set(graph: &IssueGraph, _metrics: &GraphMetrics, k: usize) -> TopKSetResult {
    let open_ids: Vec<String> = graph
        .issue_ids_sorted()
        .into_iter()
        .filter(|id| graph.issue(id).is_some_and(Issue::is_open_like))
        .collect();

    // Pre-compute transitive downstream set for each open issue (BFS through dependents).
    let downstream_map: HashMap<String, HashSet<String>> = open_ids
        .iter()
        .map(|id| {
            let downstream = bfs_downstream(graph, id);
            (id.clone(), downstream)
        })
        .collect();

    let mut selected = Vec::new();
    let mut already_unlocked = HashSet::<String>::new();
    let mut remaining: HashSet<String> = open_ids.into_iter().collect();

    for _ in 0..k {
        if remaining.is_empty() {
            break;
        }

        // Find the issue whose downstream set gains the most new unlocks.
        let best = remaining
            .iter()
            .map(|id| {
                let downstream = downstream_map.get(id).map_or(0, |set| {
                    set.iter()
                        .filter(|d| !already_unlocked.contains(*d) && *d != id)
                        .count()
                });
                (id.clone(), downstream)
            })
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

        let Some((best_id, marginal)) = best else {
            break;
        };

        if marginal == 0 && !selected.is_empty() {
            break;
        }

        // Add downstream of this issue to already_unlocked.
        if let Some(ds) = downstream_map.get(&best_id) {
            for d in ds {
                already_unlocked.insert(d.clone());
            }
        }
        already_unlocked.insert(best_id.clone());
        remaining.remove(&best_id);

        selected.push(TopKItem {
            id: best_id,
            marginal_unlocks: marginal,
            cumulative_unlocks: already_unlocked.len(),
        });
    }

    TopKSetResult {
        total_unlocked: already_unlocked.len(),
        items: selected,
    }
}

/// BFS downstream through dependents (issues that depend on `start_id`).
fn bfs_downstream(graph: &IssueGraph, start_id: &str) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start_id.to_string());

    while let Some(current) = queue.pop_front() {
        for dep in graph.dependents(&current) {
            if visited.insert(dep.clone()) {
                queue.push_back(dep);
            }
        }
    }
    visited
}

// ---------------------------------------------------------------------------
// 2. CoverageSet – greedy vertex cover of the critical path DAG
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CoverageSetResult {
    pub items: Vec<String>,
    pub paths_covered: usize,
    pub total_paths: usize,
}

/// Greedy minimum vertex cover: select issues that appear on the most critical paths.
///
/// We approximate by collecting all edges on the critical sub-DAG (issues with non-zero
/// critical depth) and greedily covering them.
fn compute_coverage_set(graph: &IssueGraph, metrics: &GraphMetrics) -> CoverageSetResult {
    // Collect edges where both endpoints have critical_depth > 0.
    let mut edges: Vec<(String, String)> = Vec::new();
    let critical_ids: HashSet<&String> = metrics
        .critical_depth
        .iter()
        .filter(|(_, d)| **d > 0)
        .map(|(id, _)| id)
        .collect();

    for id in &critical_ids {
        for dep in graph.dependents(id) {
            if critical_ids.contains(&dep) {
                edges.push(((*id).clone(), dep));
            }
        }
    }

    let total_paths = edges.len();
    let mut uncovered = edges;
    let mut selected = Vec::new();

    while !uncovered.is_empty() {
        // Count how many uncovered edges each node touches.
        let mut freq: HashMap<String, usize> = HashMap::new();
        for (a, b) in &uncovered {
            *freq.entry(a.clone()).or_default() += 1;
            *freq.entry(b.clone()).or_default() += 1;
        }

        // Pick the node with highest frequency (ties broken by ID).
        let best = freq
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

        let Some((best_id, _)) = best else {
            break;
        };

        // Remove all edges touching best_id.
        uncovered.retain(|(a, b)| a != &best_id && b != &best_id);
        selected.push(best_id);
    }

    selected.sort();

    CoverageSetResult {
        paths_covered: total_paths,
        total_paths,
        items: selected,
    }
}

// ---------------------------------------------------------------------------
// 3. KPaths – K shortest critical paths (simplified Yen's algorithm)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct KPathsResult {
    pub paths: Vec<CriticalPath>,
    pub k: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CriticalPath {
    pub path: Vec<String>,
    pub length: usize,
    pub depth_sum: usize,
}

/// Find up to `k` longest (most critical) paths through the dependency graph.
///
/// Uses iterative DFS from source nodes (those with no open blockers) to sink nodes
/// (those with no open dependents), collecting the longest paths first.
fn compute_k_paths(graph: &IssueGraph, metrics: &GraphMetrics, k: usize) -> KPathsResult {
    let open_ids: HashSet<String> = graph
        .issue_ids_sorted()
        .into_iter()
        .filter(|id| graph.issue(id).is_some_and(Issue::is_open_like))
        .collect();

    if open_ids.is_empty() {
        return KPathsResult {
            paths: Vec::new(),
            k,
        };
    }

    // Source nodes: open issues with no open blockers.
    let sources: Vec<String> = open_ids
        .iter()
        .filter(|id| graph.open_blockers(id).is_empty())
        .cloned()
        .collect();

    // DFS from each source to find all maximal paths.
    let mut all_paths: Vec<Vec<String>> = Vec::new();
    let path_cap = (10 * k).min(10_000);

    'outer: for source in &sources {
        let mut stack: Vec<(String, Vec<String>)> = vec![(source.clone(), vec![source.clone()])];
        let mut visited_from_source = HashSet::new();

        while let Some((current, path)) = stack.pop() {
            let deps: Vec<String> = graph
                .dependents(&current)
                .into_iter()
                .filter(|d| open_ids.contains(d) && !path.contains(d))
                .collect();

            if deps.is_empty() {
                // Leaf path — only keep non-trivial paths.
                if path.len() > 1 {
                    all_paths.push(path);
                    if all_paths.len() >= path_cap {
                        break 'outer;
                    }
                }
            } else {
                for dep in deps {
                    if visited_from_source.insert((current.clone(), dep.clone())) {
                        let mut new_path = path.clone();
                        new_path.push(dep.clone());
                        stack.push((dep, new_path));
                    }
                }
            }
        }
    }

    // Score by length then depth_sum, take top k.
    let mut scored: Vec<CriticalPath> = all_paths
        .into_iter()
        .map(|path| {
            let depth_sum: usize = path
                .iter()
                .filter_map(|id| metrics.critical_depth.get(id))
                .sum();
            let length = path.len();
            CriticalPath {
                path,
                length,
                depth_sum,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.length
            .cmp(&a.length)
            .then_with(|| b.depth_sum.cmp(&a.depth_sum))
    });
    scored.truncate(k);

    KPathsResult { paths: scored, k }
}

// ---------------------------------------------------------------------------
// 4. CycleBreak – minimum feedback arc set (greedy approximation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CycleBreakResult {
    pub suggestions: Vec<CycleBreakSuggestion>,
    pub cycles_before: usize,
    pub estimated_cycles_after: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleBreakSuggestion {
    pub from: String,
    pub to: String,
    pub cycles_broken: usize,
    pub collateral_score: f64,
}

/// Suggest edges to remove to break dependency cycles with minimal collateral damage.
///
/// Greedy: at each step, find the edge appearing in the most remaining cycles,
/// weighted inversely by the edge's importance (pagerank of endpoints).
fn compute_cycle_break(
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    max_suggestions: usize,
) -> CycleBreakResult {
    let cycles_before = metrics.cycles.len();

    if cycles_before == 0 {
        return CycleBreakResult {
            suggestions: Vec::new(),
            cycles_before: 0,
            estimated_cycles_after: 0,
        };
    }

    // Collect all edges within cycles.
    let mut remaining_cycles: Vec<Vec<String>> = metrics.cycles.clone();
    let mut suggestions = Vec::new();
    let mut estimated_remaining = remaining_cycles.len();

    for _ in 0..max_suggestions {
        if remaining_cycles.is_empty() {
            break;
        }

        // Count edge frequency across remaining cycles.
        let mut edge_freq: HashMap<(String, String), usize> = HashMap::new();
        for cycle in &remaining_cycles {
            // Edges within the SCC: check actual dependency edges between cycle members.
            let cycle_set: HashSet<&String> = cycle.iter().collect();
            for member in cycle {
                for blocker in graph.blockers(member) {
                    if cycle_set.contains(&blocker) {
                        *edge_freq.entry((blocker, member.clone())).or_default() += 1;
                    }
                }
            }
        }

        if edge_freq.is_empty() {
            break;
        }

        // Pick edge with highest frequency; break ties by lowest collateral (pagerank sum).
        let best = edge_freq
            .iter()
            .map(|((from, to), freq)| {
                let pr_from = metrics.pagerank.get(from).copied().unwrap_or_default();
                let pr_to = metrics.pagerank.get(to).copied().unwrap_or_default();
                let collateral = pr_from + pr_to;
                (from.clone(), to.clone(), *freq, collateral)
            })
            .max_by(|a, b| {
                a.2.cmp(&b.2)
                    .then_with(|| a.3.total_cmp(&b.3).reverse())
                    .then_with(|| b.0.cmp(&a.0))
            });

        let Some((from, to, freq, collateral)) = best else {
            break;
        };

        // Remove cycles that contained this edge.
        remaining_cycles.retain(|cycle| {
            let set: HashSet<&String> = cycle.iter().collect();
            !(set.contains(&from) && set.contains(&to))
        });

        estimated_remaining = remaining_cycles.len();

        suggestions.push(CycleBreakSuggestion {
            from,
            to,
            cycles_broken: freq,
            collateral_score: collateral,
        });
    }

    CycleBreakResult {
        suggestions,
        cycles_before,
        estimated_cycles_after: estimated_remaining,
    }
}

// ---------------------------------------------------------------------------
// 5. ParallelCut – edges whose removal maximizes parallelization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ParallelCutResult {
    pub cuts: Vec<ParallelCutEdge>,
    pub current_serial_depth: usize,
    pub estimated_depth_after: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParallelCutEdge {
    pub from: String,
    pub to: String,
    pub depth_reduction: usize,
}

/// Find edges on the critical path whose removal would reduce the longest chain.
///
/// Heuristic: edges between issues with high `critical_depth` differences that sit on
/// the longest chain are candidates for removal to increase parallelism.
fn compute_parallel_cut(
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    max_cuts: usize,
) -> ParallelCutResult {
    let max_depth = metrics.critical_depth.values().copied().max().unwrap_or(0);

    if max_depth == 0 {
        return ParallelCutResult {
            cuts: Vec::new(),
            current_serial_depth: 0,
            estimated_depth_after: 0,
        };
    }

    // Find edges on the critical path (where both endpoints are on critical path
    // and depth decreases by exactly 1).
    let mut candidates: Vec<ParallelCutEdge> = Vec::new();

    for id in graph.issue_ids_sorted() {
        let depth = metrics.critical_depth.get(&id).copied().unwrap_or(0);
        if depth == 0 {
            continue;
        }

        for blocker in graph.blockers(&id) {
            let blocker_depth = metrics.critical_depth.get(&blocker).copied().unwrap_or(0);
            // An edge is on the critical path if the blocker's depth == our depth + 1
            // (blocker is one level above dependent in the critical chain).
            if blocker_depth == depth + 1 {
                candidates.push(ParallelCutEdge {
                    from: blocker,
                    to: id.clone(),
                    depth_reduction: 1,
                });
            }
        }
    }

    // Sort by depth of 'from' node (higher = more impactful cut), descending.
    candidates.sort_by(|a, b| {
        let a_depth = metrics.critical_depth.get(&a.from).copied().unwrap_or(0);
        let b_depth = metrics.critical_depth.get(&b.from).copied().unwrap_or(0);
        b_depth
            .cmp(&a_depth)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });
    candidates.truncate(max_cuts);

    let estimated_after = if candidates.is_empty() {
        max_depth
    } else {
        max_depth.saturating_sub(1)
    };

    ParallelCutResult {
        cuts: candidates,
        current_serial_depth: max_depth,
        estimated_depth_after: estimated_after,
    }
}

// ---------------------------------------------------------------------------
// 6. ParallelGain – estimate throughput gain from removing dependencies
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ParallelGainResult {
    pub current_components: usize,
    pub current_max_chain: usize,
    pub gains: Vec<ParallelGainItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParallelGainItem {
    pub edge_from: String,
    pub edge_to: String,
    pub new_components: usize,
    pub new_max_chain: usize,
    pub parallelism_delta: f64,
}

/// Estimate how much parallelism each edge removal would provide.
///
/// For each critical-path edge, we simulate removal and measure:
/// - Change in number of connected components (more = more parallel tracks).
/// - Change in longest chain length (shorter = faster completion with parallel agents).
fn compute_parallel_gain(
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    max_items: usize,
) -> ParallelGainResult {
    let open_ids: HashSet<String> = graph
        .issue_ids_sorted()
        .into_iter()
        .filter(|id| graph.issue(id).is_some_and(Issue::is_open_like))
        .collect();

    let current_components = graph.connected_open_components().len();
    let current_max = metrics.critical_depth.values().copied().max().unwrap_or(0);

    if open_ids.is_empty() || current_max == 0 {
        return ParallelGainResult {
            current_components,
            current_max_chain: current_max,
            gains: Vec::new(),
        };
    }

    // Collect critical-path edges (same logic as parallel_cut).
    let mut critical_edges: Vec<(String, String)> = Vec::new();
    for id in &open_ids {
        let depth = metrics.critical_depth.get(id).copied().unwrap_or(0);
        if depth == 0 {
            continue;
        }
        for blocker in graph.blockers(id) {
            if !open_ids.contains(&blocker) {
                continue;
            }
            let blocker_depth = metrics.critical_depth.get(&blocker).copied().unwrap_or(0);
            if blocker_depth == depth + 1 {
                critical_edges.push((blocker, id.clone()));
            }
        }
    }

    // For each critical edge, simulate removal and compute new component count + max chain.
    let mut gains: Vec<ParallelGainItem> = critical_edges
        .iter()
        .map(|(from, to)| {
            let (new_components, new_max) =
                simulate_edge_removal(graph, &open_ids, metrics, from, to);
            let parallelism_delta = if current_max > 0 {
                (current_max as f64 - new_max as f64) / current_max as f64
            } else {
                0.0
            };
            ParallelGainItem {
                edge_from: from.clone(),
                edge_to: to.clone(),
                new_components,
                new_max_chain: new_max,
                parallelism_delta,
            }
        })
        .collect();

    gains.sort_by(|a, b| {
        b.parallelism_delta
            .total_cmp(&a.parallelism_delta)
            .then_with(|| a.edge_from.cmp(&b.edge_from))
    });
    gains.truncate(max_items);

    ParallelGainResult {
        current_components,
        current_max_chain: current_max,
        gains,
    }
}

/// Simulate removing one edge and compute connected components + max chain length.
fn simulate_edge_removal(
    graph: &IssueGraph,
    open_ids: &HashSet<String>,
    _metrics: &GraphMetrics,
    remove_from: &str,
    remove_to: &str,
) -> (usize, usize) {
    // Build adjacency (blockers → dependents) without the removed edge.
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut rev_adj: HashMap<String, Vec<String>> = HashMap::new();

    for id in open_ids {
        for blocker in graph.blockers(id) {
            if !open_ids.contains(&blocker) {
                continue;
            }
            if blocker == remove_from && id == remove_to {
                continue; // Skip removed edge.
            }
            adj.entry(blocker.clone()).or_default().push(id.clone());
            rev_adj.entry(id.clone()).or_default().push(blocker);
        }
    }

    // Connected components (undirected view).
    let mut visited = HashSet::new();
    let mut component_count = 0usize;
    for id in open_ids {
        if visited.contains(id) {
            continue;
        }
        component_count += 1;
        let mut queue = VecDeque::new();
        queue.push_back(id.clone());
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            for neighbor in adj.get(&current).into_iter().flatten() {
                if !visited.contains(neighbor) {
                    queue.push_back(neighbor.clone());
                }
            }
            for neighbor in rev_adj.get(&current).into_iter().flatten() {
                if !visited.contains(neighbor) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    // Max chain: compute critical depth in the modified graph (longest path via BFS/topological).
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for id in open_ids {
        in_degree.entry(id.clone()).or_default();
    }
    for deps in adj.values() {
        for dep in deps {
            *in_degree.entry(dep.clone()).or_default() += 1;
        }
    }

    let mut depth: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();

    for id in &queue {
        depth.insert(id.clone(), 0);
    }

    while let Some(current) = queue.pop_front() {
        let current_depth = depth.get(&current).copied().unwrap_or(0);
        for neighbor in adj.get(&current).into_iter().flatten() {
            let new_depth = current_depth + 1;
            let entry = depth.entry(neighbor.clone()).or_default();
            if new_depth > *entry {
                *entry = new_depth;
            }
            let deg = in_degree.get_mut(neighbor).unwrap();
            *deg -= 1;
            if *deg == 0 {
                queue.push_back(neighbor.clone());
            }
        }
    }

    let max_chain = depth.values().copied().max().unwrap_or(0);

    (component_count, max_chain)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute all advanced insights with default configuration.
#[must_use]
pub fn compute_advanced_insights(graph: &IssueGraph, metrics: &GraphMetrics) -> AdvancedInsights {
    compute_advanced_insights_with_config(graph, metrics, &AdvancedInsightsConfig::default())
}

/// Compute all advanced insights with custom configuration.
#[must_use]
pub fn compute_advanced_insights_with_config(
    graph: &IssueGraph,
    metrics: &GraphMetrics,
    config: &AdvancedInsightsConfig,
) -> AdvancedInsights {
    let top_k_set = compute_top_k_set(graph, metrics, config.top_k);
    let coverage_set = compute_coverage_set(graph, metrics);
    let k_paths = compute_k_paths(graph, metrics, config.k_paths_k);
    let cycle_break = compute_cycle_break(graph, metrics, config.max_cycle_break);
    let parallel_cut = compute_parallel_cut(graph, metrics, config.max_parallel_cut);
    let parallel_gain = compute_parallel_gain(graph, metrics, config.max_parallel_cut);

    AdvancedInsights {
        top_k_set,
        coverage_set,
        k_paths,
        cycle_break,
        parallel_cut,
        parallel_gain,
        config: config.clone(),
        usage_hints: vec![
            "jq '.top_k_set.items[:5]' — Top 5 issues to unlock the most downstream work"
                .to_string(),
            "jq '.coverage_set.items' — Minimal set covering all critical paths".to_string(),
            "jq '.k_paths.paths[0].path' — Longest critical path through the graph".to_string(),
            "jq '.cycle_break.suggestions' — Edges to remove to break dependency cycles"
                .to_string(),
            "jq '.parallel_cut.cuts' — Edges to remove for maximum parallelization".to_string(),
            "jq '.parallel_gain.gains[:3]' — Top 3 edges whose removal increases parallelism"
                .to_string(),
        ],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dependency, Issue};

    fn make_issue(id: &str, deps: &[&str]) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Issue {id}"),
            status: "open".to_string(),
            priority: 2,
            dependencies: deps
                .iter()
                .map(|d| Dependency {
                    issue_id: id.to_string(),
                    depends_on_id: d.to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn make_closed_issue(id: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Issue {id}"),
            status: "closed".to_string(),
            priority: 2,
            ..Default::default()
        }
    }

    // -- Empty graph --

    #[test]
    fn empty_graph_returns_empty_results() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let result = compute_advanced_insights(&graph, &metrics);

        assert!(result.top_k_set.items.is_empty());
        assert_eq!(result.top_k_set.total_unlocked, 0);
        assert!(result.coverage_set.items.is_empty());
        assert!(result.k_paths.paths.is_empty());
        assert!(result.cycle_break.suggestions.is_empty());
        assert!(result.parallel_cut.cuts.is_empty());
        assert!(result.parallel_gain.gains.is_empty());
    }

    // -- Single node --

    #[test]
    fn single_node_graph() {
        let issues = vec![make_issue("A", &[])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_advanced_insights(&graph, &metrics);

        // Single node: no downstream to unlock, no paths, no cycles.
        assert!(result.top_k_set.items.len() <= 1);
        assert!(result.k_paths.paths.is_empty());
        assert!(result.cycle_break.suggestions.is_empty());
        assert!(result.parallel_cut.cuts.is_empty());
    }

    // -- Linear chain: A -> B -> C -> D --

    #[test]
    fn linear_chain_top_k_identifies_root_blocker() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_top_k_set(&graph, &metrics, 3);

        // A blocks everything downstream, so it should be picked first.
        assert!(!result.items.is_empty());
        assert_eq!(result.items[0].id, "A");
        assert!(result.items[0].marginal_unlocks >= 3);
    }

    #[test]
    fn linear_chain_coverage_set() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_coverage_set(&graph, &metrics);

        // Should cover all critical-path edges with a small vertex set.
        assert!(!result.items.is_empty());
        assert!(result.items.len() <= 3); // Greedy cover of 3 edges needs at most 2-3 vertices.
    }

    #[test]
    fn linear_chain_k_paths_returns_full_chain() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_k_paths(&graph, &metrics, 5);

        // Should find the full A -> B -> C -> D path.
        assert!(!result.paths.is_empty());
        let longest = &result.paths[0];
        assert_eq!(longest.length, 4);
        assert_eq!(longest.path, vec!["A", "B", "C", "D"]);
    }

    #[test]
    fn linear_chain_parallel_cut() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_parallel_cut(&graph, &metrics, 5);

        // Should identify critical-path edges.
        assert!(!result.cuts.is_empty());
        // Depth: D=1, C=2, B=3, A=4 (root blocker has highest depth).
        assert_eq!(result.current_serial_depth, 4);
    }

    // -- Diamond: A -> B, A -> C, B -> D, C -> D --

    #[test]
    fn diamond_graph_top_k() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["A"]),
            make_issue("D", &["B", "C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_top_k_set(&graph, &metrics, 5);

        // A unlocks B, C, and transitively D.
        assert!(!result.items.is_empty());
        assert_eq!(result.items[0].id, "A");
    }

    #[test]
    fn diamond_parallel_gain_identifies_parallelizable_edges() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["A"]),
            make_issue("D", &["B", "C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_parallel_gain(&graph, &metrics, 5);

        // There should be critical-path edges to potentially cut.
        assert!(result.current_max_chain >= 2);
    }

    // -- Cycle: A -> B -> C -> A --

    #[test]
    fn cycle_break_identifies_edges_to_remove() {
        let issues = vec![
            make_issue("A", &["C"]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        assert!(!metrics.cycles.is_empty(), "expected cycles in graph");

        let result = compute_cycle_break(&graph, &metrics, 5);
        assert!(!result.suggestions.is_empty());
        assert!(result.cycles_before > 0);
        assert!(result.estimated_cycles_after < result.cycles_before);
    }

    // -- Closed issues are excluded --

    #[test]
    fn closed_issues_excluded_from_top_k() {
        let issues = vec![
            make_issue("A", &[]),
            make_closed_issue("B"),
            make_issue("C", &["A"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_top_k_set(&graph, &metrics, 5);

        // B is closed, only A and C are open.
        for item in &result.items {
            assert_ne!(item.id, "B");
        }
    }

    // -- No cycles --

    #[test]
    fn no_cycles_returns_empty_cycle_break() {
        let issues = vec![make_issue("A", &[]), make_issue("B", &["A"])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_cycle_break(&graph, &metrics, 5);

        assert!(result.suggestions.is_empty());
        assert_eq!(result.cycles_before, 0);
        assert_eq!(result.estimated_cycles_after, 0);
    }

    // -- Larger graph (12 nodes) --

    #[test]
    fn larger_graph_all_algorithms_produce_results() {
        // Build a graph with multiple paths and a cycle.
        //
        // Epic1: E1 -> T1, T2, T3
        // Epic2: E2 -> T4, T5
        // T3 -> T6 -> T7
        // T5 -> T7 (cross-epic dep)
        // T7 -> T8 -> T9
        // Cycle: T10 -> T11 -> T12 -> T10
        let issues = vec![
            make_issue("E1", &[]),
            make_issue("E2", &[]),
            make_issue("T1", &["E1"]),
            make_issue("T2", &["E1"]),
            make_issue("T3", &["E1"]),
            make_issue("T4", &["E2"]),
            make_issue("T5", &["E2"]),
            make_issue("T6", &["T3"]),
            make_issue("T7", &["T6", "T5"]),
            make_issue("T8", &["T7"]),
            make_issue("T9", &["T8"]),
            make_issue("T10", &["T12"]),
            make_issue("T11", &["T10"]),
            make_issue("T12", &["T11"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_advanced_insights(&graph, &metrics);

        // TopKSet should identify E1 and E2 as high-impact.
        assert!(!result.top_k_set.items.is_empty());
        assert!(result.top_k_set.total_unlocked >= 5);

        // KPaths should find paths of length >= 4.
        assert!(!result.k_paths.paths.is_empty());
        assert!(result.k_paths.paths[0].length >= 4);

        // CycleBreak should identify cycle T10-T11-T12.
        assert!(result.cycle_break.cycles_before > 0);
        assert!(!result.cycle_break.suggestions.is_empty());

        // ParallelCut and ParallelGain depend on critical_depth which requires a DAG.
        // When cycles exist (T10-T11-T12), topological sort fails and critical_depth is all
        // zeros, so these legitimately return empty results.  Verify structural invariants
        // instead of requiring non-empty output.
        assert_eq!(
            result.parallel_cut.current_serial_depth,
            result.parallel_gain.current_max_chain
        );
    }

    // -- Config customization --

    #[test]
    fn custom_config_limits_results() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        let config = AdvancedInsightsConfig {
            top_k: 1,
            k_paths_k: 1,
            max_cycle_break: 1,
            max_parallel_cut: 1,
        };
        let result = compute_advanced_insights_with_config(&graph, &metrics, &config);

        assert!(result.top_k_set.items.len() <= 1);
        assert!(result.k_paths.paths.len() <= 1);
    }

    // -- bfs_downstream ---

    #[test]
    fn bfs_downstream_no_dependents() {
        let issues = vec![make_issue("A", &[])];
        let graph = IssueGraph::build(&issues);
        let downstream = bfs_downstream(&graph, "A");
        assert!(downstream.is_empty());
    }

    #[test]
    fn bfs_downstream_transitive() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &["B"]),
        ];
        let graph = IssueGraph::build(&issues);
        let downstream = bfs_downstream(&graph, "A");
        assert!(downstream.contains("B"));
        assert!(downstream.contains("C"));
    }

    // -- TopKSet edge cases --

    #[test]
    fn top_k_set_all_closed_returns_empty() {
        let issues = vec![make_closed_issue("A"), make_closed_issue("B")];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_top_k_set(&graph, &metrics, 5);
        assert!(result.items.is_empty());
        assert_eq!(result.total_unlocked, 0);
    }

    #[test]
    fn top_k_set_independent_issues_stops_after_first() {
        // Issues with no deps and no dependents: marginal=0 after first pick
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &[]),
            make_issue("C", &[]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_top_k_set(&graph, &metrics, 10);
        // First pick has marginal=0 but is selected; second marginal=0 should stop
        assert_eq!(result.items.len(), 1);
    }

    // -- CoverageSet edge cases --

    #[test]
    fn coverage_set_no_critical_paths() {
        let issues = vec![make_issue("A", &[]), make_issue("B", &[])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_coverage_set(&graph, &metrics);
        // Independent nodes have critical_depth 0, so no critical edges
        assert!(result.items.is_empty());
        assert_eq!(result.total_paths, 0);
    }

    // -- KPaths edge cases --

    #[test]
    fn k_paths_all_closed_returns_empty() {
        let issues = vec![make_closed_issue("A"), make_closed_issue("B")];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_k_paths(&graph, &metrics, 5);
        assert!(result.paths.is_empty());
    }

    #[test]
    fn k_paths_single_edge_returns_path_of_length_2() {
        let issues = vec![make_issue("A", &[]), make_issue("B", &["A"])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_k_paths(&graph, &metrics, 5);
        assert!(!result.paths.is_empty());
        assert_eq!(result.paths[0].length, 2);
    }

    // -- ParallelCut edge cases --

    #[test]
    fn parallel_cut_no_deps_returns_empty_cuts() {
        let issues = vec![make_issue("A", &[]), make_issue("B", &[])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_parallel_cut(&graph, &metrics, 5);
        // No dependency edges means no critical-path edges to cut
        assert!(result.cuts.is_empty());
    }

    // -- ParallelGain edge cases --

    #[test]
    fn parallel_gain_no_open_returns_empty() {
        let issues = vec![make_closed_issue("A"), make_closed_issue("B")];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_parallel_gain(&graph, &metrics, 5);
        assert!(result.gains.is_empty());
    }

    #[test]
    fn parallel_gain_two_independent_chains() {
        let issues = vec![
            make_issue("A", &[]),
            make_issue("B", &["A"]),
            make_issue("C", &[]),
            make_issue("D", &["C"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_parallel_gain(&graph, &metrics, 5);
        assert!(result.current_components >= 2);
    }

    // -- Config default values --

    #[test]
    fn default_config_values() {
        let config = AdvancedInsightsConfig::default();
        assert_eq!(config.top_k, 10);
        assert_eq!(config.k_paths_k, 5);
        assert_eq!(config.max_cycle_break, 10);
        assert_eq!(config.max_parallel_cut, 10);
    }

    // -- Usage hints --

    #[test]
    fn usage_hints_populated() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        let result = compute_advanced_insights(&graph, &metrics);
        assert!(!result.usage_hints.is_empty());
        assert!(result.usage_hints.iter().any(|h| h.contains("jq")));
    }

    // -- simulate_edge_removal --

    #[test]
    fn simulate_edge_removal_splits_components() {
        // A -> B: removing this edge should split into 2 components
        let issues = vec![make_issue("A", &[]), make_issue("B", &["A"])];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let open_ids: HashSet<String> = issues.iter().map(|i| i.id.clone()).collect();

        let (components, max_chain) = simulate_edge_removal(&graph, &open_ids, &metrics, "A", "B");
        assert_eq!(components, 2);
        assert_eq!(max_chain, 0); // No edges remain, so max chain is 0
    }

    // -- Large acyclic graph: parallel_cut and parallel_gain require DAG --

    #[test]
    fn acyclic_graph_parallel_cut_and_gain() {
        // Same structure as larger_graph minus the cycle nodes.
        // E1 -> T1, T2, T3; E2 -> T4, T5; T3 -> T6 -> T7; T5 -> T7; T7 -> T8 -> T9
        let issues = vec![
            make_issue("E1", &[]),
            make_issue("E2", &[]),
            make_issue("T1", &["E1"]),
            make_issue("T2", &["E1"]),
            make_issue("T3", &["E1"]),
            make_issue("T4", &["E2"]),
            make_issue("T5", &["E2"]),
            make_issue("T6", &["T3"]),
            make_issue("T7", &["T6", "T5"]),
            make_issue("T8", &["T7"]),
            make_issue("T9", &["T8"]),
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let result = compute_advanced_insights(&graph, &metrics);

        // DAG with no cycles: critical_depth is computed properly.
        assert!(!result.parallel_cut.cuts.is_empty());
        // Longest chain: E1 -> T3 -> T6 -> T7 -> T8 -> T9 = depth 6.
        assert!(result.parallel_cut.current_serial_depth >= 5);
        assert!(!result.parallel_gain.gains.is_empty());
        assert!(result.parallel_gain.current_max_chain >= 5);
    }
}
