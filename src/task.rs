use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Generate `Display` and `FromStr` for an enum whose serde string forms are
/// the single source of truth.  Each arm is `Variant => "string"`.
/// The error message prefix (e.g. `"invalid status"`) is passed as `$err_prefix`.
macro_rules! impl_str_enum {
    (
        $ty:ty,
        $err_prefix:literal,
        $( $variant:path => $s:literal ),+ $(,)?
    ) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let s = match self {
                    $( $variant => $s, )+
                };
                f.write_str(s)
            }
        }

        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                match s {
                    $( $s => Ok($variant), )+
                    _ => Err(format!("{}: {s}", $err_prefix)),
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Status {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl_str_enum!(
    Status,
    "invalid status",
    Status::Open       => "open",
    Status::InProgress => "in_progress",
    Status::Done       => "done",
    Status::Blocked    => "blocked",
    Status::Cancelled  => "cancelled",
);

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum TaskType {
    #[default]
    #[serde(rename = "task")]
    Task,
    #[serde(rename = "epic")]
    Epic,
}

impl TaskType {
    pub fn is_task(self) -> bool {
        self == TaskType::Task
    }

    pub fn is_epic(self) -> bool {
        self == TaskType::Epic
    }
}

impl_str_enum!(
    TaskType,
    "invalid task type",
    TaskType::Task => "task",
    TaskType::Epic => "epic",
);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum Priority {
    #[serde(rename = "P0")]
    P0,
    #[serde(rename = "P1")]
    P1,
    #[serde(rename = "P2")]
    P2,
    #[serde(rename = "P3")]
    P3,
}

impl_str_enum!(
    Priority,
    "invalid priority",
    Priority::P0 => "P0",
    Priority::P1 => "P1",
    Priority::P2 => "P2",
    Priority::P3 => "P3",
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    #[serde(deserialize_with = "lenient_string")]
    pub id: String,
    #[serde(deserialize_with = "lenient_string")]
    pub title: String,
    #[serde(default, rename = "type", skip_serializing_if = "is_default_task_type")]
    pub task_type: TaskType,
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

/// Accept any YAML scalar where a string is expected.
///
/// serde_yml's emitter quotes most ambiguous strings ("234", "true") but not
/// "nan", which YAML then resolves as a float — and hand-edited files may
/// contain unquoted numbers or booleans in string positions. Rather than
/// rejecting the whole task file, coerce the scalar back to its string form.
fn lenient_string<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    struct V;
    impl serde::de::Visitor<'_> for V {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or scalar")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<String, E> {
            // Match the lowercase forms YAML emitters use for these scalars.
            if v.is_nan() {
                Ok("nan".to_string())
            } else if v.is_infinite() {
                Ok(if v > 0.0 { "inf" } else { "-inf" }.to_string())
            } else {
                Ok(v.to_string())
            }
        }
    }
    d.deserialize_any(V)
}

fn is_default_task_type(t: &TaskType) -> bool {
    t.is_task()
}

impl Task {
    pub fn new(id: String, title: String, priority: Priority) -> Self {
        let now = Utc::now();
        Task {
            id,
            title,
            task_type: TaskType::Task,
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

    /// Compact projection: id, title, type, status, priority, tags, and optional effective priority.
    pub fn summary(&self, effective_priority: Option<&Priority>) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            task_type: self.task_type,
            status: self.status,
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

    /// Epic projection: id, title, status, priority, tags, and progress.
    pub fn epic_summary(
        &self,
        progress: crate::service::EpicProgress,
    ) -> crate::service::EpicSummary {
        crate::service::EpicSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            status: self.status,
            priority: self.priority,
            tags: self.tags.clone(),
            progress,
        }
    }
}

/// Compact task projection used by list/ready/create/update responses.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    #[serde(rename = "type", skip_serializing_if = "is_default_task_type")]
    pub task_type: TaskType,
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

/// Unambiguous character set for ID generation.
/// Excludes easily confused characters: 0/o, 1/l/i.
const ID_CHARSET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// An ID is YAML-safe when its unquoted occurrence still parses as a string.
/// "nan" parses as a float, which serde_yml's emitter does not protect against
/// with quotes — such an ID would corrupt the frontmatter on the first save.
fn yaml_safe_id(id: &str) -> bool {
    matches!(
        serde_yml::from_str::<serde_yml::Value>(id),
        Ok(serde_yml::Value::String(_))
    )
}

/// Generate a short alphanumeric ID, retrying on collision or YAML-unsafe IDs.
pub fn generate_id(existing: &HashSet<String>, length: usize) -> String {
    let mut rng = rand::rng();
    loop {
        let id: String = (0..length)
            .map(|_| ID_CHARSET[rng.random_range(0..ID_CHARSET.len())] as char)
            .collect();
        if !existing.contains(&id) && yaml_safe_id(&id) {
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

    let mut task: Task = serde_yml::from_str(&yaml_str)?;
    task.body = body;
    Ok(task)
}

/// Render a task back to markdown with YAML frontmatter.
pub fn render_task(task: &Task) -> String {
    let mut yaml = serde_yml::to_string(task).expect("task serialization should not fail");
    // Some YAML serializers omit the trailing newline; the closing
    // delimiter must start on its own line.
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
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
        let allowed: Vec<char> = std::str::from_utf8(ID_CHARSET).unwrap().chars().collect();
        assert!(id.chars().all(|c| allowed.contains(&c)));
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
    fn test_generate_id_excludes_ambiguous_chars() {
        let existing = HashSet::new();
        let ambiguous = ['0', 'o', '1', 'l', 'i'];
        for _ in 0..100 {
            let id = generate_id(&existing, 6);
            assert!(!id.chars().any(|c| ambiguous.contains(&c)));
        }
    }

    #[test]
    fn test_yaml_safe_id_rejects_nan() {
        assert!(!yaml_safe_id("nan"));
        assert!(yaml_safe_id("abc"));
        assert!(yaml_safe_id("a2b"));
    }

    #[test]
    fn test_generate_id_never_yaml_ambiguous() {
        // With length 3 the generator could produce "nan" — ensure it is skipped.
        let existing = HashSet::new();
        for _ in 0..200 {
            let id = generate_id(&existing, 3);
            assert!(yaml_safe_id(&id), "generated YAML-unsafe id {id}");
        }
    }

    #[test]
    fn test_parse_unquoted_nan_id_and_title() {
        // serde_yml's emitter writes `nan` unquoted; the parser then sees a
        // float. The lenient deserializer must coerce it back to a string.
        let content = "---\nid: nan\ntitle: nan\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "nan");
        assert_eq!(task.title, "nan");
    }

    #[test]
    fn test_parse_unquoted_numeric_title() {
        // Hand-edited files may leave numbers unquoted in string positions.
        let content = "---\nid: ab2\ntitle: 42\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.title, "42");
    }

    #[test]
    fn test_nan_task_roundtrips() {
        let mut t = Task::new("nan".into(), "nan".into(), Priority::P2);
        t.body = "body".into();
        let rendered = render_task(&t);
        let parsed = parse_task(&rendered).unwrap();
        assert_eq!(parsed.id, "nan");
        assert_eq!(parsed.title, "nan");
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

    // ── TaskType tests ──────────────────────────────────────────────

    #[test]
    fn test_task_type_default() {
        assert_eq!(TaskType::default(), TaskType::Task);
    }

    #[test]
    fn test_task_type_display() {
        assert_eq!(TaskType::Task.to_string(), "task");
        assert_eq!(TaskType::Epic.to_string(), "epic");
    }

    #[test]
    fn test_task_type_from_str() {
        assert_eq!("task".parse::<TaskType>().unwrap(), TaskType::Task);
        assert_eq!("epic".parse::<TaskType>().unwrap(), TaskType::Epic);
        assert!("invalid".parse::<TaskType>().is_err());
    }

    #[test]
    fn test_task_new_defaults_to_task_type() {
        let t = Task::new("ab12".into(), "Test".into(), Priority::P1);
        assert_eq!(t.task_type, TaskType::Task);
    }

    #[test]
    fn test_task_type_not_serialized_when_task() {
        let t = Task::new("ab12".into(), "Normal task".into(), Priority::P1);
        let rendered = render_task(&t);
        assert!(!rendered.contains("type:"));
    }

    #[test]
    fn test_task_type_serialized_when_epic() {
        let mut t = Task::new("ab12".into(), "My epic".into(), Priority::P1);
        t.task_type = TaskType::Epic;
        let rendered = render_task(&t);
        assert!(rendered.contains("type: epic"));
    }

    #[test]
    fn test_parse_task_without_type_defaults_to_task() {
        let content = "---\nid: nt01\ntitle: No type\nstatus: open\npriority: P1\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.task_type, TaskType::Task);
    }

    #[test]
    fn test_parse_task_with_epic_type() {
        let content = "---\nid: ep01\ntitle: My epic\ntype: epic\nstatus: open\npriority: P1\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.task_type, TaskType::Epic);
    }

    #[test]
    fn test_epic_roundtrip() {
        let mut t = Task::new("ep02".into(), "Epic roundtrip".into(), Priority::P0);
        t.task_type = TaskType::Epic;
        let rendered = render_task(&t);
        let parsed = parse_task(&rendered).unwrap();
        assert_eq!(parsed.task_type, TaskType::Epic);
        assert_eq!(parsed.id, "ep02");
    }

    #[test]
    fn test_summary_includes_type_for_epic() {
        let mut t = Task::new("ep03".into(), "Epic sum".into(), Priority::P1);
        t.task_type = TaskType::Epic;
        let s = t.summary(None);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["type"], "epic");
    }

    #[test]
    fn test_summary_omits_type_for_task() {
        let t = Task::new("tk01".into(), "Task sum".into(), Priority::P1);
        let s = t.summary(None);
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("type").is_none());
    }

    /// Verify that Display, FromStr, and serde all agree on the exact string
    /// forms for every variant — one source of truth.
    #[test]
    fn test_enum_string_roundtrip() {
        // Status
        let status_cases: &[(Status, &str)] = &[
            (Status::Open, "open"),
            (Status::InProgress, "in_progress"),
            (Status::Done, "done"),
            (Status::Blocked, "blocked"),
            (Status::Cancelled, "cancelled"),
        ];
        for (variant, s) in status_cases {
            // Display matches
            assert_eq!(variant.to_string(), *s, "Status Display mismatch");
            // FromStr round-trips
            assert_eq!(
                &s.parse::<Status>().unwrap(),
                variant,
                "Status FromStr mismatch"
            );
            // serde JSON matches
            let json = serde_json::to_value(variant).unwrap();
            assert_eq!(json.as_str().unwrap(), *s, "Status serde mismatch");
            let de: Status = serde_json::from_value(json).unwrap();
            assert_eq!(&de, variant, "Status serde round-trip mismatch");
        }

        // Priority
        let priority_cases: &[(Priority, &str)] = &[
            (Priority::P0, "P0"),
            (Priority::P1, "P1"),
            (Priority::P2, "P2"),
            (Priority::P3, "P3"),
        ];
        for (variant, s) in priority_cases {
            assert_eq!(variant.to_string(), *s, "Priority Display mismatch");
            assert_eq!(
                &s.parse::<Priority>().unwrap(),
                variant,
                "Priority FromStr mismatch"
            );
            let json = serde_json::to_value(variant).unwrap();
            assert_eq!(json.as_str().unwrap(), *s, "Priority serde mismatch");
            let de: Priority = serde_json::from_value(json).unwrap();
            assert_eq!(&de, variant, "Priority serde round-trip mismatch");
        }

        // TaskType
        let type_cases: &[(TaskType, &str)] = &[(TaskType::Task, "task"), (TaskType::Epic, "epic")];
        for (variant, s) in type_cases {
            assert_eq!(variant.to_string(), *s, "TaskType Display mismatch");
            assert_eq!(
                &s.parse::<TaskType>().unwrap(),
                variant,
                "TaskType FromStr mismatch"
            );
            let json = serde_json::to_value(variant).unwrap();
            assert_eq!(json.as_str().unwrap(), *s, "TaskType serde mismatch");
            let de: TaskType = serde_json::from_value(json).unwrap();
            assert_eq!(&de, variant, "TaskType serde round-trip mismatch");
        }
    }
}
