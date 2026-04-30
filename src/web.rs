use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::domain::diff::{FileDiff, Hunk, ReviewStatus};
use crate::services::git::GitService;

pub async fn run() -> Result<()> {
    let repo_path = std::env::current_dir().context("failed to resolve current directory")?;
    let token = local_session_token();
    let state = Arc::new(WebState {
        git: GitService::new(&repo_path),
        repo_path,
        token,
    });

    let router = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
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

#[derive(Clone)]
struct WebState {
    git: GitService,
    repo_path: PathBuf,
    token: String,
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
    let (_, files) = state.git.collect_diff().await?;
    Ok(Json(review_state_response(&state.repo_path, files)))
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
    main { max-width: 1120px; margin: 0 auto; padding: 32px; }
    header { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; margin-bottom: 24px; }
    h1 { margin: 0; font-size: 28px; letter-spacing: -0.04em; }
    .muted { color: #94a3b8; }
    .card { background: #111827; border: 1px solid #253044; border-radius: 18px; padding: 20px; box-shadow: 0 24px 80px rgb(0 0 0 / 0.24); }
    .counts { display: flex; gap: 12px; flex-wrap: wrap; }
    .pill { border: 1px solid #334155; border-radius: 999px; padding: 8px 12px; background: #0f172a; }
    .grid { display: grid; grid-template-columns: minmax(280px, 360px) 1fr; gap: 18px; align-items: start; }
    .files { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
    .file { display: grid; grid-template-columns: auto 1fr auto; gap: 10px; align-items: center; padding: 10px 12px; border: 1px solid #263244; border-radius: 12px; background: #0f172a; }
    .icon { color: #60a5fa; font-weight: 800; }
    code { color: #bfdbfe; word-break: break-all; }
    pre { overflow: auto; margin: 0; white-space: pre-wrap; color: #cbd5e1; }
    button { cursor: pointer; border: 1px solid #334155; color: #e5e7eb; background: #1e293b; padding: 8px 12px; border-radius: 10px; }
    button:hover { background: #334155; }
    @media (max-width: 860px) { .grid { grid-template-columns: 1fr; } header { flex-direction: column; } }
  </style>
</head>
<body>
  <main>
    <header>
      <div>
        <h1>better-review web</h1>
        <p class="muted">Local browser review mode skeleton. Git state is served from the Rust review engine.</p>
      </div>
      <button id="refresh">Refresh</button>
    </header>

    <section class="card" style="margin-bottom: 18px;">
      <div class="muted">Repository</div>
      <code id="repo">Loading…</code>
      <div class="counts" style="margin-top: 14px;">
        <span class="pill"><strong id="pending">0</strong> pending</span>
        <span class="pill"><strong id="accepted">0</strong> accepted</span>
        <span class="pill"><strong id="rejected">0</strong> rejected</span>
      </div>
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

  <script>
    const token = new URLSearchParams(location.search).get('token');
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

    async function loadState() {
      const response = await fetch(`/api/state?token=${encodeURIComponent(token || '')}`);
      if (!response.ok) throw new Error(await response.text());
      const state = await response.json();
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
      for (const file of state.files) {
        const item = document.createElement('li');
        item.className = 'file';
        item.innerHTML = `<span class="icon">${iconFor(file)}</span><code></code><span class="muted">${file.hunks.length} hunks</span>`;
        item.querySelector('code').textContent = file.display_label;
        files.appendChild(item);
      }
    }

    document.getElementById('refresh').addEventListener('click', () => loadState().catch(showError));
    function showError(error) { document.getElementById('json').textContent = error.message; }
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
