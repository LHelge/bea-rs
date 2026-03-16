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
pub fn parse_task(content: &str) -> Result<Task> {
    let content = content.trim_start_matches('\u{feff}'); // strip BOM
    if !content.starts_with("---") {
        return Err(Error::InvalidFrontmatter {
            path: "".into(),
            reason: "missing opening --- delimiter".into(),
        });
    }

    let after_first = &content[3..];
    let end = after_first
        .find("\n---")
        .ok_or_else(|| Error::InvalidFrontmatter {
            path: "".into(),
            reason: "missing closing --- delimiter".into(),
        })?;

    let yaml_str = &after_first[..end];
    let body_start = end + 4; // skip \n---
    let body = if body_start < after_first.len() {
        let rest = &after_first[body_start..];
        rest.trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    let mut task: Task = serde_yaml::from_str(yaml_str)?;
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
}
