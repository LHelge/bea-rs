use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tokio::task::JoinSet;

use crate::error::{Error, Result};
use crate::task::{self, Task};

const BEARS_DIR: &str = ".bears";

/// Returns the `.bears/` directory path relative to the given base.
pub fn tasks_dir(base: &Path) -> PathBuf {
    base.join(BEARS_DIR)
}

/// Initialize a new `.bears/` directory and `.bears.yml` config.
pub fn init(base: &Path) -> Result<PathBuf> {
    let dir = base.join(BEARS_DIR);
    fs::create_dir_all(&dir)?;
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

/// Load a single task by ID.
pub fn load_one(base: &Path, id: &str) -> Result<Task> {
    let path = find_task_path(base, id)?;
    let content = fs::read_to_string(&path)?;
    task::parse_task(&content).map_err(|_| Error::InvalidFrontmatter {
        path,
        reason: "failed to parse frontmatter".into(),
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

/// Delete a task file by ID.
pub fn delete(base: &Path, id: &str) -> Result<()> {
    let path = find_task_path(base, id)?;
    fs::remove_file(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Priority;
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
    fn test_delete() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();

        let t = Task::new("ef56".into(), "Delete me".into(), Priority::P3);
        save(tmp.path(), &t).unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_ok());

        delete(tmp.path(), "ef56").unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_err());
    }
}
