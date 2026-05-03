use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui_core::style::{Modifier, Style};

use crate::services::opencode::OpencodeSession;
use crate::ui::styles;

use super::{App, KeybindingCommand, Overlay, centered_rect, close_explain_submenu, key_matches};

#[derive(Default)]
pub(super) struct SessionUiState {
    pub(super) sessions: Vec<OpencodeSession>,
    pub(super) selected: Option<usize>,
    pub(super) cursor: usize,
}

pub(super) fn handle_session_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            close_explain_submenu(app, "Session picker closed.");
        }
        KeyCode::Up => {
            app.session_state.cursor = app.session_state.cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.session_state.cursor = app.session_state.cursor.saturating_sub(1);
        }
        KeyCode::Down if app.session_state.cursor + 1 < app.session_state.sessions.len() => {
            app.session_state.cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.session_state.cursor + 1 < app.session_state.sessions.len() =>
        {
            app.session_state.cursor += 1;
        }
        KeyCode::Enter => {
            app.session_state.selected = Some(app.session_state.cursor);
            app.refresh_auto_model();
            close_explain_submenu(app, "Choose a file or hunk, then run Explain.");
            if let Some(session) = app.active_session() {
                app.status = format!("Explain will use context source {}.", session.title);
            }
        }
        _ => {}
    }
}

pub(super) fn open_session_picker(app: &mut App) {
    if app.session_state.sessions.is_empty() {
        app.status = "No opencode sessions were found for this repository.".to_string();
        return;
    }

    if let Some(selected) = app.session_state.selected {
        app.session_state.cursor = selected;
    }
    app.overlay = Overlay::SessionPicker;
    app.status = "Choose the context source for Explain.".to_string();
}

pub(super) fn draw_session_picker(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(58, 42, area);
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
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(inner);
    let items = app
        .session_state
        .sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let style = if index == app.session_state.cursor {
                Style::default()
                    .fg(styles::text_primary())
                    .bg(styles::accent_dim())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(styles::text_muted())
            };
            let marker = if app.session_state.selected == Some(index) {
                "[✓]"
            } else {
                "[ ]"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), style),
                Span::styled(session.title.clone(), style),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.session_state.cursor));
    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .title(Line::from(Span::styled(
                    "Choose context source",
                    styles::title(),
                )))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(styles::accent_bright_color()))
                .style(Style::default().bg(styles::surface_raised())),
        ),
        sections[0],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", styles::keybind()),
            Span::styled(" select", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" close", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[1],
    );
}
