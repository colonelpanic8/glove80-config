use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    attention::{Kind, Source},
    events::Event,
};

#[derive(Clone)]
struct AppState {
    events: mpsc::Sender<Event>,
}

pub fn router(events: mpsc::Sender<Event>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/claude-hook", post(hook))
        .with_state(AppState { events })
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn hook(State(state): State<AppState>, Json(payload): Json<Value>) -> impl IntoResponse {
    let Some(session_id) = payload.get("session_id").and_then(Value::as_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing session_id" })),
        );
    };
    let Some(hook_name) = payload.get("hook_event_name").and_then(Value::as_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing hook_event_name" })),
        );
    };

    let id = format!("claude:{session_id}");
    let event = match hook_name {
        "PermissionRequest" => Some(Event::Set {
            id,
            source: Source::Claude,
            kind: Kind::Approval,
        }),
        "Notification" => match payload.get("notification_type").and_then(Value::as_str) {
            Some("permission_prompt") => Some(Event::Set {
                id,
                source: Source::Claude,
                kind: Kind::Approval,
            }),
            Some("idle_prompt" | "elicitation_dialog") => Some(Event::Set {
                id,
                source: Source::Claude,
                kind: Kind::Input,
            }),
            Some("elicitation_complete" | "elicitation_response") => Some(Event::Remove { id }),
            _ => None,
        },
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PermissionDenied"
        | "ElicitationResult" | "Stop" | "StopFailure" | "SessionStart" | "SessionEnd" => {
            Some(Event::Remove { id })
        }
        _ => None,
    };

    if let Some(event) = event
        && state.events.send(event).await.is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "daemon is shutting down" })),
        );
    }

    (StatusCode::OK, Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    use super::*;

    async fn post_payload(payload: Value) -> (StatusCode, Value, Option<Event>) {
        let (sender, mut receiver) = mpsc::channel(4);
        let request = Request::builder()
            .method("POST")
            .uri("/claude-hook")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = router(sender).oneshot(request).await.unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        (status, body, receiver.try_recv().ok())
    }

    #[tokio::test]
    async fn permission_request_sets_approval_attention() {
        let (status, _, event) = post_payload(json!({
            "session_id": "abc",
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash"
        }))
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(matches!(
            event,
            Some(Event::Set {
                id,
                source: Source::Claude,
                kind: Kind::Approval,
            }) if id == "claude:abc"
        ));
    }

    #[tokio::test]
    async fn prompt_submission_clears_attention() {
        let (_, _, event) = post_payload(json!({
            "session_id": "abc",
            "hook_event_name": "UserPromptSubmit"
        }))
        .await;

        assert!(matches!(event, Some(Event::Remove { id }) if id == "claude:abc"));
    }

    #[tokio::test]
    async fn malformed_payload_is_rejected() {
        let (status, _, event) = post_payload(json!({ "hook_event_name": "Notification" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(event.is_none());
    }
}
