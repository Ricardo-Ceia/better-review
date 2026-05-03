use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use super::review_mutation::{
    accept_review_selection, reject_review_selection, unreview_current_file,
};
use super::{
    App, KeybindingCommand, ReviewFocus, Screen, cancel_current_explain, key_matches,
    move_review_cursor_by_line, open_explain_history, open_explain_menu, open_model_picker,
    open_settings, retry_current_explain, sync_cursor_line_to_hunk,
};

pub(super) async fn handle_review_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.screen != Screen::Review {
        return Ok(());
    }

    if app.review.files.is_empty() {
        return Ok(());
    }

    if app.review_busy {
        match key.code {
            KeyCode::Esc => app.review.focus = ReviewFocus::Files,
            _ => app.status = "Updating review state...".to_string(),
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Enter => {
            app.review.focus = ReviewFocus::Hunks;
            sync_cursor_line_to_hunk(&mut app.review);
        }
        KeyCode::Esc => {
            if app.review.focus == ReviewFocus::Hunks {
                app.review.focus = ReviewFocus::Files;
            } else {
                app.screen = Screen::Home;
                app.status = "Back on the better-review home screen.".to_string();
            }
        }
        KeyCode::Up if app.review.focus == ReviewFocus::Files => {
            app.review.cursor_file = app.review.cursor_file.saturating_sub(1);
            reset_current_hunk(app);
        }
        KeyCode::Up => move_review_cursor_by_line(app, -1),
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            if app.review.focus == ReviewFocus::Files {
                app.review.cursor_file = app.review.cursor_file.saturating_sub(1);
                reset_current_hunk(app);
            } else {
                move_review_cursor_by_line(app, -1);
            }
        }
        KeyCode::Down
            if app.review.focus == ReviewFocus::Files
                && app.review.cursor_file + 1 < app.review.files.len() =>
        {
            app.review.cursor_file += 1;
            reset_current_hunk(app);
        }
        KeyCode::Down => move_review_cursor_by_line(app, 1),
        _ if key_matches(app, key, KeybindingCommand::MoveDown) => {
            if app.review.focus == ReviewFocus::Files {
                if app.review.cursor_file + 1 < app.review.files.len() {
                    app.review.cursor_file += 1;
                    reset_current_hunk(app);
                }
            } else {
                move_review_cursor_by_line(app, 1);
            }
        }
        KeyCode::Tab if app.review.focus == ReviewFocus::Hunks => {
            if let Some(file) = app.review.files.get(app.review.cursor_file)
                && !file.hunks.is_empty()
            {
                app.review.cursor_hunk = (app.review.cursor_hunk + 1) % file.hunks.len();
                sync_cursor_line_to_hunk(&mut app.review);
            }
        }
        _ if key_matches(app, key, KeybindingCommand::Accept) => {
            accept_review_selection(app).await;
        }
        _ if key_matches(app, key, KeybindingCommand::Reject) => {
            reject_review_selection(app).await;
        }
        _ if key_matches(app, key, KeybindingCommand::Unreview) => {
            unreview_current_file(app).await;
        }
        _ if key_matches(app, key, KeybindingCommand::Settings) => open_settings(app),
        _ if key_matches(app, key, KeybindingCommand::Explain) => open_explain_menu(app),
        _ if key_matches(app, key, KeybindingCommand::ExplainHistory) => {
            app.why_this.return_to_menu = false;
            open_explain_history(app)
        }
        _ if key_matches(app, key, KeybindingCommand::ExplainRetry) => {
            retry_current_explain(app).await?
        }
        _ if key_matches(app, key, KeybindingCommand::ExplainCancel) => cancel_current_explain(app),
        _ if key_matches(app, key, KeybindingCommand::ExplainModel) => {
            app.why_this.return_to_menu = false;
            open_model_picker(app).await
        }
        _ => {}
    }

    Ok(())
}

fn reset_current_hunk(app: &mut App) {
    app.review.cursor_hunk = 0;
    app.review.cursor_line = 0;
}
