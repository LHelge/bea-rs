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
    /// Human-readable name (for diagnostics / future display use).
    #[allow(dead_code)]
    pub name: &'static str,
    /// Plain files to write (idempotent overwrite).
    pub files: &'static [ScaffoldFile],
    /// How to register the MCP server.
    pub mcp: McpStrategy,
}

// ── Registry ──────────────────────────────────────────────────────────────────

static CLAUDE_FILES: &[ScaffoldFile] = &[
    ScaffoldFile {
        target: "CLAUDE.md",
        content: CLAUDE_MD,
    },
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

static COPILOT_FILES: &[ScaffoldFile] = &[
    ScaffoldFile {
        target: ".github/copilot-instructions.md",
        content: COPILOT_MD,
    },
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

static CODEX_FILES: &[ScaffoldFile] = &[ScaffoldFile {
    target: "AGENTS.md",
    content: CODEX_MD,
}];

/// All supported harnesses.  Each entry is a `(&str label, &Harness)` pair
/// where `label` corresponds to the CLI flag name (`claude`, `copilot`,
/// `codex`).
pub static REGISTRY: &[(&str, &Harness)] = &[
    (
        "claude",
        &Harness {
            name: "Claude",
            files: CLAUDE_FILES,
            mcp: McpStrategy::MergeJson {
                target: ".mcp.json",
                server_key: "mcpServers",
                seed_json: CLAUDE_MCP_SEED,
            },
        },
    ),
    (
        "copilot",
        &Harness {
            name: "Copilot",
            files: COPILOT_FILES,
            mcp: McpStrategy::MergeJson {
                target: ".github/mcp.json",
                server_key: "servers",
                seed_json: COPILOT_MCP_SEED,
            },
        },
    ),
    (
        "codex",
        &Harness {
            name: "Codex",
            files: CODEX_FILES,
            mcp: McpStrategy::None,
        },
    ),
];

// ── Core helpers ──────────────────────────────────────────────────────────────

/// Write `content` to `path`, creating all parent directories as needed.
/// If the file already exists it is overwritten (idempotent refresh).
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
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

/// Scaffold all files for the given harness labels into `base`.
///
/// Returns the list of paths that were created or updated.
pub fn scaffold(base: &Path, harness_labels: &[&str]) -> Result<Vec<PathBuf>> {
    let mut written: Vec<PathBuf> = Vec::new();

    for label in harness_labels {
        let Some((_, harness)) = REGISTRY.iter().find(|(l, _)| l == label) else {
            // Unknown label — skip silently (CLI validation should prevent this)
            continue;
        };

        // Write plain files
        for f in harness.files {
            let target = base.join(f.target);
            write_file(&target, f.content)?;
            written.push(target);
        }

        // Handle MCP strategy
        match &harness.mcp {
            McpStrategy::MergeJson {
                target,
                server_key,
                seed_json,
            } => {
                let target_path = base.join(target);
                merge_mcp_json(&target_path, server_key, seed_json)?;
                written.push(target_path);
            }
            McpStrategy::None => {}
        }
    }

    Ok(written)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
