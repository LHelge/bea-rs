use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tokio::task::JoinSet;

use crate::error::{Error, Result};
use crate::task::{self, Task};

const BEARS_DIR: &str = ".bears";
const ARCHIVE_SUBDIR: &str = "archive";

/// Returns the `.bears/` directory path relative to the given base.
pub fn tasks_dir(base: &Path) -> PathBuf {
    base.join(BEARS_DIR)
}

/// Returns the `.bears/archive/` directory path relative to the given base.
pub fn archive_dir(base: &Path) -> PathBuf {
    tasks_dir(base).join(ARCHIVE_SUBDIR)
}

/// Initialize a new `.bears/` directory (and `.bears/archive/`) and `.bears.yml` config.
pub fn init(base: &Path) -> Result<PathBuf> {
    let dir = base.join(BEARS_DIR);
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join(ARCHIVE_SUBDIR))?;
    crate::config::create_default(base)?;
    Ok(dir)
}

/// Load all tasks from the `.bears/` directory.
/// Reads files in parallel using tokio. Warns and skips files with invalid frontmatter.
pub async fn load_all(base: &Path) -> Result<HashMap<String, Task>> {
    let dir = tasks_dir(base);
    if !dir.exists() {
        return Err(Error::NotInitialized);
    }

    // Collect .md file paths (directory listing is fast, no need to parallelize)
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            paths.push(path);
        }
    }

    // Read and parse all files in parallel
    let mut join_set = JoinSet::new();
    for path in paths {
        join_set.spawn(async move {
            let content = tokio::fs::read_to_string(&path).await;
            (path, content)
        });
    }

    let mut tasks = HashMap::new();
    while let Some(result) = join_set.join_next().await {
        let (path, content) = result.map_err(|e| std::io::Error::other(e.to_string()))?;
        let content = content?;
        match task::parse_task(&content) {
            Ok(t) => {
                if tasks.contains_key(&t.id) {
                    eprintln!("warning: duplicate task ID {} in {}", t.id, path.display());
                    continue;
                }
                tasks.insert(t.id.clone(), t);
            }
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", path.display());
            }
        }
    }

    Ok(tasks)
}

/// Load all archived tasks from the `.bears/archive/` directory.
/// Reads files in parallel using tokio. Warns and skips files with invalid frontmatter.
/// Returns an empty map (not an error) if the archive dir does not exist yet.
// Will be consumed by the archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub async fn load_archived(base: &Path) -> Result<HashMap<String, Task>> {
    let dir = archive_dir(base);
    if !dir.exists() {
        return Ok(HashMap::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            paths.push(path);
        }
    }

    let mut join_set = JoinSet::new();
    for path in paths {
        join_set.spawn(async move {
            let content = tokio::fs::read_to_string(&path).await;
            (path, content)
        });
    }

    let mut tasks = HashMap::new();
    while let Some(result) = join_set.join_next().await {
        let (path, content) = result.map_err(|e| std::io::Error::other(e.to_string()))?;
        let content = content?;
        match task::parse_task(&content) {
            Ok(t) => {
                if tasks.contains_key(&t.id) {
                    eprintln!(
                        "warning: duplicate archived task ID {} in {}",
                        t.id,
                        path.display()
                    );
                    continue;
                }
                tasks.insert(t.id.clone(), t);
            }
            Err(e) => {
                eprintln!("warning: skipping archived {}: {e}", path.display());
            }
        }
    }

    Ok(tasks)
}

/// Find the file path for an archived task by its exact ID.
// Will be consumed by the archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn find_archived_path(base: &Path, id: &str) -> Result<PathBuf> {
    let dir = archive_dir(base);
    if !dir.exists() {
        return Err(Error::TaskNotFound(id.into()));
    }
    let prefix = format!("{id}-");

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            return Ok(entry.path());
        }
    }

    Err(Error::TaskNotFound(id.into()))
}

/// Move a task file from `.bears/` to `.bears/archive/`.
// Will be consumed by the archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn move_to_archive(base: &Path, id: &str) -> Result<()> {
    let adir = archive_dir(base);
    fs::create_dir_all(&adir)?;
    let src = find_task_path(base, id)?;
    let filename = src
        .file_name()
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    let dst = adir.join(filename);
    fs::rename(src, dst)?;
    Ok(())
}

/// Move a task file from `.bears/archive/` back to `.bears/`.
// Will be consumed by the archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub fn move_from_archive(base: &Path, id: &str) -> Result<()> {
    let src = find_archived_path(base, id)?;
    let filename = src
        .file_name()
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    let dst = tasks_dir(base).join(filename);
    fs::rename(src, dst)?;
    Ok(())
}

/// Return the set of ALL known task IDs — both active and archived.
/// Used during ID generation so new IDs never collide with archived ones.
// Will be consumed by the archive CLI/MCP commands (separate task).
#[allow(dead_code)]
pub async fn all_known_ids(base: &Path) -> Result<HashSet<String>> {
    let active = load_all(base).await?;
    let archived = load_archived(base).await?;
    let mut ids: HashSet<String> = active.into_keys().collect();
    ids.extend(archived.into_keys());
    Ok(ids)
}

/// Return archived task IDs by scanning filenames only (no YAML parsing).
/// Used synchronously to extend the collision set during task creation.
pub fn archived_id_set(base: &Path) -> HashSet<String> {
    let dir = archive_dir(base);
    if !dir.exists() {
        return HashSet::new();
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".md") {
                return None;
            }
            // filename format: {id}-{slug}.md — extract the id prefix
            name.split('-').next().map(|id| id.to_string())
        })
        .collect()
}

/// Find the file path for a task by its ID prefix.
pub fn find_task_path(base: &Path, id: &str) -> Result<PathBuf> {
    let dir = tasks_dir(base);
    let prefix = format!("{id}-");

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            return Ok(entry.path());
        }
    }

    Err(Error::TaskNotFound(id.into()))
}

/// Load a single task by exact ID (used in tests and for file-level operations).
#[cfg(test)]
pub fn load_one(base: &Path, id: &str) -> Result<Task> {
    let path = find_task_path(base, id)?;
    let content = fs::read_to_string(&path)?;
    task::parse_task(&content).map_err(|e| match e {
        Error::InvalidFrontmatter { reason, .. } => Error::InvalidFrontmatter {
            path: path.clone(),
            reason,
        },
        other => Error::InvalidFrontmatter {
            path: path.clone(),
            reason: other.to_string(),
        },
    })
}

/// Save a task to disk. Deletes the old file if the slug has changed.
pub fn save(base: &Path, t: &Task) -> Result<()> {
    let dir = tasks_dir(base);
    let new_path = dir.join(task::filename(t));

    // Delete old file if it exists with a different name
    if let Ok(old_path) = find_task_path(base, &t.id)
        && old_path != new_path
    {
        fs::remove_file(&old_path)?;
    }

    let content = task::render_task(t);
    fs::write(&new_path, content)?;
    Ok(())
}

/// Resolve a task ID or unique prefix to a full task ID.
/// Returns the exact match if found, or the unique prefix match.
/// Errors if zero or multiple tasks match.
pub fn resolve_prefix(tasks: &HashMap<String, Task>, prefix: &str) -> Result<String> {
    // Exact match first
    if tasks.contains_key(prefix) {
        return Ok(prefix.to_string());
    }

    let matches: Vec<&String> = tasks.keys().filter(|id| id.starts_with(prefix)).collect();

    match matches.len() {
        0 => Err(Error::TaskNotFound(prefix.into())),
        1 => Ok(matches[0].clone()),
        _ => {
            let mut sorted = matches.iter().map(|s| s.as_str()).collect::<Vec<_>>();
            sorted.sort();
            Err(Error::AmbiguousPrefix {
                prefix: prefix.into(),
                matches: sorted.join(", "),
            })
        }
    }
}

/// Delete a task file by ID.
pub fn delete(base: &Path, id: &str) -> Result<()> {
    let path = find_task_path(base, id)?;
    fs::remove_file(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, TaskType};
    use tempfile::TempDir;

    #[test]
    fn test_init_creates_dir() {
        let tmp = TempDir::new().unwrap();
        let dir = init(tmp.path()).unwrap();
        assert!(dir.exists());
    }

    #[tokio::test]
    async fn test_load_all_empty() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let tasks = load_all(tmp.path()).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let mut t = Task::new("ab12".into(), "Test task".into(), Priority::P1);
        t.tags = vec!["backend".into()];
        t.body = "Some body text.\n".into();

        save(tmp.path(), &t).unwrap();

        let loaded = load_one(tmp.path(), "ab12").unwrap();
        assert_eq!(loaded.id, "ab12");
        assert_eq!(loaded.title, "Test task");
        assert_eq!(loaded.tags, vec!["backend"]);
        assert_eq!(loaded.body, "Some body text.\n");
    }

    #[test]
    fn test_save_renames_on_title_change() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("cd34".into(), "Original title".into(), Priority::P2);
        save(tmp.path(), &t).unwrap();

        let old_path = find_task_path(tmp.path(), "cd34").unwrap();
        assert!(old_path.ends_with("cd34-original-title.md"));

        let mut t2 = load_one(tmp.path(), "cd34").unwrap();
        t2.title = "New title".into();
        save(tmp.path(), &t2).unwrap();

        let new_path = find_task_path(tmp.path(), "cd34").unwrap();
        assert!(new_path.ends_with("cd34-new-title.md"));
        assert!(!old_path.exists());
    }

    #[tokio::test]
    async fn test_load_all_skips_non_md() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        // write a non-.md file to confirm it's skipped
        fs::write(tmp.path().join(BEARS_DIR).join("notes.txt"), "ignored").unwrap();
        let tasks = load_all(tmp.path()).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_not_initialized() {
        let tmp = TempDir::new().unwrap();
        assert!(matches!(
            load_all(tmp.path()).await,
            Err(Error::NotInitialized)
        ));
    }

    #[test]
    fn test_task_not_found() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        assert!(matches!(
            load_one(tmp.path(), "zzzz"),
            Err(Error::TaskNotFound(_))
        ));
    }

    #[test]
    fn test_load_one_missing_delimiter() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        // Write a file with no frontmatter delimiters
        fs::write(
            tmp.path().join(BEARS_DIR).join("bad1-no-delimiters.md"),
            "just some text, no frontmatter",
        )
        .unwrap();
        let err = load_one(tmp.path(), "bad1").unwrap_err();
        match &err {
            Error::InvalidFrontmatter { path, reason } => {
                assert!(path.ends_with("bad1-no-delimiters.md"));
                assert!(reason.contains("missing opening --- delimiter"), "{reason}");
            }
            other => panic!("expected InvalidFrontmatter, got {other:?}"),
        }
    }

    #[test]
    fn test_load_one_bad_yaml() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        // Write a file with delimiters but invalid YAML
        fs::write(
            tmp.path().join(BEARS_DIR).join("bad2-bad-yaml.md"),
            "---\n: :\nbogus yaml\n---\n",
        )
        .unwrap();
        let err = load_one(tmp.path(), "bad2").unwrap_err();
        match &err {
            Error::InvalidFrontmatter { path, reason } => {
                assert!(path.ends_with("bad2-bad-yaml.md"));
                // Should contain the serde_yaml error details, not just "failed to parse"
                assert!(!reason.contains("failed to parse frontmatter"), "{reason}");
                assert!(!reason.is_empty());
            }
            other => panic!("expected InvalidFrontmatter, got {other:?}"),
        }
    }

    #[test]
    fn test_delete() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("ef56".into(), "Delete me".into(), Priority::P3);
        save(tmp.path(), &t).unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_ok());

        delete(tmp.path(), "ef56").unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_err());
    }

    #[test]
    fn test_resolve_prefix_exact_match() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "ab12".into(),
            Task::new("ab12".into(), "T1".into(), Priority::P2),
        );
        assert_eq!(resolve_prefix(&tasks, "ab12").unwrap(), "ab12");
    }

    #[test]
    fn test_resolve_prefix_unique() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "ab12".into(),
            Task::new("ab12".into(), "T1".into(), Priority::P2),
        );
        tasks.insert(
            "cd34".into(),
            Task::new("cd34".into(), "T2".into(), Priority::P2),
        );
        assert_eq!(resolve_prefix(&tasks, "ab").unwrap(), "ab12");
    }

    #[test]
    fn test_resolve_prefix_ambiguous() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "ab12".into(),
            Task::new("ab12".into(), "T1".into(), Priority::P2),
        );
        tasks.insert(
            "ab34".into(),
            Task::new("ab34".into(), "T2".into(), Priority::P2),
        );
        let err = resolve_prefix(&tasks, "ab").unwrap_err();
        assert!(matches!(err, Error::AmbiguousPrefix { .. }));
    }

    #[test]
    fn test_resolve_prefix_no_match() {
        let mut tasks = HashMap::new();
        tasks.insert(
            "ab12".into(),
            Task::new("ab12".into(), "T1".into(), Priority::P2),
        );
        let err = resolve_prefix(&tasks, "zz").unwrap_err();
        assert!(matches!(err, Error::TaskNotFound(_)));
    }

    #[test]
    fn test_save_and_load_epic() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let mut t = Task::new("ep01".into(), "My epic".into(), Priority::P1);
        t.task_type = TaskType::Epic;
        t.body = "Epic description.\n".into();

        save(tmp.path(), &t).unwrap();

        let loaded = load_one(tmp.path(), "ep01").unwrap();
        assert_eq!(loaded.task_type, TaskType::Epic);
        assert_eq!(loaded.title, "My epic");
        assert_eq!(loaded.body, "Epic description.\n");

        // Verify the file on disk contains "type: epic"
        let path = find_task_path(tmp.path(), "ep01").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("type: epic"));
    }

    #[test]
    fn test_save_and_load_task_no_type_field() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("tk01".into(), "Regular task".into(), Priority::P2);
        save(tmp.path(), &t).unwrap();

        // Verify the file on disk does NOT contain "type:"
        let path = find_task_path(tmp.path(), "tk01").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("type:"));

        // Load it back and verify default
        let loaded = load_one(tmp.path(), "tk01").unwrap();
        assert_eq!(loaded.task_type, TaskType::Task);
    }

    #[tokio::test]
    async fn test_load_all_mixed_types() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t1 = Task::new("tk02".into(), "A task".into(), Priority::P2);
        save(tmp.path(), &t1).unwrap();

        let mut t2 = Task::new("ep02".into(), "An epic".into(), Priority::P1);
        t2.task_type = TaskType::Epic;
        save(tmp.path(), &t2).unwrap();

        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks["tk02"].task_type, TaskType::Task);
        assert_eq!(tasks["ep02"].task_type, TaskType::Epic);
    }

    // ── Archive storage layer tests ──────────────────────────────────

    #[test]
    fn test_init_creates_archive_dir() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        assert!(archive_dir(tmp.path()).exists());
    }

    #[tokio::test]
    async fn test_load_all_ignores_archive_subdir() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        // Save an active task
        let active = Task::new("ac01".into(), "Active task".into(), Priority::P2);
        save(tmp.path(), &active).unwrap();

        // Write a task file directly into the archive subdir
        let archived = Task::new("ar01".into(), "Archived task".into(), Priority::P3);
        let archived_content = crate::task::render_task(&archived);
        fs::write(
            archive_dir(tmp.path()).join(crate::task::filename(&archived)),
            archived_content,
        )
        .unwrap();

        // load_all must only return the active task, not the archived one
        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks.contains_key("ac01"));
        assert!(!tasks.contains_key("ar01"));
    }

    #[test]
    fn test_move_to_archive_and_back() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("mv01".into(), "Move me".into(), Priority::P1);
        save(tmp.path(), &t).unwrap();

        // File exists in active dir
        assert!(find_task_path(tmp.path(), "mv01").is_ok());
        assert!(find_archived_path(tmp.path(), "mv01").is_err());

        // Move to archive
        move_to_archive(tmp.path(), "mv01").unwrap();
        assert!(find_task_path(tmp.path(), "mv01").is_err());
        assert!(find_archived_path(tmp.path(), "mv01").is_ok());

        // Move back from archive
        move_from_archive(tmp.path(), "mv01").unwrap();
        assert!(find_task_path(tmp.path(), "mv01").is_ok());
        assert!(find_archived_path(tmp.path(), "mv01").is_err());
    }

    #[tokio::test]
    async fn test_load_archived() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        // Save two tasks and archive one
        let t1 = Task::new("ar01".into(), "Archive this".into(), Priority::P2);
        save(tmp.path(), &t1).unwrap();
        let t2 = Task::new("ac01".into(), "Keep active".into(), Priority::P1);
        save(tmp.path(), &t2).unwrap();

        move_to_archive(tmp.path(), "ar01").unwrap();

        let archived = load_archived(tmp.path()).await.unwrap();
        assert_eq!(archived.len(), 1);
        assert!(archived.contains_key("ar01"));

        let active = load_all(tmp.path()).await.unwrap();
        assert_eq!(active.len(), 1);
        assert!(active.contains_key("ac01"));
    }

    #[tokio::test]
    async fn test_load_archived_empty_when_no_archive_dir() {
        let tmp = TempDir::new().unwrap();
        // Don't call init — no .bears/ or archive/ exists
        // Manually create just .bears/ without archive subdir
        fs::create_dir_all(tasks_dir(tmp.path())).unwrap();
        let archived = load_archived(tmp.path()).await.unwrap();
        assert!(archived.is_empty());
    }

    #[tokio::test]
    async fn test_all_known_ids_includes_archived() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t1 = Task::new("id01".into(), "Active".into(), Priority::P2);
        save(tmp.path(), &t1).unwrap();
        let t2 = Task::new("id02".into(), "To archive".into(), Priority::P1);
        save(tmp.path(), &t2).unwrap();

        move_to_archive(tmp.path(), "id02").unwrap();

        let ids = all_known_ids(tmp.path()).await.unwrap();
        assert!(ids.contains("id01"), "active id should be included");
        assert!(ids.contains("id02"), "archived id should be included");
        assert_eq!(ids.len(), 2);
    }
}
