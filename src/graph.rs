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
    pub fn ready<'a>(
        &self,
        tasks: &'a HashMap<String, Task>,
        tag: Option<&str>,
        limit: Option<usize>,
        epic: Option<&str>,
    ) -> Vec<&'a Task> {
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

        // Sort by effective priority (P0 first), then by creation date (oldest first)
        result.sort_by(|a, b| {
            self.effective_priority(&a.id, tasks)
                .cmp(&self.effective_priority(&b.id, tasks))
                .then(a.created.cmp(&b.created))
        });

        if let Some(limit) = limit {
            result.truncate(limit);
        }

        result
    }

    /// Compute the effective priority of a task.
    /// This is the minimum (highest urgency) of the task's own priority and
    /// the priorities of all tasks that depend on it, transitively.
    pub fn effective_priority(&self, id: &str, tasks: &HashMap<String, Task>) -> Priority {
        let own = tasks.get(id).map(|t| t.priority).unwrap_or(Priority::P3);

        let mut best = own;
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(id.to_string());

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(dependents) = self.reverse.get(&current) {
                for dep in dependents {
                    if let Some(t) = tasks.get(dep.as_str()) {
                        best = best.min(t.priority);
                    }
                    queue.push_back(dep.clone());
                }
            }
        }

        best
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
    /// Cycle-safe: nodes already on the current path are emitted as leaf markers.
    pub fn dep_tree<'a>(&self, tasks: &'a HashMap<String, Task>, id: &str) -> Option<DepNode<'a>> {
        let mut visited = HashSet::new();
        self.dep_tree_inner(tasks, id, &mut visited)
    }

    fn dep_tree_inner<'a>(
        &self,
        tasks: &'a HashMap<String, Task>,
        id: &str,
        visiting: &mut HashSet<String>,
    ) -> Option<DepNode<'a>> {
        let task = tasks.get(id)?;

        if !visiting.insert(id.to_string()) {
            // Already on the current recursion path — cycle detected
            return Some(DepNode {
                task,
                children: Vec::new(),
                cycle: true,
            });
        }

        let children = task
            .depends_on
            .iter()
            .filter_map(|dep_id| self.dep_tree_inner(tasks, dep_id, visiting))
            .collect();

        visiting.remove(id);

        Some(DepNode {
            task,
            children,
            cycle: false,
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

        // Seed with zero-in-degree nodes, sorted by priority then created
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort_by(|a, b| {
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

        let mut result: Vec<&'a Task> = Vec::new();
        while !queue.is_empty() {
            let current = queue.remove(0);
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
                // Sort newly ready by priority then created
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
    /// True when this node was already seen on the current path (cycle).
    pub cycle: bool,
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
        // Diamond: a -> b, a -> c, b -> d, c -> d (no cycle — d appears twice but not cyclic)
        let tasks = make_tasks(vec![
            make_task("a", Status::Open, Priority::P1, vec!["b", "c"]),
            make_task("b", Status::Open, Priority::P1, vec!["d"]),
            make_task("c", Status::Open, Priority::P1, vec!["d"]),
            make_task("d", Status::Open, Priority::P1, vec![]),
        ]);
        let graph = Graph::build(&tasks);
        let tree = graph.dep_tree(&tasks, "a").unwrap();
        assert!(!tree.cycle);
        // d should appear under both b and c, and neither should be marked as cycle
        for child in &tree.children {
            assert!(!child.cycle);
            for grandchild in &child.children {
                assert_eq!(grandchild.task.id, "d");
                assert!(!grandchild.cycle);
            }
        }
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
}
