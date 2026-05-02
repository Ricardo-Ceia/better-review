use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use tokio::sync::{Mutex, broadcast};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::domain::diff::{FileDiff, Hunk, ReviewStatus};
use crate::services::git::{GitService, PushFailure};
use crate::services::opencode::{
    OpencodeService, OpencodeSession, why_target_for_file, why_target_for_hunk,
};
use crate::settings::{AppSettings, SettingsStore};

pub async fn run() -> Result<()> {
    let repo_path = std::env::current_dir().context("failed to resolve current directory")?;
    let git = GitService::new(&repo_path);
    let (_, files) = git.collect_diff().await?;
    let had_staged_changes_on_open = git.has_staged_changes().await?;
    let settings_store = SettingsStore::new()?;
    let settings = settings_store
        .load()
        .unwrap_or_else(|_| AppSettings::default());
    let opencode = OpencodeService::new(&repo_path).ok();
    let sessions = load_web_sessions(opencode.as_ref());
    let selected_session_id = sessions.first().map(|session| session.id.clone());
    let (events, _) = broadcast::channel(64);
    let token = local_session_token();
    let state = Arc::new(WebState {
        git,
        repo_path,
        token,
        opencode,
        settings_store,
        settings: Mutex::new(settings),
        events,
        explain: Mutex::new(WebExplainState {
            sessions,
            selected_session_id,
            models: Vec::new(),
            selected_model: None,
            history: Vec::new(),
            next_history_id: 1,
        }),
        review: Mutex::new(WebReviewState {
            files,
            had_staged_changes_on_open,
        }),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/api/events", get(api_events))
        .route("/api/refresh", post(api_refresh))
        .route("/api/settings", get(api_settings))
        .route("/api/settings/github-token", post(api_save_github_token))
        .route("/api/explain/sessions", get(api_explain_sessions))
        .route("/api/explain/session", post(api_select_explain_session))
        .route("/api/explain/models", get(api_explain_models))
        .route("/api/explain/model", post(api_select_explain_model))
        .route("/api/explain/history", get(api_explain_history))
        .route("/api/explain", post(api_request_explain))
        .route("/api/commit", post(api_commit))
        .route("/api/push", post(api_push))
        .route("/api/files/:file_index/accept", post(api_accept_file))
        .route("/api/files/:file_index/reject", post(api_reject_file))
        .route("/api/files/:file_index/unreview", post(api_unreview_file))
        .route(
            "/api/files/:file_index/hunks/:hunk_index/accept",
            post(api_accept_hunk),
        )
        .route(
            "/api/files/:file_index/hunks/:hunk_index/reject",
            post(api_reject_hunk),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("failed to bind local web server")?;
    let address = listener
        .local_addr()
        .context("failed to read server port")?;
    let url = format!("http://{address}/?token={}", state.token);

    println!("better-review web is running at {url}");
    println!("Press Ctrl+C to stop the local server.");
    if let Err(error) = open::that(&url) {
        eprintln!("Could not open browser automatically: {error}");
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("local web server failed")
}

struct WebState {
    git: GitService,
    repo_path: PathBuf,
    token: String,
    opencode: Option<OpencodeService>,
    settings_store: SettingsStore,
    settings: Mutex<AppSettings>,
    events: broadcast::Sender<WebEvent>,
    explain: Mutex<WebExplainState>,
    review: Mutex<WebReviewState>,
}

struct WebReviewState {
    files: Vec<FileDiff>,
    had_staged_changes_on_open: bool,
}

struct WebExplainState {
    sessions: Vec<WebSessionResponse>,
    selected_session_id: Option<String>,
    models: Vec<String>,
    selected_model: Option<String>,
    history: Vec<WebExplainHistoryItem>,
    next_history_id: u64,
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReviewStateResponse {
    repo_path: String,
    counts: ReviewCountsResponse,
    files: Vec<FileResponse>,
}

#[derive(Debug, Deserialize)]
struct CommitRequest {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GitHubTokenRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ExplainSessionRequest {
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExplainModelRequest {
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExplainRequest {
    file_index: usize,
    hunk_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WebExplainHistoryItem {
    id: u64,
    label: String,
    model: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WebSessionResponse {
    id: String,
    title: String,
    directory: String,
    time_updated: i64,
}

#[derive(Debug, Serialize)]
struct ExplainSessionsResponse {
    available: bool,
    selected_session_id: Option<String>,
    sessions: Vec<WebSessionResponse>,
}

#[derive(Debug, Serialize)]
struct ExplainModelsResponse {
    available: bool,
    selected_model: Option<String>,
    models: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ExplainHistoryResponse {
    runs: Vec<WebExplainHistoryItem>,
}

#[derive(Debug, Serialize)]
struct ExplainStartResponse {
    id: u64,
    label: String,
    status: String,
}

#[derive(Debug, Clone, Serialize)]
struct WebEvent {
    kind: String,
    message: String,
    run_id: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SettingsResponse {
    has_github_token: bool,
}

#[derive(Debug, Serialize)]
struct ActionResponse {
    message: String,
    state: ReviewStateResponse,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct ReviewCountsResponse {
    unreviewed: usize,
    accepted: usize,
    rejected: usize,
}

#[derive(Debug, Serialize)]
struct FileResponse {
    old_path: String,
    new_path: String,
    display_path: String,
    display_label: String,
    status: crate::domain::diff::FileStatus,
    is_binary: bool,
    review_status: ReviewStatus,
    hunks: Vec<Hunk>,
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_state(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ReviewStateResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let review = state.review.lock().await;
    Ok(Json(review_state_response(
        &state.repo_path,
        review.files.clone(),
    )))
}

async fn api_events(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    ensure_authorized(&state, auth)?;
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(|event| {
        event.ok().map(|event| {
            let payload = serde_json::to_string(&event)
                .unwrap_or_else(|_| "{\"message\":\"event serialization failed\"}".to_string());
            Ok(Event::default().event(event.kind).data(payload))
        })
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn api_refresh(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let (_, files) = state.git.collect_diff().await?;
    let mut review = state.review.lock().await;
    review.files = files;
    Ok(Json(action_response(
        &state,
        &review,
        "Refreshed review queue.",
    )))
}

async fn api_settings(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<SettingsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let settings = state.settings.lock().await;
    Ok(Json(settings_response(&settings)))
}

async fn api_save_github_token(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    Json(payload): Json<GitHubTokenRequest>,
) -> Result<Json<SettingsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let mut settings = state.settings.lock().await;
    let token = payload.token.trim().to_string();
    settings.github.token = if token.is_empty() { None } else { Some(token) };
    state.settings_store.save(&settings)?;
    Ok(Json(settings_response(&settings)))
}

async fn api_explain_sessions(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ExplainSessionsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    refresh_explain_sessions(&state).await;
    let explain = state.explain.lock().await;
    Ok(Json(explain_sessions_response(&state, &explain)))
}

async fn api_select_explain_session(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    Json(payload): Json<ExplainSessionRequest>,
) -> Result<Json<ExplainSessionsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    refresh_explain_sessions(&state).await;
    let mut explain = state.explain.lock().await;
    match payload.session_id {
        Some(session_id) => {
            if !explain
                .sessions
                .iter()
                .any(|session| session.id == session_id)
            {
                return Err(ApiError::not_found("Explain session was not found"));
            }
            explain.selected_session_id = Some(session_id);
        }
        None => explain.selected_session_id = None,
    }
    Ok(Json(explain_sessions_response(&state, &explain)))
}

async fn api_explain_models(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ExplainModelsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    refresh_explain_models(&state).await;
    let explain = state.explain.lock().await;
    Ok(Json(explain_models_response(&state, &explain)))
}

async fn api_select_explain_model(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    Json(payload): Json<ExplainModelRequest>,
) -> Result<Json<ExplainModelsResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    refresh_explain_models(&state).await;
    let mut explain = state.explain.lock().await;
    match payload.model {
        Some(model) => {
            if !explain.models.iter().any(|candidate| candidate == &model) {
                return Err(ApiError::not_found("Explain model was not found"));
            }
            explain.selected_model = Some(model);
        }
        None => explain.selected_model = None,
    }
    Ok(Json(explain_models_response(&state, &explain)))
}

async fn api_explain_history(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ExplainHistoryResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let explain = state.explain.lock().await;
    Ok(Json(ExplainHistoryResponse {
        runs: explain.history.clone(),
    }))
}

async fn api_request_explain(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    Json(payload): Json<ExplainRequest>,
) -> Result<Json<ExplainStartResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let Some(opencode) = state.opencode.clone() else {
        return Err(ApiError::bad_request(
            "Explain is unavailable because opencode is not ready",
        ));
    };

    let target = {
        let review = state.review.lock().await;
        let file = review
            .files
            .get(payload.file_index)
            .ok_or_else(|| ApiError::not_found("file index is out of range"))?;
        match payload.hunk_index {
            Some(hunk_index) => {
                let hunk = file
                    .hunks
                    .get(hunk_index)
                    .ok_or_else(|| ApiError::not_found("hunk index is out of range"))?;
                why_target_for_hunk(file, hunk)
            }
            None => why_target_for_file(file),
        }
    };

    let mut explain = state.explain.lock().await;
    let session_id = explain
        .selected_session_id
        .clone()
        .ok_or_else(|| ApiError::bad_request("choose an Explain context source first"))?;
    let model = explain.selected_model.clone();
    let model_label = model.clone().unwrap_or_else(|| "Auto".to_string());
    let id = explain.next_history_id;
    explain.next_history_id += 1;
    let label = target.label();
    explain.history.push(WebExplainHistoryItem {
        id,
        label: label.clone(),
        model: model_label.clone(),
        status: "Running".to_string(),
    });
    drop(explain);

    emit_event(
        &state,
        "explain_started",
        format!("Explain started for {label}."),
        Some(id),
    );
    let state_for_task = state.clone();
    let label_for_task = label.clone();
    tokio::spawn(async move {
        let result = opencode
            .ask_why(&session_id, &target, model.as_deref())
            .await;
        let (status, message) = match result {
            Ok(_) => ("Ready", format!("Loaded explanation for {label_for_task}.")),
            Err(error) => ("Failed", format!("Explain failed: {error}")),
        };
        {
            let mut explain = state_for_task.explain.lock().await;
            if let Some(run) = explain.history.iter_mut().find(|run| run.id == id) {
                run.status = status.to_string();
            }
        }
        emit_event(&state_for_task, "explain_finished", message, Some(id));
    });

    Ok(Json(ExplainStartResponse {
        id,
        label,
        status: "Running".to_string(),
    }))
}

async fn api_accept_file(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    AxumPath(file_index): AxumPath<usize>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let mut review = state.review.lock().await;
    let file = review_file_mut(&mut review, file_index)?;
    state.git.accept_file(file).await?;
    Ok(Json(action_response(
        &state,
        &review,
        "Accepted file changes.",
    )))
}

async fn api_reject_file(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    AxumPath(file_index): AxumPath<usize>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let mut review = state.review.lock().await;
    let file = review_file_mut(&mut review, file_index)?;
    state.git.reject_file_in_place(file).await?;
    Ok(Json(action_response(
        &state,
        &review,
        "Rejected file changes.",
    )))
}

async fn api_unreview_file(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    AxumPath(file_index): AxumPath<usize>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let mut review = state.review.lock().await;
    let file = review_file_mut(&mut review, file_index)?;
    state.git.unstage_file_in_place(file).await?;
    Ok(Json(action_response(
        &state,
        &review,
        "Moved file back to unreviewed.",
    )))
}

async fn api_accept_hunk(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    AxumPath((file_index, hunk_index)): AxumPath<(usize, usize)>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    update_hunk_review_status(&state, file_index, hunk_index, ReviewStatus::Accepted).await?;
    let review = state.review.lock().await;
    Ok(Json(action_response(&state, &review, "Accepted hunk.")))
}

async fn api_reject_hunk(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    AxumPath((file_index, hunk_index)): AxumPath<(usize, usize)>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    update_hunk_review_status(&state, file_index, hunk_index, ReviewStatus::Rejected).await?;
    let review = state.review.lock().await;
    Ok(Json(action_response(&state, &review, "Rejected hunk.")))
}

async fn api_commit(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
    Json(payload): Json<CommitRequest>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    let message = payload.message.trim();
    if message.is_empty() {
        return Err(ApiError::bad_request("write a commit message first"));
    }
    if !state.git.has_staged_changes().await? {
        return Err(ApiError::bad_request("no accepted changes are staged yet"));
    }

    let mut review = state.review.lock().await;
    if review.had_staged_changes_on_open {
        return Err(ApiError::conflict(
            "cannot commit because better-review opened with unrelated staged changes",
        ));
    }

    state.git.commit_staged(message).await?;
    let (_, files) = state.git.collect_diff().await?;
    review.files = files;
    Ok(Json(action_response(
        &state,
        &review,
        "Committed accepted changes.",
    )))
}

async fn api_push(
    State(state): State<Arc<WebState>>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_authorized(&state, auth)?;
    emit_event(
        &state,
        "publish_started",
        "Publishing reviewed commit...",
        None,
    );
    let token = state.settings.lock().await.github.token.clone();
    match state.git.push_current_branch(token.as_deref()).await {
        Ok(()) => {
            emit_event(
                &state,
                "publish_finished",
                "Pushed reviewed commit to GitHub.",
                None,
            );
            let review = state.review.lock().await;
            Ok(Json(action_response(
                &state,
                &review,
                "Pushed reviewed commit to GitHub.",
            )))
        }
        Err(error) => {
            let api_error = ApiError::from(error);
            emit_event(&state, "publish_failed", api_error.message.clone(), None);
            Err(api_error)
        }
    }
}

fn emit_event(state: &WebState, kind: &str, message: impl Into<String>, run_id: Option<u64>) {
    let _ = state.events.send(WebEvent {
        kind: kind.to_string(),
        message: message.into(),
        run_id,
    });
}

fn ensure_authorized(state: &WebState, auth: AuthQuery) -> Result<(), ApiError> {
    if auth.token.as_deref() == Some(state.token.as_str()) {
        Ok(())
    } else {
        Err(ApiError::unauthorized(
            "missing or invalid local session token",
        ))
    }
}

async fn refresh_explain_sessions(state: &WebState) {
    let Some(opencode) = &state.opencode else {
        return;
    };
    let Ok(sessions) = opencode.list_repo_sessions() else {
        return;
    };
    let mut explain = state.explain.lock().await;
    explain.sessions = sessions.into_iter().map(WebSessionResponse::from).collect();
    if let Some(selected) = &explain.selected_session_id
        && !explain
            .sessions
            .iter()
            .any(|session| &session.id == selected)
    {
        explain.selected_session_id = explain.sessions.first().map(|session| session.id.clone());
    }
}

async fn refresh_explain_models(state: &WebState) {
    let Some(opencode) = &state.opencode else {
        return;
    };
    let Ok(models) = opencode.list_models().await else {
        return;
    };
    let mut explain = state.explain.lock().await;
    explain.models = models;
    if let Some(selected) = &explain.selected_model
        && !explain.models.iter().any(|model| model == selected)
    {
        explain.selected_model = None;
    }
}

fn load_web_sessions(opencode: Option<&OpencodeService>) -> Vec<WebSessionResponse> {
    opencode
        .and_then(|service| service.list_repo_sessions().ok())
        .unwrap_or_default()
        .into_iter()
        .map(WebSessionResponse::from)
        .collect()
}

fn explain_sessions_response(
    state: &WebState,
    explain: &WebExplainState,
) -> ExplainSessionsResponse {
    ExplainSessionsResponse {
        available: state.opencode.is_some(),
        selected_session_id: explain.selected_session_id.clone(),
        sessions: explain.sessions.clone(),
    }
}

fn explain_models_response(state: &WebState, explain: &WebExplainState) -> ExplainModelsResponse {
    ExplainModelsResponse {
        available: state.opencode.is_some(),
        selected_model: explain.selected_model.clone(),
        models: explain.models.clone(),
    }
}

fn settings_response(settings: &AppSettings) -> SettingsResponse {
    SettingsResponse {
        has_github_token: settings.github.token.is_some(),
    }
}

fn action_response(state: &WebState, review: &WebReviewState, message: &str) -> ActionResponse {
    ActionResponse {
        message: message.to_string(),
        state: review_state_response(&state.repo_path, review.files.clone()),
    }
}

async fn update_hunk_review_status(
    state: &WebState,
    file_index: usize,
    hunk_index: usize,
    status: ReviewStatus,
) -> Result<(), ApiError> {
    let mut review = state.review.lock().await;
    let Some(file) = review.files.get(file_index) else {
        return Err(ApiError::not_found("file index is out of range"));
    };
    if file.hunks.get(hunk_index).is_none() {
        return Err(ApiError::not_found("hunk index is out of range"));
    }

    let mut updated_file = file.clone();
    updated_file.hunks[hunk_index].review_status = status;
    updated_file.sync_review_status();
    state.git.sync_file_hunks_to_index(&updated_file).await?;
    review.files[file_index] = updated_file;
    Ok(())
}

fn review_file_mut(
    review: &mut WebReviewState,
    file_index: usize,
) -> Result<&mut FileDiff, ApiError> {
    review
        .files
        .get_mut(file_index)
        .ok_or_else(|| ApiError::not_found("file index is out of range"))
}

fn review_state_response(repo_path: &std::path::Path, files: Vec<FileDiff>) -> ReviewStateResponse {
    let counts = review_counts(&files);
    let files = files.into_iter().map(FileResponse::from).collect();
    ReviewStateResponse {
        repo_path: repo_path.display().to_string(),
        counts,
        files,
    }
}

fn review_counts(files: &[FileDiff]) -> ReviewCountsResponse {
    let mut counts = ReviewCountsResponse::default();
    for file in files {
        if file.hunks.is_empty() {
            bump_count(&mut counts, &file.review_status);
        } else {
            for hunk in &file.hunks {
                bump_count(&mut counts, &hunk.review_status);
            }
        }
    }
    counts
}

fn bump_count(counts: &mut ReviewCountsResponse, status: &ReviewStatus) {
    match status {
        ReviewStatus::Unreviewed => counts.unreviewed += 1,
        ReviewStatus::Accepted => counts.accepted += 1,
        ReviewStatus::Rejected => counts.rejected += 1,
    }
}

impl From<OpencodeSession> for WebSessionResponse {
    fn from(session: OpencodeSession) -> Self {
        Self {
            id: session.id,
            title: session.title,
            directory: session.directory.display().to_string(),
            time_updated: session.time_updated,
        }
    }
}

impl From<FileDiff> for FileResponse {
    fn from(file: FileDiff) -> Self {
        Self {
            display_path: file.display_path().to_string(),
            display_label: file.display_label(),
            old_path: file.old_path,
            new_path: file.new_path,
            status: file.status,
            is_binary: file.is_binary,
            review_status: file.review_status,
            hunks: file.hunks,
        }
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<PushFailure> for ApiError {
    fn from(error: PushFailure) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

fn local_session_token() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

const INDEX_HTML: &str = include_str!("../assets/web/index.html");
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::{DiffLine, DiffLineKind, FileStatus, Hunk};

    #[test]
    fn web_index_includes_explain_menu_shell() {
        assert!(INDEX_HTML.contains("id=\"explainDialog\""));
        assert!(INDEX_HTML.contains("id=\"explainScope\""));
        assert!(INDEX_HTML.contains("id=\"modelDialog\""));
        assert!(INDEX_HTML.contains("id=\"historyDialog\""));
        assert!(INDEX_HTML.contains("Open Explain menu"));
    }

    #[test]
    fn review_state_counts_hunks_and_no_hunk_files() {
        let files = vec![
            FileDiff {
                new_path: "mode.sh".to_string(),
                status: FileStatus::ModeChanged,
                review_status: ReviewStatus::Accepted,
                ..FileDiff::default()
            },
            FileDiff {
                new_path: "src/lib.rs".to_string(),
                hunks: vec![
                    Hunk {
                        review_status: ReviewStatus::Unreviewed,
                        lines: vec![DiffLine {
                            kind: DiffLineKind::Add,
                            content: "new".to_string(),
                            old_line: None,
                            new_line: Some(1),
                        }],
                        ..Hunk::default()
                    },
                    Hunk {
                        review_status: ReviewStatus::Rejected,
                        ..Hunk::default()
                    },
                ],
                ..FileDiff::default()
            },
        ];

        assert_eq!(
            review_counts(&files),
            ReviewCountsResponse {
                unreviewed: 1,
                accepted: 1,
                rejected: 1,
            }
        );
    }

    #[test]
    fn explain_sessions_response_reflects_availability_and_selection() {
        let state = WebState {
            git: GitService::new("."),
            repo_path: PathBuf::from("."),
            token: "token".to_string(),
            opencode: None,
            settings_store: SettingsStore::from_path(PathBuf::from(
                "/tmp/better-review-web-test.json",
            )),
            settings: Mutex::new(AppSettings::default()),
            events: broadcast::channel(1).0,
            explain: Mutex::new(WebExplainState {
                sessions: Vec::new(),
                selected_session_id: None,
                models: Vec::new(),
                selected_model: None,
                history: Vec::new(),
                next_history_id: 1,
            }),
            review: Mutex::new(WebReviewState {
                files: Vec::new(),
                had_staged_changes_on_open: false,
            }),
        };
        let explain = WebExplainState {
            sessions: vec![WebSessionResponse {
                id: "ses_1".to_string(),
                title: "Session one".to_string(),
                directory: "/repo".to_string(),
                time_updated: 42,
            }],
            selected_session_id: Some("ses_1".to_string()),
            models: vec!["openai/gpt-5".to_string()],
            selected_model: Some("openai/gpt-5".to_string()),
            history: vec![WebExplainHistoryItem {
                id: 1,
                label: "file src/lib.rs".to_string(),
                model: "Auto".to_string(),
                status: "Ready".to_string(),
            }],
            next_history_id: 2,
        };

        let response = explain_sessions_response(&state, &explain);
        assert!(!response.available);
        assert_eq!(response.selected_session_id, Some("ses_1".to_string()));
        assert_eq!(response.sessions[0].title, "Session one");

        let model_response = explain_models_response(&state, &explain);
        assert!(!model_response.available);
        assert_eq!(
            model_response.selected_model,
            Some("openai/gpt-5".to_string())
        );
        assert_eq!(model_response.models, vec!["openai/gpt-5".to_string()]);
    }

    #[test]
    fn settings_response_redacts_github_token() {
        let mut settings = AppSettings::default();
        assert!(!settings_response(&settings).has_github_token);

        settings.github.token = Some("secret-token".to_string());
        assert!(settings_response(&settings).has_github_token);
    }

    #[test]
    fn file_response_includes_display_labels() {
        let response = FileResponse::from(FileDiff {
            old_path: "old.rs".to_string(),
            new_path: "new.rs".to_string(),
            status: FileStatus::Renamed,
            ..FileDiff::default()
        });

        assert_eq!(response.display_path, "new.rs");
        assert_eq!(response.display_label, "old.rs → new.rs");
    }
}
