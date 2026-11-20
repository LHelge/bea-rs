use std::collections::HashMap;

use crate::task::timestamp::utc;

use super::*;
use crate::snapshot::{SnapshotOptions, build};
use crate::task::{Priority, Status, Task, TaskType};

fn make(id: &str, status: Status, minute: u32, deps: &[&str]) -> Task {
    let mut t = Task::new(id.to_string(), format!("Task {id}"), Priority::P2);
    t.status = status;
    t.created = utc(2026, 1, 1, 0, minute, 0);
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

fn snap(tasks: Vec<Task>) -> Snapshot {
    build(&map(tasks), &HashMap::new(), &SnapshotOptions::default())
}

fn all() -> SnapshotOptions {
    SnapshotOptions {
        include_all: true,
        ..SnapshotOptions::default()
    }
}

/// The text of every ```mermaid block.
fn mermaid_blocks(md: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut cur: Option<String> = None;
    for line in md.lines() {
        match (&mut cur, line) {
            (None, "```mermaid") => cur = Some(String::new()),
            (Some(_), "```") => blocks.push(cur.take().unwrap()),
            (Some(b), l) => {
                assert!(!l.contains("```"), "a fence inside a diagram: {l}");
                b.push_str(l);
                b.push('\n');
            }
            (None, _) => {}
        }
    }
    assert!(cur.is_none(), "unclosed mermaid block");
    blocks
}

/// The section of `md` under the heading `## {name}`.
fn section<'a>(md: &'a str, name: &str) -> &'a str {
    let head = format!("## {name}\n");
    let start = md
        .find(&head)
        .unwrap_or_else(|| panic!("no section {name}"))
        + head.len();
    let rest = &md[start..];
    &rest[..rest.find("\n## ").map_or(rest.len(), |e| e + 1)]
}

/// The cells of a table row, split on unescaped pipes.
fn cells(row: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let last = cells.last_mut().unwrap();
                last.push(c);
                if let Some(n) = chars.next() {
                    last.push(n);
                }
            }
            '|' => cells.push(String::new()),
            c => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

/// Every table in `md` has as many cells in each row as in its header.
fn assert_tables_well_formed(md: &str) {
    let mut width: Option<usize> = None;
    for line in md.lines() {
        if line.starts_with('|') {
            let n = cells(line).len();
            match width {
                None => width = Some(n),
                Some(w) => assert_eq!(n, w, "row breaks its table: {line}"),
            }
        } else {
            width = None;
        }
    }
}

#[test]
fn empty_store_has_every_section_and_no_diagram() {
    let md = render(&snap(vec![]));
    for name in [
        "Summary",
        "Epics",
        "Ready",
        "In progress",
        "Review",
        "Board",
        "Dependency graph",
    ] {
        section(&md, name);
    }
    assert!(!md.contains("Recently archived"));
    assert!(mermaid_blocks(&md).is_empty());
    assert!(md.contains("_No tasks to draw._"));
    assert!(md.starts_with("# Bears snapshot\n"));
    assert!(md.ends_with(".\n") || md.ends_with("_\n"));
    assert!(!md.ends_with("\n\n"));
    assert!(!md.contains("Generated"));
}

#[test]
fn generated_at_is_shown_when_given() {
    let options = SnapshotOptions {
        generated_at: Some(utc(2026, 11, 20, 12, 5, 0)),
        ..SnapshotOptions::default()
    };
    let s = build(&HashMap::new(), &HashMap::new(), &options);
    assert!(render(&s).contains("Generated 2026-11-20 12:05 UTC."));
}

#[test]
fn output_is_deterministic() {
    let tasks = || {
        vec![
            epic("e", 0),
            make("e.a", Status::Open, 1, &[]),
            make("e.b", Status::Open, 1, &["e.a"]),
            make("c", Status::InProgress, 1, &["e.b"]),
            make("d", Status::Review, 2, &[]),
        ]
    };
    let mut reversed = tasks();
    reversed.reverse();
    let a = render(&snap(tasks()));
    assert_eq!(a, render(&snap(tasks())));
    assert_eq!(a, render(&snap(reversed)));
}

#[test]
fn summary_counts_statuses_and_outstanding_priorities() {
    let mut urgent = make("u", Status::Open, 0, &[]);
    urgent.priority = Priority::P0;
    let md = render(&snap(vec![
        urgent,
        make("a", Status::Open, 1, &[]),
        make("b", Status::Done, 2, &[]),
        epic("e", 3),
    ]));
    let summary = section(&md, "Summary");
    assert!(summary.contains("3 tasks, 1 epics, 0 archived."));
    assert!(summary.contains("| open | 2 |"));
    assert!(summary.contains("| done | 1 |"));
    assert!(summary.contains("| review | 0 |"));
    assert!(summary.contains("| P0 | 1 |"));
    assert!(summary.contains("| P2 | 1 |"));
    assert_tables_well_formed(&md);
}

#[test]
fn epics_show_progress_and_nesting() {
    let md = render(&snap(vec![
        epic("e", 0),
        epic("e.n", 1),
        make("e.a", Status::Done, 2, &[]),
        make("e.b", Status::Open, 3, &[]),
        make("e.n.x", Status::Open, 4, &[]),
    ]));
    let epics = section(&md, "Epics");
    // `e` has three direct children (one nested epic), one done.
    assert!(
        epics.contains("| `e` | Epic e | open | P2 | 1/3 (33%) |"),
        "{epics}"
    );
    assert!(epics.contains("| `e.n` | &emsp;Epic e.n | open | P2 | 0/1 (0%) |"));
    assert!(epics.find("`e`").unwrap() < epics.find("`e.n`").unwrap());
}

#[test]
fn queues_list_ready_in_progress_and_review_work() {
    let mut started = make("s", Status::InProgress, 0, &[]);
    started.assignee = "worker-1".into();
    started.attempts = Some(3);
    let mut reviewed = make("r", Status::Review, 1, &[]);
    reviewed.assignee = "worker-2".into();
    let mut ready = make("e.a", Status::Open, 2, &[]);
    ready.tags = vec!["backend".into(), "auth".into()];
    let md = render(&snap(vec![epic("e", 0), started, reviewed, ready]));

    let r = section(&md, "Ready");
    assert!(r.contains("| ID | Title | Priority | Epic | Tags |"));
    assert!(
        r.contains("| `e.a` | Task e.a | P2 | `e` | backend, auth |"),
        "{r}"
    );
    let ip = section(&md, "In progress");
    assert!(ip.contains("| `s` | Task s | P2 | worker-1 | 3 |"), "{ip}");
    let rv = section(&md, "Review");
    assert!(rv.contains("| `r` | Task r | P2 | worker-2 | 0 |"), "{rv}");
    assert_tables_well_formed(&md);
}

#[test]
fn empty_queues_say_none() {
    let md = render(&snap(vec![make("b", Status::Blocked, 0, &[])]));
    assert!(section(&md, "Ready").contains("_None._"));
    assert!(section(&md, "Review").contains("_None._"));
}

#[test]
fn effective_priority_is_shown_next_to_the_priority() {
    let mut urgent = make("u", Status::Open, 1, &["a"]);
    urgent.priority = Priority::P0;
    let md = render(&snap(vec![make("a", Status::Open, 0, &[]), urgent]));
    assert!(md.contains("| `a` | Task a | P2 (effective P0) |"), "{md}");
}

#[test]
fn board_collapses_columns_and_settled_work_needs_include_all() {
    let tasks = || {
        map(vec![
            make("a", Status::Open, 0, &[]),
            make("b", Status::Done, 1, &[]),
            make("c", Status::Open, 2, &["a", "b"]),
        ])
    };
    let mut archived = make("z", Status::Done, 3, &[]);
    archived.updated = utc(2026, 3, 4, 0, 0, 0);
    let archive = map(vec![archived]);

    let md = render(&build(&tasks(), &archive, &SnapshotOptions::default()));
    let board = section(&md, "Board");
    assert!(board.contains("<details><summary>open (2)</summary>\n\n| ID |"));
    assert!(
        board.contains("| `c` | Task c | P2 |  | `a`, `b` |"),
        "{board}"
    );
    assert!(!board.contains("summary>done"));
    assert!(!board.contains("blocked ("), "empty columns are left out");
    assert!(!md.contains("Recently archived"));

    let md = render(&build(&tasks(), &archive, &all()));
    assert!(section(&md, "Board").contains("<details><summary>done (1)</summary>"));
    let log = section(&md, "Recently archived");
    assert!(
        log.contains("| `z` | Task z | done | 2026-03-04 |"),
        "{log}"
    );
    assert_tables_well_formed(&md);
}

#[test]
fn graph_draws_epics_as_subgraphs_and_statuses_as_classes() {
    let md = render(&snap(vec![
        epic("e", 0),
        epic("e.n", 1),
        make("e.a", Status::Open, 2, &[]),
        make("e.n.x", Status::InProgress, 3, &["e.a"]),
        make("r", Status::Review, 4, &["e.n.x"]),
    ]));
    let blocks = mermaid_blocks(&md);
    assert_eq!(blocks.len(), 1);
    let d = &blocks[0];
    assert!(d.starts_with("flowchart LR\n"));
    assert!(d.contains("classDef open "));
    assert!(d.contains("classDef in_progress "));
    assert!(d.contains("classDef review "));
    assert!(!d.contains("classDef done "), "only used classes");
    // Nested subgraphs with generated IDs.
    let outer = d
        .find("subgraph e0 [\"e: Epic e\"]")
        .expect("outer subgraph");
    let inner = d
        .find("subgraph e1 [\"e.n: Epic e.n\"]")
        .expect("inner subgraph");
    assert!(outer < inner);
    assert!(d.contains(":::in_progress"));
    // Node IDs are generated: no task ID (with its dot) is a node ID.
    for line in d.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix('n') {
            let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            assert!(!id.is_empty(), "bad node line {line}");
        }
    }
    assert_eq!(d.matches(" --> ").count(), 2);
    assert!(!d.contains("linkStyle"));
    assert!(md.contains("Arrows point from a dependency"));
}

#[test]
fn an_epic_node_gets_its_own_id_and_shape() {
    // The epic `e` is a dependency of `t`, so it is a node and a subgraph.
    let md = render(&snap(vec![
        epic("e", 0),
        make("e.a", Status::Open, 1, &[]),
        make("t", Status::Open, 2, &["e"]),
    ]));
    let d = &mermaid_blocks(&md)[0];
    assert!(d.contains("subgraph e0 [\"e: Epic e\"]"));
    let epic_line = d
        .lines()
        .find(|l| l.contains("[[\"e: Epic e\"]]"))
        .expect("epic node");
    assert!(epic_line.trim().starts_with('n'));
}

#[test]
fn cycles_are_drawn_red_and_noted() {
    let md = render(&snap(vec![
        make("a", Status::Open, 0, &["b"]),
        make("b", Status::Open, 1, &["a"]),
        make("c", Status::Open, 2, &["a"]),
    ]));
    let d = &mermaid_blocks(&md)[0];
    // Edges sorted by (from, to): a→b, a→c, b→a; the cycle is edges 0 and 2.
    assert!(d.contains("  linkStyle 0,2 stroke:#cf222e"), "{d}");
    assert!(md.contains("2 tasks are in a dependency cycle"));
}

const HOSTILE: &str = "a | b\n| --- | --- |\r\n<script>alert(1)</script> `code` \
    [x](javascript:alert) **bold** _it_ ~~s~~ $x^2$ #1 @alice \\| !\u{7}\
    \"]) --> X; click X call evil() %%{init: {}}%% ``` ~~~ {curly} (round) & end";

#[test]
fn hostile_titles_cannot_break_tables() {
    let mut a = make("a", Status::Open, 0, &[]);
    a.title = HOSTILE.into();
    a.tags = vec!["t|g".into(), "<b>".into()];
    let mut s = make("s", Status::InProgress, 1, &[]);
    s.title = HOSTILE.into();
    s.assignee = "x | y\nz".into();
    let mut e = epic("e", 2);
    e.title = HOSTILE.into();
    let md = render(&snap(vec![a, s, e]));

    assert_tables_well_formed(&md);
    assert!(!md.contains("<script"));
    assert!(!md.contains("<b>"));
    assert!(!md.contains("@alice"), "no mention");
    assert!(md.contains("@\u{2060}alice"));
    assert!(!md.contains('\u{7}'));
    for line in md.lines().filter(|l| l.starts_with('|')) {
        // Every special character of the title arrives escaped.
        if line.contains("alert") {
            assert!(line.contains("&lt;script&gt;"));
            assert!(line.contains("\\`code\\`"));
            assert!(line.contains("\\[x\\]"));
            assert!(line.contains("\\*\\*bold\\*\\*"));
            assert!(line.contains("\\$x^2\\$"));
            assert!(line.contains("&amp; end"));
            assert!(!line.contains("```"));
        }
    }
    // Nothing but the diagram fence opens a code block.
    assert_eq!(md.matches("```").count(), 2);
}

#[test]
fn hostile_titles_cannot_break_the_diagram() {
    // Short pieces, so that none is cut off before it reaches the label.
    let pieces = [
        "\"]) --> X",
        "X; click X call evil()",
        "%%{init: {'theme':'dark'}}%%",
        "%% comment",
        "```",
        "~~~ fence",
        "<b>bold</b> & <img src=x>",
        "a | b # c",
        "{curly} (round) [square]",
        "`markdown string`",
        "line\nbreak\r\nend",
        "n0 --> n1",
        "subgraph x\nend",
    ];
    let mut e = epic("e", 0);
    e.title = pieces[0].into();
    let mut tasks = vec![e, make("b", Status::Open, 0, &["e.p0"])];
    for (i, p) in pieces.iter().enumerate() {
        let mut t = make(&format!("e.p{i}"), Status::Open, i as u32, &[]);
        t.title = (*p).into();
        tasks.push(t);
    }
    let md = render(&snap(tasks));
    let blocks = mermaid_blocks(&md);
    assert_eq!(blocks.len(), 1);
    let d = &blocks[0];
    for line in d.lines() {
        let Some(start) = line.find('"') else {
            continue;
        };
        let end = line.rfind('"').unwrap();
        assert!(end > start, "one quoted label per line: {line}");
        let label = &line[start + 1..end];
        let plain: String = {
            // Entity codes are the only `#` and `;` left.
            let mut out = String::new();
            let mut rest = label;
            while let Some(i) = rest.find('#') {
                out.push_str(&rest[..i]);
                let after = &rest[i + 1..];
                let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
                assert!(digits > 0 && after[digits..].starts_with(';'), "{label}");
                rest = &after[digits + 1..];
            }
            out.push_str(rest);
            out
        };
        for bad in [
            '"', '[', ']', '(', ')', '{', '}', '<', '>', ';', '%', '`', '|', '&', '#',
        ] {
            assert!(!plain.contains(bad), "{bad:?} in label {label}");
        }
        // The structure around the label is ours.
        let before = line[..start].trim();
        let after = &line[end + 1..];
        assert!(
            before.starts_with("subgraph e") || before.starts_with('n'),
            "{line}"
        );
        assert!(after.ends_with(":::open") || after == "]", "{line}");
    }
    assert_eq!(d.matches(" --> ").count(), 1, "a title adds no edge");
    let starting = |w: &str| d.lines().filter(|l| l.trim_start().starts_with(w)).count();
    assert_eq!(starting("subgraph"), 1, "a title adds no subgraph");
    assert_eq!(d.lines().filter(|l| l.trim() == "end").count(), 1);
    assert_eq!(starting("n"), pieces.len() + 2, "{d}");
    // Every piece reached the diagram whole.
    assert!(!d.contains('…'));
    assert_eq!(starting("click"), 0, "a title adds no click handler");
    assert_eq!(starting("%%"), 0, "a title adds no comment or directive");
}

#[test]
fn long_titles_are_cut_in_the_diagram_only() {
    let mut a = make("a", Status::Open, 0, &[]);
    a.title = "word ".repeat(30);
    let md = render(&snap(vec![a]));
    let d = &mermaid_blocks(&md)[0];
    let label = d.lines().find(|l| l.contains("a: ")).unwrap();
    assert!(label.contains("…\"]"), "{label}");
    let shown = label
        .split("a: ")
        .nth(1)
        .unwrap()
        .trim_end_matches("\"]:::open");
    assert_eq!(shown.chars().count(), MAX_LABEL_CHARS);
    // The table keeps the whole title (under `MAX_CELL_CHARS`), on one line.
    assert!(md.contains(&format!("| `a` | {} |", "word ".repeat(30).trim())));
}

#[test]
fn malformed_ids_are_escaped_not_code_spans() {
    let mut a = make("a", Status::Open, 0, &["x`|y"]);
    a.id = "B`|<i>".into();
    let md = render(&snap(vec![a]));
    assert_tables_well_formed(&md);
    assert!(md.contains("B\\`\\|&lt;i&gt;"));
    assert!(md.contains("x\\`\\|y"));
    let d = &mermaid_blocks(&md)[0];
    assert!(d.contains("B#96;#124;#60;i#62;: "), "{d}");
}

/// Two epics of `n` chained tasks each, plus a task outside both that
/// depends on both chains.
fn two_epics(n: u32) -> Snapshot {
    let mut tasks = vec![epic("e", 0), epic("f", 0)];
    for (epic_id, base) in [("e", 0), ("f", 100)] {
        for i in 0..n {
            let id = format!("{epic_id}.t{i}");
            let dep = format!("{epic_id}.t{}", i.wrapping_sub(1));
            let deps: Vec<&str> = if i == 0 { vec![] } else { vec![dep.as_str()] };
            tasks.push(make(&id, Status::Open, (base + i) % 60, &deps));
        }
    }
    tasks.push(make("z", Status::Open, 1, &["e.t0", "f.t0"]));
    snap(tasks)
}

#[test]
fn a_graph_within_the_bounds_is_one_diagram() {
    let s = two_epics(5);
    assert_eq!(s.graph.nodes.len(), 11);
    let md = render(&s);
    assert_eq!(mermaid_blocks(&md).len(), 1);
    assert!(!md.contains("drawn per top-level epic"));
}

#[test]
fn a_graph_over_the_bounds_is_drawn_per_epic() {
    let s = two_epics(5);
    let options = MarkdownOptions {
        max_graph_nodes: 6,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    let graph = section(&md, "Dependency graph");
    assert!(graph.contains("The graph has 11 tasks and 10 dependencies"));
    assert!(graph.contains("2 dependencies between tasks of different parts are not drawn."));
    let blocks = mermaid_blocks(graph);
    assert_eq!(blocks.len(), 3);
    let e = graph.find("### `e` Epic e").unwrap();
    let f = graph.find("### `f` Epic f").unwrap();
    let rest = graph.find("### Not in an epic").unwrap();
    assert!(e < f && f < rest);
    for b in &blocks {
        let nodes = b.lines().filter(|l| l.contains(":::")).count();
        assert!(nodes <= options.max_graph_nodes);
    }
    // Node IDs stay the same across the parts: `z` keeps its index.
    let whole = render(&s);
    let z_line = |md: &str| {
        md.lines()
            .find(|l| l.contains("\"z: Task z\""))
            .unwrap()
            .trim()
            .to_string()
    };
    assert_eq!(z_line(&whole), z_line(&md));
}

#[test]
fn a_part_over_the_bounds_is_a_note() {
    let s = two_epics(5);
    let options = MarkdownOptions {
        max_graph_nodes: 6,
        max_graph_edges: 3,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    // Each epic chain has 4 edges: too many; the lone `z` still fits.
    assert_eq!(mermaid_blocks(&md).len(), 1);
    assert_eq!(
        md.matches("_5 tasks and 4 dependencies: too large to draw here._")
            .count(),
        2
    );
}

#[test]
fn the_text_budget_covers_every_diagram() {
    let s = two_epics(5);
    let whole = render(&s);
    let one = mermaid_blocks(&whole)[0].len();
    // Too small for the whole graph, large enough for one epic only.
    let options = MarkdownOptions {
        max_graph_chars: one / 2,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    let drawn: usize = mermaid_blocks(&md).iter().map(String::len).sum();
    assert!(drawn <= one / 2);
    assert!(md.contains("no room left to draw them here"));
    assert!(!md.contains("too large to draw here"));
    assert!(!mermaid_blocks(&md).is_empty());
}

#[test]
fn a_graph_over_the_text_budget_says_so_not_the_node_bounds() {
    let s = two_epics(5);
    let one = mermaid_blocks(&render(&s))[0].len();
    let options = MarkdownOptions {
        max_graph_chars: one - 1,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    // 11 tasks and 10 dependencies are within the node and edge bounds.
    assert!(!md.contains("more than one diagram shows"), "{md}");
    assert!(
        md.contains(&format!(
            "One diagram of the graph's 11 tasks would take {} bytes, more than the {} this \
         document has room for, so it is drawn per top-level epic.",
            // The fences and the blank line after the diagram count too.
            "```mermaid\n".len() + one + "```\n\n".len(),
            one - 1
        )),
        "{md}"
    );
    assert!(md.contains("drawn per top-level epic"));
}

/// A store far too large for one PR comment: `n` tasks with long, hostile
/// titles under 20 epics, chained by dependencies.
fn huge(n: u32, include_all: bool) -> Snapshot {
    let mut tasks: Vec<Task> = (0..20).map(|i| epic(&format!("e{i:02}"), 0)).collect();
    for i in 0..n {
        let id = format!("e{:02}.t{i:04}", i % 20);
        let status = match i % 5 {
            0 | 1 => Status::Open,
            2 => Status::InProgress,
            3 => Status::Review,
            _ => Status::Done,
        };
        let dep = format!("e{:02}.t{:04}", i % 20, i.wrapping_sub(20));
        let deps: Vec<&str> = if i >= 20 && i % 3 == 0 {
            vec![dep.as_str()]
        } else {
            vec![]
        };
        let mut t = make(&id, status, i % 60, &deps);
        t.title = format!("{i} {}", HOSTILE.replace('\n', " "));
        t.tags = vec!["a|b".into(), "@x".into()];
        tasks.push(t);
    }
    let options = SnapshotOptions {
        include_all,
        ..SnapshotOptions::default()
    };
    let archive = map((0..30)
        .map(|i| {
            let mut t = make(&format!("z{i:02}"), Status::Done, i, &[]);
            t.title = HOSTILE.into();
            t
        })
        .collect());
    build(&map(tasks), &archive, &options)
}

/// The structure a cut must never break.
fn assert_well_formed(md: &str) {
    assert_tables_well_formed(md);
    mermaid_blocks(md);
    assert_eq!(
        md.matches("<details>").count(),
        md.matches("</details>").count(),
        "unbalanced details"
    );
    assert!(md.ends_with('\n') && !md.ends_with("\n\n"));
}

#[test]
fn a_huge_store_fits_a_pr_comment() {
    let s = huge(1500, false);
    let md = render(&s);
    assert!(md.len() <= DEFAULT_MAX_DOCUMENT_CHARS, "{} bytes", md.len());
    assert!(md.chars().count() < 65_536);
    assert_well_formed(&md);
    // The cut is visible, and the counts are complete.
    assert!(md.contains("**Shortened.**"));
    assert!(md.contains("more not shown._") || md.contains("rows not shown._"));
    assert!(md.contains("1500 tasks, 20 epics, 30 archived."));
    assert!(md.contains("| open | 600 |"));
    assert!(md.contains("| done | 300 |"));
    // Titles are cut in tables.
    assert!(md.lines().all(|l| l.len() < 4_000));
    assert_eq!(md, render(&s), "deterministic");
}

#[test]
fn a_raised_budget_shows_everything() {
    let s = huge(1500, true);
    let options = MarkdownOptions {
        max_document_chars: 16 << 20,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    assert!(md.len() > DEFAULT_MAX_DOCUMENT_CHARS);
    assert!(!md.contains("**Shortened.**"));
    assert!(!md.contains("not shown._"));
    assert_well_formed(&md);
    assert!(md.contains("## Recently archived"));
    // The graph keeps its own bounds.
    assert!(mermaid_blocks(&md).iter().map(String::len).sum::<usize>() <= DEFAULT_MAX_GRAPH_CHARS);
}

#[test]
fn no_budget_leaves_the_summary_and_the_note() {
    let options = MarkdownOptions {
        max_document_chars: 0,
        ..MarkdownOptions::default()
    };
    let md = render_with(&huge(50, false), &options);
    assert!(md.contains("## Summary"));
    assert!(md.contains("50 tasks, 20 epics, 30 archived."));
    assert!(md.contains("**Shortened.**"));
    assert!(!md.contains("## Ready"));
    assert!(!md.contains("## Dependency graph"));
    assert_well_formed(&md);
}

#[test]
fn every_budget_is_kept_without_breaking_the_structure() {
    let s = huge(300, true);
    let floor = render_with(
        &s,
        &MarkdownOptions {
            max_document_chars: 0,
            ..MarkdownOptions::default()
        },
    )
    .len();
    for budget in (floor..floor + 40_000).step_by(997) {
        let options = MarkdownOptions {
            max_document_chars: budget,
            ..MarkdownOptions::default()
        };
        let md = render_with(&s, &options);
        assert!(md.len() <= budget, "{} > {budget}", md.len());
        assert_well_formed(&md);
        assert!(md.contains("**Shortened.**"));
    }
}

#[test]
fn a_cut_table_ends_after_a_whole_row_with_a_count() {
    let tasks: Vec<Task> = (0..40)
        .map(|i| make(&format!("t{i:02}"), Status::Open, i, &[]))
        .collect();
    let s = snap(tasks);
    let full = render_with(
        &s,
        &MarkdownOptions {
            max_document_chars: 1 << 20,
            ..MarkdownOptions::default()
        },
    );
    let options = MarkdownOptions {
        max_document_chars: full.len() - 500,
        ..MarkdownOptions::default()
    };
    let md = render_with(&s, &options);
    assert!(md.len() <= options.max_document_chars);
    assert_well_formed(&md);
    let shown = section(&md, "Board")
        .lines()
        .filter(|l| l.starts_with("| `t"))
        .count();
    assert!(shown < 40, "{md}");
    let note = format!("_…and {} more not shown._", 40 - shown);
    assert!(section(&md, "Board").contains(&note), "{md}");
    assert!(md.contains("**Shortened.**"));
    assert!(!full.contains("**Shortened.**"));
}

#[test]
fn titles_are_cut_in_tables() {
    let mut a = make("a", Status::Open, 0, &[]);
    a.title = "x".repeat(MAX_CELL_CHARS + 50);
    let md = render(&snap(vec![a]));
    let cut = format!("| `a` | {}… |", "x".repeat(MAX_CELL_CHARS - 1));
    assert!(md.contains(&cut), "{md}");
}

#[test]
fn the_default_bounds_respect_github_limits() {
    // A worst case within the node bound: every label as long and as
    // escaped as it can be.
    let mut tasks = Vec::new();
    for i in 0..DEFAULT_MAX_GRAPH_NODES as u32 {
        let mut t = make(&format!("t{i:03}"), Status::Open, i % 60, &[]);
        t.title = "\"".repeat(200);
        tasks.push(t);
    }
    let md = render(&snap(tasks));
    for b in mermaid_blocks(&md) {
        assert!(b.len() <= DEFAULT_MAX_GRAPH_CHARS);
        assert!(b.len() < 50_000, "Mermaid's maxTextSize");
    }
    const { assert!(DEFAULT_MAX_GRAPH_EDGES < 500) };
}
