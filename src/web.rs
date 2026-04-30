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
use crate::services::git::GitService;

pub async fn run() -> Result<()> {
    let repo_path = std::env::current_dir().context("failed to resolve current directory")?;
    let git = GitService::new(&repo_path);
    let (_, files) = git.collect_diff().await?;
    let had_staged_changes_on_open = git.has_staged_changes().await?;
    let token = local_session_token();
    let state = Arc::new(WebState {
        git,
        repo_path,
        token,
        review: Mutex::new(WebReviewState {
            files,
            had_staged_changes_on_open,
        }),
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/api/refresh", post(api_refresh))
        .route("/api/commit", post(api_commit))
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
    review: Mutex<WebReviewState>,
}

struct WebReviewState {
    files: Vec<FileDiff>,
    had_staged_changes_on_open: bool,
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

fn ensure_authorized(state: &WebState, auth: AuthQuery) -> Result<(), ApiError> {
    if auth.token.as_deref() == Some(state.token.as_str()) {
        Ok(())
    } else {
        Err(ApiError::unauthorized(
            "missing or invalid local session token",
        ))
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
    .counts { justify-self: end; display: flex; gap: 8px; color: var(--muted); }
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
            <button id="openCommit" class="primary">Commit</button>
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
        <span><span class="key">r</span> refresh</span>
        <span><span class="key">c</span> commit</span>
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

  <script>
    const token = new URLSearchParams(location.search).get('token');
    let state = null;
    let selectedFile = 0;
    let selectedHunk = 0;
    let focus = 'files';
    let screen = 'home';

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
      renderState(await request('/api/state'));
      setStatus(message);
    }

    async function mutate(path, message) {
      const result = await request(path, { method: 'POST' });
      renderState(result.state);
      setStatus(result.message || message);
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
    document.getElementById('acceptCurrent').addEventListener('click', () => acceptCurrent().catch(showError));
    document.getElementById('rejectCurrent').addEventListener('click', () => rejectCurrent().catch(showError));
    document.getElementById('unreviewCurrent').addEventListener('click', () => unreviewCurrent().catch(showError));
    document.getElementById('openCommit').addEventListener('click', () => document.getElementById('commitDialog').showModal());
    document.getElementById('submitCommit').addEventListener('click', async (event) => {
      event.preventDefault();
      try {
        const message = document.getElementById('commitMessage').value;
        const result = await request('/api/commit', { method: 'POST', body: JSON.stringify({ message }) });
        document.getElementById('commitDialog').close();
        document.getElementById('commitMessage').value = '';
        renderState(result.state);
        setStatus(result.message);
      } catch (error) { showError(error); }
    });

    document.addEventListener('keydown', (event) => {
      if (event.target.closest('textarea, dialog')) return;
      const file = currentFile();
      if (screen === 'home') {
        if (event.key === 'Enter') { enterReview(); event.preventDefault(); }
        else if (event.key === 'r') { mutate('/api/refresh', 'Refreshed review queue.').catch(showError); event.preventDefault(); }
        else if (event.key === 'c') { document.getElementById('commitDialog').showModal(); event.preventDefault(); }
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
      else if (event.key === 'r') mutate('/api/refresh', 'Refreshed review queue.').catch(showError);
      else if (event.key === 'c') document.getElementById('commitDialog').showModal();
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
