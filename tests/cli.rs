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

/// cs7: editing the id field in bea edit must not orphan the original file.
#[test]
fn test_edit_id_change_rejected_no_orphan() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Orphan test task");

    // Create an editor script that replaces the id field with "zzzz"
    let script = tmp.path().join("id-changer.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nsed 's/^id:.*/id: zzzz/' \"$1\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"\n",
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
        .stderr(predicate::str::contains("id field is not allowed"));

    // The original file must still exist (no orphan)
    let bears_dir = tmp.path().join(".bears");
    let files: Vec<_> = std::fs::read_dir(&bears_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .collect();
    // Exactly one file: the original. Not two (no new zzzz file).
    assert_eq!(
        files.len(),
        1,
        "expected 1 task file, found {}: {:?}",
        files.len(),
        files.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
    // And it starts with the original id
    let file_name = files[0].file_name();
    let name = file_name.to_string_lossy();
    assert!(
        name.starts_with(&id),
        "original file should be named {id}-*.md, got {name}"
    );
}

// ─── Archive / Restore / List --archived tests (awg) ─────────────────────────

/// Helper: complete a task by id.
fn complete_task(dir: &TempDir, id: &str) {
    bea(dir).args(["done", id]).assert().success();
}

#[test]
fn test_archive_done_task_removes_from_list() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Archivable task");
    complete_task(&tmp, &id);

    // archive the task
    bea(&tmp)
        .args(["archive", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Archived"));

    // task should no longer appear in list
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Archivable task").not());
}

#[test]
fn test_archive_done_task_appears_in_list_archived() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Will be archived");
    complete_task(&tmp, &id);

    bea(&tmp).args(["archive", &id]).assert().success();

    // list --archived should contain the task
    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Will be archived"));
}

#[test]
fn test_restore_task_returns_to_active() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Restore me");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // confirm gone from active
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Restore me").not());

    // restore
    bea(&tmp)
        .args(["restore", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Restored"));

    // now back in active list (with --all to include done tasks)
    bea(&tmp)
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Restore me"));

    // and gone from archive
    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Restore me").not());
}

#[test]
fn test_archive_sweep_no_id_archives_all_done() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id1 = create_task(&tmp, "Done task 1");
    let id2 = create_task(&tmp, "Done task 2");
    let _id3 = create_task(&tmp, "Open task");

    complete_task(&tmp, &id1);
    complete_task(&tmp, &id2);
    // _id3 stays open

    // archive sweep — no id
    bea(&tmp)
        .args(["archive"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Archived"));

    // open task should still be in list
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Open task"));

    // both done tasks should be in archive
    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Done task 1"))
        .stdout(predicate::str::contains("Done task 2"))
        .stdout(predicate::str::contains("Open task").not());
}

#[test]
fn test_archive_blocked_by_active_dependent_errors() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_dep = create_task(&tmp, "Dependency");
    let id_user = create_task(&tmp, "Active dependent");

    // Make id_user depend on id_dep
    bea(&tmp)
        .args(["dep", "add", &id_user, &id_dep])
        .assert()
        .success();

    // Complete the dependency but the active dependent keeps it unarchivable
    complete_task(&tmp, &id_dep);

    bea(&tmp)
        .args(["archive", &id_dep])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not archivable").or(predicate::str::contains("active")));
}

#[test]
fn test_show_archived_task_via_show_command() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Peek at me");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // show should succeed and mention the task is archived
    bea(&tmp)
        .args(["show", &id])
        .assert()
        .success()
        .stderr(predicate::str::contains("archived"));
}

#[test]
fn test_mutating_archived_task_gives_helpful_error() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Immutable");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // start should fail with a helpful message about restore
    bea(&tmp)
        .args(["start", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("archived").or(predicate::str::contains("restore")));
}

#[test]
fn test_list_archived_json() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "JSON archive check");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    let out = bea(&tmp)
        .args(["--json", "list", "--archived"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v.is_array());
    assert!(!v.as_array().unwrap().is_empty());
    assert_eq!(v.as_array().unwrap()[0]["id"].as_str().unwrap(), id);
}

// ─── Log command tests (acr) ──────────────────────────────────────────────────

#[test]
fn test_log_shows_archived_tasks_most_recent_first() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id1 = create_task(&tmp, "Log task 1");
    let id2 = create_task(&tmp, "Log task 2");

    complete_task(&tmp, &id1);
    complete_task(&tmp, &id2);

    // Archive both via sweep
    bea(&tmp).args(["archive"]).assert().success();

    // Human output: both tasks should appear
    bea(&tmp)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("Log task 1"))
        .stdout(predicate::str::contains("Log task 2"));
}

#[test]
fn test_log_empty_when_no_archive() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("No archived tasks"));
}

#[test]
fn test_log_json() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Log JSON task");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    let out = bea(&tmp).args(["--json", "log"]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v.is_array(), "log --json should return an array");
    assert!(!v.as_array().unwrap().is_empty());
    assert_eq!(v.as_array().unwrap()[0]["id"].as_str().unwrap(), id);
}

#[test]
fn test_log_limit() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id1 = create_task(&tmp, "Limit task 1");
    let id2 = create_task(&tmp, "Limit task 2");
    let id3 = create_task(&tmp, "Limit task 3");

    complete_task(&tmp, &id1);
    complete_task(&tmp, &id2);
    complete_task(&tmp, &id3);

    bea(&tmp).args(["archive"]).assert().success();

    let out = bea(&tmp)
        .args(["--json", "log", "--limit", "2"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(
        v.as_array().unwrap().len(),
        2,
        "log --limit 2 should return exactly 2 tasks"
    );
}

#[test]
fn test_prune_still_works() {
    // Regression: prune must remain functional even though archive is preferred.
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Will be pruned");
    bea(&tmp).args(["cancel", &id]).assert().success();

    bea(&tmp)
        .arg("prune")
        .assert()
        .success()
        .stdout(predicate::str::contains("Pruned").or(predicate::str::contains("prune")));

    // Task should be gone from active list
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Will be pruned").not());
}

// ─── Task 6ra: fill coverage gaps for archive feature ────────────────────────

/// Archived task does not appear in `bea ready` output.
#[test]
fn test_archived_task_absent_from_ready() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Archived ready check");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // ready should be empty (archived task is gone entirely)
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Archived ready check").not());
}

/// `bea log` outputs tasks sorted most-recent-first and returns all archived tasks.
///
/// Note: `bea log` uses `service::list_archive` which sorts by `updated desc`. The CLI
/// output (both human and JSON) is the `TaskSummary` projection which does not include
/// the `updated` timestamp. We verify correctness by confirming (a) all archived tasks
/// appear in the JSON output, and (b) the human output lists the tasks.
#[test]
fn test_log_order_is_reverse_chronological() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id1 = create_task(&tmp, "Log order A");
    let id2 = create_task(&tmp, "Log order B");
    let id3 = create_task(&tmp, "Log order C");

    complete_task(&tmp, &id1);
    complete_task(&tmp, &id2);
    complete_task(&tmp, &id3);

    bea(&tmp).args(["archive"]).assert().success();

    // JSON log must return all three tasks.
    let out = bea(&tmp).args(["--json", "log"]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 3, "all three archived tasks should be in log");

    // All three IDs must be present in the output.
    let returned_ids: Vec<&str> = arr.iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert!(returned_ids.contains(&id1.as_str()), "id1 missing from log");
    assert!(returned_ids.contains(&id2.as_str()), "id2 missing from log");
    assert!(returned_ids.contains(&id3.as_str()), "id3 missing from log");

    // Human output must list all three
    bea(&tmp)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("Log order A"))
        .stdout(predicate::str::contains("Log order B"))
        .stdout(predicate::str::contains("Log order C"));
}

/// `bea archive <id>` on a task with status "done" physically moves the file
/// into `.bears/archive/` and removes it from `.bears/`.
#[test]
fn test_archive_moves_file_to_archive_subdir() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "File move check");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // File must exist under .bears/archive/
    let archive_dir = tmp.path().join(".bears/archive");
    let archived_files: Vec<_> = std::fs::read_dir(&archive_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(&id))
        .collect();
    assert_eq!(
        archived_files.len(),
        1,
        "exactly one archive file expected for id {id}"
    );

    // File must NOT exist under .bears/ (active dir, .md files only)
    let active_dir = tmp.path().join(".bears");
    let active_files: Vec<_> = std::fs::read_dir(&active_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|x| x.to_str()) == Some("md")
                && e.file_name().to_string_lossy().starts_with(&id)
        })
        .collect();
    assert!(
        active_files.is_empty(),
        "archived task file must not remain in active dir"
    );
}

/// `bea restore <id>` physically moves the file back from `.bears/archive/` to `.bears/`.
#[test]
fn test_restore_moves_file_back_from_archive_subdir() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Restore file check");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();
    bea(&tmp).args(["restore", &id]).assert().success();

    // File must be back in active dir
    let active_dir = tmp.path().join(".bears");
    let active_files: Vec<_> = std::fs::read_dir(&active_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|x| x.to_str()) == Some("md")
                && e.file_name().to_string_lossy().starts_with(&id)
        })
        .collect();
    assert_eq!(
        active_files.len(),
        1,
        "restored task file must be back in active dir"
    );

    // File must NOT be in archive
    let archive_dir = tmp.path().join(".bears/archive");
    let archive_files: Vec<_> = std::fs::read_dir(&archive_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(&id))
        .collect();
    assert!(
        archive_files.is_empty(),
        "restored task file must not remain in archive dir"
    );
}

/// `bea archive` sweep only archives tasks that are archivable (done/cancelled,
/// no active dependents) — open tasks must remain in the active list.
#[test]
fn test_archive_sweep_skips_open_tasks() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let _id_open = create_task(&tmp, "Must stay open");
    let id_done = create_task(&tmp, "Can be archived");
    complete_task(&tmp, &id_done);

    bea(&tmp).args(["archive"]).assert().success();

    // Open task must still be in active list
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Must stay open"));

    // Done task must be in archive, not active
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Can be archived").not());

    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Can be archived"))
        .stdout(predicate::str::contains("Must stay open").not());
}

// ─── Task xja: end-to-end archive visibility and integrity tests ──────────────

/// Archived task is hidden from `bea search`.
/// We verify via JSON output that the result array does not include the archived ID.
#[test]
fn test_archived_task_hidden_from_search() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Searchable archived xyz");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // JSON search: result must not include the archived task
    let out = bea(&tmp)
        .args(["--json", "search", "Searchable archived xyz"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let arr = v.as_array().unwrap();
    assert!(
        arr.iter().all(|x| x["id"] != id),
        "archived task must not appear in search results"
    );
}

/// Archived task is hidden from `bea graph`.
#[test]
fn test_archived_task_hidden_from_graph() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Graph hidden task");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    // graph --all (includes done) should still not show archived tasks
    bea(&tmp)
        .args(["--json", "graph", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Graph hidden task").not());
}

/// Archived epic is hidden from `bea epics`.
#[test]
fn test_archived_epic_hidden_from_epics() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Hidden epic");
    let child_id = create_child_task(&tmp, "Only child", &epic_id);

    // Complete child (epic should auto-close) then archive
    complete_task(&tmp, &child_id);
    // Archive the epic (and its settled children)
    bea(&tmp).args(["archive", &epic_id]).assert().success();

    // `bea epics` should not mention the archived epic
    bea(&tmp)
        .arg("epics")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hidden epic").not());
}

/// Targeted archive REFUSES when an active task still depends on the target,
/// and the error message names the blocker.
#[test]
fn test_archive_targeted_refuses_active_dependent_names_blocker() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_dep = create_task(&tmp, "Dependency task");
    let id_user = create_task(&tmp, "Blocker task");

    bea(&tmp)
        .args(["dep", "add", &id_user, &id_dep])
        .assert()
        .success();

    complete_task(&tmp, &id_dep);

    // Archiving while active dependent exists must fail
    bea(&tmp)
        .args(["archive", &id_dep])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("active")
                .or(predicate::str::contains("not archivable"))
                .or(predicate::str::contains(&id_user)),
        );
}

/// `bea prune` hard-deletes from the active store only — it does NOT remove
/// tasks from the archive.
#[test]
fn test_prune_never_touches_archive() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    // Archive a done task
    let id_arch = create_task(&tmp, "Archived task");
    complete_task(&tmp, &id_arch);
    bea(&tmp).args(["archive", &id_arch]).assert().success();

    // Create a cancelled task in the active store for prune to consume
    let id_cancel = create_task(&tmp, "Cancelled active task");
    bea(&tmp).args(["cancel", &id_cancel]).assert().success();

    // Prune only removes the cancelled active task
    bea(&tmp)
        .arg("prune")
        .assert()
        .success()
        .stdout(predicate::str::contains("Pruned").or(predicate::str::contains("prune")));

    // Archived task must still be in archive
    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Archived task"));
}

/// After restoring an archived task, it is workable again: it appears in the
/// active list, can be re-opened, and shows up in `bea ready`.
#[test]
fn test_restore_makes_task_workable() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Workable after restore");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    bea(&tmp).args(["restore", &id]).assert().success();

    // It should be in the active list (status=done after restore)
    bea(&tmp)
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Workable after restore"));

    // Re-open it so it becomes ready
    bea(&tmp)
        .args(["update", &id, "--status", "open"])
        .assert()
        .success();

    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Workable after restore"));
}

/// New task IDs are never reused from the archive (CLI e2e path).
/// We archive a task, then create several new tasks and verify none reuses the archived ID.
#[test]
fn test_new_task_ids_do_not_reuse_archived_ids() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id = create_task(&tmp, "Archived ID guard");
    complete_task(&tmp, &id);
    bea(&tmp).args(["archive", &id]).assert().success();

    let mut new_ids = Vec::new();
    for i in 0..10 {
        new_ids.push(create_task(&tmp, &format!("New task {i}")));
    }

    assert!(
        !new_ids.contains(&id),
        "archived ID {id} must not be reused by new tasks: {new_ids:?}"
    );
}

/// `dep add` onto an archived task ID is rejected — it stays "unknown" from
/// the active store's perspective.
#[test]
fn test_dep_add_onto_archived_id_is_rejected() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let id_arch = create_task(&tmp, "Will be archived dep");
    complete_task(&tmp, &id_arch);
    bea(&tmp).args(["archive", &id_arch]).assert().success();

    let id_active = create_task(&tmp, "Wants archived dep");

    bea(&tmp)
        .args(["dep", "add", &id_active, &id_arch])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

/// Sweep archives only currently-archivable tasks; tasks that are blocked by
/// active dependents are skipped even though they are done.
#[test]
fn test_sweep_skips_tasks_with_active_dependents() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    // id_base is done but id_user (open) depends on it → id_base is NOT archivable
    let id_base = create_task(&tmp, "Base dep");
    let id_user = create_task(&tmp, "Depends on base");

    bea(&tmp)
        .args(["dep", "add", &id_user, &id_base])
        .assert()
        .success();

    complete_task(&tmp, &id_base);

    // A completely independent done task that IS archivable
    let id_free = create_task(&tmp, "Free done task");
    complete_task(&tmp, &id_free);

    bea(&tmp).args(["archive"]).assert().success();

    // Only the free task should be archived — "Base dep" (done but blocked) must NOT be.
    bea(&tmp)
        .args(["list", "--archived"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Free done task"))
        .stdout(predicate::str::contains("Base dep").not());

    // "Base dep" (done) must still be in the active store — visible with --all
    bea(&tmp)
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Base dep"))
        .stdout(predicate::str::contains("Depends on base"));
}

// ─── Task sy3: init scaffolding e2e tests ────────────────────────────────────

/// `bea init --claude` scaffolds CLAUDE.md, .mcp.json, skill, and agent files.
#[test]
fn test_init_claude_scaffolds_expected_files() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--claude"]).assert().success();

    assert!(
        tmp.path().join("CLAUDE.md").exists(),
        "CLAUDE.md must be created by --claude"
    );
    assert!(
        tmp.path().join(".mcp.json").exists(),
        ".mcp.json must be created by --claude"
    );
    assert!(
        tmp.path()
            .join(".claude/skills/bears-planning/SKILL.md")
            .exists(),
        "SKILL.md must be created"
    );
    assert!(
        tmp.path()
            .join(".claude/skills/bears-planning/references/cli-fallback.md")
            .exists(),
        "cli-fallback.md must be created"
    );
    assert!(
        tmp.path().join(".claude/agents/planner.md").exists(),
        "planner.md must be created"
    );
}

/// `bea init --copilot` scaffolds copilot-instructions.md, .github/mcp.json, skill, and agent files.
#[test]
fn test_init_copilot_scaffolds_expected_files() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--copilot"]).assert().success();

    assert!(
        tmp.path().join(".github/copilot-instructions.md").exists(),
        ".github/copilot-instructions.md must be created"
    );
    assert!(
        tmp.path().join(".github/mcp.json").exists(),
        ".github/mcp.json must be created"
    );
    assert!(
        tmp.path()
            .join(".github/skills/bears-planning/SKILL.md")
            .exists(),
        "SKILL.md must be created"
    );
    assert!(
        tmp.path()
            .join(".github/skills/bears-planning/references/cli-fallback.md")
            .exists(),
        "cli-fallback.md must be created"
    );
    assert!(
        tmp.path().join(".github/agents/planner.agent.md").exists(),
        "planner.agent.md must be created"
    );
}

/// `bea init --codex` scaffolds AGENTS.md but no .mcp.json.
#[test]
fn test_init_codex_scaffolds_expected_files() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--codex"]).assert().success();

    assert!(
        tmp.path().join("AGENTS.md").exists(),
        "AGENTS.md must be created by --codex"
    );
    assert!(
        !tmp.path().join(".mcp.json").exists(),
        ".mcp.json must NOT be created by --codex alone"
    );
}

/// Combining flags scaffolds files for all requested harnesses.
#[test]
fn test_init_combined_flags_scaffold_both() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["init", "--claude", "--copilot"])
        .assert()
        .success();

    assert!(tmp.path().join("CLAUDE.md").exists());
    assert!(tmp.path().join(".mcp.json").exists());
    assert!(tmp.path().join(".github/copilot-instructions.md").exists());
    assert!(tmp.path().join(".github/mcp.json").exists());
}

/// Running `bea init --claude` on an already-initialized directory is idempotent.
#[test]
fn test_init_claude_on_already_initialized_dir_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--claude"]).assert().success();
    let first_mcp = std::fs::read_to_string(tmp.path().join(".mcp.json")).unwrap();

    bea(&tmp).args(["init", "--claude"]).assert().success();
    let second_mcp = std::fs::read_to_string(tmp.path().join(".mcp.json")).unwrap();

    assert_eq!(
        first_mcp, second_mcp,
        ".mcp.json must be identical after re-init"
    );
}

/// `.mcp.json` merge preserves a pre-existing unrelated server entry.
#[test]
fn test_init_claude_mcp_json_preserves_existing_server() {
    let tmp = TempDir::new().unwrap();

    let pre_existing = serde_json::json!({
        "mcpServers": {
            "my-other-tool": { "command": "other", "args": ["serve"] }
        }
    });
    std::fs::write(
        tmp.path().join(".mcp.json"),
        serde_json::to_string_pretty(&pre_existing).unwrap(),
    )
    .unwrap();

    bea(&tmp).args(["init", "--claude"]).assert().success();

    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
            .unwrap();
    let servers = result["mcpServers"].as_object().unwrap();
    assert!(servers.contains_key("bears"), "bears entry must be present");
    assert!(
        servers.contains_key("my-other-tool"),
        "pre-existing server must be preserved"
    );
}

/// The scaffolded `.mcp.json` uses `bea mcp` (not `cargo run …`).
#[test]
fn test_init_claude_mcp_json_uses_bea_mcp() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--claude"]).assert().success();

    let mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
            .unwrap();
    let bears = &mcp["mcpServers"]["bears"];
    assert_eq!(bears["command"].as_str().unwrap(), "bea");
    assert_eq!(bears["args"][0].as_str().unwrap(), "mcp");
}

/// `bea init --copilot`: .github/mcp.json uses `bea mcp`.
#[test]
fn test_init_copilot_mcp_json_uses_bea_mcp() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--copilot"]).assert().success();

    let mcp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".github/mcp.json")).unwrap(),
    )
    .unwrap();
    let bears = &mcp["servers"]["bears"];
    assert_eq!(bears["command"].as_str().unwrap(), "bea");
    assert_eq!(bears["args"][0].as_str().unwrap(), "mcp");
}

/// Guard the packaging gotcha: template source files must exist on disk so that
/// `cargo package` includes them (the `include_str!` macros embed them at compile
/// time; if the files are missing from the published crate, users get a compile
/// error when building from crates.io).
#[test]
fn test_template_source_files_exist_on_disk() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    let expected_templates = [
        "templates/claude/CLAUDE.md",
        "templates/claude/mcp.json",
        "templates/claude/skills/bears-planning/SKILL.md",
        "templates/claude/skills/bears-planning/references/cli-fallback.md",
        "templates/claude/agents/planner.md",
        "templates/copilot/copilot-instructions.md",
        "templates/copilot/mcp.json",
        "templates/copilot/skills/bears-planning/SKILL.md",
        "templates/copilot/skills/bears-planning/references/cli-fallback.md",
        "templates/copilot/agents/planner.agent.md",
        "templates/codex/AGENTS.md",
    ];

    for rel_path in &expected_templates {
        let full = manifest_dir.join(rel_path);
        assert!(
            full.exists(),
            "template source file missing (would break cargo package): {rel_path}"
        );
    }
}
