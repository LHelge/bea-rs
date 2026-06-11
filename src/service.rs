use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use serde::Serialize;

use crate::config;
use crate::error::{Error, Result};
use crate::graph::Graph;
use crate::store;
use crate::task::{self, Priority, Status, Task, TaskType};

/// Fields for creating a task. Build with [`NewTask::new`] and struct-update
/// syntax for the non-default fields.
pub struct NewTask {
    pub title: String,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub depends_on: Vec<String>,
    pub parent: Option<String>,
    pub body: String,
    pub task_type: TaskType,
}

impl NewTask {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            priority: Priority::P2,
            tags: Vec::new(),
            depends_on: Vec::new(),
            parent: None,
            body: String::new(),
            task_type: TaskType::Task,
        }
    }
}

/// Create a new task with validation.
pub fn create_task(base: &Path, tasks: &HashMap<String, Task>, new: NewTask) -> Result<Task> {
    let config = config::load(base)?;
    let existing_ids: HashSet<String> = tasks.keys().cloned().collect();
    let id = task::generate_id(&existing_ids, config.id_length as usize);

    // Resolve dependency IDs (prefixes allowed, like every other command)
    let mut resolved_deps = Vec::with_capacity(new.depends_on.len());
    let mut unknown = Vec::new();
    for dep in new.depends_on {
        match store::resolve_prefix(tasks, &dep) {
            Ok(dep_id) => resolved_deps.push(dep_id),
            Err(Error::TaskNotFound(_)) => unknown.push(dep),
            Err(e) => return Err(e),
        }
    }
    if !unknown.is_empty() {
        return Err(Error::UnknownDependency { ids: unknown });
    }

    // Resolve and validate the parent: must exist and be an epic
    let parent = match new.parent.filter(|p| !p.is_empty()) {
        None => None,
        Some(p) => {
            let parent_id = store::resolve_prefix(tasks, &p)?;
            if !tasks[&parent_id].task_type.is_epic() {
                return Err(Error::ParentNotEpic(parent_id));
            }
            Some(parent_id)
        }
    };

    let mut t = Task::new(id, new.title, new.priority);
    t.task_type = new.task_type;
    t.tags = new.tags;
    t.depends_on = resolved_deps;
    t.parent = parent;
    t.body = new.body;

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

/// Partial update: only `Some` fields are applied.
#[derive(Default)]
pub struct UpdateFields {
    pub status: Option<Status>,
    pub priority: Option<Priority>,
    pub tags: Option<Vec<String>>,
    pub assignee: Option<String>,
    pub body: Option<String>,
    pub title: Option<String>,
}

/// Update task fields. Only `Some` fields are changed.
pub fn update_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    fields: UpdateFields,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();

    let status_changed = fields.status.is_some();
    if let Some(s) = fields.status {
        t.status = s;
    }
    if let Some(p) = fields.priority {
        t.priority = p;
    }
    if let Some(tags) = fields.tags {
        t.tags = tags;
    }
    if let Some(a) = fields.assignee {
        t.assignee = a;
    }
    if let Some(b) = fields.body {
        t.body = b;
    }
    if let Some(title) = fields.title {
        t.title = title;
    }
    t.updated = Utc::now();

    store::save(base, &t)?;
    if status_changed {
        maybe_close_parent_epic(base, tasks, &t)?;
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
    maybe_close_parent_epic(base, tasks, &t)?;
    Ok(t)
}

/// Auto-close the parent epic when all of its children are done.
///
/// `tasks` is the pre-save snapshot, so `child` is counted as done explicitly
/// regardless of the (possibly stale) status the snapshot holds for it.
fn maybe_close_parent_epic(base: &Path, tasks: &HashMap<String, Task>, child: &Task) -> Result<()> {
    if child.status != Status::Done {
        return Ok(());
    }
    let Some(parent_id) = child.parent.as_deref() else {
        return Ok(());
    };
    let Some(parent) = tasks.get(parent_id) else {
        return Ok(());
    };
    if !parent.task_type.is_epic() || parent.status == Status::Done {
        return Ok(());
    }

    let mut done = 0;
    let mut total = 0;
    for t in tasks.values() {
        if t.parent.as_deref() == Some(parent_id) {
            total += 1;
            if t.id == child.id || t.status == Status::Done {
                done += 1;
            }
        }
    }
    if total > 0 && done >= total {
        let mut parent = parent.clone();
        parent.status = Status::Done;
        parent.updated = Utc::now();
        store::save(base, &parent)?;
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
/// References to the deleted task are removed from remaining tasks.
pub fn delete_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let t = tasks[&id].clone();
    store::delete(base, &id)?;
    scrub_references(base, tasks, &HashSet::from([id]))?;
    Ok(t)
}

/// Prune cancelled (and optionally done) tasks, returning deleted tasks.
/// References to pruned tasks are removed from remaining tasks.
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
    let deleted_ids: HashSet<String> = to_delete.iter().map(|t| t.id.clone()).collect();
    scrub_references(base, tasks, &deleted_ids)?;
    Ok(to_delete)
}

/// Remove dangling references to deleted tasks: drop deleted IDs from
/// `depends_on` lists and clear `parent` fields pointing at deleted tasks.
/// Without this, dependents would silently never become ready.
fn scrub_references(
    base: &Path,
    tasks: &HashMap<String, Task>,
    deleted: &HashSet<String>,
) -> Result<()> {
    for t in tasks.values() {
        if deleted.contains(&t.id) {
            continue;
        }
        let dangling_dep = t.depends_on.iter().any(|d| deleted.contains(d));
        let dangling_parent = t.parent.as_ref().is_some_and(|p| deleted.contains(p));
        if dangling_dep || dangling_parent {
            let mut t = t.clone();
            t.depends_on.retain(|d| !deleted.contains(d));
            if dangling_parent {
                t.parent = None;
            }
            t.updated = Utc::now();
            store::save(base, &t)?;
        }
    }
    Ok(())
}

/// Build the dependency graph from tasks.
pub fn build_graph(tasks: &HashMap<String, Task>) -> Graph {
    Graph::build(tasks)
}

/// Compute effective priorities for all tasks.
pub fn effective_priorities(tasks: &HashMap<String, Task>) -> HashMap<String, Priority> {
    Graph::build(tasks).effective_priorities(tasks)
}

/// List all epics, sorted by priority then creation date.
pub fn list_epics(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    let mut epics: Vec<&Task> = tasks.values().filter(|t| t.task_type.is_epic()).collect();
    task::sort_refs_by_priority(&mut epics);
    epics
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
pub fn epic_progress(tasks: &HashMap<String, Task>, epic_id: &str) -> EpicProgress {
    let mut done = 0;
    let mut total = 0;
    for t in tasks.values() {
        if t.parent.as_deref() == Some(epic_id) {
            total += 1;
            if t.status == Status::Done {
                done += 1;
            }
        }
    }
    EpicProgress { done, total }
}

/// Execution plan for an epic's children.
pub struct EpicPlan<'a> {
    /// Children in topological execution order.
    pub tasks: Vec<&'a Task>,
    /// Children that cannot be ordered because they are in a dependency cycle.
    pub cyclic: Vec<&'a Task>,
}

/// Return children of an epic in topological execution order.
/// Children caught in a dependency cycle are reported separately.
pub fn plan_epic<'a>(tasks: &'a HashMap<String, Task>, parent_id: &str) -> Result<EpicPlan<'a>> {
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

    let graph = Graph::build(tasks);
    let topo = graph.topo_sort_subset(&child_ids, tasks);
    Ok(EpicPlan {
        tasks: topo.sorted,
        cyclic: topo.cyclic,
    })
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

    #[tokio::test]
    async fn test_epic_auto_close() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let tasks = HashMap::new();
        let epic = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P1,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Epic,
                ..NewTask::new("My Epic")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(epic.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Child 1")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child2 = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(epic.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Child 2")
            },
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
    async fn test_create_task_rejects_unknown_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let result = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some("zzzz".into()),
                task_type: TaskType::Task,
                ..NewTask::new("Orphan")
            },
        );
        assert!(matches!(result, Err(Error::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_task_rejects_non_epic_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let plain = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Task,
                ..NewTask::new("Plain task")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let result = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(plain.id),
                task_type: TaskType::Task,
                ..NewTask::new("Child")
            },
        );
        assert!(matches!(result, Err(Error::ParentNotEpic(_))));
    }

    #[tokio::test]
    async fn test_create_task_resolves_prefixes() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let dep = Task::new("abcd".into(), "Dep".into(), Priority::P2);
        store::save(tmp.path(), &dep).unwrap();
        let mut epic = Task::new("wxyz".into(), "Epic".into(), Priority::P1);
        epic.task_type = TaskType::Epic;
        store::save(tmp.path(), &epic).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec!["ab".into()],
                parent: Some("wx".into()),
                task_type: TaskType::Task,
                ..NewTask::new("Uses prefixes")
            },
        )
        .unwrap();
        assert_eq!(t.depends_on, vec!["abcd"]);
        assert_eq!(t.parent.as_deref(), Some("wxyz"));
    }

    #[tokio::test]
    async fn test_create_task_empty_parent_is_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let t = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(String::new()),
                task_type: TaskType::Task,
                ..NewTask::new("No parent")
            },
        )
        .unwrap();
        assert_eq!(t.parent, None);
    }

    #[tokio::test]
    async fn test_epic_not_closed_by_recompleting_done_child() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let epic = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P1,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Epic,
                ..NewTask::new("Epic")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child1 = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(epic.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Child 1")
            },
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let _child2 = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(epic.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Child 2")
            },
        )
        .unwrap();

        // Complete child1, then complete it AGAIN — epic must stay open
        // because child2 is still open.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &child1.id, Status::Done).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Open);
    }

    #[tokio::test]
    async fn test_epic_auto_close_via_update_task() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let epic = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P1,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Epic,
                ..NewTask::new("Epic")
            },
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let child = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(epic.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Only child")
            },
        )
        .unwrap();

        // Complete the child through update_task — must auto-close like set_status.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        update_task(
            tmp.path(),
            &tasks,
            &child.id,
            UpdateFields {
                status: Some(Status::Done),
                ..Default::default()
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks[&epic.id].status, Status::Done);
    }

    #[tokio::test]
    async fn test_delete_scrubs_dangling_references() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        // An epic so it can serve as both a dependency target and a parent.
        let a = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Epic,
                ..NewTask::new("Dep target")
            },
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let b = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![a.id.clone()],
                parent: None,
                task_type: TaskType::Task,
                ..NewTask::new("Dependent")
            },
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let c = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: Some(a.id.clone()),
                task_type: TaskType::Task,
                ..NewTask::new("Child of deleted")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        delete_task(tmp.path(), &tasks, &a.id).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks[&b.id].depends_on.is_empty(), "dep should be scrubbed");
        assert_eq!(tasks[&c.id].parent, None, "parent should be cleared");
        // And the dependent is now ready instead of silently blocked.
        let ready = list_ready(&tasks, None, None, None);
        assert!(ready.iter().any(|t| t.id == b.id));
    }

    #[tokio::test]
    async fn test_prune_scrubs_dangling_references() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let a = create_task(
            tmp.path(),
            &HashMap::new(),
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![],
                parent: None,
                task_type: TaskType::Task,
                ..NewTask::new("Cancelled dep")
            },
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let b = create_task(
            tmp.path(),
            &tasks,
            NewTask {
                priority: Priority::P2,
                tags: vec![],
                depends_on: vec![a.id.clone()],
                parent: None,
                task_type: TaskType::Task,
                ..NewTask::new("Dependent")
            },
        )
        .unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, &a.id, Status::Cancelled).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        prune_tasks(tmp.path(), &tasks, false).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks[&b.id].depends_on.is_empty(), "dep should be scrubbed");
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
        let ids: Vec<&str> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["c1", "c2", "c3"]);
        assert!(plan.cyclic.is_empty());
    }

    #[test]
    fn test_plan_epic_independent_children() {
        let tasks = task_map(vec![
            make_epic("e1"),
            make_child("c1", "e1", Status::Open),
            make_child("c2", "e1", Status::Open),
        ]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        assert_eq!(plan.tasks.len(), 2);
    }

    #[test]
    fn test_plan_epic_no_children() {
        let tasks = task_map(vec![make_epic("e1")]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        assert!(plan.tasks.is_empty());
        assert!(plan.cyclic.is_empty());
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
