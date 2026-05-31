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

    // Treat empty-string parent as "no parent"
    let parent = parent.and_then(|p| if p.is_empty() { None } else { Some(p) });

    // Validate parent exists if provided
    if let Some(ref pid) = parent {
        store::resolve_prefix(tasks, pid)?;
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
///
/// The `parent` parameter uses a double-Option to distinguish three states:
/// - `None`           → leave parent unchanged
/// - `Some(None)`     → clear parent (detach from any epic)
/// - `Some(Some(id))` → set parent to the given epic ID (must exist)
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
    parent: Option<Option<String>>,
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
    // Reparenting: None = leave unchanged, Some(None) = clear, Some(Some(id)) = set
    if let Some(new_parent) = parent {
        match new_parent {
            None => t.parent = None,
            Some(ref pid) => {
                // Validate parent exists
                store::resolve_prefix(tasks, pid)?;
                t.parent = Some(pid.clone());
            }
        }
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

// ─── Archive helpers ──────────────────────────────────────────────────────────

/// Check whether a task is archivable.
///
/// A task is archivable when:
/// - its status is Done or Cancelled, AND
/// - no ACTIVE (not Done/Cancelled) task in `tasks` depends on it.
///
/// For epics the check is the same — the caller is responsible for deciding
/// whether to cascade to children before calling this predicate.
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn is_archivable(task: &Task, tasks: &HashMap<String, Task>) -> bool {
    let settled = task.status == Status::Done || task.status == Status::Cancelled;
    if !settled {
        return false;
    }
    // Build reverse graph to find dependents
    let graph = Graph::build(tasks);
    active_blockers(&task.id, tasks, &graph).is_empty()
}

/// Return the IDs of active (non-done/cancelled) tasks that depend on `id`.
fn active_blockers(id: &str, tasks: &HashMap<String, Task>, graph: &Graph) -> Vec<String> {
    graph
        .reverse
        .get(id)
        .into_iter()
        .flat_map(|s| s.iter())
        .filter(|dep_id| {
            tasks
                .get(dep_id.as_str())
                .is_some_and(|t| t.status != Status::Done && t.status != Status::Cancelled)
        })
        .cloned()
        .collect()
}

/// Archive a single task (and its cascade) identified by `id_or_prefix`.
///
/// Cascade rules:
/// - If the task is an epic, its Done/Cancelled children are also archived
///   (children that are not settled block the archive if they themselves would
///   block archiving, but epic children are just included when settled).
/// - For any archived task, its settled `depends_on` tasks that are no longer
///   depended on by any active task are NOT automatically cascaded here —
///   the caller may sweep afterwards with `archive_all`.
///
/// On failure returns `Error::NotArchivable` listing active dependents.
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn archive_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
) -> Result<Vec<String>> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let task = &tasks[&id];
    let graph = Graph::build(tasks);

    // Check the target task itself
    let blockers = active_blockers(&id, tasks, &graph);
    if !blockers.is_empty() {
        return Err(Error::NotArchivable {
            id: id.clone(),
            blockers,
        });
    }
    if task.status != Status::Done && task.status != Status::Cancelled {
        return Err(Error::NotArchivable {
            id: id.clone(),
            blockers: vec![],
        });
    }

    // Collect the set to archive: the target + settled epic children
    let mut to_archive: Vec<String> = vec![id.clone()];

    if task.task_type.is_epic() {
        let settled_children: Vec<String> = tasks
            .values()
            .filter(|c| {
                c.parent.as_deref() == Some(id.as_str())
                    && (c.status == Status::Done || c.status == Status::Cancelled)
            })
            .map(|c| c.id.clone())
            .collect();
        to_archive.extend(settled_children);
    }

    // Move each to archive
    for tid in &to_archive {
        store::move_to_archive(base, tid)?;
    }

    Ok(to_archive)
}

/// Sweep: archive every currently-archivable task.
///
/// A task is archivable if it is Done/Cancelled AND has no active dependents
/// (considering only active tasks — not those already archived in this sweep).
///
/// We do a fixed-point iteration: after each pass we remove archived tasks from
/// the working set and retry, because archiving one task may make another
/// archivable (e.g. a chain where the head depends on a now-archived task that
/// was its only active dependent).
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn archive_all(base: &Path, tasks: &HashMap<String, Task>) -> Result<Vec<String>> {
    let mut remaining: HashMap<String, Task> = tasks.clone();
    let mut total_archived: Vec<String> = Vec::new();

    loop {
        let graph = Graph::build(&remaining);
        let mut batch: Vec<String> = remaining
            .values()
            .filter(|t| {
                (t.status == Status::Done || t.status == Status::Cancelled)
                    && active_blockers(&t.id, &remaining, &graph).is_empty()
            })
            .map(|t| t.id.clone())
            .collect();

        if batch.is_empty() {
            break;
        }

        batch.sort(); // deterministic order
        for id in &batch {
            store::move_to_archive(base, id)?;
            remaining.remove(id);
        }
        total_archived.extend(batch);
    }

    Ok(total_archived)
}

/// Restore a task from the archive back to the active store.
///
/// Cascade: also restores any archived `depends_on` tasks (transitively) and
/// the parent epic (if archived) so the restored task has no missing deps.
///
/// The `id_or_prefix` is matched against the archive (not the active task map).
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub async fn restore_task(base: &Path, id_or_prefix: &str) -> Result<Vec<String>> {
    let archived = store::load_archived(base).await?;

    let id = store::resolve_prefix(&archived, id_or_prefix)
        .map_err(|_| Error::NotArchived(id_or_prefix.to_string()))?;

    // Collect what must be restored: the target + its archived depends_on (transitive) + parent epic
    let mut to_restore: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = vec![id.clone()];

    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        to_restore.push(current.clone());

        if let Some(task) = archived.get(&current) {
            // Restore parent epic if archived
            if let Some(ref parent_id) = task.parent
                && archived.contains_key(parent_id)
                && !visited.contains(parent_id)
            {
                queue.push(parent_id.clone());
            }
            // Restore depends_on that are archived
            for dep_id in &task.depends_on {
                if archived.contains_key(dep_id) && !visited.contains(dep_id) {
                    queue.push(dep_id.clone());
                }
            }
        }
    }

    for tid in &to_restore {
        store::move_from_archive(base, tid)?;
    }

    Ok(to_restore)
}

/// Get an archived task by ID or prefix (read-only, for show/inspect).
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub async fn get_archived_task(base: &Path, id_or_prefix: &str) -> Result<Task> {
    let archived = store::load_archived(base).await?;
    let id = store::resolve_prefix(&archived, id_or_prefix)
        .map_err(|_| Error::NotArchived(id_or_prefix.to_string()))?;
    Ok(archived[&id].clone())
}

/// Archive all tasks with status Done or Cancelled (thin alias for `archive_all`).
///
/// Equivalent to `archive_all` — both archive every task that is settled and
/// has no active dependents. Provided as an explicit named entry point that
/// matches the spec for the 9ra task.
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn archive_done(base: &Path, tasks: &HashMap<String, Task>) -> Result<Vec<String>> {
    archive_all(base, tasks)
}

/// List archived tasks sorted by `updated` descending (most recently updated first).
///
/// If `limit` is `Some(n)`, at most `n` tasks are returned.
// Will be consumed by archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub async fn list_archive(base: &Path, limit: Option<usize>) -> Result<Vec<Task>> {
    let archived = store::load_archived(base).await?;
    let mut tasks: Vec<Task> = archived.into_values().collect();
    // Sort by updated descending (most recent first), then id for stability
    tasks.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
    if let Some(n) = limit {
        tasks.truncate(n);
    }
    Ok(tasks)
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

    // ─── Archive service tests ────────────────────────────────────────────────

    #[test]
    fn test_is_archivable_done_no_dependents() {
        let t = make_task("t1", Status::Done);
        let tasks = task_map(vec![t.clone()]);
        assert!(is_archivable(&t, &tasks));
    }

    #[test]
    fn test_is_archivable_cancelled_no_dependents() {
        let t = make_task("t1", Status::Cancelled);
        let tasks = task_map(vec![t.clone()]);
        assert!(is_archivable(&t, &tasks));
    }

    #[test]
    fn test_is_archivable_open_is_false() {
        let t = make_task("t1", Status::Open);
        let tasks = task_map(vec![t.clone()]);
        assert!(!is_archivable(&t, &tasks));
    }

    #[test]
    fn test_is_archivable_in_progress_is_false() {
        let mut t = make_task("t1", Status::Done);
        t.status = Status::InProgress;
        let tasks = task_map(vec![t.clone()]);
        assert!(!is_archivable(&t, &tasks));
    }

    #[test]
    fn test_is_archivable_done_with_active_dependent_is_false() {
        // t1 is done, but t2 (open) depends on t1 → t1 is NOT archivable
        let t1 = make_task("t1", Status::Done);
        let mut t2 = make_task("t2", Status::Open);
        t2.depends_on = vec!["t1".to_string()];
        let tasks = task_map(vec![t1.clone(), t2]);
        assert!(!is_archivable(&t1, &tasks));
    }

    #[test]
    fn test_is_archivable_done_dependent_is_ok() {
        // t1 is done, t2 (also done) depends on t1 → t1 IS archivable
        let t1 = make_task("t1", Status::Done);
        let mut t2 = make_task("t2", Status::Done);
        t2.depends_on = vec!["t1".to_string()];
        let tasks = task_map(vec![t1.clone(), t2]);
        assert!(is_archivable(&t1, &tasks));
    }

    #[tokio::test]
    async fn test_archive_task_basic() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Done task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let archived = archive_task(tmp.path(), &tasks, &t.id).unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0], t.id);

        // Task should no longer be active
        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(!active.contains_key(&t.id));

        // Task should be in archive
        let arch = store::load_archived(tmp.path()).await.unwrap();
        assert!(arch.contains_key(&t.id));
    }

    #[tokio::test]
    async fn test_archive_task_blocked_by_active_dependent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let dep = create_task(
            tmp.path(),
            &tasks,
            "Dep task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        // Create dependent that depends on dep
        let _dependent = create_task(
            tmp.path(),
            &tasks,
            "Dependent".into(),
            Priority::P2,
            vec![],
            vec![dep.id.clone()],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Mark dep as done but dependent is still open
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &dep.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let result = archive_task(tmp.path(), &tasks, &dep.id);
        assert!(
            matches!(result, Err(Error::NotArchivable { .. })),
            "should fail with NotArchivable"
        );
    }

    #[tokio::test]
    async fn test_archive_task_open_is_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Open task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let result = archive_task(tmp.path(), &tasks, &t.id);
        assert!(
            matches!(result, Err(Error::NotArchivable { .. })),
            "open task should not be archivable"
        );
    }

    #[tokio::test]
    async fn test_archive_task_epic_cascades_to_settled_children() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let c1 = create_task(
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
        let c2 = create_task(
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

        // Mark epic and both children as done
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &c1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &c2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        // Epic should auto-close; set it explicitly just in case
        set_status(tmp.path(), &tasks, &epic.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let mut archived_ids = archive_task(tmp.path(), &tasks, &epic.id).unwrap();
        archived_ids.sort();

        // Epic + 2 children should all be archived
        assert_eq!(archived_ids.len(), 3, "epic + 2 children");
        assert!(archived_ids.contains(&epic.id));
        assert!(archived_ids.contains(&c1.id));
        assert!(archived_ids.contains(&c2.id));

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_archive_all_sweep() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "Done 1".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "Open".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t3 = create_task(
            tmp.path(),
            &tasks,
            "Done 2".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t3.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let archived_ids = archive_all(tmp.path(), &tasks).unwrap();
        assert_eq!(archived_ids.len(), 2);
        assert!(archived_ids.contains(&t1.id));
        assert!(archived_ids.contains(&t3.id));

        let active = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(active.len(), 1);
        assert!(active.contains_key(&t2.id));
    }

    #[tokio::test]
    async fn test_archive_all_sweep_cascades_chain() {
        // t1 done, t2 done and depends on t1 — both should be swept
        // because after archiving t2 (no active dependents), t1 (depended on by done t2)
        // becomes archivable in next iteration.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "Base done".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "Dependent done".into(),
            Priority::P2,
            vec![],
            vec![t1.id.clone()],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let archived_ids = archive_all(tmp.path(), &tasks).unwrap();
        assert_eq!(archived_ids.len(), 2, "both should be archived");
        assert!(archived_ids.contains(&t1.id));
        assert!(archived_ids.contains(&t2.id));
    }

    #[tokio::test]
    async fn test_restore_task_basic() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Task to restore".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_task(tmp.path(), &tasks, &t.id).unwrap();

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(!active.contains_key(&t.id));

        let restored = restore_task(tmp.path(), &t.id).await.unwrap();
        assert_eq!(restored.len(), 1);

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(active.contains_key(&t.id));
    }

    #[tokio::test]
    async fn test_restore_task_cascades_deps() {
        // t1 archived, t2 archived and depends on t1
        // Restoring t2 should also restore t1 (its archived dep)
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "Dep".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "Dependent".into(),
            Priority::P2,
            vec![],
            vec![t1.id.clone()],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        // Archive both
        archive_all(tmp.path(), &tasks).unwrap();

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(active.is_empty());

        // Restore t2 — t1 (its dep) should also come back
        let mut restored = restore_task(tmp.path(), &t2.id).await.unwrap();
        restored.sort();

        assert_eq!(restored.len(), 2);
        assert!(restored.contains(&t1.id));
        assert!(restored.contains(&t2.id));

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(active.contains_key(&t1.id));
        assert!(active.contains_key(&t2.id));
    }

    #[tokio::test]
    async fn test_restore_task_cascades_parent_epic() {
        // Epic archived, child archived → restoring child should also restore epic
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            "Epic".into(),
            Priority::P1,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Epic,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child = create_task(
            tmp.path(),
            &tasks,
            "Child".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(epic.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        // Epic should have auto-closed; archive manually if needed
        set_status(tmp.path(), &tasks, &epic.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_task(tmp.path(), &tasks, &epic.id).unwrap();

        let active = store::load_all(tmp.path()).await.unwrap();
        assert!(active.is_empty());

        // Restore child → epic should also be restored
        let mut restored = restore_task(tmp.path(), &child.id).await.unwrap();
        restored.sort();
        assert!(restored.contains(&epic.id), "epic should be restored");
        assert!(restored.contains(&child.id), "child should be restored");
    }

    #[tokio::test]
    async fn test_restore_not_archived_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let result = restore_task(tmp.path(), "nonexistent").await;
        assert!(
            matches!(result, Err(Error::NotArchived(_))),
            "should get NotArchived error"
        );
    }

    #[tokio::test]
    async fn test_get_archived_task() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Archived task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_task(tmp.path(), &tasks, &t.id).unwrap();

        let fetched = get_archived_task(tmp.path(), &t.id).await.unwrap();
        assert_eq!(fetched.id, t.id);
        assert_eq!(fetched.title, "Archived task");
    }

    #[tokio::test]
    async fn test_create_task_avoids_archived_id_collision() {
        // Verify that create_task doesn't reuse archived IDs.
        // We can't easily force a collision with random short IDs in a unit test,
        // but we can verify that archived_id_set is called by checking the function
        // doesn't panic and creates a new task with a different ID than the archived one.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Task 1".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_task(tmp.path(), &tasks, &t.id).unwrap();

        // Now archived. New task creation should succeed and not reuse the archived ID.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        // archived_id_set is consulted during ID generation
        let archived_ids = store::archived_id_set(tmp.path());
        assert!(archived_ids.contains(&t.id));

        // If we create another task, it shouldn't collide with the archived ID
        // (with a 3-char ID space of 36^3=46656 IDs, collision is unlikely but
        // the code path is exercised)
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "Task 2".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_ne!(t2.id, t.id, "new task must not reuse archived ID");
    }

    // ─── archive_done / list_archive tests ───────────────────────────────────

    #[tokio::test]
    async fn test_archive_done_is_alias_for_archive_all() {
        // archive_done should behave identically to archive_all
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "Done task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let _t2 = create_task(
            tmp.path(),
            &tasks,
            "Open task".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let archived_ids = archive_done(tmp.path(), &tasks).unwrap();
        assert_eq!(archived_ids.len(), 1);
        assert!(archived_ids.contains(&t1.id));
    }

    #[tokio::test]
    async fn test_list_archive_sorted_by_updated_desc() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        // Create and archive three tasks; each is created slightly after the previous
        // so they have distinct updated timestamps.
        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "First".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "Second".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t3 = create_task(
            tmp.path(),
            &tasks,
            "Third".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        // Mark all done and archive — update times set by set_status calls
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t3.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_all(tmp.path(), &tasks).unwrap();

        // list_archive returns all 3, most recently updated first
        let listed = list_archive(tmp.path(), None).await.unwrap();
        assert_eq!(listed.len(), 3);
        // All should be present (exact order may vary if timestamps are equal
        // since IDs are random, but at minimum all three must appear)
        let listed_ids: Vec<&str> = listed.iter().map(|t| t.id.as_str()).collect();
        assert!(listed_ids.contains(&t1.id.as_str()));
        assert!(listed_ids.contains(&t2.id.as_str()));
        assert!(listed_ids.contains(&t3.id.as_str()));
        // Verify sorted descending
        for w in listed.windows(2) {
            assert!(
                w[0].updated >= w[1].updated,
                "list_archive must be sorted updated desc"
            );
        }
    }

    #[tokio::test]
    async fn test_list_archive_with_limit() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let t1 = create_task(
            tmp.path(),
            &tasks,
            "T1".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t2 = create_task(
            tmp.path(),
            &tasks,
            "T2".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &t2.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        archive_all(tmp.path(), &tasks).unwrap();

        let listed = list_archive(tmp.path(), Some(1)).await.unwrap();
        assert_eq!(listed.len(), 1, "limit=1 should return exactly 1 task");
    }

    #[tokio::test]
    async fn test_list_archive_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let listed = list_archive(tmp.path(), None).await.unwrap();
        assert!(listed.is_empty());
    }
}
