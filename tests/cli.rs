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

    // Corrupting the frontmatter is an error: non-zero exit
    bea(&tmp)
        .env("EDITOR", script.to_str().unwrap())
        .args(["edit", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid frontmatter"));
}

/// Changing the task ID in the editor is rejected — the original ID is kept.
#[test]
fn test_edit_cannot_change_id() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let out = bea(&tmp)
        .args(["--json", "create", "Sticky id"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // Editor script that rewrites the id field
    let script = tmp.path().join("id-editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nsed 's/^id:.*/id: hacked/' \"$1\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"\n",
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
        .stderr(
            predicate::str::contains("ID cannot be changed")
                .or(predicate::str::contains("cannot be changed via edit")),
        );

    // Original ID still resolves; the injected one does not
    bea(&tmp).args(["show", &id]).assert().success();
    bea(&tmp).args(["show", "hacked"]).assert().failure();
}

/// Creating a task with a nonexistent or non-epic parent is rejected.
#[test]
fn test_create_validates_parent() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    bea(&tmp)
        .args(["create", "Orphan", "--parent", "zzzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));

    let plain = create_task(&tmp, "Plain task");
    bea(&tmp)
        .args(["create", "Child of task", "--parent", &plain])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not an epic"));
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

/// Deleting a task scrubs it from dependents' depends_on, so the
/// dependent becomes ready instead of being silently blocked forever.
#[test]
fn test_delete_unblocks_dependents() {
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

    // The dangling reference is removed and the dependent is ready
    let out = bea(&tmp)
        .args(["--json", "show", &id_task])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v["depends_on"].as_array().unwrap().is_empty());

    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Depends on deleted"));
}

/// Deleting an epic detaches its children to fresh root IDs and prints the
/// renames, in text and in `--json`.
#[test]
fn test_delete_epic_detaches_children() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic = create_epic(&tmp, "Doomed epic");
    let child = create_child_task(&tmp, "Survivor", &epic);
    let out = bea(&tmp)
        .args(["--json", "delete", &epic])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(v["id"], epic.as_str());
    let new_id = v["renamed"][&child].as_str().unwrap().to_string();
    assert!(!new_id.contains('.'), "{new_id}");

    let out = bea(&tmp)
        .args(["--json", "show", &new_id])
        .output()
        .unwrap();
    let shown: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(shown["title"], "Survivor");
    assert!(shown.get("parent").is_none_or(|p| p.is_null()), "{shown}");

    // Text output lists the renames under the deletion line.
    let epic = create_epic(&tmp, "Second epic");
    let child = create_child_task(&tmp, "Second survivor", &epic);
    let out = bea(&tmp).args(["delete", &epic]).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains(&format!("Deleted task {epic}")), "{stdout}");
    assert!(stdout.contains(&format!("renamed {child} → ")), "{stdout}");

    // A task without children prints no rename and no `renamed` key.
    let plain = create_task(&tmp, "Plain");
    let out = bea(&tmp)
        .args(["--json", "delete", &plain])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert!(v.get("renamed").is_none(), "{v}");
}

/// Pruning an epic detaches only the children that survive the prune; the
/// `--json` output becomes `{ pruned, renamed }` when something moved.
#[test]
fn test_prune_epic_detaches_surviving_children() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic = create_epic(&tmp, "Cancelled epic");
    let keep = create_child_task(&tmp, "Keep", &epic);
    let drop = create_child_task(&tmp, "Drop", &epic);
    bea(&tmp).args(["cancel", &drop]).assert().success();
    bea(&tmp).args(["cancel", &epic]).assert().success();

    let out = bea(&tmp).args(["--json", "prune"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let mut pruned: Vec<_> = v["pruned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    pruned.sort();
    let mut expected = vec![epic.clone(), drop.clone()];
    expected.sort();
    assert_eq!(pruned, expected);
    let renamed = v["renamed"].as_object().unwrap();
    assert_eq!(renamed.len(), 1, "{v}");
    let new_id = renamed[&keep].as_str().unwrap();
    assert!(!new_id.contains('.'), "{new_id}");
    bea(&tmp)
        .args(["show", new_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Keep"));

    // Text output, and a prune with nothing to detach keeps the plain list.
    let epic = create_epic(&tmp, "Another epic");
    let keep = create_child_task(&tmp, "Keep again", &epic);
    bea(&tmp).args(["cancel", &epic]).assert().success();
    bea(&tmp)
        .arg("prune")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Pruned {epic}")))
        .stdout(predicate::str::contains(format!("renamed {keep} → ")));

    let lone = create_task(&tmp, "Lone");
    bea(&tmp).args(["cancel", &lone]).assert().success();
    let out = bea(&tmp).args(["--json", "prune"]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1, "{v}");
}

/// Re-running `bea init` must not reset an existing .bears.yml.
#[test]
fn test_init_idempotent_preserves_config() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    std::fs::write(tmp.path().join(".bears.yml"), "id-length: 6\n").unwrap();
    bea(&tmp).arg("init").assert().success();

    let content = std::fs::read_to_string(tmp.path().join(".bears.yml")).unwrap();
    assert!(content.contains("id-length: 6"), "config was clobbered");
}

/// Completing a child via `update --status done` auto-closes the epic,
/// same as `bea done`.
#[test]
fn test_epic_auto_close_via_update_status() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Update epic");
    let child = create_child_task(&tmp, "Only child", &epic_id);

    bea(&tmp)
        .args(["update", &child, "--status", "done"])
        .assert()
        .success();
    assert_eq!(get_task_status(&tmp, &epic_id), "done");
}

/// Re-completing an already-done child must not close an epic that
/// still has open children.
#[test]
fn test_epic_not_closed_by_recompletion() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Recompletion epic");
    let child1 = create_child_task(&tmp, "Done child", &epic_id);
    let _child2 = create_child_task(&tmp, "Open child", &epic_id);

    bea(&tmp).args(["done", &child1]).assert().success();
    bea(&tmp).args(["done", &child1]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "open");
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

/// A child's ID is its parent's ID plus a segment; the file has no `parent:`
/// key, and `show` still reports the parent.
#[test]
fn test_create_child_gets_hierarchical_id() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "My epic");
    let child = create_child_task(&tmp, "Child", &epic_id);
    let (parent, leaf) = child.rsplit_once('.').expect("dotted child ID");
    assert_eq!(parent, epic_id);
    assert_eq!(leaf.len(), 2, "{child}");

    let file = std::fs::read_dir(tmp.path().join(".bears"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{child}-"))
        })
        .expect("child file");
    let content = std::fs::read_to_string(file).unwrap();
    assert!(!content.contains("parent"), "{content}");

    let out = bea(&tmp).args(["--json", "show", &child]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["parent"], epic_id.as_str());
    bea(&tmp)
        .args(["show", &child])
        .assert()
        .success()
        .stdout(predicate::str::contains("Parent:"));
}

/// A short prefix of an epic is not ambiguous with the epic's own children,
/// in the active store and in the archive alike.
#[test]
fn test_prefix_of_epic_resolves_past_its_children() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "Prefix epic");
    let prefix = &epic_id[..2];
    let out = bea(&tmp)
        .args(["--json", "create", "--epic", "--parent", prefix, "Sub-epic"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let child = v["id"].as_str().unwrap().to_string();
    assert!(child.starts_with(&format!("{epic_id}.")), "{child}");
    let grandchild = create_child_task(&tmp, "Grandchild", &child);
    let leaf = child.rsplit_once('.').unwrap().1;
    let inside_child = format!("{epic_id}.{}", &leaf[..1]);

    let show = |id: &str| -> serde_json::Value {
        let out = bea(&tmp).args(["--json", "show", id]).output().unwrap();
        assert!(out.status.success(), "{id}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    };
    assert_eq!(show(prefix)["id"], epic_id.as_str());
    assert_eq!(show(&inside_child)["id"], child.as_str());

    // Archive the finished tree, then restore it by the epic's prefix.
    bea(&tmp).args(["done", &grandchild]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "done");
    bea(&tmp).arg("archive").assert().success();
    bea(&tmp).args(["restore", prefix]).assert().success();
    assert_eq!(get_task_status(&tmp, &epic_id), "done");
    bea(&tmp)
        .args(["restore", &inside_child])
        .assert()
        .success();
    assert_eq!(get_task_status(&tmp, &child), "done");

    // An empty ID matches everything and is rejected, not resolved to the root.
    bea(&tmp)
        .args(["show", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must not be empty"));

    // Two different tasks under one prefix are still ambiguous.
    create_child_task(&tmp, "Second child", &epic_id);
    bea(&tmp)
        .args(["show", &format!("{epic_id}.")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
}

/// `update --parent` moves a task: it and its subtree are renamed, references
/// to them are rewritten (in the archive too), and the renames are reported.
#[test]
fn test_update_parent_moves_and_renames() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let epic_id = create_epic(&tmp, "My epic");
    let other_epic = create_epic(&tmp, "Other epic");
    let child = create_child_task(&tmp, "Child", &epic_id);
    let root = create_task(&tmp, "Root");
    // An archived task that depends on the child.
    let archived = create_task(&tmp, "Archived user");
    bea(&tmp)
        .args(["dep", "add", &archived, &child])
        .assert()
        .success();
    bea(&tmp).args(["done", &archived]).assert().success();
    bea(&tmp).args(["archive", &archived]).assert().success();

    // --json reports the task under its new ID and the rename map.
    let out = bea(&tmp)
        .args(["--json", "update", &child, "--parent", &other_epic])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let moved = v["id"].as_str().unwrap().to_string();
    assert!(moved.starts_with(&format!("{other_epic}.")), "{moved}");
    assert_eq!(v["renamed"][&child], moved.as_str());
    bea(&tmp).args(["show", &child]).assert().failure();
    let archive_dir = tmp.path().join(".bears").join("archive");
    let archived_file = std::fs::read_dir(&archive_dir)
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with(&archived))
        .unwrap();
    let content = std::fs::read_to_string(archived_file.path()).unwrap();
    assert!(content.contains(&moved), "{content}");

    // Human output names the old and new IDs.
    let out = bea(&tmp)
        .args(["update", &root, "--parent", &epic_id, "--title", "Renamed"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains(&format!("renamed {root} → {epic_id}.")),
        "{stdout}"
    );

    // Detaching gives a fresh root ID; the current parent is a no-op.
    let out = bea(&tmp)
        .args(["--json", "update", &moved, "--parent", ""])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!v["id"].as_str().unwrap().contains('.'), "{v}");
    let out = bea(&tmp)
        .args(["--json", "update", &epic_id, "--parent", ""])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], epic_id.as_str());
    assert!(v.get("renamed").is_none(), "{v}");

    // A task cannot move under itself.
    bea(&tmp)
        .args(["update", &epic_id, "--parent", &epic_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lies below it"));
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
        .stderr(predicate::str::contains("ID cannot be changed via edit"));

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

/// Re-running `bea init --claude` non-interactively (no TTY) preserves an edited
/// instruction file instead of clobbering it — and does not block on stdin.
#[test]
fn test_init_claude_non_interactive_preserves_edited_instruction() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--claude"]).assert().success();

    std::fs::write(tmp.path().join("CLAUDE.md"), "MY EDITS").unwrap();

    // Second run under assert_cmd has no TTY → SkipExisting, no prompt, no hang.
    bea(&tmp).args(["init", "--claude"]).assert().success();

    assert_eq!(
        std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap(),
        "MY EDITS",
        "edited instruction file must be preserved on non-interactive re-init"
    );

    // .mcp.json still has the bears entry.
    let mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
            .unwrap();
    assert!(mcp["mcpServers"]["bears"].is_object());
}

/// `bea init --claude --force` overwrites an edited instruction file.
#[test]
fn test_init_claude_force_overwrites() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).args(["init", "--claude"]).assert().success();
    std::fs::write(tmp.path().join("CLAUDE.md"), "MY EDITS").unwrap();

    bea(&tmp)
        .args(["init", "--claude", "--force"])
        .assert()
        .success();

    let md = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
    assert!(!md.contains("MY EDITS"), "force must overwrite edits");
    assert!(md.contains("Bears"), "template content must be restored");
}

/// `bea agent instructions --claude` writes only the instruction file — no
/// `.mcp.json`, and no `.bears/` directory.
#[test]
fn test_agent_instructions_only() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "instructions", "--claude"])
        .assert()
        .success();

    assert!(tmp.path().join("CLAUDE.md").exists());
    assert!(!tmp.path().join(".mcp.json").exists());
    assert!(
        !tmp.path().join(".bears").exists(),
        "agent must not create .bears/"
    );
}

/// `bea agent skills --claude` writes skill/agent files and `.mcp.json`, but not
/// the instruction file.
#[test]
fn test_agent_skills_creates_mcp() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "skills", "--claude"])
        .assert()
        .success();

    assert!(!tmp.path().join("CLAUDE.md").exists());
    assert!(tmp.path().join(".mcp.json").exists());
    assert!(
        tmp.path()
            .join(".claude/skills/bears-planning/SKILL.md")
            .exists()
    );
}

/// `bea agent all --claude` scaffolds the same agent files as `bea init --claude`.
#[test]
fn test_agent_all_matches_init() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "all", "--claude"])
        .assert()
        .success();

    assert!(tmp.path().join("CLAUDE.md").exists());
    assert!(tmp.path().join(".mcp.json").exists());
    assert!(
        tmp.path()
            .join(".claude/skills/bears-planning/SKILL.md")
            .exists()
    );
    assert!(tmp.path().join(".claude/agents/planner.md").exists());
}

/// `bea agent skills --append` is rejected (no instruction file to append to).
#[test]
fn test_agent_skills_append_rejected() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "skills", "--claude", "--append"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only valid for"));
}

/// `--force` and `--append` cannot be combined (clap-level conflict).
#[test]
fn test_agent_force_append_conflict() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "instructions", "--claude", "--force", "--append"])
        .assert()
        .failure();
}

/// `bea agent` with no harness selected is a usage error.
#[test]
fn test_agent_requires_harness() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .args(["agent", "instructions"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least one harness"));
}

/// `bea agent instructions --claude --append` preserves user text, appends the
/// template, and is idempotent on a second run.
#[test]
fn test_agent_instructions_append_preserves_and_idempotent() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("CLAUDE.md"), "MY NOTES").unwrap();

    bea(&tmp)
        .args(["agent", "instructions", "--claude", "--append"])
        .assert()
        .success();

    let after_first = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
    assert!(after_first.contains("MY NOTES"));
    assert!(after_first.contains("Bears"), "template must be appended");

    // Second append must be a no-op (marker guard).
    bea(&tmp)
        .args(["agent", "instructions", "--claude", "--append"])
        .assert()
        .success();
    let after_second = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
    assert_eq!(after_first, after_second, "re-append must not duplicate");
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

#[test]
fn test_start_with_assignee_and_release() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let output = bea(&tmp)
        .args(["--json", "create", "Claimable task"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    bea(&tmp)
        .args(["start", &id, "--assignee", "agent-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("in_progress"))
        .stdout(predicate::str::contains("(agent-1)"));

    bea(&tmp)
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent-1"));

    bea(&tmp)
        .args(["release", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("open"))
        .stdout(predicate::str::contains("(unassigned)"));

    let output = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(v["status"], "open");
    assert!(
        v.get("assignee").is_none(),
        "assignee should be cleared: {v}"
    );

    // Released work is available to the next worker.
    bea(&tmp)
        .args(["ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claimable task"));
}

#[test]
fn test_start_without_assignee_keeps_existing_one() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let output = bea(&tmp)
        .args(["--json", "create", "Sticky assignee"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    bea(&tmp)
        .args(["start", &id, "-a", "agent-1"])
        .assert()
        .success();
    bea(&tmp).args(["release", &id]).assert().success();
    bea(&tmp)
        .args(["update", &id, "--assignee", "agent-2"])
        .assert()
        .success();
    bea(&tmp)
        .args(["start", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent-2"));
}

#[test]
fn test_release_requires_in_progress() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let output = bea(&tmp)
        .args(["--json", "create", "Not started"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    bea(&tmp)
        .args(["release", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot release"))
        .stderr(predicate::str::contains("hint:"));
}

#[test]
fn test_attempts_counter() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();

    let output = bea(&tmp)
        .args(["--json", "create", "Flaky task"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    assert!(
        v.get("attempts").is_none(),
        "fresh task has no counter: {v}"
    );

    bea(&tmp)
        .args(["start", &id, "-a", "agent-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(agent-1)"));
    bea(&tmp).args(["release", &id]).assert().success();

    // Second attempt is announced as such.
    bea(&tmp)
        .args(["start", &id, "-a", "agent-2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(agent-2, attempt 2)"));

    bea(&tmp)
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Attempts: 2"));

    let output = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(v["attempts"], 2);

    // Completing does not disturb the count.
    bea(&tmp).args(["done", &id]).assert().success();
    let output = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(v["attempts"], 2);
    assert_eq!(v["status"], "done");
}

/// Create a task and return its ID.
fn create(tmp: &TempDir, title: &str) -> String {
    let output = bea(tmp).args(["--json", "create", title]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    v["id"].as_str().unwrap().to_string()
}

#[test]
fn test_review_queue_and_transitions() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let id = create(&tmp, "Add OAuth flow");

    bea(&tmp)
        .args(["start", &id, "-a", "linus"])
        .assert()
        .success();
    bea(&tmp)
        .args(["review", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("review (awaiting review)"));

    // Out of ready, into the review queue.
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks ready."));
    bea(&tmp)
        .arg("review")
        .assert()
        .success()
        .stdout(predicate::str::contains("Add OAuth flow"))
        .stdout(predicate::str::contains("(linus)"));

    // Changes requested sends it back to the pool.
    bea(&tmp)
        .args(["reject", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("open (changes requested)"));
    bea(&tmp)
        .arg("review")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing awaiting review."));
    let output = bea(&tmp).args(["--json", "show", &id]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(v["status"], "open");
    assert!(
        v["assignee"].is_null(),
        "reject hands the task back to the pool"
    );

    // Second pass: approve it.
    bea(&tmp).args(["start", &id]).assert().success();
    bea(&tmp).args(["review", &id]).assert().success();
    bea(&tmp).args(["done", &id]).assert().success();
    bea(&tmp)
        .arg("review")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing awaiting review."));
}

#[test]
fn test_review_requires_in_progress() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let id = create(&tmp, "Not started");

    bea(&tmp)
        .args(["review", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot review"))
        .stderr(predicate::str::contains("expected status in_progress"))
        .stderr(predicate::str::contains("hint:"));

    // The escape hatch still works.
    bea(&tmp).args(["status", &id, "review"]).assert().success();
    bea(&tmp)
        .arg("review")
        .assert()
        .success()
        .stdout(predicate::str::contains("Not started"));
}

#[test]
fn test_proposals_are_hidden_until_accepted() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let id = create(&tmp, "Maybe worth doing");
    let other = create(&tmp, "Real backlog item");

    bea(&tmp)
        .args(["propose", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("proposed (awaiting acceptance)"));

    // Hidden from the backlog and never ready...
    bea(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Real backlog item"))
        .stdout(predicate::str::contains("Maybe worth doing").not());
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Maybe worth doing").not());

    // ...but findable on purpose.
    bea(&tmp)
        .args(["list", "--status", "proposed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Maybe worth doing"));
    bea(&tmp)
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Maybe worth doing"));

    bea(&tmp)
        .args(["accept", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("open (accepted)"));
    bea(&tmp)
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("Maybe worth doing"));

    // `other` exists only to prove the backlog is not empty above.
    assert!(!other.is_empty());
}

/// `bea show | head` must not panic when the reader goes away.
///
/// Rust ignores SIGPIPE at startup, so `println!` to a closed pipe used to
/// panic with "failed printing to stdout: Broken pipe".
#[test]
#[cfg(unix)]
fn test_broken_pipe_exits_quietly() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command as StdCommand, Stdio};

    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let id = create(&tmp, "Big body");

    // A body far larger than the 64K pipe buffer, so the child is still
    // writing when the reader closes its end.
    let file = std::fs::read_dir(tmp.path().join(".bears"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "md"))
        .expect("task file");
    let mut body = String::new();
    for i in 0..20_000 {
        body.push_str(&format!("line {i} of a very long body\n"));
    }
    let existing = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{existing}\n{body}")).unwrap();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("bea"))
        .current_dir(tmp.path())
        .args(["show", &id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Read one line, then close the read end — this is what `head -1` does.
    {
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("Big body"), "unexpected first line: {line}");
    }

    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked"),
        "broken pipe should not panic, got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Broken pipe"),
        "broken pipe should not be reported, got stderr: {stderr}"
    );
    assert_ne!(
        out.status.code(),
        Some(101),
        "process should not exit with the panic status"
    );
}

// -- `bea doctor` --------------------------------------------------------------

/// Create a task and return its ID and file path.
fn doctor_task(dir: &TempDir, title: &str) -> (String, std::path::PathBuf) {
    let out = bea(dir).args(["--json", "create", title]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let path = std::fs::read_dir(dir.path().join(".bears"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{id}-"))
        })
        .unwrap();
    (id, path)
}

fn doctor_json(dir: &TempDir, args: &[&str]) -> (Option<i32>, serde_json::Value) {
    let out = bea(dir).arg("--json").args(args).output().unwrap();
    (
        out.status.code(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}

#[test]
fn test_doctor_healthy_store_succeeds() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    doctor_task(&tmp, "One");
    bea(&tmp)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Checked 1 task file(s): no problems found.",
        ));

    let (code, v) = doctor_json(&tmp, &["doctor"]);
    assert_eq!(code, Some(0));
    assert_eq!(v["ok"], true);
    assert_eq!(v["files"], 1);
    assert_eq!(v["findings"], serde_json::json!([]));
}

/// A duplicate ID (as a git merge of two branches leaves it) fails the
/// check; `--fix` renumbers the copy, keeps the references on the original,
/// and the store passes afterwards.
#[test]
fn test_doctor_fix_renumbers_a_duplicate() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let (id, path) = doctor_task(&tmp, "Original");
    let (dependent, _) = doctor_task(&tmp, "Dependent");
    bea(&tmp)
        .args(["dep", "add", &dependent, &id])
        .assert()
        .success();
    let copy = std::fs::read_to_string(&path)
        .unwrap()
        .replace("title: Original", "title: Copy");
    // Both files are named after the ID, so the first by path keeps it:
    // `-original` sorts before `-the-copy`.
    let copy_path = tmp.path().join(format!(".bears/{id}-the-copy.md"));
    std::fs::write(&copy_path, copy).unwrap();

    bea(&tmp)
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains(format!("duplicate ID {id}")))
        .stderr(predicate::str::contains("bea doctor --fix"));
    let (code, v) = doctor_json(&tmp, &["doctor"]);
    assert_eq!(code, Some(1));
    assert_eq!(v["ok"], false);
    assert_eq!(v["findings"][0]["kind"], "duplicate-id");
    assert_eq!(v["findings"][0]["id"], id.as_str());

    let (code, v) = doctor_json(&tmp, &["doctor", "--fix"]);
    assert_eq!(code, Some(0), "{v}");
    assert_eq!(v["ok"], true);
    assert_eq!(v["moved"], serde_json::json!([]));
    let renamed = v["renamed"].as_array().unwrap();
    assert_eq!(renamed.len(), 1);
    assert_eq!(renamed[0]["from"], id.as_str());
    let new_id = renamed[0]["to"].as_str().unwrap();
    assert!(!copy_path.exists());

    let (_, shown) = doctor_json(&tmp, &["show", new_id]);
    assert_eq!(shown["title"], "Copy");
    let (_, shown) = doctor_json(&tmp, &["show", &dependent]);
    assert_eq!(shown["depends_on"], serde_json::json!([id]));
    bea(&tmp).arg("doctor").assert().success();
}

#[test]
fn test_doctor_fix_renames_a_misnamed_file_and_prints_it() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let (id, path) = doctor_task(&tmp, "Misnamed");
    let wrong = tmp.path().join(".bears/zzz-wrong.md");
    std::fs::rename(&path, &wrong).unwrap();

    bea(&tmp)
        .args(["doctor", "--fix"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "moved .bears/zzz-wrong.md → .bears/{id}-misnamed.md"
        )))
        .stdout(predicate::str::contains("no problems found"));
    assert!(path.exists());
}

/// Problems `--fix` does not repair keep the exit status non-zero.
#[test]
fn test_doctor_fix_fails_while_problems_remain() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    let (id, path) = doctor_task(&tmp, "Dangling");
    let content = std::fs::read_to_string(&path)
        .unwrap()
        .replace("priority: P2\n", "priority: P2\ndepends_on:\n  - gone\n");
    std::fs::write(&path, content).unwrap();

    bea(&tmp)
        .args(["doctor", "--fix"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(format!(
            "{id} depends on gone, which does not exist"
        )));
}

#[test]
fn test_doctor_reports_a_legacy_store_with_migrate_hint() {
    let tmp = TempDir::new().unwrap();
    legacy_cli_store(&tmp);
    let before = bears_snapshot(&tmp);
    bea(&tmp)
        .args(["doctor", "--fix"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("legacy parent: key"))
        .stderr(predicate::str::contains("bea migrate"));
    assert_eq!(bears_snapshot(&tmp), before, "doctor must not touch it");

    let (code, v) = doctor_json(&tmp, &["doctor"]);
    assert_eq!(code, Some(1));
    assert_eq!(v["findings"][0]["kind"], "legacy-store");
    assert_eq!(v["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn test_doctor_not_initialized() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .arg("doctor")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

// -- Legacy stores and `bea migrate` ------------------------------------------

/// Write a task file in the pre-0.10 format, with a `parent:` key when
/// `parent` is not empty, into `.bears/` or `.bears/archive/`.
fn write_legacy_task(dir: &TempDir, id: &str, kind: &str, parent: &str, archived: bool) {
    let mut path = dir.path().join(".bears");
    if archived {
        path = path.join("archive");
    }
    let parent_line = if parent.is_empty() {
        String::new()
    } else {
        format!("parent: {parent}\n")
    };
    let content = format!(
        "---\nid: {id}\ntitle: Task {id}\nstatus: open\npriority: P2\ntype: {kind}\n\
         created: \"2026-01-01T00:00:00Z\"\nupdated: \"2026-01-01T00:00:00Z\"\n{parent_line}---\n\nBody of {id}.\n"
    );
    std::fs::write(path.join(format!("{id}-task-{id}.md")), content).unwrap();
}

fn legacy_cli_store(dir: &TempDir) {
    bea(dir).arg("init").assert().success();
    write_legacy_task(dir, "ep", "epic", "", false);
    write_legacy_task(dir, "ch", "task", "ep", false);
    write_legacy_task(dir, "ac", "task", "ep", true);
}

fn bears_snapshot(dir: &TempDir) -> Vec<(std::path::PathBuf, String)> {
    let root = dir.path().join(".bears");
    let mut files = Vec::new();
    for d in [root.clone(), root.join("archive")] {
        for entry in std::fs::read_dir(d).unwrap().flatten() {
            if entry.path().is_file() {
                files.push((entry.path(), std::fs::read_to_string(entry.path()).unwrap()));
            }
        }
    }
    files.sort();
    files
}

/// Every command that loads the store refuses a legacy one, before writing,
/// and points at `bea migrate`.
#[test]
fn test_legacy_store_is_refused_with_migrate_hint() {
    let tmp = TempDir::new().unwrap();
    legacy_cli_store(&tmp);
    let before = bears_snapshot(&tmp);

    for args in [
        vec!["list"],
        vec!["show", "ch"],
        vec!["done", "ch"],
        vec!["update", "ch", "--title", "Changed"],
        vec!["log"],
        vec!["restore", "ac"],
        vec!["archive"],
    ] {
        bea(&tmp)
            .args(&args)
            .assert()
            .failure()
            .stderr(predicate::str::contains("predates hierarchical IDs"))
            .stderr(predicate::str::contains("hint: run `bea migrate`"));
    }
    assert_eq!(bears_snapshot(&tmp), before, "a refused command wrote");
}

#[test]
fn test_migrate_dry_run_then_migrate() {
    let tmp = TempDir::new().unwrap();
    legacy_cli_store(&tmp);
    let before = bears_snapshot(&tmp);

    let out = bea(&tmp).args(["migrate", "--dry-run"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("renamed ch → ep."), "{stdout}");
    assert!(stdout.contains("renamed ac → ep."), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert_eq!(bears_snapshot(&tmp), before);

    let out = bea(&tmp)
        .args(["--json", "migrate", "--dry-run"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["legacy_files"], 2);
    assert_eq!(bears_snapshot(&tmp), before);

    let out = bea(&tmp).args(["--json", "migrate"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], false);
    assert_eq!(v["orphaned"], serde_json::json!([]));
    let child = v["renamed"]["ch"].as_str().unwrap().to_string();
    let archived = v["renamed"]["ac"].as_str().unwrap().to_string();
    assert!(child.starts_with("ep."), "{child}");

    // The store loads again, with the links kept in the IDs.
    let out = bea(&tmp).args(["--json", "show", &child]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let shown: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(shown["parent"], "ep");
    bea(&tmp)
        .args(["show", &archived])
        .assert()
        .success()
        .stdout(predicate::str::contains("Body of ac."));
    for (path, content) in bears_snapshot(&tmp) {
        assert!(!content.contains("parent:"), "{}", path.display());
    }

    // Running it again does nothing and succeeds.
    let after = bears_snapshot(&tmp);
    bea(&tmp)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to migrate"));
    let out = bea(&tmp).args(["--json", "migrate"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["renamed"], serde_json::json!({}));
    assert_eq!(bears_snapshot(&tmp), after);
}

/// A `parent:` that cannot become part of an ID is dropped and reported; the
/// task keeps its ID.
#[test]
fn test_migrate_reports_orphaned_parents() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp).arg("init").assert().success();
    write_legacy_task(&tmp, "nt", "task", "", false);
    write_legacy_task(&tmp, "mp", "task", "gone", false);
    write_legacy_task(&tmp, "np", "task", "nt", false);

    let out = bea(&tmp).arg("migrate").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("kept mp as is: its parent gone does not exist"),
        "{stdout}"
    );
    assert!(stdout.contains("its parent nt is not an epic"), "{stdout}");
    bea(&tmp).args(["show", "mp"]).assert().success();
    bea(&tmp).args(["show", "np"]).assert().success();
}

#[test]
fn test_migrate_not_initialized() {
    let tmp = TempDir::new().unwrap();
    bea(&tmp)
        .arg("migrate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}
