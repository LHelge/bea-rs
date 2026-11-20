//! Scaffolding framework for coding-harness integration files.
//!
//! Each harness (Claude, Copilot, Codex) is described by a static registry
//! entry: a list of `(target relative path, embedded content)` pairs and an
//! MCP-registration strategy.  Adding a new harness is a data change — no new
//! control flow required.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::Result;

// ── Embedded templates ───────────────────────────────────────────────────────

/// Embedded template content for the Claude `CLAUDE.md` instruction file.
const CLAUDE_MD: &str = include_str!("../templates/claude/CLAUDE.md");

/// Embedded template for the Claude MCP server entry (used as a seed when no
/// `.mcp.json` exists; for existing files we merge only the `bears` entry).
const CLAUDE_MCP_SEED: &str = include_str!("../templates/claude/mcp.json");

/// Embedded template for the Claude bears-planning skill.
const CLAUDE_SKILL_MD: &str = include_str!("../templates/claude/skills/bears-planning/SKILL.md");

/// Embedded CLI fallback reference for the Claude bears-planning skill.
const CLAUDE_SKILL_CLI_FALLBACK: &str =
    include_str!("../templates/claude/skills/bears-planning/references/cli-fallback.md");

/// Embedded template for the Claude planner agent.
const CLAUDE_AGENT_PLANNER: &str = include_str!("../templates/claude/agents/planner.md");

/// Embedded template content for the Copilot instruction file.
const COPILOT_MD: &str = include_str!("../templates/copilot/copilot-instructions.md");

/// Embedded template for the Copilot MCP server entry seed.
const COPILOT_MCP_SEED: &str = include_str!("../templates/copilot/mcp.json");

/// Embedded template for the Copilot bears-planning skill.
const COPILOT_SKILL_MD: &str = include_str!("../templates/copilot/skills/bears-planning/SKILL.md");

/// Embedded CLI fallback reference for the Copilot bears-planning skill.
const COPILOT_SKILL_CLI_FALLBACK: &str =
    include_str!("../templates/copilot/skills/bears-planning/references/cli-fallback.md");

/// Embedded template for the Copilot planner agent.
const COPILOT_AGENT_PLANNER: &str = include_str!("../templates/copilot/agents/planner.agent.md");

/// Embedded template content for the Codex `AGENTS.md` instruction file.
const CODEX_MD: &str = include_str!("../templates/codex/AGENTS.md");

// ── Session-start hook ───────────────────────────────────────────────────────

/// The shell command of the Claude Code `SessionStart` hook that `bea init
/// --claude` merges into `.claude/settings.json`.
///
/// Claude Code runs a hook in the session's *current* directory, which can
/// differ from the project root after a `cd`, so the command first changes to
/// `$CLAUDE_PROJECT_DIR` (the project root, set by Claude Code for hooks), or
/// stays where it is when the variable is unset. A project without a `.bears/`
/// store is no error: the hook then prints nothing and succeeds.
pub const PRIME_HOOK_COMMAND: &str =
    "cd \"${CLAUDE_PROJECT_DIR:-.}\" && { test ! -d .bears || bea prime; }";

/// Text by which an existing hook handler is recognised as the bears hook, in
/// this form or an older one (e.g. `test ! -d .bears || bea prime`).
const PRIME_HOOK_MARKER: &str = "bea prime";

/// The Claude Code project settings file that carries the hook.
const CLAUDE_SETTINGS: &str = ".claude/settings.json";

/// What [`merge_session_hook`] did with the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookMerge {
    /// The file was created or changed.
    Written,
    /// The file already held the hook in its current form; nothing was written.
    Unchanged,
    /// The file was left untouched because it does not have the expected shape
    /// (the reason says what was wrong); the hook was not added.
    Skipped(String),
}

/// The bears `SessionStart` matcher group: no matcher (every session start),
/// one command handler running [`PRIME_HOOK_COMMAND`].
fn prime_hook_group() -> Value {
    serde_json::json!({
        "hooks": [{ "type": "command", "command": PRIME_HOOK_COMMAND }]
    })
}

/// Whether a hook handler is the bears hook, current or older form.
fn is_prime_handler(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(PRIME_HOOK_MARKER))
}

/// Merge the bears `SessionStart` hook into the Claude Code settings file at
/// `path`.
///
/// - A missing file is created holding only the hook.
/// - Otherwise every other key, hook event and `SessionStart` entry is kept.
///   Every handler recognised as the bears hook (its command contains
///   `bea prime`) is removed, a matcher group left empty by that is dropped,
///   and the current bears group is put where the first of them was (appended
///   when there was none). An older form is therefore replaced, and a re-run
///   finds the same group in the same place and writes nothing.
/// - A file that is not valid JSON, not an object, or whose `hooks` is not an
///   object or `hooks.SessionStart` not an array, is left untouched and
///   reported as [`HookMerge::Skipped`].
///
/// The file is rewritten pretty-printed (keys in `serde_json`'s order, as the
/// `.mcp.json` merge does) only when its content changes.
pub fn merge_session_hook(path: &Path) -> Result<HookMerge> {
    let existing: Option<Value> = if path.exists() {
        let raw = fs::read_to_string(path)?;
        match serde_json::from_str(&raw) {
            Ok(v) => Some(v),
            Err(e) => return Ok(HookMerge::Skipped(format!("it is not valid JSON ({e})"))),
        }
    } else {
        None
    };
    let mut doc = existing
        .clone()
        .unwrap_or_else(|| Value::Object(Default::default()));

    let Some(root) = doc.as_object_mut() else {
        return Ok(HookMerge::Skipped("it is not a JSON object".to_string()));
    };
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(hooks) = hooks.as_object_mut() else {
        return Ok(HookMerge::Skipped(
            "its `hooks` is not an object".to_string(),
        ));
    };
    let session = hooks
        .entry("SessionStart")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(groups) = session.as_array_mut() else {
        return Ok(HookMerge::Skipped(
            "its `hooks.SessionStart` is not an array".to_string(),
        ));
    };

    // Remove every bears handler, remembering where the first one was.
    let mut insert_at: Option<usize> = None;
    let mut kept: Vec<Value> = Vec::with_capacity(groups.len() + 1);
    for mut group in groups.drain(..) {
        let handlers = group.get_mut("hooks").and_then(Value::as_array_mut);
        let Some(handlers) = handlers else {
            kept.push(group);
            continue;
        };
        let before = handlers.len();
        handlers.retain(|h| !is_prime_handler(h));
        if handlers.len() == before {
            kept.push(group);
            continue;
        }
        insert_at.get_or_insert(kept.len());
        if !handlers.is_empty() {
            kept.push(group);
        }
    }
    let at = insert_at.unwrap_or(kept.len());
    kept.insert(at, prime_hook_group());
    *groups = kept;

    if existing.as_ref() == Some(&doc) {
        return Ok(HookMerge::Unchanged);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(&doc)?;
    fs::write(path, format!("{pretty}\n"))?;
    Ok(HookMerge::Written)
}

// ── MCP strategy ─────────────────────────────────────────────────────────────

/// How a harness registers its MCP server.
#[derive(Clone)]
pub enum McpStrategy {
    /// Merge the `bears` entry under the given key path (e.g. `mcpServers`)
    /// into the JSON file at `target`.
    ///
    /// `seed_json` is the full seed document from the embedded template; its
    /// `server_key` object will be extracted and merged into any existing file.
    MergeJson {
        /// Path of the JSON config file relative to the project root.
        target: &'static str,
        /// Top-level key in the JSON object that contains the server map
        /// (e.g. `"mcpServers"` for Claude, `"servers"` for Copilot).
        server_key: &'static str,
        /// Embedded seed JSON document (full file content).
        seed_json: &'static str,
    },
    /// No MCP registration needed (harness finds servers another way).
    None,
}

// ── Harness descriptor ────────────────────────────────────────────────────────

/// A single file to scaffold: a target path (relative to the project root) and
/// its embedded content.
pub struct ScaffoldFile {
    /// Target path relative to the project root (e.g. `"CLAUDE.md"`).
    pub target: &'static str,
    /// Embedded file content.
    pub content: &'static str,
}

/// Describes a complete coding-harness integration.
pub struct Harness {
    /// The single top-level instruction file (`CLAUDE.md`, `AGENTS.md`,
    /// `.github/copilot-instructions.md`).
    pub instruction: ScaffoldFile,
    /// Skill, reference, and planner-agent files (may be empty, e.g. Codex).
    pub skills: &'static [ScaffoldFile],
    /// How to register the MCP server.
    pub mcp: McpStrategy,
    /// Settings file (relative to the project root) into which the bears
    /// `SessionStart` hook ([`PRIME_HOOK_COMMAND`]) is merged, or `None` for a
    /// harness without one. Merged with the skills category, like the MCP entry.
    pub session_hook: Option<&'static str>,
}

/// Which subset of a harness's files to scaffold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    /// The instruction file only.
    Instructions,
    /// Skills/references/agent files plus the MCP merge (and, for Claude, the
    /// `SessionStart` hook merge).
    Skills,
    /// Everything (instruction + skills + MCP).
    All,
}

/// How to resolve a plain file that already exists on disk.
///
/// The MCP merge and the session-hook merge are always performed regardless of
/// policy (they are safe and idempotent — only the bears entry is touched).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WritePolicy {
    /// Overwrite every plain file unconditionally.
    Force,
    /// Write only files that do not yet exist; leave existing ones untouched.
    SkipExisting,
    /// Append template content to an existing instruction file; non-instruction
    /// files are treated as `Force`.
    Append,
}

// ── Registry ──────────────────────────────────────────────────────────────────

const CLAUDE_INSTRUCTION: ScaffoldFile = ScaffoldFile {
    target: "CLAUDE.md",
    content: CLAUDE_MD,
};

static CLAUDE_SKILLS: &[ScaffoldFile] = &[
    ScaffoldFile {
        target: ".claude/skills/bears-planning/SKILL.md",
        content: CLAUDE_SKILL_MD,
    },
    ScaffoldFile {
        target: ".claude/skills/bears-planning/references/cli-fallback.md",
        content: CLAUDE_SKILL_CLI_FALLBACK,
    },
    ScaffoldFile {
        target: ".claude/agents/planner.md",
        content: CLAUDE_AGENT_PLANNER,
    },
];

const COPILOT_INSTRUCTION: ScaffoldFile = ScaffoldFile {
    target: ".github/copilot-instructions.md",
    content: COPILOT_MD,
};

static COPILOT_SKILLS: &[ScaffoldFile] = &[
    ScaffoldFile {
        target: ".github/skills/bears-planning/SKILL.md",
        content: COPILOT_SKILL_MD,
    },
    ScaffoldFile {
        target: ".github/skills/bears-planning/references/cli-fallback.md",
        content: COPILOT_SKILL_CLI_FALLBACK,
    },
    ScaffoldFile {
        target: ".github/agents/planner.agent.md",
        content: COPILOT_AGENT_PLANNER,
    },
];

const CODEX_INSTRUCTION: ScaffoldFile = ScaffoldFile {
    target: "AGENTS.md",
    content: CODEX_MD,
};

/// All supported harnesses.  Each entry is a `(&str label, &Harness)` pair
/// where `label` corresponds to the CLI flag name (`claude`, `copilot`,
/// `codex`).
pub static REGISTRY: &[(&str, &Harness)] = &[
    (
        "claude",
        &Harness {
            instruction: CLAUDE_INSTRUCTION,
            skills: CLAUDE_SKILLS,
            mcp: McpStrategy::MergeJson {
                target: ".mcp.json",
                server_key: "mcpServers",
                seed_json: CLAUDE_MCP_SEED,
            },
            session_hook: Some(CLAUDE_SETTINGS),
        },
    ),
    (
        "copilot",
        &Harness {
            instruction: COPILOT_INSTRUCTION,
            skills: COPILOT_SKILLS,
            mcp: McpStrategy::MergeJson {
                target: ".github/mcp.json",
                server_key: "servers",
                seed_json: COPILOT_MCP_SEED,
            },
            session_hook: None,
        },
    ),
    (
        "codex",
        &Harness {
            instruction: CODEX_INSTRUCTION,
            skills: &[],
            mcp: McpStrategy::None,
            session_hook: None,
        },
    ),
];

// ── Core helpers ──────────────────────────────────────────────────────────────

/// Marker separating an existing instruction file from appended bears content.
/// An HTML comment so it stays invisible in the rendered Markdown, and a stable
/// anchor that makes `--append` idempotent (re-running detects it and skips).
const APPEND_MARKER: &str = "<!-- bears:begin -->";

/// Write `content` to `path` according to `policy`, creating parent directories
/// as needed.
///
/// Returns `Ok(true)` when the file was created or modified, `Ok(false)` when it
/// was left untouched (i.e. `SkipExisting` and the file already exists).
///
/// `WritePolicy::Append` is only meaningful for instruction files (handled via
/// [`append_instruction`]); here it behaves like `Force` so non-instruction
/// files (skills/agents) are refreshed.
pub fn write_file(path: &Path, content: &str, policy: WritePolicy) -> Result<bool> {
    if policy == WritePolicy::SkipExisting && path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

/// Append `content` to the instruction file at `path`, separated by
/// [`APPEND_MARKER`].
///
/// - If the file does not exist, it is created with `content` verbatim.
/// - If the marker is already present, this is a no-op (idempotent re-append).
///
/// Returns `Ok(true)` when the file was created or modified.
fn append_instruction(path: &Path, content: &str) -> Result<bool> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing.contains(APPEND_MARKER) {
            return Ok(false); // already appended — keep idempotent
        }
        let combined = format!("{existing}\n\n{APPEND_MARKER}\n{content}");
        fs::write(path, combined)?;
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

/// Merge the `bears` MCP server entry into the JSON file at `path`.
///
/// - If the file does not exist, it is created from `seed_json` verbatim.
/// - If it exists, we parse it, add/replace **only** the `bears` key inside
///   `server_key`, preserve every other key, and write back pretty-printed.
///
/// Returns the path that was written.
pub fn merge_mcp_json(path: &Path, server_key: &str, seed_json: &str) -> Result<()> {
    // Parse the seed to extract the bears server entry.
    let seed: Value = serde_json::from_str(seed_json)?;
    let bears_entry = seed
        .get(server_key)
        .and_then(|s| s.get("bears"))
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    // Load or start from an empty object.
    let mut doc: Value = if path.exists() {
        let raw = fs::read_to_string(path)?;
        serde_json::from_str(&raw).unwrap_or(Value::Object(Default::default()))
    } else {
        Value::Object(Default::default())
    };

    // Ensure the server map exists, then insert/replace the `bears` entry.
    let servers = doc
        .as_object_mut()
        .expect("top-level JSON must be an object")
        .entry(server_key)
        .or_insert_with(|| Value::Object(Default::default()));

    servers
        .as_object_mut()
        .expect("server key must be a JSON object")
        .insert("bears".to_string(), bears_entry);

    // Write back pretty-printed with a trailing newline.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(&doc)?;
    fs::write(path, format!("{pretty}\n"))?;
    Ok(())
}

// ── Top-level scaffold entry point ────────────────────────────────────────────

/// Whether `category` includes the instruction file.
fn category_has_instruction(category: Category) -> bool {
    matches!(category, Category::Instructions | Category::All)
}

/// Whether `category` includes the skill files and MCP merge.
fn category_has_skills(category: Category) -> bool {
    matches!(category, Category::Skills | Category::All)
}

/// The plain-file targets (absolute paths) that scaffolding `category` for the
/// given harness labels would touch.
///
/// Excludes the MCP JSON file and the Claude settings file, which always merge
/// safely and should never gate an overwrite prompt. Used by callers to decide whether anything already
/// exists before prompting.
pub fn category_targets(base: &Path, harness_labels: &[&str], category: Category) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    for label in harness_labels {
        let Some((_, harness)) = REGISTRY.iter().find(|(l, _)| l == label) else {
            continue;
        };
        if category_has_instruction(category) {
            targets.push(base.join(harness.instruction.target));
        }
        if category_has_skills(category) {
            for f in harness.skills {
                targets.push(base.join(f.target));
            }
        }
    }
    targets
}

/// Scaffold the files belonging to `category` for the given harness labels into
/// `base`, resolving on-disk collisions per `policy`.
///
/// The MCP merge and the session-hook merge always run for skill-bearing
/// categories (they are safe and idempotent). Returns the list of paths that were actually created or
/// modified (skipped files are omitted).
pub fn scaffold_category(
    base: &Path,
    harness_labels: &[&str],
    category: Category,
    policy: WritePolicy,
) -> Result<Vec<PathBuf>> {
    let mut written: Vec<PathBuf> = Vec::new();

    for label in harness_labels {
        let Some((_, harness)) = REGISTRY.iter().find(|(l, _)| l == label) else {
            // Unknown label — skip silently (CLI validation should prevent this)
            continue;
        };

        if category_has_instruction(category) {
            let target = base.join(harness.instruction.target);
            let changed = if policy == WritePolicy::Append {
                append_instruction(&target, harness.instruction.content)?
            } else {
                write_file(&target, harness.instruction.content, policy)?
            };
            if changed {
                written.push(target);
            }
        }

        if category_has_skills(category) {
            // Append has no meaning for generated skill/agent files — overwrite.
            let skill_policy = if policy == WritePolicy::Append {
                WritePolicy::Force
            } else {
                policy
            };
            for f in harness.skills {
                let target = base.join(f.target);
                if write_file(&target, f.content, skill_policy)? {
                    written.push(target);
                }
            }

            // MCP registration belongs to the skills category (runtime wiring for
            // the planner skill); always merge.
            if let McpStrategy::MergeJson {
                target,
                server_key,
                seed_json,
            } = &harness.mcp
            {
                let target_path = base.join(target);
                merge_mcp_json(&target_path, server_key, seed_json)?;
                written.push(target_path);
            }

            // The session-start hook is the same kind of wiring: always merge,
            // whatever the write policy.
            if let Some(target) = harness.session_hook {
                let target_path = base.join(target);
                match merge_session_hook(&target_path)? {
                    HookMerge::Written | HookMerge::Unchanged => written.push(target_path),
                    HookMerge::Skipped(reason) => eprintln!(
                        "warning: leaving {} untouched, {reason}; the bears SessionStart hook was not added",
                        target_path.display()
                    ),
                }
            }
        }
    }

    Ok(written)
}

/// Scaffold all files for the given harness labels into `base`, overwriting any
/// that already exist.
///
/// Thin back-compat wrapper over [`scaffold_category`] preserving the original
/// "everything, force overwrite" semantics. Used by the scaffold test suite.
#[allow(dead_code)]
pub fn scaffold(base: &Path, harness_labels: &[&str]) -> Result<Vec<PathBuf>> {
    scaffold_category(base, harness_labels, Category::All, WritePolicy::Force)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Every scaffolded template tells agents that IDs are hierarchical and that
    /// moves report a `renamed` map, and none still describes a `parent:`
    /// frontmatter field or CLI spellings that do not exist.
    #[test]
    fn test_templates_describe_hierarchical_ids() {
        let templates = [
            ("claude/CLAUDE.md", CLAUDE_MD),
            ("claude SKILL.md", CLAUDE_SKILL_MD),
            ("claude cli-fallback.md", CLAUDE_SKILL_CLI_FALLBACK),
            ("claude planner.md", CLAUDE_AGENT_PLANNER),
            ("copilot-instructions.md", COPILOT_MD),
            ("copilot SKILL.md", COPILOT_SKILL_MD),
            ("copilot cli-fallback.md", COPILOT_SKILL_CLI_FALLBACK),
            ("copilot planner.agent.md", COPILOT_AGENT_PLANNER),
            ("codex/AGENTS.md", CODEX_MD),
        ];
        for (name, text) in templates {
            assert!(text.contains("a1b.c2"), "{name} must show a dotted ID");
            assert!(
                text.contains("renamed"),
                "{name} must mention the renamed map"
            );
            assert!(!text.contains("parent:"), "{name} mentions a parent: key");
            assert!(
                !text.contains("`parent`, etc."),
                "{name} lists parent as a field"
            );
            assert!(!text.contains("--type epic"), "{name}: the flag is --epic");
            assert!(
                !text.contains("dep rm "),
                "{name}: the command is dep remove"
            );
        }
    }

    /// The planning skill and CLI fallback, which use the `epic` filter, say
    /// that it reaches the tasks under nested epics.
    #[test]
    fn test_templates_describe_epic_filter_subtree() {
        for (name, text) in [
            ("claude SKILL.md", CLAUDE_SKILL_MD),
            ("claude cli-fallback.md", CLAUDE_SKILL_CLI_FALLBACK),
            ("copilot SKILL.md", COPILOT_SKILL_MD),
            ("copilot cli-fallback.md", COPILOT_SKILL_CLI_FALLBACK),
        ] {
            assert!(text.contains("nested epics"), "{name}");
        }
    }

    /// The planning skill and CLI fallback describe an epic's plan as its
    /// direct children, a nested epic being one step.
    #[test]
    fn test_templates_describe_plan_as_direct_children() {
        for (name, text) in [
            ("claude SKILL.md", CLAUDE_SKILL_MD),
            ("claude cli-fallback.md", CLAUDE_SKILL_CLI_FALLBACK),
            ("copilot SKILL.md", COPILOT_SKILL_MD),
            ("copilot cli-fallback.md", COPILOT_SKILL_CLI_FALLBACK),
        ] {
            assert!(text.contains("direct children"), "{name}");
            assert!(text.contains("nested epic is one"), "{name}");
            assert!(text.contains("--plan"), "{name}");
        }
    }

    /// Merging preserves other servers already present in the file.
    #[test]
    fn test_merge_mcp_json_preserves_other_servers() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".mcp.json");

        // Pre-populate with an unrelated server.
        let existing = serde_json::json!({
            "mcpServers": {
                "other-tool": {
                    "command": "other",
                    "args": ["serve"]
                }
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        merge_mcp_json(&path, "mcpServers", CLAUDE_MCP_SEED).unwrap();

        let result: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let servers = result["mcpServers"].as_object().unwrap();

        // Both entries must be present.
        assert!(servers.contains_key("bears"), "bears entry missing");
        assert!(
            servers.contains_key("other-tool"),
            "other-tool entry must be preserved"
        );
    }

    /// Running the merge twice produces the same file (idempotent).
    #[test]
    fn test_merge_mcp_json_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".mcp.json");

        merge_mcp_json(&path, "mcpServers", CLAUDE_MCP_SEED).unwrap();
        let after_first = fs::read_to_string(&path).unwrap();

        merge_mcp_json(&path, "mcpServers", CLAUDE_MCP_SEED).unwrap();
        let after_second = fs::read_to_string(&path).unwrap();

        assert_eq!(after_first, after_second, "merge must be idempotent");
    }

    /// Fresh-create path: when no file exists, the result is a valid JSON file
    /// that contains the `bears` server entry.
    #[test]
    fn test_merge_mcp_json_fresh_create() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".mcp.json");

        assert!(!path.exists());
        merge_mcp_json(&path, "mcpServers", CLAUDE_MCP_SEED).unwrap();

        assert!(path.exists());
        let result: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let servers = result["mcpServers"].as_object().unwrap();
        assert!(
            servers.contains_key("bears"),
            "fresh-created file must have bears entry"
        );
        let bears = &servers["bears"];
        assert_eq!(bears["command"], "bea");
    }

    /// `bea init --claude` on an already-initialized dir succeeds and writes
    /// the expected files.
    #[test]
    fn test_scaffold_claude_idempotent() {
        let tmp = TempDir::new().unwrap();

        // First scaffold
        let written = scaffold(tmp.path(), &["claude"]).unwrap();
        assert!(
            written.iter().any(|p| p.ends_with("CLAUDE.md")),
            "CLAUDE.md must be in written list"
        );
        assert!(
            written.iter().any(|p| p.ends_with(".mcp.json")),
            ".mcp.json must be in written list"
        );
        assert!(tmp.path().join("CLAUDE.md").exists());
        assert!(tmp.path().join(".mcp.json").exists());

        // Second scaffold (re-init on existing dir) must succeed
        let written2 = scaffold(tmp.path(), &["claude"]).unwrap();
        assert_eq!(
            written.len(),
            written2.len(),
            "same number of files on re-scaffold"
        );

        // Contents must be stable
        let md = fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(md.contains("Bears"), "CLAUDE.md must reference Bears");

        let mcp: Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
                .unwrap();
        assert!(mcp["mcpServers"]["bears"].is_object());
    }

    /// `bea init --claude` writes the skill, cli-fallback reference, and agent
    /// files, with the production `bea mcp` MCP server form.
    #[test]
    fn test_scaffold_claude_skill_and_agent() {
        let tmp = TempDir::new().unwrap();
        let written = scaffold(tmp.path(), &["claude"]).unwrap();

        // Skill file
        let skill_path = tmp.path().join(".claude/skills/bears-planning/SKILL.md");
        assert!(
            written.iter().any(|p| p == &skill_path),
            "SKILL.md must be in written list"
        );
        assert!(skill_path.exists(), "SKILL.md must be created");
        let skill = fs::read_to_string(&skill_path).unwrap();
        assert!(
            skill.contains("bears-planning"),
            "SKILL.md must contain skill name"
        );
        assert!(
            skill.contains("mcp__bears__"),
            "SKILL.md must reference MCP tools"
        );

        // CLI fallback reference
        let cli_ref_path = tmp
            .path()
            .join(".claude/skills/bears-planning/references/cli-fallback.md");
        assert!(
            written.iter().any(|p| p == &cli_ref_path),
            "cli-fallback.md must be in written list"
        );
        assert!(cli_ref_path.exists(), "cli-fallback.md must be created");
        let cli_ref = fs::read_to_string(&cli_ref_path).unwrap();
        assert!(
            cli_ref.contains("bea create"),
            "cli-fallback.md must contain bea create"
        );

        // Agent file
        let agent_path = tmp.path().join(".claude/agents/planner.md");
        assert!(
            written.iter().any(|p| p == &agent_path),
            "planner.md must be in written list"
        );
        assert!(agent_path.exists(), "planner.md must be created");
        let agent = fs::read_to_string(&agent_path).unwrap();
        assert!(agent.contains("planner"), "agent must have name");
        assert!(
            agent.contains("mcp__bears__"),
            "agent must reference MCP tools"
        );

        // .mcp.json uses production form: command="bea", args=["mcp"]
        let mcp: Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
                .unwrap();
        let bears = &mcp["mcpServers"]["bears"];
        assert_eq!(bears["command"], "bea", "must use production binary form");
        assert_eq!(bears["args"][0], "mcp", "must use 'mcp' subcommand");
    }

    /// `bea init --copilot` writes the skill, cli-fallback reference, agent,
    /// and MCP registration files under `.github/`.
    #[test]
    fn test_scaffold_copilot_skill_and_agent() {
        let tmp = TempDir::new().unwrap();
        let written = scaffold(tmp.path(), &["copilot"]).unwrap();

        // copilot-instructions.md
        let instr_path = tmp.path().join(".github/copilot-instructions.md");
        assert!(
            instr_path.exists(),
            "copilot-instructions.md must be created"
        );

        // Skill file
        let skill_path = tmp.path().join(".github/skills/bears-planning/SKILL.md");
        assert!(
            written.iter().any(|p| p == &skill_path),
            "SKILL.md must be in written list"
        );
        assert!(skill_path.exists(), "SKILL.md must be created");
        let skill = fs::read_to_string(&skill_path).unwrap();
        assert!(
            skill.contains("bears-planning"),
            "SKILL.md must contain skill name"
        );
        assert!(
            skill.contains("bears/*"),
            "SKILL.md must reference Copilot MCP tool prefix"
        );

        // CLI fallback reference
        let cli_ref_path = tmp
            .path()
            .join(".github/skills/bears-planning/references/cli-fallback.md");
        assert!(
            written.iter().any(|p| p == &cli_ref_path),
            "cli-fallback.md must be in written list"
        );
        assert!(cli_ref_path.exists(), "cli-fallback.md must be created");
        let cli_ref = fs::read_to_string(&cli_ref_path).unwrap();
        assert!(
            cli_ref.contains("bea create"),
            "cli-fallback.md must contain bea create"
        );

        // Agent file
        let agent_path = tmp.path().join(".github/agents/planner.agent.md");
        assert!(
            written.iter().any(|p| p == &agent_path),
            "planner.agent.md must be in written list"
        );
        assert!(agent_path.exists(), "planner.agent.md must be created");
        let agent = fs::read_to_string(&agent_path).unwrap();
        assert!(
            agent.contains("bears/*"),
            "agent must reference Copilot MCP tool prefix"
        );

        // .github/mcp.json — merged with production bears server entry
        let mcp_path = tmp.path().join(".github/mcp.json");
        assert!(
            written.iter().any(|p| p == &mcp_path),
            ".github/mcp.json must be in written list"
        );
        assert!(mcp_path.exists(), ".github/mcp.json must be created");
        let mcp: Value = serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
        let bears = &mcp["servers"]["bears"];
        assert_eq!(bears["command"], "bea", "must use production binary form");
        assert_eq!(bears["args"][0], "mcp", "must use 'mcp' subcommand");
    }

    /// Unknown harness labels are silently skipped — no panic or error.
    #[test]
    fn test_scaffold_unknown_label_skipped() {
        let tmp = TempDir::new().unwrap();
        let result = scaffold(tmp.path(), &["nonexistent-harness"]);
        assert!(result.is_ok());
        let written = result.unwrap();
        assert!(written.is_empty());
    }

    // ── Write-policy + category tests ──────────────────────────────────────────

    /// `Force` overwrites an existing file and reports it as written.
    #[test]
    fn test_write_file_force_overwrites() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("f.md");
        fs::write(&path, "old").unwrap();

        let changed = write_file(&path, "new", WritePolicy::Force).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    /// `SkipExisting` leaves an existing file untouched and reports `false`,
    /// but still creates a missing file.
    #[test]
    fn test_write_file_skip_existing() {
        let tmp = TempDir::new().unwrap();
        let existing = tmp.path().join("exists.md");
        fs::write(&existing, "keep").unwrap();

        let changed = write_file(&existing, "new", WritePolicy::SkipExisting).unwrap();
        assert!(!changed, "existing file must be skipped");
        assert_eq!(fs::read_to_string(&existing).unwrap(), "keep");

        let missing = tmp.path().join("missing.md");
        let created = write_file(&missing, "new", WritePolicy::SkipExisting).unwrap();
        assert!(created, "missing file must be created under SkipExisting");
        assert_eq!(fs::read_to_string(&missing).unwrap(), "new");
    }

    /// `append_instruction` preserves user content and adds the template under a
    /// marker; a second append is a no-op.
    #[test]
    fn test_append_instruction_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        fs::write(&path, "USER TEXT").unwrap();

        let changed = append_instruction(&path, "TEMPLATE").unwrap();
        assert!(changed);
        let after_first = fs::read_to_string(&path).unwrap();
        assert!(after_first.contains("USER TEXT"));
        assert!(after_first.contains(APPEND_MARKER));
        assert!(after_first.contains("TEMPLATE"));

        let changed_again = append_instruction(&path, "TEMPLATE").unwrap();
        assert!(!changed_again, "second append must be a no-op");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            after_first,
            "file must be unchanged on re-append"
        );
    }

    /// `append_instruction` on a missing file just creates it.
    #[test]
    fn test_append_instruction_creates_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        let changed = append_instruction(&path, "TEMPLATE").unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&path).unwrap(), "TEMPLATE");
    }

    /// `Category::Instructions` writes only the instruction file — no skills,
    /// no `.mcp.json`.
    #[test]
    fn test_scaffold_category_instructions_only() {
        let tmp = TempDir::new().unwrap();
        let written = scaffold_category(
            tmp.path(),
            &["claude"],
            Category::Instructions,
            WritePolicy::Force,
        )
        .unwrap();

        assert!(tmp.path().join("CLAUDE.md").exists());
        assert!(!tmp.path().join(".mcp.json").exists());
        assert!(
            !tmp.path()
                .join(".claude/skills/bears-planning/SKILL.md")
                .exists()
        );
        assert!(written.iter().all(|p| !p.ends_with(".mcp.json")));
    }

    /// `Category::Skills` writes the skill files and merges `.mcp.json`, but not
    /// the instruction file.
    #[test]
    fn test_scaffold_category_skills_includes_mcp() {
        let tmp = TempDir::new().unwrap();
        scaffold_category(
            tmp.path(),
            &["claude"],
            Category::Skills,
            WritePolicy::Force,
        )
        .unwrap();

        assert!(!tmp.path().join("CLAUDE.md").exists());
        assert!(tmp.path().join(".mcp.json").exists());
        assert!(
            tmp.path()
                .join(".claude/skills/bears-planning/SKILL.md")
                .exists()
        );
    }

    /// `All` + `SkipExisting` keeps an edited instruction file but still creates
    /// missing skill files and merges `.mcp.json`.
    #[test]
    fn test_scaffold_category_all_skip_existing() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "MY EDITS").unwrap();

        scaffold_category(
            tmp.path(),
            &["claude"],
            Category::All,
            WritePolicy::SkipExisting,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap(),
            "MY EDITS",
            "existing instruction file must be preserved"
        );
        assert!(
            tmp.path()
                .join(".claude/skills/bears-planning/SKILL.md")
                .exists()
        );
        assert!(tmp.path().join(".mcp.json").exists());
    }

    /// `All` + `Append` keeps user instruction text (appending the template) and
    /// overwrites the generated skill files.
    #[test]
    fn test_scaffold_category_all_append() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "MY EDITS").unwrap();

        scaffold_category(tmp.path(), &["claude"], Category::All, WritePolicy::Append).unwrap();

        let md = fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(md.contains("MY EDITS"));
        assert!(md.contains(APPEND_MARKER));
        assert!(md.contains("Bears"), "template content must be appended");
        assert!(
            tmp.path()
                .join(".claude/skills/bears-planning/SKILL.md")
                .exists()
        );
    }

    /// `category_targets` lists plain files but never `.mcp.json`.
    #[test]
    fn test_category_targets_excludes_mcp_json() {
        let tmp = TempDir::new().unwrap();
        let targets = category_targets(tmp.path(), &["claude"], Category::All);
        assert!(targets.iter().any(|p| p.ends_with("CLAUDE.md")));
        assert!(
            targets.iter().all(|p| !p.ends_with(".mcp.json")),
            ".mcp.json must be excluded from prompt-trigger targets"
        );
    }

    // ── Session-start hook merge ──────────────────────────────────────────────

    /// The bears handlers in `SessionStart`, in order.
    fn prime_commands(doc: &Value) -> Vec<String> {
        doc["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .filter_map(|h| h["command"].as_str())
            .filter(|c| c.contains("bea prime"))
            .map(str::to_string)
            .collect()
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// A missing settings file is created holding exactly the hook.
    #[test]
    fn test_merge_session_hook_fresh_create() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".claude/settings.json");
        assert_eq!(merge_session_hook(&path).unwrap(), HookMerge::Written);
        assert_eq!(
            read_json(&path),
            serde_json::json!({
                "hooks": { "SessionStart": [
                    { "hooks": [{ "type": "command", "command": PRIME_HOOK_COMMAND }] }
                ] }
            })
        );
    }

    /// Other settings, other hook events and other `SessionStart` entries
    /// (including other handlers of a group that held an old bears handler) are
    /// kept.
    #[test]
    fn test_merge_session_hook_preserves_other_settings() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let mine = serde_json::json!({ "type": "command", "command": "echo hi" });
        let existing = serde_json::json!({
            "permissions": { "allow": ["Bash(cargo test:*)"] },
            "model": "opus",
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "check" }] }],
                "SessionStart": [
                    { "matcher": "startup", "hooks": [mine.clone()] },
                    { "hooks": [
                        { "type": "command", "command": "test ! -d .bears || bea prime" },
                        { "type": "command", "command": "echo also" }
                    ] }
                ]
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        assert_eq!(merge_session_hook(&path).unwrap(), HookMerge::Written);
        let doc = read_json(&path);
        assert_eq!(doc["permissions"], existing["permissions"]);
        assert_eq!(doc["model"], "opus");
        assert_eq!(doc["hooks"]["PreToolUse"], existing["hooks"]["PreToolUse"]);
        let groups = doc["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups[0],
            serde_json::json!({ "matcher": "startup", "hooks": [mine] })
        );
        assert_eq!(groups[1], prime_hook_group());
        assert_eq!(
            groups[2],
            serde_json::json!({ "hooks": [{ "type": "command", "command": "echo also" }] })
        );
    }

    /// A second merge finds the hook and writes nothing.
    #[test]
    fn test_merge_session_hook_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(
            &path,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"x"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(merge_session_hook(&path).unwrap(), HookMerge::Written);
        let first = fs::read_to_string(&path).unwrap();
        assert_eq!(merge_session_hook(&path).unwrap(), HookMerge::Unchanged);
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
        assert_eq!(prime_commands(&read_json(&path)), [PRIME_HOOK_COMMAND]);
    }

    /// An older form of the hook is replaced in place, and duplicates collapse
    /// to one.
    #[test]
    fn test_merge_session_hook_replaces_old_form() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let old = serde_json::json!({ "hooks": [{ "type": "command", "command": "test ! -d .bears || bea prime" }] });
        let other = serde_json::json!({ "hooks": [{ "type": "command", "command": "other" }] });
        let existing = serde_json::json!({
            "hooks": { "SessionStart": [other.clone(), old.clone(), old] }
        });
        fs::write(&path, existing.to_string()).unwrap();

        assert_eq!(merge_session_hook(&path).unwrap(), HookMerge::Written);
        let doc = read_json(&path);
        assert_eq!(
            doc["hooks"]["SessionStart"],
            serde_json::json!([other, prime_hook_group()])
        );
    }

    /// A file of the wrong shape is left byte for byte as it was.
    #[test]
    fn test_merge_session_hook_leaves_unexpected_files_untouched() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        for raw in [
            "{ not json",
            "[1, 2]",
            r#"{"hooks": []}"#,
            r#"{"hooks": {"SessionStart": {"hooks": []}}}"#,
        ] {
            fs::write(&path, raw).unwrap();
            assert!(
                matches!(merge_session_hook(&path).unwrap(), HookMerge::Skipped(_)),
                "{raw}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        }
    }

    /// `--claude` merges the hook for `skills` and `all` (whatever the write
    /// policy) but not for `instructions`; Copilot and Codex get no hook, and
    /// `settings.local.json` is never touched.
    #[test]
    fn test_scaffold_session_hook_categories_and_harnesses() {
        for (category, policy) in [
            (Category::Skills, WritePolicy::SkipExisting),
            (Category::All, WritePolicy::SkipExisting),
            (Category::All, WritePolicy::Force),
            (Category::All, WritePolicy::Append),
        ] {
            let tmp = TempDir::new().unwrap();
            let settings = tmp.path().join(".claude/settings.json");
            fs::create_dir_all(settings.parent().unwrap()).unwrap();
            fs::write(&settings, r#"{"model": "opus"}"#).unwrap();
            let written = scaffold_category(tmp.path(), &["claude"], category, policy).unwrap();
            assert!(written.contains(&settings), "{category:?} {policy:?}");
            let doc = read_json(&settings);
            assert_eq!(doc["model"], "opus");
            assert_eq!(prime_commands(&doc), [PRIME_HOOK_COMMAND]);
            assert!(!tmp.path().join(".claude/settings.local.json").exists());
        }

        let tmp = TempDir::new().unwrap();
        scaffold_category(
            tmp.path(),
            &["claude"],
            Category::Instructions,
            WritePolicy::Force,
        )
        .unwrap();
        assert!(!tmp.path().join(".claude/settings.json").exists());

        let tmp = TempDir::new().unwrap();
        scaffold(tmp.path(), &["copilot", "codex"]).unwrap();
        assert!(!tmp.path().join(".claude").exists());
    }

    /// A settings file of the wrong shape does not fail the scaffolding and is
    /// not reported as written.
    #[test]
    fn test_scaffold_skips_invalid_settings() {
        let tmp = TempDir::new().unwrap();
        let settings = tmp.path().join(".claude/settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "{ broken").unwrap();
        let written = scaffold(tmp.path(), &["claude"]).unwrap();
        assert!(!written.contains(&settings));
        assert!(written.iter().any(|p| p.ends_with(".mcp.json")));
        assert_eq!(fs::read_to_string(&settings).unwrap(), "{ broken");
    }

    /// The README and the Claude instructions template show the scaffolded
    /// command, as it appears in JSON; so does CLAUDE.md where it is present
    /// (it is not in the packaged crate).
    #[test]
    fn test_docs_show_scaffolded_hook_command() {
        let quoted = serde_json::to_string(PRIME_HOOK_COMMAND).unwrap();
        assert!(CLAUDE_MD.contains(&quoted), "templates/claude/CLAUDE.md");
        assert!(include_str!("../README.md").contains(&quoted), "README.md");
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("CLAUDE.md");
        if let Ok(text) = fs::read_to_string(dev) {
            assert!(text.contains(PRIME_HOOK_COMMAND), "CLAUDE.md");
        }
    }
}
