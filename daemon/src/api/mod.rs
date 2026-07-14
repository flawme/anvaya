//! HTTP handlers.
//!
//! Routing strategy:
//!   - Each REST-style endpoint (`/api/v1/write`, `/api/v1/mkdir`, …) accepts
//!     the *bare* payload documented in the README (`{"path": …, "content": …}`)
//!     and wraps it in a [`Request`](anvaya_shared::Request) with a fresh id.
//!   - The unified `/api/v1` endpoint accepts the full envelope (used by the
//!     extension once it needs to attach a `reason` or adjust `id`).
//!
//! All paths funnel through [`dispatch`], which runs resolve → approve →
//! execute, so no endpoint can bypass the approval gate.

use axum::Json;
use axum::extract::{Path, State, FromRequest, Request as AxumRequest};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::{get, post};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::debug;

use anvaya_shared::{Action, ActionKind, Request, WriteLocation};

use crate::models::AppState;
use crate::permissions::{Approval, PendingSummary};
use anvaya_shared::ResponseError;

/// Custom JSON extractor that returns nicely formatted `ResponseError`s instead of raw text.
pub struct CustomJson<T>(pub T);

#[axum::async_trait]
impl<T, S> FromRequest<S> for CustomJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: AxumRequest, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(value) => Ok(CustomJson(value.0)),
            Err(rejection) => {
                let err = anvaya_shared::Response::err(
                    uuid::Uuid::new_v4(),
                    anvaya_shared::ErrorCode::BadRequest,
                    rejection.to_string(),
                );
                Err((axum::http::StatusCode::BAD_REQUEST, axum::Json(err)).into_response())
            }
        }
    }
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/health", post(health))
        .route("/api/v1", post(handle_uniform))
        .route("/api/v1/write", post(write))
        .route("/api/v1/mkdir", post(mkdir))
        .route("/api/v1/read", post(read))
        .route("/api/v1/list", post(list))
        .route("/api/v1/delete", post(delete))
        .route("/api/v1/move", post(mv))
        .route("/api/v1/copy", post(cp))
        .route("/api/v1/edit", post(edit))
        .route("/api/v1/grep", post(grep_handler))
        .route("/api/v1/tree", post(tree_handler))
        .route("/api/v1/stat", post(stat_handler))
        .route("/api/v1/project_info", post(project_info_handler))
        .route("/api/v1/batch", post(batch_handler))
        .route("/api/v1/glob_list", post(glob_list_handler))
        .route("/api/v1/pending", get(pending))
        .route("/api/v1/approve/{id}", post(approve))
        .route("/api/v1/deny/{id}", post(deny))
        .with_state(state)
}

async fn health() -> AxumResponse {
    axum::http::StatusCode::OK.into_response()
}

/// Bare bodies for REST-style endpoints. The `kind`/`id`/`reason` fields all
/// live in [`Request`] instead, which keeps the documented API small.
#[derive(Debug, Deserialize)]
pub struct OnePath {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct ReadBody {
    pub path: String,
    pub offset: Option<usize>,
    pub length: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TwoPath {
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Deserialize)]
pub struct WriteBody {
    pub path: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub location: WriteLocation,
}

#[derive(Debug, Deserialize)]
pub struct EditBody {
    pub path: String,
    pub search: Option<String>,
    pub replace: Option<String>,
    pub regex: Option<bool>,
    pub replace_all: Option<bool>,
    pub insert_before: Option<String>,
    pub insert_after: Option<String>,
    pub append: Option<bool>,
    pub prepend: Option<bool>,
    pub dry_run_diff: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct GrepBody {
    pub path: String,
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchBody {
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
pub struct GlobListBody {
    pub pattern: String,
}

fn mint(kind: ActionKind, action: Action) -> Request {
    Request {
        id: uuid::Uuid::new_v4(),
        kind,
        action,
        reason: None,
    }
}

async fn write(State(state): State<AppState>, CustomJson(b): CustomJson<WriteBody>) -> AxumResponse {
    let req = mint(
        ActionKind::Write,
        Action::Write {
            path: b.path,
            content: b.content,
            location: b.location,
        },
    );
    dispatch(&state, req).await
}

async fn mkdir(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::Mkdir, Action::Mkdir { path: b.path });
    dispatch(&state, req).await
}

async fn read(State(state): State<AppState>, CustomJson(b): CustomJson<ReadBody>) -> AxumResponse {
    let req = mint(ActionKind::Read, Action::Read { path: b.path, offset: b.offset, length: b.length });
    dispatch(&state, req).await
}

async fn list(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::List, Action::List { path: b.path });
    dispatch(&state, req).await
}

async fn delete(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::Delete, Action::Delete { path: b.path });
    dispatch(&state, req).await
}

async fn mv(State(state): State<AppState>, CustomJson(b): CustomJson<TwoPath>) -> AxumResponse {
    let req = mint(
        ActionKind::Move,
        Action::Move {
            src: b.src,
            dst: b.dst,
        },
    );
    dispatch(&state, req).await
}

async fn cp(State(state): State<AppState>, CustomJson(b): CustomJson<TwoPath>) -> AxumResponse {
    let req = mint(
        ActionKind::Copy,
        Action::Copy {
            src: b.src,
            dst: b.dst,
        },
    );
    dispatch(&state, req).await
}

async fn edit(State(state): State<AppState>, CustomJson(b): CustomJson<EditBody>) -> AxumResponse {
    let req = mint(
        ActionKind::Edit,
        Action::Edit {
            path: b.path,
            search: b.search,
            replace: b.replace,
            regex: b.regex,
            replace_all: b.replace_all,
            insert_before: b.insert_before,
            insert_after: b.insert_after,
            append: b.append,
            prepend: b.prepend,
            dry_run_diff: b.dry_run_diff,
        },
    );
    dispatch(&state, req).await
}

async fn grep_handler(State(state): State<AppState>, CustomJson(b): CustomJson<GrepBody>) -> AxumResponse {
    let req = mint(
        ActionKind::Grep,
        Action::Grep {
            path: b.path,
            query: b.query,
        },
    );
    dispatch(&state, req).await
}

async fn tree_handler(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::Tree, Action::Tree { path: b.path });
    dispatch(&state, req).await
}

async fn stat_handler(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::Stat, Action::Stat { path: b.path });
    dispatch(&state, req).await
}

async fn project_info_handler(State(state): State<AppState>, CustomJson(b): CustomJson<OnePath>) -> AxumResponse {
    let req = mint(ActionKind::ProjectInfo, Action::ProjectInfo { path: b.path });
    dispatch(&state, req).await
}

async fn batch_handler(State(state): State<AppState>, CustomJson(b): CustomJson<BatchBody>) -> AxumResponse {
    let req = mint(ActionKind::Batch, Action::Batch { actions: b.actions });
    dispatch(&state, req).await
}

async fn glob_list_handler(State(state): State<AppState>, CustomJson(b): CustomJson<GlobListBody>) -> AxumResponse {
    let req = mint(ActionKind::GlobList, Action::GlobList { pattern: b.pattern });
    dispatch(&state, req).await
}

/// Full-envelope endpoint. Accepts a complete [`Request`] (including a
/// client-chosen `id` and an optional `reason`).
async fn handle_uniform(State(state): State<AppState>, CustomJson(req): CustomJson<Request>) -> AxumResponse {
    dispatch(&state, req).await
}

/// List requests awaiting approval. The extension polls this.
async fn pending(State(state): State<AppState>) -> AxumResponse {
    let items: Vec<PendingSummary> = state.pending().list().await;
    Json(serde_json::json!({ "pending": items })).into_response()
}

/// Approve a pending request by id (path parameter for simplicity).
async fn approve(State(state): State<AppState>, Path(id): Path<uuid::Uuid>) -> AxumResponse {
    let ok = state.pending().decide(id, Approval::Allow).await;
    Json(serde_json::json!({ "approved": ok })).into_response()
}

/// Deny a pending request by id.
async fn deny(State(state): State<AppState>, Path(id): Path<uuid::Uuid>) -> AxumResponse {
    let ok = state.pending().decide(id, Approval::Deny).await;
    Json(serde_json::json!({ "denied": ok })).into_response()
}

/// Central dispatch: log, run the action pipeline, shape the wire response.
async fn dispatch(state: &AppState, req: Request) -> AxumResponse {
    let id = req.id;
    debug!(id = %id, kind = req.kind.as_str(), "dispatch");
    match crate::actions::execute(&req, state).await {
        Ok(result) => {
            let resp = anvaya_shared::Response::ok(id, result);
            (axum::http::StatusCode::OK, Json(resp)).into_response()
        }
        Err(err) => {
            let e: anvaya_shared::ResponseError = err.into();
            let resp = anvaya_shared::Response::err(id, e.code, e.message);
            (axum::http::StatusCode::OK, Json(resp)).into_response()
        }
    }
}
