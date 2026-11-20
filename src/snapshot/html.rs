//! Render a [`Snapshot`] as one self-contained HTML page.
//!
//! [`render`] fills the page template ([`TEMPLATE`], embedded from
//! `templates/snapshot/snapshot.html`) with the snapshot's sections: header
//! stats, epic progress bars, the ready / review / in-progress queues, the
//! status board, the dependency graph as an inline SVG laid out from
//! [`SnapshotGraph::layers`], and the recent archive log.
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

use chrono::SecondsFormat;

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

/// Append formatted text to a `String` (infallible).
macro_rules! w {
    ($out:expr, $($arg:tt)*) => {{
        let _ = write!($out, $($arg)*);
    }};
}

/// Render `snapshot` as a complete HTML document.
pub fn render(snapshot: &Snapshot) -> String {
    let generated = match snapshot.generated_at {
        Some(at) => format!(
            "Generated {}",
            escape(&at.to_rfc3339_opts(SecondsFormat::Secs, true))
        ),
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

/// Top-left corner of the node at `layer`, `position`.
fn node_xy(layer: usize, position: usize) -> (i64, i64) {
    (
        PAD + layer as i64 * (NODE_W + LAYER_GAP),
        PAD + position as i64 * (NODE_H + ROW_GAP),
    )
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

    let at: HashMap<&str, (i64, i64)> = g
        .nodes
        .iter()
        .map(|n| (n.task.id.as_str(), node_xy(n.layer, n.position)))
        .collect();

    // Edges first, so that the nodes are drawn over them.
    for e in &g.edges {
        let (Some(&(fx, fy)), Some(&(tx, ty))) = (at.get(e.from.as_str()), at.get(e.to.as_str()))
        else {
            continue;
        };
        let mid = NODE_H / 2;
        let d = if e.cyclic {
            // Same layer: a loop out of the right side and back into it.
            let (x, y1, y2) = if e.from == e.to {
                (fx + NODE_W, fy + mid - 8, fy + mid + 8)
            } else {
                (fx + NODE_W, fy + mid, ty + mid)
            };
            let bulge = x + LOOP_ROOM - 4;
            format!("M{x},{y1} C{bulge},{y1} {bulge},{y2} {x},{y2}")
        } else {
            let (x1, y1, x2, y2) = (fx + NODE_W, fy + mid, tx, ty + mid);
            let c = LAYER_GAP / 2;
            format!(
                "M{x1},{y1} C{c1},{y1} {c2},{y2} {x2},{y2}",
                c1 = x1 + c,
                c2 = x2 - c
            )
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
        let (x, y) = at[t.id.as_str()];
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
            "<g class=\"{class}\"><title>{tip}</title><rect x=\"{x}\" y=\"{y}\" width=\"{NODE_W}\" height=\"{NODE_H}\" rx=\"6\"/><text class=\"nid\" x=\"{tx}\" y=\"{y1}\">{id} · {p} · {status}</text><text class=\"ntitle\" x=\"{tx}\" y=\"{y2}\">{title}</text></g>\n",
            tip = escape(&tip),
            tx = x + 8,
            y1 = y + 17,
            y2 = y + 34,
            id = escape(&t.id),
            status = status_label(t.status),
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
            w!(out, "<tr><td>{}</td><td>", t.updated.format("%Y-%m-%d"));
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
