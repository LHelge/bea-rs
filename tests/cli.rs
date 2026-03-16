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
