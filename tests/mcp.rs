//! End-to-end MCP tests: spawn the real `bea mcp` binary as a child process
//! and drive it over stdio with the rmcp client — full JSON-RPC handshake,
//! schema validation, and tool dispatch.

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, RawContent};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use tempfile::TempDir;
use tokio::process::Command;

const BEA_BIN: &str = env!("CARGO_BIN_EXE_bea");

/// Initialize a `.bears/` dir and connect an MCP client to a spawned server.
async fn connect(tmp: &TempDir) -> RunningService<RoleClient, ()> {
    let status = std::process::Command::new(BEA_BIN)
        .arg("init")
        .current_dir(tmp.path())
        .stdout(std::process::Stdio::null())
        .status()
        .expect("failed to run bea init");
    assert!(status.success());

    let mut cmd = Command::new(BEA_BIN);
    cmd.arg("mcp").current_dir(tmp.path());
    let transport = TokioChildProcess::new(cmd).expect("failed to spawn bea mcp");
    ().serve(transport).await.expect("MCP handshake failed")
}

/// Build call_tool params (struct is non_exhaustive, so construct via default).
#[allow(clippy::field_reassign_with_default)]
fn params(name: &'static str, args: serde_json::Value) -> CallToolRequestParams {
    let mut p = CallToolRequestParams::default();
    p.name = name.into();
    p.arguments = args.as_object().cloned();
    p
}

fn extract_json(result: &CallToolResult) -> serde_json::Value {
    let text = match &result.content[0].raw {
        RawContent::Text(t) => &t.text,
        other => panic!("expected text content, got {other:?}"),
    };
    serde_json::from_str(text).unwrap()
}

fn extract_text(result: &CallToolResult) -> &str {
    match &result.content[0].raw {
        RawContent::Text(t) => &t.text,
        other => panic!("expected text content, got {other:?}"),
    }
}

async fn create_task(
    client: &RunningService<RoleClient, ()>,
    args: serde_json::Value,
) -> serde_json::Value {
    let result = client.call_tool(params("create_task", args)).await.unwrap();
    assert_ne!(result.is_error, Some(true), "{:?}", result.content);
    extract_json(&result)
}

#[tokio::test]
async fn test_handshake_and_tool_list() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let info = client.peer_info().expect("no server info");
    assert_eq!(info.server_info.name, "bears");

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "list_ready",
        "list_all_tasks",
        "list_epics",
        "get_task",
        "create_task",
        "update_task",
        "start_task",
        "complete_task",
        "cancel_task",
        "prune_tasks",
        "add_dependency",
        "remove_dependency",
        "delete_task",
        "search_tasks",
        "get_graph",
        "plan_epic",
        "archive_task",
        "restore_task",
        "list_archived",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_tool_schema_exposes_enum_values() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let tools = client.list_all_tools().await.unwrap();
    let update = tools.iter().find(|t| t.name == "update_task").unwrap();
    let schema = serde_json::to_value(&update.input_schema).unwrap();
    let schema_text = schema.to_string();
    // Typed params surface the allowed enum values to agents
    assert!(schema_text.contains("in_progress"), "{schema_text}");
    assert!(schema_text.contains("P0"), "{schema_text}");

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_create_get_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let created = create_task(
        &client,
        serde_json::json!({
            "title": "Implement OAuth",
            "priority": "P1",
            "tags": ["backend", "auth"],
            "body": "Use PKCE flow."
        }),
    )
    .await;
    assert_eq!(created["priority"], "P1");
    let id = created["id"].as_str().unwrap().to_string();

    let result = client
        .call_tool(params("get_task", serde_json::json!({ "id": id })))
        .await
        .unwrap();
    let detail = extract_json(&result);
    assert_eq!(detail["title"], "Implement OAuth");
    assert_eq!(detail["body"], "Use PKCE flow.");
    assert_eq!(detail["tags"], serde_json::json!(["backend", "auth"]));

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_ready_flow_with_dependencies() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let first = create_task(&client, serde_json::json!({ "title": "First" })).await;
    let id1 = first["id"].as_str().unwrap().to_string();
    create_task(
        &client,
        serde_json::json!({ "title": "Second", "depends_on": [id1] }),
    )
    .await;

    let ready = client
        .call_tool(params("list_ready", serde_json::json!({})))
        .await
        .unwrap();
    let arr = extract_json(&ready);
    let titles: Vec<&str> = arr
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["First"]);

    let done = client
        .call_tool(params("complete_task", serde_json::json!({ "id": id1 })))
        .await
        .unwrap();
    assert_eq!(extract_json(&done)["status"], "done");

    let ready = client
        .call_tool(params("list_ready", serde_json::json!({})))
        .await
        .unwrap();
    let arr = extract_json(&ready);
    let titles: Vec<&str> = arr
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Second"]);

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_epic_auto_close_over_mcp() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let epic = create_task(
        &client,
        serde_json::json!({ "title": "Release", "type": "epic" }),
    )
    .await;
    let epic_id = epic["id"].as_str().unwrap().to_string();
    let child = create_task(
        &client,
        serde_json::json!({ "title": "Ship it", "parent": epic_id }),
    )
    .await;
    let child_id = child["id"].as_str().unwrap().to_string();

    client
        .call_tool(params(
            "complete_task",
            serde_json::json!({ "id": child_id }),
        ))
        .await
        .unwrap();

    let result = client
        .call_tool(params("get_task", serde_json::json!({ "id": epic_id })))
        .await
        .unwrap();
    assert_eq!(extract_json(&result)["status"], "done");

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_invalid_priority_rejected_by_schema() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    // Typed params: "high" is not a Priority, so this must fail at the
    // protocol layer (invalid_params), before reaching the tool body.
    let result = client
        .call_tool(params(
            "create_task",
            serde_json::json!({ "title": "Bad", "priority": "high" }),
        ))
        .await;
    assert!(result.is_err(), "expected protocol-level invalid params");

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_unknown_id_is_tool_error_not_protocol_error() {
    let tmp = TempDir::new().unwrap();
    let client = connect(&tmp).await;

    let result = client
        .call_tool(params("get_task", serde_json::json!({ "id": "zzzz" })))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(extract_text(&result).contains("not found"));

    client.cancel().await.unwrap();
}
