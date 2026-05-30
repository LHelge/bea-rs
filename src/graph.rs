use crate::task::{self, Priority, Status, Task};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Dependency graph built from task `depends_on` fields.
pub struct Graph {
    /// task_id -> set of task IDs it depends on
    pub edges: HashMap<String, HashSet<String>>,
    /// task_id -> set of task IDs that depend on it (reverse edges)
    pub reverse: HashMap<String, HashSet<String>>,
}

impl Graph {
    /// Build a dependency graph from a set of tasks.
    pub fn build(tasks: &HashMap<String, Task>) -> Self {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut reverse: HashMap<String, HashSet<String>> = HashMap::new();

        for task in tasks.values() {
            edges.entry(task.id.clone()).or_default();
            reverse.entry(task.id.clone()).or_default();

            for dep in &task.depends_on {
                edges
                    .entry(task.id.clone())
                    .or_default()
                    .insert(dep.clone());
                reverse
                    .entry(dep.clone())
                    .or_default()
                    .insert(task.id.clone());
            }
        }

        Graph { edges, reverse }
    }

    /// Return tasks that are ready: status is Open and all dependencies are Done.
    ///
    /// Effective priorities are computed once in O(V+E) rather than per-task
    /// inside the sort comparator.
    pub fn ready<'a>(
        &self,
        tasks: &'a HashMap<String, Task>,
        tag: Option<&str>,
        limit: Option<usize>,
        epic: Option<&str>,
    ) -> Vec<&'a Task> {
        let eff = self.effective_priorities_all(tasks);

        let mut result: Vec<&Task> = tasks
            .values()
            .filter(|t| t.status == Status::Open)
            .filter(|t| t.task_type.is_task()) // epics are not directly workable
            .filter(|t| {
                // All dependencies must be done
                t.depends_on.iter().all(|dep_id| match tasks.get(dep_id) {
                    Some(dep) => dep.status == Status::Done,
                    None => false, // missing dep blocks readiness
                })
            })
            .filter(|t| task::matches_tag(t, tag))
            .filter(|t| epic.is_none_or(|e| t.parent.as_deref() == Some(e)))
            .collect();

        // Sort by effective priority (P0 first), then by creation date (oldest first).
        // The priority map was computed once above — no repeated BFS in the comparator.
        result.sort_by(|a, b| {
            eff.get(&a.id)
                .copied()
                .unwrap_or(a.priority)
                .cmp(&eff.get(&b.id).copied().unwrap_or(b.priority))
                .then(a.created.cmp(&b.created))
        });

        if let Some(limit) = limit {
            result.truncate(limit);
        }

        result
    }

    /// Compute the effective priority of a single task.
    /// This is the minimum (highest urgency) of the task's own priority and
    /// the priorities of all tasks that depend on it, transitively.
    ///
    /// For bulk computation prefer [`Graph::effective_priorities_all`] which
    /// runs in O(V+E) instead of O(V+E) per call.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn effective_priority(&self, id: &str, tasks: &HashMap<String, Task>) -> Priority {
        self.effective_priorities_all(tasks)
            .remove(id)
            .unwrap_or_else(|| tasks.get(id).map(|t| t.priority).unwrap_or(Priority::P3))
    }

    /// Compute effective priorities for ALL tasks in a single O(V+E) pass.
    ///
    /// `effective(x) = min(own(x), min over direct dependents y of effective(y))`
    ///
    /// We process nodes in reverse-topological order (dependents before their
    /// dependencies) so that by the time we visit a node its dependents are
    /// already resolved. Nodes that participate in a cycle are not reached by
    /// the topological pass and keep their own priority as a safe fallback.
    pub fn effective_priorities_all(
        &self,
        tasks: &HashMap<String, Task>,
    ) -> HashMap<String, Priority> {
        // --- Step 1: Kahn topological sort over *all* known nodes ---------------
        // We sort forward along `edges` (A depends-on B → edge A→B).
        // Collect every node id from both maps.
        let all_ids: HashSet<&str> = self
            .edges
            .keys()
            .chain(self.reverse.keys())
            .map(String::as_str)
            .collect();

        // in_degree counts how many dependencies each node has (intra-graph only).
        let mut in_degree: HashMap<&str, usize> = all_ids.iter().map(|&id| (id, 0)).collect();
        for id in &all_ids {
            if let Some(deps) = self.edges.get(*id) {
                for dep in deps {
                    if all_ids.contains(dep.as_str()) {
                        *in_degree.entry(id).or_default() += 1;
                    }
                }
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut topo_order: Vec<&str> = Vec::with_capacity(all_ids.len());
        while let Some(current) = queue.pop_front() {
            topo_order.push(current);
            if let Some(dependents) = self.reverse.get(current) {
                for dep in dependents {
                    if let Some(d) = in_degree.get_mut(dep.as_str()) {
                        *d -= 1;
                        if *d == 0 {
                            queue.push_back(dep.as_str());
                        }
                    }
                }
            }
        }
        // Any id not reached by Kahn's is in a cycle — will use own priority.

        // --- Step 2: propagate in reverse-topological order ---------------------
        // Process dependents first (end of topo_order) down to dependencies (start).
        let mut eff: HashMap<String, Priority> = HashMap::with_capacity(all_ids.len());

        // Seed: every node starts with its own priority.
        for &id in &all_ids {
            let own = tasks.get(id).map(|t| t.priority).unwrap_or(Priority::P3);
            eff.insert(id.to_string(), own);
        }

        // Walk in reverse topological order: process dependents before dependencies.
        for &id in topo_order.iter().rev() {
            // Collect the best priority among all direct dependents of `id`.
            let best_dependent: Option<Priority> = self
                .reverse
                .get(id)
                .into_iter()
                .flat_map(|s| s.iter())
                .filter_map(|dep_id| eff.get(dep_id.as_str()).copied())
                .min();

            if let Some(best) = best_dependent {
                let entry = eff.entry(id.to_string()).or_insert(Priority::P3);
                *entry = (*entry).min(best);
            }
        }

        eff
    }

    /// Check if adding an edge from -> to would create a cycle.
    /// Does BFS from `to` following edges; if we reach `from`, it's a cycle.
    pub fn would_cycle(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }

        // BFS from `to` through its dependencies. If we reach `from`, adding
        // from -> to would create a cycle (since `to` already reaches `from`).
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(to.to_string());

        while let Some(current) = queue.pop_front() {
            if current == from {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(deps) = self.edges.get(&current) {
                for dep in deps {
                    queue.push_back(dep.clone());
                }
            }
        }

        false
    }

    /// Build a dependency tree for display.
    ///
    /// Uses two sets to bound rendering:
    /// - `visiting` (path-local): cycle detection — a node on the current
    ///   recursion path is emitted as a leaf with `cycle: true`.
    /// - `seen` (render-global): DAG deduplication — a node already fully
    ///   expanded on *any* earlier path is emitted as a leaf with `seen: true`
    ///   (and no children), preventing exponential blowup on diamond shapes.
    pub fn dep_tree<'a>(&self, tasks: &'a HashMap<String, Task>, id: &str) -> Option<DepNode<'a>> {
        let mut visiting = HashSet::new();
        let mut seen = HashSet::new();
        self.dep_tree_inner(tasks, id, &mut visiting, &mut seen)
    }

    fn dep_tree_inner<'a>(
        &self,
        tasks: &'a HashMap<String, Task>,
        id: &str,
        visiting: &mut HashSet<String>,
        seen: &mut HashSet<String>,
    ) -> Option<DepNode<'a>> {
        let task = tasks.get(id)?;

        if !visiting.insert(id.to_string()) {
            // Already on the current recursion path — cycle detected.
            return Some(DepNode {
                task,
                children: Vec::new(),
                cycle: true,
                seen: false,
            });
        }

        if !seen.insert(id.to_string()) {
            // Already fully expanded on an earlier path — emit a reference leaf
            // to avoid re-expanding (prevents exponential blowup on diamonds).
            visiting.remove(id);
            return Some(DepNode {
                task,
                children: Vec::new(),
                cycle: false,
                seen: true,
            });
        }

        let children = task
            .depends_on
            .iter()
            .filter_map(|dep_id| self.dep_tree_inner(tasks, dep_id, visiting, seen))
            .collect();

        visiting.remove(id);

        Some(DepNode {
            task,
            children,
            cycle: false,
            seen: false,
        })
    }

    /// Topological sort over a subset of task IDs.
    /// Only dependency edges between tasks in the subset are considered.
    /// Tie-breaking: priority (P0 first), then creation date (oldest first).
    pub fn topo_sort_subset<'a>(
        &self,
        subset: &HashSet<String>,
        tasks: &'a HashMap<String, Task>,
    ) -> Vec<&'a Task> {
        if subset.is_empty() {
            return Vec::new();
        }

        // Compute in-degrees considering only intra-subset edges
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for id in subset {
            in_degree.insert(id.as_str(), 0);
        }
        for id in subset {
            if let Some(deps) = self.edges.get(id) {
                for dep in deps {
                    if subset.contains(dep) {
                        *in_degree.entry(id.as_str()).or_default() += 1;
                    }
                }
            }
        }

        // Seed with zero-in-degree nodes, sorted by priority then created.
        // Use VecDeque for O(1) front-pop (Vec::remove(0) is O(n)).
        // NOTE: newly-ready nodes are sorted among themselves and appended to the
        // back of the queue rather than merged into the existing entries, so the
        // overall ordering is only approximately priority-sorted when independent
        // batches interleave. This is intentional: the fully-correct merge would
        // require a priority-queue rebuild on every step and is not worth the
        // complexity for the plan display use-case.
        let mut seed: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&id, _)| id)
            .collect();
        seed.sort_by(|a, b| {
            let ta = tasks.get(*a);
            let tb = tasks.get(*b);
            match (ta, tb) {
                (Some(ta), Some(tb)) => ta
                    .priority
                    .cmp(&tb.priority)
                    .then(ta.created.cmp(&tb.created)),
                _ => std::cmp::Ordering::Equal,
            }
        });
        let mut queue: VecDeque<&str> = seed.into_iter().collect();
        let mut result: Vec<&'a Task> = Vec::new();
        while let Some(current) = queue.pop_front() {
            if let Some(task) = tasks.get(current) {
                result.push(task);
            }

            // Decrement in-degree for tasks that depend on current
            if let Some(dependents) = self.reverse.get(current) {
                let mut newly_ready: Vec<&str> = Vec::new();
                for dep in dependents {
                    if subset.contains(dep)
                        && let Some(deg) = in_degree.get_mut(dep.as_str())
                    {
                        *deg -= 1;
                        if *deg == 0 {
                            newly_ready.push(dep.as_str());
                        }
                    }
                }
                // Sort newly ready by priority then created before appending
                newly_ready.sort_by(|a, b| {
                    let ta = tasks.get(*a);
                    let tb = tasks.get(*b);
                    match (ta, tb) {
                        (Some(ta), Some(tb)) => ta
                            .priority
                            .cmp(&tb.priority)
                            .then(ta.created.cmp(&tb.created)),
                        _ => std::cmp::Ordering::Equal,
                    }
                });
                queue.extend(newly_ready);
            }
        }

        result
    }

    /// Get adjacency list for JSON output.
    pub fn adjacency_list(&self) -> HashMap<&str, Vec<&str>> {
        self.edges
            .iter()
            .map(|(k, v)| {
                let deps: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
                (k.as_str(), deps)
            })
            .collect()
    }
}

pub struct DepNode<'a> {
    pub task: &'a Task,
    pub children: Vec<DepNode<'a>>,
    /// True when this node was already seen on the **current path** (cycle).
    pub cycle: bool,
    /// True when this node was already fully expanded on an **earlier path**
    /// (DAG diamond deduplication). Distinct from `cycle`.
    pub seen: bool,
}

#[derive(Serialize)]
pub struct DepNodeJson {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: Priority,
    pub children: Vec<DepNodeJson>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cycle: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub seen: bool,
}

impl DepNodeJson {
    pub fn from_dep_node(node: &DepNode<'_>) -> Self {
        DepNodeJson {
            id: node.task.id.clone(),
            title: node.task.title.clone(),
            status: node.task.status.to_string(),
            priority: node.task.priority,
            children: node
                .children
                .iter()
                .map(DepNodeJson::from_dep_node)
                .collect(),
            cycle: node.cycle,
            seen: node.seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskType;
    use chrono::Utc;

    fn make_task(id: &str, status: Status, priority: Priority, deps: Vec<&str>) -> Task {
        Task {
            id: id.into(),
            title: format!("Task {id}"),
            task_type: TaskType::default(),
            status,
            priority,
            created: Utc::now(),
            updated: Utc::now(),
            tags: Vec::new(),
            depends_on: deps.into_iter().map(String::from).collect(),
            parent: None,
            assignee: String::new(),
            body: String::new(),
        }
    }

    fn make_tasks(tasks: Vec<Task>) -> HashMap<String, Task> {
        tasks.into_iter().map(|t| (t.id.clone(), t)).collect()
    }

    #[test]
    fn test_ready_no_deps() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P0, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        // P0 should come first
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].id, "b");
        assert_eq!(ready[1].id, "a");
    }

    #[test]
    fn test_ready_with_deps() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Done, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec!["a"]),
            make_task("c", Status::Open, Priority::P1, vec!["b"]),
        ]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        // Only b is ready (a is done, c depends on b which isn't done)
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "b");
    }

    #[test]
    fn test_ready_blocked_by_undone_dep() {
        let tasks = make_tasks(vec![
            make_task("a", Status::InProgress, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec!["a"]),
        ]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_ready_blocked_by_missing_dep() {
        // Task "b" depends on "nonexistent" which is not in the task map
        let tasks = make_tasks(vec![make_task(
            "b",
            Status::Open,
            Priority::P1,
            vec!["nonexistent"],
        )]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_ready_with_tag_filter() {
        let mut t = make_task("a", Status::Open, Priority::P1, vec![]);
        t.tags = vec!["backend".into()];
        let tasks = make_tasks(vec![t, make_task("b", Status::Open, Priority::P1, vec![])]);
        let graph = Graph::build(&tasks);

        let ready = graph.ready(&tasks, Some("backend"), None, None);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a");
    }

    #[test]
    fn test_ready_with_limit() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec![]),
            make_task("c", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, Some(2), None);
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn test_ready_excludes_epics() {
        let mut epic = make_task("e", Status::Open, Priority::P0, vec![]);
        epic.task_type = TaskType::Epic;
        let tasks = make_tasks(vec![
            epic,
            make_task("a", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        // Only task "a" should be ready, not epic "e"
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a");
    }

    #[test]
    fn test_ready_with_epic_filter() {
        let mut t1 = make_task("a", Status::Open, Priority::P1, vec![]);
        t1.parent = Some("epic1".into());
        let mut t2 = make_task("b", Status::Open, Priority::P1, vec![]);
        t2.parent = Some("epic2".into());
        let t3 = make_task("c", Status::Open, Priority::P1, vec![]);
        let tasks = make_tasks(vec![t1, t2, t3]);
        let graph = Graph::build(&tasks);

        // Filter to epic1 — only task "a"
        let ready = graph.ready(&tasks, None, None, Some("epic1"));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a");

        // Filter to epic2 — only task "b"
        let ready = graph.ready(&tasks, None, None, Some("epic2"));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "b");

        // No filter — all three
        let ready = graph.ready(&tasks, None, None, None);
        assert_eq!(ready.len(), 3);
    }

    #[test]
    fn test_cycle_self() {
        let tasks = make_tasks(vec![make_task("a", Status::Open, Priority::P1, vec![])]);
        let graph = Graph::build(&tasks);
        assert!(graph.would_cycle("a", "a"));
    }

    #[test]
    fn test_cycle_direct() {
        // a depends on b. Would adding b -> a create a cycle? Yes.
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b"]),
            make_task("b", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        assert!(graph.would_cycle("b", "a"));
    }

    #[test]
    fn test_cycle_transitive() {
        // a -> b -> c. Would adding c -> a create a cycle? Yes.
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b"]),
            make_task("b", Status::Open, Priority::P1, vec!["c"]),
            make_task("c", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        assert!(graph.would_cycle("c", "a"));
    }

    #[test]
    fn test_no_cycle() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        assert!(!graph.would_cycle("a", "b"));
    }

    #[test]
    fn test_dep_tree() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b", "c"]),
            make_task("b", Status::Open, Priority::P1, vec![]),
            make_task("c", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert_eq!(tree.task.id, "a");
        assert_eq!(tree.children.len(), 2);
    }

    #[test]
    fn test_empty_graph() {
        let tasks: HashMap<String, Task> = HashMap::new();
        let graph = Graph::build(&tasks);
        let ready = graph.ready(&tasks, None, None, None);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_adjacency_list() {
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b"]),
            make_task("b", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let adj = graph.adjacency_list();
        assert_eq!(adj.get("a").unwrap().len(), 1);
        assert!(adj.get("b").unwrap().is_empty());
    }

    #[test]
    fn test_dep_tree_direct_cycle() {
        // a -> b -> a  (direct cycle)
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b"]),
            make_task("b", Status::Open, Priority::P1, vec!["a"]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert!(!tree.cycle);
        assert_eq!(tree.children.len(), 1);
        // b's child "a" should be a cycle marker
        let b_node = &tree.children[0];
        assert_eq!(b_node.task.id, "b");
        assert!(!b_node.cycle);
        assert_eq!(b_node.children.len(), 1);
        let cycle_node = &b_node.children[0];
        assert_eq!(cycle_node.task.id, "a");
        assert!(cycle_node.cycle);
        assert!(cycle_node.children.is_empty());
    }

    #[test]
    fn test_dep_tree_transitive_cycle() {
        // a -> b -> c -> a  (transitive cycle)
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b"]),
            make_task("b", Status::Open, Priority::P1, vec!["c"]),
            make_task("c", Status::Open, Priority::P1, vec!["a"]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert!(!tree.cycle);
        let b = &tree.children[0];
        let c = &b.children[0];
        assert_eq!(c.task.id, "c");
        assert!(!c.cycle);
        let back_to_a = &c.children[0];
        assert_eq!(back_to_a.task.id, "a");
        assert!(back_to_a.cycle);
        assert!(back_to_a.children.is_empty());
    }

    #[test]
    fn test_dep_tree_self_cycle() {
        // a -> a  (self-referencing)
        let tasks = make_tasks(vec![make_task("a", Status::Open, Priority::P1, vec!["a"])]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert!(!tree.cycle);
        assert_eq!(tree.children.len(), 1);
        let self_ref = &tree.children[0];
        assert_eq!(self_ref.task.id, "a");
        assert!(self_ref.cycle);
        assert!(self_ref.children.is_empty());
    }

    #[test]
    fn test_dep_tree_no_cycle() {
        // Diamond: a -> b, a -> c, b -> d, c -> d (no cycle).
        // With DAG deduplication, d is fully expanded under the FIRST child that
        // visits it and emitted as a `seen` reference leaf under the second.
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b", "c"]),
            make_task("b", Status::Open, Priority::P1, vec!["d"]),
            make_task("c", Status::Open, Priority::P1, vec!["d"]),
            make_task("d", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert!(!tree.cycle);
        assert!(!tree.seen);
        // Both b and c appear under a (neither is cycle or seen at that level)
        assert_eq!(tree.children.len(), 2);
        for child in &tree.children {
            assert!(!child.cycle);
            // Each should have exactly one child for d
            assert_eq!(child.children.len(), 1);
            let d_node = &child.children[0];
            assert_eq!(d_node.task.id, "d");
            assert!(!d_node.cycle);
            // d is expanded fully the first time; second occurrence is `seen`
        }
        // Exactly one of the two d appearances is seen (the second visit)
        let d_nodes: Vec<_> = tree
            .children
            .iter()
            .flat_map(|c| c.children.iter())
            .collect();
        assert_eq!(d_nodes.len(), 2);
        let seen_count = d_nodes.iter().filter(|n| n.seen).count();
        let full_count = d_nodes.iter().filter(|n| !n.seen && !n.cycle).count();
        assert_eq!(
            seen_count, 1,
            "exactly one d occurrence should be a seen-ref"
        );
        assert_eq!(
            full_count, 1,
            "exactly one d occurrence should be fully expanded"
        );
    }

    #[test]
    fn test_effective_priority_no_dependents() {
        // Task with no dependents: effective == own
        let tasks = make_tasks(vec![make_task("a", Status::Open, Priority::P3, vec![])]);
        let graph = Graph::build(&tasks);
        assert_eq!(graph.effective_priority("a", &tasks), Priority::P3);
    }

    #[test]
    fn test_effective_priority_single_dependent() {
        // "a" is depended on by "b" (P1). a's own priority is P3.
        // b depends_on a → a's effective should be P1
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P3, vec![]),
            make_task("b", Status::Open, Priority::P1, vec!["a"]),
        ]);
        let graph = Graph::build(&tasks);
        assert_eq!(graph.effective_priority("a", &tasks), Priority::P1);
        // b has no dependents, so effective == own
        assert_eq!(graph.effective_priority("b", &tasks), Priority::P1);
    }

    #[test]
    fn test_effective_priority_chain() {
        // c (P0) -> b (P2) -> a (P3)
        // a's effective should be P0 (transitive through b from c)
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P3, vec![]),
            make_task("b", Status::Open, Priority::P2, vec!["a"]),
            make_task("c", Status::Open, Priority::P0, vec!["b"]),
        ]);
        let graph = Graph::build(&tasks);
        assert_eq!(graph.effective_priority("a", &tasks), Priority::P0);
        assert_eq!(graph.effective_priority("b", &tasks), Priority::P0);
        assert_eq!(graph.effective_priority("c", &tasks), Priority::P0);
    }

    #[test]
    fn test_effective_priority_diamond() {
        // d (P0) -> b (P2), d (P0) -> c (P3), b -> a (P3), c -> a (P3)
        // a's effective should be P0 (via both paths)
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P3, vec![]),
            make_task("b", Status::Open, Priority::P2, vec!["a"]),
            make_task("c", Status::Open, Priority::P3, vec!["a"]),
            make_task("d", Status::Open, Priority::P0, vec!["b", "c"]),
        ]);
        let graph = Graph::build(&tasks);
        assert_eq!(graph.effective_priority("a", &tasks), Priority::P0);
        assert_eq!(graph.effective_priority("b", &tasks), Priority::P0);
        assert_eq!(graph.effective_priority("c", &tasks), Priority::P0);
    }

    #[test]
    fn test_effective_priority_own_is_highest() {
        // Task's own priority is already highest — should stay same
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P0, vec![]),
            make_task("b", Status::Open, Priority::P3, vec!["a"]),
        ]);
        let graph = Graph::build(&tasks);
        assert_eq!(graph.effective_priority("a", &tasks), Priority::P0);
    }

    #[test]
    fn test_topo_sort_linear_chain() {
        // c depends on b, b depends on a → expect [a, b, c]
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec!["a"]),
            make_task("c", Status::Open, Priority::P1, vec!["b"]),
        ]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topo_sort_diamond() {
        // d has no deps, b depends on d, c depends on d, a depends on b and c
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b", "c"]),
            make_task("b", Status::Open, Priority::P1, vec!["d"]),
            make_task("c", Status::Open, Priority::P1, vec!["d"]),
            make_task("d", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        // d must come first, a must come last
        assert_eq!(ids[0], "d");
        assert_eq!(ids[ids.len() - 1], "a");
    }

    #[test]
    fn test_topo_sort_independent() {
        // No deps among subset, sorted by priority then created
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P2, vec![]),
            make_task("b", Status::Open, Priority::P0, vec![]),
            make_task("c", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        let ids: Vec<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
        // P0 first, then P1, then P2
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn test_topo_sort_single_task() {
        let tasks = make_tasks(vec![make_task("a", Status::Open, Priority::P1, vec![])]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = ["a"].iter().map(|s| s.to_string()).collect();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].id, "a");
    }

    #[test]
    fn test_topo_sort_empty() {
        let tasks = make_tasks(vec![make_task("a", Status::Open, Priority::P1, vec![])]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = HashSet::new();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        assert!(sorted.is_empty());
    }

    #[test]
    fn test_topo_sort_ignores_external_deps() {
        // b depends on "ext" which is not in the subset
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec![]),
            make_task("b", Status::Open, Priority::P1, vec!["ext"]),
            make_task("ext", Status::Done, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let subset: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let sorted = graph.topo_sort_subset(&subset, &tasks);
        assert_eq!(sorted.len(), 2);
    }

    // --- Coupled-graph regression guards ------------------------------------------
    //
    // Fast structural tests (always run) verify that algorithmic fixes hold for
    // small representative graphs without any timing dependency.
    //
    // Heavy timing tests are marked `#[ignore]` and must be run explicitly with
    //   cargo test -- --ignored
    // They use std::time::Instant with generous bounds; any reasonable hardware
    // should satisfy them.

    fn count_nodes(node: &DepNode<'_>) -> usize {
        1 + node.children.iter().map(count_nodes).sum::<usize>()
    }

    /// Fast structural check: effective priorities on a long chain are correct.
    /// If effective_priorities_all were O(V²) or O(V*E), the values would still
    /// be correct but the large graph below would timeout — this test documents
    /// expected values on a chain without relying on timing.
    #[test]
    fn test_effective_priority_long_chain_correct() {
        // Build chain: t0 <- t1 <- t2 <- ... <- t19
        // t19 has priority P0; all others have P3.
        // effective(t0) should be P0 (propagated from t19 through the chain).
        let n = 20usize;
        let mut task_list = Vec::new();
        for i in 0..n {
            let priority = if i == n - 1 {
                Priority::P0
            } else {
                Priority::P3
            };
            task_list.push(make_task(&format!("t{i}"), Status::Open, priority, vec![]));
        }
        // Wire chain: t_{i+1} depends on t_i
        for i in 0..n - 1 {
            task_list[i + 1].depends_on = vec![format!("t{i}")];
        }
        let tasks = make_tasks(task_list);
        let graph = Graph::build(&tasks);

        // effective(t0) must be P0 (t19 depends transitively on t0's output)
        let eff = graph.effective_priorities_all(&tasks);
        assert_eq!(
            eff["t0"],
            Priority::P0,
            "t0 effective priority should be P0"
        );
        assert_eq!(
            eff["t9"],
            Priority::P0,
            "t9 effective priority should be P0"
        );
        // t19 has own P0
        assert_eq!(eff["t19"], Priority::P0);
    }

    #[test]
    fn test_dep_tree_deep_diamond_linear_node_count() {
        // Build a "double-fan" diamond: root depends on L layers, each layer
        // node depends on all nodes in the next layer, converging to a single
        // shared leaf at the bottom.
        //
        //   root
        //    |
        //   L1a, L1b          (both depend on L2a, L2b)
        //       |
        //   L2a, L2b          (both depend on leaf)
        //       |
        //    leaf
        //
        // Without deduplication the tree would expand leaf 4 times (2^2).
        // With the `seen` fix it appears at most once as a full node plus
        // `seen`-ref placeholders — total node count stays bounded by O(V+E).
        let tasks = make_tasks(vec![
            make_task("root", Status::Open, Priority::P1, vec!["l1a", "l1b"]),
            make_task("l1a", Status::Open, Priority::P1, vec!["l2a", "l2b"]),
            make_task("l1b", Status::Open, Priority::P1, vec!["l2a", "l2b"]),
            make_task("l2a", Status::Open, Priority::P1, vec!["leaf"]),
            make_task("l2b", Status::Open, Priority::P1, vec!["leaf"]),
            make_task("leaf", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "root").unwrap();

        // 6 distinct nodes → maximum rendered nodes = 6 (V) + 4 (E where
        // second-visit refs are emitted) = at most V+E = 10. In practice we
        // get exactly V + (number of seen-ref appearances) which is at most
        // V+E, well below the exponential 2^layers.
        let node_count = count_nodes(&tree);
        let v = tasks.len(); // 6
        let e: usize = tasks.values().map(|t| t.depends_on.len()).sum(); // 8
        assert!(
            node_count <= v + e,
            "node_count={node_count} exceeded V+E={} — exponential blowup detected",
            v + e
        );
    }

    // --- #[ignore]'d timing benchmarks (run with: cargo test -- --ignored) --------
    //
    // These build a densely coupled graph of N tasks and assert that the key
    // operations complete well within a generous wall-clock bound. They are
    // `#[ignore]` to avoid slowing down the default `cargo test` run.

    /// Build N tasks wired as a full "staircase": task i depends on task i-1,
    /// plus every 5th task depends on a shared "bottom" task.
    fn make_dense_tasks(n: usize) -> HashMap<String, Task> {
        let mut list = Vec::with_capacity(n + 1);
        // Shared bottom-of-chain task
        list.push(make_task("bottom", Status::Done, Priority::P2, vec![]));
        for i in 0..n {
            let id = format!("t{i:04}");
            let priority = if i % 10 == 0 {
                Priority::P0
            } else {
                Priority::P3
            };
            list.push(make_task(&id, Status::Open, priority, vec![]));
        }
        // Wire dependencies after creation (make_task takes &str slice)
        let mut map: HashMap<String, Task> = list.into_iter().map(|t| (t.id.clone(), t)).collect();
        for i in 0..n {
            let id = format!("t{i:04}");
            let mut deps = vec!["bottom".to_string()];
            if i > 0 {
                deps.push(format!("t{:04}", i - 1));
            }
            map.get_mut(&id).unwrap().depends_on = deps;
        }
        map
    }

    #[test]
    #[ignore]
    fn bench_effective_priorities_large_graph() {
        // 500-node staircase — each node depends on 1-2 predecessors.
        // effective_priorities_all should finish in well under 1 second.
        let n = 500;
        let tasks = make_dense_tasks(n);
        let graph = Graph::build(&tasks);

        let start = std::time::Instant::now();
        let eff = graph.effective_priorities_all(&tasks);
        let elapsed = start.elapsed();

        assert_eq!(eff.len(), n + 1); // n tasks + bottom
        assert!(
            elapsed.as_millis() < 500,
            "effective_priorities_all on {n} tasks took {}ms (expected <500ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    #[ignore]
    fn bench_ready_large_graph() {
        // 500-node staircase. Only t0000 is truly ready (all deps done = bottom only).
        // Graph::ready should finish in well under 1 second.
        let n = 500;
        let tasks = make_dense_tasks(n);
        let graph = Graph::build(&tasks);

        let start = std::time::Instant::now();
        let ready = graph.ready(&tasks, None, None, None);
        let elapsed = start.elapsed();

        // t0000 depends only on "bottom" (done), so it must be ready
        assert!(
            ready.iter().any(|t| t.id == "t0000"),
            "t0000 should be in the ready list"
        );
        assert!(
            elapsed.as_millis() < 500,
            "graph.ready on {n} tasks took {}ms (expected <500ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    #[ignore]
    fn bench_dep_tree_diamond_deep() {
        // Build a deep 6-layer binary diamond: layer 0 → layer 1 (×2) → ... → layer 5 (1 node).
        // Without the `seen` fix this would be exponential (2^5 = 32 leaf expansions).
        // With the fix, node count should stay linear in V+E.
        let layers = 6usize;
        // layer 0: 1 node (root)
        // layer k: 2^k nodes, each depending on all nodes in layer k+1
        // layer 5: 1 node (shared leaf)
        // We'll use owned ids; can't use &str in make_task with dynamic strings directly.
        // Build manually.
        let mut map: HashMap<String, Task> = HashMap::new();
        let root = make_task("root", Status::Open, Priority::P1, vec![]);
        map.insert("root".to_string(), root);

        let mut layer_ids: Vec<Vec<String>> = Vec::new();
        // layer 0 = root
        layer_ids.push(vec!["root".to_string()]);
        // layers 1..=layers-1
        for l in 1..layers {
            let count = if l == layers - 1 {
                1
            } else {
                2usize.pow(l as u32)
            };
            let ids: Vec<String> = (0..count).map(|i| format!("l{l}_{i}")).collect();
            layer_ids.push(ids);
        }

        // Create all tasks (no deps yet)
        for ids in &layer_ids {
            for id in ids {
                let t = make_task(id, Status::Open, Priority::P1, vec![]);
                map.insert(id.clone(), t);
            }
        }
        // Wire: each node in layer l depends on all nodes in layer l+1
        for l in 0..layers - 1 {
            let next = layer_ids[l + 1].clone();
            for id in &layer_ids[l] {
                map.get_mut(id).unwrap().depends_on = next.clone();
            }
        }

        let graph = Graph::build(&map);
        let start = std::time::Instant::now();
        let tree = graph.dep_tree(&map, "root").unwrap();
        let elapsed = start.elapsed();

        let node_count = count_nodes(&tree);
        let v = map.len();
        let e: usize = map.values().map(|t| t.depends_on.len()).sum();
        assert!(
            node_count <= v + e,
            "node_count={node_count} exceeded V+E={} — exponential blowup",
            v + e
        );
        assert!(
            elapsed.as_millis() < 500,
            "dep_tree on {layers}-layer diamond took {}ms (expected <500ms)",
            elapsed.as_millis()
        );
    }
}
