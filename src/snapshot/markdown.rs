//! Render a [`Snapshot`] as GitHub-flavored Markdown with a Mermaid
//! dependency graph, for GitHub job summaries and PR comments.
//!
//! [`render`] (or [`render_with`] for other graph bounds) is a pure function:
//! it never loads the store or reads the clock, and the same snapshot always
//! gives the same text.
//!
//! The document has summary tables (totals, statuses, outstanding work by
//! priority), the epics with their progress, the ready, in-progress and review
//! queues, the status board in collapsible `<details>` sections, a
//! ```` ```mermaid ```` flowchart of the dependency graph, and the archive log
//! when settled work is included.
//!
//! **The graph is bounded.** GitHub renders Mermaid with a limit on the
//! diagram's text and edges, a PR comment holds at most 65 536 characters, and
//! a diagram of hundreds of tasks is unreadable anyway. When the whole graph
//! is larger than [`MarkdownOptions`] allows, it is drawn as one diagram per
//! top-level epic (plus one for the tasks in no epic), leaving out the
//! dependencies between them; a part that is still too large, or that no
//! longer fits the text budget, is replaced by a note giving its size.
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

use super::{EpicEntry, GraphEdge, GraphNode, STATUS_ORDER, Snapshot, SnapshotTask};
use crate::task::{self, Status};

/// The most tasks drawn in one Mermaid diagram by default.
pub const DEFAULT_MAX_GRAPH_NODES: usize = 100;

/// The most dependencies drawn in one Mermaid diagram by default (Mermaid's
/// own limit is 500).
pub const DEFAULT_MAX_GRAPH_EDGES: usize = 200;

/// The default budget, in bytes, for the text of all Mermaid diagrams of one
/// document together. Mermaid refuses a diagram over 50 000 characters, and a
/// PR comment holds 65 536, which leaves room for the tables.
pub const DEFAULT_MAX_GRAPH_CHARS: usize = 30_000;

/// The most characters of a title shown in a graph node; longer titles are
/// cut and end in `…`.
pub const MAX_LABEL_CHARS: usize = 40;

/// Bounds on the dependency graph of a Markdown snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// The most nodes in one diagram.
    pub max_graph_nodes: usize,
    /// The most edges in one diagram.
    pub max_graph_edges: usize,
    /// The most bytes of Mermaid text in the whole document.
    pub max_graph_chars: usize,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        MarkdownOptions {
            max_graph_nodes: DEFAULT_MAX_GRAPH_NODES,
            max_graph_edges: DEFAULT_MAX_GRAPH_EDGES,
            max_graph_chars: DEFAULT_MAX_GRAPH_CHARS,
        }
    }
}

/// Render `snapshot` with the default [`MarkdownOptions`].
pub fn render(snapshot: &Snapshot) -> String {
    render_with(snapshot, &MarkdownOptions::default())
}

/// Render `snapshot` as GitHub-flavored Markdown. See the
/// [module docs](self).
pub fn render_with(snapshot: &Snapshot, options: &MarkdownOptions) -> String {
    let s = snapshot;
    let mut out = String::new();
    out.push_str("# Bears snapshot\n\n");
    if let Some(at) = s.generated_at {
        let _ = writeln!(out, "Generated {}.", at.format("%Y-%m-%d %H:%M UTC"));
    }
    if s.include_all {
        out.push_str("Includes done and cancelled work.\n");
    } else {
        out.push_str("Open work only; done and cancelled work is counted but not listed.\n");
    }
    out.push('\n');

    summary(&mut out, s);
    epics(&mut out, s);
    queues(&mut out, s);
    board(&mut out, s);
    graph(&mut out, s, options);
    if s.include_all {
        archive(&mut out, s);
    }
    // Exactly one newline at the end.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

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

fn epics(out: &mut String, s: &Snapshot) {
    out.push_str("## Epics\n\n");
    if s.epics.is_empty() {
        out.push_str("_None._\n\n");
        return;
    }
    out.push_str("| Epic | Title | Status | Priority | Progress |\n");
    out.push_str("| --- | --- | --- | --- | ---: |\n");
    for e in &s.epics {
        let p = &e.progress;
        let progress = if p.total == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{} ({}%)", p.done, p.total, p.done * 100 / p.total)
        };
        let _ = writeln!(
            out,
            "| {} | {}{} | {} | {} | {} |",
            code(&e.id),
            "&emsp;".repeat(e.depth),
            cell(&e.title),
            e.status,
            e.priority,
            progress
        );
    }
    out.push('\n');
}

fn queues(out: &mut String, s: &Snapshot) {
    out.push_str("## Ready\n\n");
    task_table(
        out,
        &s.ready,
        &[Col::Id, Col::Title, Col::Priority, Col::Epic, Col::Tags],
    );
    out.push_str("## In progress\n\n");
    task_table(
        out,
        &s.in_progress,
        &[
            Col::Id,
            Col::Title,
            Col::Priority,
            Col::Assignee,
            Col::Attempts,
        ],
    );
    out.push_str("## Review\n\n");
    task_table(
        out,
        &s.review,
        &[
            Col::Id,
            Col::Title,
            Col::Priority,
            Col::Assignee,
            Col::Attempts,
        ],
    );
}

fn board(out: &mut String, s: &Snapshot) {
    out.push_str("## Board\n\n");
    let shown: Vec<_> = s.board.iter().filter(|c| !c.tasks.is_empty()).collect();
    if shown.is_empty() {
        out.push_str("_No tasks._\n\n");
        return;
    }
    for column in shown {
        let _ = writeln!(
            out,
            "<details><summary>{} ({})</summary>\n",
            column.status,
            column.tasks.len()
        );
        task_table(
            out,
            &column.tasks,
            &[
                Col::Id,
                Col::Title,
                Col::Priority,
                Col::Epic,
                Col::DependsOn,
            ],
        );
        out.push_str("</details>\n\n");
    }
}

fn archive(out: &mut String, s: &Snapshot) {
    out.push_str("## Recently archived\n\n");
    task_table(
        out,
        &s.archive,
        &[Col::Id, Col::Title, Col::Status, Col::Updated],
    );
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
            Col::Title => cell(&t.title),
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
            Col::Updated => t.updated.format("%Y-%m-%d").to_string(),
        }
    }
}

fn task_table(out: &mut String, tasks: &[SnapshotTask], cols: &[Col]) {
    if tasks.is_empty() {
        out.push_str("_None._\n\n");
        return;
    }
    let row = |cells: Vec<String>| format!("| {} |\n", cells.join(" | "));
    out.push_str(&row(cols.iter().map(|c| c.header().to_string()).collect()));
    out.push_str(&row(cols.iter().map(|_| "---".to_string()).collect()));
    for t in tasks {
        out.push_str(&row(cols.iter().map(|c| c.value(t)).collect()));
    }
    out.push('\n');
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

// ---------------------------------------------------------------------------
// Mermaid graph
// ---------------------------------------------------------------------------

fn graph(out: &mut String, s: &Snapshot, options: &MarkdownOptions) {
    out.push_str("## Dependency graph\n\n");
    let g = &s.graph;
    if g.nodes.is_empty() {
        out.push_str("_No tasks to draw._\n\n");
        return;
    }
    out.push_str("Arrows point from a dependency to the task that waits for it.\n\n");
    let ctx = Diagram::new(s);
    let all_nodes: Vec<&GraphNode> = g.nodes.iter().collect();
    let all_edges: Vec<&GraphEdge> = g.edges.iter().collect();
    let mut budget = options.max_graph_chars;

    if let Some(text) = ctx.fits(&all_nodes, &all_edges, options, budget) {
        out.push_str(&text);
        cycle_note(out, &all_nodes);
        return;
    }

    // Too large for one diagram: one per top-level epic, then the rest.
    let mut groups: BTreeMap<Option<usize>, Vec<&GraphNode>> = BTreeMap::new();
    for n in &all_nodes {
        groups.entry(ctx.top_epic(n)).or_default().push(n);
    }
    let group_of: HashMap<&str, Option<usize>> = all_nodes
        .iter()
        .map(|n| (n.task.id.as_str(), ctx.top_epic(n)))
        .collect();
    let crossing = all_edges
        .iter()
        .filter(|e| group_of[e.from.as_str()] != group_of[e.to.as_str()])
        .count();
    let _ = writeln!(
        out,
        "The graph has {} tasks and {} dependencies, more than one diagram shows \
         (at most {} tasks and {} dependencies), so it is drawn per top-level epic.",
        all_nodes.len(),
        all_edges.len(),
        options.max_graph_nodes,
        options.max_graph_edges
    );
    if crossing > 0 {
        let _ = writeln!(
            out,
            "{crossing} dependencies between tasks of different parts are not drawn."
        );
    }
    out.push('\n');

    // Epics first, in the order of `Snapshot::epics`; tasks in no epic last.
    let ordered = groups
        .iter()
        .filter(|(k, _)| k.is_some())
        .chain(groups.iter().filter(|(k, _)| k.is_none()));
    for (key, nodes) in ordered {
        match key {
            Some(i) => {
                let e = &s.epics[*i];
                let _ = writeln!(out, "### {} {}\n", code(&e.id), cell(&e.title));
            }
            None => out.push_str("### Not in an epic\n\n"),
        }
        let ids: HashSet<&str> = nodes.iter().map(|n| n.task.id.as_str()).collect();
        let edges: Vec<&GraphEdge> = all_edges
            .iter()
            .copied()
            .filter(|e| ids.contains(e.from.as_str()) && ids.contains(e.to.as_str()))
            .collect();
        match ctx.fits(nodes, &edges, options, budget) {
            Some(text) => {
                budget -= text.len();
                out.push_str(&text);
                cycle_note(out, nodes);
            }
            None => {
                let _ = writeln!(
                    out,
                    "_{} tasks and {} dependencies: too large to draw here._\n",
                    nodes.len(),
                    edges.len()
                );
            }
        }
    }
}

fn cycle_note(out: &mut String, nodes: &[&GraphNode]) {
    let cyclic = nodes.iter().filter(|n| n.cyclic).count();
    if cyclic > 0 {
        let _ = writeln!(
            out,
            "{cyclic} tasks are in a dependency cycle (red arrows) and can never become ready.\n"
        );
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

    /// The top-level epic whose part a node belongs to; an epic that is a
    /// node belongs to its own part.
    fn top_epic(&self, n: &GraphNode) -> Option<usize> {
        let mut cur = self
            .epic_index
            .get(n.task.id.as_str())
            .copied()
            .or_else(|| self.container(n))?;
        while let Some(p) = self.parent_of(cur) {
            cur = p;
        }
        Some(cur)
    }

    /// The diagram of `nodes` and `edges` when it is within the bounds and
    /// `budget` bytes.
    fn fits(
        &self,
        nodes: &[&GraphNode],
        edges: &[&GraphEdge],
        options: &MarkdownOptions,
        budget: usize,
    ) -> Option<String> {
        if nodes.len() > options.max_graph_nodes || edges.len() > options.max_graph_edges {
            return None;
        }
        let text = self.mermaid(nodes, edges);
        (text.len() <= budget).then_some(text)
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
    let line = one_line(text);
    let mut chars: Vec<char> = line.chars().collect();
    if chars.len() > max {
        chars.truncate(max.saturating_sub(1));
        while chars.last() == Some(&' ') {
            chars.pop();
        }
        chars.push('…');
    }
    let mut out = String::with_capacity(line.len());
    for c in chars {
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
