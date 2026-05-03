use crossterm::event::{KeyCode, KeyEvent};
use ratatui_core::style::Style;
use tokio::task::JoinHandle;

use crate::services::opencode::{WhyAnswer, WhyTarget};
use crate::ui::styles;

use super::{
    App, KeybindingCommand, Overlay, WhyThisUiState, close_explain_submenu, key_matches,
    key_status_label,
};

pub(super) struct ExplainRun {
    pub(super) id: u64,
    pub(super) label: String,
    pub(super) target: WhyTarget,
    pub(super) context_source_id: String,
    pub(super) context_source_label: String,
    pub(super) requested_model: Option<String>,
    pub(super) model_label: String,
    pub(super) cache_key: String,
    pub(super) status: ExplainRunStatus,
    pub(super) result: Option<WhyAnswer>,
    pub(super) error: Option<String>,
    pub(super) handle: Option<JoinHandle<()>>,
}

pub(super) enum ExplainRunStatus {
    Running,
    Ready,
    Failed,
    Cancelled,
}

pub(super) fn handle_explain_history_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            close_explain_submenu(app, "Closed Explain history.");
        }
        KeyCode::Up => move_explain_history_cursor(app, -1),
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            move_explain_history_cursor(app, -1)
        }
        KeyCode::Down => move_explain_history_cursor(app, 1),
        _ if key_matches(app, key, KeybindingCommand::MoveDown) => {
            move_explain_history_cursor(app, 1)
        }
        KeyCode::Enter => focus_history_run(app),
        _ if key_matches(app, key, KeybindingCommand::ExplainRetry) => retry_history_run(app),
        _ if key_matches(app, key, KeybindingCommand::ExplainCancel) => cancel_history_run(app),
        KeyCode::Backspace | KeyCode::Delete => clear_history_run(app),
        _ => {}
    }
}

pub(super) fn next_explain_run_id(why_this: &mut WhyThisUiState) -> u64 {
    why_this.next_run_id = why_this.next_run_id.saturating_add(1);
    why_this.next_run_id
}

pub(super) fn find_explain_run_index_by_id(
    why_this: &WhyThisUiState,
    run_id: u64,
) -> Option<usize> {
    why_this.runs.iter().position(|run| run.id == run_id)
}

pub(super) fn find_reusable_explain_run_index(
    why_this: &WhyThisUiState,
    cache_key: &str,
) -> Option<usize> {
    why_this.runs.iter().position(|run| {
        run.cache_key == cache_key
            && matches!(
                run.status,
                ExplainRunStatus::Running | ExplainRunStatus::Ready
            )
    })
}

#[cfg(test)]
pub(super) fn current_explain_run(app: &App) -> Option<&ExplainRun> {
    let run_id = app.why_this.current_run_id?;
    app.why_this.runs.iter().find(|run| run.id == run_id)
}

pub(super) fn selected_history_run(app: &App) -> Option<&ExplainRun> {
    app.why_this.runs.get(app.why_this.history_cursor)
}

pub(super) fn move_explain_history_cursor(app: &mut App, delta: isize) {
    if app.why_this.runs.is_empty() {
        app.status = "No explain runs yet.".to_string();
        return;
    }

    let len = app.why_this.runs.len() as isize;
    let current = app.why_this.history_cursor as isize;
    let next = (current + delta).rem_euclid(len) as usize;
    app.why_this.history_cursor = next;
    if let Some(run) = app.why_this.runs.get(next) {
        app.status = format!("Selected explain run #{}.", run.id);
    }
}

fn focus_history_run(app: &mut App) {
    let Some(run_id) = selected_history_run(app).map(|run| run.id) else {
        app.status = "No explain run selected.".to_string();
        return;
    };

    app.why_this.current_run_id = Some(run_id);
    app.overlay = Overlay::None;
    app.why_this.return_to_menu = false;
    app.status = format!("Focused explain run #{}.", run_id);
}

fn cancel_run_by_index(app: &mut App, index: usize) {
    let Some(run) = app.why_this.runs.get_mut(index) else {
        app.status = "Selected explain run no longer exists.".to_string();
        return;
    };

    if !matches!(run.status, ExplainRunStatus::Running) {
        app.status = format!("Explain run #{} is not running.", run.id);
        return;
    };

    if let Some(handle) = run.handle.take() {
        handle.abort();
    }
    run.status = ExplainRunStatus::Cancelled;
    run.error = None;
    app.status = format!("Cancelled explain run #{}.", run.id);
}

pub(super) fn cancel_current_explain(app: &mut App) {
    let Some(run_id) = app.why_this.current_run_id else {
        app.status = "No current explain run.".to_string();
        return;
    };

    if let Some(index) = find_explain_run_index_by_id(&app.why_this, run_id) {
        cancel_run_by_index(app, index);
    }
}

fn cancel_history_run(app: &mut App) {
    cancel_run_by_index(app, app.why_this.history_cursor);
}

fn clear_run_by_index(app: &mut App, index: usize) {
    let Some(run) = app.why_this.runs.get(index) else {
        app.status = "Selected explain run no longer exists.".to_string();
        return;
    };

    if matches!(run.status, ExplainRunStatus::Running) {
        app.status = format!(
            "Explain run #{} is still running. Press {} to cancel it.",
            run.id,
            key_status_label(app, KeybindingCommand::ExplainCancel)
        );
        return;
    }

    let removed = app.why_this.runs.remove(index);
    if app.why_this.current_run_id == Some(removed.id) {
        app.why_this.current_run_id = app.why_this.runs.last().map(|run| run.id);
    }
    if app.why_this.runs.is_empty() {
        app.why_this.history_cursor = 0;
        if app.overlay == Overlay::ExplainHistory {
            app.overlay = Overlay::None;
        }
    } else {
        app.why_this.history_cursor = index.min(app.why_this.runs.len().saturating_sub(1));
    }
    app.status = format!("Cleared explain run #{}.", removed.id);
}

pub(super) fn clear_history_run(app: &mut App) {
    clear_run_by_index(app, app.why_this.history_cursor);
}

pub(super) fn open_explain_history(app: &mut App) {
    app.overlay = Overlay::ExplainHistory;
    app.status = "Opened Explain history.".to_string();
}

fn retry_history_run(app: &mut App) {
    if let Some(run_id) = selected_history_run(app).map(|run| run.id) {
        app.why_this.current_run_id = Some(run_id);
        app.status = format!("Focused explain run #{} for retry.", run_id);
    }
}

pub(super) fn explain_run_status_label(status: &ExplainRunStatus) -> &'static str {
    match status {
        ExplainRunStatus::Running => "running",
        ExplainRunStatus::Ready => "ready",
        ExplainRunStatus::Failed => "failed",
        ExplainRunStatus::Cancelled => "cancelled",
    }
}

pub(super) fn explain_run_status_style(status: &ExplainRunStatus) -> Style {
    match status {
        ExplainRunStatus::Running => styles::accent_bold(),
        ExplainRunStatus::Ready => Style::default().fg(styles::success()),
        ExplainRunStatus::Failed => Style::default().fg(styles::danger()),
        ExplainRunStatus::Cancelled => styles::muted(),
    }
}
