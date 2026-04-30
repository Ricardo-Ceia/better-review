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
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; min-height: 100vh; background: #0b1020; color: #e5e7eb; }
    main { max-width: 1180px; margin: 0 auto; padding: 32px; }
    header { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; margin-bottom: 24px; }
    h1 { margin: 0; font-size: 28px; letter-spacing: -0.04em; }
    h2, h3 { letter-spacing: -0.03em; }
    .muted { color: #94a3b8; }
    .card { background: #111827; border: 1px solid #253044; border-radius: 18px; padding: 20px; box-shadow: 0 24px 80px rgb(0 0 0 / 0.24); }
    .counts, .actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
    .pill { border: 1px solid #334155; border-radius: 999px; padding: 8px 12px; background: #0f172a; }
    .grid { display: grid; grid-template-columns: minmax(320px, 420px) 1fr; gap: 18px; align-items: start; }
    .files { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
    .file { display: grid; gap: 10px; padding: 12px; border: 1px solid #263244; border-radius: 14px; background: #0f172a; }
    .file-header { display: grid; grid-template-columns: auto 1fr auto; gap: 10px; align-items: center; }
    .icon { color: #60a5fa; font-weight: 800; }
    .status { font-size: 12px; text-transform: lowercase; color: #94a3b8; }
    .hunks { display: grid; gap: 8px; margin-top: 4px; }
    .hunk { border-top: 1px solid #1f2937; padding-top: 8px; display: grid; gap: 8px; }
    code { color: #bfdbfe; word-break: break-all; }
    pre { overflow: auto; margin: 0; white-space: pre-wrap; color: #cbd5e1; max-height: 68vh; }
    button { cursor: pointer; border: 1px solid #334155; color: #e5e7eb; background: #1e293b; padding: 8px 12px; border-radius: 10px; }
    button:hover { background: #334155; }
    button.primary { border-color: #2563eb; background: #1d4ed8; }
    button.danger { border-color: #7f1d1d; background: #450a0a; }
    textarea { width: 100%; min-height: 96px; box-sizing: border-box; border-radius: 12px; border: 1px solid #334155; background: #020617; color: #e5e7eb; padding: 12px; }
    dialog { border: 1px solid #334155; border-radius: 18px; background: #111827; color: #e5e7eb; max-width: 560px; width: calc(100% - 40px); }
    dialog::backdrop { background: rgb(0 0 0 / 0.62); }
    @media (max-width: 920px) { .grid { grid-template-columns: 1fr; } header { flex-direction: column; } }
  </style>
</head>
<body>
  <main>
    <header>
      <div>
        <h1>better-review web</h1>
        <p class="muted">Local browser review mode. Review state and git decisions are handled by the Rust review engine.</p>
      </div>
      <div class="actions">
        <button id="refresh">Refresh</button>
        <button id="openCommit" class="primary">Commit accepted</button>
      </div>
    </header>

    <section class="card" style="margin-bottom: 18px;">
      <div class="muted">Repository</div>
      <code id="repo">Loading…</code>
      <div class="counts" style="margin-top: 14px;">
        <span class="pill"><strong id="pending">0</strong> pending</span>
        <span class="pill"><strong id="accepted">0</strong> accepted</span>
        <span class="pill"><strong id="rejected">0</strong> rejected</span>
      </div>
      <p id="status" class="muted">Loading review state…</p>
    </section>

    <section class="grid">
      <div class="card">
        <h2 style="margin-top: 0;">Files</h2>
        <ul id="files" class="files"><li class="muted">Loading files…</li></ul>
      </div>
      <div class="card">
        <h2 style="margin-top: 0;">State JSON</h2>
        <pre id="json">Loading…</pre>
      </div>
    </section>
  </main>

  <dialog id="commitDialog">
    <form method="dialog" style="display: grid; gap: 14px;">
      <h2 style="margin: 0;">Commit accepted changes</h2>
      <textarea id="commitMessage" placeholder="Write the commit message for accepted changes"></textarea>
      <div class="actions" style="justify-content: flex-end;">
        <button value="cancel">Cancel</button>
        <button id="submitCommit" class="primary" value="default">Commit</button>
      </div>
    </form>
  </dialog>

  <script>
    const token = new URLSearchParams(location.search).get('token');
    let currentState = null;

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

    async function loadState() {
      renderState(await request('/api/state'));
      setStatus('Review state loaded.');
    }

    async function mutate(path, message) {
      const result = await request(path, { method: 'POST' });
      renderState(result.state);
      setStatus(result.message || message);
    }

    function renderState(state) {
      currentState = state;
      document.getElementById('repo').textContent = state.repo_path;
      document.getElementById('pending').textContent = state.counts.unreviewed;
      document.getElementById('accepted').textContent = state.counts.accepted;
      document.getElementById('rejected').textContent = state.counts.rejected;
      document.getElementById('json').textContent = JSON.stringify(state, null, 2);
      const files = document.getElementById('files');
      files.innerHTML = '';
      if (!state.files.length) {
        files.innerHTML = '<li class="muted">No reviewable changes.</li>';
        return;
      }
      state.files.forEach((file, fileIndex) => files.appendChild(renderFile(file, fileIndex)));
    }

    function renderFile(file, fileIndex) {
      const item = document.createElement('li');
      item.className = 'file';
      const hunkCount = file.hunks.length;
      item.innerHTML = `
        <div class="file-header">
          <span class="icon">${iconFor(file)}</span>
          <code></code>
          <span class="status">${file.review_status}</span>
        </div>
        <div class="actions">
          <button data-action="accept-file">Accept file</button>
          <button data-action="reject-file" class="danger">Reject file</button>
          <button data-action="unreview-file">Unreview</button>
          <span class="muted">${hunkCount} hunks</span>
        </div>
        <div class="hunks"></div>`;
      item.querySelector('code').textContent = file.display_label;
      item.querySelector('[data-action="accept-file"]').addEventListener('click', () => mutate(`/api/files/${fileIndex}/accept`, 'Accepted file.').catch(showError));
      item.querySelector('[data-action="reject-file"]').addEventListener('click', () => mutate(`/api/files/${fileIndex}/reject`, 'Rejected file.').catch(showError));
      item.querySelector('[data-action="unreview-file"]').addEventListener('click', () => mutate(`/api/files/${fileIndex}/unreview`, 'Moved file back to unreviewed.').catch(showError));

      const hunks = item.querySelector('.hunks');
      file.hunks.forEach((hunk, hunkIndex) => {
        const row = document.createElement('div');
        row.className = 'hunk';
        row.innerHTML = `
          <code></code>
          <div class="actions">
            <span class="status">${hunk.review_status}</span>
            <button data-action="accept-hunk">Accept hunk</button>
            <button data-action="reject-hunk" class="danger">Reject hunk</button>
          </div>`;
        row.querySelector('code').textContent = hunk.header;
        row.querySelector('[data-action="accept-hunk"]').addEventListener('click', () => mutate(`/api/files/${fileIndex}/hunks/${hunkIndex}/accept`, 'Accepted hunk.').catch(showError));
        row.querySelector('[data-action="reject-hunk"]').addEventListener('click', () => mutate(`/api/files/${fileIndex}/hunks/${hunkIndex}/reject`, 'Rejected hunk.').catch(showError));
        hunks.appendChild(row);
      });
      return item;
    }

    function setStatus(message) { document.getElementById('status').textContent = message; }
    function showError(error) { setStatus(error.message); document.getElementById('json').textContent = error.message; }

    document.getElementById('refresh').addEventListener('click', () => mutate('/api/refresh', 'Refreshed review queue.').catch(showError));
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
      } catch (error) {
        showError(error);
      }
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
