use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::graph::Graph;
use crate::store;
use crate::task::{self, Priority, Status, Task};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: String) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

fn tool_result(data: Value) -> Value {
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&data).unwrap_or_default()
        }],
        "isError": false
    })
}

fn tool_error(message: &str) -> Value {
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": message
        }],
        "isError": true
    })
}

const TOOL_DEFINITIONS: &str = r#"[
  {
    "name": "list_ready",
    "description": "List tasks that are ready to work on (open with all dependencies done)",
    "inputSchema": {
      "type": "object",
      "properties": {
        "limit": { "type": "integer", "description": "Max number of results" },
        "tag": { "type": "string", "description": "Filter by tag" }
      }
    }
  },
  {
    "name": "list_tasks",
    "description": "List tasks with optional filters",
    "inputSchema": {
      "type": "object",
      "properties": {
        "status": { "type": "string", "enum": ["open", "in_progress", "done", "blocked", "cancelled"] },
        "priority": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
        "tag": { "type": "string", "description": "Filter by tag" }
      }
    }
  },
  {
    "name": "get_task",
    "description": "Get full details of a single task",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string", "description": "Task ID" }
      },
      "required": ["id"]
    }
  },
  {
    "name": "create_task",
    "description": "Create a new task",
    "inputSchema": {
      "type": "object",
      "properties": {
        "title": { "type": "string" },
        "priority": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
        "tags": { "type": "array", "items": { "type": "string" } },
        "depends_on": { "type": "array", "items": { "type": "string" } },
        "parent": { "type": "string" },
        "body": { "type": "string" }
      },
      "required": ["title"]
    }
  },
  {
    "name": "update_task",
    "description": "Update task fields",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string" },
        "status": { "type": "string", "enum": ["open", "in_progress", "done", "blocked", "cancelled"] },
        "priority": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
        "tags": { "type": "array", "items": { "type": "string" } },
        "assignee": { "type": "string" },
        "body": { "type": "string" }
      },
      "required": ["id"]
    }
  },
  {
    "name": "start_task",
    "description": "Start a task (set status to in_progress)",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string" }
      },
      "required": ["id"]
    }
  },
  {
    "name": "complete_task",
    "description": "Complete a task (set status to done)",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string" }
      },
      "required": ["id"]
    }
  },
  {
    "name": "add_dependency",
    "description": "Add a dependency between tasks",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string", "description": "Task that will depend on another" },
        "depends_on": { "type": "string", "description": "Task to depend on" }
      },
      "required": ["id", "depends_on"]
    }
  },
  {
    "name": "remove_dependency",
    "description": "Remove a dependency between tasks",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": { "type": "string" },
        "depends_on": { "type": "string" }
      },
      "required": ["id", "depends_on"]
    }
  },
  {
    "name": "search_tasks",
    "description": "Search tasks by text query",
    "inputSchema": {
      "type": "object",
      "properties": {
        "query": { "type": "string" }
      },
      "required": ["query"]
    }
  },
  {
    "name": "get_graph",
    "description": "Get the full dependency graph as an adjacency list",
    "inputSchema": {
      "type": "object",
      "properties": {}
    }
  }
]"#;

pub fn run(base: &Path) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break; // EOF
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(Value::Null, -32700, format!("Parse error: {e}"));
                send_response(&mut writer, &resp)?;
                continue;
            }
        };

        if req.jsonrpc != "2.0" {
            let resp = JsonRpcResponse::error(
                req.id.unwrap_or(Value::Null),
                -32600,
                "Invalid JSON-RPC version".into(),
            );
            send_response(&mut writer, &resp)?;
            continue;
        }

        let id = req.id.clone().unwrap_or(Value::Null);

        match req.method.as_str() {
            "initialize" => {
                let resp = JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "bears",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                );
                send_response(&mut writer, &resp)?;
            }
            "notifications/initialized" => {
                // Notification, no response needed
            }
            "tools/list" => {
                let tools: Value = serde_json::from_str(TOOL_DEFINITIONS)
                    .expect("TOOL_DEFINITIONS should be valid JSON");
                let resp = JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }));
                send_response(&mut writer, &resp)?;
            }
            "tools/call" => {
                let tool_name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));

                let result = handle_tool_call(base, tool_name, &args);
                let resp = JsonRpcResponse::success(id, result);
                send_response(&mut writer, &resp)?;
            }
            _ => {
                let resp =
                    JsonRpcResponse::error(id, -32601, format!("Method not found: {}", req.method));
                send_response(&mut writer, &resp)?;
            }
        }
    }

    Ok(())
}

fn send_response(writer: &mut impl Write, resp: &JsonRpcResponse) -> Result<()> {
    let json = serde_json::to_string(resp)?;
    writeln!(writer, "{json}")?;
    writer.flush()?;
    Ok(())
}

fn handle_tool_call(base: &Path, name: &str, args: &Value) -> Value {
    let result = match name {
        "list_ready" => tool_list_ready(base, args),
        "list_tasks" => tool_list_tasks(base, args),
        "get_task" => tool_get_task(base, args),
        "create_task" => tool_create_task(base, args),
        "update_task" => tool_update_task(base, args),
        "start_task" => tool_start_task(base, args),
        "complete_task" => tool_complete_task(base, args),
        "add_dependency" => tool_add_dependency(base, args),
        "remove_dependency" => tool_remove_dependency(base, args),
        "search_tasks" => tool_search_tasks(base, args),
        "get_graph" => tool_get_graph(base, args),
        _ => return tool_error(&format!("Unknown tool: {name}")),
    };

    match result {
        Ok(v) => tool_result(v),
        Err(e) => tool_error(&e.to_string()),
    }
}

fn task_summary(t: &Task) -> Value {
    serde_json::json!({
        "id": t.id,
        "title": t.title,
        "status": t.status,
        "priority": t.priority,
        "tags": t.tags,
    })
}

fn tool_list_ready(base: &Path, args: &Value) -> Result<Value> {
    let tasks = store::load_all(base)?;
    let graph = Graph::build(&tasks);
    let tag = args.get("tag").and_then(|v| v.as_str());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let ready = graph.ready(&tasks, tag, limit);
    let summaries: Vec<Value> = ready.iter().map(|t| task_summary(t)).collect();
    Ok(serde_json::json!(summaries))
}

fn tool_list_tasks(base: &Path, args: &Value) -> Result<Value> {
    let tasks = store::load_all(base)?;
    let status: Option<Status> = args
        .get("status")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());
    let priority: Option<Priority> = args
        .get("priority")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value(Value::String(s.into())).ok());
    let tag = args.get("tag").and_then(|v| v.as_str());

    let mut filtered: Vec<&Task> = tasks
        .values()
        .filter(|t| status.as_ref().is_none_or(|s| t.status == *s))
        .filter(|t| priority.as_ref().is_none_or(|p| t.priority == *p))
        .filter(|t| {
            tag.as_ref()
                .is_none_or(|tag| t.tags.iter().any(|tt| tt == tag))
        })
        .collect();
    filtered.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    let summaries: Vec<Value> = filtered.iter().map(|t| task_summary(t)).collect();
    Ok(serde_json::json!(summaries))
}

fn tool_get_task(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;
    let t = store::load_one(base, id)?;
    Ok(serde_json::json!({
        "id": t.id,
        "title": t.title,
        "status": t.status,
        "priority": t.priority,
        "tags": t.tags,
        "depends_on": t.depends_on,
        "parent": t.parent,
        "assignee": t.assignee,
        "created": t.created,
        "updated": t.updated,
        "body": t.body,
    }))
}

fn tool_create_task(base: &Path, args: &Value) -> Result<Value> {
    let title =
        args.get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidFrontmatter {
                path: "".into(),
                reason: "missing title".into(),
            })?;

    let priority: Priority = args
        .get("priority")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value(Value::String(s.into())).ok())
        .unwrap_or(Priority::P2);

    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let depends_on: Vec<String> = args
        .get("depends_on")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let parent = args
        .get("parent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let tasks = store::load_all(base)?;
    let existing_ids: HashSet<String> = tasks.keys().cloned().collect();
    let id = task::generate_id(&existing_ids);

    let mut t = Task::new(id, title.into(), priority);
    t.tags = tags;
    t.depends_on = depends_on;
    t.parent = parent;
    t.body = body.into();

    store::save(base, &t)?;
    Ok(task_summary(&t))
}

fn tool_update_task(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;

    let mut t = store::load_one(base, id)?;

    if let Some(s) = args.get("status").and_then(|v| v.as_str())
        && let Ok(status) = s.parse::<Status>()
    {
        t.status = status;
    }
    if let Some(p) = args
        .get("priority")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value::<Priority>(Value::String(s.into())).ok())
    {
        t.priority = p;
    }
    if let Some(tags) = args.get("tags").and_then(|v| v.as_array()) {
        t.tags = tags
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(a) = args.get("assignee").and_then(|v| v.as_str()) {
        t.assignee = a.into();
    }
    if let Some(b) = args.get("body").and_then(|v| v.as_str()) {
        t.body = b.into();
    }

    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(task_summary(&t))
}

fn tool_start_task(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;
    let mut t = store::load_one(base, id)?;
    t.status = Status::InProgress;
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(task_summary(&t))
}

fn tool_complete_task(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;
    let mut t = store::load_one(base, id)?;
    t.status = Status::Done;
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(task_summary(&t))
}

fn tool_add_dependency(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;
    let depends_on = args
        .get("depends_on")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing depends_on".into()))?;

    // Verify dependency target exists
    let _ = store::load_one(base, depends_on)?;
    let mut t = store::load_one(base, id)?;

    // Check for cycles
    let tasks = store::load_all(base)?;
    let graph = Graph::build(&tasks);
    if graph.would_cycle(id, depends_on) {
        return Err(Error::CycleDetected {
            from: id.into(),
            to: depends_on.into(),
        });
    }

    if !t.depends_on.contains(&depends_on.to_string()) {
        t.depends_on.push(depends_on.to_string());
        t.updated = Utc::now();
        store::save(base, &t)?;
    }

    Ok(task_summary(&t))
}

fn tool_remove_dependency(base: &Path, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing id".into()))?;
    let depends_on = args
        .get("depends_on")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::TaskNotFound("missing depends_on".into()))?;

    let mut t = store::load_one(base, id)?;
    t.depends_on.retain(|d| d != depends_on);
    t.updated = Utc::now();
    store::save(base, &t)?;
    Ok(task_summary(&t))
}

fn tool_search_tasks(base: &Path, args: &Value) -> Result<Value> {
    let query =
        args.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidFrontmatter {
                path: "".into(),
                reason: "missing query".into(),
            })?;

    let tasks = store::load_all(base)?;
    let query_lower = query.to_lowercase();
    let mut results: Vec<&Task> = tasks
        .values()
        .filter(|t| {
            t.title.to_lowercase().contains(&query_lower)
                || t.body.to_lowercase().contains(&query_lower)
                || t.tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query_lower))
                || t.id.contains(&query_lower)
        })
        .collect();
    results.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    let summaries: Vec<Value> = results.iter().map(|t| task_summary(t)).collect();
    Ok(serde_json::json!(summaries))
}

fn tool_get_graph(base: &Path, _args: &Value) -> Result<Value> {
    let tasks = store::load_all(base)?;
    let graph = Graph::build(&tasks);
    let adj = graph.adjacency_list();
    Ok(serde_json::json!(adj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let tmp = TempDir::new().unwrap();
        store::init(tmp.path(), None).unwrap();
        tmp
    }

    #[test]
    fn test_tool_create_and_list() {
        let tmp = setup();
        let args = serde_json::json!({
            "title": "Test task",
            "priority": "P1",
            "tags": ["backend"]
        });
        let result = tool_create_task(tmp.path(), &args).unwrap();
        assert_eq!(result["title"], "Test task");
        let id = result["id"].as_str().unwrap();

        let list = tool_list_tasks(tmp.path(), &serde_json::json!({})).unwrap();
        let arr = list.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], id);
    }

    #[test]
    fn test_tool_get_task() {
        let tmp = setup();
        let result = tool_create_task(
            tmp.path(),
            &serde_json::json!({"title": "Detail task", "body": "Some body"}),
        )
        .unwrap();
        let id = result["id"].as_str().unwrap();

        let detail = tool_get_task(tmp.path(), &serde_json::json!({"id": id})).unwrap();
        assert_eq!(detail["title"], "Detail task");
        assert_eq!(detail["body"], "Some body");
    }

    #[test]
    fn test_tool_start_complete() {
        let tmp = setup();
        let result =
            tool_create_task(tmp.path(), &serde_json::json!({"title": "Flow task"})).unwrap();
        let id = result["id"].as_str().unwrap();

        let started = tool_start_task(tmp.path(), &serde_json::json!({"id": id})).unwrap();
        assert_eq!(started["status"], "in_progress");

        let completed = tool_complete_task(tmp.path(), &serde_json::json!({"id": id})).unwrap();
        assert_eq!(completed["status"], "done");
    }

    #[test]
    fn test_tool_ready() {
        let tmp = setup();
        let t1 = tool_create_task(tmp.path(), &serde_json::json!({"title": "First"})).unwrap();
        let id1 = t1["id"].as_str().unwrap();

        tool_create_task(
            tmp.path(),
            &serde_json::json!({"title": "Second", "depends_on": [id1]}),
        )
        .unwrap();

        // Only first should be ready
        let ready = tool_list_ready(tmp.path(), &serde_json::json!({})).unwrap();
        let arr = ready.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "First");

        // Complete first
        tool_complete_task(tmp.path(), &serde_json::json!({"id": id1})).unwrap();

        // Now second should be ready
        let ready = tool_list_ready(tmp.path(), &serde_json::json!({})).unwrap();
        let arr = ready.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "Second");
    }

    #[test]
    fn test_tool_dependency_cycle() {
        let tmp = setup();
        let t1 = tool_create_task(tmp.path(), &serde_json::json!({"title": "A"})).unwrap();
        let id_a = t1["id"].as_str().unwrap();

        let t2 = tool_create_task(
            tmp.path(),
            &serde_json::json!({"title": "B", "depends_on": [id_a]}),
        )
        .unwrap();
        let id_b = t2["id"].as_str().unwrap();

        let result = tool_add_dependency(
            tmp.path(),
            &serde_json::json!({"id": id_a, "depends_on": id_b}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_search() {
        let tmp = setup();
        tool_create_task(
            tmp.path(),
            &serde_json::json!({"title": "Implement OAuth", "tags": ["auth"]}),
        )
        .unwrap();
        tool_create_task(tmp.path(), &serde_json::json!({"title": "Fix database"})).unwrap();

        let results =
            tool_search_tasks(tmp.path(), &serde_json::json!({"query": "OAuth"})).unwrap();
        let arr = results.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "Implement OAuth");
    }

    #[test]
    fn test_tool_graph() {
        let tmp = setup();
        let t1 = tool_create_task(tmp.path(), &serde_json::json!({"title": "A"})).unwrap();
        let id_a = t1["id"].as_str().unwrap();

        tool_create_task(
            tmp.path(),
            &serde_json::json!({"title": "B", "depends_on": [id_a]}),
        )
        .unwrap();

        let graph = tool_get_graph(tmp.path(), &serde_json::json!({})).unwrap();
        assert!(graph.is_object());
    }

    #[test]
    fn test_mcp_initialize_and_tools_list() {
        // Test the JSON-RPC protocol flow
        let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let tools_req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let input = format!("{init_req}\n{tools_req}\n");

        let tmp = setup();
        let stdin = io::Cursor::new(input.as_bytes());
        let mut stdout = Vec::new();

        // Run server loop manually
        let reader = io::BufReader::new(stdin);
        for line in reader.lines() {
            let line = line.unwrap();
            if line.is_empty() {
                continue;
            }
            let req: JsonRpcRequest = serde_json::from_str(&line).unwrap();
            let id = req.id.clone().unwrap_or(Value::Null);
            let resp = match req.method.as_str() {
                "initialize" => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "bears", "version": env!("CARGO_PKG_VERSION") }
                    }),
                ),
                "tools/list" => {
                    let tools: Value = serde_json::from_str(TOOL_DEFINITIONS).unwrap();
                    JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
                }
                "tools/call" => {
                    let tool_name = req
                        .params
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args = req.params.get("arguments").cloned().unwrap_or_default();
                    let result = handle_tool_call(tmp.path(), tool_name, &args);
                    JsonRpcResponse::success(id, result)
                }
                _ => JsonRpcResponse::error(id, -32601, "Method not found".into()),
            };
            send_response(&mut stdout, &resp).unwrap();
        }

        let output = String::from_utf8(stdout).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Verify initialize response
        let init_resp: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(init_resp["result"]["serverInfo"]["name"], "bears");

        // Verify tools/list response
        let tools_resp: Value = serde_json::from_str(lines[1]).unwrap();
        let tools = tools_resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 11);
    }
}
