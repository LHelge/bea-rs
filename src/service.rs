use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

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

    // Treat an empty-string parent as "no parent" and resolve a prefix to the
    // full id. The parent must exist and be an epic; it becomes the prefix of
    // the new task's ID, which is where the parent is recorded.
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

    // Active and archived IDs and their ancestors, so a new task never reuses
    // an archived ID or the ID of a deleted epic whose children remain.
    let existing_ids = store::reserved_ids(base, tasks);
    let id = match &parent {
        Some(parent_id) => task::generate_child_id(parent_id, &existing_ids, &config),
        None => task::generate_id(&existing_ids, config.id_length as usize),
    };

    let mut t = Task::new(id, title, priority);
    t.task_type = task_type;
    t.tags = tags;
    t.depends_on = resolved_deps;
    t.body = body;

    store::save(base, &t)?;
    Ok(t)
}

/// Resolve the value of an `epic` filter to the epic's full ID, the way every
/// other ID argument is resolved (an exact ID, or a prefix through
/// [`store::resolve_prefix`]).
///
/// `tasks` are the active tasks. An exact active ID is taken without reading
/// the archive. Otherwise the value is resolved against active and archived
/// IDs together, so an epic that lives only in the archive still filters its
/// active children, and a prefix of such an epic names the epic rather than
/// one of those children.
///
/// Errors: [`Error::InvalidId`] for an empty value, [`Error::TaskNotFound`]
/// when it names no task, [`Error::AmbiguousPrefix`] when it names several,
/// and [`Error::NotAnEpic`] when the task it names is not an epic (only epics
/// have tasks below them, so the filter could only ever be empty).
///
/// The `list_*` functions and [`Graph::bounded_adjacency_list`] take the
/// resolved ID; they match it exactly.
pub async fn resolve_epic_filter(
    base: &Path,
    tasks: &HashMap<String, Task>,
    epic: &str,
) -> Result<String> {
    let is_epic = |t: &Task| {
        if t.task_type.is_epic() {
            Ok(t.id.clone())
        } else {
            Err(Error::NotAnEpic(t.id.clone()))
        }
    };
    if let Some(t) = tasks.get(epic) {
        return is_epic(t);
    }
    let mut all = store::load_archived(base).await?;
    all.extend(tasks.iter().map(|(id, t)| (id.clone(), t.clone())));
    let id = store::resolve_prefix(&all, epic)?;
    is_epic(&all[&id])
}

/// List tasks with optional filters, sorted by priority then creation date.
///
/// `epic` is an exact epic ID (see [`resolve_epic_filter`]); it matches every
/// task below that epic.
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
        .filter(|t| task::in_epic(t, epic))
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
        .filter(|t| task::in_epic(t, epic))
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
/// The assignee is cleared: a rejected task returns to the pool, and whoever
/// claims it next picks up the rework from the findings recorded in the task
/// body.
pub fn reject_task(base: &Path, tasks: &HashMap<String, Task>, id_or_prefix: &str) -> Result<Task> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let t = &tasks[&id];
    if t.status != Status::Review {
        return Err(Error::InvalidStatus {
            id: t.id.clone(),
            action: "reject",
            expected: Status::Review,
            actual: t.status,
        });
    }
    let mut t = t.clone();
    t.status = Status::Open;
    t.assignee = String::new();
    t.updated = Utc::now();
    store::save(base, &t)?;
    on_status_changed(base, tasks, &t)?;
    Ok(t)
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

/// What [`update_task`] did: the task as it now stands, and the IDs that a
/// parent change renamed.
#[derive(Debug, Clone)]
pub struct UpdatedTask {
    /// The updated task, under its new ID if it moved.
    pub task: Task,
    /// Old ID → new ID for every task the move renamed: the task itself and
    /// every task below it, active and archived. Empty when the parent did
    /// not change.
    pub renamed: BTreeMap<String, String>,
}

/// Update task fields. Only `Some` fields are changed.
///
/// The `parent` parameter uses a double-Option to distinguish three states:
/// - `None`           → leave parent unchanged
/// - `Some(None)`     → make the task a root task
/// - `Some(Some(id))` → put the task under the given epic
///
/// A task's parent is part of its ID, so a parent that differs from the
/// current one moves the task: it gets a fresh ID (a root ID when detached, a
/// child ID of the new epic otherwise), every task below it keeps its own
/// segments under the new prefix, and every `depends_on` naming one of them,
/// active or archived, is rewritten (see [`store::rename_ids`]). The new
/// parent must be an existing epic ([`Error::TaskNotFound`],
/// [`Error::ParentNotEpic`]) outside the task's own subtree
/// ([`Error::ParentInSubtree`]); all of that is checked before anything is
/// written. Naming the current parent changes nothing.
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
) -> Result<UpdatedTask> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let mut t = tasks[&id].clone();

    let renamed = match parent {
        Some(new_parent) => {
            let new_parent = new_parent
                .filter(|p| !p.is_empty())
                .map(|p| store::resolve_prefix(tasks, &p))
                .transpose()?;
            if new_parent.as_deref() == t.parent() {
                BTreeMap::new()
            } else {
                plan_move(base, tasks, &id, new_parent.as_deref())?
            }
        }
        None => BTreeMap::new(),
    };

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
    t.updated = Utc::now();

    // The field changes are saved under the old ID first, then the subtree
    // is renamed, so an interruption leaves either the old or the new state.
    store::save(base, &t)?;
    if renamed.is_empty() {
        if status_changed {
            on_status_changed(base, tasks, &t)?;
        }
        return Ok(UpdatedTask { task: t, renamed });
    }

    store::rename_ids(base, &renamed)?;
    let t = store::load_one(base, &renamed[&id])?;
    // Apply status-change side effects (e.g. epic auto-close) against the
    // store as it stands after the move, so the new parent is the one checked.
    if status_changed {
        on_status_changed(base, &apply_renames(tasks, &renamed), &t)?;
    }
    Ok(UpdatedTask { task: t, renamed })
}

/// Work out the renames that move task `id` under `new_parent` (`None`: to
/// the top level), after checking that the move is allowed. Writes nothing.
fn plan_move(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: &str,
    new_parent: Option<&str>,
) -> Result<BTreeMap<String, String>> {
    if let Some(parent) = new_parent {
        if parent == id || task::is_descendant(parent, id) {
            return Err(Error::ParentInSubtree {
                id: id.to_string(),
                parent: parent.to_string(),
            });
        }
        if !tasks[parent].task_type.is_epic() {
            return Err(Error::ParentNotEpic(parent.to_string()));
        }
    }

    let config = config::load(base)?;
    let reserved = store::reserved_ids(base, tasks);
    let new_id = match new_parent {
        Some(parent) => task::generate_child_id(parent, &reserved, &config),
        None => task::generate_id(&reserved, config.id_length as usize),
    };

    // The task and everything below it, active and archived. The new ID is
    // fresh and so are all of its descendants (the reserved pool holds every
    // ancestor of every known ID), so none of the new IDs can collide.
    let archived = store::archived_id_set(base);
    Ok(tasks
        .keys()
        .chain(archived.iter())
        .filter(|old| old.as_str() == id || task::is_descendant(old, id))
        .map(|old| (old.clone(), rebase(old, id, &new_id)))
        .collect())
}

/// A copy of `tasks` with `renames` applied to IDs and `depends_on` entries,
/// as [`store::rename_ids`] applies them on disk.
fn apply_renames(
    tasks: &HashMap<String, Task>,
    renames: &BTreeMap<String, String>,
) -> HashMap<String, Task> {
    tasks
        .values()
        .map(|t| {
            let mut t = t.clone();
            if let Some(new) = renames.get(&t.id) {
                t.id = new.clone();
            }
            for dep in &mut t.depends_on {
                if let Some(new) = renames.get(dep) {
                    *dep = new.clone();
                }
            }
            (t.id.clone(), t)
        })
        .collect()
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
) -> Result<UpdatedTask> {
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

    let Some(parent_id) = t.parent() else {
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
        .filter(|c| c.is_child_of(parent_id))
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
        overrides.insert(parent_id.to_string(), Status::Done);

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

/// What [`delete_task`] did: the task it deleted, and the IDs it renamed to
/// detach the task's children.
#[derive(Debug, Clone)]
pub struct DeletedTask {
    /// The deleted task, as it was before the deletion.
    pub task: Task,
    /// Old ID → new ID for every task the deletion moved: each direct child
    /// of the deleted task (active or archived) detached to a fresh root ID,
    /// and everything below it. Empty when the task had no children.
    pub renamed: BTreeMap<String, String>,
}

/// What [`prune_tasks`] did: the tasks it deleted, and the IDs it renamed to
/// detach the children that survived.
#[derive(Debug, Clone)]
pub struct PrunedTasks {
    /// The deleted tasks, under the IDs they had before the prune.
    pub tasks: Vec<Task>,
    /// Old ID → new ID for every surviving task the prune moved, as in
    /// [`DeletedTask::renamed`]. Empty when no pruned task had a surviving
    /// child.
    pub renamed: BTreeMap<String, String>,
}

/// Delete a task by ID or prefix, returning the deleted task.
///
/// References to the deleted task are removed from remaining tasks. A
/// child's parent is part of its ID, so the children of a deleted epic are
/// first detached to fresh root IDs, subtrees and archived children
/// included, as [`update_task`] detaches a task; the epic is deleted last.
/// The move does not open or close any epic.
pub fn delete_task(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id_or_prefix: &str,
) -> Result<DeletedTask> {
    let id = store::resolve_prefix(tasks, id_or_prefix)?;
    let t = tasks[&id].clone();
    let renamed = delete_detaching(base, tasks, &HashSet::from([id]))?;
    Ok(DeletedTask { task: t, renamed })
}

/// Prune cancelled (and optionally done) tasks, returning the deleted tasks.
///
/// References to pruned tasks are removed from remaining tasks, and the
/// children that survive a pruned epic are detached to fresh root IDs as in
/// [`delete_task`]. Children pruned together with their epic are deleted,
/// not moved.
pub fn prune_tasks(
    base: &Path,
    tasks: &HashMap<String, Task>,
    include_done: bool,
) -> Result<PrunedTasks> {
    let to_delete: Vec<Task> = tasks
        .values()
        .filter(|t| t.status == Status::Cancelled || (include_done && t.status == Status::Done))
        .cloned()
        .collect();
    let deleted: HashSet<String> = to_delete.iter().map(|t| t.id.clone()).collect();
    let renamed = delete_detaching(base, tasks, &deleted)?;
    Ok(PrunedTasks {
        tasks: to_delete,
        renamed,
    })
}

/// Delete the active tasks in `deleted`, detaching the tasks that survive
/// them first, and return the renames of the survivors.
///
/// Order matters for interruption safety: every rename happens first (a
/// deleted task inside a detached subtree moves with it), then the tasks are
/// deleted deepest first, so no state on the way has a task whose parent is
/// gone.
fn delete_detaching(
    base: &Path,
    tasks: &HashMap<String, Task>,
    deleted: &HashSet<String>,
) -> Result<BTreeMap<String, String>> {
    let renames = plan_detach(base, tasks, deleted)?;
    store::rename_ids(base, &renames)?;

    let mut now_deleted: Vec<String> = deleted
        .iter()
        .map(|id| renames.get(id).unwrap_or(id).clone())
        .collect();
    now_deleted.sort_by_key(|id| std::cmp::Reverse(task::ancestors(id).count()));
    for id in &now_deleted {
        store::delete(base, id)?;
    }
    scrub_references(
        base,
        &apply_renames(tasks, &renames),
        &now_deleted.into_iter().collect(),
    )?;

    Ok(renames
        .into_iter()
        .filter(|(old, _)| !deleted.contains(old))
        .collect())
}

/// Work out the renames that detach every task, active or archived, whose
/// parent is in `deleted` to a fresh root ID, its subtree moving with it.
/// Writes nothing.
///
/// A task below a detached task keeps its segments under the new root. That
/// includes tasks that are themselves in `deleted`, so they can still be
/// deleted under their new ID; when a detached task lies below another one,
/// the nearer one decides its new ID.
fn plan_detach(
    base: &Path,
    tasks: &HashMap<String, Task>,
    deleted: &HashSet<String>,
) -> Result<BTreeMap<String, String>> {
    let archived = store::archived_id_set(base);
    let known: HashSet<&String> = tasks.keys().chain(archived.iter()).collect();
    let mut detached: Vec<&String> = known
        .iter()
        .copied()
        .filter(|id| !deleted.contains(*id))
        .filter(|id| task::parent_of(id).is_some_and(|p| deleted.contains(p)))
        .collect();
    if detached.is_empty() {
        return Ok(BTreeMap::new());
    }
    detached.sort();

    let config = config::load(base)?;
    let mut reserved = store::reserved_ids(base, tasks);
    let new_roots: HashMap<&str, String> = detached
        .into_iter()
        .map(|id| {
            let new = task::generate_id(&reserved, config.id_length as usize);
            reserved.insert(new.clone());
            (id.as_str(), new)
        })
        .collect();

    Ok(known
        .into_iter()
        .filter_map(|id| {
            std::iter::once(id.as_str())
                .chain(task::ancestors(id))
                .find_map(|a| new_roots.get(a).map(|new| (a, new)))
                .map(|(a, new)| (id.clone(), rebase(id, a, new)))
        })
        .collect())
}

/// `id`, which is `from` or below it, with the `from` prefix replaced by `to`.
fn rebase(id: &str, from: &str, to: &str) -> String {
    format!("{to}{}", &id[from.len()..])
}

/// Remove dangling references to deleted tasks: drop deleted IDs from
/// `depends_on` lists. Without this, dependents would silently never become
/// ready.
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
        if t.depends_on.iter().any(|d| deleted.contains(d)) {
            let mut t = t.clone();
            t.depends_on.retain(|d| !deleted.contains(d));
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
                c.is_child_of(&id) && (c.status == Status::Done || c.status == Status::Cancelled)
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
            if let Some(parent_id) = task.parent()
                && archived.contains_key(parent_id)
                && !visited.contains(parent_id)
            {
                queue.push(parent_id.to_string());
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

/// Compute progress for an epic by counting its direct children (tasks whose
/// ID derives `epic_id` as their parent).
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
        if t.is_child_of(epic_id) {
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
        .filter(|t| t.is_child_of(&resolved))
        .map(|t| t.id.clone())
        .collect();

    let graph = Graph::build(tasks);
    let topo = graph.topo_sort_subset(&child_ids, tasks);
    Ok(EpicPlan {
        tasks: topo.sorted,
        cyclic: topo.cyclic,
    })
}

// ── Store check and repair (`bea doctor`) ──────────────────────────

/// One inconsistency [`check_store`] found. Paths are relative to the base
/// directory (`.bears/…`, `.bears/archive/…`).
///
/// Serialized with a `kind` tag in kebab case (`"kind": "duplicate-id"`) and
/// the variant's fields alongside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Finding {
    /// Task files still carry a legacy `parent:` key. Nothing else is
    /// checked until the store is migrated.
    LegacyStore { files: Vec<PathBuf> },
    /// A task file that cannot be read or parsed; every loader skips it.
    UnreadableFile { file: PathBuf, reason: String },
    /// A task ID that [`task::validate_id`] rejects.
    MalformedId {
        id: String,
        file: PathBuf,
        reason: String,
    },
    /// Two or more files, active or archived, carry the same ID. `files`
    /// lists the one that keeps the ID first, and `titles` the title in each
    /// file, in the same order. `dependents` (tasks whose `depends_on` names
    /// the ID) and `descendants` (tasks below the ID) cannot tell the copies
    /// apart; a repair leaves them all with the first.
    DuplicateId {
        id: String,
        files: Vec<PathBuf>,
        titles: Vec<String>,
        dependents: Vec<String>,
        descendants: Vec<String>,
    },
    /// A file whose name does not start with its task's ID, so lookups by ID
    /// cannot find it. `expected` is where the task belongs.
    FilenameMismatch {
        id: String,
        file: PathBuf,
        expected: PathBuf,
    },
    /// A `depends_on` entry naming no task, active or archived.
    DanglingDependency { id: String, depends_on: String },
    /// A task whose ID puts it under a parent that does not exist.
    MissingParent { id: String, parent: String },
    /// A task whose ID puts it under a task that is not an epic.
    ParentNotEpic { id: String, parent: String },
    /// Tasks that depend on each other in a cycle, sorted by ID.
    DependencyCycle { ids: Vec<String> },
}

impl Finding {
    /// Whether [`repair_store`] repairs this kind of finding.
    pub fn is_fixable(&self) -> bool {
        matches!(
            self,
            Finding::DuplicateId { .. } | Finding::FilenameMismatch { .. }
        )
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Finding::LegacyStore { files } => write!(
                f,
                "{} task file(s) carry a legacy parent: key: {}",
                files.len(),
                list(files)
            ),
            Finding::UnreadableFile { file, reason } => {
                write!(f, "cannot read {}: {reason}", file.display())
            }
            Finding::MalformedId { id, file, reason } => {
                write!(f, "malformed ID {id} in {}: {reason}", file.display())
            }
            Finding::DuplicateId {
                id,
                files,
                titles,
                dependents,
                descendants,
            } => {
                let copies: Vec<String> = files
                    .iter()
                    .zip(titles)
                    .map(|(p, t)| format!("{} ({t:?})", p.display()))
                    .collect();
                write!(f, "duplicate ID {id} in {}", copies.join(", "))?;
                if let Some(attached) = describe_attached(dependents, descendants) {
                    write!(f, "; {attached} (these cannot tell the copies apart)")?;
                }
                Ok(())
            }
            Finding::FilenameMismatch { id, file, expected } => write!(
                f,
                "{} holds task {id} but should be {}",
                file.display(),
                expected.display()
            ),
            Finding::DanglingDependency { id, depends_on } => {
                write!(f, "{id} depends on {depends_on}, which does not exist")
            }
            Finding::MissingParent { id, parent } => {
                write!(f, "{id} is under {parent}, which does not exist")
            }
            Finding::ParentNotEpic { id, parent } => {
                write!(f, "{id} is under {parent}, which is not an epic")
            }
            Finding::DependencyCycle { ids } => {
                write!(f, "dependency cycle: {}", ids.join(" ↔ "))
            }
        }
    }
}

/// "below it: a.b, a.c; depends on it: x", or `None` when both are empty.
fn describe_attached(dependents: &[String], descendants: &[String]) -> Option<String> {
    let mut parts = Vec::new();
    if !descendants.is_empty() {
        parts.push(format!("below it: {}", descendants.join(", ")));
    }
    if !dependents.is_empty() {
        parts.push(format!("depends on it: {}", dependents.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// A duplicate that [`repair_store`] gave a fresh ID.
///
/// References to the duplicated ID are not rewritten: nothing on disk says
/// which copy they meant. `dependents` and `descendants` are the tasks that
/// therefore stay attached to the copy that kept the ID (`kept_file`), and
/// that the user should check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Renumbered {
    /// The duplicated ID, which the other file keeps.
    pub from: String,
    /// The new ID of this task.
    pub to: String,
    /// The file the task now lives in, relative to the base.
    pub file: PathBuf,
    /// This task's title.
    pub title: String,
    /// The file of the copy that kept the ID, relative to the base.
    pub kept_file: PathBuf,
    /// The title of the copy that kept the ID.
    pub kept_title: String,
    /// Tasks whose `depends_on` names the ID and still points at the kept copy.
    pub dependents: Vec<String>,
    /// Tasks below the ID, which stay below the kept copy.
    pub descendants: Vec<String>,
}

impl Renumbered {
    /// Whether any task stayed attached to the kept copy and needs checking.
    pub fn has_attached(&self) -> bool {
        !self.dependents.is_empty() || !self.descendants.is_empty()
    }

    /// A description of the attached tasks ("below it: …; depends on it: …"),
    /// or `None` when there are none.
    pub fn describe_attached(&self) -> Option<String> {
        describe_attached(&self.dependents, &self.descendants)
    }
}

/// A file [`repair_store`] renamed to match its task's ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MovedFile {
    pub id: String,
    pub from: PathBuf,
    pub to: PathBuf,
}

/// What [`check_store`] or [`repair_store`] found and did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StoreReport {
    /// How many task files, active and archived, were checked.
    pub files: usize,
    /// The problems in the store; after a repair, those that remain.
    pub findings: Vec<Finding>,
    /// Duplicates given a fresh ID by a repair.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub renamed: Vec<Renumbered>,
    /// Files renamed to match their task's ID by a repair.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<MovedFile>,
}

impl StoreReport {
    /// Whether no problems were found (or remain, after a repair).
    pub fn is_healthy(&self) -> bool {
        self.findings.is_empty()
    }

    /// Whether a repair renumbered a copy that left tasks attached to the
    /// copy that kept the ID (see [`Renumbered::has_attached`]). Such a store
    /// can be healthy and still want a person to check those references.
    pub fn needs_review(&self) -> bool {
        self.renamed.iter().any(Renumbered::has_attached)
    }
}

/// Check the store, active and archive, for inconsistencies the loaders do
/// not catch or silently paper over. Read-only.
///
/// Reports, in this order: a legacy store (and nothing else, since its IDs
/// are about to change), files that cannot be parsed, malformed IDs,
/// duplicate IDs, files whose name does not match their ID, dangling
/// `depends_on` entries, tasks whose parent is missing or not an epic, and
/// dependency cycles. Each kind is sorted by ID or path.
pub fn check_store(base: &Path) -> Result<StoreReport> {
    let files = store::scan_task_files(base)?;
    Ok(StoreReport {
        files: files.len(),
        findings: StoreScan::new(&files).findings(base),
        ..StoreReport::default()
    })
}

/// Repair what can be repaired safely, then check the store again.
///
/// Of every set of files sharing an ID, one keeps it: the active file before
/// an archived one, then the file whose name starts with the ID, then the
/// first by path (the file a lookup by that ID already finds). Every other
/// copy gets a fresh ID at the same place in the hierarchy (a child ID under
/// the same parent, or a root ID), keeping its title, body and `updated`.
///
/// References are deliberately **not** rewritten: a `depends_on` naming the
/// duplicated ID, or a task below it, records only the ID, so nothing says
/// which copy it meant, and guessing would silently move it. They all stay
/// with the copy that keeps the ID, and each [`Renumbered`] lists them
/// (`dependents`, `descendants`) beside both copies' files and titles, so the
/// user can check them and move any that belong to the renumbered copy.
/// Duplicates of a malformed ID are left alone.
///
/// A file whose name does not match its ID is renamed to `{id}-{slug}.md`,
/// unless another file is already there.
///
/// Nothing else is changed: the returned report's findings are what remain.
/// A legacy store is not touched at all.
pub fn repair_store(base: &Path) -> Result<StoreReport> {
    let files = store::scan_task_files(base)?;
    let scan = StoreScan::new(&files);
    if !scan.legacy.is_empty() {
        return Ok(StoreReport {
            files: files.len(),
            findings: scan.findings(base),
            ..StoreReport::default()
        });
    }
    let config = config::load(base)?;

    let mut pool = task::with_ancestors(scan.parsed().map(|(_, t)| t.id.as_str()).chain(
        files.iter().filter_map(|f| {
            f.path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(task::id_from_filename)
        }),
    ));

    // (file index, task to write there)
    let mut plan: Vec<(usize, Task)> = Vec::new();
    let mut renumbered = HashSet::new();
    for (id, group) in &scan.by_id {
        if group.len() < 2 || !task::is_valid_id(id) {
            continue;
        }
        for &i in &group[1..] {
            let new_id = match task::parent_of(id) {
                Some(parent) => task::generate_child_id(parent, &pool, &config),
                None => task::generate_id(&pool, config.id_length as usize),
            };
            pool.insert(new_id.clone());
            let mut t = scan.task(i).clone();
            t.id = new_id;
            plan.push((i, t));
            renumbered.insert(i);
        }
    }
    for (i, t) in scan.parsed() {
        if renumbered.contains(&i) || !task::is_valid_id(&t.id) || filename_matches(&files[i], t) {
            continue;
        }
        if files[i].path.with_file_name(task::filename(t)).exists() {
            continue;
        }
        plan.push((i, t.clone()));
    }

    let mut written: HashMap<usize, PathBuf> = HashMap::new();
    let mut renamed_files = Vec::new();
    let mut moved = Vec::new();
    for (i, t) in plan {
        let from = &files[i].path;
        let to = store::rewrite_task_file(from, &t)?;
        if renumbered.contains(&i) {
            renamed_files.push((i, t.id));
        } else {
            moved.push(MovedFile {
                id: t.id,
                from: relative(base, from),
                to: relative(base, &to),
            });
        }
        written.insert(i, to);
    }
    let final_path = |i: usize| relative(base, written.get(&i).unwrap_or(&files[i].path));
    let renamed = renamed_files
        .into_iter()
        .map(|(i, to)| {
            let old = scan.task(i);
            let kept = scan.by_id[old.id.as_str()][0];
            let (dependents, descendants) = scan.attached(&old.id);
            Renumbered {
                from: old.id.clone(),
                to,
                file: final_path(i),
                title: old.title.clone(),
                kept_file: final_path(kept),
                kept_title: scan.task(kept).title.clone(),
                dependents,
                descendants,
            }
        })
        .collect();

    let mut report = check_store(base)?;
    report.renamed = renamed;
    report.moved = moved;
    Ok(report)
}

/// `path` relative to `base`, or unchanged when it is not under it.
fn relative(base: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(base).unwrap_or(path).to_path_buf()
}

/// Whether the name of `file` starts with the ID of `t`, its parsed task.
fn filename_matches(file: &store::TaskFile, t: &Task) -> bool {
    file.path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(task::id_from_filename)
        == Some(t.id.as_str())
}

/// The scanned task files of a store, indexed for [`check_store`] and
/// [`repair_store`].
struct StoreScan<'a> {
    files: &'a [store::TaskFile],
    /// Files carrying a legacy `parent:` key.
    legacy: Vec<usize>,
    /// ID → indices of the files carrying it, the one that keeps it first.
    by_id: BTreeMap<&'a str, Vec<usize>>,
}

impl<'a> StoreScan<'a> {
    fn new(files: &'a [store::TaskFile]) -> Self {
        let mut legacy = Vec::new();
        let mut by_id: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, f) in files.iter().enumerate() {
            if let Ok(t) = &f.task {
                if t.legacy_parent.is_some() {
                    legacy.push(i);
                }
                by_id.entry(t.id.as_str()).or_default().push(i);
            }
        }
        // Files are active first, then by path; a file named after its ID
        // goes before one that is not, within the same directory.
        for group in by_id.values_mut() {
            group.sort_by_key(|&i| {
                let f = &files[i];
                let named = f.task.as_ref().is_ok_and(|t| filename_matches(f, t));
                (f.archived, !named, i)
            });
        }
        StoreScan {
            files,
            legacy,
            by_id,
        }
    }

    /// The parsed task of file `i`, which must be one.
    fn task(&self, i: usize) -> &'a Task {
        self.files[i]
            .task
            .as_ref()
            .expect("indexed files are parsed")
    }

    /// The tasks that refer to `id` without saying which copy they mean:
    /// those whose `depends_on` names it, and those below it. Both sorted,
    /// `id` itself excluded.
    fn attached(&self, id: &str) -> (Vec<String>, Vec<String>) {
        let dependents: std::collections::BTreeSet<&str> = self
            .parsed()
            .filter(|(_, t)| t.id != id && t.depends_on.iter().any(|d| d == id))
            .map(|(_, t)| t.id.as_str())
            .collect();
        let descendants = self
            .by_id
            .keys()
            .filter(|k| task::is_descendant(k, id))
            .map(|k| k.to_string())
            .collect();
        (
            dependents.into_iter().map(String::from).collect(),
            descendants,
        )
    }

    /// Every parsed file, by index.
    fn parsed(&self) -> impl Iterator<Item = (usize, &'a Task)> + '_ {
        self.files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.task.as_ref().ok().map(|t| (i, t)))
    }

    fn findings(&self, base: &Path) -> Vec<Finding> {
        let path = |i: usize| relative(base, &self.files[i].path);
        if !self.legacy.is_empty() {
            return vec![Finding::LegacyStore {
                files: self.legacy.iter().map(|&i| path(i)).collect(),
            }];
        }
        let mut findings = Vec::new();

        for (i, f) in self.files.iter().enumerate() {
            if let Err(reason) = &f.task {
                findings.push(Finding::UnreadableFile {
                    file: path(i),
                    reason: reason.clone(),
                });
            }
        }
        for (i, t) in self.parsed() {
            if let Err(Error::InvalidId { reason, .. }) = task::validate_id(&t.id) {
                findings.push(Finding::MalformedId {
                    id: t.id.clone(),
                    file: path(i),
                    reason,
                });
            }
        }
        for (id, group) in &self.by_id {
            if group.len() > 1 {
                let (dependents, descendants) = self.attached(id);
                findings.push(Finding::DuplicateId {
                    id: id.to_string(),
                    files: group.iter().map(|&i| path(i)).collect(),
                    titles: group.iter().map(|&i| self.task(i).title.clone()).collect(),
                    dependents,
                    descendants,
                });
            }
        }
        for (i, t) in self.parsed() {
            if task::is_valid_id(&t.id) && !filename_matches(&self.files[i], t) {
                findings.push(Finding::FilenameMismatch {
                    id: t.id.clone(),
                    file: path(i),
                    expected: relative(base, &self.files[i].path.with_file_name(task::filename(t))),
                });
            }
        }

        // The graph checks see each ID once, as the task that keeps it.
        let primary: HashMap<String, Task> = self
            .by_id
            .iter()
            .map(|(id, group)| (id.to_string(), self.task(group[0]).clone()))
            .collect();

        let dangling: std::collections::BTreeSet<(&str, &str)> = self
            .parsed()
            .flat_map(|(_, t)| {
                t.depends_on
                    .iter()
                    .filter(|d| !self.by_id.contains_key(d.as_str()))
                    .map(|d| (t.id.as_str(), d.as_str()))
            })
            .collect();
        findings.extend(
            dangling
                .into_iter()
                .map(|(id, dep)| Finding::DanglingDependency {
                    id: id.into(),
                    depends_on: dep.into(),
                }),
        );

        let mut not_epic = Vec::new();
        for id in self.by_id.keys() {
            let Some(parent) = task::parent_of(id) else {
                continue;
            };
            match primary.get(parent) {
                None => findings.push(Finding::MissingParent {
                    id: id.to_string(),
                    parent: parent.into(),
                }),
                Some(p) if !p.task_type.is_epic() => not_epic.push(Finding::ParentNotEpic {
                    id: id.to_string(),
                    parent: parent.into(),
                }),
                Some(_) => {}
            }
        }
        findings.extend(not_epic);

        findings.extend(
            Graph::build(&primary)
                .cycles()
                .into_iter()
                .map(|ids| Finding::DependencyCycle { ids }),
        );
        findings
    }
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

    /// A task whose ID puts it directly under `parent`: `{parent}.{leaf}`.
    fn make_child(leaf: &str, parent: &str, status: Status) -> Task {
        make_task(&format!("{parent}.{leaf}"), status)
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
        .unwrap()
        .task;
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
        assert!(
            t.assignee.is_empty(),
            "reject hands the task back to the pool"
        );
        // The task no longer belongs to worker-1, so its fence now refuses it.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let err = release_task_fenced(tmp.path(), &tasks, "aaa", "worker-1").unwrap_err();
        assert!(matches!(err, Error::FenceViolation { .. }));
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
        let c = make_child("cc", "aaa", Status::Open);
        store::save(tmp.path(), &c).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let deleted = delete_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(deleted.task.id, "aaa");

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks["bbb"].depends_on.is_empty(), "dep should be scrubbed");
        // The child is detached to a fresh root ID instead of being orphaned.
        let new = &deleted.renamed["aaa.cc"];
        assert_eq!(deleted.renamed.len(), 1);
        assert!(!tasks.contains_key("aaa.cc"));
        assert_eq!(tasks[new].parent(), None);
        assert_eq!(tasks[new].title, "Task aaa.cc");
        assert!(!tasks.contains_key("aaa"));
        // And the dependent is now ready instead of silently blocked.
        let ready = list_ready(&tasks, None, None, None);
        assert!(ready.iter().any(|t| t.id == "bbb"));
    }

    #[tokio::test]
    async fn test_delete_without_children_renames_nothing() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_epic("aaa")).unwrap();
        store::save(tmp.path(), &make_task("bbb", Status::Open)).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let deleted = delete_task(tmp.path(), &tasks, "aaa").unwrap();
        assert!(deleted.renamed.is_empty());
        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.keys().collect::<Vec<_>>(), vec!["bbb"]);
    }

    #[tokio::test]
    async fn test_delete_epic_detaches_subtrees_and_archived_children() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        store::save(tmp.path(), &make_epic("aaa")).unwrap();
        let mut sub = make_child("ee", "aaa", Status::Open);
        sub.task_type = TaskType::Epic;
        store::save(tmp.path(), &sub).unwrap();
        store::save(tmp.path(), &make_child("gg", "aaa.ee", Status::Open)).unwrap();
        store::save(tmp.path(), &make_child("dd", "aaa", Status::Done)).unwrap();
        store::move_to_archive(tmp.path(), "aaa.dd").unwrap();
        // References from active and archived tasks into the moved subtrees.
        let mut user = make_task("uuu", Status::Open);
        user.depends_on = vec!["aaa.ee.gg".into(), "aaa.dd".into()];
        store::save(tmp.path(), &user).unwrap();
        let mut old = make_task("ooo", Status::Done);
        old.depends_on = vec!["aaa.ee.gg".into()];
        store::save(tmp.path(), &old).unwrap();
        store::move_to_archive(tmp.path(), "ooo").unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let renamed = delete_task(tmp.path(), &tasks, "aaa").unwrap().renamed;

        let ee = renamed["aaa.ee"].clone();
        let dd = renamed["aaa.dd"].clone();
        assert_eq!(renamed["aaa.ee.gg"], format!("{ee}.gg"));
        assert_eq!(renamed.len(), 3, "{renamed:?}");
        assert_ne!(ee, dd);
        assert!(task::parent_of(&ee).is_none() && task::parent_of(&dd).is_none());

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert!(tasks[&ee].task_type.is_epic());
        assert!(tasks.contains_key(&format!("{ee}.gg")));
        assert!(!tasks.keys().any(|id| id.starts_with("aaa")));
        assert_eq!(
            tasks["uuu"].depends_on,
            vec![format!("{ee}.gg"), dd.clone()]
        );
        let archived = store::load_archived(tmp.path()).await.unwrap();
        assert!(
            archived.contains_key(&dd),
            "archived child renamed in place"
        );
        assert!(!archived.contains_key("aaa.dd"));
        assert_eq!(archived["ooo"].depends_on, vec![format!("{ee}.gg")]);
        // The detached epic's status is left alone.
        assert_eq!(tasks[&ee].status, Status::Open);
    }

    #[tokio::test]
    async fn test_prune_detaches_only_surviving_children() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let mut epic = make_epic("aaa");
        epic.status = Status::Cancelled;
        store::save(tmp.path(), &epic).unwrap();
        store::save(tmp.path(), &make_child("xx", "aaa", Status::Cancelled)).unwrap();
        store::save(tmp.path(), &make_child("kk", "aaa", Status::Open)).unwrap();
        // A pruned task inside a surviving subtree, with a survivor below it.
        store::save(tmp.path(), &make_child("pp", "aaa.kk", Status::Cancelled)).unwrap();
        let mut deep = make_child("qq", "aaa.kk.pp", Status::Open);
        deep.depends_on = vec!["aaa.kk.pp".into(), "aaa.kk".into()];
        store::save(tmp.path(), &deep).unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let pruned = prune_tasks(tmp.path(), &tasks, false).unwrap();

        let mut gone: Vec<_> = pruned.tasks.iter().map(|t| t.id.as_str()).collect();
        gone.sort();
        assert_eq!(gone, vec!["aaa", "aaa.kk.pp", "aaa.xx"]);
        // Only survivors appear in the mapping.
        let mut moved: Vec<_> = pruned.renamed.keys().map(String::as_str).collect();
        moved.sort();
        assert_eq!(moved, vec!["aaa.kk", "aaa.kk.pp.qq"]);
        let kk = &pruned.renamed["aaa.kk"];
        let qq = &pruned.renamed["aaa.kk.pp.qq"];
        assert!(task::parent_of(kk).is_none() && task::parent_of(qq).is_none());

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let mut ids: Vec<_> = tasks.keys().cloned().collect();
        ids.sort();
        let mut expected = vec![kk.clone(), qq.clone()];
        expected.sort();
        assert_eq!(ids, expected);
        assert_eq!(
            tasks[qq].depends_on,
            vec![kk.clone()],
            "pruned dep scrubbed"
        );
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

    /// A store with a nested epic: `e1` holds the task `e1.a` and the epic
    /// `e1.s`, which holds `e1.s.t`. `e1x.z` shares a string prefix with `e1`
    /// but is not under it, and `zz` is a root task.
    fn nested_epic_store(status: Status) -> HashMap<String, Task> {
        task_map(vec![
            make_epic("e1"),
            make_child("a", "e1", status),
            make_epic("e1.s"),
            make_child("t", "e1.s", status),
            make_task("e1x.z", status),
            make_task("zz", status),
        ])
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        let mut ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn test_epic_filters_match_every_descendant() {
        let tasks = nested_epic_store(Status::Open);
        let listed = list_tasks(&tasks, None, None, None, true, false, Some("e1"));
        assert_eq!(ids(&listed), ["e1.a", "e1.s", "e1.s.t"]);

        let ready = list_ready(&tasks, None, None, Some("e1"));
        assert_eq!(ids(&ready), ["e1.a", "e1.s.t"], "epics are never ready");

        let tasks = nested_epic_store(Status::Review);
        let queue = list_review(&tasks, None, None, Some("e1"));
        assert_eq!(ids(&queue), ["e1.a", "e1.s.t"]);

        // The nested epic filters its own subtree only.
        let ready = list_ready(&nested_epic_store(Status::Open), None, None, Some("e1.s"));
        assert_eq!(ids(&ready), ["e1.s.t"]);
    }

    #[tokio::test]
    async fn test_resolve_epic_filter_resolves_like_other_id_arguments() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        for t in nested_epic_store(Status::Open).into_values() {
            store::save(base, &t).unwrap();
        }
        store::save(base, &make_epic("q7")).unwrap();
        store::save(base, &make_child("k", "q7", Status::Open)).unwrap();
        let tasks = store::load_all(base).await.unwrap();
        let resolve = async |v: &str| resolve_epic_filter(base, &tasks, v).await;

        assert_eq!(resolve("e1").await.unwrap(), "e1");
        assert_eq!(resolve("e1.s").await.unwrap(), "e1.s");
        // A prefix of an epic resolves past the epic's own children.
        assert_eq!(resolve("q").await.unwrap(), "q7");
        assert!(matches!(
            resolve("e1.").await,
            Err(Error::AmbiguousPrefix { .. })
        ));
        assert!(matches!(
            resolve("e").await,
            Err(Error::AmbiguousPrefix { .. })
        ));
        assert!(matches!(resolve("e1.a").await, Err(Error::NotAnEpic(id)) if id == "e1.a"));
        assert!(matches!(resolve("zz").await, Err(Error::NotAnEpic(id)) if id == "zz"));
        assert!(matches!(resolve("nope").await, Err(Error::TaskNotFound(_))));
        assert!(matches!(resolve("").await, Err(Error::InvalidId { .. })));
    }

    /// An epic that lives only in the archive still filters its active
    /// children, by its full ID or a prefix: the prefix resolves against
    /// active and archived IDs together, so it names the epic and not its
    /// one active child.
    #[tokio::test]
    async fn test_resolve_epic_filter_finds_an_archived_epic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        let mut epic = make_epic("arc");
        epic.status = Status::Done;
        store::save(base, &epic).unwrap();
        store::save(base, &make_child("ch", "arc", Status::Open)).unwrap();
        store::move_to_archive(base, "arc").unwrap();
        let tasks = store::load_all(base).await.unwrap();

        for value in ["arc", "ar"] {
            let epic = resolve_epic_filter(base, &tasks, value).await.unwrap();
            assert_eq!(epic, "arc", "{value}");
            assert_eq!(
                ids(&list_ready(&tasks, None, None, Some(&epic))),
                ["arc.ch"]
            );
        }
    }

    /// Progress counts direct children: the nested epic is one unit of work.
    #[test]
    fn test_epic_progress_counts_direct_children_only() {
        let mut tasks = nested_epic_store(Status::Done);
        tasks.get_mut("e1.s").unwrap().status = Status::Open;
        tasks.insert("e1.s.u".into(), make_child("u", "e1.s", Status::Open));
        let p = epic_progress(&tasks, "e1");
        assert_eq!((p.done, p.total), (1, 2));
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

    /// Auto-close looks at direct children only: a nested epic that is itself
    /// resolved counts as resolved, whatever is still open below it.
    #[tokio::test]
    async fn test_epic_auto_close_looks_at_direct_children_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        let mut sub = make_epic("e1.s");
        sub.status = Status::Done;
        for t in [
            make_epic("e1"),
            make_child("a", "e1", Status::Open),
            sub,
            make_child("t", "e1.s", Status::Open),
        ] {
            store::save(base, &t).unwrap();
        }
        let tasks = store::load_all(base).await.unwrap();

        set_status(base, &tasks, "e1.a", Status::Done).unwrap();
        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks["e1"].status, Status::Done);
        assert_eq!(tasks["e1.s.t"].status, Status::Open);
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
        c2.depends_on = vec!["e1.c1".to_string()];
        let mut c3 = make_child("c3", "e1", Status::Open);
        c3.depends_on = vec!["e1.c2".to_string()];

        let tasks = task_map(vec![make_epic("e1"), c1, c2, c3]);
        let plan = plan_epic(&tasks, "e1").unwrap();
        let ids: Vec<&str> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["e1.c1", "e1.c2", "e1.c3"]);
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
    async fn test_create_child_gets_hierarchical_id() {
        // A parent passed as a prefix is resolved to the full id, which becomes
        // the prefix of the child's ID, so epic_progress counts the child.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();

        let mut epic = Task::new("epicid".into(), "Epic".into(), Priority::P1);
        epic.task_type = TaskType::Epic;
        store::save(tmp.path(), &epic).unwrap();

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
        assert_eq!(created.parent(), Some("epicid"), "{}", created.id);
        let segment = task::leaf_segment(&created.id);
        assert_eq!(segment.len(), 2, "default child-id-length: {}", created.id);

        // A nested epic's child is a grandchild of the top epic.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let nested = create_task(
            tmp.path(),
            &tasks,
            "Nested epic".into(),
            Priority::P2,
            vec![],
            vec![],
            Some("epicid".into()),
            String::new(),
            TaskType::Epic,
        )
        .unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let grandchild = create_task(
            tmp.path(),
            &tasks,
            "Grandchild".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(nested.id.clone()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(grandchild.parent(), Some(nested.id.as_str()));
        assert_eq!(task::id_depth(&grandchild.id), 2);

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(epic_progress(&tasks, "epicid").total, 2);
        assert_eq!(epic_progress(&tasks, &nested.id).total, 1);
        // The file carries no `parent:` key.
        let path = store::find_task_path(tmp.path(), &created.id).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(!content.contains("parent"), "{content}");
    }

    #[tokio::test]
    async fn test_create_root_task_uses_id_length() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let created = create_task(
            tmp.path(),
            &HashMap::new(),
            "Root".into(),
            Priority::P2,
            vec![],
            vec![],
            Some(String::new()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(created.parent(), None);
        assert_eq!(created.id.len(), 3);
    }

    #[tokio::test]
    async fn test_create_child_avoids_archived_ids() {
        // With child-id-length 1 an epic has 31 possible one-char children.
        // Seven of them sit in the archive (under the growth limit, so the
        // segment stays one char): a new child must take none of them.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        std::fs::write(tmp.path().join(".bears.yml"), "child-id-length: 1\n").unwrap();
        store::save(tmp.path(), &make_epic("eee")).unwrap();
        let archived: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"]
            .iter()
            .map(|leaf| {
                let child = make_child(leaf, "eee", Status::Done);
                store::save(tmp.path(), &child).unwrap();
                store::move_to_archive(tmp.path(), &child.id).unwrap();
                child.id
            })
            .collect();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let created = create_task(
            tmp.path(),
            &tasks,
            "Child".into(),
            Priority::P2,
            vec![],
            vec![],
            Some("eee".into()),
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(created.parent(), Some("eee"));
        assert_eq!(task::leaf_segment(&created.id).len(), 1, "{}", created.id);
        assert!(!archived.contains(&created.id), "{}", created.id);
    }

    #[tokio::test]
    async fn test_create_root_task_skips_ancestor_of_orphan() {
        // Every two-char root except `kk` is the deleted parent of an orphan,
        // half of them active and half archived: the only root left is `kk`.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        std::fs::write(tmp.path().join(".bears.yml"), "id-length: 2\n").unwrap();
        let chars = "abcdefghjkmnpqrstuvwxyz23456789";
        let roots = chars
            .chars()
            .flat_map(|a| chars.chars().map(move |b| format!("{a}{b}")))
            .filter(|root| root != "kk");
        for (i, root) in roots.enumerate() {
            let orphan = make_task(&format!("{root}.aa"), Status::Done);
            store::save(tmp.path(), &orphan).unwrap();
            if i % 2 == 0 {
                store::move_to_archive(tmp.path(), &orphan.id).unwrap();
            }
        }

        let tasks = store::load_all(tmp.path()).await.unwrap();
        let created = create_task(
            tmp.path(),
            &tasks,
            "Root".into(),
            Priority::P2,
            vec![],
            vec![],
            None,
            String::new(),
            TaskType::Task,
        )
        .unwrap();
        assert_eq!(created.id, "kk");
    }

    #[tokio::test]
    async fn test_reserved_ids_include_ancestors_of_orphans() {
        // Orphans `abc.c2.d7` (active) and `abc.c3.d1` (archived) were left
        // behind when nested epics `abc.c2` and `abc.c3` were deleted: a new
        // child of `abc` must take neither ID.
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_epic("abc")).unwrap();
        store::save(tmp.path(), &make_task("abc.c2.d7", Status::Open)).unwrap();
        store::save(tmp.path(), &make_task("abc.c3.d1", Status::Done)).unwrap();
        store::move_to_archive(tmp.path(), "abc.c3.d1").unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        let reserved = store::reserved_ids(tmp.path(), &tasks);
        for id in ["abc", "abc.c2", "abc.c2.d7", "abc.c3", "abc.c3.d1"] {
            assert!(reserved.contains(id), "{id} should be reserved");
        }
        // The reserved slots count as used siblings, and the generator never
        // returns a reserved ID.
        for _ in 0..20 {
            let created = create_task(
                tmp.path(),
                &store::load_all(tmp.path()).await.unwrap(),
                "Child".into(),
                Priority::P2,
                vec![],
                vec![],
                Some("abc".into()),
                String::new(),
                TaskType::Task,
            )
            .unwrap();
            assert_eq!(created.parent(), Some("abc"));
            assert!(!["abc.c2", "abc.c3"].contains(&created.id.as_str()));
        }
    }

    /// Call `update_task` changing only the parent (and the title, so the
    /// field edits can be seen to land on the moved task).
    fn move_task(
        base: &Path,
        tasks: &HashMap<String, Task>,
        id: &str,
        parent: Option<&str>,
        status: Option<Status>,
    ) -> Result<UpdatedTask> {
        update_task(
            base,
            tasks,
            id,
            status,
            None,
            None,
            None,
            None,
            Some("Moved".into()),
            Some(parent.map(String::from)),
        )
    }

    fn archive_file(base: &Path, t: &Task) {
        std::fs::write(
            store::archive_dir(base).join(task::filename(t)),
            task::render_task(t),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_update_task_moves_subtree_and_rewrites_references() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        store::save(base, &make_epic("e1")).unwrap();
        store::save(base, &make_epic("e2")).unwrap();
        store::save(base, &make_epic("e1.ab")).unwrap();
        store::save(base, &make_task("e1.ab.cd", Status::Open)).unwrap();
        store::save(base, &make_task("e1.zz", Status::Open)).unwrap();
        let mut user = make_task("uuu", Status::Open);
        user.depends_on = vec!["e1.ab.cd".into(), "e1.zz".into()];
        store::save(base, &user).unwrap();
        // An archived descendant and an archived dependant.
        archive_file(base, &make_task("e1.ab.ef", Status::Done));
        let mut archived_user = make_task("aaa", Status::Done);
        archived_user.depends_on = vec!["e1.ab".into()];
        archive_file(base, &archived_user);
        let tasks = store::load_all(base).await.unwrap();

        let updated = move_task(base, &tasks, "e1.ab", Some("e2"), None).unwrap();

        let new_id = updated.task.id.clone();
        assert_eq!(task::parent_of(&new_id), Some("e2"), "{new_id}");
        assert_eq!(updated.task.title, "Moved");
        assert_eq!(updated.task.task_type, TaskType::Epic);
        let expected: BTreeMap<String, String> = [
            ("e1.ab".to_string(), new_id.clone()),
            ("e1.ab.cd".to_string(), format!("{new_id}.cd")),
            ("e1.ab.ef".to_string(), format!("{new_id}.ef")),
        ]
        .into_iter()
        .collect();
        assert_eq!(updated.renamed, expected);

        let active = store::load_all(base).await.unwrap();
        let mut ids: Vec<_> = active.keys().cloned().collect();
        ids.sort();
        let mut want = vec![
            "e1".to_string(),
            "e1.zz".into(),
            "e2".into(),
            "uuu".into(),
            new_id.clone(),
            format!("{new_id}.cd"),
        ];
        want.sort();
        assert_eq!(ids, want);
        assert_eq!(active[&new_id].title, "Moved");
        assert_eq!(
            active["uuu"].depends_on,
            [format!("{new_id}.cd"), "e1.zz".to_string()]
        );
        assert!(active[&format!("{new_id}.cd")].is_child_of(&new_id));

        let archived = store::load_archived(base).await.unwrap();
        assert!(archived.contains_key(&format!("{new_id}.ef")));
        assert!(!archived.contains_key("e1.ab.ef"));
        assert_eq!(archived["aaa"].depends_on, vec![new_id]);
    }

    #[tokio::test]
    async fn test_update_task_detach_gives_a_fresh_root_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        store::save(base, &make_epic("e1")).unwrap();
        store::save(base, &make_child("c1", "e1", Status::Open)).unwrap();
        store::save(base, &make_child("c2", "e1", Status::Open)).unwrap();

        // `None` and `Some("")` both mean the top level.
        for (id, parent) in [("e1.c1", None), ("e1.c2", Some(""))] {
            let tasks = store::load_all(base).await.unwrap();
            let reserved = store::reserved_ids(base, &tasks);
            let updated = move_task(base, &tasks, id, parent, None).unwrap();
            let new_id = &updated.task.id;
            assert_eq!(task::id_depth(new_id), 0, "{new_id}");
            assert_eq!(new_id.len(), config::load(base).unwrap().id_length as usize);
            assert!(!reserved.contains(new_id), "{new_id} was taken");
            assert_eq!(
                updated.renamed,
                BTreeMap::from([(id.to_string(), new_id.clone())])
            );
            assert!(store::find_task_path(base, id).is_err());
            assert_eq!(store::load_one(base, new_id).unwrap().title, "Moved");
        }
    }

    #[tokio::test]
    async fn test_update_task_moves_a_root_task_under_an_epic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        store::save(base, &make_epic("e1")).unwrap();
        store::save(base, &make_task("rrr", Status::Open)).unwrap();
        let tasks = store::load_all(base).await.unwrap();

        let updated = move_task(base, &tasks, "rrr", Some("e1"), None).unwrap();
        let new_id = &updated.task.id;
        assert_eq!(task::parent_of(new_id), Some("e1"));
        let child_length = config::load(base).unwrap().child_id_length as usize;
        assert_eq!(task::leaf_segment(new_id).len(), child_length);
        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(
            list_tasks(&tasks, None, None, None, true, false, Some("e1")).len(),
            1
        );
    }

    #[tokio::test]
    async fn test_update_task_move_applies_status_side_effects_at_the_new_parent() {
        // A done task moved into an epic whose other children are all done
        // closes that epic, as finishing it there would have.
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        store::save(base, &make_epic("e1")).unwrap();
        store::save(base, &make_epic("e2")).unwrap();
        store::save(base, &make_child("c1", "e1", Status::InProgress)).unwrap();
        store::save(base, &make_child("c2", "e1", Status::Open)).unwrap();
        store::save(base, &make_child("d1", "e2", Status::Done)).unwrap();
        let tasks = store::load_all(base).await.unwrap();

        let updated = move_task(base, &tasks, "e1.c2", Some("e2"), Some(Status::Done)).unwrap();
        assert_eq!(updated.task.status, Status::Done);
        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks["e2"].status, Status::Done, "e2 should auto-close");
        assert_eq!(tasks["e1"].status, Status::Open);
    }

    #[tokio::test]
    async fn test_update_task_rejects_invalid_parents_before_writing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        store::save(base, &make_epic("e1")).unwrap();
        store::save(base, &make_epic("e1.ab")).unwrap();
        store::save(base, &make_task("e1.ab.cd", Status::Open)).unwrap();
        store::save(base, &make_task("rrr", Status::Open)).unwrap();
        let tasks = store::load_all(base).await.unwrap();

        // Under itself or under one of its own descendants.
        for (id, parent) in [("e1", "e1"), ("e1", "e1.ab"), ("e1.ab", "e1.ab")] {
            let err = move_task(base, &tasks, id, Some(parent), None).unwrap_err();
            match &err {
                Error::ParentInSubtree { id: got, parent: p } => {
                    assert_eq!((got.as_str(), p.as_str()), (id, parent));
                }
                other => panic!("{id} under {parent}: {other:?}"),
            }
            assert!(err.to_string().contains(id), "{err}");
        }
        assert!(matches!(
            move_task(base, &tasks, "e1.ab.cd", Some("rrr"), None),
            Err(Error::ParentNotEpic(p)) if p == "rrr"
        ));
        assert!(matches!(
            move_task(base, &tasks, "rrr", Some("zzz"), None),
            Err(Error::TaskNotFound(_))
        ));

        // Nothing was written.
        let reloaded = store::load_all(base).await.unwrap();
        assert_eq!(
            reloaded.keys().collect::<HashSet<_>>(),
            tasks.keys().collect::<HashSet<_>>()
        );
        assert!(reloaded.values().all(|t| t.title != "Moved"));
    }

    #[tokio::test]
    async fn test_update_task_accepts_unchanged_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        store::save(tmp.path(), &make_epic("e1")).unwrap();
        store::save(tmp.path(), &make_child("c1", "e1", Status::Open)).unwrap();
        store::save(tmp.path(), &make_task("rrr", Status::Open)).unwrap();
        let tasks = store::load_all(tmp.path()).await.unwrap();

        for (id, parent) in [
            ("e1.c1", Some("e1".to_string())),
            ("rrr", None),
            ("rrr", Some(String::new())),
        ] {
            let updated = update_task(
                tmp.path(),
                &tasks,
                id,
                None,
                Some(Priority::P0),
                None,
                None,
                None,
                None,
                Some(parent),
            )
            .unwrap()
            .task;
            assert_eq!(updated.id, id);
            assert_eq!(updated.priority, Priority::P0);
        }
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

    /// Archiving an epic takes its settled direct children along, not the
    /// tasks below a nested epic.
    #[tokio::test]
    async fn test_archive_task_epic_cascades_to_direct_children_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        store::init(base).unwrap();
        for mut t in nested_epic_store(Status::Done).into_values() {
            t.status = Status::Done;
            store::save(base, &t).unwrap();
        }
        let tasks = store::load_all(base).await.unwrap();

        let mut archived = archive_task(base, &tasks, "e1").unwrap();
        archived.sort();
        assert_eq!(archived, ["e1", "e1.a", "e1.s"]);
        let active = store::load_all(base).await.unwrap();
        assert!(active.contains_key("e1.s.t"));
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
        .unwrap()
        .task;
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
        .unwrap()
        .task;
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

        // Reviewer wants changes: back to the pool — open, assignee cleared.
        let tasks = store::load_all(tmp.path()).await.unwrap();
        let t = reject_task(tmp.path(), &tasks, "aaa").unwrap();
        assert_eq!(t.status, Status::Open);
        assert!(
            t.assignee.is_empty(),
            "reject hands the task back to the pool"
        );
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
        review_task(tmp.path(), &tasks, "eee.bbb").unwrap();

        let tasks = store::load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks["eee"].status,
            Status::Open,
            "an epic must not auto-close on unreviewed work"
        );
    }

    // ── check_store / repair_store ──────────────────────────────────

    /// Write `t` to `.bears/{name}` (or `.bears/archive/{name}`) as is,
    /// whatever its ID says the filename should be.
    fn write_raw(base: &Path, name: &str, t: &Task, archived: bool) {
        let dir = if archived {
            store::archive_dir(base)
        } else {
            store::tasks_dir(base)
        };
        std::fs::write(dir.join(name), task::render_task(t)).unwrap();
    }

    fn doctor_store() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        tmp
    }

    fn kinds(report: &StoreReport) -> Vec<String> {
        report
            .findings
            .iter()
            .map(|f| {
                serde_json::to_value(f).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn test_store_report_needs_review() {
        let renumbered = |dependents: &[&str], descendants: &[&str]| Renumbered {
            from: "aaa".into(),
            to: "bbb".into(),
            file: PathBuf::from(".bears/bbb-copy.md"),
            title: "Copy".into(),
            kept_file: PathBuf::from(".bears/aaa-original.md"),
            kept_title: "Original".into(),
            dependents: dependents.iter().map(|s| s.to_string()).collect(),
            descendants: descendants.iter().map(|s| s.to_string()).collect(),
        };
        let report = |renamed| StoreReport {
            renamed,
            ..StoreReport::default()
        };

        assert!(!report(vec![]).needs_review());
        assert!(!report(vec![renumbered(&[], &[])]).needs_review());
        assert!(report(vec![renumbered(&["ccc"], &[])]).needs_review());
        assert!(report(vec![renumbered(&[], &["aaa.x1"])]).needs_review());
        assert!(report(vec![renumbered(&[], &[]), renumbered(&["ccc"], &[])]).needs_review());
    }

    #[test]
    fn test_check_store_healthy() {
        let tmp = doctor_store();
        store::save(tmp.path(), &make_epic("eee")).unwrap();
        let mut child = make_child("x1", "eee", Status::Open);
        child.depends_on = vec!["aaa".into()];
        store::save(tmp.path(), &child).unwrap();
        write_raw(
            tmp.path(),
            "aaa-task-aaa.md",
            &make_task("aaa", Status::Done),
            true,
        );

        let report = check_store(tmp.path()).unwrap();
        assert!(report.is_healthy(), "{:?}", report.findings);
        assert_eq!(report.files, 3);
    }

    #[test]
    fn test_check_store_not_initialized() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(matches!(
            check_store(tmp.path()),
            Err(Error::NotInitialized)
        ));
    }

    #[test]
    fn test_check_store_reports_each_kind() {
        let tmp = doctor_store();
        let base = tmp.path();
        // Graph problems.
        let mut a = make_task("aaa", Status::Open);
        a.depends_on = vec!["bbb".into(), "gone".into()];
        let mut b = make_task("bbb", Status::Open);
        b.depends_on = vec!["aaa".into()];
        store::save(base, &a).unwrap();
        store::save(base, &b).unwrap();
        // Hierarchy problems.
        store::save(base, &make_child("x1", "nop", Status::Open)).unwrap();
        store::save(base, &make_child("y2", "aaa", Status::Open)).unwrap();
        // File problems.
        write_raw(base, "ccc-one.md", &make_task("ccc", Status::Open), false);
        write_raw(base, "ccc-two.md", &make_task("ccc", Status::Done), true);
        write_raw(
            base,
            "zzz-misnamed.md",
            &make_task("ddd", Status::Open),
            false,
        );
        write_raw(
            base,
            "Bad_1-bad.md",
            &make_task("Bad_1", Status::Open),
            false,
        );
        std::fs::write(store::tasks_dir(base).join("broken.md"), "no frontmatter").unwrap();

        let report = check_store(base).unwrap();
        assert_eq!(
            kinds(&report),
            vec![
                "unreadable-file",
                "malformed-id",
                "duplicate-id",
                "filename-mismatch",
                "dangling-dependency",
                "missing-parent",
                "parent-not-epic",
                "dependency-cycle",
            ]
        );
        let f = &report.findings;
        assert_eq!(
            f[0],
            Finding::UnreadableFile {
                file: PathBuf::from(".bears/broken.md"),
                reason: "missing opening --- delimiter".into(),
            }
        );
        assert_eq!(
            f[2],
            Finding::DuplicateId {
                id: "ccc".into(),
                files: vec![
                    PathBuf::from(".bears/ccc-one.md"),
                    PathBuf::from(".bears/archive/ccc-two.md"),
                ],
                titles: vec!["Task ccc".into(), "Task ccc".into()],
                dependents: vec![],
                descendants: vec![],
            },
            "the active copy is listed first, as the one that keeps the ID"
        );
        assert_eq!(
            f[3],
            Finding::FilenameMismatch {
                id: "ddd".into(),
                file: PathBuf::from(".bears/zzz-misnamed.md"),
                expected: PathBuf::from(".bears/ddd-task-ddd.md"),
            }
        );
        assert_eq!(
            f[4],
            Finding::DanglingDependency {
                id: "aaa".into(),
                depends_on: "gone".into(),
            }
        );
        assert_eq!(
            f[5],
            Finding::MissingParent {
                id: "nop.x1".into(),
                parent: "nop".into(),
            }
        );
        assert_eq!(
            f[6],
            Finding::ParentNotEpic {
                id: "aaa.y2".into(),
                parent: "aaa".into(),
            }
        );
        assert_eq!(
            f[7],
            Finding::DependencyCycle {
                ids: vec!["aaa".into(), "bbb".into()],
            }
        );
        assert_eq!(f.iter().filter(|f| f.is_fixable()).count(), 2);
    }

    #[test]
    fn test_check_store_legacy_store_reports_only_that() {
        let tmp = doctor_store();
        let mut legacy = make_task("aaa", Status::Open);
        legacy.depends_on = vec!["gone".into()];
        let content = task::render_task(&legacy).replacen("---\n", "---\nparent: eee\n", 1);
        let path = store::tasks_dir(tmp.path()).join("aaa-task-aaa.md");
        std::fs::write(&path, &content).unwrap();

        let report = check_store(tmp.path()).unwrap();
        assert_eq!(
            report.findings,
            vec![Finding::LegacyStore {
                files: vec![PathBuf::from(".bears/aaa-task-aaa.md")],
            }]
        );
        // A repair touches nothing.
        let repaired = repair_store(tmp.path()).unwrap();
        assert_eq!(repaired.findings, report.findings);
        assert!(repaired.renamed.is_empty() && repaired.moved.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }

    #[tokio::test]
    async fn test_repair_store_renumbers_duplicates_and_keeps_references() {
        let tmp = doctor_store();
        let base = tmp.path();
        let mut first = make_epic("aaa");
        first.title = "First".into();
        let mut second = make_epic("aaa");
        second.status = Status::InProgress;
        second.title = "Second".into();
        second.body = "second body".into();
        write_raw(base, "aaa-first.md", &first, false);
        write_raw(base, "aaa-second.md", &second, false);
        let mut dependent = make_task("bbb", Status::Open);
        dependent.depends_on = vec!["aaa".into()];
        store::save(base, &dependent).unwrap();
        store::save(base, &make_child("x1", "aaa", Status::Open)).unwrap();

        // The check already names what cannot tell the copies apart.
        let check = check_store(base).unwrap();
        assert!(matches!(
            &check.findings[0],
            Finding::DuplicateId { titles, dependents, descendants, .. }
                if titles == &["First", "Second"]
                    && dependents == &["bbb"]
                    && descendants == &["aaa.x1"]
        ));
        assert!(check.findings[0].to_string().contains("below it: aaa.x1"));

        let report = repair_store(base).unwrap();
        assert!(report.moved.is_empty());
        assert_eq!(report.renamed.len(), 1);
        let r = &report.renamed[0];
        assert_eq!(r.from, "aaa");
        assert_eq!(task::id_depth(&r.to), 0, "a root duplicate stays a root");
        assert_eq!(r.file, PathBuf::from(format!(".bears/{}-second.md", r.to)));
        assert_eq!(r.title, "Second");
        assert_eq!(r.kept_file, PathBuf::from(".bears/aaa-first.md"));
        assert_eq!(r.kept_title, "First");
        assert_eq!(r.dependents, vec!["bbb"]);
        assert_eq!(r.descendants, vec!["aaa.x1"]);
        assert!(r.has_attached());
        assert!(report.is_healthy(), "{:?}", report.findings);

        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks.len(), 4);
        assert!(tasks.contains_key("aaa.x1"), "children are not moved");
        assert_eq!(tasks["aaa"].title, "First", "the first file keeps the ID");
        let moved = &tasks[&r.to];
        assert_eq!(moved.title, "Second");
        assert_eq!(moved.body, "second body");
        assert_eq!(moved.updated, second.updated, "updated is kept");
        assert_eq!(tasks["bbb"].depends_on, vec!["aaa"], "references stay put");
        assert!(!store::tasks_dir(base).join("aaa-second.md").exists());
    }

    /// `rewrite_task_file` rewrites in place before renaming, so a repair
    /// interrupted between the two leaves the renumbered task under its old
    /// filename. The next repair only moves that file: no second new ID.
    #[tokio::test]
    async fn test_repair_store_finishes_an_interrupted_renumbering() {
        let tmp = doctor_store();
        let base = tmp.path();
        let mut first = make_task("aaa", Status::Open);
        first.title = "First".into();
        write_raw(base, "aaa-first.md", &first, false);
        // The duplicate already rewritten as `bcd`, not yet renamed.
        let mut second = make_task("bcd", Status::Open);
        second.title = "Second".into();
        write_raw(base, "aaa-second.md", &second, false);

        let report = repair_store(base).unwrap();
        assert!(report.is_healthy(), "{:?}", report.findings);
        assert!(report.renamed.is_empty(), "no new ID: {:?}", report.renamed);
        assert_eq!(
            report.moved,
            vec![MovedFile {
                id: "bcd".into(),
                from: PathBuf::from(".bears/aaa-second.md"),
                to: PathBuf::from(".bears/bcd-second.md"),
            }]
        );
        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks["bcd"].title, "Second");
    }

    #[test]
    fn test_rewrite_task_file_leaves_one_file() {
        let tmp = doctor_store();
        let base = tmp.path();
        let mut t = make_task("aaa", Status::Open);
        write_raw(base, "zzz-old.md", &t, false);
        t.id = "bcd".into();
        let old = store::tasks_dir(base).join("zzz-old.md");
        let new = store::rewrite_task_file(&old, &t).unwrap();
        assert_eq!(new, store::tasks_dir(base).join("bcd-task-aaa.md"));
        let names: Vec<_> = std::fs::read_dir(store::tasks_dir(base))
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("bcd-task-aaa.md")]);
        // An occupied target is refused before anything is written.
        write_raw(base, "zzz-other.md", &make_task("ccc", Status::Open), false);
        let other = store::tasks_dir(base).join("zzz-other.md");
        let mut clash = t.clone();
        clash.body = "changed".into();
        assert!(matches!(
            store::rewrite_task_file(&other, &clash),
            Err(Error::IdTaken(_))
        ));
        assert!(
            task::parse_task(&std::fs::read_to_string(&other).unwrap())
                .unwrap()
                .id
                == "ccc"
        );
    }

    #[tokio::test]
    async fn test_repair_store_renumbers_a_child_under_its_parent() {
        let tmp = doctor_store();
        let base = tmp.path();
        store::save(base, &make_epic("eee")).unwrap();
        // The archived copy gives way to the active one, even though its
        // path sorts first.
        write_raw(
            base,
            "eee.x1-a.md",
            &make_child("x1", "eee", Status::Done),
            true,
        );
        write_raw(
            base,
            "eee.x1-b.md",
            &make_child("x1", "eee", Status::Open),
            false,
        );

        let report = repair_store(base).unwrap();
        assert!(report.is_healthy(), "{:?}", report.findings);
        let r = &report.renamed[0];
        assert_eq!(task::parent_of(&r.to), Some("eee"));
        assert!(r.file.starts_with(".bears/archive"));

        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks["eee.x1"].status, Status::Open);
        let archived = store::load_archived(base).await.unwrap();
        assert_eq!(archived[&r.to].status, Status::Done);
    }

    #[tokio::test]
    async fn test_repair_store_renames_misnamed_files() {
        let tmp = doctor_store();
        let base = tmp.path();
        write_raw(
            base,
            "zzz-misnamed.md",
            &make_task("ddd", Status::Open),
            false,
        );
        // A duplicate pair where only the later file is named after its ID:
        // that one keeps the ID, the other is renumbered.
        let mut misnamed = make_task("fff", Status::Open);
        misnamed.title = "Misnamed".into();
        write_raw(base, "aaa-misnamed.md", &misnamed, false);
        write_raw(
            base,
            "fff-task-fff.md",
            &make_task("fff", Status::Open),
            false,
        );

        let report = repair_store(base).unwrap();
        assert!(report.is_healthy(), "{:?}", report.findings);
        assert_eq!(
            report.moved,
            vec![MovedFile {
                id: "ddd".into(),
                from: PathBuf::from(".bears/zzz-misnamed.md"),
                to: PathBuf::from(".bears/ddd-task-ddd.md"),
            }]
        );
        assert_eq!(report.renamed.len(), 1);
        assert_eq!(report.renamed[0].from, "fff");

        let tasks = store::load_all(base).await.unwrap();
        assert_eq!(tasks["fff"].title, "Task fff");
        assert_eq!(tasks[&report.renamed[0].to].title, "Misnamed");
        assert!(store::find_task_path(base, "ddd").is_ok());
    }

    #[test]
    fn test_repair_store_leaves_report_only_findings() {
        let tmp = doctor_store();
        let base = tmp.path();
        let mut a = make_task("aaa", Status::Open);
        a.depends_on = vec!["gone".into()];
        store::save(base, &a).unwrap();
        store::save(base, &make_child("x1", "nop", Status::Open)).unwrap();
        // The target name is taken, so the misnamed file stays.
        write_raw(
            base,
            "zzz-misnamed.md",
            &make_task("ddd", Status::Open),
            false,
        );
        std::fs::write(store::tasks_dir(base).join("ddd-task-ddd.md"), "garbage").unwrap();

        let before = check_store(base).unwrap();
        let report = repair_store(base).unwrap();
        assert!(report.renamed.is_empty() && report.moved.is_empty());
        assert_eq!(report.findings, before.findings);
        assert_eq!(
            kinds(&report),
            vec![
                "unreadable-file",
                "filename-mismatch",
                "dangling-dependency",
                "missing-parent"
            ]
        );
    }
}
