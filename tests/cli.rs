use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bea(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bea").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

#[test]
fn test_init() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized bears"));
}

#[test]
fn test_init_json() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["--json", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\""));
}

#[test]
fn test_list_not_initialized() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn test_create_and_list() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .args([
            "create",
            "My first task",
            "--priority",
            "P1",
            "--tag",
            "backend",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task"));

    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("My first task"));
}

#[test]
fn test_create_and_show_json() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let output = bea(&tmp)
        .args(["--json", "create", "JSON task", "--priority", "P0"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    bea(&tmp)
        .args(["--json", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("JSON task"));
}

#[test]
fn test_ready_flow() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    // Create first task
    let out = bea(&tmp)
        .args(["--json", "create", "First task", "--priority", "P1"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id1 = v["id"].as_str().unwrap().to_string();

    // Create second task depending on first
    let out = bea(&tmp)
        .args([
            "--json",
            "create",
            "Second task",
            "--priority",
            "P1",
            "--depends-on",
            &id1,
        ])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id2 = v["id"].as_str().unwrap().to_string();

    // Ready should show only first task
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("First task"))
        .stdout(predicate::str::contains("Second task").not());

    // Complete first task
    bea(&tmp).args(["done", &id1]).assert().success();

    // Now second task should be ready
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains(&id2));
}

#[test]
fn test_dep_cycle_rejected() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "Task A"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id_a = v["id"].as_str().unwrap().to_string();

    let out = bea(&tmp)
        .args(["--json", "create", "Task B", "--depends-on", &id_a])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id_b = v["id"].as_str().unwrap().to_string();

    // Adding A depends on B should fail (cycle: A <- B -> A)
    bea(&tmp)
        .args(["dep", "add", &id_a, &id_b])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}

#[test]
fn test_create_with_unknown_dependency() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .args(["create", "Task with bad dep", "--depends-on", "zzzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown dependency"));
}

#[test]
fn test_search() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .args(["create", "Implement OAuth", "--tag", "auth"])
        .assert()
        .success();
    bea(&tmp)
        .args(["create", "Fix database bug"])
        .assert()
        .success();

    bea(&tmp)
        .args(["search", "OAuth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Implement OAuth"))
        .stdout(predicate::str::contains("database").not());
}

#[test]
fn test_status_commands() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "Work item"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // Start
    bea(&tmp)
        .args(["start", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("in_progress"));

    // Done
    bea(&tmp)
        .args(["done", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

#[test]
fn test_list_json() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp).args(["create", "A task"]).assert().success();

    let out = bea(&tmp).args(["--json", "list"]).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn test_edit_modifies_body() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "Edit me", "--body", "original body"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // Create a portable editor script that appends a line
    let script = tmp.path().join("append-editor.sh");
    std::fs::write(&script, "#!/bin/sh\necho 'appended line' >> \"$1\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    bea(&tmp)
        .env("EDITOR", script.to_str().unwrap())
        .args(["edit", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Edited task"));

    // Verify the body changed
    let out = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v["body"].as_str().unwrap().contains("appended line"));
}

#[test]
fn test_edit_no_changes() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "No change task"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // Use 'true' as $EDITOR — does nothing
    bea(&tmp)
        .env("EDITOR", "true")
        .args(["edit", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("No changes"));
}

#[test]
fn test_edit_invalid_task_id() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .env("EDITOR", "true")
        .args(["edit", "xxxx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_edit_bad_frontmatter() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "Break me"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // Create a portable editor script that corrupts the frontmatter
    let script = tmp.path().join("corrupt-editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nsed 's/^status:.*/status: [invalid/' \"$1\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    bea(&tmp)
        .env("EDITOR", script.to_str().unwrap())
        .args(["edit", &id])
        .assert()
        .success()
        .stderr(predicate::str::contains("Invalid frontmatter"));
}

#[test]
fn test_completions_zsh() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef"));
}

#[test]
fn test_completions_bash() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completions_fish() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

// --- Prefix ID resolution tests ---

/// Helper: create a task and return its full ID.
fn create_task(dir: &TempDir, title: &str) -> String {
    let out = bea(dir).args(["--json", "create", title]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    v["id"].as_str().unwrap().to_string()
}

#[test]
fn test_prefix_show_resolves() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Prefix show test");

    // Use first 2 chars as prefix
    let prefix = &id[..2];
    bea(&tmp)
        .args(["--json", "show", prefix])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prefix show test"));
}

#[test]
fn test_prefix_start_and_done() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Prefix status test");
    let prefix = &id[..2];

    bea(&tmp)
        .args(["start", prefix])
        .assert()
        .success()
        .stdout(predicate::str::contains("in_progress"));

    bea(&tmp)
        .args(["done", prefix])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

#[test]
fn test_prefix_dep_add() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_a = create_task(&tmp, "Dep target");
    let id_b = create_task(&tmp, "Dep source");

    let prefix_a = &id_a[..2];
    let prefix_b = &id_b[..2];

    // If prefixes happen to collide, use full IDs as fallback
    let (use_a, use_b) = if prefix_a == prefix_b {
        (id_a.as_str(), id_b.as_str())
    } else {
        (prefix_a, prefix_b)
    };

    bea(&tmp)
        .args(["dep", "add", use_b, use_a])
        .assert()
        .success();

    // Verify the dependency was added
    let out = bea(&tmp).args(["--json", "show", &id_b]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let deps = v["depends_on"].as_array().unwrap();
    assert!(deps.iter().any(|d| d.as_str().unwrap() == id_a));
}

#[test]
fn test_prefix_ambiguous_error() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    // Create many tasks to increase chance of a shared prefix
    let mut ids = Vec::new();
    for i in 0..30 {
        ids.push(create_task(&tmp, &format!("Ambiguous task {i}")));
    }

    // Find two IDs that share a first character
    let mut found_prefix = None;
    'outer: for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            if ids[i].as_bytes()[0] == ids[j].as_bytes()[0] {
                found_prefix = Some(String::from(&ids[i][..1]));
                break 'outer;
            }
        }
    }

    if let Some(prefix) = found_prefix {
        bea(&tmp)
            .args(["show", &prefix])
            .assert()
            .failure()
            .stderr(predicate::str::contains("ambiguous"));
    }
    // If no collision found (extremely unlikely with 30 tasks), skip gracefully
}

#[test]
fn test_prefix_exact_match_preferred() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Exact match task");

    // Using the full ID should always work even if it's a prefix of nothing else
    bea(&tmp)
        .args(["--json", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exact match task"));
}

#[test]
fn test_prefix_not_found() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    create_task(&tmp, "Some task");

    bea(&tmp)
        .args(["show", "zzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// --- Regression tests for review-driven fixes ---

/// 22b4: dep tree with a cycle shows [CYCLE] marker (doesn't hang).
#[test]
fn test_dep_tree_cycle_safe() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_a = create_task(&tmp, "Task A");
    let id_b = create_task(&tmp, "Task B");

    // A depends on B
    bea(&tmp)
        .args(["dep", "add", &id_a, &id_b])
        .assert()
        .success();

    // Force a cycle by manually adding B -> A in the file
    let bears_dir = tmp.path().join(".bears");
    for entry in std::fs::read_dir(&bears_dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&id_b)
        {
            let content = std::fs::read_to_string(&path).unwrap();
            // Task B has no depends_on field; inject one before closing ---
            let patched =
                content.replacen("\n---\n", &format!("\ndepends_on:\n- {id_a}\n---\n"), 1);
            std::fs::write(&path, patched).unwrap();
            break;
        }
    }

    // dep tree should complete (not hang) and show CYCLE marker in JSON
    let out = bea(&tmp)
        .args(["--json", "dep", "tree", &id_a])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();

    // Find a node with cycle: true somewhere in the tree
    fn has_cycle(node: &serde_json::Value) -> bool {
        if node.get("cycle").and_then(|v| v.as_bool()).unwrap_or(false) {
            return true;
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            return children.iter().any(has_cycle);
        }
        false
    }
    assert!(has_cycle(&json), "expected cycle marker in dep tree JSON");
}

/// 22b4: `graph` command with cyclic data completes.
#[test]
fn test_graph_with_cycle_completes() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_a = create_task(&tmp, "Cycle A");
    let id_b = create_task(&tmp, "Cycle B");

    bea(&tmp)
        .args(["dep", "add", &id_a, &id_b])
        .assert()
        .success();

    // Force cycle in file
    let bears_dir = tmp.path().join(".bears");
    for entry in std::fs::read_dir(&bears_dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&id_b)
        {
            let content = std::fs::read_to_string(&path).unwrap();
            let patched =
                content.replacen("\n---\n", &format!("\ndepends_on:\n- {id_a}\n---\n"), 1);
            std::fs::write(&path, patched).unwrap();
            break;
        }
    }

    // graph should still complete, not loop forever
    bea(&tmp).args(["--json", "graph"]).assert().success();
}

/// ff18: ready excludes tasks whose dependency was deleted (missing dep ID).
#[test]
fn test_ready_missing_dep_blocks() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_dep = create_task(&tmp, "Will be deleted");
    let id_task = create_task(&tmp, "Depends on deleted");

    bea(&tmp)
        .args(["dep", "add", &id_task, &id_dep])
        .assert()
        .success();

    // Delete the dependency task
    bea(&tmp).args(["delete", &id_dep]).assert().success();

    // The dependent task should NOT appear in ready
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Depends on deleted").not());
}

/// bc2f: init creates .bears/ directory (not .tasks/).
#[test]
fn test_init_creates_bears_dir() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    assert!(tmp.path().join(".bears").is_dir());
    assert!(!tmp.path().join(".tasks").exists());
}

/// 612b: edit works with a multi-word EDITOR value.
#[test]
fn test_edit_multiword_editor() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Multi-word editor test");

    // Create a wrapper script that accepts and ignores extra args
    let script = tmp.path().join("my-editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\n# Ignore --wait flag, append text to file\necho 'added by editor' >> \"$2\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Use "script --wait" as EDITOR (multi-word)
    let editor_val = format!("{} --wait", script.display());
    bea(&tmp)
        .env("EDITOR", &editor_val)
        .args(["edit", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Edited task"));

    // Verify body was modified
    let out = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v["body"].as_str().unwrap().contains("added by editor"));
}

/// ff18: creating a task with unknown depends_on is rejected.
#[test]
fn test_update_with_unknown_dep_via_dep_add() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Has deps");

    // dep add with nonexistent target should fail
    bea(&tmp)
        .args(["dep", "add", &id, "zzzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

/// bc2f + 0e02: load_all skips files with invalid frontmatter gracefully.
#[test]
fn test_list_skips_corrupt_file() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    create_task(&tmp, "Good task");

    // Write a corrupt .md file into .bears/
    let corrupt_path = tmp.path().join(".bears/xxxx-corrupt.md");
    std::fs::write(&corrupt_path, "not valid frontmatter at all").unwrap();

    // list should still work, showing the good task
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Good task"));
}

fn create_epic(dir: &TempDir, title: &str) -> String {
    let out = bea(dir)
        .args(["--json", "create", "--epic", title])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    v["id"].as_str().unwrap().to_string()
}

fn create_child_task(dir: &TempDir, title: &str, parent: &str) -> String {
    let out = bea(dir)
        .args(["--json", "create", "--parent", parent, title])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    v["id"].as_str().unwrap().to_string()
}

fn get_task_status(dir: &TempDir, id: &str) -> String {
    let out = bea(dir).args(["--json", "show", id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    v["status"].as_str().unwrap().to_string()
}

#[test]
fn test_epic_auto_close_when_all_children_done() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "My epic");
    let child1 = create_child_task(&tmp, "Child 1", &epic_id);
    let child2 = create_child_task(&tmp, "Child 2", &epic_id);

    // Complete first child — epic should stay open
    bea(&tmp).args(["done", &child1]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "open");

    // Complete second child — epic should auto-close
    bea(&tmp).args(["done", &child2]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "done");
}

#[test]
fn test_epic_not_in_ready() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Hidden epic");
    let _child = create_child_task(&tmp, "Visible child", &epic_id);

    // ready should show the child but not the epic
    bea(&tmp)
        .args(["--json", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Visible child"))
        .stdout(predicate::str::contains("Hidden epic").not());
}

#[test]
fn test_epic_stays_open_with_partial_completion() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Partial epic");
    let child1 = create_child_task(&tmp, "Done child", &epic_id);
    let _child2 = create_child_task(&tmp, "Open child", &epic_id);

    bea(&tmp).args(["done", &child1]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "open");
}

#[test]
fn test_epic_epics_command_output() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Release v2");
    let child = create_child_task(&tmp, "Write docs", &epic_id);

    // bea epics should list the epic with progress
    bea(&tmp)
        .arg("epics")
        .assert()
        .success()
        .stdout(predicate::str::contains("Epic:"))
        .stdout(predicate::str::contains("Release v2"))
        .stdout(predicate::str::contains("[0/1]"));

    // Complete child and check progress updates
    bea(&tmp).args(["done", &child]).assert().success();
    bea(&tmp)
        .args(["--json", "epics"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"done\""));
}

#[test]
fn test_epic_ready_with_epic_filter() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic1 = create_epic(&tmp, "Epic One");
    let epic2 = create_epic(&tmp, "Epic Two");
    let child1 = create_child_task(&tmp, "Task for E1", &epic1);
    let _child2 = create_child_task(&tmp, "Task for E2", &epic2);

    // --epic filters to only children of that epic
    bea(&tmp)
        .args(["--json", "ready", "--epic", &epic1])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task for E1"))
        .stdout(predicate::str::contains("Task for E2").not());

    bea(&tmp)
        .args(["--json", "ready", "--epic", &epic2])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task for E2"))
        .stdout(predicate::str::contains("Task for E1").not());

    // Unfiltered ready shows both children (but no epics)
    bea(&tmp)
        .args(["--json", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task for E1"))
        .stdout(predicate::str::contains("Task for E2"))
        .stdout(predicate::str::contains("Epic One").not())
        .stdout(predicate::str::contains("Epic Two").not());

    // Complete child1 to verify epic filter still works
    bea(&tmp).args(["done", &child1]).assert().success();
    bea(&tmp)
        .args(["--json", "ready", "--epic", &epic1])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task for E1").not());
}

#[test]
fn test_epic_show_displays_type() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "My Big Epic");

    // JSON show should include type
    bea(&tmp)
        .args(["--json", "show", &epic_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"type\": \"epic\""));

    // Human show should display "Epic:" prefix
    bea(&tmp)
        .args(["show", &epic_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Epic:"));
}

#[test]
fn test_epic_list_with_epic_filter() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic1 = create_epic(&tmp, "Epic Alpha");
    let _epic2 = create_epic(&tmp, "Epic Beta");
    create_child_task(&tmp, "Child of Alpha", &epic1);
    create_child_task(&tmp, "Standalone task", "");

    // --epic filter on list shows only children of that epic
    bea(&tmp)
        .args(["--json", "list", "--epic", &epic1])
        .assert()
        .success()
        .stdout(predicate::str::contains("Child of Alpha"));
}
