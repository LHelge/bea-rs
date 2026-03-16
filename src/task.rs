use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Done,
    Blocked,
    Cancelled,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Open => write!(f, "open"),
            Status::InProgress => write!(f, "in_progress"),
            Status::Done => write!(f, "done"),
            Status::Blocked => write!(f, "blocked"),
            Status::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for Status {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "done" => Ok(Status::Done),
            "blocked" => Ok(Status::Blocked),
            "cancelled" => Ok(Status::Cancelled),
            _ => Err(format!("invalid status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let rank = |p: &Priority| -> u8 {
            match p {
                Priority::P0 => 0,
                Priority::P1 => 1,
                Priority::P2 => 2,
                Priority::P3 => 3,
            }
        };
        rank(self).cmp(&rank(other))
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::P0 => write!(f, "P0"),
            Priority::P1 => write!(f, "P1"),
            Priority::P2 => write!(f, "P2"),
            Priority::P3 => write!(f, "P3"),
        }
    }
}

impl std::str::FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "P0" => Ok(Priority::P0),
            "P1" => Ok(Priority::P1),
            "P2" => Ok(Priority::P2),
            "P3" => Ok(Priority::P3),
            _ => Err(format!("invalid priority: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    #[serde(skip)]
    pub body: String,
}

impl Task {
    pub fn new(id: String, title: String, priority: Priority) -> Self {
        let now = Utc::now();
        Task {
            id,
            title,
            status: Status::Open,
            priority,
            created: now,
            updated: now,
            tags: Vec::new(),
            depends_on: Vec::new(),
            parent: None,
            assignee: String::new(),
            body: String::new(),
        }
    }

    /// Compact projection: id, title, status, priority, tags, and optional effective priority.
    pub fn summary(&self, effective_priority: Option<&Priority>) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            status: self.status.clone(),
            priority: self.priority,
            tags: self.tags.clone(),
            effective_priority: effective_priority
                .filter(|ep| *ep < &self.priority)
                .copied(),
        }
    }

    /// Full projection: all summary fields plus body, deps, parent, assignee, timestamps.
    pub fn detail(&self, effective_priority: Option<&Priority>) -> TaskDetail {
        TaskDetail {
            summary: self.summary(effective_priority),
            body: self.body.clone(),
            depends_on: self.depends_on.clone(),
            parent: self.parent.clone(),
            assignee: self.assignee.clone(),
            created: self.created,
            updated: self.updated,
        }
    }
}

/// Compact task projection used by list/ready/create/update responses.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_priority: Option<Priority>,
}

/// Full task projection used by show/get_task responses.
#[derive(Debug, Serialize)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub summary: TaskSummary,
    pub body: String,
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub assignee: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

// ── Shared filtering & sorting helpers ──────────────────────────────

/// Whether a task is in an "active" status (not done or cancelled).
pub fn is_active(task: &Task) -> bool {
    !matches!(task.status, Status::Done | Status::Cancelled)
}

/// Whether a task matches the given tag (if any).
pub fn matches_tag(task: &Task, tag: Option<&str>) -> bool {
    tag.is_none_or(|t| task.tags.iter().any(|tt| tt == t))
}

/// Canonical sort: priority ascending (P0 first), then creation date ascending.
pub fn sort_by_priority_owned(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));
}

/// Generate a short hex ID from UUID v4, retrying on collision.
pub fn generate_id(existing: &HashSet<String>, length: usize) -> String {
    loop {
        let uuid = uuid::Uuid::new_v4().to_string().replace('-', "");
        let id = &uuid[..length];
        let id = id.to_string();
        if !existing.contains(&id) {
            return id;
        }
    }
}

/// Convert a title to a URL-friendly slug.
pub fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive dashes and trim dashes from ends
    let mut result = String::new();
    let mut prev_dash = true; // treat start as dash to trim leading
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    // Trim trailing dash
    if result.ends_with('-') {
        result.pop();
    }
    result
}

/// Build the filename for a task: `{id}-{slug}.md`
pub fn filename(task: &Task) -> String {
    format!("{}-{}.md", task.id, slugify(&task.title))
}

/// Parse a task from markdown content with YAML frontmatter.
///
/// Handles BOM, CRLF/LF line endings, and validates that `---` delimiters
/// appear on their own lines. Returns a clear error for malformed input.
pub fn parse_task(content: &str) -> Result<Task> {
    // Strip BOM and normalize CRLF → LF
    let content = content.trim_start_matches('\u{feff}');
    let content = content.replace("\r\n", "\n");

    let mut lines = content.split('\n');

    // First line must be exactly "---"
    match lines.next() {
        Some(line) if line.trim_end() == "---" => {}
        _ => {
            return Err(Error::InvalidFrontmatter {
                path: "".into(),
                reason: "missing opening --- delimiter".into(),
            });
        }
    }

    // Collect YAML lines until we hit a closing "---"
    let mut yaml_lines = Vec::new();
    let mut found_closing = false;
    for line in &mut lines {
        if line.trim_end() == "---" {
            found_closing = true;
            break;
        }
        yaml_lines.push(line);
    }

    if !found_closing {
        return Err(Error::InvalidFrontmatter {
            path: "".into(),
            reason: "missing closing --- delimiter".into(),
        });
    }

    let yaml_str = yaml_lines.join("\n");

    // Everything after the closing delimiter is the body
    let remaining: Vec<&str> = lines.collect();
    let body_raw = remaining.join("\n");
    let body = body_raw.trim_start_matches('\n').to_string();

    let mut task: Task = serde_yaml::from_str(&yaml_str)?;
    task.body = body;
    Ok(task)
}

/// Render a task back to markdown with YAML frontmatter.
pub fn render_task(task: &Task) -> String {
    let yaml = serde_yaml::to_string(task).expect("task serialization should not fail");
    if task.body.is_empty() {
        format!("---\n{yaml}---\n")
    } else {
        format!("---\n{yaml}---\n\n{}", task.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Implement OAuth flow"), "implement-oauth-flow");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Fix bug #123!"), "fix-bug-123");
    }

    #[test]
    fn test_slugify_leading_trailing() {
        assert_eq!(slugify("  hello world  "), "hello-world");
    }

    #[test]
    fn test_slugify_consecutive_special() {
        assert_eq!(slugify("a---b___c"), "a-b-c");
    }

    #[test]
    fn test_generate_id_unique() {
        let existing = HashSet::new();
        let id = generate_id(&existing, 4);
        assert_eq!(id.len(), 4);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_id_avoids_collision() {
        let mut existing = HashSet::new();
        let id1 = generate_id(&existing, 4);
        existing.insert(id1.clone());
        let id2 = generate_id(&existing, 4);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_parse_render_roundtrip() {
        let content = "---
id: a1b2
title: Test task
status: open
priority: P1
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
tags:
- backend
depends_on:
- f4c9
assignee: ''
---

This is the body.
";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "a1b2");
        assert_eq!(task.title, "Test task");
        assert_eq!(task.status, Status::Open);
        assert_eq!(task.priority, Priority::P1);
        assert_eq!(task.tags, vec!["backend"]);
        assert_eq!(task.depends_on, vec!["f4c9"]);
        assert_eq!(task.body, "This is the body.\n");

        // Re-render and re-parse
        let rendered = render_task(&task);
        let task2 = parse_task(&rendered).unwrap();
        assert_eq!(task2.id, task.id);
        assert_eq!(task2.title, task.title);
        assert_eq!(task2.body, task.body);
    }

    #[test]
    fn test_parse_no_body() {
        let content = "---
id: x1y2
title: No body task
status: done
priority: P3
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
---
";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "x1y2");
        assert_eq!(task.body, "");
    }

    #[test]
    fn test_parse_missing_delimiter() {
        let content = "id: broken\ntitle: No delimiters\n";
        assert!(parse_task(content).is_err());
    }

    #[test]
    fn test_filename() {
        let task = Task::new("a1b2".into(), "Implement OAuth flow".into(), Priority::P1);
        assert_eq!(filename(&task), "a1b2-implement-oauth-flow.md");
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::P0 < Priority::P1);
        assert!(Priority::P1 < Priority::P2);
        assert!(Priority::P2 < Priority::P3);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(Status::InProgress.to_string(), "in_progress");
        assert_eq!(Status::Open.to_string(), "open");
    }

    #[test]
    fn test_status_from_str() {
        assert_eq!("in_progress".parse::<Status>().unwrap(), Status::InProgress);
        assert!("invalid".parse::<Status>().is_err());
    }

    #[test]
    fn test_summary_basic_fields() {
        let t = Task::new("ab12".into(), "Test task".into(), Priority::P2);
        let s = t.summary(None);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["id"], "ab12");
        assert_eq!(json["title"], "Test task");
        assert_eq!(json["status"], "open");
        assert_eq!(json["priority"], "P2");
        assert!(json.get("effective_priority").is_none());
    }

    #[test]
    fn test_summary_with_effective_priority() {
        let t = Task::new("cd34".into(), "High eff".into(), Priority::P3);
        let s = t.summary(Some(&Priority::P1));
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["effective_priority"], "P1");
    }

    #[test]
    fn test_summary_effective_not_set_when_same() {
        let t = Task::new("ef56".into(), "Same prio".into(), Priority::P1);
        let s = t.summary(Some(&Priority::P1));
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("effective_priority").is_none());
    }

    #[test]
    fn test_detail_has_all_fields() {
        let mut t = Task::new("gh78".into(), "Detail test".into(), Priority::P1);
        t.body = "Some body".into();
        t.depends_on = vec!["ab12".into()];
        t.parent = Some("zz99".into());
        t.assignee = "alice".into();

        let d = t.detail(Some(&Priority::P0));
        let json = serde_json::to_value(&d).unwrap();
        // Flattened summary fields
        assert_eq!(json["id"], "gh78");
        assert_eq!(json["title"], "Detail test");
        assert_eq!(json["priority"], "P1");
        assert_eq!(json["effective_priority"], "P0");
        // Detail-only fields
        assert_eq!(json["body"], "Some body");
        assert_eq!(json["depends_on"], serde_json::json!(["ab12"]));
        assert_eq!(json["parent"], "zz99");
        assert_eq!(json["assignee"], "alice");
        assert!(json.get("created").is_some());
        assert!(json.get("updated").is_some());
    }

    #[test]
    fn test_is_active() {
        let mut t = Task::new("a1".into(), "T".into(), Priority::P2);
        assert!(is_active(&t));

        t.status = Status::InProgress;
        assert!(is_active(&t));

        t.status = Status::Done;
        assert!(!is_active(&t));

        t.status = Status::Cancelled;
        assert!(!is_active(&t));

        t.status = Status::Blocked;
        assert!(is_active(&t));
    }

    #[test]
    fn test_matches_tag() {
        let mut t = Task::new("b2".into(), "T".into(), Priority::P2);
        t.tags = vec!["backend".into(), "api".into()];

        assert!(matches_tag(&t, None));
        assert!(matches_tag(&t, Some("backend")));
        assert!(matches_tag(&t, Some("api")));
        assert!(!matches_tag(&t, Some("frontend")));
    }

    #[test]
    fn test_sort_by_priority_owned() {
        let mut t1 = Task::new("a1".into(), "Low".into(), Priority::P3);
        t1.created = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut t2 = Task::new("b2".into(), "High".into(), Priority::P0);
        t2.created = chrono::DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut t3 = Task::new("c3".into(), "Also low, older".into(), Priority::P3);
        t3.created = chrono::DateTime::parse_from_rfc3339("2025-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let mut tasks = vec![t1, t3, t2];
        sort_by_priority_owned(&mut tasks);

        assert_eq!(tasks[0].id, "b2"); // P0
        assert_eq!(tasks[1].id, "c3"); // P3, older
        assert_eq!(tasks[2].id, "a1"); // P3, newer
    }

    // ── Frontmatter parsing edge-case tests ─────────────────────────

    #[test]
    fn test_parse_crlf_line_endings() {
        let content = "---\r\nid: cr01\r\ntitle: CRLF task\r\nstatus: open\r\npriority: P2\r\ncreated: 2026-03-15T10:30:00Z\r\nupdated: 2026-03-15T10:30:00Z\r\n---\r\n\r\nBody with CRLF.\r\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "cr01");
        assert_eq!(task.title, "CRLF task");
        assert_eq!(task.body, "Body with CRLF.\n");
    }

    #[test]
    fn test_parse_bom_prefix() {
        let content = "\u{feff}---\nid: bom1\ntitle: BOM task\nstatus: open\npriority: P1\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "bom1");
        assert_eq!(task.body, "");
    }

    #[test]
    fn test_parse_bom_with_crlf() {
        let content = "\u{feff}---\r\nid: bc01\r\ntitle: BOM+CRLF\r\nstatus: done\r\npriority: P0\r\ncreated: 2026-03-15T10:30:00Z\r\nupdated: 2026-03-15T10:30:00Z\r\n---\r\n\r\nBoth BOM and CRLF.\r\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "bc01");
        assert_eq!(task.title, "BOM+CRLF");
        assert_eq!(task.body, "Both BOM and CRLF.\n");
    }

    #[test]
    fn test_parse_missing_opening_delimiter() {
        let content = "id: broken\ntitle: No opening\n---\n";
        let err = parse_task(content).unwrap_err();
        assert!(err.to_string().contains("opening ---"));
    }

    #[test]
    fn test_parse_missing_closing_delimiter() {
        let content = "---\nid: broken\ntitle: No closing\n";
        let err = parse_task(content).unwrap_err();
        assert!(err.to_string().contains("closing ---"));
    }

    #[test]
    fn test_parse_no_body_no_trailing_newline() {
        let content = "---\nid: nb01\ntitle: Minimal\nstatus: open\npriority: P3\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "nb01");
        assert_eq!(task.body, "");
    }

    #[test]
    fn test_parse_body_preserves_internal_triple_dashes() {
        let content = "---\nid: td01\ntitle: Triple dashes in body\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n\nSome text\n--- not a delimiter\nMore text\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "td01");
        assert!(task.body.contains("--- not a delimiter"));
        assert!(task.body.contains("More text"));
    }
}
