//! JSON-RPC 2.0 handler for A2A protocol operations.
//!
//! Dispatches JSON-RPC methods:
//! - `message/send` → create task + process message
//! - `tasks/get`    → retrieve task by ID
//! - `tasks/cancel` → cancel a running task

use crate::a2a::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// In-memory task store. Production would use SQLite.
pub type TaskStore = Arc<RwLock<HashMap<String, Task>>>;

/// Create a new empty task store.
pub fn new_task_store() -> TaskStore {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Dispatch a JSON-RPC request to the appropriate handler.
pub async fn dispatch(req: JsonRpcRequest, store: TaskStore) -> JsonRpcResponse {
    match req.method.as_str() {
        "message/send" => handle_send_message(req.id, req.params, store).await,
        "tasks/get" => handle_get_task(req.id, req.params, store).await,
        "tasks/cancel" => handle_cancel_task(req.id, req.params, store).await,
        _ => JsonRpcResponse::error(
            req.id,
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", req.method),
        ),
    }
}

/// Handle `message/send` — create a task and process the message.
async fn handle_send_message(
    id: serde_json::Value,
    params: serde_json::Value,
    store: TaskStore,
) -> JsonRpcResponse {
    // Parse params
    let send_params: SendMessageParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("Invalid params: {}", e),
            );
        }
    };

    // Extract text from message parts
    let user_text = send_params
        .message
        .parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");

    // Create task
    let task_id = Uuid::new_v4().to_string();
    let context_id = send_params
        .message
        .context_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let task = Task {
        id: task_id.clone(),
        context_id: Some(context_id.clone()),
        status: TaskStatus {
            state: TaskState::Working,
            message: Some(Message {
                message_id: Some(Uuid::new_v4().to_string()),
                context_id: Some(context_id.clone()),
                task_id: Some(task_id.clone()),
                role: Role::Agent,
                parts: vec![Part::text(format!(
                    "Task created. Processing: {}",
                    if user_text.len() > 100 {
                        format!("{}...", &user_text[..user_text.floor_char_boundary(100)])
                    } else {
                        user_text.clone()
                    }
                ))],
                metadata: None,
            }),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        },
        artifacts: vec![],
        history: vec![send_params.message],
        metadata: None,
    };

    // Store task
    {
        let mut tasks = store.write().await;
        tasks.insert(task_id.clone(), task.clone());
    }

    tracing::info!("A2A: Created task {} for message: {}", task_id, user_text);

    // Return task immediately (async processing would happen in background)
    let task_json =
        serde_json::to_value(&task).unwrap_or_else(|_| serde_json::json!({"error": "serialize"}));
    JsonRpcResponse::success(id, task_json)
}

/// Handle `tasks/get` — retrieve a task by ID.
async fn handle_get_task(
    id: serde_json::Value,
    params: serde_json::Value,
    store: TaskStore,
) -> JsonRpcResponse {
    let get_params: GetTaskParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("Invalid params: {}", e),
            );
        }
    };

    let tasks = store.read().await;
    match tasks.get(&get_params.id) {
        Some(task) => {
            let task_json = serde_json::to_value(task)
                .unwrap_or_else(|_| serde_json::json!({"error": "serialize"}));
            JsonRpcResponse::success(id, task_json)
        }
        None => JsonRpcResponse::error(
            id,
            error_codes::TASK_NOT_FOUND,
            format!("Task not found: {}", get_params.id),
        ),
    }
}

/// Handle `tasks/cancel` — cancel a running task.
async fn handle_cancel_task(
    id: serde_json::Value,
    params: serde_json::Value,
    store: TaskStore,
) -> JsonRpcResponse {
    let cancel_params: CancelTaskParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("Invalid params: {}", e),
            );
        }
    };

    let mut tasks = store.write().await;
    match tasks.get_mut(&cancel_params.id) {
        Some(task) => {
            // Only cancel if not in terminal state
            match task.status.state {
                TaskState::Completed | TaskState::Failed | TaskState::Canceled => {
                    return JsonRpcResponse::error(
                        id,
                        error_codes::UNSUPPORTED_OPERATION,
                        format!(
                            "Cannot cancel task in {:?} state",
                            task.status.state
                        ),
                    );
                }
                _ => {
                    task.status.state = TaskState::Canceled;
                    task.status.timestamp = Some(chrono::Utc::now().to_rfc3339());
                    tracing::info!("A2A: Canceled task {}", cancel_params.id);
                    let task_json = serde_json::to_value(&*task)
                        .unwrap_or_else(|_| serde_json::json!({"error": "serialize"}));
                    JsonRpcResponse::success(id, task_json)
                }
            }
        }
        None => JsonRpcResponse::error(
            id,
            error_codes::TASK_NOT_FOUND,
            format!("Task not found: {}", cancel_params.id),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_send_request() -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "message/send".to_string(),
            params: serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"text": "Hello, agent!"}]
                }
            }),
            id: serde_json::json!(1),
        }
    }

    #[tokio::test]
    async fn test_send_message() {
        let store = new_task_store();
        let req = make_send_request();
        let resp = dispatch(req, store.clone()).await;

        assert!(resp.result.is_some());
        assert!(resp.error.is_none());

        let result = resp.result.expect("has result");
        assert!(result.get("id").is_some());
        assert_eq!(
            result.get("status").and_then(|s| s.get("state")).and_then(|s| s.as_str()),
            Some("working")
        );

        // Task should be stored
        let tasks = store.read().await;
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let store = new_task_store();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tasks/get".to_string(),
            params: serde_json::json!({"id": "nonexistent"}),
            id: serde_json::json!(2),
        };
        let resp = dispatch(req, store).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().expect("err").code, -32001);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let store = new_task_store();

        // First create a task
        let send_req = make_send_request();
        let send_resp = dispatch(send_req, store.clone()).await;
        let task_id = send_resp
            .result
            .as_ref()
            .and_then(|r| r.get("id"))
            .and_then(|id| id.as_str())
            .expect("task id");

        // Then cancel it
        let cancel_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tasks/cancel".to_string(),
            params: serde_json::json!({"id": task_id}),
            id: serde_json::json!(3),
        };
        let cancel_resp = dispatch(cancel_req, store).await;
        assert!(cancel_resp.result.is_some());

        let result = cancel_resp.result.expect("result");
        assert_eq!(
            result.get("status").and_then(|s| s.get("state")).and_then(|s| s.as_str()),
            Some("canceled")
        );
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let store = new_task_store();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "unknown/method".to_string(),
            params: serde_json::json!({}),
            id: serde_json::json!(99),
        };
        let resp = dispatch(req, store).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().expect("err").code, -32601);
    }
}
