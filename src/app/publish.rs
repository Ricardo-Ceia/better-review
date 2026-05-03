use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui_core::style::{Modifier, Style};

use crate::services::git::PushFailure;
use crate::ui::styles;

use super::{App, KeybindingCommand, Message, Overlay, centered_rect, key_for, key_matches};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishAction {
    PushCurrentBranch,
    NotNow,
}

const PUBLISH_ACTIONS: &[PublishAction] =
    &[PublishAction::PushCurrentBranch, PublishAction::NotNow];

pub(super) fn push_reviewed_changes(app: &mut App) {
    if app.publish_busy {
        app.status = "Publish is already running.".to_string();
        return;
    }

    let git = app.git.clone();
    let token = app.settings.github.token.clone();
    let tx = app.tx.clone();
    app.publish_busy = true;
    app.status = "Publishing reviewed commit...".to_string();
    tokio::spawn(async move {
        let result = git.push_current_branch(token.as_deref()).await;
        let _ = tx.send(Message::Publish { result });
    });
}

pub(super) fn handle_publish_result(app: &mut App, result: Result<(), PushFailure>) {
    app.publish_busy = false;
    match result {
        Ok(()) => {
            app.overlay = Overlay::None;
            app.status = "Pushed reviewed commit to GitHub.".to_string();
        }
        Err(error) => {
            app.status = error.message;
        }
    }
}

pub(super) fn handle_publish_prompt_key(app: &mut App, key: KeyEvent) {
    if app.publish_busy {
        if key.code == KeyCode::Esc {
            app.status = "Publish is still running.".to_string();
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.status = "Publish skipped. Commit remains local.".to_string();
        }
        KeyCode::Up => {
            app.publish_cursor = app.publish_cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.publish_cursor = app.publish_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.publish_cursor + 1 < PUBLISH_ACTIONS.len() => {
            app.publish_cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.publish_cursor + 1 < PUBLISH_ACTIONS.len() =>
        {
            app.publish_cursor += 1;
        }
        KeyCode::Enter => match selected_publish_action(app) {
            PublishAction::PushCurrentBranch => push_reviewed_changes(app),
            PublishAction::NotNow => {
                app.overlay = Overlay::None;
                app.status = "Publish skipped. Commit remains local.".to_string();
            }
        },
        _ => {}
    }
}

fn selected_publish_action(app: &App) -> PublishAction {
    PUBLISH_ACTIONS[app.publish_cursor.min(PUBLISH_ACTIONS.len() - 1)]
}

pub(super) fn draw_publish_prompt(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(54, 34, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default().style(Style::default().bg(styles::surface_raised())),
        modal,
    );
    let inner = modal.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Publish", styles::title())),
            Line::from(Span::styled(
                "Your reviewed commit is local.",
                styles::muted(),
            )),
        ])
        .style(Style::default().bg(styles::surface_raised())),
        sections[0],
    );

    let mut state = ListState::default().with_selected(Some(app.publish_cursor));
    frame.render_stateful_widget(
        List::new(publish_prompt_items(app))
            .block(Block::default().style(Style::default().bg(styles::surface_raised()))),
        sections[1],
        &mut state,
    );

    frame.render_widget(
        Paragraph::new(publish_status_line(app))
            .style(Style::default().bg(styles::surface_raised()))
            .wrap(Wrap { trim: true }),
        sections[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(
                    "{}/{}",
                    key_for(app, KeybindingCommand::MoveDown),
                    key_for(app, KeybindingCommand::MoveUp)
                ),
                styles::keybind(),
            ),
            Span::styled(" move", styles::muted()),
            Span::raw("  "),
            Span::styled("Enter", styles::keybind()),
            Span::styled(" choose", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" later", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[3],
    );
}

pub(super) fn publish_status_line(app: &App) -> Line<'static> {
    if app.publish_busy {
        return Line::from(vec![
            Span::styled("Publishing", styles::keybind()),
            Span::styled("  pushing current branch...", styles::muted()),
        ]);
    }

    if app.status.trim().is_empty() {
        return Line::from(Span::styled(
            "Push uses your GitHub token from Settings when needed.",
            styles::muted(),
        ));
    }

    Line::from(Span::styled(app.status.clone(), styles::muted()))
}

fn publish_prompt_items(app: &App) -> Vec<ListItem<'static>> {
    PUBLISH_ACTIONS
        .iter()
        .copied()
        .enumerate()
        .map(|(index, action)| {
            let selected = index == app.publish_cursor;
            let style = if selected && !app.publish_busy {
                Style::default()
                    .fg(styles::text_primary())
                    .bg(styles::accent_dim())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(styles::text_muted())
            };
            let marker = if selected { ">" } else { " " };
            ListItem::new(Line::from(Span::styled(
                format!("{marker} {}", publish_action_label(action)),
                style,
            )))
        })
        .collect()
}

fn publish_action_label(action: PublishAction) -> &'static str {
    match action {
        PublishAction::PushCurrentBranch => "Push current branch",
        PublishAction::NotNow => "Not now",
    }
}
