use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use serde::Serialize;

use crate::config;
use crate::error::{Error, Result};
use crate::graph::Graph;
use crate::store;
use crate::task::{self, Priority, Status, Task, TaskType};

/// Create a new task with validation.
#[allow(clippy::too_many_arguments)]
pub fn create_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    title: String,
    priority: Priority,
    tags: Vec<String>,
    depends_on: Vec<String>,
    parent: Option<String>,
    body: String,
    task_type: TaskType,
) -> Result<Task> {
    let config = config::load(base)?;
    let mut existing_ids: HashSet<String> = tasks.keys().cloned().collect();
    // Also exclude archived IDs so new tasks never reuse an archived ID.
    existing_ids.extend(store::archived_id_set(base));
    let id = task::generate_id(&existing_ids, config.id_length as usize);

    let unknown: Vec<String> = depends_on
        .iter()
        .filter(|dep| !tasks.contains_key(dep.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(Error::UnknownDependency { ids: unknown });
    }

    let mut t = Task::new(id, title, priority);
    t.task_type = task_type;
    t.tags = tags;
    t.depends_on = depends_on;
    t.parent = parent;
    t.body = body;

    store::save(base, &t)?;
    Ok(t)
}

/// List tasks with optional filters, sorted by priority then creation date.
pub fn list_tasks(
    tasks: &HashMap<String, Task>,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<&str>,
    include_all: bool,
    epic: Option<&str>,
) -> Vec<Task> {
    let mut filtered: Vec<Task> = tasks
        .values()
        .filter(|t| {
            if status.is_some() || include_all {
                true
            } else {
                task::is_active(t)
            }
        })
        .filter(|t| status.as_ref().is_none_or(|s| t.status == *s))
        .filter(|t| priority.as_ref().is_none_or(|p| t.priority == *p))
        .filter(|t| task::matches_tag(t, tag))
        .filter(|t| epic.is_none_or(|e| t.parent.as_deref() == Some(e)))
        .cloned()
        .collect();
    task::sort_by_priority_owned(&mut filtered);
    filtered
}

/// Return tasks that are ready to work on.
pub fn list_ready(
    tasks: &HashMap<String, Task>,
    tag: Option<&str>,
    limit: Option<usize>,
    epic: Option<&str>,
) -> Vec<Task> {
    let graph = Graph::build(tasks);
    let ready = graph.ready(tasks, tag, limit, epic);
    ready.into_iter().cloned().collect()
}

/// Get a single task by ID or prefix.
pub fn get_task(tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    Ok(tasks[&id].clone())
}

/// Update task fields. Only `Some` fields are changed.
#[allow(clippy::too_many_arguments)]
pub fn update_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    status: Option<Status>,
    priority: Option<Priority>,
    tags: Option<Vec<String>>,
    assignee: Option<String>,
    body: Option<String>,
    title: Option<String>,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();

    let status_changed = status.as_ref().is_some_and(|s| *s != t.status);
    if let Some(s) = status {
        t.status = s;
    }
    if let Some(p) = priority {
        t.priority = p;
    }
    if let Some(tags) = tags {
        t.tags = tags;
    }
    if let Some(a) = assignee {
        t.assignee = a;
    }
    if let Some(b) = body {
        t.body = b;
    }
    if let Some(title) = title {
        t.title = title;
    }
    t.updated = Utc::now();

    store::save(base, &t)?;

    // Apply status-change side effects (e.g. epic auto-close) when status changed.
    if status_changed {
        on_status_changed(base, tasks, &t)?;
    }

    Ok(t)
}

/// Set task status by ID or prefix.
pub fn set_status(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    status: Status,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();
    t.status = status;
    t.updated = Utc::now();
    store::save(base, &t)?;

    on_status_changed(base, tasks, &t)?;

    Ok(t)
}

/// Apply side effects after a task's status has been changed and saved.
///
/// Triggers epic auto-close check and cascades up through nested epics.
/// `tasks` is the pre-change snapshot; `t` is the task with its NEW status.
fn on_status_changed(base: &Path, tasks: &HashMap<String, Task>, t: &Task) -> Result<()> {
    // `overrides` tracks tasks that have been auto-closed during this call so
    // that recursive ancestor checks see the up-to-date statuses even though
    // `tasks` is an immutable pre-change snapshot.
    let mut overrides: HashMap<String, Status> = HashMap::new();
    overrides.insert(t.id.clone(), t.status.clone());
    maybe_close_parent_epic(base, tasks, t, &mut overrides)
}

/// Resolve the effective status of a task, preferring the `overrides` map.
fn effective_status<'a>(task: &'a Task, overrides: &'a HashMap<String, Status>) -> &'a Status {
    overrides.get(&task.id).unwrap_or(&task.status)
}

/// Check whether `t`'s parent epic should auto-close, and if so close it and
/// recurse up through ancestor epics. `overrides` accumulates newly-written
/// statuses so that each level sees the current state without re-reading disk.
fn maybe_close_parent_epic(
    base: &Path,
    tasks: &HashMap<String, Task>,
    t: &Task,
    overrides: &mut HashMap<String, Status>,
) -> Result<()> {
    // Trigger auto-close check when the child transitions to Done or Cancelled.
    let t_status = effective_status(t, overrides);
    let is_resolved = *t_status == Status::Done || *t_status == Status::Cancelled;
    if !is_resolved {
        return Ok(());
    }

    let Some(ref parent_id) = t.parent else {
        return Ok(());
    };
    let Some(parent) = tasks.get(parent_id) else {
        return Ok(());
    };
    if !parent.task_type.is_epic() {
        return Ok(());
    }
    // Skip if already (auto-)closed in this call chain.
    if *effective_status(parent, overrides) == Status::Done {
        return Ok(());
    }

    // An epic is fully resolved when every child is Done or Cancelled
    // (cancelled = resolved and non-blocking). We consult `overrides` for
    // up-to-date statuses written during this recursive call.
    let children: Vec<_> = tasks
        .values()
        .filter(|c| c.parent.as_deref() == Some(parent_id))
        .collect();
    let has_children = !children.is_empty();
    let all_resolved = children.iter().all(|c| {
        let s = effective_status(c, overrides);
        *s == Status::Done || *s == Status::Cancelled
    });

    if has_children && all_resolved {
        let mut closed_parent = parent.clone();
        closed_parent.status = Status::Done;
        closed_parent.updated = Utc::now();
        store::save(base, &closed_parent)?;
        overrides.insert(parent_id.clone(), Status::Done);

        // Cascade: re-run the check for the newly-closed epic's own parent.
        maybe_close_parent_epic(base, tasks, parent, overrides)?;
    }

    Ok(())
}

/// Add a dependency with cycle detection. Both IDs support prefix matching.
pub fn add_dependency(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    dep_or_prefix: &str,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let depends_on = store::resolve_prefix(tasks, dep_or_prefix)?;

    let graph = Graph::build(tasks);
    if graph.would_cycle(&id, &depends_on) {
        return Err(Error::CycleDetected {
            from: id,
            to: depends_on,
        });
    }

    let mut t = tasks[&id].clone();
    if !t.depends_on.contains(&depends_on) {
        t.depends_on.push(depends_on);
        t.updated = Utc::now();
        store::save(base, &t)?;
    }

    Ok(t)
}

/// Remove a dependency. Both IDs support prefix matching.
pub fn remove_dependency(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    dep_or_prefix: &str,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let depends_on = store::resolve_prefix(tasks, dep_or_prefix)?;
    let mut t = tasks[&id].clone();
    t.depends_on.retain(|d| d != &depends_on);
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(t)
}

/// Search tasks by text query.
pub fn search_tasks(tasks: &HashMap<String, Task>, query: &str, include_all: bool) -> Vec<Task> {
    let query_lower = query.to_lowercase();
    let mut results: Vec<Task> = tasks
        .values()
        .filter(|t| include_all || task::is_active(t))
        .filter(|t| {
            t.title.to_lowercase().contains(&query_lower)
                || t.body.to_lowercase().contains(&query_lower)
                || t.tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query_lower))
                || t.id.contains(&query_lower)
        })
        .cloned()
        .collect();
    task::sort_by_priority_owned(&mut results);
    results
}

/// Delete a task by ID or prefix, returning the deleted task.
pub fn delete_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let t = tasks[&id].clone();
    store::delete(base, &id)?;
    Ok(t)
}

/// Prune cancelled (and optionally done) tasks, returning deleted tasks.
pub fn prune_tasks(
    base: &Path,
    tasks: &HashMap<String, Task>,
    include_done: bool,
) -> Result<Vec<Task>> {
    let to_delete: Vec<Task> = tasks
        .values()
        .filter(|t| t.status == Status::Cancelled || (include_done && t.status == Status::Done))
        .cloned()
        .collect();

    for t in &to_delete {
        store::delete(base, &t.id)?;
    }
    Ok(to_delete)
}

/// Build the dependency graph from tasks.
pub fn build_graph(tasks: &HashMap<String, Task>) -> Graph {
    Graph::build(tasks)
}

/// Compute effective priorities for all tasks in a single O(V+E) pass.
pub fn effective_priorities(tasks: &HashMap<String, Task>) -> HashMap<String, Priority> {
    Graph::build(tasks).effective_priorities_all(tasks)
}

/// Progress of an epic: how many children are done vs total.
#[derive(Debug, Clone, Serialize)]
pub struct EpicProgress {
    pub done: usize,
    pub total: usize,
}

/// Compact epic projection used by the epics command.
#[derive(Debug, Serialize)]
pub struct EpicSummary {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub progress: EpicProgress,
}

/// Compute progress for an epic by counting children (tasks with parent == epic_id).
///
/// Semantics: cancelled children are treated as resolved and non-blocking.
/// - `total` = non-cancelled children (active workload)
/// - `done`  = Done children
///
/// A fully-resolved epic (all children Done or Cancelled) satisfies `done == total`
/// because cancelled children contribute to neither count.
pub fn epic_progress(tasks: &HashMap<String, Task>, epic_id: &str) -> EpicProgress {
    let mut done = 0;
    let mut total = 0;
    for t in tasks.values() {
        if t.parent.as_deref() == Some(epic_id) {
            if t.status == Status::Cancelled {
                // Cancelled = resolved but not counted in the active workload.
                continue;
            }
            total += 1;
            if t.status == Status::Done {
                done += 1;
            }
        }
    }
    EpicProgress { done, total }
}

/// Return children of a parent task in topological execution order.
/// Works for any task with children, not restricted to epics.
pub fn plan_epic<'a>(tasks: &'a HashMap<String, Task>, parent_id: &str) -> Result<Vec<&'a Task>> {
    // Validate parent exists and is an epic
    let resolved = store::resolve_prefix(tasks, parent_id)?;
    let parent = tasks
        .get(&resolved)
        .ok_or_else(|| Error::TaskNotFound(parent_id.to_string()))?;
    if !parent.task_type.is_epic() {
        return Err(Error::NotAnEpic(resolved));
    }

    // Collect child IDs
    let child_ids: HashSet<String> = tasks
        .values()
        .filter(|t| t.parent.as_deref() == Some(resolved.as_str()))
        .map(|t| t.id.clone())
        .collect();

    if child_ids.is_empty() {
        return Ok(Vec::new());
    }

    let graph = Graph::build(tasks);
    Ok(graph.topo_sort_subset(&child_ids, tasks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, status: Status) -> Task {
        let mut t = Task::new(id.to_string(), format!("Task {id}"), Priority::P2);
        t.status = status;
        t
    }

    fn make_epic(id: &str) -> Task {
        let mut t = Task::new(id.to_string(), format!("Epic {id}"), Priority::P1);
        t.task_type = TaskType::Epic;
        t
    }

    fn make_child(id: &str, parent: &str, status: Status) -> Task {
        let mut t = make_task(id, status);
        t.parent = Some(parent.to_string());
        t
    }

    fn task_map(tasks: Vec<Task>) -> HashMap<String, Task> {
        tasks.into_iter().map(|t| (t.id.clone(), t)).collect()
    }

    #[test]
    fn test_epic_progress_no_children() {
        let tasks = task_map(vec![make_epic("e1")]);
        let p = epic_progress(&tasks, "e1");
        assert_eq!(p.done, 0);
        assert_eq!(p.total, 0);
    }

    #[test]
    fn test_epic_progress_mixed() {
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Done),
            make_child("c2", "e1", Status::Open),
            make_child("c3", "e1", Status::InProgress),
        ]);
        let p = epic_progress(&tasks, "e1");
        assert_eq!(p.done, 1);
        assert_eq!(p.total, 3);
    }

    #[test]
    fn test_epic_progress_all_done() {
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Done),
            make_child("c2", "e1", Status::Done),
        ]);
        let p = epic_progress(&tasks, "e1");
        assert_eq!(p.done, 2);
        assert_eq!(p.total, 2);
    }

    #[test]
    fn test_epic_progress_cancelled_excluded_from_total() {
        // Cancelled children are non-blocking: excluded from total, not counted in done.
        // A fully-resolved epic (done + cancelled) shows done == total.
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Done),
            make_child("c2", "e1", Status::Cancelled),
        ]);
        let p = epic_progress(&tasks, "e1");
        assert_eq!(p.done, 1);
        assert_eq!(p.total, 1); // cancelled child excluded
    }

    #[test]
    fn test_epic_progress_mixed_with_cancelled() {
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Done),
            make_child("c2", "e1", Status::Open),
            make_child("c3", "e1", Status::Cancelled),
        ]);
        let p = epic_progress(&tasks, "e1");
        assert_eq!(p.done, 1);
        assert_eq!(p.total, 2); // cancelled child excluded
    }

    #[tokio::test]
    async fn test_epic_auto_close_with_done_and_cancelled() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "My Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            "Child 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child2 = create_task(
            tmp.path(),
            &tasks,
            "Child 2".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Done + Cancelled = all resolved → epic should auto-close
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Open);

        // Cancel the last child — should trigger auto-close
        set_status(tmp.path(), &tasks, &child2.id, Status::Cancelled).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&epic.id].status,
            Status::Done,
            "epic should auto-close when children are [done, cancelled]"
        );
    }

    #[tokio::test]
    async fn test_epic_auto_close_cancel_last_open_child() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "My Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            "Child 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Cancelling the only/last open child must trigger auto-close
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Cancelled).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&epic.id].status,
            Status::Done,
            "epic should auto-close when cancelling the last open child"
        );
    }

    #[tokio::test]
    async fn test_epic_auto_close() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "My Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            "Child 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child2 = create_task(
            tmp.path(),
            &tasks,
            "Child 2".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Complete first child — epic stays open
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Open);

        // Complete second child — epic auto-closes
        set_status(tmp.path(), &tasks, &child2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Done);
    }

    #[tokio::test]
    async fn test_epic_auto_close_via_update_task() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "My Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            "Child 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child2 = create_task(
            tmp.path(),
            &tasks,
            "Child 2".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Complete first child via update_task — epic stays open
        let tasks = store::load_all(tmp.path()).await.unwrap();
        update_task(
            tmp.path(),
            &tasks,
            &child1.id,
            Some(Status::Done),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Open);

        // Complete second child via update_task — epic auto-closes
        update_task(
            tmp.path(),
            &tasks,
            &child2.id,
            Some(Status::Done),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Done);
    }

    #[tokio::test]
    async fn test_epic_no_over_close_on_re_complete() {
        // Regression: re-completing an already-done child must NOT auto-close the epic
        // when another child is still open.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "My Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            "Child 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let _child2 = create_task(
            tmp.path(),
            &tasks,
            "Child 2".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Complete child1 for the first time
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&epic.id].status,
            Status::Open,
            "epic should stay open"
        );

        // Re-complete child1 (already done) — child2 is still open, epic must NOT close
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&epic.id].status,
            Status::Open,
            "epic must not close when re-completing an already-done child while another is open"
        );
    }

    #[tokio::test]
    async fn test_epic_cascade_auto_close_nested() {
        // Verify that closing the last leaf cascades up through ≥2 epic levels.
        //
        // Structure:
        //   outer_epic
        //     └─ inner_epic
        //          ├─ leaf1 (will be Done first)
        //          └─ leaf2 (completing this triggers the cascade)
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let outer = create_task(
            tmp.path(),
            &tasks,
            "Outer Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let inner = create_task(
            tmp.path(),
            &tasks,
            "Inner Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            Some(outer.id.clone()),
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let leaf1 = create_task(
            tmp.path(),
            &tasks,
            "Leaf 1".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(inner.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let leaf2 = create_task(
            tmp.path(),
            &tasks,
            "Leaf 2".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(inner.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Complete leaf1 — nothing should close yet
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &leaf1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&inner.id].status,
            Status::Open,
            "inner should stay open"
        );
        assert_eq!(
            tasks[&outer.id].status,
            Status::Open,
            "outer should stay open"
        );

        // Complete leaf2 — inner_epic should auto-close, then outer_epic should cascade-close
        set_status(tmp.path(), &tasks, &leaf2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks[&inner.id].status,
            Status::Done,
            "inner epic should auto-close when all its children are done"
        );
        assert_eq!(
            tasks[&outer.id].status,
            Status::Done,
            "outer epic should cascade-close when inner epic closes"
        );
    }

    #[test]
    fn test_plan_epic_linear_chain() {
        let mut c1 = make_child("c1", "e1", Status::Open);
        c1.depends_on = vec![];
        let mut c2 = make_child("c2", "e1", Status::Open);
        c2.depends_on = vec!["c1".to_string()];
        let mut c3 = make_child("c3", "e1", Status::Open);
        c3.depends_on = vec!["c2".to_string()];

        let tasks = task_map(vec![make_epic("e1"), c1, c2, c3]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        let ids: Vec<&str> = plan.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["c1", "c2", "c3"]);
    }

    #[test]
    fn test_plan_epic_independent_children() {
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Open),
            make_child("c2", "e1", Status::Open),
        ]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn test_plan_epic_no_children() {
        let tasks = task_map(vec![make_epic("e1")]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn test_plan_epic_not_found() {
        let tasks = task_map(vec![]);
        let result = plan_epic(&tasks, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_epic_non_epic_parent() {
        // plan_epic rejects non-epic parents
        let parent = make_task("p1", Status::Open);
        let tasks = task_map(vec![
            parent,
            make_child("c1", "p1", Status::Open),
            make_child("c2", "p1", Status::Done),
        ]);
        let result = plan_epic(&tasks, "p1");
        assert!(result.is_err());
    }
}
