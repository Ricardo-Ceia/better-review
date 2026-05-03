use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui_core::style::{Color, Modifier, Style};

use crate::domain::diff::{DiffLineKind, FileDiff, FileStatus, Hunk, ReviewStatus};
use crate::ui::review::{ReviewRenderRow, ReviewRenderSideLine, build_review_render_rows};
use crate::ui::styles;

use super::review_display::{
    apply_bg_to_spans, diff_change_bar_color, diff_change_bar_for, diff_change_bar_style_for,
    diff_content_style, diff_line_bg, diff_marker_style_for, diff_row_style_for,
    line_number_style_for, review_marker, sidebar_file_label_parts, truncate_path,
};
use super::{
    App, HOME_DEFAULT_STATUS, KeybindingCommand, ReviewFocus, centered_rect, key_for, key_label,
};

pub(super) fn draw_review(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    styles::set_palette(app.palette);
    frame.render_widget(
        Block::default().style(Style::default().bg(styles::base_bg())),
        area,
    );

    if app.review.files.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(Span::raw("")),
            Line::from(Span::raw("")),
            Line::from(Span::styled("No code changes yet", styles::title())),
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                "Run your coding agent in another pane or window, then come back here to review.",
                styles::muted(),
            )),
            Line::from(Span::styled(
                "Relaunch better-review after your agent finishes to load new changes.",
                styles::muted(),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(styles::border_muted())),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(empty, centered_rect(78, 38, area));
        return;
    }

    let canvas = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(3)])
        .split(canvas);
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(40)])
        .split(layout[0]);

    draw_review_sidebar(frame, sections[0], app);
    if let Some(file) = app.review.files.get(app.review.cursor_file) {
        draw_review_diff(frame, sections[1], app, file);
        draw_review_footer(frame, layout[1], app, file);
    }
}

fn draw_review_sidebar(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let counts = app.review_counts();
    let title_style = if app.review.focus == ReviewFocus::Files {
        styles::accent_bold()
    } else {
        styles::title()
    };

    let items = app
        .review
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let selected = index == app.review.cursor_file;
            let row_bg = if selected {
                styles::accent_dim()
            } else {
                styles::surface()
            };
            let row_style = if selected {
                Style::default()
                    .fg(styles::text_primary())
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(styles::text_muted()).bg(row_bg)
            };

            let marker = review_marker(file.review_status.clone(), file.status, false);
            let selection_bar = if selected { "▌" } else { " " };
            let (tree_prefix, parent, leaf) = sidebar_file_label_parts(file);
            let (added, removed) = file_line_stats(file);
            let status_icon = file_sidebar_icon(file);
            let status_style = file_sidebar_icon_style(file, row_bg);

            ListItem::new(Line::from(vec![
                Span::styled(
                    selection_bar,
                    Style::default()
                        .fg(styles::accent_bright_color())
                        .bg(row_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {marker} "), row_style),
                Span::styled(format!("{status_icon} "), status_style),
                Span::styled(tree_prefix, styles::subtle().bg(row_bg)),
                Span::styled(
                    parent,
                    Style::default().fg(styles::syntax_comment()).bg(row_bg),
                ),
                Span::styled(truncate_path(&leaf, 16), row_style),
                Span::styled("  ", row_style),
                Span::styled(
                    format!("+{added}"),
                    Style::default()
                        .fg(diff_change_bar_color(DiffLineKind::Add))
                        .bg(row_bg),
                ),
                Span::styled(" ", row_style),
                Span::styled(
                    format!("-{removed}"),
                    Style::default()
                        .fg(diff_change_bar_color(DiffLineKind::Remove))
                        .bg(row_bg),
                ),
            ]))
        })
        .collect::<Vec<_>>();

    let mut sidebar_state = ListState::default().with_selected(Some(app.review.cursor_file));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(Line::from(vec![
                        Span::styled(" [1] ", title_style),
                        Span::styled("Files", styles::title()),
                        Span::styled(
                            format!(
                                "  {} pending  {} accepted  {} rejected",
                                counts.unreviewed, counts.accepted, counts.rejected
                            ),
                            styles::subtle(),
                        ),
                    ]))
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.review.focus == ReviewFocus::Files {
                            styles::accent_bright_color()
                        } else {
                            styles::border_muted()
                        }),
                    )
                    .style(Style::default().bg(styles::surface())),
            )
            .style(Style::default().bg(styles::surface())),
        area,
        &mut sidebar_state,
    );
}

fn draw_review_diff(frame: &mut ratatui::Frame, area: Rect, app: &App, file: &FileDiff) {
    if file.is_binary {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::raw("")),
                Line::from(Span::styled("Binary file", styles::title())),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "This change cannot be shown as a text diff.",
                    styles::muted(),
                )),
            ])
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(Line::from(Span::styled(
                        format!(" {} ", file.display_label()),
                        styles::title(),
                    )))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(styles::border_muted()))
                    .style(Style::default().bg(styles::surface())),
            ),
            area,
        );
        return;
    }

    if file.hunks.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::raw("")),
                Line::from(Span::styled("No text hunks", styles::title())),
                Line::from(Span::raw("")),
                Line::from(Span::styled(
                    "This file changed, but there is no patch body to render.",
                    styles::muted(),
                )),
            ])
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(Line::from(Span::styled(
                        format!(" {} ", file.display_label()),
                        styles::title(),
                    )))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(styles::border_muted()))
                    .style(Style::default().bg(styles::surface())),
            ),
            area,
        );
        return;
    }

    let rows = build_review_render_rows(file);
    let diff_focus_style = if app.review.focus == ReviewFocus::Hunks {
        Style::default().fg(styles::accent_bright_color())
    } else {
        Style::default().fg(styles::border_muted())
    };

    match file.status {
        FileStatus::Added => {
            let (added, removed) = file_line_stats(file);
            let lines = render_review_panel_lines(app, file, &rows, None, area.width);
            let scroll = diff_scroll_offset(app, area, &lines);
            frame.render_widget(
                Paragraph::new(lines)
                    .scroll((scroll, 0))
                    .block(
                        Block::default()
                            .title(Line::from(vec![
                                Span::styled(" [2] ", styles::accent_bold()),
                                Span::styled(file.display_label(), styles::title()),
                                Span::styled("  new file  ", styles::subtle()),
                                Span::styled(
                                    format!("+{added}"),
                                    Style::default().fg(diff_change_bar_color(DiffLineKind::Add)),
                                ),
                                Span::styled(
                                    format!(" -{removed}"),
                                    Style::default()
                                        .fg(diff_change_bar_color(DiffLineKind::Remove)),
                                ),
                            ]))
                            .borders(Borders::ALL)
                            .border_style(diff_focus_style)
                            .style(Style::default().bg(styles::surface())),
                    )
                    .style(Style::default().bg(styles::surface())),
                area,
            );
        }
        FileStatus::Deleted => {
            let (added, removed) = file_line_stats(file);
            let lines = render_review_panel_lines(app, file, &rows, Some(true), area.width);
            let scroll = diff_scroll_offset(app, area, &lines);
            frame.render_widget(
                Paragraph::new(lines)
                    .scroll((scroll, 0))
                    .block(
                        Block::default()
                            .title(Line::from(vec![
                                Span::styled(" [2] ", styles::accent_bold()),
                                Span::styled(file.display_label(), styles::title()),
                                Span::styled("  deleted file  ", styles::subtle()),
                                Span::styled(
                                    format!("+{added}"),
                                    Style::default().fg(diff_change_bar_color(DiffLineKind::Add)),
                                ),
                                Span::styled(
                                    format!(" -{removed}"),
                                    Style::default()
                                        .fg(diff_change_bar_color(DiffLineKind::Remove)),
                                ),
                            ]))
                            .borders(Borders::ALL)
                            .border_style(diff_focus_style)
                            .style(Style::default().bg(styles::surface())),
                    )
                    .style(Style::default().bg(styles::surface())),
                area,
            );
        }
        FileStatus::Modified
        | FileStatus::Renamed
        | FileStatus::Copied
        | FileStatus::ModeChanged => {
            let (added, removed) = file_line_stats(file);
            let lines = render_review_unified_lines(app, file, area.width.saturating_sub(2));
            let scroll = diff_scroll_offset(app, area, &lines);
            let status_label = file_status_panel_label(file.status);

            frame.render_widget(
                Paragraph::new(lines)
                    .scroll((scroll, 0))
                    .block(
                        Block::default()
                            .title(Line::from(vec![
                                Span::styled(" [2] ", styles::accent_bold()),
                                Span::styled(file.display_label(), styles::title()),
                                Span::styled(format!("  {status_label}  "), styles::subtle()),
                                Span::styled(
                                    format!("+{added}"),
                                    Style::default().fg(diff_change_bar_color(DiffLineKind::Add)),
                                ),
                                Span::styled(
                                    format!(" -{removed}"),
                                    Style::default()
                                        .fg(diff_change_bar_color(DiffLineKind::Remove)),
                                ),
                            ]))
                            .borders(Borders::ALL)
                            .border_style(diff_focus_style)
                            .style(Style::default().bg(styles::surface())),
                    )
                    .style(Style::default().bg(styles::surface())),
                area,
            );
        }
    }
}

pub(super) fn file_sidebar_icon(file: &FileDiff) -> &'static str {
    if file.is_binary {
        return "◈";
    }

    match file.status {
        FileStatus::Added => "+",
        FileStatus::Deleted => "−",
        FileStatus::Renamed => "→",
        FileStatus::Copied => "⧉",
        FileStatus::ModeChanged => "⚙",
        FileStatus::Modified if file.hunks.is_empty() => "○",
        FileStatus::Modified => "✎",
    }
}

fn file_sidebar_icon_style(file: &FileDiff, bg: Color) -> Style {
    let fg = if file.is_binary {
        styles::syntax_string()
    } else {
        match file.status {
            FileStatus::Added => diff_change_bar_color(DiffLineKind::Add),
            FileStatus::Deleted => diff_change_bar_color(DiffLineKind::Remove),
            FileStatus::Renamed | FileStatus::Copied | FileStatus::ModeChanged => {
                styles::accent_bright_color()
            }
            FileStatus::Modified if file.hunks.is_empty() => styles::text_muted(),
            FileStatus::Modified => styles::accent_bright_color(),
        }
    };

    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
}

pub(super) fn file_status_panel_label(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "new file",
        FileStatus::Deleted => "deleted file",
        FileStatus::Renamed => "renamed",
        FileStatus::Copied => "copied",
        FileStatus::ModeChanged => "metadata",
        FileStatus::Modified => "unified",
    }
}

fn file_line_stats(file: &FileDiff) -> (usize, usize) {
    file.hunks.iter().fold((0, 0), |(added, removed), hunk| {
        hunk.lines
            .iter()
            .fold((added, removed), |(added, removed), line| match line.kind {
                DiffLineKind::Add => (added + 1, removed),
                DiffLineKind::Remove => (added, removed + 1),
                DiffLineKind::Context => (added, removed),
            })
    })
}

fn hunk_line_stats(hunk: &Hunk) -> (usize, usize) {
    hunk.lines
        .iter()
        .fold((0, 0), |(added, removed), line| match line.kind {
            DiffLineKind::Add => (added + 1, removed),
            DiffLineKind::Remove => (added, removed + 1),
            DiffLineKind::Context => (added, removed),
        })
}

fn review_hunk_header_line(
    file: &FileDiff,
    hunk: &Hunk,
    hunk_index: usize,
    is_current_hunk: bool,
    is_current_line: bool,
) -> Line<'static> {
    let (added, removed) = hunk_line_stats(hunk);
    let mut header_style = Style::default()
        .fg(styles::syntax_function())
        .bg(if is_current_hunk {
            styles::accent_dim()
        } else {
            styles::surface_raised()
        });
    if is_current_hunk {
        header_style = header_style.add_modifier(Modifier::BOLD);
    }
    if is_current_line {
        header_style = header_style.add_modifier(Modifier::BOLD);
    }

    Line::from(vec![
        Span::styled(if is_current_hunk { "▌ " } else { "  " }, header_style),
        Span::styled(
            format!(
                "{} Hunk {} ",
                review_marker(hunk.review_status.clone(), file.status, true),
                hunk_index + 1
            ),
            header_style,
        ),
        Span::styled(hunk.header.clone(), header_style),
        Span::styled("  ", header_style),
        review_hunk_status_span(&hunk.review_status),
        Span::styled("  ", header_style),
        Span::styled(
            format!("+{added}"),
            Style::default()
                .fg(diff_change_bar_color(DiffLineKind::Add))
                .bg(if is_current_hunk {
                    styles::accent_dim()
                } else {
                    styles::surface_raised()
                }),
        ),
        Span::styled(" ", header_style),
        Span::styled(
            format!("-{removed}"),
            Style::default()
                .fg(diff_change_bar_color(DiffLineKind::Remove))
                .bg(if is_current_hunk {
                    styles::accent_dim()
                } else {
                    styles::surface_raised()
                }),
        ),
    ])
}

pub(super) fn render_review_unified_lines(
    app: &App,
    file: &FileDiff,
    content_width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut logical_line = 1;

    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        let is_current_hunk =
            app.review.focus == ReviewFocus::Hunks && app.review.cursor_hunk == hunk_index;
        let is_current_line =
            app.review.focus == ReviewFocus::Hunks && app.review.cursor_line == logical_line;
        lines.push(review_hunk_header_line(
            file,
            hunk,
            hunk_index,
            is_current_hunk,
            is_current_line,
        ));
        logical_line += 1;

        for line in &hunk.lines {
            let is_current_line =
                app.review.focus == ReviewFocus::Hunks && app.review.cursor_line == logical_line;
            let old = line
                .old_line
                .map(|n| format!("{n:>4}"))
                .unwrap_or_else(|| "    ".to_string());
            let new = line
                .new_line
                .map(|n| format!("{n:>4}"))
                .unwrap_or_else(|| "    ".to_string());
            let marker = match line.kind {
                DiffLineKind::Add => "+",
                DiffLineKind::Remove => "-",
                DiffLineKind::Context => " ",
            };

            let mut spans = vec![
                Span::styled(
                    format!("{old} {new} "),
                    line_number_style_for(line.kind, is_current_line),
                ),
                Span::styled(
                    diff_change_bar_for(line.kind, is_current_hunk),
                    diff_change_bar_style_for(line.kind, is_current_hunk, is_current_line),
                ),
                Span::styled(marker, diff_marker_style_for(line.kind, is_current_line)),
                Span::styled(" ", diff_row_style_for(line.kind, is_current_line)),
            ];
            let mut content_spans =
                syntax_highlight_diff_content(&line.content, line.kind, Modifier::empty());
            apply_bg_to_spans(&mut content_spans, diff_line_bg(line.kind, is_current_line));
            spans.extend(content_spans);
            fill_diff_row_background(&mut spans, line.kind, content_width, is_current_line);
            lines.push(Line::from(spans));
            logical_line += 1;
        }

        lines.push(Line::from(Span::raw("")));
        logical_line += 1;
    }

    lines
}

fn render_review_panel_lines(
    app: &App,
    file: &FileDiff,
    rows: &[ReviewRenderRow],
    old_panel: Option<bool>,
    panel_width: u16,
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let logical_line = index + 1;
            match row {
                ReviewRenderRow::HunkHeader {
                    hunk_index,
                    header,
                    status,
                } => {
                    let is_current_hunk = app.review.focus == ReviewFocus::Hunks
                        && app.review.cursor_hunk == *hunk_index;
                    let is_current_line = app.review.focus == ReviewFocus::Hunks
                        && app.review.cursor_line == logical_line;
                    if let Some(hunk) = file.hunks.get(*hunk_index) {
                        review_hunk_header_line(
                            file,
                            hunk,
                            *hunk_index,
                            is_current_hunk,
                            is_current_line,
                        )
                    } else {
                        Line::from(vec![
                            Span::styled(header.clone(), styles::muted()),
                            review_hunk_status_span(status),
                        ])
                    }
                }
                ReviewRenderRow::Diff {
                    hunk_index,
                    old,
                    new,
                } => {
                    let is_current_hunk = app.review.focus == ReviewFocus::Hunks
                        && app.review.cursor_hunk == *hunk_index;
                    let is_current_line = app.review.focus == ReviewFocus::Hunks
                        && app.review.cursor_line == logical_line;
                    let side = match old_panel {
                        Some(true) => old.as_ref(),
                        Some(false) => new.as_ref(),
                        None => new.as_ref(),
                    };
                    Line::from(render_review_side_spans(
                        side,
                        panel_width,
                        is_current_hunk,
                        is_current_line,
                    ))
                }
                ReviewRenderRow::Spacer => Line::from(Span::raw("")),
            }
        })
        .collect()
}

fn render_review_side_spans(
    line: Option<&ReviewRenderSideLine>,
    panel_width: u16,
    is_current_hunk: bool,
    is_current_line: bool,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    let Some(line) = line else {
        let placeholder_width = usize::from(panel_width.saturating_sub(8)).clamp(4, 64);
        spans.push(Span::styled("     ", styles::subtle()));
        spans.push(Span::styled("  ", styles::subtle()));
        spans.push(Span::styled(
            "╱".repeat(placeholder_width),
            styles::subtle(),
        ));
        return spans;
    };

    let prefix = line
        .line_number
        .map(|number| format!("{number:>4} "))
        .unwrap_or_else(|| "     ".to_string());
    let marker = match line.kind {
        DiffLineKind::Add => "+",
        DiffLineKind::Remove => "-",
        DiffLineKind::Context => " ",
    };

    spans.push(Span::styled(
        prefix,
        line_number_style_for(line.kind, is_current_line),
    ));
    spans.push(Span::styled(
        diff_change_bar_for(line.kind, is_current_hunk),
        diff_change_bar_style_for(line.kind, is_current_hunk, is_current_line),
    ));
    spans.push(Span::styled(
        marker,
        diff_marker_style_for(line.kind, is_current_line),
    ));
    spans.push(Span::styled(
        " ",
        diff_row_style_for(line.kind, is_current_line),
    ));
    let mut content_spans =
        syntax_highlight_diff_content(&line.content, line.kind, Modifier::empty());
    apply_bg_to_spans(&mut content_spans, diff_line_bg(line.kind, is_current_line));
    spans.extend(content_spans);
    fill_diff_row_background(&mut spans, line.kind, panel_width, is_current_line);
    spans
}

fn fill_diff_row_background(
    spans: &mut Vec<Span<'static>>,
    kind: DiffLineKind,
    width: u16,
    is_current_line: bool,
) {
    let used = spans_width(spans);
    let target = usize::from(width);
    if used < target {
        spans.push(Span::styled(
            " ".repeat(target - used),
            diff_row_style_for(kind, is_current_line),
        ));
    }
}

pub(super) fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

fn review_hunk_status_span(status: &ReviewStatus) -> Span<'static> {
    match status {
        ReviewStatus::Accepted => Span::styled(
            " [accepted]",
            Style::default()
                .fg(styles::success())
                .bg(styles::code_add_bg()),
        ),
        ReviewStatus::Rejected => Span::styled(
            " [rejected]",
            Style::default()
                .fg(styles::danger())
                .bg(styles::code_remove_bg()),
        ),
        ReviewStatus::Unreviewed => Span::styled(" [unreviewed]", styles::muted()),
    }
}

fn draw_review_footer(frame: &mut ratatui::Frame, area: Rect, app: &App, file: &FileDiff) {
    let counts = app.review_counts();
    frame.render_widget(
        Block::default().style(Style::default().bg(styles::surface_raised())),
        area,
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let (added, removed) = file_line_stats(file);
    let focus_label = if app.review.focus == ReviewFocus::Files {
        "file".to_string()
    } else {
        format!(
            "hunk {}/{}",
            app.review.cursor_hunk.saturating_add(1),
            file.hunks.len().max(1)
        )
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(
                    " {} / {} ",
                    app.review.cursor_file + 1,
                    app.review.files.len()
                ),
                Style::default()
                    .fg(styles::text_primary())
                    .bg(styles::accent_dim())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                truncate_path(&file.display_label(), 48),
                Style::default()
                    .fg(styles::syntax_variable())
                    .bg(styles::surface_raised())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {focus_label}"),
                styles::subtle().bg(styles::surface_raised()),
            ),
            Span::styled("  ", Style::default().bg(styles::surface_raised())),
            Span::styled(
                format!("+{added}"),
                Style::default()
                    .fg(diff_change_bar_color(DiffLineKind::Add))
                    .bg(styles::surface_raised())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default().bg(styles::surface_raised())),
            Span::styled(
                format!("-{removed}"),
                Style::default()
                    .fg(diff_change_bar_color(DiffLineKind::Remove))
                    .bg(styles::surface_raised())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {} pending  {} accepted  {} rejected",
                    counts.unreviewed, counts.accepted, counts.rejected
                ),
                styles::muted().bg(styles::surface_raised()),
            ),
        ])),
        rows[0],
    );

    let mut command_spans = Vec::new();
    append_footer_key(&mut command_spans, "Enter", "hunks");
    append_footer_key(&mut command_spans, "Tab", "next");
    append_footer_key(
        &mut command_spans,
        &key_label(key_for(app, KeybindingCommand::Accept)),
        "accept",
    );
    append_footer_key(
        &mut command_spans,
        &key_label(key_for(app, KeybindingCommand::Reject)),
        "reject",
    );
    append_footer_key(
        &mut command_spans,
        &key_label(key_for(app, KeybindingCommand::Unreview)),
        "unreview",
    );
    append_footer_key(
        &mut command_spans,
        &key_label(key_for(app, KeybindingCommand::Explain)),
        "explain",
    );
    append_footer_key(
        &mut command_spans,
        &key_label(key_for(app, KeybindingCommand::Commit)),
        "commit",
    );
    append_footer_key(&mut command_spans, "Ctrl+P", "commands");
    append_footer_key(&mut command_spans, "Esc", "back");
    frame.render_widget(Paragraph::new(Line::from(command_spans)), rows[1]);

    let status = if app.status.trim().is_empty() || app.status == HOME_DEFAULT_STATUS {
        "Review generated changes deliberately. Accept only what belongs.".to_string()
    } else {
        app.status.clone()
    };
    frame.render_widget(
        Paragraph::new(status).style(styles::subtle().bg(styles::surface_raised())),
        rows[2],
    );
}

fn append_footer_key(spans: &mut Vec<Span<'static>>, key: &str, label: &str) {
    spans.push(Span::styled(
        " ",
        Style::default().bg(styles::surface_raised()),
    ));
    spans.push(Span::styled(
        format!(" {key} "),
        Style::default()
            .fg(styles::text_primary())
            .bg(styles::accent_dim())
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" {label} "),
        styles::muted().bg(styles::surface_raised()),
    ));
}

const DIFF_SYNTAX_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "else", "enum", "extern", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "return", "self",
    "Self", "static", "struct", "trait", "type", "use", "where", "while",
];

fn syntax_highlight_diff_content(
    content: &str,
    kind: DiffLineKind,
    modifier: Modifier,
) -> Vec<Span<'static>> {
    let base_style = diff_content_style(kind).add_modifier(modifier);
    if content.is_empty() {
        return vec![Span::styled(String::new(), base_style)];
    }

    let chars = content.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '/' && chars.get(index + 1) == Some(&'/') {
            let comment = chars[index..].iter().collect::<String>();
            spans.push(Span::styled(
                comment,
                syntax_tint(base_style, styles::syntax_comment()),
            ));
            break;
        }

        let current = chars[index];

        if matches!(current, '"' | '\'' | '`') {
            let quote = current;
            let start = index;
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index = (index + 2).min(chars.len());
                    continue;
                }
                if chars[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            spans.push(Span::styled(
                chars[start..index].iter().collect::<String>(),
                syntax_tint(base_style, styles::syntax_string()),
            ));
            continue;
        }

        if current.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '.'))
            {
                index += 1;
            }
            spans.push(Span::styled(
                chars[start..index].iter().collect::<String>(),
                syntax_tint(base_style, styles::accent_bright_color()),
            ));
            continue;
        }

        if is_identifier_start(current) {
            let start = index;
            index += 1;
            while index < chars.len() && is_identifier_continue(chars[index]) {
                index += 1;
            }

            let token = chars[start..index].iter().collect::<String>();
            let next_char = next_non_whitespace_char(&chars, index);
            let style = if DIFF_SYNTAX_KEYWORDS.contains(&token.as_str()) {
                syntax_tint(base_style, styles::syntax_keyword())
            } else if next_char == Some('(') {
                syntax_tint(base_style, styles::syntax_function())
            } else if token
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
                && token.len() > 1
            {
                syntax_tint(base_style, styles::accent_bright_color())
            } else {
                base_style
            };

            spans.push(Span::styled(token, style));
            continue;
        }

        let start = index;
        index += 1;
        while index < chars.len() {
            let ch = chars[index];
            let starts_comment = ch == '/' && chars.get(index + 1) == Some(&'/');
            if starts_comment
                || matches!(ch, '"' | '\'' | '`')
                || ch.is_ascii_digit()
                || is_identifier_start(ch)
            {
                break;
            }
            index += 1;
        }

        spans.push(Span::styled(
            chars[start..index].iter().collect::<String>(),
            base_style,
        ));
    }

    if spans.is_empty() {
        spans.push(Span::styled(content.to_string(), base_style));
    }

    spans
}

fn syntax_tint(base: Style, fg: Color) -> Style {
    let mut style = Style::default().fg(fg).add_modifier(base.add_modifier);
    if let Some(bg) = base.bg {
        style = style.bg(bg);
    }
    style
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn next_non_whitespace_char(chars: &[char], mut index: usize) -> Option<char> {
    while index < chars.len() {
        if !chars[index].is_whitespace() {
            return Some(chars[index]);
        }
        index += 1;
    }
    None
}

pub(super) fn diff_scroll_offset(app: &App, area: Rect, diff_lines: &[Line<'_>]) -> u16 {
    if app.review.focus != ReviewFocus::Hunks {
        return 0;
    }

    let visible_height = usize::from(area.height.max(1));
    if visible_height == 0 {
        return 0;
    }

    let total_lines = diff_lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);
    let selected_row = app.review.cursor_line.saturating_sub(1);
    let preferred_top = selected_row.saturating_sub(visible_height.saturating_sub(3));
    preferred_top.min(max_scroll).min(u16::MAX as usize) as u16
}
