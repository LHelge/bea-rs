//! Render a [`Snapshot`] as GitHub-flavored Markdown with a Mermaid
//! dependency graph, for GitHub job summaries and PR comments.
//!
//! [`render`] (or [`render_with`] for other bounds) is a pure function: it
//! never loads the store or reads the clock, and the same snapshot always
//! gives the same text.
//!
//! The document has summary tables (totals, statuses, outstanding work by
//! priority), the epics with their progress, the ready, in-progress and review
//! queues, the status board in collapsible `<details>` sections, a
//! ```` ```mermaid ```` flowchart of the dependency graph, and the archive log
//! when settled work is included.
//!
//! **The document is bounded.** A PR comment holds at most 65 536 characters,
//! so with the default [`MarkdownOptions`] the whole document stays within
//! [`DEFAULT_MAX_DOCUMENT_CHARS`] bytes, however large the store. The header
//! and the summary counts are always complete. The graph may use at most half
//! of what is left, and the other sections share the rest fairly: in document
//! order, each gets all it needs when that is no more than an equal share of
//! what is still left for it and the sections after it, and an equal share
//! otherwise, so one long table cannot crowd out the others (the board's
//! columns share its room the same way). A table that does not fit ends after
//! its last whole row with "…and N more not shown", a section with no room at
//! all is left out, a graph part with no room left becomes a note, and a note
//! under the summary then says that the document was cut. Nothing is
//! ever cut inside a table row, a `<details>` block or a Mermaid fence.
//! Titles are cut to [`MAX_CELL_CHARS`] in tables and headings.
//!
//! **The graph is bounded.** GitHub renders Mermaid with a limit on the
//! diagram's text and edges, and a diagram of hundreds of tasks is unreadable
//! anyway. When the whole graph is larger than [`MarkdownOptions`] allows (in
//! tasks, dependencies or text), it is drawn as one diagram per top-level epic
//! (plus one for the tasks in no epic), leaving out the dependencies between
//! them, and a note says which bound it exceeded. An epic still over the node
//! or edge bounds is drawn per nested epic in its place (plus one for its own
//! tasks), and so on down. A part that is still too large, or that no longer
//! fits the text budget, is replaced by a note giving its size.
//!
//! **Titles are untrusted.** Every title, tag, assignee and ID is escaped for
//! its context: in tables, Markdown and HTML syntax is neutralised (pipes,
//! backticks, emphasis, links, `$` math, `<tags>`), newlines become spaces,
//! and `@` cannot start a mention; in Mermaid labels everything that is not a
//! letter, digit or harmless punctuation becomes an entity code (`#35;`).
//! Mermaid node and subgraph IDs are generated (`n0`, `e0`), never taken from
//! task IDs, which contain dots.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use super::{BoardColumn, EpicEntry, GraphEdge, GraphNode, STATUS_ORDER, Snapshot, SnapshotTask};
use crate::task::{self, Status};

/// The most tasks drawn in one Mermaid diagram by default.
pub const DEFAULT_MAX_GRAPH_NODES: usize = 100;

/// The most dependencies drawn in one Mermaid diagram by default (Mermaid's
/// own limit is 500).
pub const DEFAULT_MAX_GRAPH_EDGES: usize = 200;

/// The default budget, in bytes, for the text of all Mermaid diagrams of one
/// document together; Mermaid refuses a diagram over 50 000 characters. The
/// graph never takes more than half of the document budget left after the
/// summary, whatever this says.
pub const DEFAULT_MAX_GRAPH_CHARS: usize = 30_000;

/// The default budget, in bytes, for the whole document: a margin under the
/// 65 536 characters of a GitHub PR comment (a text never has more characters
/// than bytes). A job summary takes 1 MiB, so a caller writing only one can
/// raise it.
pub const DEFAULT_MAX_DOCUMENT_CHARS: usize = 60_000;

/// The most characters of a title shown in a graph node; longer titles are
/// cut and end in `…`.
pub const MAX_LABEL_CHARS: usize = 40;

/// The most characters of a title shown in a table cell or a heading; longer
/// titles are cut and end in `…`.
pub const MAX_CELL_CHARS: usize = 200;

/// Size bounds of a Markdown snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// The most nodes in one diagram.
    pub max_graph_nodes: usize,
    /// The most edges in one diagram.
    pub max_graph_edges: usize,
    /// The most bytes of Mermaid text in the whole document.
    pub max_graph_chars: usize,
    /// The most bytes of the whole document. The header and summary are
    /// always written in full (well under 1 KB), even past a smaller budget.
    pub max_document_chars: usize,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        MarkdownOptions {
            max_graph_nodes: DEFAULT_MAX_GRAPH_NODES,
            max_graph_edges: DEFAULT_MAX_GRAPH_EDGES,
            max_graph_chars: DEFAULT_MAX_GRAPH_CHARS,
            max_document_chars: DEFAULT_MAX_DOCUMENT_CHARS,
        }
    }
}

/// The note under the summary when anything was left out for the budget.
const CUT_NOTE: &str = "> **Shortened.** Some lists below were cut or left out to keep this \
    document within its size limit; the counts above are complete.\n\n";

/// Render `snapshot` with the default [`MarkdownOptions`].
pub fn render(snapshot: &Snapshot) -> String {
    render_with(snapshot, &MarkdownOptions::default())
}

/// Render `snapshot` as GitHub-flavored Markdown. See the
/// [module docs](self).
pub fn render_with(snapshot: &Snapshot, options: &MarkdownOptions) -> String {
    let s = snapshot;
    let mut head = String::new();
    head.push_str("# Bears snapshot\n\n");
    if let Some(at) = s.generated_at {
        let _ = writeln!(head, "Generated {}.", at.strftime("%Y-%m-%d %H:%M UTC"));
    }
    if s.include_all {
        head.push_str("Includes done and cancelled work.\n");
    } else {
        head.push_str("Open work only; done and cancelled work is counted but not listed.\n");
    }
    head.push('\n');
    summary(&mut head, s);

    // What is left once the summary and the cut note are written.
    let mut avail = options
        .max_document_chars
        .saturating_sub(head.len() + CUT_NOTE.len());
    let mut cut = false;
    let mut take = |text: (String, bool), avail: &mut usize| {
        *avail -= text.0.len();
        cut |= text.1;
        text.0
    };

    let graph = take(
        section("Dependency graph", avail / 2, |l| graph(s, options, l)),
        &mut avail,
    );

    // The other sections, in document order; the graph goes after the board.
    let queue_cols = [
        Col::Id,
        Col::Title,
        Col::Priority,
        Col::Assignee,
        Col::Attempts,
    ];
    let ready_cols = [Col::Id, Col::Title, Col::Priority, Col::Epic, Col::Tags];
    let archive_cols = [Col::Id, Col::Title, Col::Status, Col::Updated];
    type Render<'r> = Box<dyn Fn(usize) -> Body + 'r>;
    let mut sections: Vec<(&str, Render)> = vec![("Epics", Box::new(|l| epics(s, l)))];
    for (title, tasks, cols) in [
        ("Ready", &s.ready, &ready_cols),
        ("In progress", &s.in_progress, &queue_cols),
        ("Review", &s.review, &queue_cols),
    ] {
        sections.push((title, Box::new(move |l| task_table(tasks, cols, l))));
    }
    const BOARD: usize = 4;
    sections.push(("Board", Box::new(|l| board(s, l))));
    debug_assert_eq!(sections[BOARD].0, "Board");
    if s.include_all {
        sections.push((
            "Recently archived",
            Box::new(|l| task_table(&s.archive, &archive_cols, l)),
        ));
    }

    // Each section gets a fair share of what is left (see `fair_share`), so
    // that a long table cannot crowd out the ones after it; what a section
    // leaves of its share goes to the sections after it.
    let wanted: Vec<usize> = sections
        .iter()
        .map(|(title, render)| section(title, usize::MAX, render).0.len())
        .collect();
    let mut texts = Vec::new();
    for (i, (title, render)) in sections.iter().enumerate() {
        let share = fair_share(&wanted[i..], avail);
        texts.push(take(section(title, share, render), &mut avail));
    }
    let after_board = texts.split_off(BOARD + 1);
    let body = texts.concat() + &graph + &after_board.concat();

    let mut out = head;
    if cut {
        out.push_str(CUT_NOTE);
    }
    out.push_str(&body);
    // Exactly one newline at the end.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

/// A body within `limit` bytes, and whether anything was left out of it;
/// `None` when not even a note fits.
type Body = Option<(String, bool)>;

/// `## title` and its body within `limit` bytes; nothing (and cut) when the
/// body does not fit.
fn section(title: &str, limit: usize, body: impl FnOnce(usize) -> Body) -> (String, bool) {
    let head = format!("## {title}\n\n");
    if head.len() > limit {
        return (String::new(), true);
    }
    match body(limit - head.len()) {
        Some((text, cut)) => (head + &text, cut),
        None => (String::new(), true),
    }
}

/// The share of `avail` bytes for the first of sections that want `wanted`
/// bytes each, when `avail` is divided max-min fairly: a section wanting no
/// more than an equal share gets all it wants, and the others split what is
/// left equally. The shares never add up to more than `avail`.
fn fair_share(wanted: &[usize], avail: usize) -> usize {
    let Some(&first) = wanted.first() else {
        return 0;
    };
    let mut sorted = wanted.to_vec();
    sorted.sort_unstable();
    let mut left = avail;
    for (i, &w) in sorted.iter().enumerate() {
        let equal = left / (sorted.len() - i);
        if w > equal {
            // Every section from here on wants more than an equal share.
            return first.min(equal);
        }
        left -= w;
    }
    first
}

/// `text` when it fits in `limit` bytes.
fn fits(text: String, limit: usize) -> Body {
    (text.len() <= limit).then_some((text, false))
}

fn summary(out: &mut String, s: &Snapshot) {
    out.push_str("## Summary\n\n");
    let _ = writeln!(
        out,
        "{} tasks, {} epics, {} archived.\n",
        s.totals.tasks, s.totals.epics, s.totals.archived
    );
    out.push_str("| Status | Tasks |\n| --- | ---: |\n");
    for c in &s.by_status {
        let _ = writeln!(out, "| {} | {} |", c.status, c.count);
    }
    out.push('\n');
    out.push_str("| Priority | Outstanding |\n| --- | ---: |\n");
    for c in &s.outstanding_by_priority {
        let _ = writeln!(out, "| {} | {} |", c.priority, c.count);
    }
    out.push('\n');
}

fn epics(s: &Snapshot, limit: usize) -> Body {
    let header = "| Epic | Title | Status | Priority | Progress |\n\
                  | --- | --- | --- | --- | ---: |\n";
    let rows: Vec<String> = s
        .epics
        .iter()
        .map(|e| {
            let p = &e.progress;
            let progress = if p.total == 0 {
                "0/0".to_string()
            } else {
                format!("{}/{} ({}%)", p.done, p.total, p.done * 100 / p.total)
            };
            format!(
                "| {} | {}{} | {} | {} | {} |\n",
                code(&e.id),
                "&emsp;".repeat(e.depth),
                title_cell(&e.title),
                e.status,
                e.priority,
                progress
            )
        })
        .collect();
    table(header, &rows, limit)
}

fn board(s: &Snapshot, limit: usize) -> Body {
    let shown: Vec<_> = s.board.iter().filter(|c| !c.tasks.is_empty()).collect();
    if shown.is_empty() {
        return fits("_No tasks._\n\n".into(), limit);
    }
    let cols = [
        Col::Id,
        Col::Title,
        Col::Priority,
        Col::Epic,
        Col::DependsOn,
    ];
    let open = |column: &BoardColumn| {
        format!(
            "<details><summary>{} ({})</summary>\n\n",
            column.status,
            column.tasks.len()
        )
    };
    let close = "</details>\n\n";
    // Each column whole; all of them when they fit, so that the board fits
    // in exactly its own size.
    let blocks: Vec<String> = shown
        .iter()
        .map(|column| {
            let table =
                task_table(&column.tasks, &cols, usize::MAX).map_or_else(String::new, |t| t.0);
            open(column) + &table + close
        })
        .collect();
    if blocks.iter().map(String::len).sum::<usize>() <= limit {
        return Some((blocks.concat(), false));
    }
    let wanted: Vec<usize> = blocks.iter().map(String::len).collect();
    let more = |k: usize| more_note(k, shown.len(), "column");
    let reserve = more_reserve(shown.len(), "column");
    if reserve > limit {
        return None;
    }
    let mut out = String::new();
    let mut cut = false;
    for (i, column) in shown.iter().enumerate() {
        let open = open(column);
        // A fair share of the room, as between sections, so that a long
        // column cannot crowd out the ones after it.
        let room = fair_share(&wanted[i..], limit - out.len() - reserve);
        let table = room
            .checked_sub(open.len() + close.len())
            .and_then(|inner| task_table(&column.tasks, &cols, inner))
            // A column without a single row is not worth its block.
            .filter(|(t, _)| t.starts_with('|'));
        match table {
            Some((t, c)) => {
                out += &open;
                out += &t;
                out += close;
                cut |= c;
            }
            None => {
                out += &more(shown.len() - i);
                return Some((out, true));
            }
        }
    }
    Some((out, cut))
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Col {
    Id,
    Title,
    Status,
    Priority,
    Epic,
    Tags,
    Assignee,
    Attempts,
    DependsOn,
    Updated,
}

impl Col {
    fn header(self) -> &'static str {
        match self {
            Col::Id => "ID",
            Col::Title => "Title",
            Col::Status => "Status",
            Col::Priority => "Priority",
            Col::Epic => "Epic",
            Col::Tags => "Tags",
            Col::Assignee => "Assignee",
            Col::Attempts => "Attempts",
            Col::DependsOn => "Depends on",
            Col::Updated => "Updated",
        }
    }

    fn value(self, t: &SnapshotTask) -> String {
        match self {
            Col::Id => code(&t.id),
            Col::Title => title_cell(&t.title),
            Col::Status => t.status.to_string(),
            Col::Priority => match t.effective_priority {
                Some(eff) => format!("{} (effective {eff})", t.priority),
                None => t.priority.to_string(),
            },
            Col::Epic => t.epic.as_deref().map(code).unwrap_or_default(),
            Col::Tags => t
                .tags
                .iter()
                .map(|g| cell(g))
                .collect::<Vec<_>>()
                .join(", "),
            Col::Assignee => cell(&t.assignee),
            Col::Attempts => t.attempts.to_string(),
            Col::DependsOn => t
                .depends_on
                .iter()
                .map(|d| code(d))
                .collect::<Vec<_>>()
                .join(", "),
            Col::Updated => t.updated.strftime("%Y-%m-%d").to_string(),
        }
    }
}

fn task_table(tasks: &[SnapshotTask], cols: &[Col], limit: usize) -> Body {
    let row = |cells: Vec<String>| format!("| {} |\n", cells.join(" | "));
    let header = row(cols.iter().map(|c| c.header().to_string()).collect())
        + &row(cols.iter().map(|_| "---".to_string()).collect());
    let rows: Vec<String> = tasks
        .iter()
        .map(|t| row(cols.iter().map(|c| c.value(t)).collect()))
        .collect();
    table(&header, &rows, limit)
}

/// A table of `rows` (whole lines) under `header` within `limit` bytes: as
/// many whole rows as fit, then "…and N more not shown". `_None._` for no
/// rows.
fn table(header: &str, rows: &[String], limit: usize) -> Body {
    if rows.is_empty() {
        return fits("_None._\n\n".into(), limit);
    }
    let full = header.len() + rows.iter().map(String::len).sum::<usize>() + 1;
    if full <= limit {
        return Some((format!("{header}{}\n", rows.concat()), false));
    }
    let more = |k: usize| format!("_…and {k} more not shown._\n\n");
    let reserve = more(rows.len()).len();
    let mut out = header.to_string();
    let mut shown = 0;
    for r in rows {
        if out.len() + r.len() + 1 + reserve > limit {
            break;
        }
        out += r;
        shown += 1;
    }
    if shown == 0 {
        let note = format!(
            "_{} {} not shown._\n\n",
            rows.len(),
            plural(rows.len(), "row")
        );
        return (note.len() <= limit).then_some((note, true));
    }
    out.push('\n');
    out += &more(rows.len() - shown);
    Some((out, true))
}

/// The note that `left` of `total` items were not shown: "…and N more" when
/// some were.
fn more_note(left: usize, total: usize, noun: &str) -> String {
    let nouns = plural(left, noun);
    if left == total {
        format!("_{left} {nouns} not shown._\n\n")
    } else {
        format!("_…and {left} more {nouns} not shown._\n\n")
    }
}

/// Room for any [`more_note`] about up to `total` items: the longer
/// "…and N more" form with the largest number.
fn more_reserve(total: usize, noun: &str) -> usize {
    more_note(total, total + 1, noun).len()
}

/// `noun` with an `s` unless there is one.
fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

/// A title for a table cell or heading: cut to [`MAX_CELL_CHARS`], escaped.
fn title_cell(title: &str) -> String {
    cell(&cut(title, MAX_CELL_CHARS))
}

/// Untrusted text made safe for a GFM table cell, and for Markdown in
/// general: one line, no table, HTML, emphasis, link, code, math or mention
/// syntax.
fn cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in one_line(text).chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            // A word joiner after `@` keeps GitHub from reading a mention.
            '@' => out.push_str("@\u{2060}"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '~' | '|' | '$' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// An ID as a code span when it is a well-formed ID (which has no character
/// a code span or table could trip over); a malformed one, as only hand
/// edits make, is escaped like any other text.
fn code(id: &str) -> String {
    if task::is_valid_id(id) {
        format!("`{id}`")
    } else {
        cell(id)
    }
}

/// Control characters and line breaks as spaces, runs of whitespace as one
/// space, trimmed.
fn one_line(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text` on one line and at most `max` characters, cut with `…`.
fn cut(text: &str, max: usize) -> String {
    let line = one_line(text);
    if line.chars().count() <= max {
        return line;
    }
    let mut kept: String = line.chars().take(max.saturating_sub(1)).collect();
    kept.truncate(kept.trim_end().len());
    kept.push('…');
    kept
}

// ---------------------------------------------------------------------------
// Mermaid graph
// ---------------------------------------------------------------------------

/// Why a diagram was not drawn.
enum TooLarge {
    /// More nodes or edges than [`MarkdownOptions`] allows.
    Bounds,
    /// Its text, `len` bytes, is more than the `budget` left for it.
    Text { len: usize, budget: usize },
}

fn graph(s: &Snapshot, options: &MarkdownOptions, limit: usize) -> Body {
    let g = &s.graph;
    if g.nodes.is_empty() {
        return fits("_No tasks to draw._\n\n".into(), limit);
    }
    let mut out = String::from("Arrows point from a dependency to the task that waits for it.\n\n");
    if out.len() > limit {
        return None;
    }
    let ctx = Diagram::new(s);
    let all_nodes: Vec<&GraphNode> = g.nodes.iter().collect();
    let all_edges: Vec<&GraphEdge> = g.edges.iter().collect();
    let mut budget = options.max_graph_chars;

    let note = cycle_note(&all_nodes);
    let room = (limit - out.len()).saturating_sub(note.len());
    let reason = match ctx.diagram(&all_nodes, &all_edges, options, budget.min(room)) {
        Ok(text) => {
            out += &text;
            out += &note;
            return Some((out, false));
        }
        Err(reason) => reason,
    };

    // Too large for one diagram: one per top-level epic, then the rest, and
    // an epic still over the bounds per nested epic in turn.
    let mut parts = Vec::new();
    let mut nested = false;
    ctx.split(
        None,
        all_nodes.clone(),
        &all_edges,
        options,
        &mut parts,
        &mut nested,
    );
    let part_of: HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.nodes.iter().map(move |n| (n.task.id.as_str(), i)))
        .collect();
    let crossing = all_edges
        .iter()
        .filter(|e| part_of[e.from.as_str()] != part_of[e.to.as_str()])
        .count();
    let mut intro = match reason {
        TooLarge::Bounds => format!(
            "The graph has {} tasks and {} dependencies, more than one diagram shows \
             (at most {} tasks and {} dependencies), so it is drawn per top-level epic.\n",
            all_nodes.len(),
            all_edges.len(),
            options.max_graph_nodes,
            options.max_graph_edges
        ),
        TooLarge::Text { len, budget } => format!(
            "One diagram of the graph's {} tasks would take {len} bytes, more than the \
             {budget} this document has room for, so it is drawn per top-level epic.\n",
            all_nodes.len()
        ),
    };
    if nested {
        intro.push_str("An epic still too large for one diagram is drawn per nested epic.\n");
    }
    if crossing > 0 {
        let _ = writeln!(
            intro,
            "{crossing} dependencies between tasks of different parts are not drawn."
        );
    }
    intro.push('\n');

    let more = |k: usize| more_note(k, parts.len(), "part");
    let reserve = more_reserve(parts.len(), "part");
    if out.len() + intro.len() + reserve > limit {
        return None;
    }
    out += &intro;

    let mut cut_off = false;
    for (i, p) in parts.iter().enumerate() {
        let heading = match p.key {
            PartKey::Epic(e) => {
                let e = &s.epics[e];
                format!("### {} {}\n\n", code(&e.id), title_cell(&e.title))
            }
            PartKey::Directly(e) => {
                let e = &s.epics[e];
                format!(
                    "### {} {}, not in a nested epic\n\n",
                    code(&e.id),
                    title_cell(&e.title)
                )
            }
            PartKey::NoEpic => "### Not in an epic\n\n".to_string(),
        };
        let edges = inner_edges(&p.nodes, &all_edges);
        let note = cycle_note(&p.nodes);
        let room = limit - out.len() - reserve;
        let space = room.saturating_sub(heading.len() + note.len());
        let part = match ctx.diagram(&p.nodes, &edges, options, budget.min(space)) {
            Ok(text) => {
                budget -= text.len();
                format!("{heading}{text}{note}")
            }
            Err(reason) => {
                let why = match reason {
                    TooLarge::Bounds => "too large to draw here",
                    TooLarge::Text { len, .. } => {
                        // Left out for the document's size limit, not the
                        // graph's own text bound: say so under the summary,
                        // as for a cut table.
                        cut_off |= len <= budget;
                        "no room left to draw them here"
                    }
                };
                format!(
                    "{heading}_{} tasks and {} dependencies: {why}._\n\n",
                    p.nodes.len(),
                    edges.len()
                )
            }
        };
        if part.len() > room {
            out += &more(parts.len() - i);
            cut_off = true;
            break;
        }
        out += &part;
    }
    Some((out, cut_off))
}

/// What one part of a split graph holds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PartKey {
    /// An epic's whole subtree (index into `Snapshot::epics`).
    Epic(usize),
    /// What an epic split per nested epic holds outside them: the epic's own
    /// tasks, and the epic itself when it is a node.
    Directly(usize),
    /// The tasks in no epic.
    NoEpic,
}

/// One diagram of a graph too large for one.
struct Part<'a> {
    key: PartKey,
    nodes: Vec<&'a GraphNode>,
}

/// The edges of `edges` between two of `nodes`.
fn inner_edges<'a>(nodes: &[&GraphNode], edges: &[&'a GraphEdge]) -> Vec<&'a GraphEdge> {
    let ids: HashSet<&str> = nodes.iter().map(|n| n.task.id.as_str()).collect();
    edges
        .iter()
        .copied()
        .filter(|e| ids.contains(e.from.as_str()) && ids.contains(e.to.as_str()))
        .collect()
}

fn cycle_note(nodes: &[&GraphNode]) -> String {
    let cyclic = nodes.iter().filter(|n| n.cyclic).count();
    if cyclic > 0 {
        format!(
            "{cyclic} tasks are in a dependency cycle (red arrows) and can never become ready.\n\n"
        )
    } else {
        String::new()
    }
}

/// What every diagram of one snapshot shares: the generated Mermaid IDs.
struct Diagram<'a> {
    s: &'a Snapshot,
    /// Task ID to its index in `graph.nodes`: Mermaid node `n{index}`.
    node_index: HashMap<&'a str, usize>,
    /// Epic ID to its index in `epics`: Mermaid subgraph `e{index}`.
    epic_index: HashMap<&'a str, usize>,
}

impl<'a> Diagram<'a> {
    fn new(s: &'a Snapshot) -> Self {
        Diagram {
            s,
            node_index: s
                .graph
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.task.id.as_str(), i))
                .collect(),
            epic_index: s
                .epics
                .iter()
                .enumerate()
                .map(|(i, e)| (e.id.as_str(), i))
                .collect(),
        }
    }

    /// The subgraph a node is drawn in: its innermost shown epic.
    fn container(&self, n: &GraphNode) -> Option<usize> {
        n.task
            .epic
            .as_deref()
            .and_then(|e| self.epic_index.get(e).copied())
    }

    fn parent_of(&self, epic: usize) -> Option<usize> {
        self.s.epics[epic]
            .parent
            .as_deref()
            .and_then(|p| self.epic_index.get(p).copied())
    }

    /// Where a node sits in the epic tree: an epic that is a node at itself,
    /// any other node at its innermost shown epic.
    fn anchor(&self, n: &GraphNode) -> Option<usize> {
        self.epic_index
            .get(n.task.id.as_str())
            .copied()
            .or_else(|| self.container(n))
    }

    /// The epic directly below `under` (`None`: the top) on the way down to
    /// `anchor`, or `None` when `anchor` is `under` itself (or no epic).
    fn branch(&self, anchor: Option<usize>, under: Option<usize>) -> Option<usize> {
        let mut cur = anchor?;
        if Some(cur) == under {
            return None;
        }
        loop {
            let parent = self.parent_of(cur);
            if parent == under {
                return Some(cur);
            }
            cur = parent?;
        }
    }

    /// Split `nodes`, all below `under` (`None`: the whole graph), into
    /// parts: one per epic directly below `under`, in the order of
    /// `Snapshot::epics`, then one for the rest. An epic over the node or
    /// edge bounds that has nested epics holding nodes is split the same way
    /// in its place, and `nested` is set.
    fn split<'n>(
        &self,
        under: Option<usize>,
        nodes: Vec<&'n GraphNode>,
        edges: &[&GraphEdge],
        options: &MarkdownOptions,
        parts: &mut Vec<Part<'n>>,
        nested: &mut bool,
    ) {
        let mut groups: BTreeMap<Option<usize>, Vec<&'n GraphNode>> = BTreeMap::new();
        for n in nodes {
            groups
                .entry(self.branch(self.anchor(n), under))
                .or_default()
                .push(n);
        }
        let rest = groups.remove(&None);
        for (key, group) in groups {
            let Some(e) = key else { continue };
            let too_large = group.len() > options.max_graph_nodes
                || inner_edges(&group, edges).len() > options.max_graph_edges;
            let splittable = group
                .iter()
                .any(|n| self.branch(self.anchor(n), Some(e)).is_some());
            if too_large && splittable {
                *nested = true;
                self.split(Some(e), group, edges, options, parts, nested);
            } else {
                parts.push(Part {
                    key: PartKey::Epic(e),
                    nodes: group,
                });
            }
        }
        if let Some(group) = rest {
            let key = match under {
                Some(e) => PartKey::Directly(e),
                None => PartKey::NoEpic,
            };
            parts.push(Part { key, nodes: group });
        }
    }

    /// The diagram of `nodes` and `edges` when it is within the bounds and
    /// `budget` bytes.
    fn diagram(
        &self,
        nodes: &[&GraphNode],
        edges: &[&GraphEdge],
        options: &MarkdownOptions,
        budget: usize,
    ) -> Result<String, TooLarge> {
        if nodes.len() > options.max_graph_nodes || edges.len() > options.max_graph_edges {
            return Err(TooLarge::Bounds);
        }
        let text = self.mermaid(nodes, edges);
        if text.len() <= budget {
            Ok(text)
        } else {
            Err(TooLarge::Text {
                len: text.len(),
                budget,
            })
        }
    }

    /// A fenced Mermaid flowchart of `nodes` and the `edges` between them,
    /// each node in the subgraph of its epic.
    fn mermaid(&self, nodes: &[&GraphNode], edges: &[&GraphEdge]) -> String {
        let mut out = String::from("```mermaid\nflowchart LR\n");
        let used: Vec<Status> = STATUS_ORDER
            .iter()
            .copied()
            .filter(|st| nodes.iter().any(|n| n.task.status == *st))
            .collect();
        for st in &used {
            let _ = writeln!(out, "  classDef {} {}", class_name(*st), class_style(*st));
        }

        // Which subgraphs hold a node, directly or further in.
        let mut shown: HashSet<usize> = HashSet::new();
        for n in nodes {
            let mut cur = self.container(n);
            while let Some(e) = cur {
                if !shown.insert(e) {
                    break;
                }
                cur = self.parent_of(e);
            }
        }
        self.contents(&mut out, None, nodes, &shown, 1);

        let mut cyclic = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            let _ = writeln!(
                out,
                "  n{} --> n{}",
                self.node_index[e.from.as_str()],
                self.node_index[e.to.as_str()]
            );
            if e.cyclic {
                cyclic.push(i.to_string());
            }
        }
        if !cyclic.is_empty() {
            let _ = writeln!(
                out,
                "  linkStyle {} stroke:#cf222e,stroke-width:2px",
                cyclic.join(",")
            );
        }
        out.push_str("```\n\n");
        out
    }

    /// The subgraphs and nodes directly inside `container` (`None`: the top).
    fn contents(
        &self,
        out: &mut String,
        container: Option<usize>,
        nodes: &[&GraphNode],
        shown: &HashSet<usize>,
        depth: usize,
    ) {
        let indent = "  ".repeat(depth);
        for (i, e) in self.s.epics.iter().enumerate() {
            if shown.contains(&i) && self.parent_of(i) == container {
                let _ = writeln!(out, "{indent}subgraph e{i} [\"{}\"]", epic_label(e));
                self.contents(out, Some(i), nodes, shown, depth + 1);
                let _ = writeln!(out, "{indent}end");
            }
        }
        for n in nodes.iter().filter(|n| self.container(n) == container) {
            let label = format!(
                "{}: {}",
                mermaid_text(&n.task.id, usize::MAX),
                mermaid_text(&n.task.title, MAX_LABEL_CHARS)
            );
            let (open, close) = if n.task.task_type.is_epic() {
                ("[[", "]]")
            } else {
                ("[", "]")
            };
            let _ = writeln!(
                out,
                "{indent}n{}{open}\"{label}\"{close}:::{}",
                self.node_index[n.task.id.as_str()],
                class_name(n.task.status)
            );
        }
    }
}

fn epic_label(e: &EpicEntry) -> String {
    format!(
        "{}: {}",
        mermaid_text(&e.id, usize::MAX),
        mermaid_text(&e.title, MAX_LABEL_CHARS)
    )
}

fn class_name(status: Status) -> &'static str {
    match status {
        Status::Proposed => "proposed",
        Status::Open => "open",
        Status::Blocked => "blocked",
        Status::InProgress => "in_progress",
        Status::Review => "review",
        Status::Done => "done",
        Status::Cancelled => "cancelled",
    }
}

/// Light fills with dark text, readable on GitHub's light and dark themes.
fn class_style(status: Status) -> &'static str {
    match status {
        Status::Proposed => "fill:#f6f8fa,stroke:#8c959f,color:#1f2328,stroke-dasharray:4 3",
        Status::Open => "fill:#ddf4ff,stroke:#0969da,color:#1f2328",
        Status::Blocked => "fill:#ffebe9,stroke:#cf222e,color:#1f2328",
        Status::InProgress => "fill:#fff8c5,stroke:#9a6700,color:#1f2328",
        Status::Review => "fill:#fbefff,stroke:#8250df,color:#1f2328",
        Status::Done => "fill:#dafbe1,stroke:#1a7f37,color:#1f2328",
        Status::Cancelled => "fill:#eaeef2,stroke:#6e7781,color:#57606a",
    }
}

/// Untrusted text for a quoted Mermaid label: one line, at most `max`
/// characters (cut with `…`), and every ASCII character other than letters,
/// digits, spaces and harmless punctuation written as an entity code, so that
/// no quote, bracket, `#`, `;`, `%`, backtick or `<` can end the label, start
/// a comment or directive, or close the code fence.
fn mermaid_text(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in cut(text, max).chars() {
        if c.is_ascii_alphanumeric() || " .,-_:/'!?+=*@$^~".contains(c) || !c.is_ascii() {
            out.push(c);
        } else {
            let _ = write!(out, "#{};", c as u32);
        }
    }
    out
}

#[cfg(test)]
mod tests;
