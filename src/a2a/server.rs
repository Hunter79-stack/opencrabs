//! A2A Gateway HTTP server powered by axum.
//!
//! Serves:
//! - `GET  /.well-known/agent.json` — Agent Card discovery
//! - `POST /a2a/v1`                 — JSON-RPC 2.0 endpoint
//! - `GET  /a2a/health`             — Health check

use crate::a2a::{agent_card, handler, types::*};
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

/// Shared state for the A2A gateway.
#[derive(Clone)]
pub struct A2aState {
    pub task_store: handler::TaskStore,
    pub host: String,
    pub port: u16,
}

/// Build the axum router for the A2A gateway.
pub fn build_router(state: A2aState) -> Router {
    Router::new()
        .route("/.well-known/agent.json", get(get_agent_card))
        .route("/a2a/v1", post(handle_jsonrpc))
        .route("/a2a/health", get(health_check))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// A2A Gateway server configuration.
pub struct GatewayParams {
    pub bind: String,
    pub port: u16,
    pub enabled: bool,
}

/// Start the A2A gateway server.
///
/// This runs as a background task — call from `tokio::spawn`.
pub async fn start_server(params: &GatewayParams) -> anyhow::Result<()> {
    if !params.enabled {
        tracing::info!("A2A gateway disabled in config");
        return Ok(());
    }

    let state = A2aState {
        task_store: handler::new_task_store(),
        host: params.bind.clone(),
        port: params.port,
    };

    let app = build_router(state);
    let addr: SocketAddr = format!("{}:{}", params.bind, params.port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gateway address: {}", e))?;

    tracing::info!("🐝 A2A Gateway starting on http://{}", addr);
    tracing::info!(
        "   Agent Card: http://{}/.well-known/agent.json",
        addr
    );
    tracing::info!("   JSON-RPC:   http://{}/a2a/v1", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// GET /.well-known/agent.json — Agent Card discovery.
async fn get_agent_card(State(state): State<A2aState>) -> Json<AgentCard> {
    let card = agent_card::build_agent_card(&state.host, state.port);
    Json(card)
}

/// POST /a2a/v1 — JSON-RPC 2.0 endpoint.
async fn handle_jsonrpc(
    State(state): State<A2aState>,
    Json(req): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    // Validate JSON-RPC version
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                req.id,
                error_codes::INVALID_REQUEST,
                "Invalid JSON-RPC version, expected 2.0",
            )),
        );
    }

    let response = handler::dispatch(req, state.task_store).await;
    (StatusCode::OK, Json(response))
}

/// GET /a2a/health — Health check.
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": crate::VERSION,
        "protocol": "A2A",
        "protocol_version": "1.0",
        "provider": "OpenCrabs Community"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_state() -> A2aState {
        A2aState {
            task_store: handler::new_task_store(),
            host: "127.0.0.1".to_string(),
            port: 18789,
        }
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/a2a/health")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_agent_card_endpoint() {
        let app = build_router(test_state());
        let req = Request::builder()
            .uri("/.well-known/agent.json")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jsonrpc_send_message() {
        let app = build_router(test_state());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "message/send",
            "params": {
                "message": {
                    "role": "user",
                    "parts": [{"text": "Hello from A2A test!"}]
                }
            },
            "id": 1
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/v1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).expect("json")))
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
