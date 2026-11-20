use std::collections::{BTreeMap, HashMap, HashSet};
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

    // Read all files in parallel
    let mut join_set = JoinSet::new();
    for path in paths {
        join_set.spawn(async move {
            let content = tokio::fs::read_to_string(&path).await;
            (path, content)
        });
    }

    // Collect all results first, then sort by path so the in-memory winner
    // for a duplicate ID is the lexicographically-first filename — matching
    // the deterministic rule used by find_task_path.
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        let (path, content) = result.map_err(|e| std::io::Error::other(e.to_string()))?;
        results.push((path, content));
    }
    results.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut tasks = HashMap::new();
    for (path, content) in results {
        let content = match content {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", path.display());
                continue;
            }
        };
        match task::parse_task(&content) {
            Ok(t) => {
                if tasks.contains_key(&t.id) {
                    eprintln!("warning: duplicate task ID {} in {}", t.id, path.display());
                    continue;
                }
                tasks.insert(t.id.clone(), t);
            }
            Err(e) => {
                // Patch real path into InvalidFrontmatter so the message
                // reads "invalid frontmatter in <path>: <reason>" instead of
                // the awkward "invalid frontmatter in : <reason>" that
                // parse_task produces (it always sets path = "").
                let e = match e {
                    Error::InvalidFrontmatter { reason, .. } => Error::InvalidFrontmatter {
                        path: path.clone(),
                        reason,
                    },
                    other => other,
                };
                eprintln!("warning: skipping {}: {e}", path.display());
            }
        }
    }

    Ok(tasks)
}

/// Load all archived tasks from the `.bears/archive/` directory.
/// Reads files in parallel using tokio. Warns and skips files with invalid frontmatter.
/// Returns an empty map (not an error) if the archive dir does not exist yet.
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

    // Collect and sort by path for the same determinism as load_all.
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        let (path, content) = result.map_err(|e| std::io::Error::other(e.to_string()))?;
        results.push((path, content));
    }
    results.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut tasks = HashMap::new();
    for (path, content) in results {
        let content = match content {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: skipping archived {}: {e}", path.display());
                continue;
            }
        };
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
                let e = match e {
                    Error::InvalidFrontmatter { reason, .. } => Error::InvalidFrontmatter {
                        path: path.clone(),
                        reason,
                    },
                    other => other,
                };
                eprintln!("warning: skipping archived {}: {e}", path.display());
            }
        }
    }

    Ok(tasks)
}

/// Find the file path for an archived task by its exact ID.
/// Uses the same lexicographic-first rule as find_task_path.
pub fn find_archived_path(base: &Path, id: &str) -> Result<PathBuf> {
    let dir = archive_dir(base);
    if !dir.exists() {
        return Err(Error::TaskNotFound(id.into()));
    }
    let prefix = format!("{id}-");

    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            matches.push(entry.path());
        }
    }

    matches.sort();
    matches
        .into_iter()
        .next()
        .ok_or_else(|| Error::TaskNotFound(id.into()))
}

/// Move a task file from `.bears/` to `.bears/archive/`.
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
pub fn move_from_archive(base: &Path, id: &str) -> Result<()> {
    let src = find_archived_path(base, id)?;
    let filename = src
        .file_name()
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;
    let dst = tasks_dir(base).join(filename);
    fs::rename(src, dst)?;
    Ok(())
}

/// Every ID a new task must not take: the IDs of `tasks` (the active store)
/// and of the archive, plus every ancestor of each.
///
/// Ancestors count even when their task no longer exists, since a new task
/// with that ID would adopt the orphans below it. This is the collision pool
/// for [`task::generate_id`] and [`task::generate_child_id`].
pub fn reserved_ids(base: &Path, tasks: &HashMap<String, Task>) -> HashSet<String> {
    let archived = archived_id_set(base);
    task::with_ancestors(tasks.keys().chain(archived.iter()))
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
            task::id_from_filename(&name.to_string_lossy()).map(str::to_string)
        })
        .collect()
}

/// Find the file path for a task by its ID prefix.
/// When multiple files share the same ID prefix (duplicate-ID situation),
/// returns the lexicographically first filename so the on-disk winner is
/// deterministic and matches the winner chosen by load_all.
pub fn find_task_path(base: &Path, id: &str) -> Result<PathBuf> {
    let dir = tasks_dir(base);
    let prefix = format!("{id}-");

    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            matches.push(entry.path());
        }
    }

    matches.sort();
    matches
        .into_iter()
        .next()
        .ok_or_else(|| Error::TaskNotFound(id.into()))
}

/// Load a single task by exact ID — a fresh read from disk, bypassing any
/// snapshot (used for file-level operations and the assignee fence).
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
    let new_path = tasks_dir(base).join(task::filename(t));
    let old_path = find_task_path(base, &t.id).ok();

    write_task_file(&new_path, t)?;

    // Remove the old file only after the new one is safely in place
    if let Some(old_path) = old_path
        && old_path != new_path
    {
        fs::remove_file(&old_path)?;
    }
    Ok(())
}

/// Write `t` to `path` atomically (temp file + rename in the same
/// directory), so a crash cannot leave a truncated task behind.
fn write_task_file(path: &Path, t: &Task) -> Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::TaskNotFound(t.id.clone()))?;
    let tmp_path = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
    fs::write(&tmp_path, task::render_task(t))?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Read and parse every task file in `dir`, sorted by path. A missing
/// directory yields nothing; files that cannot be read or parsed are skipped
/// with a warning, as [`load_all`] does.
fn read_task_files(dir: &Path) -> Result<Vec<(PathBuf, Task)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        match fs::read_to_string(&path)
            .map_err(Error::from)
            .and_then(|c| task::parse_task(&c))
        {
            Ok(t) => files.push((path, t)),
            Err(e) => eprintln!("warning: skipping {}: {e}", path.display()),
        }
    }
    Ok(files)
}

/// Rename tasks across the active store and the archive.
///
/// `renames` maps old IDs to new IDs. Each task whose ID is a key is written
/// under its new ID in the directory it already lives in (active or
/// archive), and every `depends_on` entry naming a key is rewritten in every
/// task, active and archived. Nothing else changes, `updated` included: a
/// caller that wants the rename to count as an edit records that itself.
/// Renaming a task does not rename the tasks below it; a caller moving a
/// subtree puts every one of its IDs in `renames`.
///
/// Everything is checked before anything is written: each key must be the ID
/// of an active or archived task ([`Error::TaskNotFound`]), each new ID must
/// be well formed ([`Error::InvalidId`]), and no new ID may be shared by two
/// keys or belong to a task that keeps its ID ([`Error::IdTaken`]).
///
/// The files under their new IDs are written first, then the rewritten
/// references, and the old files are removed last. An interruption therefore
/// leaves some tasks on disk under both their old and their new ID, but never
/// loses one, and no rewritten reference names an ID that has no file.
pub fn rename_ids(base: &Path, renames: &BTreeMap<String, String>) -> Result<()> {
    if !tasks_dir(base).exists() {
        return Err(Error::NotInitialized);
    }
    if renames.is_empty() {
        return Ok(());
    }
    let mut files = read_task_files(&tasks_dir(base))?;
    files.extend(read_task_files(&archive_dir(base))?);

    let present: HashSet<&str> = files.iter().map(|(_, t)| t.id.as_str()).collect();
    if let Some(missing) = renames.keys().find(|old| !present.contains(old.as_str())) {
        return Err(Error::TaskNotFound(missing.clone()));
    }
    let mut targets = HashSet::new();
    for new in renames.values() {
        task::validate_id(new)?;
        let kept_by_other = present.contains(new.as_str()) && !renames.contains_key(new);
        if kept_by_other || !targets.insert(new.as_str()) {
            return Err(Error::IdTaken(new.clone()));
        }
    }

    let mut moved = Vec::new();
    let mut rewritten = Vec::new();
    let mut old_paths = Vec::new();
    for (path, mut t) in files {
        let mut changed = false;
        for dep in &mut t.depends_on {
            if let Some(new) = renames.get(dep) {
                *dep = new.clone();
                changed = true;
            }
        }
        if let Some(new) = renames.get(&t.id) {
            t.id = new.clone();
            let new_path = path.with_file_name(task::filename(&t));
            moved.push((new_path, t));
            old_paths.push(path);
        } else if changed {
            rewritten.push((path, t));
        }
    }

    let mut written = HashSet::new();
    for (path, t) in moved.iter().chain(rewritten.iter()) {
        write_task_file(path, t)?;
        written.insert(path.clone());
    }
    for path in old_paths {
        // A new file may have taken an old file's name (a swap of IDs).
        if !written.contains(&path) {
            fs::remove_file(&path)?;
        }
    }
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

    /// load_all must skip files with invalid frontmatter and continue loading
    /// the rest. The test also verifies the warning message includes the real
    /// file path (not an empty string as parse_task would produce).
    #[tokio::test]
    async fn test_load_all_skips_invalid_frontmatter() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        // A valid task
        let t = Task::new("ok01".into(), "Good task".into(), Priority::P2);
        save(tmp.path(), &t).unwrap();

        // A file with no frontmatter delimiters (parse_task will set path = "")
        let bad_path = tmp.path().join(BEARS_DIR).join("bad-no-delimiters.md");
        fs::write(&bad_path, "just some text, no frontmatter").unwrap();

        // load_all should skip the bad file and return only the good task
        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.len(), 1, "bad file should be skipped");
        assert!(tasks.contains_key("ok01"));

        // load_all re-injects the real path, so find_task_path must not return
        // the bad file as a task (it's not a valid task file with id prefix).
        // What we really care about is that loading succeeded without panic/error.
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
                // Should contain the YAML parser's error details, not just "failed to parse"
                assert!(!reason.contains("failed to parse frontmatter"), "{reason}");
                assert!(!reason.is_empty());
                // One line with a position, so it fits in a warning.
                assert!(!reason.contains('\n'), "{reason}");
                assert!(reason.contains("line 1"), "{reason}");
            }
            other => panic!("expected InvalidFrontmatter, got {other:?}"),
        }
    }

    #[test]
    fn test_save_leaves_no_temp_files() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("at01".into(), "Atomic".into(), Priority::P2);
        save(tmp.path(), &t).unwrap();

        let leftovers: Vec<_> = fs::read_dir(tmp.path().join(BEARS_DIR))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("tmp"))
            .collect();
        assert!(leftovers.is_empty());
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

    /// Two files that share the same task ID (a corrupt state that can arise
    /// e.g. from a botched manual edit) must be resolved deterministically:
    /// load_all keeps the task from the lexicographically-first filename, and
    /// find_task_path returns that same file — so memory and disk agree.
    #[tokio::test]
    async fn test_load_all_duplicate_id_deterministic() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let dir = tmp.path().join(BEARS_DIR);

        // Build a minimal valid frontmatter block for a shared ID "dup1".
        // "aaa" slug sorts before "zzz", so "dup1-aaa-slug.md" must win.
        let make_content = |title: &str| {
            format!(
                "---\nid: dup1\ntitle: {title}\nstatus: open\npriority: P2\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n"
            )
        };

        let first_file = dir.join("dup1-aaa-slug.md"); // lex-first
        let second_file = dir.join("dup1-zzz-slug.md"); // lex-second

        fs::write(&first_file, make_content("First file")).unwrap();
        fs::write(&second_file, make_content("Second file")).unwrap();

        // load_all: only one entry for "dup1", and it comes from the lex-first file
        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(
            tasks.len(),
            1,
            "duplicate should be deduplicated to one entry"
        );
        assert_eq!(
            tasks["dup1"].title, "First file",
            "lex-first file must win in load_all"
        );

        // find_task_path must return the same lex-first file
        let path = find_task_path(tmp.path(), "dup1").unwrap();
        assert!(
            path.ends_with("dup1-aaa-slug.md"),
            "find_task_path must return lex-first file, got: {}",
            path.display()
        );
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
    fn test_dotted_ids_find_and_load_by_exact_id() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let parent = Task::new("a1b".into(), "Parent".into(), Priority::P2);
        let child = Task::new("a1b.c2".into(), "Child".into(), Priority::P2);
        save(tmp.path(), &parent).unwrap();
        save(tmp.path(), &child).unwrap();

        let parent_path = find_task_path(tmp.path(), "a1b").unwrap();
        assert!(parent_path.ends_with("a1b-parent.md"), "{parent_path:?}");
        assert_eq!(load_one(tmp.path(), "a1b.c2").unwrap().title, "Child");
        assert_eq!(load_one(tmp.path(), "a1b").unwrap().title, "Parent");

        delete(tmp.path(), "a1b").unwrap();
        assert!(find_task_path(tmp.path(), "a1b").is_err());
        assert!(find_task_path(tmp.path(), "a1b.c2").is_ok());
    }

    #[test]
    fn test_resolve_prefix_prefers_exact_dotted_parent() {
        let mut tasks = HashMap::new();
        for id in ["a1b", "a1b.c2"] {
            tasks.insert(
                id.to_string(),
                Task::new(id.into(), id.into(), Priority::P2),
            );
        }
        assert_eq!(resolve_prefix(&tasks, "a1b").unwrap(), "a1b");
        assert_eq!(resolve_prefix(&tasks, "a1b.c").unwrap(), "a1b.c2");
    }

    #[test]
    fn test_archived_id_set_reads_dotted_ids() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        for id in ["a1b", "a1b.c2", "a1b.c2.d7"] {
            let t = Task::new(id.into(), "Some task".into(), Priority::P2);
            fs::write(
                archive_dir(tmp.path()).join(task::filename(&t)),
                task::render_task(&t),
            )
            .unwrap();
        }
        fs::write(archive_dir(tmp.path()).join("notes.txt"), "x").unwrap();
        let ids = archived_id_set(tmp.path());
        let expected: HashSet<String> = ["a1b", "a1b.c2", "a1b.c2.d7"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(ids, expected);
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

    fn renames(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn archive(base: &Path, t: &Task) {
        fs::write(
            archive_dir(base).join(task::filename(t)),
            task::render_task(t),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_rename_ids_across_active_and_archive() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let mut epic = Task::new("e1".into(), "Epic".into(), Priority::P1);
        epic.task_type = TaskType::Epic;
        let mut child = Task::new("e1.c2".into(), "Child".into(), Priority::P2);
        child.depends_on = vec!["e1.d3".into()];
        let old_child = Task::new("e1.d3".into(), "Old child".into(), Priority::P2);
        let mut user = Task::new("u1".into(), "User".into(), Priority::P2);
        user.depends_on = vec!["e1.c2".into(), "zz".into()];
        let mut archived_user = Task::new("au".into(), "Archived user".into(), Priority::P2);
        archived_user.depends_on = vec!["e1".into()];
        let untouched = Task::new("zz".into(), "Untouched".into(), Priority::P2);
        for t in [&epic, &child, &user, &untouched] {
            save(tmp.path(), t).unwrap();
        }
        archive(tmp.path(), &old_child);
        archive(tmp.path(), &archived_user);
        let zz_path = find_task_path(tmp.path(), "zz").unwrap();
        let untouched_before = fs::read_to_string(&zz_path).unwrap();
        let untouched_mtime = fs::metadata(&zz_path).unwrap().modified().unwrap();

        rename_ids(
            tmp.path(),
            &renames(&[("e1", "n9"), ("e1.c2", "n9.c2"), ("e1.d3", "n9.d3")]),
        )
        .unwrap();

        let active = load_all(tmp.path()).await.unwrap();
        let mut ids: Vec<_> = active.keys().cloned().collect();
        ids.sort();
        assert_eq!(ids, ["n9", "n9.c2", "u1", "zz"]);
        assert_eq!(active["n9"].task_type, TaskType::Epic);
        assert_eq!(active["n9.c2"].depends_on, ["n9.d3"]);
        assert_eq!(active["u1"].depends_on, ["n9.c2", "zz"]);
        // A rename is not an edit: `updated` stays as it was.
        assert_eq!(active["n9.c2"].updated, child.updated);
        assert_eq!(active["u1"].updated, user.updated);
        // A task with nothing to rewrite is not touched at all.
        assert_eq!(fs::read_to_string(&zz_path).unwrap(), untouched_before);
        assert_eq!(
            fs::metadata(&zz_path).unwrap().modified().unwrap(),
            untouched_mtime
        );

        let archived = load_archived(tmp.path()).await.unwrap();
        let mut ids: Vec<_> = archived.keys().cloned().collect();
        ids.sort();
        assert_eq!(ids, ["au", "n9.d3"]);
        assert_eq!(archived["au"].depends_on, ["n9"]);

        // The old files are gone, from both directories.
        for id in ["e1", "e1.c2"] {
            assert!(find_task_path(tmp.path(), id).is_err(), "{id}");
        }
        assert!(find_archived_path(tmp.path(), "e1.d3").is_err());
        let leftovers = fs::read_dir(tasks_dir(tmp.path()))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[tokio::test]
    async fn test_rename_ids_swaps_ids_with_the_same_slug() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let mut a = Task::new("aa".into(), "Same".into(), Priority::P0);
        a.depends_on = vec!["bb".into()];
        let b = Task::new("bb".into(), "Same".into(), Priority::P3);
        save(tmp.path(), &a).unwrap();
        save(tmp.path(), &b).unwrap();

        rename_ids(tmp.path(), &renames(&[("aa", "bb"), ("bb", "aa")])).unwrap();

        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks["bb"].priority, Priority::P0);
        assert_eq!(tasks["bb"].depends_on, ["aa"]);
        assert_eq!(tasks["aa"].priority, Priority::P3);
    }

    #[tokio::test]
    async fn test_rename_ids_validates_before_writing() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let mut a = Task::new("aa".into(), "A".into(), Priority::P2);
        a.depends_on = vec!["bb".into()];
        save(tmp.path(), &a).unwrap();
        save(
            tmp.path(),
            &Task::new("bb".into(), "B".into(), Priority::P2),
        )
        .unwrap();
        archive(
            tmp.path(),
            &Task::new("cc".into(), "C".into(), Priority::P2),
        );
        let before = load_all(tmp.path()).await.unwrap();

        let err = |pairs: &[(&str, &str)]| rename_ids(tmp.path(), &renames(pairs)).unwrap_err();
        assert!(matches!(err(&[("nope", "xx")]), Error::TaskNotFound(id) if id == "nope"));
        // Taken by a task that keeps its ID, active or archived.
        assert!(matches!(err(&[("bb", "xx"), ("aa", "cc")]), Error::IdTaken(id) if id == "cc"));
        assert!(matches!(err(&[("aa", "bb")]), Error::IdTaken(id) if id == "bb"));
        // Two tasks cannot share a new ID.
        assert!(matches!(err(&[("aa", "xx"), ("bb", "xx")]), Error::IdTaken(id) if id == "xx"));
        assert!(matches!(err(&[("aa", "Bad_1")]), Error::InvalidId { .. }));

        let after = load_all(tmp.path()).await.unwrap();
        assert_eq!(
            after.keys().collect::<HashSet<_>>(),
            before.keys().collect::<HashSet<_>>()
        );
        assert_eq!(after["aa"].depends_on, ["bb"]);
        assert!(find_archived_path(tmp.path(), "cc").is_ok());

        // An empty map is a no-op.
        rename_ids(tmp.path(), &BTreeMap::new()).unwrap();
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
}
