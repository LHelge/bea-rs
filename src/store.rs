use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::task::{self, Task};

const TASKS_DIR: &str = ".tasks";
const CONFIG_FILE: &str = "config.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_project_name")]
    pub project: String,
    #[serde(default = "default_priority")]
    pub default_priority: task::Priority,
}

fn default_project_name() -> String {
    "my-project".into()
}

fn default_priority() -> task::Priority {
    task::Priority::P2
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project: default_project_name(),
            default_priority: default_priority(),
        }
    }
}

/// Returns the `.tasks/` directory path relative to the given base.
pub fn tasks_dir(base: &Path) -> PathBuf {
    base.join(TASKS_DIR)
}

/// Initialize a new `.tasks/` directory with a config file.
pub fn init(base: &Path, project_name: Option<&str>) -> Result<PathBuf> {
    let dir = tasks_dir(base);
    fs::create_dir_all(&dir)?;

    let config = Config {
        project: project_name.unwrap_or("my-project").into(),
        ..Config::default()
    };
    let config_path = dir.join(CONFIG_FILE);
    let yaml = serde_yaml::to_string(&config)?;
    fs::write(&config_path, yaml)?;

    Ok(dir)
}

/// Load the config from `.tasks/config.yaml`.
#[allow(dead_code)]
pub fn load_config(base: &Path) -> Result<Config> {
    let dir = tasks_dir(base);
    if !dir.exists() {
        return Err(Error::NotInitialized);
    }
    let config_path = dir.join(CONFIG_FILE);
    if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}

/// Load all tasks from the `.tasks/` directory.
/// Warns and skips files with invalid frontmatter.
pub fn load_all(base: &Path) -> Result<HashMap<String, Task>> {
    let dir = tasks_dir(base);
    if !dir.exists() {
        return Err(Error::NotInitialized);
    }

    let mut tasks = HashMap::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
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
#[allow(dead_code)]
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
    fn test_init_creates_dir_and_config() {
        let tmp = TempDir::new().unwrap();
        let dir = init(tmp.path(), Some("test-project")).unwrap();
        assert!(dir.exists());
        assert!(dir.join(CONFIG_FILE).exists());

        let config = load_config(tmp.path()).unwrap();
        assert_eq!(config.project, "test-project");
    }

    #[test]
    fn test_load_all_empty() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();
        let tasks = load_all(tmp.path()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();

        let mut t = Task::new("ab12".into(), "Test task".into(), Priority::P1);
        t.tags = vec!["backend".into()];
        t.body = "Some body text.\n".into();

        save(tmp.path(), &t).unwrap();

        // Load back
        let loaded = load_one(tmp.path(), "ab12").unwrap();
        assert_eq!(loaded.id, "ab12");
        assert_eq!(loaded.title, "Test task");
        assert_eq!(loaded.tags, vec!["backend"]);
        assert_eq!(loaded.body, "Some body text.\n");
    }

    #[test]
    fn test_save_renames_on_title_change() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();

        let t = Task::new("cd34".into(), "Original title".into(), Priority::P2);
        save(tmp.path(), &t).unwrap();

        let old_path = find_task_path(tmp.path(), "cd34").unwrap();
        assert!(old_path.ends_with("cd34-original-title.md"));

        // Change title and save again
        let mut t2 = load_one(tmp.path(), "cd34").unwrap();
        t2.title = "New title".into();
        save(tmp.path(), &t2).unwrap();

        let new_path = find_task_path(tmp.path(), "cd34").unwrap();
        assert!(new_path.ends_with("cd34-new-title.md"));
        assert!(!old_path.exists());
    }

    #[test]
    fn test_load_all_skips_non_md() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();

        // config.yaml should be skipped
        let tasks = load_all(tmp.path()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_not_initialized() {
        let tmp = TempDir::new().unwrap();
        assert!(matches!(load_all(tmp.path()), Err(Error::NotInitialized)));
    }

    #[test]
    fn test_task_not_found() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();
        assert!(matches!(
            load_one(tmp.path(), "zzzz"),
            Err(Error::TaskNotFound(_))
        ));
    }

    #[test]
    fn test_delete() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), None).unwrap();

        let t = Task::new("ef56".into(), "Delete me".into(), Priority::P3);
        save(tmp.path(), &t).unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_ok());

        delete(tmp.path(), "ef56").unwrap();
        assert!(find_task_path(tmp.path(), "ef56").is_err());
    }
}
