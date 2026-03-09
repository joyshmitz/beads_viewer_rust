use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use petgraph::algo::kosaraju_scc;
use petgraph::graph::DiGraph;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::model::Issue;

// ---------------------------------------------------------------------------
// AnalysisConfig – per-metric enable/disable toggles and size thresholds
// ---------------------------------------------------------------------------

/// Configuration controlling which graph metrics are computed and their resource bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub enable_pagerank: bool,
    pub enable_betweenness: bool,
    pub enable_eigenvector: bool,
    pub enable_hits: bool,
    pub enable_cycles: bool,
    pub enable_critical_path: bool,
    pub enable_k_core: bool,
    pub enable_articulation: bool,
    pub enable_slack: bool,

    /// Skip betweenness for graphs exceeding this node count (expensive: O(V*E)).
    pub betweenness_max_nodes: usize,
    /// Skip eigenvector for graphs exceeding this node count.
    pub eigenvector_max_nodes: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self::full()
    }
}

impl AnalysisConfig {
    /// All metrics enabled, generous size limits (matches current behavior).
    #[must_use]
    pub const fn full() -> Self {
        Self {
            enable_pagerank: true,
            enable_betweenness: true,
            enable_eigenvector: true,
            enable_hits: true,
            enable_cycles: true,
            enable_critical_path: true,
            enable_k_core: true,
            enable_articulation: true,
            enable_slack: true,
            betweenness_max_nodes: 10_000,
            eigenvector_max_nodes: 10_000,
        }
    }

    /// Adaptive config based on graph size.
    #[must_use]
    pub const fn for_size(node_count: usize) -> Self {
        Self {
            enable_pagerank: true,
            enable_betweenness: node_count <= 10_000,
            enable_eigenvector: node_count <= 10_000,
            enable_hits: node_count <= 50_000,
            enable_cycles: true,
            enable_critical_path: true,
            enable_k_core: true,
            enable_articulation: true,
            enable_slack: true,
            betweenness_max_nodes: 10_000,
            eigenvector_max_nodes: 10_000,
        }
    }

    /// Minimal config for triage scoring (only PageRank + betweenness + basic).
    #[must_use]
    pub const fn triage_only() -> Self {
        Self {
            enable_pagerank: true,
            enable_betweenness: true,
            enable_eigenvector: false,
            enable_hits: false,
            enable_cycles: true,
            enable_critical_path: true,
            enable_k_core: false,
            enable_articulation: false,
            enable_slack: false,
            betweenness_max_nodes: 10_000,
            eigenvector_max_nodes: 10_000,
        }
    }
}

/// Record of a metric that was skipped during analysis.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedMetric {
    pub metric: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct IssueGraph {
    graph: DiGraph<(), ()>,
    node_to_id: Vec<String>,
    issues: HashMap<String, Issue>,
    blockers_by_issue: HashMap<String, Vec<String>>,
    dependents_by_issue: HashMap<String, Vec<String>>,
    successors: Vec<Vec<usize>>,   // issue -> blockers
    predecessors: Vec<Vec<usize>>, // issue <- dependents
    edge_count: usize,
}

#[derive(Debug, Clone)]
pub struct GraphMetrics {
    pub pagerank: HashMap<String, f64>,
    pub betweenness: HashMap<String, f64>,
    pub eigenvector: HashMap<String, f64>,
    pub hubs: HashMap<String, f64>,
    pub authorities: HashMap<String, f64>,
    pub blocks_count: HashMap<String, usize>,
    pub blocked_by_count: HashMap<String, usize>,
    pub critical_depth: HashMap<String, usize>,
    pub k_core: HashMap<String, u32>,
    pub articulation_points: HashSet<String>,
    pub slack: HashMap<String, f64>,
    pub cycles: Vec<Vec<String>>,
    pub skipped_metrics: Vec<SkippedMetric>,
    pub config: AnalysisConfig,
}

struct BetweennessScratch {
    stack: Vec<usize>,
    pred: Vec<Vec<usize>>,
    sigma: Vec<f64>,
    dist: Vec<i32>,
    delta: Vec<f64>,
    queue: Vec<usize>,
}

impl BetweennessScratch {
    fn new(node_count: usize) -> Self {
        Self {
            stack: Vec::with_capacity(node_count),
            pred: (0..node_count)
                .map(|_| Vec::with_capacity(4))
                .collect::<Vec<_>>(),
            sigma: vec![0.0; node_count],
            dist: vec![-1; node_count],
            delta: vec![0.0; node_count],
            queue: Vec::with_capacity(node_count),
        }
    }

    fn reset(&mut self) {
        self.stack.clear();
        self.queue.clear();
        self.sigma.fill(0.0);
        self.dist.fill(-1);
        self.delta.fill(0.0);
        for preds in &mut self.pred {
            preds.clear();
        }
    }
}

impl IssueGraph {
    #[must_use]
    pub fn build(issues: &[Issue]) -> Self {
        let mut graph = DiGraph::<(), ()>::new();
        let mut node_indices = Vec::with_capacity(issues.len());
        let mut node_to_id = Vec::with_capacity(issues.len());
        let mut id_to_index = HashMap::with_capacity(issues.len());
        let mut issue_map = HashMap::with_capacity(issues.len());

        for (index, issue) in issues.iter().enumerate() {
            let node = graph.add_node(());
            node_indices.push(node);
            node_to_id.push(issue.id.clone());
            id_to_index.insert(issue.id.clone(), index);
            issue_map.insert(issue.id.clone(), issue.clone());
        }

        let mut blockers_by_issue: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependents_by_issue: HashMap<String, Vec<String>> = HashMap::new();
        let mut successors = vec![Vec::<usize>::new(); issues.len()];
        let mut predecessors = vec![Vec::<usize>::new(); issues.len()];
        let mut seen = HashSet::<(usize, usize)>::new();
        let mut edge_count = 0usize;

        for issue in issues {
            let Some(&source_index) = id_to_index.get(&issue.id) else {
                continue;
            };

            for dep in &issue.dependencies {
                if !dep.is_blocking() || dep.depends_on_id.trim().is_empty() {
                    continue;
                }

                let Some(&target_index) = id_to_index.get(&dep.depends_on_id) else {
                    continue;
                };

                if !seen.insert((source_index, target_index)) {
                    continue;
                }

                graph.add_edge(node_indices[source_index], node_indices[target_index], ());
                successors[source_index].push(target_index);
                predecessors[target_index].push(source_index);

                blockers_by_issue
                    .entry(issue.id.clone())
                    .or_default()
                    .push(dep.depends_on_id.clone());
                dependents_by_issue
                    .entry(dep.depends_on_id.clone())
                    .or_default()
                    .push(issue.id.clone());

                edge_count = edge_count.saturating_add(1);
            }
        }

        for neighbors in &mut successors {
            neighbors.sort_unstable();
        }
        for neighbors in &mut predecessors {
            neighbors.sort_unstable();
        }
        for blockers in blockers_by_issue.values_mut() {
            blockers.sort();
            blockers.dedup();
        }
        for dependents in dependents_by_issue.values_mut() {
            dependents.sort();
            dependents.dedup();
        }

        Self {
            graph,
            node_to_id,
            issues: issue_map,
            blockers_by_issue,
            dependents_by_issue,
            successors,
            predecessors,
            edge_count,
        }
    }

    #[must_use]
    pub fn issue(&self, id: &str) -> Option<&Issue> {
        self.issues.get(id)
    }

    #[must_use]
    pub fn issue_ids_sorted(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.issues.keys().cloned().collect();
        ids.sort();
        ids
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.node_to_id.len()
    }

    #[must_use]
    pub const fn edge_count(&self) -> usize {
        self.edge_count
    }

    #[must_use]
    pub fn blockers(&self, issue_id: &str) -> Vec<String> {
        self.blockers_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn dependents(&self, issue_id: &str) -> Vec<String> {
        self.dependents_by_issue
            .get(issue_id)
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn open_blockers(&self, issue_id: &str) -> Vec<String> {
        self.blockers(issue_id)
            .into_iter()
            .filter(|blocker_id| self.issues.get(blocker_id).is_some_and(Issue::is_open_like))
            .collect()
    }

    #[must_use]
    pub fn actionable_ids(&self) -> Vec<String> {
        // Phase 1: Compute directly blocked issues (open blocking dependencies).
        let mut directly_blocked = HashSet::<String>::new();
        for (id, issue) in &self.issues {
            if issue.is_closed_like() {
                continue;
            }
            if !self.open_blockers(id).is_empty() {
                directly_blocked.insert(id.clone());
            }
        }

        // Phase 2: Build parent->children index from parent-child dependencies.
        // A child has a dep with dep_type="parent-child" and depends_on_id pointing
        // to the parent. We invert: parent -> [children].
        let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
        for issue in self.issues.values() {
            for dep in &issue.dependencies {
                if dep.is_parent_child()
                    && !dep.depends_on_id.trim().is_empty()
                    && self.issues.contains_key(&dep.depends_on_id)
                {
                    children_of
                        .entry(dep.depends_on_id.clone())
                        .or_default()
                        .push(issue.id.clone());
                }
            }
        }

        // Phase 3: Propagate blocked status through parent-child relationships.
        // If a parent is blocked, its children are also blocked (transitively).
        let mut blocked = directly_blocked.clone();
        let max_depth = 50;
        for _ in 0..max_depth {
            let mut newly_blocked = Vec::<String>::new();
            for parent_id in &blocked {
                if let Some(children) = children_of.get(parent_id) {
                    for child_id in children {
                        if !blocked.contains(child_id)
                            && self
                                .issues
                                .get(child_id)
                                .is_some_and(|issue| !issue.is_closed_like())
                        {
                            newly_blocked.push(child_id.clone());
                        }
                    }
                }
            }
            if newly_blocked.is_empty() {
                break;
            }
            for id in newly_blocked {
                blocked.insert(id);
            }
        }

        // Phase 4: Collect actionable issues (open, not blocked).
        let mut ids = self.issue_ids_sorted();
        ids.retain(|id| {
            self.issues.get(id).is_some_and(Issue::is_open_like) && !blocked.contains(id)
        });
        ids
    }

    #[must_use]
    pub fn connected_open_components(&self) -> Vec<Vec<String>> {
        let open_ids: HashSet<String> = self
            .issues
            .values()
            .filter(|issue| issue.is_open_like())
            .map(|issue| issue.id.clone())
            .collect();

        let mut seen = HashSet::<String>::new();
        let mut components = Vec::<Vec<String>>::new();

        for start_id in &open_ids {
            if seen.contains(start_id) {
                continue;
            }

            let mut stack = vec![start_id.clone()];
            let mut component = Vec::<String>::new();
            seen.insert(start_id.clone());

            while let Some(id) = stack.pop() {
                component.push(id.clone());

                let neighbors = self
                    .blockers(&id)
                    .into_iter()
                    .chain(self.dependents(&id).into_iter());

                for neighbor in neighbors {
                    if !open_ids.contains(&neighbor) {
                        continue;
                    }
                    if seen.insert(neighbor.clone()) {
                        stack.push(neighbor);
                    }
                }
            }

            component.sort();
            components.push(component);
        }

        components.sort_by(|a, b| a.first().cmp(&b.first()));
        components
    }

    /// Compute all metrics using the default (full) config.
    #[must_use]
    pub fn compute_metrics(&self) -> GraphMetrics {
        self.compute_metrics_with_config(&AnalysisConfig::default())
    }

    /// Compute metrics respecting the provided configuration.
    #[must_use]
    pub fn compute_metrics_with_config(&self, config: &AnalysisConfig) -> GraphMetrics {
        let n = self.node_count();
        let mut skipped = Vec::<SkippedMetric>::new();

        let pagerank = if config.enable_pagerank {
            self.compute_pagerank()
        } else {
            skipped.push(SkippedMetric {
                metric: "PageRank",
                reason: "disabled by config".to_string(),
            });
            HashMap::new()
        };

        let betweenness = if config.enable_betweenness && n <= config.betweenness_max_nodes {
            self.compute_betweenness()
        } else {
            let reason = if !config.enable_betweenness {
                "disabled by config".to_string()
            } else {
                format!(
                    "graph too large ({n} nodes > {} max)",
                    config.betweenness_max_nodes
                )
            };
            skipped.push(SkippedMetric {
                metric: "Betweenness",
                reason,
            });
            HashMap::new()
        };

        let eigenvector = if config.enable_eigenvector && n <= config.eigenvector_max_nodes {
            self.compute_eigenvector()
        } else {
            let reason = if !config.enable_eigenvector {
                "disabled by config".to_string()
            } else {
                format!(
                    "graph too large ({n} nodes > {} max)",
                    config.eigenvector_max_nodes
                )
            };
            skipped.push(SkippedMetric {
                metric: "Eigenvector",
                reason,
            });
            HashMap::new()
        };

        let (hubs, authorities) = if config.enable_hits {
            self.compute_hits()
        } else {
            skipped.push(SkippedMetric {
                metric: "HITS",
                reason: "disabled by config".to_string(),
            });
            (HashMap::new(), HashMap::new())
        };

        // blocks_count and blocked_by_count are always computed (cheap: O(V)).
        let mut blocks_count = HashMap::new();
        let mut blocked_by_count = HashMap::new();
        for id in self.issue_ids_sorted() {
            blocks_count.insert(id.clone(), self.dependents(&id).len());
            blocked_by_count.insert(id.clone(), self.blockers(&id).len());
        }

        let critical_depth = if config.enable_critical_path {
            self.compute_critical_depth()
        } else {
            skipped.push(SkippedMetric {
                metric: "CriticalPath",
                reason: "disabled by config".to_string(),
            });
            HashMap::new()
        };

        let k_core = if config.enable_k_core {
            self.compute_k_core()
        } else {
            skipped.push(SkippedMetric {
                metric: "KCore",
                reason: "disabled by config".to_string(),
            });
            HashMap::new()
        };

        let articulation_points = if config.enable_articulation {
            self.compute_articulation_points()
        } else {
            skipped.push(SkippedMetric {
                metric: "Articulation",
                reason: "disabled by config".to_string(),
            });
            HashSet::new()
        };

        let slack = if config.enable_slack {
            self.compute_slack()
        } else {
            skipped.push(SkippedMetric {
                metric: "Slack",
                reason: "disabled by config".to_string(),
            });
            HashMap::new()
        };

        let cycles = if config.enable_cycles {
            self.find_cycles()
        } else {
            skipped.push(SkippedMetric {
                metric: "Cycles",
                reason: "disabled by config".to_string(),
            });
            Vec::new()
        };

        GraphMetrics {
            pagerank,
            betweenness,
            eigenvector,
            hubs,
            authorities,
            blocks_count,
            blocked_by_count,
            critical_depth,
            k_core,
            articulation_points,
            slack,
            cycles,
            skipped_metrics: skipped,
            config: config.clone(),
        }
    }

    fn compute_pagerank(&self) -> HashMap<String, f64> {
        let node_count = self.node_to_id.len();
        if node_count == 0 {
            return HashMap::new();
        }

        let damping = 0.85_f64;
        let base = (1.0_f64 - damping) / node_count as f64;
        let mut ranks = vec![1.0_f64 / node_count as f64; node_count];

        for _ in 0..100 {
            let mut next = vec![base; node_count];

            let dangling_sum = (0..node_count)
                .filter(|&node| self.successors[node].is_empty())
                .map(|node| ranks[node])
                .sum::<f64>();
            let dangling_contrib = damping * dangling_sum / node_count as f64;
            for value in &mut next {
                *value += dangling_contrib;
            }

            for (node, rank) in ranks.iter().enumerate().take(node_count) {
                let out_degree = self.successors[node].len();
                if out_degree == 0 {
                    continue;
                }

                let share = *rank / out_degree as f64;
                for &target in &self.successors[node] {
                    next[target] += damping * share;
                }
            }

            let delta = ranks
                .iter()
                .zip(next.iter())
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>();

            ranks = next;
            if delta < 1e-9 {
                break;
            }
        }

        self.map_from_f64_scores(&ranks)
    }

    fn compute_betweenness(&self) -> HashMap<String, f64> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        if n > 512 {
            let sample_size = (n / 5).clamp(128, 512);
            return self.compute_betweenness_sampled(sample_size);
        }

        let mut bc = vec![0.0_f64; n];
        let mut scratch = BetweennessScratch::new(n);
        for source in 0..n {
            self.single_source_betweenness(source, &mut bc, &mut scratch);
        }

        self.map_from_f64_scores(&bc)
    }

    fn compute_betweenness_sampled(&self, sample_size: usize) -> HashMap<String, f64> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        let pivot_count = sample_size.min(n);
        let mut pivots = Vec::<usize>::with_capacity(pivot_count);
        let mut used = HashSet::<usize>::with_capacity(pivot_count);
        let step = (n / pivot_count.max(1)).max(1);

        for i in 0..pivot_count {
            let mut candidate = (i * step) % n;
            while !used.insert(candidate) {
                candidate = (candidate + 1) % n;
            }
            pivots.push(candidate);
        }

        let mut bc = vec![0.0_f64; n];
        let mut scratch = BetweennessScratch::new(n);
        for pivot in pivots {
            self.single_source_betweenness(pivot, &mut bc, &mut scratch);
        }

        let scale = n as f64 / pivot_count.max(1) as f64;
        for value in &mut bc {
            *value *= scale;
        }

        self.map_from_f64_scores(&bc)
    }

    fn single_source_betweenness(
        &self,
        source: usize,
        bc: &mut [f64],
        scratch: &mut BetweennessScratch,
    ) {
        scratch.reset();
        scratch.sigma[source] = 1.0;
        scratch.dist[source] = 0;
        scratch.queue.push(source);

        let mut queue_head = 0usize;
        while queue_head < scratch.queue.len() {
            let v = scratch.queue[queue_head];
            queue_head += 1;
            scratch.stack.push(v);

            for &w in &self.successors[v] {
                if scratch.dist[w] < 0 {
                    scratch.dist[w] = scratch.dist[v] + 1;
                    scratch.queue.push(w);
                }

                if scratch.dist[w] == scratch.dist[v] + 1 {
                    scratch.sigma[w] += scratch.sigma[v];
                    scratch.pred[w].push(v);
                }
            }
        }

        while let Some(w) = scratch.stack.pop() {
            let sigma_w = scratch.sigma[w];
            let delta_w = scratch.delta[w];
            for &v in &scratch.pred[w] {
                if sigma_w > 0.0 {
                    scratch.delta[v] += (scratch.sigma[v] / sigma_w) * (1.0 + delta_w);
                }
            }

            if w != source {
                bc[w] += scratch.delta[w];
            }
        }
    }

    fn compute_eigenvector(&self) -> HashMap<String, f64> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        let init = 1.0 / (n as f64).sqrt();
        let mut current = vec![init; n];
        let mut next = vec![0.0_f64; n];

        for _ in 0..80 {
            next.fill(0.0);

            for (node, target) in next.iter_mut().enumerate() {
                for &pred in &self.predecessors[node] {
                    *target += current[pred];
                }
            }

            let norm = next.iter().map(|value| value * value).sum::<f64>().sqrt();
            if norm < 1e-12 {
                break;
            }
            for value in &mut next {
                *value /= norm;
            }

            let delta = current
                .iter()
                .zip(next.iter())
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>();

            current.clone_from_slice(&next);
            if delta < 1e-7 {
                break;
            }
        }

        self.map_from_f64_scores(&current)
    }

    fn compute_hits(&self) -> (HashMap<String, f64>, HashMap<String, f64>) {
        let n = self.node_to_id.len();
        if n == 0 {
            return (HashMap::new(), HashMap::new());
        }

        let mut hubs = vec![1.0 / n as f64; n];
        let mut authorities = vec![1.0 / n as f64; n];

        for _ in 0..100 {
            let mut next_auth = vec![0.0_f64; n];
            let mut next_hubs = vec![0.0_f64; n];

            for (node, target) in next_auth.iter_mut().enumerate() {
                for &pred in &self.predecessors[node] {
                    *target += hubs[pred];
                }
            }

            for (node, target) in next_hubs.iter_mut().enumerate() {
                for &succ in &self.successors[node] {
                    *target += next_auth[succ];
                }
            }

            normalize_l2(&mut next_auth);
            normalize_l2(&mut next_hubs);

            let auth_delta = authorities
                .iter()
                .zip(next_auth.iter())
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>();
            let hubs_delta = hubs
                .iter()
                .zip(next_hubs.iter())
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>();

            authorities = next_auth;
            hubs = next_hubs;

            if auth_delta + hubs_delta < 1e-7 {
                break;
            }
        }

        (
            self.map_from_f64_scores(&hubs),
            self.map_from_f64_scores(&authorities),
        )
    }

    fn compute_critical_depth(&self) -> HashMap<String, usize> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        let mut heights = vec![0usize; n];
        if let Some(order) = self.topological_order() {
            for node in order {
                let max_pred = self.predecessors[node]
                    .iter()
                    .map(|&pred| heights[pred])
                    .max()
                    .unwrap_or(0);
                heights[node] = max_pred.saturating_add(1);
            }
        }

        self.map_from_usize_scores(&heights)
    }

    fn compute_slack(&self) -> HashMap<String, f64> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        let Some(order) = self.topological_order() else {
            return self.map_from_f64_scores(&vec![0.0; n]);
        };

        let mut dist_from_start = vec![0usize; n];
        for &node in &order {
            let max_pred = self.predecessors[node]
                .iter()
                .map(|&pred| dist_from_start[pred])
                .max()
                .unwrap_or(0);
            dist_from_start[node] = max_pred.saturating_add(1);
        }

        let mut dist_to_end = vec![0usize; n];
        for &node in order.iter().rev() {
            let max_succ = self.successors[node]
                .iter()
                .map(|&succ| dist_to_end[succ])
                .max()
                .unwrap_or(0);
            dist_to_end[node] = max_succ.saturating_add(1);
        }

        let longest_path = (0..n)
            .map(|index| dist_from_start[index] + dist_to_end[index] - 1)
            .max()
            .unwrap_or(0);

        let slack = (0..n)
            .map(|index| {
                let path_through_node = dist_from_start[index] + dist_to_end[index] - 1;
                longest_path.saturating_sub(path_through_node) as f64
            })
            .collect::<Vec<_>>();

        self.map_from_f64_scores(&slack)
    }

    fn compute_k_core(&self) -> HashMap<String, u32> {
        let n = self.node_to_id.len();
        if n == 0 {
            return HashMap::new();
        }

        let neighbors = self.undirected_neighbors();
        let mut degree = neighbors.iter().map(HashSet::len).collect::<Vec<_>>();
        let mut removed = vec![false; n];
        let mut core = vec![0u32; n];

        let mut heap = BinaryHeap::<Reverse<(usize, usize)>>::new();
        for (index, &deg) in degree.iter().enumerate() {
            heap.push(Reverse((deg, index)));
        }

        let mut current_core = 0usize;

        while let Some(Reverse((deg, node))) = heap.pop() {
            if removed[node] || deg != degree[node] {
                continue;
            }

            removed[node] = true;
            current_core = current_core.max(deg);
            core[node] = u32::try_from(current_core).unwrap_or(u32::MAX);

            for &neighbor in &neighbors[node] {
                if removed[neighbor] {
                    continue;
                }

                degree[neighbor] = degree[neighbor].saturating_sub(1);
                heap.push(Reverse((degree[neighbor], neighbor)));
            }
        }

        self.map_from_u32_scores(&core)
    }

    fn compute_articulation_points(&self) -> HashSet<String> {
        let n = self.node_to_id.len();
        if n <= 2 {
            return HashSet::new();
        }

        let neighbors = self
            .undirected_neighbors()
            .into_iter()
            .map(|set| {
                let mut values = set.into_iter().collect::<Vec<_>>();
                values.sort_unstable();
                values
            })
            .collect::<Vec<_>>();

        let mut disc = vec![0usize; n];
        let mut low = vec![0usize; n];
        let mut parent = vec![usize::MAX; n];
        let mut visited = vec![false; n];
        let mut is_ap = vec![false; n];
        let mut time = 0usize;

        for node in 0..n {
            if !visited[node] {
                tarjan_articulation_dfs(
                    node,
                    &neighbors,
                    &mut disc,
                    &mut low,
                    &mut parent,
                    &mut visited,
                    &mut is_ap,
                    &mut time,
                );
            }
        }

        is_ap
            .iter()
            .enumerate()
            .filter_map(|(index, &value)| {
                if value {
                    Some(self.node_to_id[index].clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();

        for component in kosaraju_scc(&self.graph) {
            if component.len() > 1 {
                // Report all SCC members (matches Go behavior which reports
                // full strongly-connected components, not minimal cycle paths)
                let mut ids: Vec<String> = component
                    .iter()
                    .map(|node| self.node_to_id[node.index()].clone())
                    .collect();
                ids.sort();
                cycles.push(ids);
                continue;
            }

            let node = component[0];
            let has_self_loop = self
                .graph
                .edges(node)
                .any(|edge| edge.target().index() == node.index());
            if has_self_loop {
                cycles.push(vec![self.node_to_id[node.index()].clone()]);
            }
        }

        cycles.sort_by(|a, b| a.first().cmp(&b.first()));
        cycles
    }

    fn topological_order(&self) -> Option<Vec<usize>> {
        let n = self.node_to_id.len();
        if n == 0 {
            return Some(Vec::new());
        }

        let mut in_degree = self.predecessors.iter().map(Vec::len).collect::<Vec<_>>();
        let mut heap = BinaryHeap::<Reverse<usize>>::new();

        for (node, &degree) in in_degree.iter().enumerate() {
            if degree == 0 {
                heap.push(Reverse(node));
            }
        }

        let mut order = Vec::with_capacity(n);
        while let Some(Reverse(node)) = heap.pop() {
            order.push(node);

            for &succ in &self.successors[node] {
                in_degree[succ] = in_degree[succ].saturating_sub(1);
                if in_degree[succ] == 0 {
                    heap.push(Reverse(succ));
                }
            }
        }

        if order.len() == n { Some(order) } else { None }
    }

    fn undirected_neighbors(&self) -> Vec<HashSet<usize>> {
        let n = self.node_to_id.len();
        let mut neighbors = vec![HashSet::<usize>::new(); n];

        for node in 0..n {
            for &succ in &self.successors[node] {
                if node == succ {
                    continue;
                }
                neighbors[node].insert(succ);
                neighbors[succ].insert(node);
            }
        }

        neighbors
    }

    fn map_from_f64_scores(&self, scores: &[f64]) -> HashMap<String, f64> {
        let mut map = HashMap::with_capacity(scores.len());
        for (index, value) in scores.iter().enumerate() {
            map.insert(self.node_to_id[index].clone(), *value);
        }
        map
    }

    fn map_from_usize_scores(&self, scores: &[usize]) -> HashMap<String, usize> {
        let mut map = HashMap::with_capacity(scores.len());
        for (index, value) in scores.iter().enumerate() {
            map.insert(self.node_to_id[index].clone(), *value);
        }
        map
    }

    fn map_from_u32_scores(&self, scores: &[u32]) -> HashMap<String, u32> {
        let mut map = HashMap::with_capacity(scores.len());
        for (index, value) in scores.iter().enumerate() {
            map.insert(self.node_to_id[index].clone(), *value);
        }
        map
    }
}

fn normalize_l2(values: &mut [f64]) {
    let norm = values.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > 0.0 {
        for value in values {
            *value /= norm;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tarjan_articulation_dfs(
    node: usize,
    neighbors: &[Vec<usize>],
    disc: &mut [usize],
    low: &mut [usize],
    parent: &mut [usize],
    visited: &mut [bool],
    is_ap: &mut [bool],
    time: &mut usize,
) {
    visited[node] = true;
    *time = time.saturating_add(1);
    disc[node] = *time;
    low[node] = *time;
    let mut children = 0usize;

    for &next in &neighbors[node] {
        if !visited[next] {
            children = children.saturating_add(1);
            parent[next] = node;

            tarjan_articulation_dfs(next, neighbors, disc, low, parent, visited, is_ap, time);

            low[node] = low[node].min(low[next]);

            if parent[node] == usize::MAX && children > 1 {
                is_ap[node] = true;
            }
            if parent[node] != usize::MAX && low[next] >= disc[node] {
                is_ap[node] = true;
            }
        } else if next != parent[node] {
            low[node] = low[node].min(disc[next]);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Dependency, Issue};

    use super::{AnalysisConfig, IssueGraph};

    #[test]
    fn critical_depth_matches_dependency_direction() {
        let issues = vec![
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
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![Dependency {
                    issue_id: "B".to_string(),
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        assert_eq!(metrics.critical_depth.get("A"), Some(&2));
        assert_eq!(metrics.critical_depth.get("B"), Some(&1));
        assert_eq!(metrics.slack.get("A"), Some(&0.0));
        assert_eq!(metrics.slack.get("B"), Some(&0.0));
    }

    #[test]
    fn articulation_detects_cut_vertex() {
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
                dependencies: vec![Dependency {
                    issue_id: "B".to_string(),
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "C".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![Dependency {
                    issue_id: "C".to_string(),
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        assert!(metrics.articulation_points.contains("A"));
    }

    #[test]
    fn betweenness_finds_middle_node_in_chain() {
        let issues = vec![
            Issue {
                id: "A".to_string(),
                title: "A".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![Dependency {
                    issue_id: "A".to_string(),
                    depends_on_id: "B".to_string(),
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
                dependencies: vec![Dependency {
                    issue_id: "B".to_string(),
                    depends_on_id: "C".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "C".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();

        let a = metrics.betweenness.get("A").copied().unwrap_or_default();
        let b = metrics.betweenness.get("B").copied().unwrap_or_default();
        let c = metrics.betweenness.get("C").copied().unwrap_or_default();

        assert!(b > a);
        assert!(b > c);
    }

    #[test]
    fn connected_open_components_group_blocker_cluster() {
        let issues = vec![
            Issue {
                id: "bd-3q0".to_string(),
                title: "Primary blocker".to_string(),
                status: "in_progress".to_string(),
                issue_type: "feature".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "bd-3q1".to_string(),
                title: "Blocked follow-on".to_string(),
                status: "blocked".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![Dependency {
                    issue_id: "bd-3q1".to_string(),
                    depends_on_id: "bd-3q0".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "bd-3q2".to_string(),
                title: "Independent slice".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 3,
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let components = graph.connected_open_components();
        assert_eq!(
            components,
            vec![
                vec!["bd-3q0".to_string(), "bd-3q1".to_string()],
                vec!["bd-3q2".to_string()],
            ]
        );

        let metrics = graph.compute_metrics();
        assert_eq!(metrics.blocks_count.get("bd-3q0"), Some(&1));
        assert!(metrics.cycles.is_empty());
    }

    #[test]
    fn actionable_excludes_children_of_blocked_parent_epic() {
        // Parent epic E is blocked by blocker B.
        // Child task C has a parent-child dep on E.
        // C should NOT be actionable because its parent E is blocked.
        let issues = vec![
            Issue {
                id: "B".to_string(),
                title: "Blocker".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "E".to_string(),
                title: "Epic".to_string(),
                status: "blocked".to_string(),
                issue_type: "epic".to_string(),
                priority: 2,
                dependencies: vec![Dependency {
                    issue_id: "E".to_string(),
                    depends_on_id: "B".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "Child task".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 3,
                dependencies: vec![Dependency {
                    issue_id: "C".to_string(),
                    depends_on_id: "E".to_string(),
                    dep_type: "parent-child".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let actionable = graph.actionable_ids();

        // Only B should be actionable (it has no blockers).
        // E is blocked by B, and C is blocked transitively via parent E.
        assert_eq!(actionable, vec!["B".to_string()]);
    }

    #[test]
    fn actionable_includes_children_of_unblocked_parent() {
        // Parent epic E has no blockers.
        // Child task C has a parent-child dep on E.
        // Both should be actionable.
        let issues = vec![
            Issue {
                id: "E".to_string(),
                title: "Epic".to_string(),
                status: "open".to_string(),
                issue_type: "epic".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "C".to_string(),
                title: "Child task".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![Dependency {
                    issue_id: "C".to_string(),
                    depends_on_id: "E".to_string(),
                    dep_type: "parent-child".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let actionable = graph.actionable_ids();

        assert_eq!(actionable, vec!["C".to_string(), "E".to_string()]);
    }

    #[test]
    fn actionable_handles_mixed_prefix_datasets() {
        // Dataset with mixed prefixes (bd- and bv- style IDs).
        // This tests graceful handling of mixed-prefix datasets.
        let issues = vec![
            Issue {
                id: "bd-100".to_string(),
                title: "Beads style".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 1,
                ..Issue::default()
            },
            Issue {
                id: "bv-200".to_string(),
                title: "Viewer style".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 2,
                dependencies: vec![Dependency {
                    issue_id: "bv-200".to_string(),
                    depends_on_id: "bd-100".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "gh-300".to_string(),
                title: "GitHub style".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                priority: 3,
                ..Issue::default()
            },
        ];

        let graph = IssueGraph::build(&issues);
        let actionable = graph.actionable_ids();

        // bd-100 is actionable (no blockers), gh-300 is actionable
        // bv-200 is blocked by bd-100
        assert_eq!(actionable, vec!["bd-100".to_string(), "gh-300".to_string()]);
    }

    #[test]
    fn empty_graph_produces_empty_metrics() {
        let graph = IssueGraph::build(&[]);
        let metrics = graph.compute_metrics();
        assert!(metrics.pagerank.is_empty());
        assert!(metrics.betweenness.is_empty());
        assert!(metrics.cycles.is_empty());
        assert!(metrics.articulation_points.is_empty());
        assert_eq!(graph.actionable_ids().len(), 0);
    }

    #[test]
    fn single_node_graph() {
        let issues = vec![Issue {
            id: "SOLO".to_string(),
            title: "Alone".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        assert!(metrics.pagerank.contains_key("SOLO"));
        assert!(metrics.cycles.is_empty());
        assert_eq!(graph.actionable_ids(), vec!["SOLO".to_string()]);
        assert!(graph.open_blockers("SOLO").is_empty());
    }

    #[test]
    fn cycle_detected_in_mutual_dependency() {
        let issues = vec![
            Issue {
                id: "X".to_string(),
                title: "X".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![Dependency {
                    issue_id: "X".to_string(),
                    depends_on_id: "Y".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
            Issue {
                id: "Y".to_string(),
                title: "Y".to_string(),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                dependencies: vec![Dependency {
                    issue_id: "Y".to_string(),
                    depends_on_id: "X".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        assert!(
            !metrics.cycles.is_empty(),
            "mutual dependency should form a cycle"
        );
    }

    #[test]
    fn closed_issues_not_actionable() {
        let issues = vec![Issue {
            id: "DONE".to_string(),
            title: "Done".to_string(),
            status: "closed".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        assert!(graph.actionable_ids().is_empty());
    }

    #[test]
    fn pagerank_sums_near_one() {
        let issues: Vec<Issue> = (0..5)
            .map(|i| Issue {
                id: format!("N-{i}"),
                title: format!("Node {i}"),
                status: "open".to_string(),
                issue_type: "task".to_string(),
                ..Issue::default()
            })
            .collect();
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        let total: f64 = metrics.pagerank.values().sum();
        assert!(
            (total - 1.0).abs() < 0.1,
            "PageRank should sum near 1.0, got {total}"
        );
    }

    // -- AnalysisConfig tests --

    #[test]
    fn default_config_computes_all_metrics() {
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
                dependencies: vec![Dependency {
                    depends_on_id: "A".to_string(),
                    dep_type: "blocks".to_string(),
                    ..Dependency::default()
                }],
                ..Issue::default()
            },
        ];
        let graph = IssueGraph::build(&issues);
        let metrics = graph.compute_metrics();
        assert!(metrics.skipped_metrics.is_empty());
        assert!(!metrics.pagerank.is_empty());
        assert!(!metrics.betweenness.is_empty());
    }

    #[test]
    fn triage_config_skips_non_essential_metrics() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let config = AnalysisConfig::triage_only();
        let metrics = graph.compute_metrics_with_config(&config);

        // PageRank and betweenness should be computed.
        assert!(!metrics.pagerank.is_empty());
        assert!(!metrics.betweenness.is_empty());

        // Eigenvector, HITS, KCore, Articulation, Slack should be skipped.
        let skipped_names: Vec<&str> = metrics.skipped_metrics.iter().map(|s| s.metric).collect();
        assert!(skipped_names.contains(&"Eigenvector"));
        assert!(skipped_names.contains(&"HITS"));
        assert!(skipped_names.contains(&"KCore"));
        assert!(skipped_names.contains(&"Articulation"));
        assert!(skipped_names.contains(&"Slack"));
        assert!(metrics.eigenvector.is_empty());
        assert!(metrics.hubs.is_empty());
    }

    #[test]
    fn config_disables_individual_metrics() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let mut config = AnalysisConfig::full();
        config.enable_pagerank = false;
        config.enable_cycles = false;

        let metrics = graph.compute_metrics_with_config(&config);
        assert!(metrics.pagerank.is_empty());
        assert!(metrics.cycles.is_empty());
        let skipped_names: Vec<&str> = metrics.skipped_metrics.iter().map(|s| s.metric).collect();
        assert!(skipped_names.contains(&"PageRank"));
        assert!(skipped_names.contains(&"Cycles"));
        // Other metrics still computed.
        assert!(!metrics.betweenness.is_empty());
    }

    #[test]
    fn config_size_threshold_skips_betweenness() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);

        // Set threshold to 0 so even 1 node exceeds it.
        let mut config = AnalysisConfig::full();
        config.betweenness_max_nodes = 0;

        let metrics = graph.compute_metrics_with_config(&config);
        assert!(metrics.betweenness.is_empty());
        let bt_skip = metrics
            .skipped_metrics
            .iter()
            .find(|s| s.metric == "Betweenness");
        assert!(bt_skip.is_some());
        assert!(bt_skip.unwrap().reason.contains("too large"));
    }

    #[test]
    fn config_for_size_adapts_to_graph() {
        let small = AnalysisConfig::for_size(100);
        assert!(small.enable_betweenness);
        assert!(small.enable_eigenvector);
        assert!(small.enable_hits);

        let large = AnalysisConfig::for_size(50_001);
        assert!(!large.enable_betweenness);
        assert!(!large.enable_eigenvector);
        assert!(!large.enable_hits);
    }

    #[test]
    fn config_serializes_to_json() {
        let config = AnalysisConfig::full();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enable_pagerank\":true"));
        assert!(json.contains("\"betweenness_max_nodes\":10000"));
    }

    #[test]
    fn config_deserializes_from_json() {
        let json = r#"{
            "enable_pagerank": false,
            "enable_betweenness": true,
            "enable_eigenvector": true,
            "enable_hits": true,
            "enable_cycles": true,
            "enable_critical_path": true,
            "enable_k_core": true,
            "enable_articulation": true,
            "enable_slack": true,
            "betweenness_max_nodes": 5000,
            "eigenvector_max_nodes": 5000
        }"#;
        let config: AnalysisConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enable_pagerank);
        assert_eq!(config.betweenness_max_nodes, 5000);
    }

    #[test]
    fn metrics_config_field_matches_input() {
        let issues = vec![Issue {
            id: "A".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            issue_type: "task".to_string(),
            ..Issue::default()
        }];
        let graph = IssueGraph::build(&issues);
        let config = AnalysisConfig::triage_only();
        let metrics = graph.compute_metrics_with_config(&config);
        assert!(!metrics.config.enable_eigenvector);
        assert!(metrics.config.enable_pagerank);
    }
}
