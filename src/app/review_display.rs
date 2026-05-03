use ratatui::text::Span;
use ratatui_core::style::{Color, Modifier, Style};

use crate::domain::diff::{DiffLineKind, FileDiff, FileStatus, ReviewStatus};
use crate::ui::styles;

pub(super) fn sidebar_file_label_parts(file: &FileDiff) -> (String, String, String) {
    match file.status {
        FileStatus::Renamed | FileStatus::Copied | FileStatus::ModeChanged => {
            ("• ".to_string(), String::new(), file.display_label())
        }
        _ => tree_sidebar_parts(file.display_path()),
    }
}

fn tree_sidebar_parts(path: &str) -> (String, String, String) {
    let mut parts = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return ("• ".to_string(), String::new(), path.to_string());
    }

    let leaf = parts.pop().unwrap_or_default().to_string();
    let depth = parts.len();
    let tree_prefix = if depth == 0 {
        "• ".to_string()
    } else {
        format!("{}└─ ", "  ".repeat(depth.saturating_sub(1)))
    };
    let parent = if parts.is_empty() {
        String::new()
    } else {
        format!("{}/", parts.join("/"))
    };

    (tree_prefix, parent, leaf)
}

pub(super) fn review_marker(
    status: ReviewStatus,
    file_status: crate::domain::diff::FileStatus,
    is_hunk: bool,
) -> &'static str {
    match status {
        ReviewStatus::Accepted => "[✓]",
        ReviewStatus::Rejected => "[x]",
        ReviewStatus::Unreviewed if is_hunk => "[ ]",
        ReviewStatus::Unreviewed => match file_status {
            crate::domain::diff::FileStatus::Added => "[+]",
            crate::domain::diff::FileStatus::Deleted => "[-]",
            crate::domain::diff::FileStatus::Renamed => "[→]",
            crate::domain::diff::FileStatus::Copied => "[⧉]",
            crate::domain::diff::FileStatus::ModeChanged => "[m]",
            crate::domain::diff::FileStatus::Modified => "[ ]",
        },
    }
}

pub(super) fn truncate_path(path: &str, max_len: usize) -> String {
    if path.chars().count() <= max_len {
        return path.to_string();
    }
    let suffix = path
        .chars()
        .rev()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    format!("...{}", suffix.chars().rev().collect::<String>())
}

#[cfg(test)]
pub(super) fn line_number_style(kind: DiffLineKind) -> Style {
    line_number_style_for(kind, false)
}

pub(super) fn line_number_style_for(kind: DiffLineKind, is_current_line: bool) -> Style {
    match kind {
        DiffLineKind::Add | DiffLineKind::Remove => Style::default()
            .fg(diff_change_bar_color(kind))
            .bg(if is_current_line {
                diff_line_bg(kind, true)
            } else {
                styles::surface_raised()
            }),
        DiffLineKind::Context => styles::subtle().bg(if is_current_line {
            diff_line_bg(kind, true)
        } else {
            styles::surface_raised()
        }),
    }
}

#[cfg(test)]
pub(super) fn diff_change_bar(kind: DiffLineKind) -> &'static str {
    diff_change_bar_for(kind, false)
}

pub(super) fn diff_change_bar_for(kind: DiffLineKind, is_current_hunk: bool) -> &'static str {
    match kind {
        DiffLineKind::Add | DiffLineKind::Remove => "▌",
        DiffLineKind::Context if is_current_hunk => "▌",
        DiffLineKind::Context => " ",
    }
}

#[cfg(test)]
pub(super) fn diff_change_bar_style(kind: DiffLineKind) -> Style {
    diff_change_bar_style_for(kind, false, false)
}

pub(super) fn diff_change_bar_style_for(
    kind: DiffLineKind,
    is_current_hunk: bool,
    is_current_line: bool,
) -> Style {
    let bg = diff_line_bg(kind, is_current_line);
    match kind {
        DiffLineKind::Add | DiffLineKind::Remove => Style::default()
            .fg(diff_change_bar_color(kind))
            .bg(bg)
            .add_modifier(Modifier::BOLD),
        DiffLineKind::Context if is_current_hunk => Style::default()
            .fg(styles::accent_bright_color())
            .bg(bg)
            .add_modifier(Modifier::BOLD),
        DiffLineKind::Context => Style::default().fg(bg).bg(bg),
    }
}

pub(super) fn diff_change_bar_color(kind: DiffLineKind) -> Color {
    match kind {
        DiffLineKind::Add => Color::Indexed(40),
        DiffLineKind::Remove => Color::Indexed(160),
        DiffLineKind::Context => styles::border_muted(),
    }
}

pub(super) fn diff_row_bg(kind: DiffLineKind) -> Color {
    match kind {
        DiffLineKind::Add => Color::Indexed(22),
        DiffLineKind::Remove => Color::Indexed(52),
        DiffLineKind::Context => styles::surface(),
    }
}

pub(super) fn diff_current_line_bg(kind: DiffLineKind) -> Color {
    match kind {
        DiffLineKind::Add => Color::Indexed(28),
        DiffLineKind::Remove => Color::Indexed(88),
        DiffLineKind::Context => styles::accent_dim(),
    }
}

pub(super) fn diff_line_bg(kind: DiffLineKind, is_current_line: bool) -> Color {
    if is_current_line {
        diff_current_line_bg(kind)
    } else {
        diff_row_bg(kind)
    }
}

#[cfg(test)]
pub(super) fn diff_marker_style(kind: DiffLineKind) -> Style {
    diff_marker_style_for(kind, false)
}

pub(super) fn diff_marker_style_for(kind: DiffLineKind, is_current_line: bool) -> Style {
    match kind {
        DiffLineKind::Add | DiffLineKind::Remove => Style::default()
            .fg(diff_change_bar_color(kind))
            .bg(diff_line_bg(kind, is_current_line))
            .add_modifier(Modifier::BOLD),
        DiffLineKind::Context => Style::default()
            .fg(styles::text_muted())
            .bg(diff_line_bg(kind, is_current_line)),
    }
}

pub(super) fn diff_content_style(kind: DiffLineKind) -> Style {
    match kind {
        DiffLineKind::Add => Style::default()
            .fg(styles::syntax_string())
            .bg(diff_row_bg(kind)),
        DiffLineKind::Remove => Style::default()
            .fg(styles::text_primary())
            .bg(diff_row_bg(kind)),
        DiffLineKind::Context => Style::default()
            .fg(styles::text_muted())
            .bg(diff_row_bg(kind)),
    }
}

pub(super) fn diff_row_style_for(kind: DiffLineKind, is_current_line: bool) -> Style {
    match kind {
        DiffLineKind::Add => Style::default()
            .fg(styles::syntax_string())
            .bg(diff_line_bg(kind, is_current_line)),
        DiffLineKind::Remove => Style::default()
            .fg(styles::text_primary())
            .bg(diff_line_bg(kind, is_current_line)),
        DiffLineKind::Context => Style::default()
            .fg(styles::text_muted())
            .bg(diff_line_bg(kind, is_current_line)),
    }
}

pub(super) fn apply_bg_to_spans(spans: &mut [Span<'static>], bg: Color) {
    for span in spans {
        span.style = span.style.bg(bg);
    }
}
