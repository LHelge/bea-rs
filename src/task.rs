use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::config::Config;
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

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Suggested but not yet accepted into the backlog. Never ready.
    #[serde(rename = "proposed")]
    Proposed,
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "in_progress")]
    InProgress,
    /// Work is finished and waiting for someone else to look at it.
    #[serde(rename = "review")]
    Review,
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
    Status::Proposed   => "proposed",
    Status::Open       => "open",
    Status::InProgress => "in_progress",
    Status::Review     => "review",
    Status::Done       => "done",
    Status::Blocked    => "blocked",
    Status::Cancelled  => "cancelled",
);

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    /// How many attempts have been started on this task. A missing field means
    /// zero — the counter only appears once a task has actually been started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    /// The `parent:` key of a file written before hierarchical IDs, read but
    /// never written back.
    ///
    /// A task's parent is derived from its ID (see [`Task::parent`]); this
    /// value has no effect on it. It is kept only so that a legacy store can
    /// be recognised and converted to hierarchical IDs.
    #[serde(default, rename = "parent", skip_serializing)]
    pub legacy_parent: Option<String>,
    #[serde(skip)]
    pub body: String,
}

/// Accept any YAML scalar where a string is expected.
///
/// Files written by older versions of bears may hold `nan` or `yes` unquoted,
/// and hand-edited files may leave numbers or booleans unquoted in string
/// positions. Rather than rejecting the whole task file, keep the scalar as
/// text. The YAML deserializer is target-typed, so asking for a string yields
/// the scalar exactly as written (`1.50` stays `1.50`); the bool and number
/// arms only matter for a deserializer that resolves scalar types itself.
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
    d.deserialize_string(V)
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
            assignee: String::new(),
            attempts: None,
            legacy_parent: None,
            body: String::new(),
        }
    }

    /// The ID of this task's parent epic, derived from its ID: `a1b` for
    /// `a1b.c2`, `None` for a root task.
    pub fn parent(&self) -> Option<&str> {
        parent_of(&self.id)
    }

    /// Whether this task is a direct child of `parent_id`.
    pub fn is_child_of(&self, parent_id: &str) -> bool {
        self.parent() == Some(parent_id)
    }

    /// Attempts started on this task, treating a missing counter as zero.
    pub fn attempt_count(&self) -> u32 {
        self.attempts.unwrap_or(0)
    }

    /// Compact projection: id, title, type, status, priority, tags, assignee, attempts, and optional effective priority.
    pub fn summary(&self, effective_priority: Option<&Priority>) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            task_type: self.task_type,
            status: self.status,
            priority: self.priority,
            tags: self.tags.clone(),
            assignee: self.assignee.clone(),
            attempts: self.attempts.filter(|n| *n > 0),
            effective_priority: effective_priority
                .filter(|ep| *ep < &self.priority)
                .copied(),
        }
    }

    /// Full projection: all summary fields plus body, deps, parent, timestamps.
    pub fn detail(&self, effective_priority: Option<&Priority>) -> TaskDetail {
        TaskDetail {
            summary: self.summary(effective_priority),
            body: self.body.clone(),
            depends_on: self.depends_on.clone(),
            parent: self.parent().map(String::from),
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
    #[serde(skip_serializing_if = "String::is_empty")]
    pub assignee: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
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

/// An ID is YAML-safe when its unquoted occurrence reads as a string in any
/// YAML reader, not as a number, boolean or null ("nan", "234", "yes"). YAML
/// 1.1 and 1.2 resolvers disagree on some of these, and older versions of
/// bears wrote `nan` unquoted, so the check is deliberately conservative and
/// independent of the YAML crate in use.
fn yaml_safe_id(id: &str) -> bool {
    const NON_STRING_WORDS: &[&str] = &[
        "~", "null", "true", "false", "yes", "no", "y", "n", "on", "off",
    ];
    // Rust's float parser accepts every numeric form the ID charset can
    // spell: integers, exponents ("2e5"), "nan", "inf" and "infinity".
    id.parse::<f64>().is_err() && !NON_STRING_WORDS.contains(&id.to_ascii_lowercase().as_str())
}

/// Serializer settings that give task frontmatter its on-disk shape: block
/// sequences indented under their key, and long titles kept on one line
/// instead of folded into a block scalar.
fn yaml_options() -> serde_saphyr::SerializerOptions {
    let mut options = serde_saphyr::SerializerOptions::default();
    options.compact_list_indent = false;
    options.prefer_block_scalars = false;
    options
}

/// Deserialize YAML, reporting errors on one line with their position (no
/// source snippet), so they fit in a warning or an MCP error message.
pub(crate) fn from_yaml<T: serde::de::DeserializeOwned>(yaml: &str) -> Result<T> {
    let mut options = serde_saphyr::Options::default();
    options.with_snippet = false;
    Ok(serde_saphyr::from_str_with_options(yaml, options)?)
}

/// Serialize a value as YAML in the shape bears writes to disk.
pub(crate) fn to_yaml<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_saphyr::to_string_with_options(value, yaml_options())?)
}

/// A random ID segment of `length` characters drawn from [`ID_CHARSET`].
fn random_segment(rng: &mut impl rand::Rng, length: usize) -> String {
    (0..length)
        .map(|_| ID_CHARSET[rng.random_range(0..ID_CHARSET.len())] as char)
        .collect()
}

/// Generate a short alphanumeric ID, retrying on collision or YAML-unsafe IDs.
pub fn generate_id(existing: &HashSet<String>, length: usize) -> String {
    let mut rng = rand::rng();
    loop {
        let id = random_segment(&mut rng, length);
        if !existing.contains(&id) && yaml_safe_id(&id) {
            return id;
        }
    }
}

// ---------------------------------------------------------------------------
// Hierarchical IDs
//
// An ID encodes its parent chain: `a1b` is a root, `a1b.c2` its child and
// `a1b.c2.d7` a grandchild. Every segment is lowercase alphanumeric. The ID
// functions below work on plain `&str` so they apply equally to `Task::id`,
// `depends_on` entries and IDs read from filenames.
// ---------------------------------------------------------------------------

/// Separator between the segments of a hierarchical ID.
pub const ID_SEPARATOR: char = '.';

/// Fraction of a parent's sibling space (at the current segment length) that
/// may be used before new children get a segment one character longer.
pub const CHILD_SPACE_FILL_LIMIT: f64 = 0.25;

/// Check that `id` is a well-formed task ID: one or more non-empty segments
/// of lowercase ASCII letters and digits, separated by [`ID_SEPARATOR`].
///
/// This accepts every generated ID, flat or dotted, and any hand-made ID of
/// lowercase letters and digits, not only those drawn from the generator's
/// charset. Earlier versions never validated IDs, so a hand-edited file may
/// carry one this rejects (`Foo_1`); loaders must warn about such a task
/// rather than silently drop it.
pub fn validate_id(id: &str) -> Result<()> {
    let invalid = |reason: &str| Error::InvalidId {
        id: id.into(),
        reason: reason.into(),
    };
    if id.is_empty() {
        return Err(invalid("it is empty"));
    }
    for segment in id.split(ID_SEPARATOR) {
        if segment.is_empty() {
            return Err(invalid("it has an empty segment"));
        }
        if !segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(invalid(
                "segments may only contain lowercase letters and digits",
            ));
        }
    }
    Ok(())
}

/// Whether `id` is a well-formed task ID; see [`validate_id`].
pub fn is_valid_id(id: &str) -> bool {
    validate_id(id).is_ok()
}

/// The segments of `id`, root first: `a1b.c2` yields `a1b`, `c2`.
pub fn id_segments(id: &str) -> std::str::Split<'_, char> {
    id.split(ID_SEPARATOR)
}

/// How many ancestors `id` has: 0 for a root ID, 1 for its child, and so on.
pub fn id_depth(id: &str) -> usize {
    id.matches(ID_SEPARATOR).count()
}

/// The ID of the parent `id` encodes, or `None` for a root ID.
pub fn parent_of(id: &str) -> Option<&str> {
    id.rsplit_once(ID_SEPARATOR).map(|(parent, _)| parent)
}

/// The root segment of `id`: `a1b` for `a1b.c2.d7`.
pub fn root_of(id: &str) -> &str {
    id.split_once(ID_SEPARATOR).map_or(id, |(root, _)| root)
}

/// The last segment of `id`: `d7` for `a1b.c2.d7`, the whole ID for a root.
pub fn leaf_segment(id: &str) -> &str {
    id.rsplit_once(ID_SEPARATOR).map_or(id, |(_, leaf)| leaf)
}

/// Whether `id` lies strictly below `ancestor` in the ID hierarchy.
///
/// `a1b.c2` and `a1b.c2.d7` are descendants of `a1b`; `a1b` itself is not,
/// and neither is `a1bx.c2` (segments compare whole, not by prefix).
pub fn is_descendant(id: &str, ancestor: &str) -> bool {
    id.strip_prefix(ancestor)
        .is_some_and(|rest| rest.starts_with(ID_SEPARATOR) && rest.len() > 1)
}

/// The IDs in `ids` whose parent is exactly `parent`.
fn direct_children<'a>(
    parent: &'a str,
    ids: &'a HashSet<String>,
) -> impl Iterator<Item = &'a String> + 'a {
    ids.iter().filter(move |id| parent_of(id) == Some(parent))
}

/// The segment length a new child of `parent` gets.
///
/// Starts at `min_length` and grows by one while the children of `parent`
/// whose last segment has the current length fill more than
/// [`CHILD_SPACE_FILL_LIMIT`] of the space of segments of that length.
/// Existing children keep their length, so a parent can have children of
/// several lengths.
pub fn child_segment_length(parent: &str, existing: &HashSet<String>, min_length: usize) -> usize {
    let mut counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for id in direct_children(parent, existing) {
        *counts.entry(leaf_segment(id).len()).or_default() += 1;
    }
    let mut length = min_length.max(1);
    loop {
        let space = (ID_CHARSET.len() as f64).powi(length as i32);
        let used = counts.get(&length).copied().unwrap_or(0) as f64;
        if used <= space * CHILD_SPACE_FILL_LIMIT {
            return length;
        }
        length += 1;
    }
}

/// Generate a new child ID under `parent`: `parent` plus a random segment.
///
/// `existing` is the collision pool, and should hold every known ID, active
/// and archived; the siblings that decide the segment length (see
/// [`child_segment_length`]) are read from it. The segment starts at the
/// configured `child-id-length`.
pub fn generate_child_id(parent: &str, existing: &HashSet<String>, config: &Config) -> String {
    let length = child_segment_length(parent, existing, config.child_id_length as usize);
    let mut rng = rand::rng();
    loop {
        let id = format!("{parent}{ID_SEPARATOR}{}", random_segment(&mut rng, length));
        if !existing.contains(&id) && yaml_safe_id(&id) {
            return id;
        }
    }
}

/// The task ID a task filename (`{id}-{slug}.md`) starts with.
///
/// Returns `None` when `name` is not a `.md` file. Dotted IDs are read
/// whole, since `-` never occurs inside an ID.
pub fn id_from_filename(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".md")?;
    stem.split('-').next().filter(|id| !id.is_empty())
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

    let mut task: Task = from_yaml(&yaml_str)?;
    task.body = body;
    Ok(task)
}

/// Render a task back to markdown with YAML frontmatter.
pub fn render_task(task: &Task) -> String {
    let mut yaml = to_yaml(task).expect("task serialization should not fail");
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
    fn test_validate_id_accepts_flat_and_dotted() {
        for id in [
            "a1b",
            "ab12",
            "dup1",
            "8ph",
            "a1b.c2",
            "a1b.c2.d7",
            "a1b.c23",
        ] {
            validate_id(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(is_valid_id(id));
        }
    }

    #[test]
    fn test_validate_id_rejects_malformed() {
        for id in [
            "", ".", "a1b.", ".a1b", "a1b..c2", "A1b", "a1b.C2", "a-b", "a1b.c 2", "a1b/c2", "ä1b",
        ] {
            assert!(!is_valid_id(id), "{id:?} should be invalid");
        }
        let err = validate_id("a1b..c2").unwrap_err();
        assert!(matches!(err, Error::InvalidId { .. }), "{err:?}");
        assert!(err.to_string().contains("a1b..c2"), "{err}");
    }

    #[test]
    fn test_id_segments_and_depth() {
        assert_eq!(id_segments("a1b").collect::<Vec<_>>(), ["a1b"]);
        assert_eq!(
            id_segments("a1b.c2.d7").collect::<Vec<_>>(),
            ["a1b", "c2", "d7"]
        );
        assert_eq!(id_depth("a1b"), 0);
        assert_eq!(id_depth("a1b.c2"), 1);
        assert_eq!(id_depth("a1b.c2.d7"), 2);
    }

    #[test]
    fn test_parent_root_and_leaf() {
        assert_eq!(parent_of("a1b"), None);
        assert_eq!(parent_of("a1b.c2"), Some("a1b"));
        assert_eq!(parent_of("a1b.c2.d7"), Some("a1b.c2"));
        assert_eq!(root_of("a1b"), "a1b");
        assert_eq!(root_of("a1b.c2.d7"), "a1b");
        assert_eq!(leaf_segment("a1b"), "a1b");
        assert_eq!(leaf_segment("a1b.c2.d7"), "d7");
    }

    #[test]
    fn test_is_descendant() {
        assert!(is_descendant("a1b.c2", "a1b"));
        assert!(is_descendant("a1b.c2.d7", "a1b"));
        assert!(is_descendant("a1b.c2.d7", "a1b.c2"));
        assert!(!is_descendant("a1b", "a1b"), "not its own descendant");
        assert!(
            !is_descendant("a1b", "a1b.c2"),
            "ancestor is not a descendant"
        );
        assert!(!is_descendant("a1bx.c2", "a1b"), "segments compare whole");
        assert!(!is_descendant("a1b.", "a1b"));
        assert!(!is_descendant("x9k.c2", "a1b"));
    }

    #[test]
    fn test_id_from_filename() {
        assert_eq!(id_from_filename("a1b-fix-login.md"), Some("a1b"));
        assert_eq!(id_from_filename("a1b.c2-fix-login.md"), Some("a1b.c2"));
        assert_eq!(id_from_filename("a1b.c2.d7-x.md"), Some("a1b.c2.d7"));
        assert_eq!(id_from_filename("a1b.c2-x.txt"), None);
        assert_eq!(id_from_filename("-x.md"), None);
    }

    fn child_config(child_id_length: u8) -> Config {
        Config {
            child_id_length,
            ..Config::default()
        }
    }

    #[test]
    fn test_generate_child_id_shape() {
        let existing: HashSet<String> = ["a1b".to_string()].into();
        for _ in 0..100 {
            let id = generate_child_id("a1b", &existing, &Config::default());
            assert_eq!(parent_of(&id), Some("a1b"));
            assert_eq!(leaf_segment(&id).len(), 2, "{id}");
            assert!(is_valid_id(&id), "{id}");
            assert!(yaml_safe_id(&id), "{id}");
            let allowed = std::str::from_utf8(ID_CHARSET).unwrap();
            assert!(leaf_segment(&id).chars().all(|c| allowed.contains(c)));
        }
        let grandchild = generate_child_id("a1b.c2", &existing, &Config::default());
        assert_eq!(parent_of(&grandchild), Some("a1b.c2"));
        assert_eq!(
            leaf_segment(&grandchild).len(),
            2,
            "same length at every depth"
        );
    }

    #[test]
    fn test_generate_child_id_uses_configured_length() {
        let existing = HashSet::new();
        let id = generate_child_id("a1b", &existing, &child_config(4));
        assert_eq!(leaf_segment(&id).len(), 4, "{id}");
    }

    /// Every two-char child of `parent`, in charset order.
    fn all_two_char_children(parent: &str) -> Vec<String> {
        let chars = std::str::from_utf8(ID_CHARSET).unwrap();
        chars
            .chars()
            .flat_map(|a| chars.chars().map(move |b| format!("{parent}.{a}{b}")))
            .collect()
    }

    #[test]
    fn test_child_segment_growth_threshold() {
        // 31^2 = 961 two-char segments; 25% of that is 240.25.
        let children = all_two_char_children("a1b");
        let mut existing: HashSet<String> = children[..240].iter().cloned().collect();
        assert_eq!(child_segment_length("a1b", &existing, 2), 2, "at 25%");
        existing.insert(children[240].clone());
        assert_eq!(child_segment_length("a1b", &existing, 2), 3, "past 25%");
        let id = generate_child_id("a1b", &existing, &Config::default());
        assert_eq!(leaf_segment(&id).len(), 3, "{id}");
    }

    #[test]
    fn test_child_segment_growth_counts_only_direct_children() {
        let children = all_two_char_children("a1b");
        let mut existing: HashSet<String> = children[..241].iter().cloned().collect();
        // A full sibling space under a different parent does not matter...
        assert_eq!(child_segment_length("x9k", &existing, 2), 2);
        // ...nor do grandchildren, or children of a longer length.
        let grandchildren: HashSet<String> = all_two_char_children("x9k.c2").into_iter().collect();
        existing.extend(grandchildren);
        assert_eq!(child_segment_length("x9k", &existing, 2), 2);
        existing.insert("x9k.abc".into());
        assert_eq!(child_segment_length("x9k", &existing, 2), 2);
        // A prefix-sharing parent is a different parent.
        assert_eq!(child_segment_length("a1", &existing, 2), 2);
    }

    #[test]
    fn test_child_segment_growth_keeps_existing_lengths() {
        // Past the limit, children of both lengths coexist: the two-char
        // siblings keep counting against the two-char space, so every new
        // child keeps the longer segment instead of dropping back to two
        // chars, and none of them collides with a sibling of either length.
        let children = all_two_char_children("a1b");
        let mut existing: HashSet<String> = children[..241].iter().cloned().collect();
        for _ in 0..100 {
            let id = generate_child_id("a1b", &existing, &Config::default());
            assert_eq!(leaf_segment(&id).len(), 3, "{id}");
            assert!(existing.insert(id.clone()), "collided with {id}");
        }
        let lengths: HashSet<usize> = direct_children("a1b", &existing)
            .map(|id| leaf_segment(id).len())
            .collect();
        assert_eq!(lengths, [2, 3].into(), "both lengths remain");
    }

    #[test]
    fn test_generate_child_id_avoids_collisions() {
        // Start with 100 of the 961 two-char slots taken and generate up to
        // the growth limit: every new child is two chars and none collides.
        let children = all_two_char_children("a1b");
        let mut existing: HashSet<String> = children[..100].iter().cloned().collect();
        while existing.len() < 240 {
            let id = generate_child_id("a1b", &existing, &Config::default());
            assert_eq!(leaf_segment(&id).len(), 2, "{id}");
            assert!(existing.insert(id.clone()), "collided with {id}");
        }
    }

    #[test]
    fn test_dotted_id_task_roundtrips() {
        let task = Task::new("a1b.c2".into(), "Child".into(), Priority::P2);
        assert_eq!(filename(&task), "a1b.c2-child.md");
        let parsed = parse_task(&render_task(&task)).unwrap();
        assert_eq!(parsed.id, "a1b.c2");
        let numeric = Task::new("23.45".into(), "Numeric".into(), Priority::P2);
        assert_eq!(parse_task(&render_task(&numeric)).unwrap().id, "23.45");
    }

    #[test]
    fn test_generate_child_id_skips_yaml_unsafe_ids() {
        // A numeric root plus an all-digit child reads as a float ("23.45").
        let existing = HashSet::new();
        for _ in 0..200 {
            let id = generate_child_id("23", &existing, &Config::default());
            assert!(yaml_safe_id(&id), "generated YAML-unsafe id {id}");
        }
    }

    #[test]
    fn test_yaml_safe_id_rejects_nan() {
        assert!(!yaml_safe_id("nan"));
        assert!(yaml_safe_id("abc"));
        assert!(yaml_safe_id("a2b"));
    }

    #[test]
    fn test_yaml_safe_id_rejects_other_non_strings() {
        for id in [
            "234", "2e5", "inf", "infinity", "yes", "true", "off", "null", "NaN",
        ] {
            assert!(!yaml_safe_id(id), "{id} should be unsafe");
        }
        for id in ["8ph", "2j2", "yem", "nax", "e23"] {
            assert!(yaml_safe_id(id), "{id} should be safe");
        }
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
        // Older bears versions wrote `nan` unquoted, which YAML resolves as a
        // float. The lenient deserializer must keep it as a string.
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
        let mut t = Task::new("zz99.gh".into(), "Detail test".into(), Priority::P1);
        t.body = "Some body".into();
        t.depends_on = vec!["ab12".into()];
        t.assignee = "alice".into();

        let d = t.detail(Some(&Priority::P0));
        let json = serde_json::to_value(&d).unwrap();
        // Flattened summary fields
        assert_eq!(json["id"], "zz99.gh");
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

    #[test]
    fn test_attempts_missing_reads_as_zero() {
        let content = "---\nid: ab2\ntitle: No counter\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.attempts, None);
        assert_eq!(task.attempt_count(), 0);

        // An untouched task does not gain the field on write.
        let rendered = render_task(&task);
        assert!(!rendered.contains("attempts"), "{rendered}");
    }

    #[test]
    fn test_attempts_roundtrip() {
        let mut t = Task::new("ab2".into(), "Retried".into(), Priority::P2);
        t.attempts = Some(3);
        let rendered = render_task(&t);
        assert!(rendered.contains("attempts: 3"), "{rendered}");

        let parsed = parse_task(&rendered).unwrap();
        assert_eq!(parsed.attempt_count(), 3);
    }

    /// Compare every persisted field, since `Task` has no `PartialEq`.
    fn assert_same_task(a: &Task, b: &Task, context: &str) {
        assert_eq!(a.id, b.id, "{context}");
        assert_eq!(a.title, b.title, "{context}");
        assert_eq!(a.task_type, b.task_type, "{context}");
        assert_eq!(a.status, b.status, "{context}");
        assert_eq!(a.priority, b.priority, "{context}");
        assert_eq!(a.created, b.created, "{context}");
        assert_eq!(a.updated, b.updated, "{context}");
        assert_eq!(a.tags, b.tags, "{context}");
        assert_eq!(a.depends_on, b.depends_on, "{context}");
        assert_eq!(a.assignee, b.assignee, "{context}");
        assert_eq!(a.attempts, b.attempts, "{context}");
        assert_eq!(a.body, b.body, "{context}");
    }

    #[test]
    fn test_render_frontmatter_shape() {
        // The exact on-disk shape: field order, indented block lists, quoted
        // timestamps, and plain IDs even when they start with a digit.
        let content = "---
id: 8z2
title: \"Shape: quoting, lists and order\"
type: epic
status: in_progress
priority: P1
created: \"2026-03-15T10:30:00Z\"
updated: \"2026-03-15T10:30:00.123456789Z\"
tags:
  - backend
  - auth
depends_on:
  - 2j2
  - f4c
assignee: alice
attempts: 2
---

Body text.
";
        let task = parse_task(content).unwrap();
        assert_eq!(render_task(&task), content);
    }

    #[test]
    fn test_parent_is_derived_from_id() {
        let root = Task::new("a1b".into(), "Root".into(), Priority::P2);
        assert_eq!(root.parent(), None);
        let child = Task::new("a1b.c2".into(), "Child".into(), Priority::P2);
        assert_eq!(child.parent(), Some("a1b"));
        assert!(child.is_child_of("a1b"));
        let grandchild = Task::new("a1b.c2.d7".into(), "Grandchild".into(), Priority::P2);
        assert_eq!(grandchild.parent(), Some("a1b.c2"));
        assert!(grandchild.is_child_of("a1b.c2"));
        assert!(!grandchild.is_child_of("a1b"));
        assert!(!root.is_child_of("a1b"));
    }

    #[test]
    fn test_parent_is_not_written() {
        let t = Task::new("a1b.c2".into(), "Child".into(), Priority::P2);
        let rendered = render_task(&t);
        assert!(!rendered.contains("parent"), "{rendered}");
    }

    #[test]
    fn test_legacy_parent_is_read_but_not_written() {
        // Files written before hierarchical IDs carry a `parent:` key. It must
        // still load, stay visible for migration, and never act as the parent.
        let content = "---\nid: c3d\ntitle: Legacy child\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\nparent: 8ph\n---\n";
        let task = parse_task(content).unwrap();
        assert_eq!(task.legacy_parent.as_deref(), Some("8ph"));
        assert_eq!(task.parent(), None);
        assert_eq!(task.detail(None).parent, None);

        let rendered = render_task(&task);
        assert!(!rendered.contains("parent"), "{rendered}");
        assert_eq!(parse_task(&rendered).unwrap().legacy_parent, None);
    }

    #[test]
    fn test_render_long_title_stays_on_one_line() {
        let title =
            "A long title that runs well past the eighty characters where folding would start";
        let t = Task::new("ab2".into(), title.into(), Priority::P2);
        let rendered = render_task(&t);
        assert!(
            rendered.contains(&format!("title: {title}\n")),
            "{rendered}"
        );
    }

    #[test]
    fn test_render_quotes_yaml_ambiguous_words() {
        for word in ["yes", "no", "on", "off", "nan", "inf", "true", "null", "42"] {
            let t = Task::new("ab2".into(), word.into(), Priority::P2);
            let rendered = render_task(&t);
            assert!(
                rendered.contains(&format!("title: \"{word}\"\n")),
                "{rendered}"
            );
            assert_eq!(parse_task(&rendered).unwrap().title, word);
        }
    }

    #[test]
    fn test_parse_unquoted_scalars_keep_their_text() {
        // Older versions of bears wrote `yes` unquoted, and hand edits may
        // leave any scalar unquoted: string fields keep it exactly as written.
        for word in ["yes", "off", "true", "1.50", "0x1F", "2e5"] {
            let content = format!(
                "---\nid: ab2\ntitle: {word}\nstatus: open\npriority: P2\ncreated: 2026-03-15T10:30:00Z\nupdated: 2026-03-15T10:30:00Z\nassignee: {word}\n---\n"
            );
            let task = parse_task(&content).unwrap();
            assert_eq!(task.title, word);
            assert_eq!(task.assignee, word);
        }
    }

    #[test]
    fn test_parse_legacy_serde_yaml_shape() {
        // Archived tasks written by early versions: unquoted timestamps,
        // unindented lists, single-quoted titles.
        let content = "---
id: 2j2
title: 'TUI app shell: event loop and terminal setup'
status: done
priority: P1
created: 2026-03-16T21:49:49.049228511Z
updated: 2026-03-17T09:32:08.887318Z
tags:
- feature
- tui
depends_on:
- zdq
---

Body.
";
        let task = parse_task(content).unwrap();
        assert_eq!(task.id, "2j2");
        assert_eq!(task.title, "TUI app shell: event loop and terminal setup");
        assert_eq!(task.tags, vec!["feature", "tui"]);
        assert_eq!(task.depends_on, vec!["zdq"]);
        assert_eq!(
            task.updated
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            "2026-03-17T09:32:08.887318Z"
        );
        let reparsed = parse_task(&render_task(&task)).unwrap();
        assert_same_task(&reparsed, &task, "legacy round-trip");
    }

    #[test]
    fn test_repository_store_roundtrips() {
        // Every task this repository tracks, active and archived, must load
        // and survive a render/parse cycle unchanged, except for a legacy
        // `parent:` key, which is read but never written back. `.bears/` is
        // not part of the published crate, so the test has nothing to check
        // there.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".bears");
        if !root.is_dir() {
            return;
        }
        let mut checked = 0;
        for dir in [root.clone(), root.join("archive")] {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.extension().is_none_or(|ext| ext != "md") {
                    continue;
                }
                let context = path.display().to_string();
                let content = std::fs::read_to_string(&path).unwrap();
                let task = parse_task(&content).unwrap_or_else(|e| panic!("{context}: {e}"));
                let rendered = render_task(&task);
                let reparsed = parse_task(&rendered).unwrap_or_else(|e| panic!("{context}: {e}"));
                assert_same_task(&reparsed, &task, &context);
                assert!(
                    !rendered.lines().any(|l| l.starts_with("parent:")),
                    "{context}"
                );
                assert_eq!(reparsed.legacy_parent, None, "{context}");
                assert_eq!(render_task(&reparsed), rendered, "{context}");
                checked += 1;
            }
        }
        assert!(checked > 0, "no task files found under {}", root.display());
    }

    #[test]
    fn test_summary_hides_zero_attempts() {
        let mut t = Task::new("ab2".into(), "Task".into(), Priority::P2);
        let json = serde_json::to_value(t.summary(None)).unwrap();
        assert!(json.get("attempts").is_none(), "{json}");

        t.attempts = Some(2);
        let json = serde_json::to_value(t.summary(None)).unwrap();
        assert_eq!(json["attempts"], 2);
    }
}
