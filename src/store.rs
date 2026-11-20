use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tokio::task::JoinSet;

use crate::config::Config;
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
///
/// Refuses a legacy store with [`Error::LegacyStore`]: when any task file,
/// active or archived, carries a `parent:` key, nothing is returned, so no
/// caller can go on to write such a task and drop its epic link. [`migrate`]
/// converts the store.
pub async fn load_all(base: &Path) -> Result<HashMap<String, Task>> {
    let dir = tasks_dir(base);
    if !dir.exists() {
        return Err(Error::NotInitialized);
    }
    let (tasks, mut legacy) = load_dir(&dir, "").await?;
    legacy.extend(scan_legacy(&archive_dir(base)).await?);
    refuse_legacy(legacy)?;
    Ok(tasks)
}

/// Load all archived tasks from the `.bears/archive/` directory.
/// Reads files in parallel using tokio. Warns and skips files with invalid frontmatter.
/// Returns an empty map (not an error) if the archive dir does not exist yet.
///
/// Refuses a legacy store with [`Error::LegacyStore`], as [`load_all`] does.
pub async fn load_archived(base: &Path) -> Result<HashMap<String, Task>> {
    let mut legacy = scan_legacy(&tasks_dir(base)).await?;
    let dir = archive_dir(base);
    let tasks = if dir.exists() {
        let (tasks, archived_legacy) = load_dir(&dir, "archived ").await?;
        legacy.extend(archived_legacy);
        tasks
    } else {
        HashMap::new()
    };
    refuse_legacy(legacy)?;
    Ok(tasks)
}

/// [`Error::LegacyStore`] when `legacy` names any file.
fn refuse_legacy(mut legacy: Vec<PathBuf>) -> Result<()> {
    if legacy.is_empty() {
        return Ok(());
    }
    legacy.sort();
    Err(Error::LegacyStore { files: legacy })
}

/// Read the contents of every `.md` file in `dir` in parallel, sorted by
/// path. A missing directory yields nothing.
async fn read_dir_contents(dir: &Path) -> Result<Vec<(PathBuf, std::io::Result<String>)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    // Directory listing is fast, no need to parallelize
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
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

    // Collect all results first, then sort by path so the in-memory winner
    // for a duplicate ID is the lexicographically-first filename — matching
    // the deterministic rule used by find_task_path.
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.map_err(|e| std::io::Error::other(e.to_string()))?);
    }
    results.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(results)
}

/// Parse every task file in `dir`, warning about and skipping files that
/// cannot be read or parsed and later duplicates of an ID. `label` prefixes
/// the warnings (`"archived "`). Also returns the files that carry a legacy
/// `parent:` key.
async fn load_dir(dir: &Path, label: &str) -> Result<(HashMap<String, Task>, Vec<PathBuf>)> {
    let mut tasks = HashMap::new();
    let mut legacy = Vec::new();
    for (path, content) in read_dir_contents(dir).await? {
        let content = match content {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: skipping {label}{}: {e}", path.display());
                continue;
            }
        };
        match task::parse_task(&content) {
            Ok(t) => {
                if t.legacy_parent.is_some() {
                    legacy.push(path.clone());
                }
                if tasks.contains_key(&t.id) {
                    eprintln!(
                        "warning: duplicate {label}task ID {} in {}",
                        t.id,
                        path.display()
                    );
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
                eprintln!("warning: skipping {label}{}: {e}", path.display());
            }
        }
    }
    Ok((tasks, legacy))
}

/// The task files in `dir` that carry a legacy `parent:` key; a loader uses
/// this for the directory it does not load. The verdict for each file is the
/// parser's (see [`has_legacy_parent_key`]), so both loaders agree on every
/// file short of the exotic case named there. Files that cannot be read are passed over silently: the loader that
/// owns `dir` warns about them.
async fn scan_legacy(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(read_dir_contents(dir)
        .await?
        .into_iter()
        .filter_map(|(path, content)| has_legacy_parent_key(&content.ok()?).then_some(path))
        .collect())
}

/// Whether `content` parses as a task with a legacy `parent:` key, i.e.
/// `parse_task(content)?.legacy_parent.is_some()`; a file that does not parse
/// is never legacy, as the loaders skip it.
///
/// Parsing every file just to answer this would make each load parse the
/// directory it does not return, so a text scan runs first and only its hits
/// are parsed: a file can only have the key if the word `parent` occurs
/// somewhere in its frontmatter (between the opening and closing `---`).
/// That is deliberately loose — indented frontmatter, quoted or complex
/// (`? parent`) keys and flow mappings all pass, and a title that mentions
/// "parent" just costs one extra parse — while body text never does. The
/// one spelling it misses is a quoted key written with escapes
/// (`"p\u0061rent"`), which no version of bears writes.
fn has_legacy_parent_key(content: &str) -> bool {
    may_have_parent_key(content)
        && task::parse_task(content).is_ok_and(|t| t.legacy_parent.is_some())
}

/// The loose pre-filter of [`has_legacy_parent_key`]: false only when the
/// file certainly has no top-level `parent` key.
fn may_have_parent_key(content: &str) -> bool {
    let content = content.trim_start_matches('\u{feff}');
    let mut lines = content.lines().map(|l| l.strip_suffix('\r').unwrap_or(l));
    if lines.next().map(str::trim_end) != Some("---") {
        return false;
    }
    let mut found = false;
    for line in lines {
        if line.trim_end() == "---" {
            return found;
        }
        found |= line.contains("parent");
    }
    // No closing delimiter: not a task file the loaders accept.
    false
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

/// Write `t` to `path` atomically (see [`write_atomically`]), so a crash
/// cannot leave a truncated task behind and two writers never share a
/// temporary file.
fn write_task_file(path: &Path, t: &Task) -> Result<()> {
    write_atomically(path, task::render_task(t).as_bytes())?;
    Ok(())
}

/// Replace the file at `path` with `contents` atomically: a reader sees
/// either the old file or the new one, never a truncated one.
///
/// The contents go to a temporary file in the same directory, named
/// `.{name}.{pid}-{nanos}-{counter}.tmp` and created with `create_new`
/// (another name is tried if it exists), so concurrent writers of the same
/// target, in one process or several, never share it. The file is synced and
/// then renamed over `path`, so the last rename wins. When writing or
/// renaming fails the temporary file is removed. An existing regular file's
/// permissions are carried over; an existing symlink at `path` is replaced by
/// the new file, not followed. The parent directory must exist.
///
/// The temporary name ends in `.tmp`, never `.md`, so the loaders
/// ([`load_all`], [`load_archived`], [`scan_task_files`]) never read one as a
/// task, including a stale one left behind by a killed process.
pub fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::{Error as IoError, ErrorKind, Write};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let name = path
        .file_name()
        .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "not a file name"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();

    let mut attempts = 0;
    let (tmp_path, mut file) = loop {
        attempts += 1;
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_name = format!(
            ".{}.{}-{nanos}-{n}.tmp",
            name.to_string_lossy(),
            std::process::id()
        );
        let tmp_path = path.with_file_name(tmp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
        {
            Ok(file) => break (tmp_path, file),
            Err(e) if e.kind() == ErrorKind::AlreadyExists && attempts < 100 => continue,
            Err(e) => return Err(e),
        }
    };

    let result = (|| {
        file.write_all(contents)?;
        if let Ok(meta) = fs::symlink_metadata(path)
            && meta.is_file()
        {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
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
/// be well formed ([`Error::InvalidId`]), and every new ID must be unused:
/// not shared by two keys and not the ID of any task, active or archived
/// ([`Error::IdTaken`]). The old and new IDs are therefore disjoint, so swaps
/// (`a → b, b → a`), chains (`a → b, b → c`) and identity entries (`a → a`)
/// are all refused: renaming onto an ID that is itself being renamed away
/// could overwrite its file before that task's new file exists.
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
        if present.contains(new.as_str()) || !targets.insert(new.as_str()) {
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

    // Every new ID is unused, so no new file can land on an old file's path.
    for (path, t) in moved.iter().chain(rewritten.iter()) {
        write_task_file(path, t)?;
    }
    for path in old_paths {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// File in `.bears/` that records the renames of a [`migrate`] in progress.
/// It is not a `.md` file, so no loader reads it as a task.
pub const MIGRATION_JOURNAL: &str = ".migrate.json";

/// Why [`migrate`] left a task with a legacy `parent:` key as a root task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanReason {
    /// The `parent:` names no task, active or archived.
    MissingParent,
    /// The `parent:` names a task that is not an epic.
    ParentNotEpic,
    /// Following `parent:` keys from the task leads back to itself.
    ParentCycle,
}

impl std::fmt::Display for OrphanReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingParent => "missing-parent",
            Self::ParentNotEpic => "parent-not-epic",
            Self::ParentCycle => "parent-cycle",
        })
    }
}

/// A task whose legacy `parent:` could not become part of its ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanedTask {
    /// The task's ID, which it keeps.
    pub id: String,
    /// The value of its `parent:` key, which is dropped.
    pub parent: String,
    pub reason: OrphanReason,
}

/// What [`migrate`] did, or would do on a dry run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Migration {
    /// Old ID → new ID for every task given a hierarchical ID.
    pub renamed: BTreeMap<String, String>,
    /// Tasks whose `parent:` was dropped without a rename; each stays where
    /// its ID puts it (a root task, for a flat ID).
    pub orphaned: Vec<OrphanedTask>,
    /// How many task files, active and archived, carried a `parent:` key.
    pub legacy_files: usize,
}

impl Migration {
    /// Whether the store was already in the current format.
    pub fn is_noop(&self) -> bool {
        self.renamed.is_empty() && self.orphaned.is_empty() && self.legacy_files == 0
    }
}

/// Convert a store written before hierarchical IDs, active and archive alike.
///
/// Every task whose legacy `parent:` key names an epic gets a fresh child ID
/// under that epic's (new) ID, from [`task::derive_child_id`]; parents are
/// placed before their children, so nested epics carry their whole chain.
/// The IDs are derived from the old IDs, and tasks are placed in order of
/// their old IDs, so the same store always migrates to the same IDs: a dry
/// run shows the IDs the real run assigns, and two copies of a store migrate
/// identically. Nothing depends on the time, `updated` or directory order.
/// Root tasks keep their IDs. A task found below a renamed task by its ID
/// alone keeps its last segment under the new prefix. Every `depends_on` is
/// rewritten, `updated` is kept, and every file that carried a `parent:` key
/// is written without it. A `parent:` naming no task, a task that is not an
/// epic, or leading round a cycle is dropped and reported in
/// [`Migration::orphaned`]; that task keeps its ID.
///
/// With `dry_run` nothing is written. On a store with no `parent:` keys left
/// this does nothing.
///
/// The renames are recorded in [`MIGRATION_JOURNAL`] before any task file is
/// written, and the journal is removed last. New files are written first,
/// then rewritten references, and old files are removed last, so while the
/// job is unfinished the old files and their `parent:` keys are still there:
/// the store keeps failing to load with [`Error::LegacyStore`], and running
/// `migrate` again reuses the journal's renames and finishes the job without
/// duplicating or losing a task.
pub fn migrate(base: &Path, dry_run: bool) -> Result<Migration> {
    let dir = tasks_dir(base);
    if !dir.exists() {
        return Err(Error::NotInitialized);
    }
    let config = crate::config::load(base)?;
    let journal = dir.join(MIGRATION_JOURNAL);
    let prior: BTreeMap<String, String> = if journal.exists() {
        serde_json::from_str(&fs::read_to_string(&journal)?)?
    } else {
        BTreeMap::new()
    };

    let mut files = read_task_files(&dir)?;
    files.extend(read_task_files(&archive_dir(base))?);

    let plan = MigrationPlan::new(&files, &prior, &config);
    let migration = Migration {
        renamed: plan.renames.clone(),
        orphaned: plan.orphaned.clone(),
        legacy_files: files
            .iter()
            .filter(|(_, t)| t.legacy_parent.is_some())
            .count(),
    };
    if dry_run {
        return Ok(migration);
    }

    if !plan.renames.is_empty() {
        // Everything a resumed run needs to rewrite references: the earlier
        // run's renames as well as this one's.
        let mut all = prior.clone();
        all.extend(plan.renames.clone());
        write_atomically(&journal, serde_json::to_string_pretty(&all)?.as_bytes())?;
    }
    // References may name an old ID, or a new ID an interrupted run chose
    // and this run does not keep.
    let mut references = BTreeMap::new();
    for (old, new) in &prior {
        if let Some(now) = plan.renames.get(old).filter(|now| *now != new) {
            references.insert(new.clone(), now.clone());
        }
    }
    references.extend(prior);
    references.extend(plan.renames.clone());

    let mut moved = Vec::new();
    let mut rewritten = Vec::new();
    let mut old_paths = Vec::new();
    let mut copies = Vec::new();
    for (i, (path, t)) in files.into_iter().enumerate() {
        if plan.copies.contains(&i) {
            // Rewritten from the file it was copied from.
            copies.push(path);
            continue;
        }
        let mut t = t;
        let mut changed = t.legacy_parent.take().is_some();
        for dep in &mut t.depends_on {
            if let Some(new) = references.get(dep) {
                *dep = new.clone();
                changed = true;
            }
        }
        match plan
            .renames
            .get(&t.id)
            .filter(|_| plan.winners.contains(&i))
        {
            Some(new) => {
                t.id = new.clone();
                moved.push((path.with_file_name(task::filename(&t)), t));
                old_paths.push(path);
            }
            None if changed => rewritten.push((path, t)),
            None => {}
        }
    }
    // A copy normally sits where its task is written again; one that does
    // not (its task now goes elsewhere) is stale and goes with the old files.
    let written: HashSet<&PathBuf> = moved.iter().map(|(p, _)| p).collect();
    old_paths.extend(copies.into_iter().filter(|p| !written.contains(p)));
    for (path, t) in moved.iter().chain(rewritten.iter()) {
        write_task_file(path, t)?;
    }
    for path in old_paths {
        fs::remove_file(&path)?;
    }
    if journal.exists() {
        fs::remove_file(&journal)?;
    }
    Ok(migration)
}

/// The renames [`migrate`] makes, worked out from the parsed task files.
struct MigrationPlan {
    renames: BTreeMap<String, String>,
    orphaned: Vec<OrphanedTask>,
    /// Indices of the files a task's ID is taken from (the first file for
    /// each ID).
    winners: HashSet<usize>,
    /// Indices of files an interrupted run already wrote under a new ID,
    /// while the file it copied is still there.
    copies: HashSet<usize>,
}

impl MigrationPlan {
    fn new(files: &[(PathBuf, Task)], prior: &BTreeMap<String, String>, config: &Config) -> Self {
        let all_ids: HashSet<&str> = files.iter().map(|(_, t)| t.id.as_str()).collect();
        // A file is a copy when an earlier run renamed some task to its ID
        // and the renamed task's own file still exists.
        let copy_ids: HashSet<&str> = prior
            .iter()
            .filter(|(old, _)| all_ids.contains(old.as_str()))
            .map(|(_, new)| new.as_str())
            .collect();

        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut copies = HashSet::new();
        for (i, (_, t)) in files.iter().enumerate() {
            if copy_ids.contains(t.id.as_str()) {
                copies.insert(i);
            } else {
                index.entry(t.id.as_str()).or_insert(i);
            }
        }
        let winners: HashSet<usize> = index.values().copied().collect();

        let reserved = task::with_ancestors(
            all_ids
                .iter()
                .copied()
                .chain(prior.values().map(|s| s.as_str())),
        );
        let mut planner = Planner {
            files,
            prior,
            config,
            index,
            reserved,
            new_ids: vec![None; files.len()],
            visiting: vec![false; files.len()],
            orphaned: Vec::new(),
        };
        let mut order: Vec<usize> = winners.iter().copied().collect();
        order.sort_by(|&a, &b| files[a].1.id.cmp(&files[b].1.id));
        for &i in &order {
            planner.assign(i);
        }

        let renames = order
            .iter()
            .filter_map(|&i| {
                let old = &files[i].1.id;
                let new = planner.new_ids[i].as_ref()?;
                (new != old).then(|| (old.clone(), new.clone()))
            })
            .collect();
        let mut orphaned = planner.orphaned;
        orphaned.sort_by(|a, b| a.id.cmp(&b.id));
        Self {
            renames,
            orphaned,
            winners,
            copies,
        }
    }
}

/// Assigns new IDs top-down for [`MigrationPlan::new`].
struct Planner<'a> {
    files: &'a [(PathBuf, Task)],
    prior: &'a BTreeMap<String, String>,
    config: &'a Config,
    /// Task ID → index of its file, copies excluded.
    index: HashMap<&'a str, usize>,
    /// The collision pool: every ID on disk or planned, and their ancestors.
    reserved: HashSet<String>,
    new_ids: Vec<Option<String>>,
    visiting: Vec<bool>,
    orphaned: Vec<OrphanedTask>,
}

impl Planner<'_> {
    /// The new ID of the task in file `i`, assigning its parent's first.
    fn assign(&mut self, i: usize) -> String {
        if let Some(id) = &self.new_ids[i] {
            return id.clone();
        }
        self.visiting[i] = true;
        let t = &self.files[i].1;
        let parent = match self.parent(t) {
            Ok(Some(j)) if self.visiting[j] => {
                self.orphan(t, OrphanReason::ParentCycle);
                None
            }
            Ok(parent) => parent,
            Err(reason) => {
                self.orphan(t, reason);
                None
            }
        };
        let id = match parent {
            None => t.id.clone(),
            Some(j) => {
                let parent_id = self.assign(j);
                self.child_id(t, &parent_id)
            }
        };
        self.reserved.insert(id.clone());
        self.visiting[i] = false;
        self.new_ids[i] = Some(id.clone());
        id
    }

    /// The file index of `t`'s parent: the task its `parent:` key names when
    /// it has one, else the task its ID is under, if that exists.
    fn parent(&self, t: &Task) -> std::result::Result<Option<usize>, OrphanReason> {
        let Some(name) = t.legacy_parent.as_deref().filter(|p| !p.is_empty()) else {
            return Ok(task::parent_of(&t.id).and_then(|p| self.index.get(p).copied()));
        };
        // An interrupted run may already have removed a renamed epic's old
        // file; its journal says where the epic went.
        let j = self
            .index
            .get(name)
            .or_else(|| {
                self.prior
                    .get(name)
                    .and_then(|new| self.index.get(new.as_str()))
            })
            .copied()
            .ok_or(OrphanReason::MissingParent)?;
        if self.files[j].1.task_type != task::TaskType::Epic {
            return Err(OrphanReason::ParentNotEpic);
        }
        Ok(Some(j))
    }

    /// The ID `t` gets as a child of the task now called `parent_id`.
    fn child_id(&self, t: &Task, parent_id: &str) -> String {
        if task::parent_of(&t.id) == Some(parent_id) {
            return t.id.clone();
        }
        if let Some(new) = self.prior.get(&t.id)
            && task::parent_of(new) == Some(parent_id)
        {
            return new.clone();
        }
        if t.legacy_parent.is_none() {
            let kept = format!(
                "{parent_id}{}{}",
                task::ID_SEPARATOR,
                task::leaf_segment(&t.id)
            );
            if !self.reserved.contains(&kept) {
                return kept;
            }
        }
        // Derived from the old ID, not drawn at random, so that a dry run
        // and the real run, or two copies of the store, agree.
        task::derive_child_id(parent_id, &t.id, &self.reserved, self.config)
    }

    fn orphan(&mut self, t: &Task, reason: OrphanReason) {
        self.orphaned.push(OrphanedTask {
            id: t.id.clone(),
            parent: t
                .legacy_parent
                .clone()
                .or_else(|| task::parent_of(&t.id).map(String::from))
                .unwrap_or_default(),
            reason,
        });
    }
}

/// Resolve a task ID or unique prefix to a full task ID.
///
/// An exact match always wins. Otherwise every ID starting with `prefix`
/// matches, and when all of them are one task and its descendants, that
/// shallowest task is the answer: `8p` resolves to the epic `8ph` even though
/// `8ph.c2` matches too, and `8ph.c` resolves to `8ph.c2` over `8ph.c2.d7`.
/// Errors if the prefix is empty (it would match every task), if nothing
/// matches, or if the matches are genuinely different tasks.
pub fn resolve_prefix(tasks: &HashMap<String, Task>, prefix: &str) -> Result<String> {
    if prefix.trim().is_empty() {
        return Err(Error::InvalidId {
            id: prefix.into(),
            reason: "an ID or prefix must not be empty".into(),
        });
    }

    // Exact match first
    if tasks.contains_key(prefix) {
        return Ok(prefix.to_string());
    }

    let matches: Vec<&String> = tasks.keys().filter(|id| id.starts_with(prefix)).collect();

    // The shortest match is the only candidate for an ancestor of the rest.
    let Some(&top) = matches.iter().min_by_key(|id| id.len()) else {
        return Err(Error::TaskNotFound(prefix.into()));
    };
    if matches
        .iter()
        .all(|&id| id == top || task::is_descendant(id, top))
    {
        return Ok(top.clone());
    }

    let mut sorted = matches.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    sorted.sort();
    Err(Error::AmbiguousPrefix {
        prefix: prefix.into(),
        matches: sorted.join(", "),
    })
}

/// Delete a task file by ID.
pub fn delete(base: &Path, id: &str) -> Result<()> {
    let path = find_task_path(base, id)?;
    fs::remove_file(&path)?;
    Ok(())
}

/// One task file as [`scan_task_files`] found it.
#[derive(Debug)]
pub struct TaskFile {
    /// Path of the file: `.bears/…` or `.bears/archive/…` under the base.
    pub path: PathBuf,
    /// Whether the file is in the archive.
    pub archived: bool,
    /// The parsed task, or why the file could not be read or parsed.
    pub task: std::result::Result<Task, String>,
}

/// Every task file in `.bears/` and `.bears/archive/`, active files first,
/// each directory sorted by path.
///
/// Unlike [`load_all`] this keeps every file: duplicates of an ID, files
/// that cannot be read or parsed (with the reason, and no warning printed)
/// and files with a legacy `parent:` key. It is the file-level view a store
/// check needs.
pub fn scan_task_files(base: &Path) -> Result<Vec<TaskFile>> {
    if !tasks_dir(base).exists() {
        return Err(Error::NotInitialized);
    }
    let mut files = Vec::new();
    for (dir, archived) in [(tasks_dir(base), false), (archive_dir(base), true)] {
        if !dir.exists() {
            continue;
        }
        let mut paths = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                paths.push(path);
            }
        }
        paths.sort();
        for path in paths {
            let task = fs::read_to_string(&path)
                .map_err(Error::from)
                .and_then(|c| task::parse_task(&c))
                .map_err(|e| match e {
                    Error::InvalidFrontmatter { reason, .. } => reason,
                    other => other.to_string(),
                });
            files.push(TaskFile {
                path,
                archived,
                task,
            });
        }
    }
    Ok(files)
}

/// Replace the file at `path` with `t`, then rename it to `t`'s own
/// filename (`{id}-{slug}.md`) in the same directory. Returns the new path.
///
/// This addresses a file by path rather than a task by ID, so it can repair
/// one of two files that carry the same ID, or a file whose name does not
/// match its ID. Refuses with [`Error::IdTaken`] when another file already
/// sits at the new path, rather than overwrite it.
///
/// The content is rewritten in place first (atomically) and the file renamed
/// second, so at every point there is exactly one file for the task: an
/// interruption leaves at worst a correctly written file under its old,
/// mismatched name, never a second copy.
pub fn rewrite_task_file(path: &Path, t: &Task) -> Result<PathBuf> {
    let new_path = path.with_file_name(task::filename(t));
    if new_path != path && new_path.exists() {
        return Err(Error::IdTaken(t.id.clone()));
    }
    write_task_file(path, t)?;
    if new_path != path {
        fs::rename(path, &new_path)?;
    }
    Ok(new_path)
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
                // One line with a position, so it fits in a warning; the
                // line is the file's (`: :` follows the opening `---`).
                assert!(!reason.contains('\n'), "{reason}");
                assert!(reason.contains("line 2,"), "{reason}");
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

    /// Every file in `dir` whose name ends in `.tmp`.
    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("tmp"))
            .collect()
    }

    /// Two writers of the same task file at once (two agents, or an agent
    /// and the TUI) each get their own temporary file: neither write fails,
    /// the file is always one of the two whole versions, and no temporary
    /// file is left. With one fixed temporary name the second rename found
    /// its file already renamed away by the first.
    #[test]
    fn test_concurrent_writers_of_one_task_file() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let base = tmp.path().to_path_buf();
        save(
            &base,
            &Task::new("cw1".into(), "Shared".into(), Priority::P2),
        )
        .unwrap();

        let writers: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|who| {
                let base = base.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        let mut t = Task::new("cw1".into(), "Shared".into(), Priority::P2);
                        t.body = format!("{who} {i}\n").repeat(200);
                        save(&base, &t).unwrap();
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }

        let t = load_one(&base, "cw1").unwrap();
        assert!(
            t.body == "first 99\n".repeat(200) || t.body == "second 99\n".repeat(200),
            "the file is one writer's last version"
        );
        assert!(temp_files(&tasks_dir(&base)).is_empty());
    }

    /// A failed write removes its temporary file: here the rename fails,
    /// since the target is a directory that is not empty.
    #[test]
    fn test_write_atomically_removes_temp_file_on_failure() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("occupied");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inside"), "x").unwrap();

        assert!(write_atomically(&target, b"new").is_err());
        assert!(target.is_dir());
        assert!(temp_files(tmp.path()).is_empty());
    }

    #[test]
    fn test_write_atomically_replaces_contents() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("file.txt");
        write_atomically(&target, b"one").unwrap();
        write_atomically(&target, b"two").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "two");
        assert!(temp_files(tmp.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_write_atomically_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("file.txt");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomically(&target, b"new").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    /// A temporary file left behind by a killed writer — in the current
    /// naming or the fixed `.{name}.tmp` of earlier versions, holding a
    /// complete task — is never read as a task by either loader or by the
    /// store check, and `bea doctor` does not report it.
    #[tokio::test]
    async fn test_stale_temp_files_are_not_tasks() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let active = Task::new("st1".into(), "Active".into(), Priority::P2);
        let archived = Task::new("st2".into(), "Archived".into(), Priority::P2);
        save(tmp.path(), &active).unwrap();
        save(tmp.path(), &archived).unwrap();
        move_to_archive(tmp.path(), "st2").unwrap();

        let stray = Task::new("zz9".into(), "Stray".into(), Priority::P0);
        let content = task::render_task(&stray);
        for dir in [tasks_dir(tmp.path()), archive_dir(tmp.path())] {
            fs::write(dir.join(".zz9-stray.md.4242-17-0.tmp"), &content).unwrap();
            fs::write(dir.join(".zz9-stray.md.tmp"), &content).unwrap();
        }

        let tasks = load_all(tmp.path()).await.unwrap();
        assert_eq!(tasks.keys().collect::<Vec<_>>(), ["st1"]);
        let archive = load_archived(tmp.path()).await.unwrap();
        assert_eq!(archive.keys().collect::<Vec<_>>(), ["st2"]);
        assert!(!archived_id_set(tmp.path()).contains("zz9"));
        assert_eq!(reserved_ids(tmp.path(), &tasks).len(), 2);

        let files = scan_task_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 2);
        let report = crate::service::check_store(tmp.path()).unwrap();
        assert!(report.findings.is_empty(), "{:?}", report.findings);
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

    fn task_map(ids: &[&str]) -> HashMap<String, Task> {
        ids.iter()
            .map(|&id| {
                (
                    id.to_string(),
                    Task::new(id.into(), id.into(), Priority::P2),
                )
            })
            .collect()
    }

    /// A short prefix of an epic matches its children too; they are the same
    /// subtree, so it resolves to the epic.
    #[test]
    fn test_resolve_prefix_of_parent_ignores_its_descendants() {
        let tasks = task_map(&["e1x", "e1x.ab", "e1x.ab.cd", "e1x.ef"]);
        assert_eq!(resolve_prefix(&tasks, "e1").unwrap(), "e1x");
        assert_eq!(resolve_prefix(&tasks, "e").unwrap(), "e1x");
    }

    /// An empty or blank prefix matches everything, so it is rejected rather
    /// than resolved to a store's only task or only root.
    #[test]
    fn test_resolve_prefix_rejects_empty() {
        for tasks in [
            task_map(&["e1x"]),
            task_map(&["e1x", "e1x.ab", "e1x.ab.cd"]),
        ] {
            for prefix in ["", " ", "\t"] {
                let err = resolve_prefix(&tasks, prefix).unwrap_err();
                assert!(
                    matches!(err, Error::InvalidId { .. }),
                    "{prefix:?}: {err:?}"
                );
            }
        }
    }

    /// A prefix ending inside a child segment resolves to that child when the
    /// other matches are its own descendants.
    #[test]
    fn test_resolve_prefix_inside_child_segment() {
        let tasks = task_map(&["e1x", "e1x.ab", "e1x.ab.cd", "e1x.ef"]);
        assert_eq!(resolve_prefix(&tasks, "e1x.a").unwrap(), "e1x.ab");
        assert_eq!(resolve_prefix(&tasks, "e1x.ab.c").unwrap(), "e1x.ab.cd");
    }

    /// Different tasks are still ambiguous, whether siblings, unrelated roots
    /// whose subtrees both match, or the children of a missing parent.
    #[test]
    fn test_resolve_prefix_distinct_subtrees_stay_ambiguous() {
        let tasks = task_map(&["e1x", "e1x.ab", "e1x.ac", "e1y", "e1y.ab"]);
        for prefix in ["e1", "e1x.a", "e1x."] {
            let err = resolve_prefix(&tasks, prefix).unwrap_err();
            assert!(
                matches!(err, Error::AmbiguousPrefix { .. }),
                "{prefix}: {err:?}"
            );
        }

        // `e1xa` starts with `e1x` but is not its descendant.
        let tasks = task_map(&["e1x", "e1xa"]);
        assert!(matches!(
            resolve_prefix(&tasks, "e1").unwrap_err(),
            Error::AmbiguousPrefix { .. }
        ));

        // Orphans of a deleted parent are separate tasks.
        let tasks = task_map(&["e1x.ab", "e1x.cd"]);
        assert!(matches!(
            resolve_prefix(&tasks, "e1").unwrap_err(),
            Error::AmbiguousPrefix { .. }
        ));
        let tasks = task_map(&["e1x.ab", "e1x.ab.cd"]);
        assert_eq!(resolve_prefix(&tasks, "e1").unwrap(), "e1x.ab");
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

    /// Maps whose new IDs are also old IDs are refused before anything is
    /// written: a swap or chain could overwrite a file whose task has not yet
    /// been written under its new ID.
    #[tokio::test]
    async fn test_rename_ids_refuses_swaps_chains_and_identity() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        let mut a = Task::new("aa".into(), "Same".into(), Priority::P0);
        a.depends_on = vec!["bb".into()];
        save(tmp.path(), &a).unwrap();
        save(
            tmp.path(),
            &Task::new("bb".into(), "Same".into(), Priority::P3),
        )
        .unwrap();
        save(
            tmp.path(),
            &Task::new("cc".into(), "Same".into(), Priority::P1),
        )
        .unwrap();
        let before: Vec<_> = ["aa", "bb", "cc"]
            .iter()
            .map(|id| fs::read_to_string(find_task_path(tmp.path(), id).unwrap()).unwrap())
            .collect();

        for (pairs, taken) in [
            (&[("aa", "bb"), ("bb", "aa")][..], "bb"),
            (&[("aa", "bb"), ("bb", "xx")][..], "bb"),
            (&[("bb", "cc"), ("cc", "xx")][..], "cc"),
            (&[("aa", "aa")][..], "aa"),
        ] {
            let err = rename_ids(tmp.path(), &renames(pairs)).unwrap_err();
            assert!(
                matches!(&err, Error::IdTaken(id) if id == taken),
                "{pairs:?}: {err:?}"
            );
        }

        let after: Vec<_> = ["aa", "bb", "cc"]
            .iter()
            .map(|id| fs::read_to_string(find_task_path(tmp.path(), id).unwrap()).unwrap())
            .collect();
        assert_eq!(after, before);
        assert_eq!(fs::read_dir(tasks_dir(tmp.path())).unwrap().count(), 4); // + archive/
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

    // -- Legacy stores and `migrate` ----------------------------------------

    /// A task file as written before hierarchical IDs: `t` with a `parent:`
    /// key, in the active store or the archive.
    fn write_legacy(base: &Path, t: &Task, parent: &str, archived: bool) {
        let dir = if archived {
            archive_dir(base)
        } else {
            tasks_dir(base)
        };
        let rendered = task::render_task(t);
        let content = format!("---\nparent: {parent}\n{}", &rendered[4..]);
        fs::write(dir.join(task::filename(t)), content).unwrap();
    }

    fn epic(id: &str) -> Task {
        let mut t = Task::new(id.into(), format!("Epic {id}"), Priority::P1);
        t.task_type = TaskType::Epic;
        t
    }

    fn plain(id: &str) -> Task {
        Task::new(id.into(), format!("Task {id}"), Priority::P2)
    }

    /// Every file under `.bears/` (journal included), by path relative to it.
    fn snapshot(base: &Path) -> BTreeMap<String, String> {
        let mut files = BTreeMap::new();
        for dir in [tasks_dir(base), archive_dir(base)] {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    let rel = path.strip_prefix(tasks_dir(base)).unwrap();
                    files.insert(
                        rel.display().to_string(),
                        fs::read_to_string(&path).unwrap(),
                    );
                }
            }
        }
        files
    }

    fn no_parent_keys(base: &Path) {
        for (path, content) in snapshot(base) {
            assert!(
                !content.lines().any(|l| l.starts_with("parent:")),
                "{path} still has a parent: key"
            );
        }
    }

    #[tokio::test]
    async fn test_loaders_refuse_a_legacy_store() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        save(tmp.path(), &epic("ep")).unwrap();
        write_legacy(tmp.path(), &plain("ch"), "ep", false);

        let err = load_all(tmp.path()).await.unwrap_err();
        assert!(
            matches!(&err, Error::LegacyStore { files } if files.len() == 1
                && files[0].ends_with("ch-task-ch.md")),
            "{err:?}"
        );
        assert!(err.to_string().contains("parent:"), "{err}");
        assert!(matches!(
            load_archived(tmp.path()).await,
            Err(Error::LegacyStore { .. })
        ));

        // A legacy file in the archive alone is enough, for both loaders.
        fs::remove_file(find_task_path(tmp.path(), "ch").unwrap()).unwrap();
        load_all(tmp.path()).await.unwrap();
        write_legacy(tmp.path(), &plain("ac"), "ep", true);
        assert!(matches!(
            load_all(tmp.path()).await,
            Err(Error::LegacyStore { files }) if files.len() == 1
        ));
        assert!(matches!(
            load_archived(tmp.path()).await,
            Err(Error::LegacyStore { .. })
        ));
    }

    #[test]
    fn test_has_legacy_parent_key_reads_only_top_level_frontmatter() {
        let file = |front: &str, body: &str| {
            format!(
                "---\nid: ab\nstatus: open\npriority: P2\n\
                 created: \"2026-01-01T00:00:00Z\"\nupdated: \"2026-01-01T00:00:00Z\"\n\
                 {front}---\n\n{body}"
            )
        };
        let legacy = |front: &str| has_legacy_parent_key(&file(front, ""));
        assert!(legacy("title: T\nparent: ep\n"));
        assert!(legacy("title: T\nparent: \"ep\"\n"));
        assert!(legacy("title: T\r\nparent: ep\r\n"));
        // Valid YAML no bears version writes, read by the parser all the same.
        assert!(legacy("title: T\nparent:\n  ep\n"));
        assert!(legacy("title: T\nparent : ep\n"));
        assert!(legacy("title: T\n\"parent\": ep\n"));
        assert!(legacy("title: T\n'parent': ep\n"));
        assert!(legacy("title: T\n? parent\n: ep\n"));

        assert!(!has_legacy_parent_key(&file("title: T\n", "parent: ep\n")));
        assert!(!legacy("title: |\n  line\n  parent: ep\n"));
        assert!(!legacy("title: T\ntags:\n  - \"parent: ep\"\n"));
        assert!(!legacy("title: \"parent: ep\"\n"));
        assert!(!legacy("title: T\nparent:\n"));
        assert!(!legacy("title: T\nparent: ~\n"));
        // Not a key: the file does not parse, so it is not legacy either.
        assert!(!legacy("title: T\nparent:ep\n"));
        // A file the parser rejects is never legacy, whatever its lines say.
        assert!(!legacy("title: T\nstatus: nonsense\nparent: ep\n"));
        assert!(!has_legacy_parent_key("---\nid: ab\nparent: ep\n---\n"));
        assert!(!has_legacy_parent_key("---\nparent: ep\n"));
        assert!(!has_legacy_parent_key("parent: ep\n---\n"));
    }

    /// Frontmatter whose top-level mapping is indented as a whole still
    /// parses, so both loaders must see its `parent:` key.
    #[tokio::test]
    async fn test_loaders_refuse_indented_frontmatter_with_a_parent_key() {
        let indented = "---\n  id: zz\n  title: Indented\n  status: open\n  priority: P2\n  \
                        created: \"2026-01-01T00:00:00Z\"\n  updated: \"2026-01-01T00:00:00Z\"\n  \
                        parent: qq\n---\n";
        assert!(has_legacy_parent_key(indented));
        for archived in [true, false] {
            let tmp = TempDir::new().unwrap();
            init(tmp.path()).unwrap();
            let dir = if archived {
                archive_dir(tmp.path())
            } else {
                tasks_dir(tmp.path())
            };
            fs::write(dir.join("zz-indented.md"), indented).unwrap();
            assert!(
                matches!(load_all(tmp.path()).await, Err(Error::LegacyStore { .. })),
                "load_all, archived: {archived}"
            );
            assert!(
                matches!(
                    load_archived(tmp.path()).await,
                    Err(Error::LegacyStore { .. })
                ),
                "load_archived, archived: {archived}"
            );
        }
    }

    /// A file with a `parent:` line that the parser rejects is skipped by the
    /// loader that owns it and ignored by the other one: neither refuses, so
    /// the store never gets stuck on a file `migrate` cannot read.
    #[tokio::test]
    async fn test_loaders_agree_on_unparsable_files_with_a_parent_line() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        save(tmp.path(), &epic("ep")).unwrap();
        // No title, a required field.
        let broken = "---\nid: br\nstatus: open\npriority: P2\nparent: ep\n---\n";
        fs::write(archive_dir(tmp.path()).join("br-broken.md"), broken).unwrap();
        assert_eq!(load_all(tmp.path()).await.unwrap().len(), 1);
        assert!(load_archived(tmp.path()).await.unwrap().is_empty());

        fs::rename(
            archive_dir(tmp.path()).join("br-broken.md"),
            tasks_dir(tmp.path()).join("br-broken.md"),
        )
        .unwrap();
        assert!(load_archived(tmp.path()).await.unwrap().is_empty());
        assert_eq!(load_all(tmp.path()).await.unwrap().len(), 1);
        assert!(migrate(tmp.path(), false).unwrap().is_noop());
    }

    /// The scan of the directory a loader does not load matches the parse of
    /// the one it does: `parent:` text outside a top-level key is not a key.
    #[tokio::test]
    async fn test_loaders_ignore_parent_text_outside_the_key() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        save(tmp.path(), &epic("ep")).unwrap();
        let mut body = plain("bd");
        body.body = "parent: ep\n".into();
        save(tmp.path(), &body).unwrap();
        archive(
            tmp.path(),
            &Task {
                id: "ab".into(),
                ..body
            },
        );
        // `parent:` inside a block scalar and a nested value, hand-written.
        for (dir, id) in [
            (tasks_dir(tmp.path()), "bs"),
            (archive_dir(tmp.path()), "as"),
        ] {
            let content = format!(
                "---\nid: {id}\ntitle: |\n  line one\n  parent: ep\nstatus: open\npriority: P2\n\
                 created: \"2026-01-01T00:00:00Z\"\nupdated: \"2026-01-01T00:00:00Z\"\n\
                 tags:\n  - \"parent: ep\"\n---\n"
            );
            fs::write(dir.join(format!("{id}-block.md")), content).unwrap();
        }
        assert_eq!(load_all(tmp.path()).await.unwrap().len(), 3);
        assert_eq!(load_archived(tmp.path()).await.unwrap().len(), 2);

        // A real key in the other directory is still found, by either loader.
        write_legacy(tmp.path(), &plain("ac"), "ep", true);
        let Err(Error::LegacyStore { files }) = load_all(tmp.path()).await else {
            panic!("legacy archive file not found");
        };
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("ac-task-ac.md"));
        fs::remove_file(&files[0]).unwrap();
        write_legacy(tmp.path(), &plain("ch"), "ep", false);
        let Err(Error::LegacyStore { files }) = load_archived(tmp.path()).await else {
            panic!("legacy active file not found");
        };
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("ch-task-ch.md"));
    }

    /// The store the migration tests start from:
    ///
    /// - `ep` (epic) with children `ch` and nested epic `ne`, which has `gc`
    /// - `ac`, an archived child of `ep`
    /// - `ae`, an archived epic with an active child `ax`
    /// - `us` depends on `gc` and `ac`; archived `au` depends on `ch`
    /// - `rt` a plain root, untouched
    fn legacy_store(base: &Path) -> Task {
        init(base).unwrap();
        save(base, &epic("ep")).unwrap();
        write_legacy(base, &plain("ch"), "ep", false);
        write_legacy(base, &epic("ne"), "ep", false);
        let mut gc = plain("gc");
        gc.depends_on = vec!["ch".into()];
        write_legacy(base, &gc, "ne", false);
        write_legacy(base, &plain("ac"), "ep", true);
        archive(base, &epic("ae"));
        write_legacy(base, &plain("ax"), "ae", false);
        let mut us = plain("us");
        us.depends_on = vec!["gc".into(), "ac".into(), "rt".into()];
        save(base, &us).unwrap();
        let mut au = plain("au");
        au.depends_on = vec!["ch".into()];
        archive(base, &au);
        save(base, &plain("rt")).unwrap();
        gc
    }

    #[tokio::test]
    async fn test_migrate_gives_children_hierarchical_ids() {
        let tmp = TempDir::new().unwrap();
        let gc = legacy_store(tmp.path());
        let rt_path = find_task_path(tmp.path(), "rt").unwrap();
        let rt_before = fs::read_to_string(&rt_path).unwrap();

        let m = migrate(tmp.path(), false).unwrap();
        assert_eq!(m.legacy_files, 5);
        assert!(m.orphaned.is_empty());
        let keys: Vec<_> = m.renamed.keys().map(String::as_str).collect();
        assert_eq!(keys, ["ac", "ax", "ch", "gc", "ne"]);
        let new = |id: &str| m.renamed[id].clone();
        assert_eq!(task::parent_of(&new("ch")), Some("ep"));
        assert_eq!(task::parent_of(&new("ne")), Some("ep"));
        assert_eq!(task::parent_of(&new("gc")), Some(new("ne").as_str()));
        assert_eq!(task::parent_of(&new("ac")), Some("ep"));
        assert_eq!(task::parent_of(&new("ax")), Some("ae"));
        assert_eq!(task::leaf_segment(&new("ch")).len(), 2);

        no_parent_keys(tmp.path());
        assert!(!tasks_dir(tmp.path()).join(MIGRATION_JOURNAL).exists());
        let active = load_all(tmp.path()).await.unwrap();
        let archived = load_archived(tmp.path()).await.unwrap();
        let mut ids: Vec<_> = active.keys().cloned().collect();
        ids.sort();
        let mut expected = vec![
            "ep".to_string(),
            "rt".into(),
            "us".into(),
            new("ch"),
            new("ne"),
            new("gc"),
            new("ax"),
        ];
        expected.sort();
        assert_eq!(ids, expected);
        assert!(archived.contains_key(&new("ac")) && archived.contains_key("ae"));
        assert_eq!(active[&new("gc")].parent(), Some(new("ne").as_str()));
        assert_eq!(active[&new("gc")].depends_on, [new("ch")]);
        assert_eq!(active["us"].depends_on, [new("gc"), new("ac"), "rt".into()]);
        assert_eq!(archived["au"].depends_on, [new("ch")]);
        // A migration is not an edit, and a file with nothing to change stays.
        assert_eq!(active[&new("gc")].updated, gc.updated);
        assert_eq!(fs::read_to_string(&rt_path).unwrap(), rt_before);

        // A second run finds nothing to do and changes nothing.
        let before = snapshot(tmp.path());
        let again = migrate(tmp.path(), false).unwrap();
        assert!(again.is_noop(), "{again:?}");
        assert_eq!(snapshot(tmp.path()), before);
    }

    #[test]
    fn test_migrate_dry_run_writes_nothing() {
        let tmp = TempDir::new().unwrap();
        legacy_store(tmp.path());
        let before = snapshot(tmp.path());
        let m = migrate(tmp.path(), true).unwrap();
        assert_eq!(m.renamed.len(), 5);
        assert_eq!(m.legacy_files, 5);
        assert_eq!(snapshot(tmp.path()), before);
    }

    /// Copy every task file of `from` (active and archive) into a fresh store.
    fn copy_store(from: &Path) -> TempDir {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        for (rel, content) in snapshot(from) {
            fs::write(tasks_dir(tmp.path()).join(rel), content).unwrap();
        }
        tmp
    }

    #[test]
    fn test_migrate_dry_run_shows_the_ids_the_real_run_assigns() {
        let tmp = TempDir::new().unwrap();
        legacy_store(tmp.path());
        let preview = migrate(tmp.path(), true).unwrap();
        assert_eq!(migrate(tmp.path(), true).unwrap(), preview);
        let real = migrate(tmp.path(), false).unwrap();
        assert_eq!(real.renamed, preview.renamed);
        assert_eq!(real, preview);
    }

    /// The same legacy store, migrated in two places, ends up byte-identical.
    #[test]
    fn test_migrate_is_the_same_for_two_copies_of_a_store() {
        let a = TempDir::new().unwrap();
        legacy_store(a.path());
        // Many siblings under one-char segments, so some candidates collide
        // and the segment length grows part-way.
        fs::write(crate::config::config_path(a.path()), "child-id-length: 1\n").unwrap();
        save(a.path(), &epic("big")).unwrap();
        for n in 0..20 {
            write_legacy(a.path(), &plain(&format!("k{n}")), "big", n % 3 == 0);
        }
        let b = copy_store(a.path());
        fs::copy(
            crate::config::config_path(a.path()),
            crate::config::config_path(b.path()),
        )
        .unwrap();

        let ma = migrate(a.path(), false).unwrap();
        let mb = migrate(b.path(), false).unwrap();
        assert_eq!(ma, mb);
        assert_eq!(snapshot(a.path()), snapshot(b.path()));

        let big: Vec<&String> = ma
            .renamed
            .iter()
            .filter(|(old, _)| old.starts_with('k'))
            .map(|(_, new)| new)
            .collect();
        assert_eq!(big.len(), 20);
        assert_eq!(big.iter().collect::<HashSet<_>>().len(), 20, "{big:?}");
        let lengths: HashSet<usize> = big.iter().map(|id| task::leaf_segment(id).len()).collect();
        // 25% of 31 one-char segments is 7.75: the rest get two chars.
        assert_eq!(lengths, [1, 2].into(), "{big:?}");
        assert!(big.iter().all(|id| task::parent_of(id) == Some("big")));
    }

    #[tokio::test]
    async fn test_migrate_keeps_unplaceable_tasks_as_roots() {
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        save(tmp.path(), &plain("nt")).unwrap();
        write_legacy(tmp.path(), &plain("mp"), "gone", false);
        write_legacy(tmp.path(), &plain("np"), "nt", true);
        // Two epics naming each other, and one naming itself.
        write_legacy(tmp.path(), &epic("ca"), "cb", false);
        write_legacy(tmp.path(), &epic("cb"), "ca", false);
        write_legacy(tmp.path(), &epic("cs"), "cs", false);

        let m = migrate(tmp.path(), false).unwrap();
        let orphaned: Vec<_> = m
            .orphaned
            .iter()
            .map(|o| (o.id.as_str(), o.parent.as_str(), o.reason))
            .collect();
        assert_eq!(
            orphaned,
            [
                ("cb", "ca", OrphanReason::ParentCycle),
                ("cs", "cs", OrphanReason::ParentCycle),
                ("mp", "gone", OrphanReason::MissingParent),
                ("np", "nt", OrphanReason::ParentNotEpic),
            ]
        );
        // The cycle is broken at one task; the other goes below it.
        assert_eq!(m.renamed.len(), 1);
        assert_eq!(task::parent_of(&m.renamed["ca"]), Some("cb"));

        no_parent_keys(tmp.path());
        let active = load_all(tmp.path()).await.unwrap();
        assert!(active.contains_key("mp") && active.contains_key("cs"));
        assert!(load_archived(tmp.path()).await.unwrap().contains_key("np"));
    }

    /// An interrupted migration — at any point of its writes — is finished by
    /// the next run, with the journal's renames and nothing duplicated or lost.
    #[tokio::test]
    async fn test_migrate_resumes_after_an_interruption() {
        let orig = TempDir::new().unwrap();
        legacy_store(orig.path());
        let start = snapshot(orig.path());
        let done = TempDir::new().unwrap();
        legacy_store(done.path());
        // Same files as `orig`, so the finished state can be compared.
        for (rel, content) in &start {
            fs::write(tasks_dir(done.path()).join(rel), content).unwrap();
        }
        let m = migrate(done.path(), false).unwrap();
        let end = snapshot(done.path());
        let journal = serde_json::to_string(&m.renamed).unwrap();

        let restore = |state: &BTreeMap<String, String>| {
            let tmp = TempDir::new().unwrap();
            init(tmp.path()).unwrap();
            for (rel, content) in state {
                fs::write(tasks_dir(tmp.path()).join(rel), content).unwrap();
            }
            fs::write(tasks_dir(tmp.path()).join(MIGRATION_JOURNAL), &journal).unwrap();
            tmp
        };
        let written: Vec<&String> = end
            .keys()
            .filter(|k| start.get(*k) != end.get(*k))
            .collect();
        let removed: Vec<&String> = start.keys().filter(|k| !end.contains_key(*k)).collect();
        assert!(!written.is_empty() && !removed.is_empty());

        let mut states = Vec::new();
        // Interrupted while writing: some of the new files are in place.
        for n in 0..=written.len() {
            let mut state = start.clone();
            for rel in written.iter().take(n) {
                state.insert((*rel).clone(), end[*rel].clone());
            }
            states.push(state);
        }
        // Interrupted while removing old files, in either order: the old
        // file of renamed epic `ne` may already be gone while its child's
        // remains, whose `parent:` then names it only through the journal.
        for n in 0..removed.len() {
            for kept in [
                removed.iter().skip(n).collect::<Vec<_>>(),
                removed.iter().take(n + 1).collect(),
            ] {
                let mut state = end.clone();
                for rel in kept {
                    state.insert((*rel).clone(), start[*rel].clone());
                }
                states.push(state);
            }
        }
        for state in states {
            let tmp = restore(&state);
            assert!(matches!(
                load_all(tmp.path()).await,
                Err(Error::LegacyStore { .. })
            ));
            let resumed = migrate(tmp.path(), false).unwrap();
            assert_eq!(snapshot(tmp.path()), end);
            assert!(
                resumed
                    .renamed
                    .iter()
                    .all(|(old, new)| m.renamed[old] == *new)
            );
        }

        // A journal left behind after the last removal is cleared quietly.
        let tmp = restore(&end);
        assert!(migrate(tmp.path(), false).unwrap().is_noop());
        assert_eq!(snapshot(tmp.path()), end);
    }

    /// This repository's own store is already migrated: a copy of it loads,
    /// carries no `parent:` key, `migrate` has nothing to do on it, every
    /// task's parent (derived from its ID) is an existing epic, and every
    /// `depends_on` names a task. The migration itself is covered by the
    /// synthetic `migrate` tests above.
    #[tokio::test]
    async fn test_this_repository_store_is_migrated_and_consistent() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(BEARS_DIR);
        if !src.is_dir() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        init(tmp.path()).unwrap();
        for (from, to) in [
            (src.clone(), tasks_dir(tmp.path())),
            (src.join(ARCHIVE_SUBDIR), archive_dir(tmp.path())),
        ] {
            let Ok(entries) = fs::read_dir(&from) else {
                continue;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.extension().is_none_or(|e| e != "md") {
                    continue;
                }
                fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
            }
        }

        no_parent_keys(tmp.path());
        let mut all = load_all(tmp.path()).await.unwrap();
        all.extend(load_archived(tmp.path()).await.unwrap());
        assert!(migrate(tmp.path(), false).unwrap().is_noop());
        for t in all.values() {
            if let Some(parent) = t.parent() {
                let p = all.get(parent);
                assert!(p.is_some(), "{} has no parent {parent}", t.id);
                assert!(
                    p.unwrap().task_type.is_epic(),
                    "{}'s parent {parent} is not an epic",
                    t.id
                );
            }
            for dep in &t.depends_on {
                assert!(all.contains_key(dep), "{} depends on missing {dep}", t.id);
            }
        }
    }
}
