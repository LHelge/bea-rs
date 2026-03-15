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
        .args(["init", "--name", "test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized .tasks/"));
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
