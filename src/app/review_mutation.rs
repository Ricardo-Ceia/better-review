use crate::domain::diff::{FileDiff, ReviewStatus};

use super::{App, Message, ReviewFocus, sync_cursor_line_to_hunk};

pub(super) fn handle_hunk_sync_result(
    app: &mut App,
    file_index: usize,
    original_file: FileDiff,
    updated_file: FileDiff,
    success_status: String,
    result: Result<(), String>,
) {
    app.review_busy = false;
    if let Some(file) = app.review.files.get_mut(file_index) {
        match result {
            Ok(()) => {
                *file = updated_file;
                sync_cursor_line_to_hunk(&mut app.review);
                app.status = success_status;
            }
            Err(err) => {
                *file = original_file;
                app.status = err;
            }
        }
    }
}

pub(super) async fn accept_review_selection(app: &mut App) {
    if app.review.focus == ReviewFocus::Files {
        if let Some(file) = app.review.files.get_mut(app.review.cursor_file) {
            match app.git.accept_file(file).await {
                Ok(()) => app.status = "Accepted file changes.".to_string(),
                Err(err) => app.status = format!("Could not accept file: {err}"),
            }
        }
    } else if let Some(file) = app.review.files.get_mut(app.review.cursor_file)
        && file.hunks.get(app.review.cursor_hunk).is_some()
    {
        let file_index = app.review.cursor_file;
        let original_file = file.clone();
        let mut updated_file = file.clone();
        updated_file.hunks[app.review.cursor_hunk].review_status = ReviewStatus::Accepted;
        updated_file.sync_review_status();

        let tx = app.tx.clone();
        let git = app.git.clone();
        app.review_busy = true;
        app.status = "Applying accepted hunk...".to_string();

        tokio::spawn(async move {
            let result = git
                .sync_file_hunks_to_index(&updated_file)
                .await
                .map_err(|err| format!("Could not accept hunk: {err}"));
            let _ = tx.send(Message::HunkSync {
                file_index,
                original_file,
                updated_file,
                success_status: "Accepted hunk.".to_string(),
                result,
            });
        });
    }
}

pub(super) async fn reject_review_selection(app: &mut App) {
    if app.review.focus == ReviewFocus::Files {
        if let Some(file) = app.review.files.get_mut(app.review.cursor_file) {
            let result = app.git.reject_file_in_place(file).await;

            match result {
                Ok(()) => app.status = "Rejected file changes.".to_string(),
                Err(err) => app.status = format!("Could not reject file: {err}"),
            }
        }
    } else if let Some(file) = app.review.files.get_mut(app.review.cursor_file)
        && file.hunks.get(app.review.cursor_hunk).is_some()
    {
        let file_index = app.review.cursor_file;
        let original_file = file.clone();
        let mut updated_file = file.clone();
        updated_file.hunks[app.review.cursor_hunk].review_status = ReviewStatus::Rejected;
        updated_file.sync_review_status();

        let tx = app.tx.clone();
        let git = app.git.clone();
        app.review_busy = true;
        app.status = "Rejecting hunk...".to_string();

        tokio::spawn(async move {
            let result = git
                .sync_file_hunks_to_index(&updated_file)
                .await
                .map_err(|err| format!("Could not reject hunk: {err}"));
            let _ = tx.send(Message::HunkSync {
                file_index,
                original_file,
                updated_file,
                success_status: "Rejected hunk.".to_string(),
                result,
            });
        });
    }
}

pub(super) async fn unreview_current_file(app: &mut App) {
    if let Some(file) = app.review.files.get_mut(app.review.cursor_file) {
        let result = app.git.unstage_file_in_place(file).await;

        match result {
            Ok(()) => app.status = "Moved file back to unreviewed.".to_string(),
            Err(err) => app.status = format!("Could not unstage file: {err}"),
        }
    }
}
