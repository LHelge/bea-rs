//! Render a [`Snapshot`] as one self-contained HTML page.
//!
//! [`render`] fills the page template ([`TEMPLATE`], embedded from
//! `templates/snapshot/snapshot.html`) with the snapshot's sections: header
//! stats, epic progress bars, the ready / review / in-progress queues, the
//! status board, the dependency graph as an inline SVG laid out from
//! [`SnapshotGraph::layers`], and the recent archive log.
//!
//! The graph is drawn whole, however large: the SVG grows with the number of
//! layers and the size of the largest layer, and the panel holding it scrolls
//! in both directions within at most 85% of the window's height (CSS in the
//! template), so the sections after it stay a short scroll away. Unlike the
//! Markdown renderer there is no size limit to keep, and a part left out
//! would be a part nobody can see, so the page does not bound or split it.
//!
//! The page makes no external request of any kind: the CSS is inline, there
//! are no fonts, images or scripts, and it needs no JavaScript. Light and dark
//! themes follow `prefers-color-scheme`. Every piece of task content (IDs,
//! titles, tags, assignees) is HTML-escaped wherever it is written, in text,
//! in attributes and inside the SVG.
//!
//! Rendering is pure and deterministic: the same snapshot always gives the
//! same bytes, and nothing here reads the store or the clock.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::{BoardColumn, EpicEntry, Snapshot, SnapshotGraph, SnapshotTask};
use crate::task::{Status, TaskType};

/// The page template. `{{generated}}` and `{{sections}}` are replaced by
/// [`render`]; everything else is copied as is.
pub const TEMPLATE: &str = include_str!("../../templates/snapshot/snapshot.html");

/// Width of a graph node box, in SVG user units (pixels).
const NODE_W: i64 = 220;
/// Height of a graph node box.
const NODE_H: i64 = 44;
/// Horizontal gap between two layers, where the edges run.
const LAYER_GAP: i64 = 72;
/// Vertical gap between two nodes of a layer.
const ROW_GAP: i64 = 14;
/// Margin around the drawing.
const PAD: i64 = 12;
/// Room right of the last layer for the loops of cyclic edges.
const LOOP_ROOM: i64 = 40;
/// The most characters of a title shown in a graph node; the full title is
/// in the node's tooltip.
const NODE_TITLE_CHARS: usize = 28;
/// The most characters of a node's `id · priority · status` line. The line
/// is 11px monospace, whose characters are at most about 0.6em (6.6px) wide,
/// so 30 of them take under 200px of the 204px inside the box. A longer ID is
/// shortened from the front (see [`short_id`]); the full ID is in the tooltip.
const NODE_ID_LINE_CHARS: usize = 30;
const _: () = assert!(NODE_ID_LINE_CHARS as i64 * 66 / 10 <= NODE_W - 16);
/// How far above and below the middle of a node's right side the two ends of
/// a cyclic edge between two different nodes sit, so that `a → b` and
/// `b → a` start and end at different points.
const CYCLE_END_OFFSET: i64 = 5;

/// Append formatted text to a `String` (infallible).
macro_rules! w {
    ($out:expr, $($arg:tt)*) => {{
        let _ = write!($out, $($arg)*);
    }};
}

/// Render `snapshot` as a complete HTML document.
pub fn render(snapshot: &Snapshot) -> String {
    let generated = match snapshot.generated_at {
        // Worded and formatted as in the Markdown header.
        Some(at) => format!("Generated {}.", at.strftime("%Y-%m-%d %H:%M UTC")),
        None => String::from("A point-in-time picture of the task store."),
    };

    let mut sections = String::new();
    overview(&mut sections, snapshot);
    epics(&mut sections, &snapshot.epics);
    queues(&mut sections, snapshot);
    board(&mut sections, &snapshot.board);
    graph(&mut sections, &snapshot.graph);
    if snapshot.include_all {
        archive(&mut sections, &snapshot.archive);
    }
    footer(&mut sections, snapshot);

    fill(
        TEMPLATE,
        &[("generated", &generated), ("sections", &sections)],
    )
}

/// Escape `s` for HTML text and for quoted attribute values alike.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Replace each `{{key}}` of `template` by its value, in one pass, so that a
/// value is never searched for placeholders itself. Unknown keys are kept.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out =
        String::with_capacity(template.len() + values.iter().map(|v| v.1.len()).sum::<usize>());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let key = &after[..end];
                match values.iter().find(|(k, _)| *k == key) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push_str("{{");
                        out.push_str(key);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A status as people read it (`in_progress` → `in progress`).
fn status_label(s: Status) -> String {
    s.to_string().replace('_', " ")
}

/// `n` out of `total` as a whole percentage (0 when `total` is 0).
fn percent(n: usize, total: usize) -> usize {
    (n * 100).checked_div(total).unwrap_or(0)
}

/// `n` out of `total` as a percentage with one decimal (0 when `total` is
/// 0), for bar widths.
fn share(n: usize, total: usize) -> f64 {
    (n * 1000).checked_div(total).unwrap_or(0) as f64 / 10.0
}

fn priority_badge(t: &SnapshotTask) -> String {
    match t.effective_priority {
        Some(eff) => format!(
            r#"<span class="badge p-{eff}" title="{eff} inherited from dependent tasks; own priority {own}">{eff} ↑</span>"#,
            own = t.priority
        ),
        None => format!(r#"<span class="badge p-{p}">{p}</span>"#, p = t.priority),
    }
}

fn status_badge(s: Status) -> String {
    format!(r#"<span class="badge s-{s}">{}</span>"#, status_label(s))
}

fn tags(out: &mut String, tags: &[String]) {
    for tag in tags {
        w!(out, r#" <span class="tag">{}</span>"#, escape(tag));
    }
}

/// `ID P1 Title`, the one-line form of a task in lists.
fn task_line(out: &mut String, t: &SnapshotTask) {
    w!(
        out,
        r#"<span class="id">{}</span> {} {}"#,
        escape(&t.id),
        priority_badge(t),
        escape(&t.title)
    );
    if t.task_type == TaskType::Epic {
        out.push_str(r#" <span class="tag">epic</span>"#);
    }
}

fn overview(out: &mut String, s: &Snapshot) {
    out.push_str("<section id=\"overview\">\n<div class=\"stats\">\n");
    let stats = [
        (s.totals.tasks, "tasks"),
        (s.totals.epics, "epics"),
        (s.ready.len(), "ready"),
        (s.in_progress.len(), "in progress"),
        (s.review.len(), "in review"),
        (s.totals.archived, "archived"),
    ];
    for (n, label) in stats {
        w!(
            out,
            "<div class=\"panel stat\"><div class=\"n\">{n}</div><div class=\"l\">{label}</div></div>\n"
        );
    }
    out.push_str("</div>\n<div class=\"overview\">\n");

    // Tasks per status, as a stacked bar.
    let total: usize = s.by_status.iter().map(|c| c.count).sum();
    out.push_str("<div class=\"panel\">\n<h3>Tasks by status</h3>\n<div class=\"bar\">");
    for c in s.by_status.iter().filter(|c| c.count > 0) {
        w!(
            out,
            r#"<span class="s-{st}" style="width:{w}%" title="{label}: {n}"></span>"#,
            st = c.status,
            w = share(c.count, total),
            label = status_label(c.status),
            n = c.count
        );
    }
    out.push_str("</div>\n<div class=\"legend\">");
    for c in &s.by_status {
        w!(
            out,
            r#"<span><span class="dot s-{st}"></span>{label} {n}</span>"#,
            st = c.status,
            label = status_label(c.status),
            n = c.count
        );
    }
    out.push_str("</div>\n</div>\n");

    // Outstanding work per priority.
    let outstanding: usize = s.outstanding_by_priority.iter().map(|c| c.count).sum();
    out.push_str("<div class=\"panel\">\n<h3>Outstanding by priority</h3>\n<div class=\"bar\">");
    for c in s.outstanding_by_priority.iter().filter(|c| c.count > 0) {
        w!(
            out,
            r#"<span class="p-{p}" style="width:{w}%" title="{p}: {n}"></span>"#,
            p = c.priority,
            w = share(c.count, outstanding),
            n = c.count
        );
    }
    out.push_str("</div>\n<div class=\"legend\">");
    for c in &s.outstanding_by_priority {
        w!(
            out,
            r#"<span><span class="dot p-{p}"></span>{p} {n}</span>"#,
            p = c.priority,
            n = c.count
        );
    }
    out.push_str("</div>\n</div>\n</div>\n</section>\n");
}

fn epics(out: &mut String, epics: &[EpicEntry]) {
    out.push_str("<section id=\"epics\">\n<h2>Epics</h2>\n<div class=\"panel\">\n");
    if epics.is_empty() {
        out.push_str("<p class=\"empty\">No epics.</p>\n");
    } else {
        out.push_str("<ul class=\"epics\">\n");
        for e in epics {
            let (done, total) = (e.progress.done, e.progress.total);
            w!(
                out,
                "<li><div style=\"padding-left:{indent}rem\"><span class=\"id\">{id}</span> <span class=\"badge p-{p}\">{p}</span> {title} {status}",
                indent = e.depth as f64 * 1.25,
                id = escape(&e.id),
                p = e.priority,
                title = escape(&e.title),
                status = status_badge(e.status),
            );
            tags(out, &e.tags);
            out.push_str("</div><div class=\"bar\">");
            let children: usize = e.children_by_status.iter().map(|c| c.count).sum();
            for c in e.children_by_status.iter().filter(|c| c.count > 0) {
                w!(
                    out,
                    r#"<span class="s-{st}" style="width:{w}%" title="{label}: {n}"></span>"#,
                    st = c.status,
                    w = share(c.count, children),
                    label = status_label(c.status),
                    n = c.count
                );
            }
            w!(
                out,
                "</div><div class=\"count\" title=\"done / total, cancelled not counted\">{done}/{total} · {pc}%</div></li>\n",
                pc = percent(done, total)
            );
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</div>\n</section>\n");
}

fn queues(out: &mut String, s: &Snapshot) {
    out.push_str("<section id=\"queues\">\n<h2>Queues</h2>\n<div class=\"queues\">\n");
    let lists: [(&str, &[SnapshotTask], &str); 3] = [
        ("Ready", &s.ready, "Nothing is ready."),
        ("In progress", &s.in_progress, "Nothing is in progress."),
        ("In review", &s.review, "Nothing is waiting for review."),
    ];
    for (heading, tasks, none) in lists {
        w!(
            out,
            "<div class=\"panel\">\n<h3>{heading} <span class=\"muted\">{}</span></h3>\n",
            tasks.len()
        );
        if tasks.is_empty() {
            w!(out, "<p class=\"empty\">{none}</p>\n");
        } else {
            out.push_str("<ul>\n");
            for t in tasks {
                out.push_str("<li>");
                task_line(out, t);
                if !t.assignee.is_empty() || t.attempts > 0 {
                    out.push_str(" <span class=\"muted\">");
                    if !t.assignee.is_empty() {
                        w!(out, "@{}", escape(&t.assignee));
                    }
                    if t.attempts > 0 {
                        if !t.assignee.is_empty() {
                            out.push_str(", ");
                        }
                        w!(out, "attempt {}", t.attempts);
                    }
                    out.push_str("</span>");
                }
                out.push_str("</li>\n");
            }
            out.push_str("</ul>\n");
        }
        out.push_str("</div>\n");
    }
    out.push_str("</div>\n</section>\n");
}

fn board(out: &mut String, columns: &[BoardColumn]) {
    out.push_str("<section id=\"board\">\n<h2>Board</h2>\n<div class=\"board\">\n");
    for col in columns {
        w!(
            out,
            "<div class=\"panel column\">\n<h3><span><span class=\"dot s-{st}\"></span>{label}</span><span class=\"muted\">{n}</span></h3>\n",
            st = col.status,
            label = status_label(col.status),
            n = col.tasks.len()
        );
        if col.tasks.is_empty() {
            out.push_str("<p class=\"empty\">Empty.</p>\n");
        } else {
            out.push_str("<ul class=\"card-list\">\n");
            for t in &col.tasks {
                let p = t.effective_priority.unwrap_or(t.priority);
                w!(
                    out,
                    "<li class=\"card p-{p}\"><div><span class=\"id\">{id}</span> {badge}</div><div>{title}</div>",
                    id = escape(&t.id),
                    badge = priority_badge(t),
                    title = escape(&t.title),
                );
                let mut meta = Vec::new();
                if let Some(epic) = &t.epic {
                    meta.push(format!("in <span class=\"id\">{}</span>", escape(epic)));
                }
                if !t.assignee.is_empty() {
                    meta.push(format!("@{}", escape(&t.assignee)));
                }
                if t.attempts > 0 {
                    meta.push(format!("attempt {}", t.attempts));
                }
                if t.ready {
                    meta.push(String::from("ready"));
                }
                if !meta.is_empty() || !t.tags.is_empty() {
                    w!(out, "<div class=\"meta\">{}", meta.join(" · "));
                    tags(out, &t.tags);
                    out.push_str("</div>");
                }
                out.push_str("</li>\n");
            }
            out.push_str("</ul>\n");
        }
        out.push_str("</div>\n");
    }
    out.push_str("</div>\n</section>\n");
}

/// The first [`NODE_TITLE_CHARS`] characters of `title`, with an ellipsis
/// when it was cut. Counts characters, never splitting one.
fn truncate(title: &str) -> String {
    let mut chars = title.chars();
    let head: String = chars.by_ref().take(NODE_TITLE_CHARS).collect();
    if chars.next().is_some() {
        let mut cut: String = head.chars().take(NODE_TITLE_CHARS - 1).collect();
        cut.push('…');
        cut
    } else {
        head
    }
}

/// `id` within `room` characters: as it is when it fits, else its last whole
/// segments after `…` (`….d7.e4`, the leaf being what tells siblings apart),
/// else, when not even the leaf segment fits, its last characters after `…`.
fn short_id(id: &str, room: usize) -> String {
    if id.chars().count() <= room {
        return id.to_string();
    }
    let segments: Vec<&str> = id.split('.').collect();
    for k in 1..segments.len() {
        let tail = format!("….{}", segments[k..].join("."));
        if tail.chars().count() <= room {
            return tail;
        }
    }
    let keep = room.saturating_sub(1);
    let chars: Vec<char> = id.chars().collect();
    let mut out = String::from("…");
    out.extend(&chars[chars.len().saturating_sub(keep)..]);
    out
}

/// A node's first line, `id · priority · status`, within
/// [`NODE_ID_LINE_CHARS`] characters (unescaped).
fn id_line(id: &str, priority: crate::task::Priority, status: Status) -> String {
    let rest = format!(" · {priority} · {}", status_label(status));
    let room = NODE_ID_LINE_CHARS.saturating_sub(rest.chars().count());
    format!("{}{rest}", short_id(id, room))
}

/// Left edge of the nodes of `layer`.
fn layer_x(layer: usize) -> i64 {
    PAD + layer as i64 * (NODE_W + LAYER_GAP)
}

/// Top edge of the node at `position`.
fn row_y(position: usize) -> i64 {
    PAD + position as i64 * (NODE_H + ROW_GAP)
}

/// Top-left corner of the node at `layer`, `position`.
fn node_xy(layer: usize, position: usize) -> (i64, i64) {
    (layer_x(layer), row_y(position))
}

/// The middle of gap `gap` of a layer: 0 is above its first node, `g` the gap
/// between nodes `g - 1` and `g`.
fn gap_y(gap: usize) -> i64 {
    row_y(gap) - ROW_GAP / 2
}

/// Where an edge crosses a layer of `len` nodes that it does not stop at: the
/// gap between two nodes nearest to `target` (the height a straight line would
/// cross at), or `target` itself when that is below the layer's last node.
fn lane_y(len: usize, target: i64) -> i64 {
    if target >= gap_y(len) {
        return target;
    }
    (0..=len)
        .map(gap_y)
        .min_by_key(|y| ((y - target).abs(), *y))
        .unwrap_or(target)
}

/// A cubic curve from `(x1, y1)` (the current point) to `(x2, y2)`, leaving
/// and arriving horizontally; it stays between `x1` and `x2`.
fn curve(d: &mut String, (x1, y1): (i64, i64), (x2, y2): (i64, i64)) {
    let c = LAYER_GAP / 2;
    w!(d, " C{},{y1} {},{y2} {x2},{y2}", x1 + c, x2 - c);
}

/// The path of a dependency from the node at `from` to the node at `to`
/// (each `(layer, position)`), `to` in a later layer. An edge to the next
/// layer is one curve. An edge that skips layers runs through each layer in
/// between along a gap between its nodes ([`lane_y`]), horizontally across the
/// layer and in curves through the space between layers, so it never passes
/// behind a node box.
fn edge_path(from: (usize, usize), to: (usize, usize), layers: &[Vec<String>]) -> String {
    let mid = NODE_H / 2;
    let (x1, y1) = (layer_x(from.0) + NODE_W, row_y(from.1) + mid);
    let (x2, y2) = (layer_x(to.0), row_y(to.1) + mid);
    let mut d = format!("M{x1},{y1}");
    let mut at = (x1, y1);
    let span = (to.0 - from.0) as i64;
    for layer in from.0 + 1..to.0 {
        let step = (layer - from.0) as i64;
        let target = y1 + (y2 - y1) * step / span;
        let lane = lane_y(layers.get(layer).map_or(0, Vec::len), target);
        let left = layer_x(layer);
        curve(&mut d, at, (left, lane));
        w!(d, " L{},{lane}", left + NODE_W);
        at = (left + NODE_W, lane);
    }
    curve(&mut d, at, (x2, y2));
    d
}

/// The path of a cyclic edge, between two nodes of one layer or from a node
/// to itself: a loop out of the right side and back into it. Between two
/// different nodes it leaves above the middle of the dependency's side and
/// arrives below the middle of the dependent's, and a loop running upwards
/// bulges half as far as one running downwards, so two nodes that depend on
/// each other get two distinct curves.
fn cyclic_path(from: (i64, i64), to: (i64, i64), self_loop: bool) -> String {
    let mid = NODE_H / 2;
    let x = from.0 + NODE_W;
    let (y1, y2, reach) = if self_loop {
        (from.1 + mid - 8, from.1 + mid + 8, LOOP_ROOM - 4)
    } else {
        let reach = if from.1 < to.1 {
            LOOP_ROOM - 4
        } else {
            LOOP_ROOM / 2
        };
        (
            from.1 + mid - CYCLE_END_OFFSET,
            to.1 + mid + CYCLE_END_OFFSET,
            reach,
        )
    };
    let bulge = x + reach;
    format!("M{x},{y1} C{bulge},{y1} {bulge},{y2} {x},{y2}")
}

fn graph(out: &mut String, g: &SnapshotGraph) {
    out.push_str(
        "<section id=\"graph\">\n<h2>Dependency graph</h2>\n<div class=\"panel graph\">\n",
    );
    if g.nodes.is_empty() {
        out.push_str("<p class=\"empty\">No tasks to draw.</p>\n</div>\n</section>\n");
        return;
    }
    let layers = g.layers.len().max(1) as i64;
    let rows = g.layers.iter().map(Vec::len).max().unwrap_or(1).max(1) as i64;
    let has_loops = g.edges.iter().any(|e| e.cyclic);
    let width = 2 * PAD
        + layers * NODE_W
        + (layers - 1) * LAYER_GAP
        + if has_loops { LOOP_ROOM } else { 0 };
    let height = 2 * PAD + rows * NODE_H + (rows - 1) * ROW_GAP;

    w!(
        out,
        "<svg width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Dependency graph: {n} tasks, {e} dependencies\">\n",
        n = g.nodes.len(),
        e = g.edges.len()
    );
    out.push_str(concat!(
        "<defs>",
        "<marker id=\"arrow\" viewBox=\"0 0 8 8\" refX=\"8\" refY=\"4\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"M0,0 L8,4 L0,8 z\"/></marker>",
        "<marker id=\"arrow-cyclic\" class=\"cyclic\" viewBox=\"0 0 8 8\" refX=\"8\" refY=\"4\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"M0,0 L8,4 L0,8 z\"/></marker>",
        "</defs>\n"
    ));

    let place: HashMap<&str, (usize, usize)> = g
        .nodes
        .iter()
        .map(|n| (n.task.id.as_str(), (n.layer, n.position)))
        .collect();

    // Edges first, so that the nodes are drawn over them.
    for e in &g.edges {
        let (Some(&from), Some(&to)) = (place.get(e.from.as_str()), place.get(e.to.as_str()))
        else {
            continue;
        };
        let d = if e.cyclic || to.0 <= from.0 {
            // Same layer (an edge never points to an earlier one).
            cyclic_path(node_xy(from.0, from.1), node_xy(to.0, to.1), e.from == e.to)
        } else {
            edge_path(from, to, &g.layers)
        };
        let (class, marker) = if e.cyclic {
            ("edge cyclic", "arrow-cyclic")
        } else {
            ("edge", "arrow")
        };
        w!(
            out,
            "<path class=\"{class}\" d=\"{d}\" marker-end=\"url(#{marker})\"><title>{to} depends on {from}</title></path>\n",
            to = escape(&e.to),
            from = escape(&e.from)
        );
    }

    for n in &g.nodes {
        let t = &n.task;
        let (x, y) = node_xy(n.layer, n.position);
        let mut class = format!("node s-{}", t.status);
        if t.task_type == TaskType::Epic {
            class.push_str(" epic");
        }
        if n.cyclic {
            class.push_str(" cyclic");
        }
        if t.ready {
            class.push_str(" ready");
        }
        let p = t.effective_priority.unwrap_or(t.priority);
        let mut tip = format!("{}: {} ({}, {})", t.id, t.title, p, status_label(t.status));
        if !t.assignee.is_empty() {
            let _ = write!(tip, ", @{}", t.assignee);
        }
        if n.cyclic {
            tip.push_str(", in a dependency cycle");
        }
        w!(
            out,
            "<g class=\"{class}\"><title>{tip}</title><rect x=\"{x}\" y=\"{y}\" width=\"{NODE_W}\" height=\"{NODE_H}\" rx=\"6\"/><text class=\"nid\" x=\"{tx}\" y=\"{y1}\">{id_line}</text><text class=\"ntitle\" x=\"{tx}\" y=\"{y2}\">{title}</text></g>\n",
            tip = escape(&tip),
            tx = x + 8,
            y1 = y + 17,
            y2 = y + 34,
            id_line = escape(&id_line(&t.id, p, t.status)),
            title = escape(&truncate(&t.title)),
        );
    }
    out.push_str("</svg>\n</div>\n</section>\n");
}

fn archive(out: &mut String, tasks: &[SnapshotTask]) {
    out.push_str("<section id=\"archive\">\n<h2>Recently archived</h2>\n<div class=\"panel\">\n");
    if tasks.is_empty() {
        out.push_str("<p class=\"empty\">The archive is empty.</p>\n");
    } else {
        out.push_str(
            "<table>\n<thead><tr><th>Updated</th><th>Task</th><th>Status</th></tr></thead>\n<tbody>\n",
        );
        for t in tasks {
            w!(out, "<tr><td>{}</td><td>", t.updated.strftime("%Y-%m-%d"));
            task_line(out, t);
            tags(out, &t.tags);
            w!(out, "</td><td>{}</td></tr>\n", status_badge(t.status));
        }
        out.push_str("</tbody>\n</table>\n");
    }
    out.push_str("</div>\n</section>\n");
}

fn footer(out: &mut String, s: &Snapshot) {
    out.push_str("<footer>");
    if s.include_all {
        out.push_str("Settled work (done and cancelled) is included.");
    } else {
        out.push_str(
            "Done and cancelled work is counted above but left out of the epics, board and graph.",
        );
    }
    out.push_str(" Rendered by bears.</footer>\n");
}

#[cfg(test)]
mod tests;
