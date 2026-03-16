use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::task::{Priority, Status, Task};

/// Dependency graph built from task `depends_on` fields.
pub struct Graph {
    /// task_id -> set of task IDs it depends on
    pub edges: HashMap<String, HashSet<String>>,
}

impl Graph {
    /// Build a dependency graph from a set of tasks.
    pub fn build(tasks: &HashMap<String, Task>) -> Self {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();

        for task in tasks.values() {
            edges.entry(task.id.clone()).or_default();

            for dep in &task.depends_on {
                edges
                    .entry(task.id.clone())
                    .or_default()
                    .insert(dep.clone());
            }
        }

        Graph { edges }
    }

    /// Return tasks that are ready: status is Open and all dependencies are Done.
    pub fn ready<'a>(
        &self,
        tasks: &'a HashMap<String, Task>,
        tag: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<&'a Task> {
        let mut result: Vec<&Task> = tasks
            .values()
            .filter(|t| t.status == Status::Open)
            .filter(|t| {
                // All dependencies must be done
                t.depends_on.iter().all(|dep_id| match tasks.get(dep_id) {
                    Some(dep) => dep.status == Status::Done,
                    None => true, // missing dep is treated as satisfied
                })
            })
            .filter(|t| match tag {
                Some(tag) => t.tags.iter().any(|tt| tt == tag),
                None => true,
            })
            .collect();

        // Sort by priority (P0 first), then by creation date (oldest first)
        result.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

        if let Some(limit) = limit {
            result.truncate(limit);
        }

        result
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
    use chrono::Utc;

    fn make_task(id: &str, status: Status, priority: Priority, deps: Vec<&str>) -> Task {
        Task {
            id: id.into(),
            title: format!("Task {id}"),
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
        let ready = graph.ready(&tasks, None, None);
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
        let ready = graph.ready(&tasks, None, None);
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
        let ready = graph.ready(&tasks, None, None);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_ready_with_tag_filter() {
        let mut t = make_task("a", Status::Open, Priority::P1, vec![]);
        t.tags = vec!["backend".into()];
        let tasks = make_tasks(vec![t, make_task("b", Status::Open, Priority::P1, vec![])]);
        let graph = Graph::build(&tasks);

        let ready = graph.ready(&tasks, Some("backend"), None);
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
        let ready = graph.ready(&tasks, None, Some(2));
        assert_eq!(ready.len(), 2);
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
        let ready = graph.ready(&tasks, None, None);
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
}
