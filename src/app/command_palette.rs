use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui_core::style::{Modifier, Style};
use ratatui_textarea::TextArea;

use crate::ui::styles;

use super::{
    App, KeybindingCommand, Overlay, ReviewFocus, Screen, centered_rect, handle_review_key,
    key_for, key_label, open_explain_menu, open_settings, open_theme_picker,
    refresh_review_files_for_user, sync_cursor_line_to_hunk,
};

#[derive(Default)]
pub(super) struct CommandPaletteUiState {
    pub(super) cursor: usize,
    pub(super) query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandPaletteAction {
    Refresh,
    EnterReview,
    BackHome,
    FocusFiles,
    FocusHunks,
    Accept,
    Reject,
    Unreview,
    Explain,
    Commit,
    Settings,
    Theme,
}

pub(super) struct CommandPaletteItem {
    pub(super) action: CommandPaletteAction,
    pub(super) label: &'static str,
    pub(super) detail: &'static str,
    pub(super) shortcut: String,
    pub(super) enabled: bool,
}

pub(super) fn is_command_palette_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('k'))
}

pub(super) fn open_command_palette(app: &mut App) {
    app.overlay = Overlay::CommandPalette;
    app.command_palette.cursor = 0;
    app.command_palette.query.clear();
    app.status = "Command palette opened.".to_string();
}

fn command_palette_items(app: &App) -> Vec<CommandPaletteItem> {
    let review_available = !app.review.files.is_empty();
    let in_review = app.screen == Screen::Review && review_available;
    let has_hunks = in_review
        && app
            .review
            .files
            .get(app.review.cursor_file)
            .is_some_and(|file| !file.hunks.is_empty());

    vec![
        CommandPaletteItem {
            action: CommandPaletteAction::Refresh,
            label: "Refresh changes",
            detail: "Reload the current worktree diff",
            shortcut: key_label(key_for(app, KeybindingCommand::Refresh)),
            enabled: !app.review_busy,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::EnterReview,
            label: "Enter review",
            detail: "Open the review workspace",
            shortcut: "Enter".to_string(),
            enabled: app.screen == Screen::Home && review_available,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::BackHome,
            label: "Back to home",
            detail: "Return to the better-review home screen",
            shortcut: "Esc".to_string(),
            enabled: app.screen == Screen::Review,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::FocusFiles,
            label: "Focus files",
            detail: "Move focus to the changed-file sidebar",
            shortcut: "Esc".to_string(),
            enabled: in_review && app.review.focus == ReviewFocus::Hunks,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::FocusHunks,
            label: "Focus hunks",
            detail: "Move focus into the diff hunks",
            shortcut: "Enter".to_string(),
            enabled: has_hunks,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Accept,
            label: "Accept selection",
            detail: "Stage the current file or hunk for commit",
            shortcut: key_label(key_for(app, KeybindingCommand::Accept)),
            enabled: in_review && !app.review_busy,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Reject,
            label: "Reject selection",
            detail: "Leave the current file or hunk out of the commit",
            shortcut: key_label(key_for(app, KeybindingCommand::Reject)),
            enabled: in_review && !app.review_busy,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Unreview,
            label: "Move file to unreviewed",
            detail: "Unstage the current file and mark it pending",
            shortcut: key_label(key_for(app, KeybindingCommand::Unreview)),
            enabled: in_review && !app.review_busy,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Explain,
            label: "Explain selection",
            detail: "Ask Explain about the current file or hunk",
            shortcut: key_label(key_for(app, KeybindingCommand::Explain)),
            enabled: in_review,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Commit,
            label: "Commit accepted changes",
            detail: "Write a commit message for accepted changes",
            shortcut: key_label(key_for(app, KeybindingCommand::Commit)),
            enabled: review_available && !app.review_busy,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Settings,
            label: "Open settings",
            detail: "Theme, GitHub token, keybindings, and Explain defaults",
            shortcut: key_label(key_for(app, KeybindingCommand::Settings)),
            enabled: true,
        },
        CommandPaletteItem {
            action: CommandPaletteAction::Theme,
            label: "Change theme",
            detail: "Choose a polished editor color palette",
            shortcut: "theme".to_string(),
            enabled: true,
        },
    ]
}

pub(super) fn command_palette_filtered_items(app: &App) -> Vec<CommandPaletteItem> {
    let query = app.command_palette.query.trim().to_lowercase();
    let mut items = command_palette_items(app);
    if query.is_empty() {
        return items;
    }

    items.retain(|item| {
        let haystack = format!("{} {} {}", item.label, item.detail, item.shortcut).to_lowercase();
        haystack.contains(&query)
    });
    items
}

fn clamp_command_palette_cursor(app: &mut App) {
    let len = command_palette_filtered_items(app).len();
    if len == 0 {
        app.command_palette.cursor = 0;
    } else {
        app.command_palette.cursor = app.command_palette.cursor.min(len - 1);
    }
}

pub(super) async fn handle_command_palette_key(
    app: &mut App,
    key: KeyEvent,
    commit_message: &mut TextArea<'static>,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.status = "Command palette closed.".to_string();
        }
        KeyCode::Up => {
            app.command_palette.cursor = app.command_palette.cursor.saturating_sub(1);
        }
        KeyCode::Down => {
            let len = command_palette_filtered_items(app).len();
            if app.command_palette.cursor + 1 < len {
                app.command_palette.cursor += 1;
            }
        }
        KeyCode::Backspace => {
            app.command_palette.query.pop();
            clamp_command_palette_cursor(app);
        }
        KeyCode::Enter => {
            let items = command_palette_filtered_items(app);
            if let Some(item) = items.get(app.command_palette.cursor) {
                if item.enabled {
                    execute_command_palette_action(app, item.action, commit_message).await?;
                } else {
                    app.status = format!("{} is unavailable right now.", item.label);
                }
            }
        }
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.command_palette.query.push(ch);
            clamp_command_palette_cursor(app);
        }
        _ => {}
    }
    Ok(())
}

pub(super) async fn execute_command_palette_action(
    app: &mut App,
    action: CommandPaletteAction,
    commit_message: &mut TextArea<'static>,
) -> Result<()> {
    app.overlay = Overlay::None;
    match action {
        CommandPaletteAction::Refresh => refresh_review_files_for_user(app).await?,
        CommandPaletteAction::EnterReview => {
            if app.review.files.is_empty() {
                app.status = "No reviewable changes yet.".to_string();
            } else {
                app.screen = Screen::Review;
                app.status = "Review workspace ready.".to_string();
            }
        }
        CommandPaletteAction::BackHome => {
            app.screen = Screen::Home;
            app.review.focus = ReviewFocus::Files;
            app.status = "Back on the better-review home screen.".to_string();
        }
        CommandPaletteAction::FocusFiles => {
            app.review.focus = ReviewFocus::Files;
            app.status = "Focused changed files.".to_string();
        }
        CommandPaletteAction::FocusHunks => {
            app.review.focus = ReviewFocus::Hunks;
            sync_cursor_line_to_hunk(&mut app.review);
            app.status = "Focused diff hunks.".to_string();
        }
        CommandPaletteAction::Accept => {
            let key = KeyEvent::new(
                KeyCode::Char(key_for(app, KeybindingCommand::Accept)),
                KeyModifiers::NONE,
            );
            handle_review_key(app, key).await?;
        }
        CommandPaletteAction::Reject => {
            let key = KeyEvent::new(
                KeyCode::Char(key_for(app, KeybindingCommand::Reject)),
                KeyModifiers::NONE,
            );
            handle_review_key(app, key).await?;
        }
        CommandPaletteAction::Unreview => {
            let key = KeyEvent::new(
                KeyCode::Char(key_for(app, KeybindingCommand::Unreview)),
                KeyModifiers::NONE,
            );
            handle_review_key(app, key).await?;
        }
        CommandPaletteAction::Explain => open_explain_menu(app),
        CommandPaletteAction::Commit => {
            if app.review.files.is_empty() {
                app.status =
                    "Cannot commit yet because there are no reviewable changes.".to_string();
            } else if app.review_busy {
                app.status = "Wait for the current review update to finish.".to_string();
            } else {
                *commit_message = app.open_commit_prompt();
            }
        }
        CommandPaletteAction::Settings => open_settings(app),
        CommandPaletteAction::Theme => open_theme_picker(app),
    }
    Ok(())
}

pub(super) fn draw_command_palette(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(66, 58, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(styles::accent_bright_color()))
            .style(Style::default().bg(styles::surface_raised())),
        modal,
    );

    let inner = modal.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Command Palette", styles::title()),
                Span::styled("  Ctrl+P / Ctrl+K", styles::subtle()),
            ]),
            Line::from(Span::styled(
                "Run review actions without memorizing keybindings.",
                styles::muted(),
            )),
        ])
        .style(Style::default().bg(styles::surface_raised())),
        sections[0],
    );

    let query = if app.command_palette.query.is_empty() {
        "type to filter commands".to_string()
    } else {
        app.command_palette.query.clone()
    };
    let query_style = if app.command_palette.query.is_empty() {
        styles::subtle()
    } else {
        styles::title()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", styles::accent_bold()),
            Span::styled(query, query_style),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(styles::border_muted())),
        )
        .style(Style::default().bg(styles::surface_raised())),
        sections[1],
    );

    let items = command_palette_filtered_items(app);
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("No commands match your search.")
                .alignment(Alignment::Center)
                .style(styles::muted().bg(styles::surface_raised())),
            sections[2],
        );
    } else {
        let visible = usize::from(sections[2].height.max(1));
        let selected = app
            .command_palette
            .cursor
            .min(items.len().saturating_sub(1));
        let scroll = selected.saturating_sub(visible.saturating_sub(1));
        let list_items = items
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible)
            .map(|(index, item)| {
                let selected = index == selected;
                let row_bg = if selected {
                    styles::accent_dim()
                } else {
                    styles::surface_raised()
                };
                let label_style = if !item.enabled {
                    styles::subtle().bg(row_bg)
                } else if selected {
                    Style::default()
                        .fg(styles::text_primary())
                        .bg(row_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    styles::title().bg(row_bg)
                };
                let detail_style = if item.enabled {
                    styles::muted().bg(row_bg)
                } else {
                    styles::subtle().bg(row_bg)
                };
                let marker = if selected { "▌" } else { " " };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        marker,
                        Style::default()
                            .fg(styles::accent_bright_color())
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" ", Style::default().bg(row_bg)),
                    Span::styled(item.label.to_string(), label_style),
                    Span::styled("  ", Style::default().bg(row_bg)),
                    Span::styled(item.detail.to_string(), detail_style),
                    Span::styled("  ", Style::default().bg(row_bg)),
                    Span::styled(
                        item.shortcut.clone(),
                        Style::default()
                            .fg(styles::accent_bright_color())
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
            })
            .collect::<Vec<_>>();

        frame.render_widget(
            List::new(list_items)
                .block(Block::default().style(Style::default().bg(styles::surface_raised()))),
            sections[2],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑/↓", styles::keybind()),
            Span::styled(" move  ", styles::muted()),
            Span::styled("Enter", styles::keybind()),
            Span::styled(" run  ", styles::muted()),
            Span::styled("Backspace", styles::keybind()),
            Span::styled(" edit  ", styles::muted()),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" close", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[3],
    );
}
