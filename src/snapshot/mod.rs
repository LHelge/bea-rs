//! A point-in-time picture of a bears store, for rendering into a static
//! artifact (`bea snapshot`).
//!
//! [`build`] turns the active and archived task maps into a [`Snapshot`]:
//! counts per status and priority, epic progress, the ready, review and
//! in-progress queues, a status board, the dependency graph with a layered
//! layout, and the recent archive log. It is pure, serialisable data; the
//! renderers in the submodules of this module turn it into Markdown or HTML
//! without going back to the store.
//!
//! The queues and progress come from the same functions as `bea ready`,
//! `bea review`, `bea epics` and `bea log` ([`service::list_ready`],
//! [`service::list_review`], [`service::epic_progress`],
//! [`service::archive_log`]), so a snapshot agrees with them.
//!
//! A snapshot is **deterministic**: the same store and options always give an
//! equal value. Every list has a fixed order (ties are broken by ID), and the
//! only time in it is [`SnapshotOptions::generated_at`], which the caller
//! passes in.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use jiff::Timestamp;
use serde::Serialize;

use crate::error::Result;
use crate::graph::{self, Graph};
use crate::service::{self, EpicProgress};
use crate::store;
use crate::task::{self, Priority, Status, Task, TaskType};

pub mod html;
pub mod markdown;

/// How many archive entries a snapshot lists by default.
pub const DEFAULT_ARCHIVE_LIMIT: usize = 10;

/// The statuses in workflow order: the order of [`Snapshot::by_status`], of
/// [`EpicEntry::children_by_status`] and of the [`Snapshot::board`] columns.
pub const STATUS_ORDER: [Status; 7] = [
    Status::Proposed,
    Status::Open,
    Status::Blocked,
    Status::InProgress,
    Status::Review,
    Status::Done,
    Status::Cancelled,
];

/// The priorities, most urgent first: the order of
/// [`Snapshot::outstanding_by_priority`].
pub const PRIORITY_ORDER: [Priority; 4] = [Priority::P0, Priority::P1, Priority::P2, Priority::P3];

/// What a snapshot includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotOptions {
    /// Include settled work in the detail: done and cancelled tasks on the
    /// board and in the graph, done and cancelled epics, and the archive log.
    /// Without it those are left out, and only the counts mention them.
    pub include_all: bool,
    /// The most archive entries to list (only with `include_all`).
    pub archive_limit: usize,
    /// When the snapshot was taken, copied into [`Snapshot::generated_at`].
    /// The snapshot never reads the clock itself, so that it stays
    /// deterministic; `None` leaves the time out.
    pub generated_at: Option<Timestamp>,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        SnapshotOptions {
            include_all: false,
            archive_limit: DEFAULT_ARCHIVE_LIMIT,
            generated_at: None,
        }
    }
}

/// A point-in-time picture of a store. See the [module docs](self).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// [`SnapshotOptions::generated_at`], when given.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::task::timestamp::option::serialize"
    )]
    pub generated_at: Option<Timestamp>,
    /// [`SnapshotOptions::include_all`]: whether settled work is in the detail.
    pub include_all: bool,
    /// How many tasks, epics and archived tasks the store holds.
    pub totals: Totals,
    /// Active (not archived) tasks per status, epics not counted, one entry
    /// per status in [`STATUS_ORDER`], zeros included. Independent of
    /// `include_all`.
    pub by_status: Vec<StatusCount>,
    /// Outstanding (not done or cancelled) active tasks per priority, epics
    /// not counted, one entry per priority in [`PRIORITY_ORDER`], zeros
    /// included. Independent of `include_all`.
    pub outstanding_by_priority: Vec<PriorityCount>,
    /// Epics in tree order: each epic followed by the epics nested in it, and
    /// siblings by priority, creation date and ID. Done and cancelled epics
    /// only with `include_all`.
    pub epics: Vec<EpicEntry>,
    /// The ready queue, exactly as `bea ready` lists it.
    pub ready: Vec<SnapshotTask>,
    /// The review queue, exactly as `bea review` lists it.
    pub review: Vec<SnapshotTask>,
    /// Tasks in progress, with their assignee and attempts, as
    /// `bea list --status in_progress` lists them.
    pub in_progress: Vec<SnapshotTask>,
    /// One column per status in [`STATUS_ORDER`], empty ones included, with
    /// the active tasks (not epics) in that status; the `done` and
    /// `cancelled` columns only with `include_all`.
    pub board: Vec<BoardColumn>,
    /// The dependency graph of the tasks on the board, laid out in layers.
    pub graph: SnapshotGraph,
    /// The most recently archived tasks, newest first, as `bea log` lists
    /// them; empty without `include_all`.
    pub archive: Vec<SnapshotTask>,
}

/// Store-wide counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// Active tasks that are not epics, in any status.
    pub tasks: usize,
    /// Active epics, in any status.
    pub epics: usize,
    /// Tasks and epics in the archive.
    pub archived: usize,
}

/// How many tasks have a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StatusCount {
    pub status: Status,
    pub count: usize,
}

/// How many tasks have a priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PriorityCount {
    pub priority: Priority,
    pub count: usize,
}

/// One task as a snapshot shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotTask {
    pub id: String,
    pub title: String,
    /// Left out of the serialised form for a plain task, as in task files.
    #[serde(rename = "type", skip_serializing_if = "is_plain_task")]
    pub task_type: TaskType,
    pub status: Status,
    pub priority: Priority,
    /// The priority inherited from dependent tasks, only when it is more
    /// urgent than [`priority`](Self::priority) (as in `TaskSummary`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_priority: Option<Priority>,
    pub tags: Vec<String>,
    /// Empty when unassigned.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    /// Attempts started on the task; 0 when it was never started.
    pub attempts: u32,
    /// The innermost epic of [`Snapshot::epics`] the task lies under, if any:
    /// the subgraph or group to draw it in. `None` for a root task, and for a
    /// task whose epics are not in the snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic: Option<String>,
    /// The IDs the task depends on, as written in its file.
    pub depends_on: Vec<String>,
    /// Whether the task is ready to work on (`graph::is_task_ready`).
    pub ready: bool,
    #[serde(serialize_with = "crate::task::timestamp::serialize")]
    pub created: Timestamp,
    #[serde(serialize_with = "crate::task::timestamp::serialize")]
    pub updated: Timestamp,
}

/// One epic with its progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EpicEntry {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    /// The innermost enclosing epic in [`Snapshot::epics`], if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// How many epics of [`Snapshot::epics`] enclose this one (0 at the top).
    pub depth: usize,
    /// Done / total direct children, as `bea epics` shows it.
    pub progress: EpicProgress,
    /// Direct children (active, epics included) per status, one entry per
    /// status in [`STATUS_ORDER`], zeros included.
    pub children_by_status: Vec<StatusCount>,
}

/// One column of the status board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoardColumn {
    pub status: Status,
    /// By priority, creation date and ID.
    pub tasks: Vec<SnapshotTask>,
}

/// The dependency graph, laid out in layers for drawing.
///
/// Layering is longest-path over the DAG: a node with no dependency in the
/// graph is in layer 0, every other node one layer after its deepest
/// dependency, so every edge points to a later layer. The tasks of a
/// dependency cycle cannot be ordered; they share one layer and are marked
/// [`cyclic`](GraphNode::cyclic), as are the edges between them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotGraph {
    /// Sorted by layer, then position.
    pub nodes: Vec<GraphNode>,
    /// Sorted by `from`, then `to`.
    pub edges: Vec<GraphEdge>,
    /// The node IDs of each layer, in position order: `layers[l][p]` is the
    /// node with `layer == l` and `position == p`.
    pub layers: Vec<Vec<String>>,
}

/// One node of [`SnapshotGraph`].
///
/// The nodes are the tasks on the board, plus every epic in the snapshot that
/// takes part in a dependency between them. Dependencies on tasks outside the
/// graph (settled ones without `include_all`, archived or missing ones) are
/// left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphNode {
    #[serde(flatten)]
    pub task: SnapshotTask,
    /// The layer, counted from 0.
    pub layer: usize,
    /// The place within the layer, counted from 0. Nodes are ordered by the
    /// mean position of their dependencies (fewer crossing edges), then
    /// grouped by epic, then by priority, creation date and ID.
    pub position: usize,
    /// Whether the node is part of a dependency cycle.
    pub cyclic: bool,
}

/// A dependency: `from` has to be done before `to` (`to` depends on `from`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    /// Whether both ends are in the same dependency cycle.
    pub cyclic: bool,
}

/// Load the store at `base`, active tasks and archive, and take a snapshot.
pub async fn load(base: &Path, options: &SnapshotOptions) -> Result<Snapshot> {
    let active = store::load_all(base).await?;
    let archived = store::load_archived(base).await?;
    Ok(build(&active, &archived, options))
}

/// Take a snapshot of already loaded tasks: `active` from
/// [`store::load_all`], `archived` from [`store::load_archived`].
pub fn build(
    active: &HashMap<String, Task>,
    archived: &HashMap<String, Task>,
    options: &SnapshotOptions,
) -> Snapshot {
    let eff = service::effective_priorities(active);
    let epics = epic_entries(active, options.include_all);
    // Borrowed from `active`'s keys, so that `epics` can move into the result.
    let epic_ids: HashSet<&str> = epics
        .iter()
        .filter_map(|e| active.get_key_value(&e.id))
        .map(|(id, _)| id.as_str())
        .collect();
    let entry = |t: &Task| snapshot_task(t, active, &eff, &epic_ids);

    let work: Vec<&Task> = active.values().filter(|t| t.task_type.is_task()).collect();

    let ready = service::list_ready(active, None, None, None);
    let review = service::list_review(active, None, None, None);
    let in_progress = service::list_tasks(
        active,
        Some(Status::InProgress),
        None,
        None,
        false,
        false,
        None,
    );

    let board: Vec<BoardColumn> = STATUS_ORDER
        .iter()
        .filter(|s| options.include_all || !is_settled(**s))
        .map(|&status| {
            let mut tasks: Vec<&Task> = work
                .iter()
                .copied()
                .filter(|t| t.status == status)
                .collect();
            tasks.sort_by(|a, b| task::canonical_order(a, b));
            BoardColumn {
                status,
                tasks: tasks.into_iter().map(entry).collect(),
            }
        })
        .collect();

    let on_board: Vec<&Task> = work
        .iter()
        .copied()
        .filter(|t| options.include_all || task::is_active(t))
        .collect();
    let graph = layered_graph(active, &on_board, &epic_ids, &entry);

    let archive = if options.include_all {
        service::archive_log(archived, Some(options.archive_limit))
            .iter()
            .map(|t| snapshot_task(t, archived, &HashMap::new(), &epic_ids))
            .collect()
    } else {
        Vec::new()
    };

    Snapshot {
        generated_at: options.generated_at,
        include_all: options.include_all,
        totals: Totals {
            tasks: work.len(),
            epics: active.len() - work.len(),
            archived: archived.len(),
        },
        by_status: count_statuses(work.iter().copied()),
        outstanding_by_priority: PRIORITY_ORDER
            .iter()
            .map(|&priority| PriorityCount {
                priority,
                count: work
                    .iter()
                    .filter(|t| t.priority == priority && task::is_active(t))
                    .count(),
            })
            .collect(),
        epics,
        ready: ready.iter().map(entry).collect(),
        review: review.iter().map(entry).collect(),
        in_progress: in_progress.iter().map(entry).collect(),
        board,
        graph,
        archive,
    }
}

fn is_plain_task(t: &TaskType) -> bool {
    t.is_task()
}

fn is_settled(status: Status) -> bool {
    matches!(status, Status::Done | Status::Cancelled)
}

fn count_statuses<'a>(tasks: impl Iterator<Item = &'a Task> + Clone) -> Vec<StatusCount> {
    STATUS_ORDER
        .iter()
        .map(|&status| StatusCount {
            status,
            count: tasks.clone().filter(|t| t.status == status).count(),
        })
        .collect()
}

/// The innermost strict ancestor of `id` that is in `epics`.
fn enclosing_epic(id: &str, epics: &HashSet<&str>) -> Option<String> {
    task::ancestors(id)
        .find(|a| epics.contains(a))
        .map(String::from)
}

fn snapshot_task(
    t: &Task,
    tasks: &HashMap<String, Task>,
    eff: &HashMap<String, Priority>,
    epics: &HashSet<&str>,
) -> SnapshotTask {
    SnapshotTask {
        id: t.id.clone(),
        title: t.title.clone(),
        task_type: t.task_type,
        status: t.status,
        priority: t.priority,
        effective_priority: eff.get(&t.id).copied().filter(|p| *p < t.priority),
        tags: t.tags.clone(),
        assignee: t.assignee.clone(),
        attempts: t.attempt_count(),
        epic: enclosing_epic(&t.id, epics),
        depends_on: t.depends_on.clone(),
        ready: graph::is_task_ready(tasks, t),
        created: t.created,
        updated: t.updated,
    }
}

/// The epics to show, in tree order (see [`Snapshot::epics`]).
fn epic_entries(active: &HashMap<String, Task>, include_all: bool) -> Vec<EpicEntry> {
    let mut epics: Vec<&Task> = active
        .values()
        .filter(|t| t.task_type.is_epic() && (include_all || task::is_active(t)))
        .collect();
    epics.sort_by(|a, b| task::canonical_order(a, b));
    let ids: HashSet<&str> = epics.iter().map(|t| t.id.as_str()).collect();

    // Children of each shown epic, in sibling order; `None` holds the top.
    let mut children: BTreeMap<Option<String>, Vec<&Task>> = BTreeMap::new();
    for &e in &epics {
        children
            .entry(enclosing_epic(&e.id, &ids))
            .or_default()
            .push(e);
    }

    let mut out = Vec::with_capacity(epics.len());
    // Depth-first, pre-order; the stack holds (epic, depth) in reverse.
    let mut stack: Vec<(&Task, usize)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|&e| (e, 0))
        .collect();
    while let Some((e, depth)) = stack.pop() {
        let kids: Vec<&Task> = active.values().filter(|t| t.is_child_of(&e.id)).collect();
        out.push(EpicEntry {
            id: e.id.clone(),
            title: e.title.clone(),
            status: e.status,
            priority: e.priority,
            tags: e.tags.clone(),
            parent: enclosing_epic(&e.id, &ids),
            depth,
            progress: service::epic_progress(active, &e.id),
            children_by_status: count_statuses(kids.iter().copied()),
        });
        if let Some(nested) = children.get(&Some(e.id.clone())) {
            stack.extend(nested.iter().rev().map(|&n| (n, depth + 1)));
        }
    }
    out
}

/// Lay out the dependency graph of `on_board` plus the shown epics that take
/// part in a dependency between them (see [`SnapshotGraph`]).
fn layered_graph(
    active: &HashMap<String, Task>,
    on_board: &[&Task],
    epic_ids: &HashSet<&str>,
    entry: &dyn Fn(&Task) -> SnapshotTask,
) -> SnapshotGraph {
    // Candidate nodes: the board's tasks and the shown epics.
    let candidates: HashSet<&str> = on_board
        .iter()
        .map(|t| t.id.as_str())
        .chain(epic_ids.iter().copied())
        .collect();
    let deps_in = |t: &Task, set: &HashSet<&str>| -> BTreeSet<String> {
        t.depends_on
            .iter()
            .filter(|d| set.contains(d.as_str()))
            .cloned()
            .collect()
    };
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for &id in &candidates {
        for dep in deps_in(&active[id], &candidates) {
            edges.insert((dep, id.to_string()));
        }
    }
    // An epic is a node only when it is at one end of an edge.
    let in_edge: HashSet<&str> = edges
        .iter()
        .flat_map(|(a, b)| [a.as_str(), b.as_str()])
        .collect();
    let nodes: HashSet<&str> = candidates
        .iter()
        .copied()
        .filter(|id| active[*id].task_type.is_task() || in_edge.contains(id))
        .collect();

    // Dependency cycles: tasks of one cycle are condensed into one unit.
    let sub: HashMap<String, Task> = nodes
        .iter()
        .map(|&id| {
            let mut t = active[id].clone();
            t.depends_on = deps_in(&active[id], &nodes).into_iter().collect();
            t.body.clear();
            (id.to_string(), t)
        })
        .collect();
    let cycles = Graph::build(&sub).cycles();
    let mut unit: HashMap<&str, usize> = HashMap::new();
    for (i, cycle) in cycles.iter().enumerate() {
        for id in cycle {
            unit.insert(id.as_str(), i);
        }
    }
    let mut sorted_nodes: Vec<&str> = nodes.iter().copied().collect();
    sorted_nodes.sort_unstable();
    for &id in &sorted_nodes {
        let next = cycles.len() + unit.len();
        unit.entry(id).or_insert(next);
    }
    let cyclic = |id: &str| unit[id] < cycles.len();

    // Longest-path layering over the condensation (Kahn's algorithm).
    let mut unit_deps: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut unit_dependents: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for &id in &sorted_nodes {
        unit_deps.entry(unit[id]).or_default();
    }
    for (from, to) in &edges {
        let (uf, ut) = (unit[from.as_str()], unit[to.as_str()]);
        if uf != ut {
            unit_deps.entry(ut).or_default().insert(uf);
            unit_dependents.entry(uf).or_default().insert(ut);
        }
    }
    let mut pending: BTreeMap<usize, usize> =
        unit_deps.iter().map(|(&u, d)| (u, d.len())).collect();
    let mut layer_of_unit: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue: Vec<usize> = pending
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(&u, _)| u)
        .collect();
    while let Some(u) = queue.pop() {
        let layer = unit_deps[&u]
            .iter()
            .map(|d| layer_of_unit[d] + 1)
            .max()
            .unwrap_or(0);
        layer_of_unit.insert(u, layer);
        for &next in unit_dependents.get(&u).into_iter().flatten() {
            let n = pending.get_mut(&next).expect("every unit has a count");
            *n -= 1;
            if *n == 0 {
                queue.push(next);
            }
        }
    }

    // Order each layer: mean position of the dependencies in earlier layers,
    // then epic, priority, creation date and ID.
    let layer_count = layer_of_unit.values().max().map_or(0, |m| m + 1);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); layer_count];
    for &id in &sorted_nodes {
        layers[layer_of_unit[&unit[id]]].push(id.to_string());
    }
    let mut position: HashMap<String, usize> = HashMap::new();
    let mut layer_of: HashMap<String, usize> = HashMap::new();
    for (l, ids) in layers.iter_mut().enumerate() {
        let barycenter = |id: &str| -> Option<f64> {
            let placed: Vec<usize> = sub[id]
                .depends_on
                .iter()
                .filter(|d| layer_of.get(d.as_str()).is_some_and(|dl| *dl < l))
                .map(|d| position[d.as_str()])
                .collect();
            (!placed.is_empty()).then(|| placed.iter().sum::<usize>() as f64 / placed.len() as f64)
        };
        let mut keyed: Vec<(Option<f64>, Option<String>, &Task)> = ids
            .iter()
            .map(|id| {
                (
                    barycenter(id),
                    enclosing_epic(id, epic_ids),
                    &active[id.as_str()],
                )
            })
            .collect();
        keyed.sort_by(|a, b| {
            let bary = match (a.0, b.0) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                (None, None) => std::cmp::Ordering::Equal,
                // Nodes without placed dependencies go last.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
            };
            bary.then_with(|| a.1.cmp(&b.1))
                .then_with(|| task::canonical_order(a.2, b.2))
        });
        *ids = keyed.iter().map(|(_, _, t)| t.id.clone()).collect();
        for (p, id) in ids.iter().enumerate() {
            position.insert(id.clone(), p);
            layer_of.insert(id.clone(), l);
        }
    }

    let nodes = layers
        .iter()
        .enumerate()
        .flat_map(|(l, ids)| ids.iter().enumerate().map(move |(p, id)| (l, p, id)))
        .map(|(layer, position, id)| GraphNode {
            task: entry(&active[id.as_str()]),
            layer,
            position,
            cyclic: cyclic(id),
        })
        .collect();
    let edges = edges
        .into_iter()
        .map(|(from, to)| {
            let cyclic = unit[from.as_str()] == unit[to.as_str()];
            GraphEdge { from, to, cyclic }
        })
        .collect();
    SnapshotGraph {
        nodes,
        edges,
        layers,
    }
}

#[cfg(test)]
mod tests;
