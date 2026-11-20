use std::path::PathBuf;

use crate::task::Status;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not initialized: no `.bears` directory found")]
    NotInitialized,

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("ambiguous prefix '{prefix}' matches multiple tasks: {matches}")]
    AmbiguousPrefix { prefix: String, matches: String },

    #[error("task {0} is not an epic")]
    NotAnEpic(String),

    #[error("parent {0} is not an epic — only epics can have child tasks")]
    ParentNotEpic(String),

    /// A task was asked to move under itself or one of its own descendants.
    #[error("cannot move task {id} under {parent}: {parent} is the task itself or lies below it")]
    ParentInSubtree { id: String, parent: String },

    /// A rename would give a task an ID that another task already has.
    #[error("task ID {0} is already taken")]
    IdTaken(String),

    #[error("adding dependency would create a cycle: {from} -> {to}")]
    CycleDetected { from: String, to: String },

    #[error("unknown dependency ID(s): {}", ids.join(", "))]
    UnknownDependency { ids: Vec<String> },

    /// A string is not a well-formed task ID.
    #[error("invalid task ID '{id}': {reason}")]
    InvalidId { id: String, reason: String },

    #[error("invalid config: {reason}")]
    InvalidConfig { reason: String },

    #[error("{0}")]
    InvalidUsage(String),

    #[error("invalid frontmatter in {path}: {reason}")]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error("editor failed: {reason}")]
    EditorFailed { reason: String },

    /// The task cannot be archived because active tasks depend on it.
    #[error("task {id} is not archivable — active dependents: {}", blockers.join(", "))]
    NotArchivable { id: String, blockers: Vec<String> },

    /// A workflow shortcut was used on a task whose status does not allow it.
    ///
    /// The workflow verbs (`release`, `review`, `reject`, `propose`, `accept`)
    /// each move a task along one specific edge; setting a status directly is
    /// always available as the escape hatch.
    #[error("cannot {action} task {id}: expected status {expected}, but it is {actual}")]
    InvalidStatus {
        id: String,
        action: &'static str,
        expected: Status,
        actual: Status,
    },

    /// A claim was refused because the task already belongs to someone else.
    ///
    /// Returned by the claiming primitives (`claim_task`, `assign_task`) after
    /// a fresh read from disk, so two workers racing for the same task cannot
    /// both walk away believing they hold it.
    #[error("task {id} is already claimed by '{assignee}'")]
    AlreadyClaimed { id: String, assignee: String },

    /// A fenced mutation was refused because the task's assignee no longer
    /// matches the token the caller claimed it with.
    ///
    /// This is the stale-writer guard: a worker whose task has been released,
    /// reassigned, or reaped gets this error instead of silently clobbering
    /// the new holder's state.
    #[error("fence violation on task {id}: expected assignee '{expected}', but it is '{actual}'")]
    FenceViolation {
        id: String,
        expected: String,
        actual: String,
    },

    /// The task is not found in the archive.
    #[error("task not found in archive: {0}")]
    NotArchived(String),

    /// The store was written before hierarchical IDs: some task files, active
    /// or archived, still carry a `parent:` key. Loading it is refused so
    /// that no write can drop the key and lose the epic link; the store has
    /// to be converted with [`store::migrate`](crate::store::migrate) first.
    #[error(
        "the task store predates hierarchical IDs: {} task file(s) carry a legacy `parent:` key{}",
        files.len(),
        files.first().map(|f| format!(", e.g. {}", f.display())).unwrap_or_default()
    )]
    LegacyStore { files: Vec<PathBuf> },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// YAML could not be parsed (task frontmatter or `.bears.yml`).
    ///
    /// Shown on one line, with its position, worded for whoever edits the
    /// file (see [`yaml_message`]) rather than in the YAML library's own
    /// words, which give advice to the Rust developer calling it. The
    /// library's error is kept but is deliberately not the `source()`, so
    /// nothing that walks the error chain brings that advice back.
    #[error("{}", yaml_message(.0))]
    Yaml(serde_saphyr::Error),

    /// A value could not be written as YAML.
    #[error(transparent)]
    YamlSerialize(#[from] serde_saphyr::SerializeError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Render a YAML parse error for whoever edits the file: one line, ending
/// with the position (`at line L, column C`), with no source snippet.
fn yaml_message(err: &serde_saphyr::Error) -> String {
    let mut options = serde_saphyr::RenderOptions::new(&FileMessages);
    options.snippets = serde_saphyr::SnippetMode::Off;
    err.render_with_options(options)
}

/// Words the YAML library's errors in terms of the file being edited.
///
/// The library's own messages can carry advice for the Rust developer calling
/// it (`use Option<String>`, `set DuplicateKeyPolicy in Options`) or name Rust
/// types (`invalid u32`). The cases a hand edit is likely to hit are reworded
/// here, by the error's variant and fields, not its message; the rest goes to the
/// library's user-facing formatter, which falls back to the library's own
/// message, so an error this does not know still says what went wrong.
struct FileMessages;

impl serde_saphyr::MessageFormatter for FileMessages {
    fn format_message<'a>(&self, err: &'a serde_saphyr::Error) -> std::borrow::Cow<'a, str> {
        use serde_saphyr::Error as E;
        use std::borrow::Cow;
        let fallback = || serde_saphyr::UserMessageFormatter.format_message(err);
        match err {
            E::NullIntoString { .. } | E::InvalidCharNull { .. } => {
                Cow::Borrowed("expected a value, found null")
            }
            E::DuplicateMappingKey { key: Some(key), .. } => {
                Cow::Owned(format!("duplicate key `{key}`"))
            }
            E::DuplicateMappingKey { key: None, .. } => Cow::Borrowed("duplicate key"),
            E::Unexpected { expected, .. } => match *expected {
                "sequence start" => Cow::Borrowed("expected a list"),
                "mapping start" => Cow::Borrowed("expected a mapping of keys to values"),
                "string scalar" => Cow::Borrowed("expected a single value, not a list or mapping"),
                _ => fallback(),
            },
            E::InvalidScalar { ty, .. } => match *ty {
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => {
                    Cow::Borrowed("expected a whole number of 0 or more")
                }
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => {
                    Cow::Borrowed("expected a whole number")
                }
                "f32" | "f64" => Cow::Borrowed("expected a number"),
                "bool" => Cow::Borrowed("expected true or false"),
                _ => fallback(),
            },
            E::SerdeUnknownVariant {
                variant, expected, ..
            } => Cow::Owned(format!(
                "unknown value `{variant}`, expected one of {}",
                expected.join(", ")
            )),
            _ => fallback(),
        }
    }
}

impl From<serde_saphyr::Error> for Error {
    fn from(err: serde_saphyr::Error) -> Self {
        Error::Yaml(err)
    }
}
