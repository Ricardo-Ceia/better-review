use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use tokio::sync::Mutex;

use crate::domain::diff::{FileDiff, Hunk, ReviewStatus};
use crate::services::git::{GitService, PushFailure};
use crate::services::opencode::{OpencodeService, OpencodeSession};
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
    let token = local_session_token();
    let state = Arc::new(WebState {
        git,
        repo_path,
        token,
        opencode,
        settings_store,
        settings: Mutex::new(settings),
        explain: Mutex::new(WebExplainState {
            sessions,
            selected_session_id,
            models: Vec::new(),
            selected_model: None,
            history: Vec::new(),
        }),
        review: Mutex::new(WebReviewState {
            files,
            had_staged_changes_on_open,
        }),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/api/refresh", post(api_refresh))
        .route("/api/settings", get(api_settings))
        .route("/api/settings/github-token", post(api_save_github_token))
        .route("/api/explain/sessions", get(api_explain_sessions))
        .route("/api/explain/session", post(api_select_explain_session))
        .route("/api/explain/models", get(api_explain_models))
        .route("/api/explain/model", post(api_select_explain_model))
        .route("/api/explain/history", get(api_explain_history))
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
    let token = state.settings.lock().await.github.token.clone();
    state
        .git
        .push_current_branch(token.as_deref())
        .await
        .map_err(ApiError::from)?;
    let review = state.review.lock().await;
    Ok(Json(action_response(
        &state,
        &review,
        "Pushed reviewed commit to GitHub.",
    )))
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

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>better-review web</title>
  <style>
    :root {
      color-scheme: dark;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      --bg: #0b1020;
      --surface: #111827;
      --surface-raised: #172033;
      --panel: #0f172a;
      --border: #253044;
      --muted: #94a3b8;
      --text: #e5e7eb;
      --accent: #60a5fa;
      --accent-dim: #1d4ed8;
      --green: #22c55e;
      --red: #ef4444;
      --yellow: #facc15;
      --code-add: #052e16;
      --code-remove: #3f1018;
      --code-focus: #1e3a8a;
    }
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; background: radial-gradient(circle at top, #172554 0, var(--bg) 38%); color: var(--text); }
    button, textarea { font: inherit; }
    button { cursor: pointer; border: 1px solid #334155; color: var(--text); background: #1e293b; padding: 7px 10px; border-radius: 10px; }
    button:hover { background: #334155; }
    button.primary { border-color: #2563eb; background: #1d4ed8; }
    button.danger { border-color: #7f1d1d; background: #450a0a; }
    button.ghost { background: transparent; }
    code, pre, .mono { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace; }
    .app { min-height: 100vh; display: grid; grid-template-rows: auto 1fr auto; }
    .topbar { height: 48px; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; padding: 0 18px; border-bottom: 1px solid var(--border); background: rgb(11 16 32 / 0.88); backdrop-filter: blur(12px); }
    .brand { grid-column: 2; font-weight: 800; letter-spacing: -0.04em; }
    .brand span { color: var(--accent); }
    .repo { justify-self: start; color: var(--muted); min-width: 0; max-width: 44vw; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .counts { justify-self: end; display: flex; gap: 8px; color: var(--muted); align-items: center; }
    .pill { border: 1px solid #334155; border-radius: 999px; padding: 5px 9px; background: #0f172a; }
    .workspace { min-height: 0; display: grid; grid-template-columns: minmax(280px, 34vw) minmax(0, 1fr); gap: 12px; padding: 12px; }
    .hidden { display: none !important; }
    .home { min-height: 0; display: grid; place-items: center; padding: 40px 18px; }
    .home-card { width: min(760px, 100%); background: rgb(17 24 39 / 0.94); border: 1px solid var(--border); border-radius: 24px; padding: 32px; box-shadow: 0 24px 80px rgb(0 0 0 / 0.24); }
    .home-kicker { color: var(--muted); text-transform: uppercase; letter-spacing: 0.16em; font-size: 12px; }
    .home-title { margin: 10px 0 8px; font-size: clamp(32px, 6vw, 64px); line-height: 0.95; letter-spacing: -0.08em; }
    .home-title span { color: var(--accent); }
    .home-detail { color: var(--muted); font-size: 17px; max-width: 58ch; }
    .home-progress { margin: 24px 0; display: grid; gap: 10px; }
    .progress-track { height: 12px; border-radius: 999px; overflow: hidden; background: #020617; border: 1px solid #334155; }
    .progress-fill { height: 100%; width: 0%; background: linear-gradient(90deg, var(--accent), var(--green)); transition: width 180ms ease; }
    .home-actions { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 20px; }
    .panel { min-height: 0; background: rgb(17 24 39 / 0.94); border: 1px solid var(--border); border-radius: 16px; overflow: hidden; box-shadow: 0 24px 80px rgb(0 0 0 / 0.22); }
    .panel-title { height: 42px; display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 0 14px; border-bottom: 1px solid var(--border); color: var(--muted); }
    .panel-title strong { color: var(--text); }
    .files { list-style: none; margin: 0; padding: 10px; display: grid; gap: 8px; overflow: auto; max-height: calc(100vh - 160px); }
    .file { display: grid; grid-template-columns: auto auto 1fr auto; align-items: center; gap: 8px; padding: 9px 10px; border: 1px solid transparent; border-radius: 12px; background: var(--panel); color: var(--muted); }
    .file.selected { border-color: var(--accent); background: #172554; color: var(--text); }
    .selection-bar { color: var(--accent); font-weight: 900; }
    .review-marker { color: var(--muted); }
    .review-marker.accepted { color: var(--green); }
    .review-marker.rejected { color: var(--red); }
    .file-icon { color: var(--accent); font-weight: 900; min-width: 1.2em; text-align: center; }
    .file-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .stats { color: var(--muted); white-space: nowrap; font-size: 12px; }
    .diff-panel { display: grid; grid-template-rows: auto 1fr; }
    .diff-body { min-height: 0; overflow: auto; }
    .empty { min-height: 320px; display: grid; place-items: center; text-align: center; color: var(--muted); padding: 30px; }
    .file-actions, .hunk-actions { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
    .hunk { border-bottom: 1px solid #1f2937; }
    .hunk-header { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 9px 12px; background: #111827; border-top: 1px solid #1f2937; color: var(--muted); }
    .hunk.selected .hunk-header { background: #172554; color: var(--text); box-shadow: inset 3px 0 0 var(--accent); }
    .diff-table { width: 100%; border-collapse: collapse; table-layout: fixed; }
    .diff-table td { padding: 0; vertical-align: top; }
    .line-no { width: 58px; user-select: none; text-align: right; color: #64748b; background: #0b1220; padding: 2px 8px !important; border-right: 1px solid #1f2937; }
    .line-prefix { width: 24px; text-align: center; color: #94a3b8; }
    .line-content { white-space: pre-wrap; overflow-wrap: anywhere; padding: 2px 10px !important; }
    .line-add { background: var(--code-add); }
    .line-add .line-prefix, .line-add .line-content { color: #bbf7d0; }
    .line-remove { background: var(--code-remove); }
    .line-remove .line-prefix, .line-remove .line-content { color: #fecdd3; }
    .line-context { background: #0f172a; color: #cbd5e1; }
    .binary-card { margin: 18px; padding: 34px; border: 1px dashed #334155; border-radius: 16px; text-align: center; color: var(--muted); }
    .footer { min-height: 74px; display: grid; grid-template-rows: auto auto; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--border); background: var(--surface-raised); }
    .footer-main { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
    .footer-path { color: #bfdbfe; font-weight: 800; }
    .keybar { display: flex; gap: 12px; flex-wrap: wrap; color: var(--muted); }
    .key { color: var(--text); background: #0f172a; border: 1px solid #334155; border-radius: 7px; padding: 2px 6px; }
    textarea { width: 100%; min-height: 104px; border-radius: 12px; border: 1px solid #334155; background: #020617; color: var(--text); padding: 12px; }
    dialog { border: 1px solid #334155; border-radius: 18px; background: #111827; color: var(--text); max-width: 560px; width: calc(100% - 40px); }
    dialog::backdrop { background: rgb(0 0 0 / 0.62); }
    .palette-dialog { padding: 0; overflow: hidden; }
    .palette-input { width: 100%; border: 0; border-bottom: 1px solid #334155; background: #020617; color: var(--text); padding: 14px 16px; outline: none; }
    .palette-list { list-style: none; margin: 0; padding: 8px; display: grid; gap: 6px; max-height: 420px; overflow: auto; }
    .palette-item { display: grid; grid-template-columns: 1fr auto; gap: 12px; padding: 10px 12px; border-radius: 12px; color: var(--muted); }
    .palette-item.selected { background: #172554; color: var(--text); }
    .palette-item.disabled { opacity: 0.45; }
    .palette-detail { display: block; color: var(--muted); font-size: 12px; margin-top: 2px; }
    .session-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; max-height: 360px; overflow: auto; }
    .session-item { display: grid; gap: 3px; padding: 10px 12px; border: 1px solid #263244; border-radius: 12px; background: #0f172a; }
    .session-item.selected { border-color: var(--accent); background: #172554; }
    .history-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; max-height: 360px; overflow: auto; }
    .history-item { display: grid; gap: 3px; padding: 10px 12px; border: 1px solid #263244; border-radius: 12px; background: #0f172a; }
    @media (max-width: 920px) { .workspace { grid-template-columns: 1fr; } .files { max-height: 40vh; } .topbar { grid-template-columns: 1fr; height: auto; gap: 6px; padding: 10px 14px; } .brand, .repo, .counts { grid-column: auto; justify-self: start; max-width: 100%; } }
  </style>
</head>
<body>
  <div class="app">
    <header class="topbar">
      <div id="repo" class="repo mono">Loading…</div>
      <div class="brand"><span>›</span> better-review</div>
      <div class="counts">
        <span class="pill"><strong id="pending">0</strong> pending</span>
        <span class="pill"><strong id="accepted">0</strong> accepted</span>
        <span class="pill"><strong id="rejected">0</strong> rejected</span>
        <button id="openSettings" class="ghost">Settings</button>
      </div>
    </header>

    <section id="home" class="home">
      <div class="home-card">
        <div class="home-kicker">AI writes the code. You review it.</div>
        <h1 id="homeTitle" class="home-title">Loading <span>review</span></h1>
        <p id="homeDetail" class="home-detail">Loading the current worktree state.</p>
        <div class="home-progress">
          <div class="progress-track"><div id="homeProgress" class="progress-fill"></div></div>
          <div id="homeCounts" class="muted">0 pending · 0 accepted · 0 rejected</div>
        </div>
        <div class="home-actions">
          <button id="enterReview" class="primary">Enter review</button>
          <button id="homeRefresh">Refresh</button>
          <button id="homeCommit">Commit accepted</button>
        </div>
        <p id="homeStatus" class="muted">Press r to refresh or Enter to review when changes exist.</p>
      </div>
    </section>

    <main id="workspace" class="workspace hidden">
      <aside class="panel">
        <div class="panel-title">
          <span><span class="key">1</span> <strong>Files</strong></span>
          <button id="refresh" class="ghost">Refresh</button>
        </div>
        <ul id="files" class="files"><li class="empty">Loading files…</li></ul>
      </aside>

      <section class="panel diff-panel">
        <div class="panel-title">
          <span><span class="key">2</span> <strong id="diffTitle">Review</strong></span>
          <div class="file-actions">
            <button id="acceptCurrent">Accept</button>
            <button id="rejectCurrent" class="danger">Reject</button>
            <button id="unreviewCurrent">Unreview</button>
            <button id="openExplain">Explain</button>
            <button id="openCommit" class="primary">Commit</button>
            <button id="publishCurrent">Publish</button>
          </div>
        </div>
        <div id="diff" class="diff-body"><div class="empty">Loading review state…</div></div>
      </section>
    </main>

    <footer id="footer" class="footer hidden">
      <div class="footer-main">
        <span id="position" class="pill">0 / 0</span>
        <span id="footerPath" class="footer-path mono">No selection</span>
        <span id="focusLabel" class="muted">files</span>
        <span id="lineStats" class="muted">+0 -0</span>
        <span id="status" class="muted">Loading review state…</span>
      </div>
      <div class="keybar">
        <span><span class="key">j/k</span> move</span>
        <span><span class="key">Enter</span> hunks</span>
        <span><span class="key">Esc</span> files</span>
        <span><span class="key">Tab</span> next</span>
        <span><span class="key">y</span> accept</span>
        <span><span class="key">x</span> reject</span>
        <span><span class="key">u</span> unreview</span>
        <span><span class="key">e</span> explain</span>
        <span><span class="key">r</span> refresh</span>
        <span><span class="key">c</span> commit</span>
        <span><span class="key">p</span> publish</span>
        <span><span class="key">s</span> settings</span>
        <span><span class="key">Ctrl+P</span> commands</span>
      </div>
    </footer>
  </div>

  <dialog id="commitDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Commit accepted changes</h2>
      <textarea id="commitMessage" placeholder="Write the commit message for accepted changes"></textarea>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Cancel</button>
        <button id="submitCommit" class="primary" value="default">Commit</button>
      </div>
    </form>
  </dialog>

  <dialog id="explainDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Explain selection</h2>
      <div>
        <div class="muted">Scope</div>
        <code id="explainScope">No selection</code>
      </div>
      <div>
        <div class="muted">Context</div>
        <p id="explainContext" class="muted" style="margin: 4px 0 0;">Loading context source…</p>
      </div>
      <div>
        <div class="muted">Model</div>
        <p id="explainModel" class="muted" style="margin: 4px 0 0;">Auto</p>
      </div>
      <div>
        <div class="muted">Answer</div>
        <p id="explainAnswer" class="muted" style="margin: 4px 0 0;">No explanation has been requested yet.</p>
      </div>
      <div class="file-actions" style="justify-content: flex-end;">
        <button id="chooseExplainContext" value="default">Choose context</button>
        <button id="chooseExplainModel" value="default">Choose model</button>
        <button id="openExplainHistory" value="default">History</button>
        <button value="cancel">Close</button>
        <button id="requestExplain" class="primary" value="default">Explain</button>
      </div>
    </form>
  </dialog>

  <dialog id="sessionDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Choose Explain context</h2>
      <p id="sessionStatus" class="muted">Loading sessions…</p>
      <ul id="sessionList" class="session-list"></ul>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Close</button>
      </div>
    </form>
  </dialog>

  <dialog id="modelDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Choose Explain model</h2>
      <p id="modelStatus" class="muted">Loading models…</p>
      <ul id="modelList" class="session-list"></ul>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Close</button>
      </div>
    </form>
  </dialog>

  <dialog id="historyDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Explain history</h2>
      <p id="historyStatus" class="muted">No explanations in this session yet.</p>
      <ul id="historyList" class="history-list"></ul>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Close</button>
      </div>
    </form>
  </dialog>

  <dialog id="publishDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Publish reviewed commit</h2>
      <p class="muted">Push the current branch using git. If your remote uses HTTPS, save a GitHub token in Settings first.</p>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Not now</button>
        <button id="submitPublish" class="primary" value="default">Push current branch</button>
      </div>
    </form>
  </dialog>

  <dialog id="settingsDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Settings</h2>
      <p id="githubTokenStatus" class="muted">GitHub token is not set.</p>
      <textarea id="githubTokenInput" placeholder="Paste a GitHub token for HTTPS publishing"></textarea>
      <p class="muted">Stored locally and only used for git push authentication.</p>
      <div class="file-actions" style="justify-content: flex-end;">
        <button value="cancel">Cancel</button>
        <button id="saveGithubToken" class="primary" value="default">Save token</button>
      </div>
    </form>
  </dialog>

  <dialog id="commandPalette" class="palette-dialog">
    <input id="paletteInput" class="palette-input" placeholder="Type a command…" autocomplete="off" />
    <ul id="paletteList" class="palette-list"></ul>
  </dialog>

  <script>
    const token = new URLSearchParams(location.search).get('token');
    let state = null;
    let selectedFile = 0;
    let selectedHunk = 0;
    let focus = 'files';
    let screen = 'home';
    let paletteCursor = 0;
    let settings = { has_github_token: false };
    let explainSessions = { available: false, selected_session_id: null, sessions: [] };
    let explainModels = { available: false, selected_model: null, models: [] };
    let explainHistory = { runs: [] };

    const iconFor = (file) => {
      if (file.is_binary) return '◈';
      switch (file.status) {
        case 'Added': return '+';
        case 'Deleted': return '−';
        case 'Renamed': return '→';
        case 'Copied': return '⧉';
        case 'ModeChanged': return '⚙';
        default: return file.hunks.length ? '✎' : '○';
      }
    };
    const markerFor = (status) => status === 'Accepted' ? '[✓]' : status === 'Rejected' ? '[x]' : '[ ]';
    const prefixFor = (kind) => kind === 'Add' ? '+' : kind === 'Remove' ? '-' : ' ';
    const lineClass = (kind) => kind === 'Add' ? 'line-add' : kind === 'Remove' ? 'line-remove' : 'line-context';

    function commandItems() {
      const file = currentFile();
      const reviewAvailable = !!state?.files.length;
      const inReview = screen === 'review' && reviewAvailable;
      const hasHunks = inReview && !!file?.hunks.length;
      return [
        { label: 'Refresh changes', detail: 'Reload the current worktree diff', shortcut: 'r', enabled: true, run: () => mutate('/api/refresh', 'Refreshed review queue.') },
        { label: 'Enter review', detail: 'Open the review workspace', shortcut: 'Enter', enabled: screen === 'home' && reviewAvailable, run: () => enterReview() },
        { label: 'Back to home', detail: 'Return to the better-review home screen', shortcut: 'Esc', enabled: screen === 'review', run: () => { screen = 'home'; focus = 'files'; renderState(state); setStatus('Back on the better-review home screen.'); } },
        { label: 'Focus files', detail: 'Move focus to the changed-file sidebar', shortcut: 'Esc', enabled: inReview && focus === 'hunks', run: () => { focus = 'files'; renderState(state); setStatus('Focused changed files.'); } },
        { label: 'Focus hunks', detail: 'Move focus into the diff hunks', shortcut: 'Enter', enabled: hasHunks, run: () => { focus = 'hunks'; renderState(state); setStatus('Focused diff hunks.'); } },
        { label: 'Accept selection', detail: 'Stage the current file or hunk for commit', shortcut: 'y', enabled: inReview, run: acceptCurrent },
        { label: 'Reject selection', detail: 'Leave the current file or hunk out of the commit', shortcut: 'x', enabled: inReview, run: rejectCurrent },
        { label: 'Move file to unreviewed', detail: 'Unstage the current file and mark it pending', shortcut: 'u', enabled: inReview, run: unreviewCurrent },
        { label: 'Open Explain menu', detail: 'Preview the current file or hunk explanation target', shortcut: 'e', enabled: inReview, run: openExplainMenu },
        { label: 'Choose Explain context', detail: 'Select the opencode session used for Explain', shortcut: 'o', enabled: true, run: openSessionPicker },
        { label: 'Choose Explain model', detail: 'Select the model used for Explain', shortcut: 'm', enabled: true, run: openModelPicker },
        { label: 'Open Explain history', detail: 'Show explanations from this browser session', shortcut: 'h', enabled: true, run: openExplainHistory },
        { label: 'Commit accepted changes', detail: 'Write a commit message for accepted changes', shortcut: 'c', enabled: reviewAvailable, run: () => document.getElementById('commitDialog').showModal() },
        { label: 'Publish current branch', detail: 'Push the reviewed commit from the current branch', shortcut: 'p', enabled: true, run: () => document.getElementById('publishDialog').showModal() },
        { label: 'Open settings', detail: 'Configure GitHub token for HTTPS publishing', shortcut: 's', enabled: true, run: openSettings },
      ];
    }

    function filteredCommandItems() {
      const query = document.getElementById('paletteInput').value.trim().toLowerCase();
      const items = commandItems();
      if (!query) return items;
      return items.filter((item) => `${item.label} ${item.detail} ${item.shortcut}`.toLowerCase().includes(query));
    }

    function openCommandPalette() {
      paletteCursor = 0;
      document.getElementById('paletteInput').value = '';
      renderCommandPalette();
      document.getElementById('commandPalette').showModal();
      document.getElementById('paletteInput').focus();
      setStatus('Command palette opened.');
    }

    function renderCommandPalette() {
      const list = document.getElementById('paletteList');
      const items = filteredCommandItems();
      paletteCursor = clamp(paletteCursor, 0, Math.max(0, items.length - 1));
      list.innerHTML = '';
      if (!items.length) { list.innerHTML = '<li class="palette-item disabled">No commands found</li>'; return; }
      items.forEach((item, index) => {
        const row = document.createElement('li');
        row.className = `palette-item ${index === paletteCursor ? 'selected' : ''} ${item.enabled ? '' : 'disabled'}`;
        row.innerHTML = `<div><strong></strong><span class="palette-detail"></span></div><span class="key"></span>`;
        row.querySelector('strong').textContent = item.label;
        row.querySelector('.palette-detail').textContent = item.detail;
        row.querySelector('.key').textContent = item.shortcut;
        row.addEventListener('mouseenter', () => { paletteCursor = index; renderCommandPalette(); });
        row.addEventListener('click', () => runPaletteCommand(item));
        list.appendChild(row);
      });
    }

    async function runPaletteCommand(item = filteredCommandItems()[paletteCursor]) {
      if (!item) return;
      if (!item.enabled) { setStatus(`${item.label} is unavailable right now.`); return; }
      document.getElementById('commandPalette').close();
      await item.run();
    }

    async function request(path, options = {}) {
      const separator = path.includes('?') ? '&' : '?';
      const response = await fetch(`${path}${separator}token=${encodeURIComponent(token || '')}`, {
        headers: { 'content-type': 'application/json', ...(options.headers || {}) },
        ...options,
      });
      if (!response.ok) {
        let message = await response.text();
        try { message = JSON.parse(message).error || message; } catch (_) {}
        throw new Error(message);
      }
      return response.json();
    }

    async function loadState(message = 'Review state loaded.') {
      settings = await request('/api/settings');
      explainSessions = await request('/api/explain/sessions');
      explainModels = await request('/api/explain/models');
      explainHistory = await request('/api/explain/history');
      renderSettingsStatus();
      renderExplainContext();
      renderExplainModel();
      renderState(await request('/api/state'));
      setStatus(message);
    }

    async function mutate(path, message) {
      const result = await request(path, { method: 'POST' });
      renderState(result.state);
      setStatus(result.message || message);
    }

    function renderSettingsStatus() {
      document.getElementById('githubTokenStatus').textContent = settings.has_github_token ? 'GitHub token is saved.' : 'GitHub token is not set.';
    }

    async function openSettings() {
      settings = await request('/api/settings');
      renderSettingsStatus();
      document.getElementById('githubTokenInput').value = '';
      document.getElementById('settingsDialog').showModal();
      document.getElementById('githubTokenInput').focus();
      setStatus('Settings opened.');
    }

    async function saveGithubToken() {
      settings = await request('/api/settings/github-token', {
        method: 'POST',
        body: JSON.stringify({ token: document.getElementById('githubTokenInput').value }),
      });
      renderSettingsStatus();
      document.getElementById('settingsDialog').close();
      document.getElementById('githubTokenInput').value = '';
      setStatus(settings.has_github_token ? 'GitHub token saved.' : 'GitHub token cleared.');
    }

    async function publishCurrentBranch() {
      const result = await request('/api/push', { method: 'POST' });
      renderState(result.state);
      document.getElementById('publishDialog').close();
      setStatus(result.message);
    }

    function selectedExplainSession() {
      return explainSessions.sessions.find((session) => session.id === explainSessions.selected_session_id);
    }

    function explainContextLabel() {
      if (!explainSessions.available) return 'Explain is unavailable because opencode is not ready.';
      const session = selectedExplainSession();
      if (!session) return 'No context source selected.';
      return `${session.title} (${session.id})`;
    }

    function renderExplainContext() {
      const context = document.getElementById('explainContext');
      if (context) context.textContent = explainContextLabel();
    }

    function explainModelLabel() {
      if (!explainModels.available) return 'Explain is unavailable because opencode is not ready.';
      return explainModels.selected_model || 'Auto';
    }

    function renderExplainModel() {
      const model = document.getElementById('explainModel');
      if (model) model.textContent = explainModelLabel();
    }

    async function openSessionPicker() {
      explainSessions = await request('/api/explain/sessions');
      renderExplainContext();
      renderSessionList();
      document.getElementById('sessionDialog').showModal();
      setStatus('Choose an Explain context source.');
    }

    function renderSessionList() {
      const status = document.getElementById('sessionStatus');
      const list = document.getElementById('sessionList');
      list.innerHTML = '';
      if (!explainSessions.available) {
        status.textContent = 'Explain is unavailable because opencode is not ready.';
        return;
      }
      if (!explainSessions.sessions.length) {
        status.textContent = 'No opencode sessions were found for this repository.';
        return;
      }
      status.textContent = 'Select the opencode session to use as Explain context.';
      explainSessions.sessions.forEach((session) => {
        const row = document.createElement('li');
        row.className = `session-item ${session.id === explainSessions.selected_session_id ? 'selected' : ''}`;
        row.innerHTML = '<strong></strong><span class="muted mono"></span><span class="muted"></span>';
        row.querySelector('strong').textContent = session.title || session.id;
        row.querySelector('.mono').textContent = session.id;
        row.querySelectorAll('.muted')[1].textContent = session.directory;
        row.addEventListener('click', () => selectExplainSession(session.id).catch(showError));
        list.appendChild(row);
      });
    }

    async function selectExplainSession(sessionId) {
      explainSessions = await request('/api/explain/session', {
        method: 'POST',
        body: JSON.stringify({ session_id: sessionId }),
      });
      renderExplainContext();
      renderSessionList();
      document.getElementById('sessionDialog').close();
      setStatus(`Explain will use context source ${explainContextLabel()}.`);
    }

    async function openModelPicker() {
      explainModels = await request('/api/explain/models');
      renderExplainModel();
      renderModelList();
      document.getElementById('modelDialog').showModal();
      setStatus('Choose an Explain model.');
    }

    function renderModelList() {
      const status = document.getElementById('modelStatus');
      const list = document.getElementById('modelList');
      list.innerHTML = '';
      if (!explainModels.available) {
        status.textContent = 'Explain is unavailable because opencode is not ready.';
        return;
      }
      status.textContent = 'Choose Auto or a specific opencode model.';
      renderModelRow(list, null, 'Auto');
      explainModels.models.forEach((model) => renderModelRow(list, model, model));
    }

    function renderModelRow(list, model, label) {
      const row = document.createElement('li');
      row.className = `session-item ${model === explainModels.selected_model ? 'selected' : ''}`;
      row.innerHTML = '<strong></strong><span class="muted"></span>';
      row.querySelector('strong').textContent = label;
      row.querySelector('.muted').textContent = model ? 'Explicit model' : 'Use saved/session default when available';
      row.addEventListener('click', () => selectExplainModel(model).catch(showError));
      list.appendChild(row);
    }

    async function selectExplainModel(model) {
      explainModels = await request('/api/explain/model', {
        method: 'POST',
        body: JSON.stringify({ model }),
      });
      renderExplainModel();
      renderModelList();
      document.getElementById('modelDialog').close();
      setStatus(`Explain model set to ${explainModelLabel()}.`);
    }

    async function openExplainHistory() {
      explainHistory = await request('/api/explain/history');
      renderExplainHistory();
      document.getElementById('historyDialog').showModal();
      setStatus('Explain history opened.');
    }

    function renderExplainHistory() {
      const status = document.getElementById('historyStatus');
      const list = document.getElementById('historyList');
      list.innerHTML = '';
      if (!explainHistory.runs.length) {
        status.textContent = 'No explanations in this session yet.';
        return;
      }
      status.textContent = 'Explain runs from this browser session.';
      explainHistory.runs.forEach((run) => {
        const row = document.createElement('li');
        row.className = 'history-item';
        row.innerHTML = '<strong></strong><span class="muted"></span><span class="muted"></span>';
        row.querySelector('strong').textContent = run.label;
        row.querySelectorAll('.muted')[0].textContent = `${run.status} · ${run.model}`;
        row.querySelectorAll('.muted')[1].textContent = `run ${run.id}`;
        list.appendChild(row);
      });
    }

    function explainTargetLabel() {
      const file = currentFile();
      if (!file) return 'No selection';
      if (focus === 'hunks' && file.hunks.length) return `hunk ${file.display_label} ${file.hunks[selectedHunk].header}`;
      return `file ${file.display_label}`;
    }

    async function openExplainMenu() {
      explainSessions = await request('/api/explain/sessions');
      explainModels = await request('/api/explain/models');
      document.getElementById('explainScope').textContent = explainTargetLabel();
      renderExplainContext();
      renderExplainModel();
      document.getElementById('explainAnswer').textContent = 'No explanation has been requested yet.';
      document.getElementById('explainDialog').showModal();
      setStatus('Explain menu opened.');
    }

    function requestExplainPreview() {
      document.getElementById('explainAnswer').textContent = 'Explain execution will be wired up after context and model selection are available.';
      setStatus('Explain request flow is not connected yet.');
    }

    function renderState(nextState) {
      state = nextState;
      selectedFile = clamp(selectedFile, 0, Math.max(0, state.files.length - 1));
      const file = currentFile();
      selectedHunk = clamp(selectedHunk, 0, Math.max(0, (file?.hunks.length || 1) - 1));

      document.getElementById('repo').textContent = state.repo_path;
      document.getElementById('pending').textContent = state.counts.unreviewed;
      document.getElementById('accepted').textContent = state.counts.accepted;
      document.getElementById('rejected').textContent = state.counts.rejected;
      if (!state.files.length) screen = 'home';
      renderHome();
      renderFiles();
      renderDiff();
      renderFooter();
      renderLayout();
    }

    function renderLayout() {
      const onHome = screen === 'home';
      document.getElementById('home').classList.toggle('hidden', !onHome);
      document.getElementById('workspace').classList.toggle('hidden', onHome);
      document.getElementById('footer').classList.toggle('hidden', onHome);
    }

    function renderHome() {
      const total = state.counts.unreviewed + state.counts.accepted + state.counts.rejected;
      const reviewed = state.counts.accepted + state.counts.rejected;
      const progress = total ? Math.round((reviewed / total) * 100) : 0;
      let title = 'No changes';
      let detail = 'Run your coding agent or make changes, then refresh the review queue.';
      if (total && state.counts.unreviewed) {
        title = 'Ready to review';
        detail = 'Open the review workspace and accept only the file or hunk changes that belong.';
      } else if (state.counts.accepted) {
        title = 'Ready to commit';
        detail = 'All current review items have a decision. Commit accepted staged changes when ready.';
      } else if (total) {
        title = 'Nothing accepted';
        detail = 'Rejected changes stay in your worktree and are left out of the commit.';
      }
      document.getElementById('homeTitle').innerHTML = `${title.replace('review', '<span>review</span>')}`;
      document.getElementById('homeDetail').textContent = detail;
      document.getElementById('homeProgress').style.width = `${progress}%`;
      document.getElementById('homeCounts').textContent = `${state.counts.unreviewed} pending · ${state.counts.accepted} accepted · ${state.counts.rejected} rejected`;
      document.getElementById('enterReview').disabled = !state.files.length;
    }

    function enterReview() {
      if (!state?.files.length) { setStatus('No reviewable changes yet. Refresh after making changes.'); return; }
      screen = 'review';
      focus = 'files';
      renderState(state);
      setStatus('Review workspace ready.');
    }

    function renderFiles() {
      const files = document.getElementById('files');
      files.innerHTML = '';
      if (!state.files.length) {
        files.innerHTML = '<li class="empty">No reviewable changes.<br><span class="muted">Run your agent, then refresh.</span></li>';
        return;
      }
      state.files.forEach((file, index) => {
        const item = document.createElement('li');
        item.className = `file ${index === selectedFile ? 'selected' : ''}`;
        const stats = lineStats(file);
        item.innerHTML = `
          <span class="selection-bar">${index === selectedFile ? '▌' : ' '}</span>
          <span class="review-marker ${file.review_status.toLowerCase()}">${markerFor(file.review_status)}</span>
          <span class="file-label"><span class="file-icon">${iconFor(file)}</span> <span class="mono"></span></span>
          <span class="stats">+${stats.added} -${stats.removed}</span>`;
        item.querySelector('.mono').textContent = file.display_label;
        item.addEventListener('click', () => { selectedFile = index; selectedHunk = 0; focus = 'files'; screen = 'review'; renderState(state); });
        files.appendChild(item);
      });
    }

    function renderDiff() {
      const diff = document.getElementById('diff');
      const title = document.getElementById('diffTitle');
      const file = currentFile();
      diff.innerHTML = '';
      if (!file) {
        title.textContent = 'Review';
        diff.innerHTML = '<div class="empty">No changes to review.</div>';
        return;
      }

      title.textContent = file.display_label;
      if (file.is_binary || !file.hunks.length) {
        diff.innerHTML = `<div class="binary-card"><h2>${file.is_binary ? 'Binary file' : 'No text hunks'}</h2><p>${file.is_binary ? 'This change cannot be shown as a text diff.' : 'This file changed, but there is no patch body to render.'}</p></div>`;
        return;
      }

      file.hunks.forEach((hunk, hunkIndex) => {
        const section = document.createElement('section');
        section.className = `hunk ${focus === 'hunks' && hunkIndex === selectedHunk ? 'selected' : ''}`;
        section.innerHTML = `
          <div class="hunk-header">
            <code></code>
            <div class="hunk-actions">
              <span class="review-marker ${hunk.review_status.toLowerCase()}">${markerFor(hunk.review_status)}</span>
              <button data-action="accept-hunk">Accept</button>
              <button data-action="reject-hunk" class="danger">Reject</button>
            </div>
          </div>
          <table class="diff-table"><tbody></tbody></table>`;
        section.querySelector('code').textContent = hunk.header;
        section.querySelector('[data-action="accept-hunk"]').addEventListener('click', () => mutate(`/api/files/${selectedFile}/hunks/${hunkIndex}/accept`, 'Accepted hunk.').catch(showError));
        section.querySelector('[data-action="reject-hunk"]').addEventListener('click', () => mutate(`/api/files/${selectedFile}/hunks/${hunkIndex}/reject`, 'Rejected hunk.').catch(showError));
        const body = section.querySelector('tbody');
        hunk.lines.forEach((line) => body.appendChild(renderDiffLine(line)));
        diff.appendChild(section);
      });
      scrollSelectedHunkIntoView();
    }

    function renderDiffLine(line) {
      const row = document.createElement('tr');
      row.className = lineClass(line.kind);
      row.innerHTML = `
        <td class="line-no">${line.old_line ?? ''}</td>
        <td class="line-no">${line.new_line ?? ''}</td>
        <td class="line-prefix">${prefixFor(line.kind)}</td>
        <td class="line-content"></td>`;
      row.querySelector('.line-content').textContent = line.content;
      return row;
    }

    function renderFooter() {
      const file = currentFile();
      document.getElementById('position').textContent = `${state.files.length ? selectedFile + 1 : 0} / ${state.files.length}`;
      document.getElementById('footerPath').textContent = file ? file.display_label : 'No selection';
      document.getElementById('focusLabel').textContent = file && focus === 'hunks' ? `hunk ${selectedHunk + 1}/${Math.max(file.hunks.length, 1)}` : 'file';
      const stats = file ? lineStats(file) : { added: 0, removed: 0 };
      document.getElementById('lineStats').textContent = `+${stats.added} -${stats.removed}`;
    }

    function lineStats(file) {
      return file.hunks.reduce((stats, hunk) => {
        hunk.lines.forEach((line) => {
          if (line.kind === 'Add') stats.added += 1;
          if (line.kind === 'Remove') stats.removed += 1;
        });
        return stats;
      }, { added: 0, removed: 0 });
    }

    function currentFile() { return state?.files[selectedFile]; }
    function clamp(value, min, max) { return Math.min(max, Math.max(min, value)); }
    function setStatus(message) { document.getElementById('status').textContent = message; document.getElementById('homeStatus').textContent = message; }
    function showError(error) { setStatus(error.message); }
    function scrollSelectedHunkIntoView() {
      if (focus !== 'hunks') return;
      document.querySelector('.hunk.selected')?.scrollIntoView({ block: 'nearest' });
    }

    async function acceptCurrent() {
      const file = currentFile();
      if (!file) return;
      if (focus === 'hunks' && file.hunks.length) {
        await mutate(`/api/files/${selectedFile}/hunks/${selectedHunk}/accept`, 'Accepted hunk.');
      } else {
        await mutate(`/api/files/${selectedFile}/accept`, 'Accepted file.');
      }
    }
    async function rejectCurrent() {
      const file = currentFile();
      if (!file) return;
      if (focus === 'hunks' && file.hunks.length) {
        await mutate(`/api/files/${selectedFile}/hunks/${selectedHunk}/reject`, 'Rejected hunk.');
      } else {
        await mutate(`/api/files/${selectedFile}/reject`, 'Rejected file.');
      }
    }
    async function unreviewCurrent() {
      if (!currentFile()) return;
      await mutate(`/api/files/${selectedFile}/unreview`, 'Moved file back to unreviewed.');
    }

    document.getElementById('refresh').addEventListener('click', () => mutate('/api/refresh', 'Refreshed review queue.').catch(showError));
    document.getElementById('homeRefresh').addEventListener('click', () => mutate('/api/refresh', 'Refreshed review queue.').catch(showError));
    document.getElementById('enterReview').addEventListener('click', enterReview);
    document.getElementById('homeCommit').addEventListener('click', () => document.getElementById('commitDialog').showModal());
    document.getElementById('openSettings').addEventListener('click', () => openSettings().catch(showError));
    document.getElementById('acceptCurrent').addEventListener('click', () => acceptCurrent().catch(showError));
    document.getElementById('rejectCurrent').addEventListener('click', () => rejectCurrent().catch(showError));
    document.getElementById('unreviewCurrent').addEventListener('click', () => unreviewCurrent().catch(showError));
    document.getElementById('openExplain').addEventListener('click', () => openExplainMenu().catch(showError));
    document.getElementById('chooseExplainContext').addEventListener('click', (event) => { event.preventDefault(); openSessionPicker().catch(showError); });
    document.getElementById('chooseExplainModel').addEventListener('click', (event) => { event.preventDefault(); openModelPicker().catch(showError); });
    document.getElementById('openExplainHistory').addEventListener('click', (event) => { event.preventDefault(); openExplainHistory().catch(showError); });
    document.getElementById('requestExplain').addEventListener('click', (event) => { event.preventDefault(); requestExplainPreview(); });
    document.getElementById('openCommit').addEventListener('click', () => document.getElementById('commitDialog').showModal());
    document.getElementById('publishCurrent').addEventListener('click', () => document.getElementById('publishDialog').showModal());
    document.getElementById('submitPublish').addEventListener('click', (event) => { event.preventDefault(); publishCurrentBranch().catch(showError); });
    document.getElementById('saveGithubToken').addEventListener('click', (event) => { event.preventDefault(); saveGithubToken().catch(showError); });
    document.getElementById('paletteInput').addEventListener('input', () => { paletteCursor = 0; renderCommandPalette(); });
    document.getElementById('paletteInput').addEventListener('keydown', (event) => {
      if (event.key === 'Escape') { document.getElementById('commandPalette').close(); setStatus('Command palette closed.'); event.preventDefault(); }
      else if (event.key === 'ArrowDown' || event.key === 'j') { paletteCursor += 1; renderCommandPalette(); event.preventDefault(); }
      else if (event.key === 'ArrowUp' || event.key === 'k') { paletteCursor -= 1; renderCommandPalette(); event.preventDefault(); }
      else if (event.key === 'Enter') { runPaletteCommand().catch(showError); event.preventDefault(); }
    });

    document.getElementById('submitCommit').addEventListener('click', async (event) => {
      event.preventDefault();
      try {
        const message = document.getElementById('commitMessage').value;
        const result = await request('/api/commit', { method: 'POST', body: JSON.stringify({ message }) });
        document.getElementById('commitDialog').close();
        document.getElementById('commitMessage').value = '';
        renderState(result.state);
        setStatus(result.message);
        document.getElementById('publishDialog').showModal();
      } catch (error) { showError(error); }
    });

    document.addEventListener('keydown', (event) => {
      if ((event.ctrlKey || event.metaKey) && (event.key === 'p' || event.key === 'k')) {
        event.preventDefault();
        openCommandPalette();
        return;
      }
      if (event.target.closest('textarea, dialog')) return;
      const file = currentFile();
      if (screen === 'home') {
        if (event.key === 'Enter') { enterReview(); event.preventDefault(); }
        else if (event.key === 'r') { mutate('/api/refresh', 'Refreshed review queue.').catch(showError); event.preventDefault(); }
        else if (event.key === 'c') { document.getElementById('commitDialog').showModal(); event.preventDefault(); }
        else if (event.key === 'p') { document.getElementById('publishDialog').showModal(); event.preventDefault(); }
        else if (event.key === 's') { openSettings().catch(showError); event.preventDefault(); }
        else if (event.key === 'o') { openSessionPicker().catch(showError); event.preventDefault(); }
        else if (event.key === 'm') { openModelPicker().catch(showError); event.preventDefault(); }
        else if (event.key === 'h') { openExplainHistory().catch(showError); event.preventDefault(); }
        return;
      }
      if (event.key === 'j' || event.key === 'ArrowDown') {
        if (focus === 'hunks' && file?.hunks.length) selectedHunk = clamp(selectedHunk + 1, 0, file.hunks.length - 1);
        else { selectedFile = clamp(selectedFile + 1, 0, Math.max(0, (state?.files.length || 1) - 1)); selectedHunk = 0; }
        renderState(state); event.preventDefault();
      } else if (event.key === 'k' || event.key === 'ArrowUp') {
        if (focus === 'hunks' && file?.hunks.length) selectedHunk = clamp(selectedHunk - 1, 0, file.hunks.length - 1);
        else { selectedFile = clamp(selectedFile - 1, 0, Math.max(0, (state?.files.length || 1) - 1)); selectedHunk = 0; }
        renderState(state); event.preventDefault();
      } else if (event.key === 'Enter') {
        if (file?.hunks.length) focus = 'hunks'; renderState(state); event.preventDefault();
      } else if (event.key === 'Escape') {
        if (focus === 'hunks') focus = 'files'; else screen = 'home'; renderState(state); event.preventDefault();
      } else if (event.key === 'Tab') {
        if (file?.hunks.length) { selectedHunk = (selectedHunk + 1) % file.hunks.length; focus = 'hunks'; renderState(state); }
        event.preventDefault();
      } else if (event.key === 'y') acceptCurrent().catch(showError);
      else if (event.key === 'x') rejectCurrent().catch(showError);
      else if (event.key === 'u') unreviewCurrent().catch(showError);
      else if (event.key === 'e') openExplainMenu().catch(showError);
      else if (event.key === 'o') openSessionPicker().catch(showError);
      else if (event.key === 'm') openModelPicker().catch(showError);
      else if (event.key === 'h') openExplainHistory().catch(showError);
      else if (event.key === 'r') mutate('/api/refresh', 'Refreshed review queue.').catch(showError);
      else if (event.key === 'c') document.getElementById('commitDialog').showModal();
      else if (event.key === 'p') document.getElementById('publishDialog').showModal();
      else if (event.key === 's') openSettings().catch(showError);
    });

    loadState().catch(showError);
  </script>
</body>
</html>
"#;
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
            explain: Mutex::new(WebExplainState {
                sessions: Vec::new(),
                selected_session_id: None,
                models: Vec::new(),
                selected_model: None,
                history: Vec::new(),
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
