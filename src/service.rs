use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;

use crate::config;
use crate::error::{Error, Result};
use crate::graph::Graph;
use crate::store;
use crate::task::{self, Priority, Status, Task};

/// Create a new task with validation.
pub async fn create_task(
    base: &Path,
    title: String,
    priority: Priority,
    tags: Vec<String>,
    depends_on: Vec<String>,
    parent: Option<String>,
    body: String,
) -> Result<Task> {
    let config = config::load(base)?;
    let tasks = store::load_all(base).await?;
    let existing_ids: HashSet<String> = tasks.keys().cloned().collect();
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
    t.tags = tags;
    t.depends_on = depends_on;
    t.parent = parent;
    t.body = body;

    store::save(base, &t)?;
    Ok(t)
}

/// List tasks with optional filters, sorted by priority then creation date.
pub async fn list_tasks(
    base: &Path,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<&str>,
    include_all: bool,
) -> Result<Vec<Task>> {
    let tasks = store::load_all(base).await?;
    let mut filtered: Vec<Task> = tasks
        .into_values()
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
        .collect();
    task::sort_by_priority_owned(&mut filtered);
    Ok(filtered)
}

/// Return tasks that are ready to work on.
pub async fn list_ready(base: &Path, tag: Option<&str>, limit: Option<usize>) -> Result<Vec<Task>> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    let ready = graph.ready(&tasks, tag, limit);
    Ok(ready.into_iter().cloned().collect())
}

/// Get a single task by ID.
pub fn get_task(base: &Path, id: &str) -> Result<Task> {
    store::load_one(base, id)
}

/// Update task fields. Only `Some` fields are changed.
#[allow(clippy::too_many_arguments)]
pub fn update_task(
    base: &Path,
    id: &str,
    status: Option<Status>,
    priority: Option<Priority>,
    tags: Option<Vec<String>>,
    assignee: Option<String>,
    body: Option<String>,
    title: Option<String>,
) -> Result<Task> {
    let mut t = store::load_one(base, id)?;

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
    Ok(t)
}

/// Set task status.
pub fn set_status(base: &Path, id: &str, status: Status) -> Result<Task> {
    let mut t = store::load_one(base, id)?;
    t.status = status;
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(t)
}

/// Add a dependency with cycle detection.
pub async fn add_dependency(base: &Path, id: &str, depends_on: &str) -> Result<Task> {
    store::load_one(base, depends_on)?;
    let mut t = store::load_one(base, id)?;

    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    if graph.would_cycle(id, depends_on) {
        return Err(Error::CycleDetected {
            from: id.into(),
            to: depends_on.into(),
        });
    }

    if !t.depends_on.contains(&depends_on.to_string()) {
        t.depends_on.push(depends_on.to_string());
        t.updated = Utc::now();
        store::save(base, &t)?;
    }

    Ok(t)
}

/// Remove a dependency.
pub fn remove_dependency(base: &Path, id: &str, depends_on: &str) -> Result<Task> {
    let mut t = store::load_one(base, id)?;
    t.depends_on.retain(|d| d != depends_on);
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(t)
}

/// Search tasks by text query.
pub async fn search_tasks(base: &Path, query: &str, include_all: bool) -> Result<Vec<Task>> {
    let tasks = store::load_all(base).await?;
    let query_lower = query.to_lowercase();
    let mut results: Vec<Task> = tasks
        .into_values()
        .filter(|t| include_all || task::is_active(t))
        .filter(|t| {
            t.title.to_lowercase().contains(&query_lower)
                || t.body.to_lowercase().contains(&query_lower)
                || t.tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query_lower))
                || t.id.contains(&query_lower)
        })
        .collect();
    task::sort_by_priority_owned(&mut results);
    Ok(results)
}

/// Delete a task by ID, returning the deleted task.
pub fn delete_task(base: &Path, id: &str) -> Result<Task> {
    let t = store::load_one(base, id)?;
    store::delete(base, id)?;
    Ok(t)
}

/// Prune cancelled (and optionally done) tasks, returning deleted tasks.
pub async fn prune_tasks(base: &Path, include_done: bool) -> Result<Vec<Task>> {
    let tasks = store::load_all(base).await?;
    let to_delete: Vec<Task> = tasks
        .into_values()
        .filter(|t| t.status == Status::Cancelled || (include_done && t.status == Status::Done))
        .collect();

    for t in &to_delete {
        store::delete(base, &t.id)?;
    }
    Ok(to_delete)
}

/// Get the dependency graph data.
pub async fn get_graph(base: &Path) -> Result<(HashMap<String, Task>, Graph)> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    Ok((tasks, graph))
}

/// Compute effective priorities for all tasks.
pub async fn effective_priorities(base: &Path) -> Result<HashMap<String, Priority>> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    let mut map = HashMap::new();
    for id in tasks.keys() {
        map.insert(id.clone(), graph.effective_priority(id, &tasks));
    }
    Ok(map)
}
