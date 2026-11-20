use super::*;
use crate::snapshot::{SnapshotOptions, build};
use crate::task::{Priority, Task};
use chrono::{TimeZone, Utc};
use std::collections::HashMap;

fn make(id: &str, status: Status, minute: u32, deps: &[&str]) -> Task {
    let mut t = Task::new(id.to_string(), format!("Task {id}"), Priority::P2);
    t.status = status;
    t.created = Utc.with_ymd_and_hms(2026, 1, 1, 0, minute, 0).unwrap();
    t.updated = t.created;
    t.depends_on = deps.iter().map(|d| d.to_string()).collect();
    t
}

fn epic(id: &str, minute: u32) -> Task {
    let mut t = make(id, Status::Open, minute, &[]);
    t.task_type = TaskType::Epic;
    t.title = format!("Epic {id}");
    t
}

fn map(tasks: Vec<Task>) -> HashMap<String, Task> {
    tasks.into_iter().map(|t| (t.id.clone(), t)).collect()
}

/// An epic with a done and an open child, a chain of dependencies, a task in
/// progress, one in review and a dependency cycle.
fn store() -> HashMap<String, Task> {
    let mut wip = make("w1", Status::InProgress, 6, &[]);
    wip.assignee = "alice".into();
    wip.attempts = Some(2);
    wip.tags = vec!["backend".into()];
    map(vec![
        epic("ep", 0),
        make("ep.a1", Status::Done, 1, &[]),
        make("ep.b2", Status::Open, 2, &["ep.a1"]),
        make("c3", Status::Open, 3, &["ep.b2"]),
        make("d4", Status::Open, 4, &["c3"]),
        make("r5", Status::Review, 5, &[]),
        wip,
        make("x1", Status::Open, 7, &["x2"]),
        make("x2", Status::Open, 8, &["x1"]),
    ])
}

fn archived() -> HashMap<String, Task> {
    map(vec![make("old", Status::Done, 9, &[])])
}

fn options(include_all: bool) -> SnapshotOptions {
    SnapshotOptions {
        include_all,
        generated_at: Some(Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap()),
        ..SnapshotOptions::default()
    }
}

fn snapshot(include_all: bool) -> Snapshot {
    build(&store(), &archived(), &options(include_all))
}

const HOSTILE: &str = r#"<script>alert("x")</script> & 'q' <img src=x onerror=1>"#;

/// The snapshot with every piece of task content replaced by hostile text.
fn hostile_snapshot() -> Snapshot {
    let mut s = snapshot(true);
    let fix = |t: &mut SnapshotTask| {
        t.title = HOSTILE.into();
        t.tags = vec![HOSTILE.into()];
        t.assignee = HOSTILE.into();
        t.epic = Some(HOSTILE.into());
    };
    s.ready.iter_mut().for_each(fix);
    s.review.iter_mut().for_each(fix);
    s.in_progress.iter_mut().for_each(fix);
    s.archive.iter_mut().for_each(fix);
    for col in &mut s.board {
        col.tasks.iter_mut().for_each(fix);
    }
    for e in &mut s.epics {
        e.title = HOSTILE.into();
        e.tags = vec![HOSTILE.into()];
        e.id = format!("{HOSTILE}{}", e.id);
    }
    // IDs in the graph, consistently in nodes, edges and layers.
    let rename = |id: &mut String| *id = format!("{HOSTILE}{id}");
    for n in &mut s.graph.nodes {
        fix(&mut n.task);
        rename(&mut n.task.id);
    }
    for e in &mut s.graph.edges {
        rename(&mut e.from);
        rename(&mut e.to);
    }
    s
}

#[test]
fn escape_covers_text_and_attribute_metacharacters() {
    assert_eq!(
        escape(r#"<a href="x">'&'</a>"#),
        "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
    );
    assert_eq!(escape("plain ünïcode"), "plain ünïcode");
}

#[test]
fn fill_is_one_pass_and_keeps_unknown_keys() {
    let out = fill(
        "a {{one}} b {{two}} {{nope}} {{",
        &[("one", "{{two}}"), ("two", "2")],
    );
    assert_eq!(out, "a {{two}} b 2 {{nope}} {{");
}

#[test]
fn template_has_its_placeholders_and_output_fills_them() {
    assert!(TEMPLATE.contains("{{generated}}") && TEMPLATE.contains("{{sections}}"));
    let html = render(&snapshot(true));
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(!html.contains("{{generated}}") && !html.contains("{{sections}}"));
    assert!(html.trim_end().ends_with("</html>"));
}

#[test]
fn render_is_deterministic() {
    let a = render(&snapshot(true));
    // A second build from freshly constructed maps renders the same bytes.
    let b = render(&build(&store(), &archived(), &options(true)));
    assert_eq!(a, b);
    assert_eq!(render(&snapshot(false)), render(&snapshot(false)));
}

#[test]
fn generated_time_is_shown_only_when_given() {
    let html = render(&snapshot(false));
    assert!(html.contains("Generated 2026-11-20T12:00:00Z"));
    let mut s = snapshot(false);
    s.generated_at = None;
    assert!(!render(&s).contains("Generated"));
}

#[test]
fn has_light_and_dark_themes_and_inline_css_only() {
    let html = render(&snapshot(false));
    assert!(html.contains("@media (prefers-color-scheme: dark)"));
    assert!(html.contains("<style>"));
    assert_eq!(html.matches("<style").count(), 1);
}

/// Nothing a browser would fetch: no URLs, no linked resources, no scripts.
fn assert_self_contained(html: &str) {
    let lower = html.to_ascii_lowercase();
    for needle in [
        "http://",
        "https://",
        "//cdn",
        "src=",
        "srcset",
        "href=",
        "@import",
        "@font-face",
        "<link",
        "<script",
        "<img",
        "<iframe",
        "<object",
        "<embed",
        "javascript:",
        " on", // no event handler attributes such as onclick= / onerror=
    ] {
        if needle == " on" {
            for (i, _) in lower.match_indices(" on") {
                let rest = &lower[i + 3..];
                let word: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                assert!(
                    !rest[word.len()..].starts_with('='),
                    "event handler attribute at {i}: {}",
                    &lower[i..(i + 40).min(lower.len())]
                );
            }
            continue;
        }
        assert!(!lower.contains(needle), "output contains {needle:?}");
    }
    // `url(` only for in-document references (SVG markers).
    for (i, _) in lower.match_indices("url(") {
        assert!(lower[i + 4..].starts_with('#'), "external url( at {i}");
    }
}

#[test]
fn output_makes_no_external_requests() {
    assert_self_contained(&render(&snapshot(true)));
    assert_self_contained(&render(&snapshot(false)));
    assert_self_contained(&render(&build(
        &HashMap::new(),
        &HashMap::new(),
        &SnapshotOptions::default(),
    )));
}

#[test]
fn hostile_task_content_is_escaped_everywhere() {
    let html = render(&hostile_snapshot());
    assert!(!html.contains(HOSTILE));
    assert!(!html.contains("<script"));
    assert!(!html.contains("<img"));
    assert!(!html.contains(r#""x""#));
    assert!(html.contains("&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &#39;q&#39;"));
    // The hostile text survives only as escaped text: no tag, and no
    // attribute value broken out of.
    assert!(!html.contains("\"x\""));
    // Inside the SVG too: tooltips and node text.
    let svg = &html[html.find("<svg").unwrap()..html.find("</svg>").unwrap()];
    assert!(svg.contains("<title>&lt;script&gt;"));
    assert!(svg.contains("class=\"ntitle\""));
    assert!(!svg.contains(HOSTILE));
}

#[test]
fn sections_are_present_and_archive_only_with_include_all() {
    let html = render(&snapshot(true));
    for id in ["overview", "epics", "queues", "board", "graph", "archive"] {
        assert!(html.contains(&format!("<section id=\"{id}\">")), "{id}");
    }
    assert!(html.contains("Task old"), "archive entry listed");

    let html = render(&snapshot(false));
    assert!(!html.contains("<section id=\"archive\">"));
    assert!(html.contains("left out of the epics, board and graph"));

    let s = build(&store(), &HashMap::new(), &options(true));
    let html = render(&s);
    assert!(html.contains("<section id=\"archive\">"));
    assert!(html.contains("The archive is empty."));
}

#[test]
fn header_shows_totals_and_counts() {
    let s = snapshot(false);
    let html = render(&s);
    let stat =
        |n: usize, label: &str| format!("<div class=\"n\">{n}</div><div class=\"l\">{label}</div>");
    assert!(html.contains(&stat(s.totals.tasks, "tasks")));
    assert!(html.contains(&stat(1, "epics")));
    assert!(html.contains(&stat(1, "archived")));
    assert!(html.contains(&stat(s.ready.len(), "ready")));
    assert!(html.contains(&stat(1, "in progress")));
    assert!(html.contains(&stat(1, "in review")));
    // Every status is in the legend, zeros included.
    for status in crate::snapshot::STATUS_ORDER {
        assert!(html.contains(&format!("<span class=\"dot s-{status}\"></span>")));
    }
    assert!(html.contains("proposed 0"));
}

#[test]
fn epic_progress_bar_and_count() {
    let html = render(&snapshot(false));
    // `ep` has one done and one open child.
    assert!(html.contains("Epic ep"));
    assert!(html.contains("1/2 · 50%"));
    assert!(html.contains(r#"<span class="s-done" style="width:50%""#));

    let empty = render(&build(
        &HashMap::new(),
        &HashMap::new(),
        &SnapshotOptions::default(),
    ));
    assert!(empty.contains("No epics."));
    assert!(empty.contains("No tasks to draw."));
    assert!(empty.contains("Nothing is ready."));
}

#[test]
fn queues_show_assignee_and_attempts() {
    let html = render(&snapshot(false));
    assert!(html.contains("@alice, attempt 2"));
    assert!(html.contains("<span class=\"tag\">backend</span>"));
}

#[test]
fn board_has_one_column_per_status() {
    let s = snapshot(false);
    let html = render(&s);
    let board = &html[html.find("<section id=\"board\">").unwrap()..];
    let board = &board[..board.find("</section>").unwrap()];
    assert_eq!(
        board.matches("class=\"panel column\"").count(),
        s.board.len()
    );
    let cards: usize = s.board.iter().map(|c| c.tasks.len()).sum();
    assert_eq!(board.matches("<li class=\"card").count(), cards);
    assert!(board.contains("in <span class=\"id\">ep</span>"));
}

#[test]
fn graph_draws_every_node_and_edge_from_the_layers() {
    let s = snapshot(false);
    let html = render(&s);
    let svg = &html[html.find("<svg").unwrap()..html.find("</svg>").unwrap()];
    assert_eq!(svg.matches("<g class=\"node").count(), s.graph.nodes.len());
    assert_eq!(
        svg.matches("<path class=\"edge").count(),
        s.graph.edges.len()
    );
    assert_eq!(
        svg.matches("<path class=\"edge cyclic\"").count(),
        s.graph.edges.iter().filter(|e| e.cyclic).count()
    );
    assert!(svg.contains("node s-open cyclic"));

    // Each node is placed at its layer and position.
    for n in &s.graph.nodes {
        let (x, y) = node_xy(n.layer, n.position);
        assert!(
            svg.contains(&format!("<rect x=\"{x}\" y=\"{y}\" ")),
            "node {} at {x},{y}",
            n.task.id
        );
    }
    // Non-cyclic edges run from the right side of the dependency to the left
    // side of the dependent, so left to right.
    let c3 = s.graph.nodes.iter().find(|n| n.task.id == "c3").unwrap();
    let d4 = s.graph.nodes.iter().find(|n| n.task.id == "d4").unwrap();
    assert!(d4.layer > c3.layer);
    let (fx, fy) = node_xy(c3.layer, c3.position);
    let (tx, ty) = node_xy(d4.layer, d4.position);
    let (x1, y1, x2, y2) = (fx + NODE_W, fy + NODE_H / 2, tx, ty + NODE_H / 2);
    let c = LAYER_GAP / 2;
    let d = format!("M{x1},{y1} C{},{y1} {},{y2} {x2},{y2}", x1 + c, x2 - c);
    assert!(svg.contains(&format!("d=\"{d}\" marker-end=\"url(#arrow)\"")));
    assert!(svg.contains("<title>d4 depends on c3</title>"));

    // The drawing is sized to hold every layer.
    let layers = s.graph.layers.len() as i64;
    let width = 2 * PAD + layers * NODE_W + (layers - 1) * LAYER_GAP + LOOP_ROOM;
    assert!(svg.contains(&format!("<svg width=\"{width}\" ")));
}

#[test]
fn epics_in_the_graph_are_marked() {
    let mut tasks = store();
    // A task depending on the epic makes the epic a node.
    tasks.insert("e9".into(), make("e9", Status::Open, 10, &["ep"]));
    let s = build(&tasks, &HashMap::new(), &options(false));
    let html = render(&s);
    assert!(html.contains("<g class=\"node s-open epic\">"));
}

#[test]
fn long_titles_are_truncated_in_nodes_by_character() {
    let long = "é".repeat(NODE_TITLE_CHARS + 5);
    let cut = truncate(&long);
    assert_eq!(cut.chars().count(), NODE_TITLE_CHARS);
    assert!(cut.ends_with('…'));
    let exact = "a".repeat(NODE_TITLE_CHARS);
    assert_eq!(truncate(&exact), exact);

    let mut tasks = store();
    tasks.get_mut("d4").unwrap().title = long.clone();
    let html = render(&build(&tasks, &HashMap::new(), &options(false)));
    // The full title stays in the tooltip.
    assert!(html.contains(&format!("d4: {long}")));
    assert!(html.contains(&format!(">{cut}</text>")));
}
