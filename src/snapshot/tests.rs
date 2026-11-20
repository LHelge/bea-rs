use super::*;
use chrono::TimeZone;

/// A task created `minute` minutes after a fixed instant, so that orderings
/// by creation date are deterministic.
fn make(id: &str, status: Status, priority: Priority, minute: u32, deps: &[&str]) -> Task {
    let mut t = Task::new(id.to_string(), format!("Task {id}"), priority);
    t.status = status;
    t.created = Utc.with_ymd_and_hms(2026, 1, 1, 0, minute, 0).unwrap();
    t.updated = t.created;
    t.depends_on = deps.iter().map(|d| d.to_string()).collect();
    t
}

fn task(id: &str, status: Status, minute: u32, deps: &[&str]) -> Task {
    make(id, status, Priority::P2, minute, deps)
}

fn epic(id: &str, status: Status, minute: u32) -> Task {
    let mut t = make(id, status, Priority::P1, minute, &[]);
    t.task_type = TaskType::Epic;
    t
}

fn map(tasks: Vec<Task>) -> HashMap<String, Task> {
    tasks.into_iter().map(|t| (t.id.clone(), t)).collect()
}

fn ids(tasks: &[SnapshotTask]) -> Vec<&str> {
    tasks.iter().map(|t| t.id.as_str()).collect()
}

fn all() -> SnapshotOptions {
    SnapshotOptions {
        include_all: true,
        ..SnapshotOptions::default()
    }
}

fn node<'a>(s: &'a Snapshot, id: &str) -> &'a GraphNode {
    s.graph
        .nodes
        .iter()
        .find(|n| n.task.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn node_ids(s: &Snapshot) -> Vec<&str> {
    s.graph.nodes.iter().map(|n| n.task.id.as_str()).collect()
}

fn column(s: &Snapshot, status: Status) -> Option<&BoardColumn> {
    s.board.iter().find(|c| c.status == status)
}

#[test]
fn empty_store_has_every_count_and_no_detail() {
    let s = build(
        &HashMap::new(),
        &HashMap::new(),
        &SnapshotOptions::default(),
    );
    assert_eq!(
        s.totals,
        Totals {
            tasks: 0,
            epics: 0,
            archived: 0
        }
    );
    let statuses: Vec<Status> = s.by_status.iter().map(|c| c.status).collect();
    assert_eq!(statuses, STATUS_ORDER);
    assert!(s.by_status.iter().all(|c| c.count == 0));
    let priorities: Vec<Priority> = s
        .outstanding_by_priority
        .iter()
        .map(|c| c.priority)
        .collect();
    assert_eq!(priorities, PRIORITY_ORDER);
    assert!(s.epics.is_empty() && s.ready.is_empty() && s.archive.is_empty());
    assert_eq!(s.graph, SnapshotGraph::default());
    assert_eq!(s.generated_at, None);
}

#[test]
fn counts_cover_the_whole_active_store_without_epics() {
    let mut p0 = task("d", Status::Open, 3, &[]);
    p0.priority = Priority::P0;
    let active = map(vec![
        task("a", Status::Open, 0, &[]),
        task("b", Status::Done, 1, &[]),
        task("c", Status::InProgress, 2, &[]),
        p0,
        epic("e", Status::Open, 4),
    ]);
    let archived = map(vec![task("z", Status::Done, 5, &[])]);
    let s = build(&active, &archived, &SnapshotOptions::default());

    assert_eq!(
        s.totals,
        Totals {
            tasks: 4,
            epics: 1,
            archived: 1
        }
    );
    let count = |st: Status| s.by_status.iter().find(|c| c.status == st).unwrap().count;
    assert_eq!(count(Status::Open), 2);
    assert_eq!(count(Status::Done), 1);
    assert_eq!(count(Status::InProgress), 1);
    assert_eq!(count(Status::Review), 0);
    // Outstanding work only: the done P2 task is not counted.
    let by_p: Vec<usize> = s.outstanding_by_priority.iter().map(|c| c.count).collect();
    assert_eq!(by_p, vec![1, 0, 2, 0]);
    // Counts do not depend on the option.
    let s_all = build(&active, &archived, &all());
    assert_eq!(s_all.by_status, s.by_status);
    assert_eq!(s_all.outstanding_by_priority, s.outstanding_by_priority);
    assert_eq!(s_all.totals, s.totals);
}

#[test]
fn queues_agree_with_the_service_listings() {
    let mut started = task("s", Status::InProgress, 4, &[]);
    started.assignee = "worker-1".into();
    started.attempts = Some(2);
    let mut urgent = task("u", Status::Open, 9, &[]);
    urgent.priority = Priority::P0;
    let active = map(vec![
        task("a", Status::Open, 0, &[]),
        task("b", Status::Open, 1, &["a"]),
        task("c", Status::Done, 2, &[]),
        task("d", Status::Open, 3, &["c"]),
        started,
        task("r", Status::Review, 5, &[]),
        urgent,
        epic("e", Status::Open, 6),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());

    let expect = |v: Vec<Task>| v.into_iter().map(|t| t.id).collect::<Vec<_>>();
    assert_eq!(
        ids(&s.ready),
        expect(service::list_ready(&active, None, None, None))
    );
    assert_eq!(ids(&s.ready), vec!["u", "a", "d"]);
    assert_eq!(
        ids(&s.review),
        expect(service::list_review(&active, None, None, None))
    );
    assert_eq!(ids(&s.in_progress), vec!["s"]);
    assert_eq!(s.in_progress[0].assignee, "worker-1");
    assert_eq!(s.in_progress[0].attempts, 2);
    assert!(s.ready.iter().all(|t| t.ready));
    let b = s.board.iter().flat_map(|c| &c.tasks).find(|t| t.id == "b");
    assert!(!b.unwrap().ready);
}

#[test]
fn ties_are_broken_by_id_whatever_the_map_order() {
    // Same priority and creation date: only the ID orders them.
    let tasks: Vec<Task> = ["q", "c", "m", "a"]
        .iter()
        .map(|id| task(id, Status::Open, 0, &[]))
        .collect();
    let forward = map(tasks.clone());
    let backward = map(tasks.into_iter().rev().collect());
    let s = build(&forward, &HashMap::new(), &all());
    assert_eq!(ids(&s.ready), vec!["a", "c", "m", "q"]);
    assert_eq!(
        ids(&column(&s, Status::Open).unwrap().tasks),
        vec!["a", "c", "m", "q"]
    );
    for _ in 0..10 {
        let again = build(&backward, &HashMap::new(), &all());
        assert_eq!(again, s);
    }
}

#[test]
fn snapshot_is_deterministic_and_takes_its_time_from_the_caller() {
    let active = map(vec![
        epic("e", Status::Open, 0),
        task("e.a", Status::Open, 1, &[]),
        task("e.b", Status::Open, 1, &["e.a"]),
        task("e.c", Status::Open, 1, &["e.a"]),
        task("x", Status::Blocked, 2, &["e.b", "e.c"]),
    ]);
    let archived = map(vec![task("old", Status::Done, 3, &[])]);
    let a = build(&active, &archived, &all());
    let b = build(&active.clone(), &archived.clone(), &all());
    assert_eq!(a, b);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert!(!serde_json::to_string(&a).unwrap().contains("generated_at"));

    let at = Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap();
    let timed = build(
        &active,
        &archived,
        &SnapshotOptions {
            generated_at: Some(at),
            ..SnapshotOptions::default()
        },
    );
    assert_eq!(timed.generated_at, Some(at));
}

#[test]
fn board_leaves_settled_columns_out_unless_include_all() {
    let active = map(vec![
        task("a", Status::Open, 0, &[]),
        task("b", Status::Done, 1, &[]),
        task("c", Status::Cancelled, 2, &[]),
        epic("e", Status::Open, 3),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    let statuses: Vec<Status> = s.board.iter().map(|c| c.status).collect();
    assert_eq!(
        statuses,
        vec![
            Status::Proposed,
            Status::Open,
            Status::Blocked,
            Status::InProgress,
            Status::Review
        ]
    );
    // Epics are not on the board.
    assert_eq!(ids(&column(&s, Status::Open).unwrap().tasks), vec!["a"]);

    let s = build(&active, &HashMap::new(), &all());
    let statuses: Vec<Status> = s.board.iter().map(|c| c.status).collect();
    assert_eq!(statuses, STATUS_ORDER);
    assert_eq!(ids(&column(&s, Status::Done).unwrap().tasks), vec!["b"]);
    assert_eq!(
        ids(&column(&s, Status::Cancelled).unwrap().tasks),
        vec!["c"]
    );
}

#[test]
fn epics_are_listed_in_tree_order_with_progress() {
    let mut second = epic("f", Status::Open, 1);
    second.priority = Priority::P0;
    let active = map(vec![
        epic("e", Status::Open, 0),
        second,
        epic("e.n", Status::InProgress, 2),
        task("e.a", Status::Done, 3, &[]),
        task("e.b", Status::Open, 4, &[]),
        task("e.c", Status::Cancelled, 5, &[]),
        task("e.n.x", Status::Review, 6, &[]),
        epic("g", Status::Done, 7),
        task("g.a", Status::Done, 8, &[]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());

    let shape: Vec<(&str, Option<&str>, usize)> = s
        .epics
        .iter()
        .map(|e| (e.id.as_str(), e.parent.as_deref(), e.depth))
        .collect();
    assert_eq!(
        shape,
        vec![("f", None, 0), ("e", None, 0), ("e.n", Some("e"), 1)]
    );
    for e in &s.epics {
        assert_eq!(e.progress, service::epic_progress(&active, &e.id));
    }
    let e = &s.epics[1];
    // e.a done, e.b open, e.n in progress; e.c cancelled is not counted.
    assert_eq!(e.progress, EpicProgress { done: 1, total: 3 });
    let count = |st: Status| {
        e.children_by_status
            .iter()
            .find(|c| c.status == st)
            .unwrap()
            .count
    };
    assert_eq!(count(Status::Done), 1);
    assert_eq!(count(Status::Cancelled), 1);
    assert_eq!(count(Status::InProgress), 1);
    assert_eq!(e.children_by_status.len(), STATUS_ORDER.len());

    // Tasks name their innermost shown epic.
    let x = s.review.iter().find(|t| t.id == "e.n.x").unwrap();
    assert_eq!(x.epic.as_deref(), Some("e.n"));
    let b = s.ready.iter().find(|t| t.id == "e.b").unwrap();
    assert_eq!(b.epic.as_deref(), Some("e"));

    // The done epic only with include_all.
    let s = build(&active, &HashMap::new(), &all());
    let order: Vec<&str> = s.epics.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(order, vec!["f", "e", "e.n", "g"]);
    let ga = s
        .board
        .iter()
        .flat_map(|c| &c.tasks)
        .find(|t| t.id == "g.a");
    assert_eq!(ga.unwrap().epic.as_deref(), Some("g"));
}

#[test]
fn task_under_a_hidden_epic_climbs_to_the_next_shown_one() {
    let active = map(vec![
        epic("e", Status::Open, 0),
        epic("e.n", Status::Done, 1),
        task("e.n.x", Status::Open, 2, &[]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    assert_eq!(s.epics.len(), 1);
    assert_eq!(s.ready[0].epic.as_deref(), Some("e"));
}

#[test]
fn graph_layers_follow_the_longest_path() {
    // a ← b ← d, a ← c ← d, a ← d: d is two layers after a.
    let active = map(vec![
        task("a", Status::Open, 0, &[]),
        task("b", Status::Open, 1, &["a"]),
        task("c", Status::Open, 2, &["a"]),
        task("d", Status::Open, 3, &["a", "b", "c"]),
        task("lone", Status::Open, 4, &[]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    assert_eq!(node(&s, "a").layer, 0);
    assert_eq!(node(&s, "b").layer, 1);
    assert_eq!(node(&s, "c").layer, 1);
    assert_eq!(node(&s, "d").layer, 2);
    // A task without dependencies is a node too, in layer 0.
    assert_eq!(node(&s, "lone").layer, 0);
    assert_eq!(
        s.graph.layers,
        vec![vec!["a", "lone"], vec!["b", "c"], vec!["d"]]
    );
    let edges: Vec<(&str, &str)> = s
        .graph
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    assert_eq!(
        edges,
        vec![("a", "b"), ("a", "c"), ("a", "d"), ("b", "d"), ("c", "d")]
    );
    for e in &s.graph.edges {
        assert!(node(&s, &e.from).layer < node(&s, &e.to).layer);
        assert!(!e.cyclic);
    }
    // Nodes are sorted by layer and position, and agree with `layers`.
    for n in &s.graph.nodes {
        assert_eq!(s.graph.layers[n.layer][n.position], n.task.id);
    }
    assert_eq!(node_ids(&s), vec!["a", "lone", "b", "c", "d"]);
}

#[test]
fn graph_orders_a_layer_by_the_position_of_its_dependencies() {
    // Layer 0 is p, q (by creation); their dependents are created the other
    // way round but follow their dependencies' order.
    let active = map(vec![
        task("p", Status::Open, 0, &[]),
        task("q", Status::Open, 1, &[]),
        task("x", Status::Open, 2, &["q"]),
        task("y", Status::Open, 3, &["p"]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    assert_eq!(s.graph.layers, vec![vec!["p", "q"], vec!["y", "x"]]);
}

#[test]
fn graph_leaves_settled_dependencies_out_unless_include_all() {
    let active = map(vec![
        task("a", Status::Done, 0, &[]),
        task("b", Status::Open, 1, &["a", "gone"]),
    ]);
    let archived = map(vec![task("old", Status::Done, 2, &[])]);
    let s = build(&active, &archived, &SnapshotOptions::default());
    assert_eq!(node_ids(&s), vec!["b"]);
    assert!(s.graph.edges.is_empty());
    assert_eq!(node(&s, "b").layer, 0);
    // The dependency is still in the task's own list.
    assert_eq!(node(&s, "b").task.depends_on, vec!["a", "gone"]);

    let s = build(&active, &archived, &all());
    assert_eq!(node_ids(&s), vec!["a", "b"]);
    assert_eq!(node(&s, "b").layer, 1);
    assert_eq!(s.graph.edges.len(), 1);
}

#[test]
fn graph_puts_a_cycle_in_one_layer() {
    let active = map(vec![
        task("root", Status::Open, 0, &[]),
        task("a", Status::Open, 1, &["b", "root"]),
        task("b", Status::Open, 2, &["a"]),
        task("c", Status::Open, 3, &["a"]),
        task("self", Status::Open, 4, &["self"]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    assert_eq!(node(&s, "root").layer, 0);
    assert_eq!(node(&s, "a").layer, 1);
    assert_eq!(node(&s, "b").layer, 1);
    assert_eq!(node(&s, "c").layer, 2);
    assert!(node(&s, "a").cyclic && node(&s, "b").cyclic && node(&s, "self").cyclic);
    assert!(!node(&s, "root").cyclic && !node(&s, "c").cyclic);
    let cyclic: Vec<(&str, &str)> = s
        .graph
        .edges
        .iter()
        .filter(|e| e.cyclic)
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    assert_eq!(cyclic, vec![("a", "b"), ("b", "a"), ("self", "self")]);
}

#[test]
fn graph_has_an_epic_only_when_it_is_in_a_dependency() {
    let active = map(vec![
        epic("e", Status::Open, 0),
        task("e.a", Status::Open, 1, &[]),
        epic("f", Status::Open, 2),
        task("after", Status::Open, 3, &["f"]),
    ]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    // `e` is in no dependency, so only its task is a node; tasks outside any
    // epic come before an epic's group within a layer.
    assert_eq!(node_ids(&s), vec!["f", "e.a", "after"]);
    assert_eq!(node(&s, "f").task.task_type, TaskType::Epic);
    assert_eq!(node(&s, "e.a").task.epic.as_deref(), Some("e"));
}

#[test]
fn effective_priority_is_carried_when_more_urgent() {
    let mut urgent = task("u", Status::Open, 1, &["base"]);
    urgent.priority = Priority::P0;
    let active = map(vec![task("base", Status::Open, 0, &[]), urgent]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    assert_eq!(node(&s, "base").task.effective_priority, Some(Priority::P0));
    assert_eq!(node(&s, "u").task.effective_priority, None);
}

#[test]
fn archive_log_only_with_include_all_newest_first_and_limited() {
    let mut archived = Vec::new();
    for i in 0..5 {
        let mut t = task(&format!("z{i}"), Status::Done, i, &[]);
        t.updated = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, i).unwrap();
        archived.push(t);
    }
    let archived = map(archived);
    let s = build(&HashMap::new(), &archived, &SnapshotOptions::default());
    assert!(s.archive.is_empty());
    assert_eq!(s.totals.archived, 5);

    let s = build(
        &HashMap::new(),
        &archived,
        &SnapshotOptions {
            include_all: true,
            archive_limit: 3,
            generated_at: None,
        },
    );
    assert_eq!(ids(&s.archive), vec!["z4", "z3", "z2"]);
    let expect: Vec<String> = service::archive_log(&archived, Some(3))
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ids(&s.archive), expect);
}

#[test]
fn serialises_in_the_shape_of_task_files() {
    let mut t = task("a", Status::InProgress, 0, &[]);
    t.attempts = Some(1);
    let active = map(vec![t, epic("e", Status::Open, 1)]);
    let s = build(&active, &HashMap::new(), &SnapshotOptions::default());
    let json = serde_json::to_value(&s).unwrap();
    let a = &json["in_progress"][0];
    assert_eq!(a["status"], "in_progress");
    assert_eq!(a["priority"], "P2");
    assert!(a.get("type").is_none());
    assert!(a.get("assignee").is_none());
    assert_eq!(a["attempts"], 1);
    assert_eq!(json["epics"][0]["progress"]["total"], 0);
    let n = &json["graph"]["nodes"][0];
    assert_eq!(n["id"], "a");
    assert_eq!(n["layer"], 0);
}

#[tokio::test]
async fn load_reads_the_store_and_its_archive() {
    let tmp = tempfile::TempDir::new().unwrap();
    store::init(tmp.path()).unwrap();
    store::save(tmp.path(), &task("a", Status::Open, 0, &[])).unwrap();
    store::save(tmp.path(), &task("b", Status::Done, 1, &[])).unwrap();
    store::move_to_archive(tmp.path(), "b").unwrap();

    let s = load(tmp.path(), &all()).await.unwrap();
    assert_eq!(s.totals.tasks, 1);
    assert_eq!(s.totals.archived, 1);
    assert_eq!(ids(&s.ready), vec!["a"]);
    assert_eq!(ids(&s.archive), vec!["b"]);
}

#[tokio::test]
async fn load_refuses_an_uninitialised_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = load(tmp.path(), &SnapshotOptions::default()).await;
    assert!(matches!(err, Err(crate::Error::NotInitialized)));
}

#[tokio::test]
async fn snapshot_of_this_repository_is_consistent() {
    // On a copy, so that a tracker writing to the live store cannot change
    // it between the two loads. `.bears/` is not in the published crate.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(".bears");
    if !src.is_dir() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    store::init(tmp.path()).unwrap();
    for (from, to) in [
        (src.clone(), store::tasks_dir(tmp.path())),
        (src.join("archive"), store::archive_dir(tmp.path())),
    ] {
        let Ok(entries) = std::fs::read_dir(&from) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "md") {
                std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
            }
        }
    }
    let base = tmp.path();
    let s = load(base, &all()).await.unwrap();
    assert_eq!(s, load(base, &all()).await.unwrap());
    assert!(s.totals.tasks > 0);
    for e in &s.graph.edges {
        let (from, to) = (node(&s, &e.from), node(&s, &e.to));
        assert!(from.layer < to.layer || e.cyclic, "{e:?}");
    }
    for (l, layer) in s.graph.layers.iter().enumerate() {
        for (p, id) in layer.iter().enumerate() {
            let n = node(&s, id);
            assert_eq!((n.layer, n.position), (l, p));
        }
    }
    let on_board: usize = s.board.iter().map(|c| c.tasks.len()).sum();
    assert_eq!(on_board, s.totals.tasks);
}
