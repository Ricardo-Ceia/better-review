use crate::domain::diff::FileDiff;
use crate::services::opencode::{WhyTarget, why_target_for_file, why_target_for_hunk};

use super::App;

#[derive(Default)]
pub(super) struct ReviewUiState {
    pub(super) files: Vec<FileDiff>,
    pub(super) cursor_file: usize,
    pub(super) cursor_hunk: usize,
    pub(super) cursor_line: usize,
    pub(super) focus: ReviewFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ReviewFocus {
    #[default]
    Files,
    Hunks,
}

pub(super) fn review_render_line_count(file: &FileDiff) -> usize {
    crate::ui::review::review_render_line_count(file)
}

pub(super) fn hunk_line_start(file: &FileDiff, hunk_index: usize) -> usize {
    crate::ui::review::hunk_line_start(file, hunk_index)
}

pub(super) fn hunk_index_for_line(file: &FileDiff, line_index: usize) -> usize {
    crate::ui::review::hunk_index_for_line(file, line_index)
}

pub(super) fn sync_cursor_line_to_hunk(review: &mut ReviewUiState) {
    let Some(file) = review.files.get(review.cursor_file) else {
        review.cursor_line = 0;
        return;
    };

    if file.hunks.is_empty() {
        review.cursor_line = 0;
        review.cursor_hunk = 0;
        return;
    }

    review.cursor_hunk = review.cursor_hunk.min(file.hunks.len().saturating_sub(1));
    review.cursor_line = hunk_line_start(file, review.cursor_hunk);
}

pub(super) fn move_review_cursor_by_line(app: &mut App, delta: isize) {
    let Some(file) = app.review.files.get(app.review.cursor_file) else {
        return;
    };

    let max_line = review_render_line_count(file).saturating_sub(1) as isize;
    let next_line = (app.review.cursor_line as isize + delta).clamp(0, max_line) as usize;
    app.review.cursor_line = next_line;

    if !file.hunks.is_empty() {
        app.review.cursor_hunk = hunk_index_for_line(file, next_line);
    }
}

pub(super) fn current_why_target(review: &ReviewUiState) -> Option<(String, WhyTarget)> {
    let file = review.files.get(review.cursor_file)?;
    if review.focus == ReviewFocus::Files || file.hunks.is_empty() {
        let target = why_target_for_file(file);
        let label = target.label();
        return Some((label, target));
    }

    let hunk = file.hunks.get(review.cursor_hunk)?;
    let target = why_target_for_hunk(file, hunk);
    let label = target.label();
    Some((label, target))
}
