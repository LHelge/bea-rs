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

    // Resolve dependency IDs (prefixes allowed, like every other command)
    let mut resolved_deps = Vec::with_capacity(depends_on.len());
    let mut unknown = Vec::new();
    for dep in depends_on {
        match store::resolve_prefix(tasks, &dep) {
            Ok(dep_id) => resolved_deps.push(dep_id),
            Err(Error::TaskNotFound(_)) => unknown.push(dep),
            Err(e) => return Err(e),
        }
    }
    if !unknown.is_empty() {
        return Err(Error::UnknownDependency { ids: unknown });
    }

    // Treat empty-string parent as "no parent", and resolve a prefix to the
    // full id so the stored parent matches the canonical task id (otherwise
    // epic_progress / reparenting lookups by full id would miss it).
    // The parent must exist and be an epic.
    let parent = match parent.filter(|p| !p.is_empty()) {
        Some(pid) => {
            let parent_id = store::resolve_prefix(tasks, &pid)?;
            if !tasks[&parent_id].task_type.is_epic() {
                return Err(Error::ParentNotEpic(parent_id));
            }
            Some(parent_id)
        }
        None => None,
    };

    let mut t = Task::new(id, title, priority);
    t.task_type = task_type;
    t.tags = tags;
    t.depends_on = resolved_deps;
    t.parent = parent;
    t.body = body;

    store::save(base, &t)?;
    Ok(t)
}

/// List tasks with optional filters, sorted by priority then creation date.
/// List tasks with optional filters.
///
/// `include_all` also yields done/cancelled tasks. Proposals are separate:
/// they stay out of every listing unless `include_proposed` is set or the
/// caller filters for `status = proposed` explicitly, so an unvetted proposal
/// queue cannot drown the accepted backlog.
#[allow(clippy::too_many_arguments)]
pub fn list_tasks(
    tasks: &HashMap<String, Task>,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<&str>,
    include_all: bool,
    include_proposed: bool,
    epic: Option<&str>,
) -> Vec<Task> {
    let mut filtered: Vec<Task> = tasks
        .values()
        .filter(|t| {
            if status.is_some() {
                true
            } else if t.status == Status::Proposed {
                include_proposed
            } else {
                include_all || task::is_active(t)
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

/// Return tasks waiting for review, in canonical priority order.
///
/// The review queue is deliberately separate from [`list_ready`]: reviewing
/// someone else's finished work and starting fresh work are different jobs, so
/// an orchestrator can dispatch them to different workers.
pub fn list_review(
    tasks: &HashMap<String, Task>,
    tag: Option<&str>,
    limit: Option<usize>,
    epic: Option<&str>,
) -> Vec<Task> {
    let mut queue: Vec<Task> = tasks
        .values()
        .filter(|t| t.status == Status::Review)
        .filter(|t| task::matches_tag(t, tag))
        .filter(|t| epic.is_none_or(|e| t.parent.as_deref() == Some(e)))
        .cloned()
        .collect();
    task::sort_by_priority_owned(&mut queue);
    if let Some(limit) = limit {
        queue.truncate(limit);
    }
    queue
}

/// Move a task along one edge of the review workflow.
///
/// Each verb accepts exactly one starting status so that a shortcut can never
/// quietly undo unrelated state (submitting a `done` task for review, say).
/// `set_status` / `update_task` remain the escape hatch for any other move.
fn transition(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    action: &'static str,
    expected: Status,
    to: Status,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let t = &tasks[&id];
    if t.status != expected {
        return Err(Error::InvalidStatus {
            id: t.id.clone(),
            action,
            expected,
            actual: t.status,
        });
    }
    set_status(base, tasks, &id, to)
}

/// Submit in-progress work for review.
pub fn review_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    transition(
        base,
        tasks,
        id_or_prefix,
        "review",
        Status::InProgress,
        Status::Review,
    )
}

/// Send a task under review back for changes.
///
/// The assignee is kept: a rejected task carries who wrote it, so an
/// orchestrator can route the rework back to the same worker. Use
/// [`release_task`] instead to hand it to the pool.
pub fn reject_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    transition(
        base,
        tasks,
        id_or_prefix,
        "reject",
        Status::Review,
        Status::Open,
    )
}

/// Demote an open task to a proposal awaiting acceptance.
pub fn propose_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
) -> Result<Task> {
    transition(
        base,
        tasks,
        id_or_prefix,
        "propose",
        Status::Open,
        Status::Proposed,
    )
}

/// Accept a proposal into the backlog, making it eligible for `ready`.
pub fn accept_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    transition(
        base,
        tasks,
        id_or_prefix,
        "accept",
        Status::Proposed,
        Status::Open,
    )
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

    let previous_status = t.status;
    let status_changed = status.as_ref().is_some_and(|s| *s != t.status);
    if let Some(s) = status {
        t.status = s;
        record_attempt(&mut t, previous_status);
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
                // Validate parent exists and store its canonical full id (not the
                // typed prefix) so epic_progress lookups by full id match.
                t.parent = Some(store::resolve_prefix(tasks, pid)?);
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
    let previous = t.status;
    t.status = status;
    record_attempt(&mut t, previous);
    t.updated = Utc::now();
    store::save(base, &t)?;

    on_status_changed(base, tasks, &t)?;

    Ok(t)
}

/// Start a task: set its status to `in_progress` and optionally claim it for
/// an assignee.
///
/// `assignee` of `None` leaves the current assignee untouched; `Some("")`
/// clears it.
pub fn start_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    assignee: Option<String>,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();
    let previous = t.status;
    t.status = Status::InProgress;
    record_attempt(&mut t, previous);
    if let Some(a) = assignee {
        t.assignee = a;
    }
    t.updated = Utc::now();
    store::save(base, &t)?;

    on_status_changed(base, tasks, &t)?;

    Ok(t)
}

/// Release an in-progress task back to the pool: status returns to `open` and
/// the assignee is cleared, so another worker can pick it up.
///
/// Only `in_progress` tasks can be released — releasing anything else is an
/// error, so a stuck worker cannot accidentally reopen finished work.
pub fn release_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();
    if t.status != Status::InProgress {
        return Err(Error::InvalidStatus {
            id: t.id.clone(),
            action: "release",
            expected: Status::InProgress,
            actual: t.status,
        });
    }
    t.status = Status::Open;
    t.assignee = String::new();
    t.updated = Utc::now();
    store::save(base, &t)?;

    on_status_changed(base, tasks, &t)?;

    Ok(t)
}

// ---------------------------------------------------------------------------
// Assignee fencing
//
// `assignee` doubles as a fencing token for orchestrators driving a shared
// store: work is taken through the claiming primitives below, and every later
// mutation on the claimed task goes through a `_fenced` variant carrying that
// token. A writer whose task has since been released, reassigned, or reaped
// fails with a distinct error instead of silently clobbering the new holder's
// state. All checks run against a fresh read from disk, not the caller's
// snapshot. The unfenced functions remain the human/CLI path.
// ---------------------------------------------------------------------------

/// Verify — against a fresh read from disk — that a task's assignee still
/// matches the token the caller claimed it with.
///
/// Returns the resolved task ID so fenced wrappers can reuse it. The
/// read-check-write is not a file lock: concurrent writers sharing one store
/// are expected to serialize among themselves (typically one orchestrator
/// process holding the store behind a mutex); the fence is what catches a
/// *stale* writer inside that discipline.
fn assert_fence(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    expected_assignee: &str,
) -> Result<String> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let current = store::load_one(base, &id)?;
    if current.assignee != expected_assignee {
        return Err(Error::FenceViolation {
            id,
            expected: expected_assignee.to_string(),
            actual: current.assignee,
        });
    }
    Ok(id)
}

/// Claim an open task and start it: the contention-checked counterpart of
/// [`start_task`].
///
/// The task must be `open` and unclaimed (or already claimed by this same
/// `assignee`) on a fresh read from disk; otherwise the claim is refused with
/// [`Error::AlreadyClaimed`] rather than silently stealing it. On success the
/// task is `in_progress`, assigned, and the attempt is counted.
pub fn claim_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    assignee: &str,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = store::load_one(base, &id)?;
    if t.status != Status::Open {
        return Err(Error::InvalidStatus {
            id,
            action: "claim",
            expected: Status::Open,
            actual: t.status,
        });
    }
    if !t.assignee.is_empty() && t.assignee != assignee {
        return Err(Error::AlreadyClaimed {
            id,
            assignee: t.assignee,
        });
    }
    let previous = t.status;
    t.status = Status::InProgress;
    record_attempt(&mut t, previous);
    t.assignee = assignee.to_string();
    t.updated = Utc::now();
    store::save(base, &t)?;
    on_status_changed(base, tasks, &t)?;
    Ok(t)
}

/// Claim a task for an assignee without touching its status.
///
/// The review-queue counterpart of [`claim_task`]: a reviewer takes a task
/// that stays in `review` while they work it. Refused with
/// [`Error::AlreadyClaimed`] when someone else holds the task on a fresh
/// read, and with [`Error::InvalidUsage`] on `proposed` or terminal tasks,
/// which have no work to claim.
pub fn assign_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    assignee: &str,
) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = store::load_one(base, &id)?;
    if matches!(
        t.status,
        Status::Done | Status::Cancelled | Status::Proposed
    ) {
        return Err(Error::InvalidUsage(format!(
            "cannot assign task {id}: a {} task has no work to claim",
            t.status
        )));
    }
    if !t.assignee.is_empty() && t.assignee != assignee {
        return Err(Error::AlreadyClaimed {
            id,
            assignee: t.assignee,
        });
    }
    t.assignee = assignee.to_string();
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(t)
}

/// Fenced [`release_task`]: refused with [`Error::FenceViolation`] unless the
/// task is still assigned to `expected_assignee`.
pub fn release_task_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    release_task(base, tasks, &id)
}

/// Fenced [`review_task`]: submit for review only while still holding the task.
pub fn review_task_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    review_task(base, tasks, &id)
}

/// Fenced [`reject_task`]: send back for changes only while still holding the
/// task (as its reviewer, via [`assign_task`]).
pub fn reject_task_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    reject_task(base, tasks, &id)
}

/// Fenced [`set_status`]: the escape hatch, guarded by the fence.
pub fn set_status_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    status: Status,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    set_status(base, tasks, &id, status)
}

/// Fenced [`update_task`]: field updates (body, tags, …) guarded by the fence.
#[allow(clippy::too_many_arguments)]
pub fn update_task_fenced(
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
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    update_task(
        base, tasks, &id, status, priority, tags, assignee, body, title, parent,
    )
}

/// Fenced [`add_dependency`]: the fence applies to the task whose
/// `depends_on` list changes, not to the dependency target.
pub fn add_dependency_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    dep_or_prefix: &str,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    add_dependency(base, tasks, &id, dep_or_prefix)
}

/// Fenced [`remove_dependency`]: the fence applies to the task whose
/// `depends_on` list changes, not to the dependency target.
pub fn remove_dependency_fenced(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
    dep_or_prefix: &str,
    expected_assignee: &str,
) -> Result<Task> {
    let id = assert_fence(base, tasks, id_or_prefix, expected_assignee)?;
    remove_dependency(base, tasks, &id, dep_or_prefix)
}

/// Count a new attempt when a task transitions *into* `in_progress`.
///
/// An attempt starts when work is claimed, not when it is given up: that way
/// the counter is already durable if the worker dies without releasing, and a
/// task that succeeded on the third try reads `attempts: 3`. Re-starting a
/// task that is already in progress (e.g. to hand it to another assignee) is
/// the same attempt, so it does not bump the counter.
fn record_attempt(t: &mut Task, previous: Status) {
    if t.status == Status::InProgress && previous != Status::InProgress {
        t.attempts = Some(t.attempt_count().saturating_add(1));
    }
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
    overrides.insert(t.id.clone(), t.status);
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
///
/// Only applies to hard deletion (delete/prune) — archived tasks keep their
/// IDs reserved and still resolve via the archive, so no scrubbing there.
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

// ─── Archive helpers ──────────────────────────────────────────────────────────

/// Check whether a task is archivable.
///
/// A task is archivable when:
/// - its status is Done or Cancelled, AND
/// - no ACTIVE (not Done/Cancelled) task in `tasks` depends on it.
///
/// For epics the check is the same — the caller is responsible for deciding
/// whether to cascade to children before calling this predicate.
//
// Public predicate exercised by the unit tests; the CLI/MCP archive paths go
// through `archive_task`/`archive_all` (which need the blocker list, not a bool).
#[cfg_attr(not(test), allow(dead_code))]
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
pub async fn get_archived_task(base: &Path, id_or_prefix: &str) -> Result<Task> {
    let archived = store::load_archived(base).await?;
    let id = store::resolve_prefix(&archived, id_or_prefix)
        .map_err(|_| Error::NotArchived(id_or_prefix.to_string()))?;
    Ok(archived[&id].clone())
}

/// List archived tasks sorted by `updated` descending (most recently updated first).
///
/// If `limit` is `Some(n)`, at most `n` tasks are returned.
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

    #[tokio::test]
    async fn test_claim_task_claims_and_counts_attempt() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let t = claim_task(tmp.path(), &tasks, "aaa", "worker-1").unwrap();
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.assignee, "worker-1");
        assert_eq!(t.attempt_count(), 1);
    }

    #[tokio::test]
    async fn test_claim_task_refuses_contended_claim() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::Open);
        a.assignee = "worker-1".into();
        store::save(tmp.path(), &a).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let err = claim_task(tmp.path(), &tasks, "aaa", "worker-2").unwrap_err();
        assert!(matches!(
            err,
            Error::AlreadyClaimed { ref assignee, .. } if assignee == "worker-1"
        ));
        // The refused claim must not have touched the file.
        let on_disk = store::load_one(tmp.path(), "aaa").unwrap();
        assert_eq!(on_disk.assignee, "worker-1");
        assert_eq!(on_disk.status, Status::Open);
    }

    #[tokio::test]
    async fn test_claim_task_checks_disk_not_snapshot() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();
        // Snapshot taken while the task is unclaimed…
        let stale = store::load_all(tmp.path()).await.unwrap();
        // …then someone else claims it on disk.
        let mut a = stale["aaa"].clone();
        a.assignee = "worker-1".into();
        store::save(tmp.path(), &a).unwrap();

        let err = claim_task(tmp.path(), &stale, "aaa", "worker-2").unwrap_err();
        assert!(matches!(err, Error::AlreadyClaimed { .. }));
    }

    #[tokio::test]
    async fn test_claim_task_requires_open() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Review)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let err = claim_task(tmp.path(), &tasks, "aaa", "worker-1").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidStatus {
                action: "claim",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn test_assign_task_keeps_status_and_refuses_contention() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Review)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let t = assign_task(tmp.path(), &tasks, "aaa", "reviewer-1").unwrap();
        assert_eq!(t.status, Status::Review, "status must not change");
        assert_eq!(t.assignee, "reviewer-1");

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let err = assign_task(tmp.path(), &tasks, "aaa", "reviewer-2").unwrap_err();
        assert!(matches!(err, Error::AlreadyClaimed { .. }));

        // Terminal and proposed tasks have no work to claim.
        store::save(tmp.path(), &make_task("bbb", Status::Done)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let err = assign_task(tmp.path(), &tasks, "bbb", "reviewer-1").unwrap_err();
        assert!(matches!(err, Error::InvalidUsage(_)));
    }

    #[tokio::test]
    async fn test_fenced_mutations_reject_stale_writer() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::InProgress);
        a.assignee = "worker-1".into();
        store::save(tmp.path(), &a).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        // worker-1 was reaped and the task reassigned on disk…
        let mut a = tasks["aaa"].clone();
        a.assignee = "worker-2".into();
        store::save(tmp.path(), &a).unwrap();

        // …so every fenced write from worker-1, even via a stale snapshot, fails.
        let err = update_task_fenced(
            tmp.path(),
            &tasks,
            "aaa",
            None,
            None,
            None,
            None,
            Some("stale findings".into()),
            None,
            None,
            "worker-1",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Error::FenceViolation { ref actual, .. } if actual == "worker-2"
        ));
        let err = release_task_fenced(tmp.path(), &tasks, "aaa", "worker-1").unwrap_err();
        assert!(matches!(err, Error::FenceViolation { .. }));
        let err = review_task_fenced(tmp.path(), &tasks, "aaa", "worker-1").unwrap_err();
        assert!(matches!(err, Error::FenceViolation { .. }));
        let err =
            set_status_fenced(tmp.path(), &tasks, "aaa", Status::Done, "worker-1").unwrap_err();
        assert!(matches!(err, Error::FenceViolation { .. }));

        // The body write never happened.
        let on_disk = store::load_one(tmp.path(), "aaa").unwrap();
        assert_eq!(on_disk.body, "");
    }

    #[tokio::test]
    async fn test_fenced_mutations_pass_for_current_holder() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::InProgress);
        a.assignee = "worker-1".into();
        store::save(tmp.path(), &a).unwrap();
        store::save(tmp.path(), &make_task("bbb", Status::Open)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let t = update_task_fenced(
            tmp.path(),
            &tasks,
            "aaa",
            None,
            None,
            None,
            None,
            Some("findings".into()),
            None,
            None,
            "worker-1",
        )
        .unwrap();
        assert_eq!(t.body, "findings");

        let t = add_dependency_fenced(tmp.path(), &tasks, "aaa", "bbb", "worker-1").unwrap();
        assert_eq!(t.depends_on, vec!["bbb".to_string()]);
        let t = remove_dependency_fenced(tmp.path(), &tasks, "aaa", "bbb", "worker-1").unwrap();
        assert!(t.depends_on.is_empty());

        let t = review_task_fenced(tmp.path(), &tasks, "aaa", "worker-1").unwrap();
        assert_eq!(t.status, Status::Review);
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = reject_task_fenced(tmp.path(), &tasks, "aaa", "worker-1").unwrap();
        assert_eq!(t.status, Status::Open);
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = release_task_fenced(tmp.path(), &tasks, "aaa", "worker-1");
        // After reject the task is open, so release (which requires in_progress)
        // correctly refuses — the fence itself passed.
        assert!(matches!(t.unwrap_err(), Error::InvalidStatus { .. }));
    }

    #[tokio::test]
    async fn test_delete_scrubs_dangling_references() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        // An epic so it can serve as both a dependency target and a parent.
        let mut a = make_epic("aaa");
        a.id = "aaa".into();
        store::save(tmp.path(), &a).unwrap();
        let mut b = make_task("bbb", Status::Open);
        b.depends_on = vec!["aaa".into()];
        store::save(tmp.path(), &b).unwrap();
        let c = make_child("ccc", "aaa", Status::Open);
        store::save(tmp.path(), &c).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        delete_task(tmp.path(), &tasks, "aaa").unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks["bbb"].depends_on.is_empty(), "dep should be scrubbed");
        assert_eq!(tasks["ccc"].parent, None, "parent should be cleared");
        // And the dependent is now ready instead of silently blocked.
        let ready = list_ready(&tasks, None, None, None);
        assert!(ready.iter().any(|t| t.id == "bbb"));
    }

    #[tokio::test]
    async fn test_prune_scrubs_dangling_references() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let a = make_task("aaa", Status::Cancelled);
        store::save(tmp.path(), &a).unwrap();
        let mut b = make_task("bbb", Status::Open);
        b.depends_on = vec!["aaa".into()];
        store::save(tmp.path(), &b).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        prune_tasks(tmp.path(), &tasks, false).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks["bbb"].depends_on.is_empty(), "dep should be scrubbed");
    }

    #[tokio::test]
    async fn test_create_task_rejects_unknown_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let result = create_task(
            tmp.path(),
            &HashMap::new(),
            "Orphan".into(),
            Priority::P2,
            vec![],
            vec![],
            Some("zzzz".into()),
            String::new(),
            TaskType::Task,
        );
        assert!(matches!(result, Err(Error::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_task_rejects_non_epic_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let plain = make_task("ppp", Status::Open);
        store::save(tmp.path(), &plain).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let result = create_task(
            tmp.path(),
            &tasks,
            "Child".into(),
            Priority::P2,
            vec![],
            vec![],
            Some("ppp".into()),
            String::new(),
            TaskType::Task,
        );
        assert!(matches!(result, Err(Error::ParentNotEpic(_))));
    }

    #[tokio::test]
    async fn test_create_task_resolves_dep_prefixes() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let dep = Task::new("abcd".into(), "Dep".into(), Priority::P2);
        store::save(tmp.path(), &dep).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = create_task(
            tmp.path(),
            &tasks,
            "Uses prefixes".into(),
            Priority::P2,
            vec![],
            vec!["ab".into()],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(t.depends_on, vec!["abcd"]);
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

    #[tokio::test]
    async fn test_parent_prefix_stored_as_canonical_id() {
        // A parent passed as a prefix must be stored as the resolved full id, so
        // epic_progress (which matches children on the full id) counts them.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let mut epic = Task::new("epicid".into(), "Epic".into(), Priority::P1);
        epic.task_type = TaskType::Epic;
        store::save(tmp.path(), &epic).unwrap();
        let child = Task::new("chld".into(), "Existing child".into(), Priority::P2);
        store::save(tmp.path(), &child).unwrap();

        // update_task reparenting with a prefix ("epi" → "epicid").
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let updated = update_task(
            tmp.path(),
            &tasks,
            "chld",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Some("epi".into())),
        )
        .unwrap();
        assert_eq!(
            updated.parent.as_deref(),
            Some("epicid"),
            "update_task should store the resolved full parent id, not the prefix"
        );

        // create_task with a prefix parent ("epi" → "epicid").
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let created = create_task(
            tmp.path(),
            &tasks,
            "New child".into(),
            Priority::P2,
            vec![],
            vec![],
            Some("epi".into()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(
            created.parent.as_deref(),
            Some("epicid"),
            "create_task should store the resolved full parent id, not the prefix"
        );

        // Both children are now visible to the epic via its full id.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(epic_progress(&tasks, "epicid").total, 2);
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

    // ─── list_archive tests ───────────────────────────────────────────────────

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

    // ── start / release ────────────────────────────────────────────

    #[tokio::test]
    async fn test_start_task_with_assignee() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = start_task(tmp.path(), &tasks, "aaa", Some("agent-1".into())).unwrap();
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.assignee, "agent-1");

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks["aaa"].assignee, "agent-1", "assignee should persist");
    }

    #[tokio::test]
    async fn test_start_task_without_assignee_leaves_it_unchanged() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::Open);
        a.assignee = "agent-1".into();
        store::save(tmp.path(), &a).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = start_task(tmp.path(), &tasks, "aaa", None).unwrap();
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.assignee, "agent-1", "omitted assignee must not clear it");
    }

    #[tokio::test]
    async fn test_start_task_empty_assignee_clears_it() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::Open);
        a.assignee = "agent-1".into();
        store::save(tmp.path(), &a).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = start_task(tmp.path(), &tasks, "aaa", Some(String::new())).unwrap();
        assert_eq!(t.assignee, "");
    }

    #[tokio::test]
    async fn test_release_resets_status_and_assignee() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut a = make_task("aaa", Status::InProgress);
        a.assignee = "agent-1".into();
        store::save(tmp.path(), &a).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = release_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Open);
        assert_eq!(t.assignee, "");

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks["aaa"].status, Status::Open);
        assert_eq!(tasks["aaa"].assignee, "");
    }

    #[tokio::test]
    async fn test_release_rejects_non_in_progress_tasks() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        for status in [
            Status::Open,
            Status::Done,
            Status::Cancelled,
            Status::Blocked,
        ] {
            let mut a = make_task("aaa", status);
            a.assignee = "agent-1".into();
            store::save(tmp.path(), &a).unwrap();

            let tasks = store::load_all(tmp.path()).await.unwrap();
            let err = release_task(tmp.path(), &tasks, "aaa").unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidStatus {
                        action: "release",
                        ..
                    }
                ),
                "releasing a {status} task should fail, got {err:?}"
            );

            // The task itself must be untouched.
            let tasks = store::load_all(tmp.path()).await.unwrap();
            assert_eq!(tasks["aaa"].status, status);
            assert_eq!(tasks["aaa"].assignee, "agent-1");
        }
    }

    #[tokio::test]
    async fn test_release_unknown_task_is_an_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(release_task(tmp.path(), &tasks, "zzz").is_err());
    }

    // ── attempts ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_attempts_counts_each_start() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks["aaa"].attempt_count(),
            0,
            "missing counter reads as 0"
        );
        assert_eq!(
            tasks["aaa"].attempts, None,
            "no counter is written up front"
        );

        let t = start_task(tmp.path(), &tasks, "aaa", Some("agent-1".into())).unwrap();
        assert_eq!(t.attempt_count(), 1);

        // Failed attempt handed back, then picked up by another worker.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = release_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.attempt_count(), 1, "release must not count as an attempt");

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = start_task(tmp.path(), &tasks, "aaa", Some("agent-2".into())).unwrap();
        assert_eq!(t.attempt_count(), 2);

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks["aaa"].attempts, Some(2), "counter persists");
    }

    #[tokio::test]
    async fn test_restarting_an_in_progress_task_is_the_same_attempt() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        start_task(tmp.path(), &tasks, "aaa", Some("agent-1".into())).unwrap();

        // Handing the in-progress task to another assignee is not a new attempt.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = start_task(tmp.path(), &tasks, "aaa", Some("agent-2".into())).unwrap();
        assert_eq!(t.attempt_count(), 1);
        assert_eq!(t.assignee, "agent-2");
    }

    #[tokio::test]
    async fn test_attempts_counted_on_every_path_into_in_progress() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        // `set_status` (bea status / TUI) counts.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = set_status(tmp.path(), &tasks, "aaa", Status::InProgress).unwrap();
        assert_eq!(t.attempt_count(), 1);

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, "aaa", Status::Open).unwrap();

        // `update_task` with a status change counts too.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = update_task(
            tmp.path(),
            &tasks,
            "aaa",
            Some(Status::InProgress),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(t.attempt_count(), 2);

        // A non-status update leaves the counter alone.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = update_task(
            tmp.path(),
            &tasks,
            "aaa",
            None,
            Some(Priority::P0),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(t.attempt_count(), 2);
    }

    #[tokio::test]
    async fn test_other_status_changes_do_not_count_attempts() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, "aaa", Status::Blocked).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = set_status(tmp.path(), &tasks, "aaa", Status::Done).unwrap();
        assert_eq!(t.attempt_count(), 0);
        assert_eq!(t.attempts, None);
    }

    // ── review & proposal workflow ─────────────────────────────────

    #[tokio::test]
    async fn test_review_workflow_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        start_task(tmp.path(), &tasks, "aaa", Some("agent-1".into())).unwrap();

        // Finished work goes to review instead of straight to done.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = review_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Review);
        assert_eq!(t.assignee, "agent-1", "review keeps the author");

        // Reviewer wants changes: back to open, author retained.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = reject_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Open);
        assert_eq!(t.assignee, "agent-1");
        assert_eq!(t.attempt_count(), 1, "a rejection is not a new attempt");

        // Second pass through, approved this time.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        start_task(tmp.path(), &tasks, "aaa", None).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        review_task(tmp.path(), &tasks, "aaa").unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = set_status(tmp.path(), &tasks, "aaa", Status::Done).unwrap();
        assert_eq!(t.status, Status::Done);
        assert_eq!(t.attempt_count(), 2);
    }

    #[tokio::test]
    async fn test_workflow_verbs_reject_wrong_starting_status() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        // review needs in_progress, reject needs review, accept needs proposed.
        for (action, result) in [
            ("review", review_task(tmp.path(), &tasks, "aaa")),
            ("reject", reject_task(tmp.path(), &tasks, "aaa")),
            ("accept", accept_task(tmp.path(), &tasks, "aaa")),
        ] {
            let err = result.unwrap_err();
            assert!(
                matches!(&err, Error::InvalidStatus { action: a, actual, .. }
                    if *a == action && *actual == Status::Open),
                "{action} on an open task should fail, got {err:?}"
            );
        }

        // Nothing was written.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks["aaa"].status, Status::Open);
    }

    #[tokio::test]
    async fn test_propose_accept_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = propose_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Proposed);

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = accept_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Open);
    }

    #[tokio::test]
    async fn test_proposed_and_review_are_never_ready() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Proposed)).unwrap();
        store::save(tmp.path(), &make_task("bbb", Status::Review)).unwrap();
        store::save(tmp.path(), &make_task("ccc", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let ready = list_ready(&tasks, None, None, None);
        let ready_ids: Vec<&str> = ready.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ready_ids, vec!["ccc"]);

        let queue = list_review(&tasks, None, None, None);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "bbb");
    }

    #[tokio::test]
    async fn test_review_does_not_unblock_dependents() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Review)).unwrap();
        let mut b = make_task("bbb", Status::Open);
        b.depends_on = vec!["aaa".into()];
        store::save(tmp.path(), &b).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(
            list_ready(&tasks, None, None, None).is_empty(),
            "unreviewed work must not unblock its dependents"
        );

        // Once approved, the dependent frees up.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        set_status(tmp.path(), &tasks, "aaa", Status::Done).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let ready = list_ready(&tasks, None, None, None);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "bbb");
    }

    #[tokio::test]
    async fn test_proposals_hidden_from_default_listing() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_task("aaa", Status::Proposed)).unwrap();
        store::save(tmp.path(), &make_task("bbb", Status::Open)).unwrap();
        store::save(tmp.path(), &make_task("ccc", Status::Done)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let ids = |v: Vec<Task>| -> Vec<String> { v.into_iter().map(|t| t.id).collect() };

        // Default: no proposals, no done tasks.
        assert_eq!(
            ids(list_tasks(&tasks, None, None, None, false, false, None)),
            vec!["bbb"]
        );
        // include_all alone still hides proposals (the MCP default).
        assert_eq!(
            ids(list_tasks(&tasks, None, None, None, true, false, None)),
            vec!["bbb", "ccc"]
        );
        // Everything, the way `bea list --all` asks for it.
        let mut all = ids(list_tasks(&tasks, None, None, None, true, true, None));
        all.sort();
        assert_eq!(all, vec!["aaa", "bbb", "ccc"]);
        // An explicit status filter always wins.
        assert_eq!(
            ids(list_tasks(
                &tasks,
                Some(Status::Proposed),
                None,
                None,
                false,
                false,
                None
            )),
            vec!["aaa"]
        );
    }

    #[tokio::test]
    async fn test_review_queue_filters_and_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mut low = make_task("aaa", Status::Review);
        low.priority = Priority::P3;
        low.tags = vec!["backend".into()];
        store::save(tmp.path(), &low).unwrap();
        let mut high = make_task("bbb", Status::Review);
        high.priority = Priority::P0;
        store::save(tmp.path(), &high).unwrap();
        store::save(tmp.path(), &make_task("ccc", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let queue = list_review(&tasks, None, None, None);
        let ids: Vec<&str> = queue.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["bbb", "aaa"], "highest priority reviewed first");

        let tagged = list_review(&tasks, Some("backend"), None, None);
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].id, "aaa");

        assert_eq!(list_review(&tasks, None, Some(1), None).len(), 1);
    }

    #[tokio::test]
    async fn test_epic_does_not_close_while_a_child_is_in_review() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let epic = make_epic("eee");
        store::save(tmp.path(), &epic).unwrap();
        store::save(tmp.path(), &make_child("aaa", "eee", Status::Done)).unwrap();
        store::save(tmp.path(), &make_child("bbb", "eee", Status::InProgress)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        review_task(tmp.path(), &tasks, "bbb").unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks["eee"].status,
            Status::Open,
            "an epic must not auto-close on unreviewed work"
        );
    }
}
