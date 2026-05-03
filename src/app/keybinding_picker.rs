use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui_core::style::{Modifier, Style};

use crate::ui::styles;

use super::keybindings::{
    command_binding, command_label, is_valid_keybinding_char, keybinding_conflict,
    selected_keybinding_command, set_command_binding,
};
use super::{
    App, KEYBINDING_COMMANDS, KeybindingCommand, Overlay, centered_rect, key_for, key_matches,
    save_settings,
};

pub(super) fn handle_keybinding_picker_key(app: &mut App, key: KeyEvent) {
    if let Some(command) = app.keybinding_capture {
        match key.code {
            KeyCode::Esc => {
                app.keybinding_capture = None;
                app.status = "Keybinding unchanged.".to_string();
            }
            KeyCode::Char(ch) if is_valid_keybinding_char(ch) => {
                if let Some(conflict) = keybinding_conflict(&app.settings.keybindings, command, ch)
                {
                    app.status = format!(
                        "Key '{}' is already assigned to {}.",
                        ch,
                        command_label(conflict)
                    );
                } else {
                    set_command_binding(&mut app.settings.keybindings, command, ch);
                    save_settings(app);
                    app.keybinding_capture = None;
                    app.status = format!("{} set to '{}'.", command_label(command), ch);
                }
            }
            KeyCode::Char(_) => {
                app.status = "Use a lowercase letter key.".to_string();
            }
            _ => {
                app.status = "Use a lowercase letter key, or Esc to cancel.".to_string();
            }
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::Settings;
            app.status = "Back to settings.".to_string();
        }
        KeyCode::Up => {
            app.keybinding_cursor = app.keybinding_cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.keybinding_cursor = app.keybinding_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.keybinding_cursor + 1 < KEYBINDING_COMMANDS.len() => {
            app.keybinding_cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.keybinding_cursor + 1 < KEYBINDING_COMMANDS.len() =>
        {
            app.keybinding_cursor += 1;
        }
        KeyCode::Enter | KeyCode::Right => {
            let command = selected_keybinding_command(app);
            app.keybinding_capture = Some(command);
            app.status = format!(
                "Press a lowercase letter for {}, or Esc to cancel.",
                command_label(command)
            );
        }
        _ => {}
    }
}

pub(super) fn open_keybinding_picker(app: &mut App) {
    app.overlay = Overlay::KeybindingPicker;
    app.keybinding_cursor = 0;
    app.keybinding_capture = None;
    app.status = "Choose a command to rebind.".to_string();
}

pub(super) fn draw_keybinding_picker(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(62, 58, area);
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
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Keybindings", styles::title()),
            Span::styled("  Each command needs its own letter.", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[0],
    );

    let rows = keybinding_picker_items(app);
    let mut state = ListState::default().with_selected(Some(app.keybinding_cursor));
    frame.render_stateful_widget(
        List::new(rows)
            .block(Block::default().style(Style::default().bg(styles::surface_raised()))),
        sections[1],
        &mut state,
    );

    let help = if let Some(command) = app.keybinding_capture {
        Line::from(vec![
            Span::styled("Press a lowercase letter", styles::keybind()),
            Span::styled(format!(" for {}", command_label(command)), styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" cancel", styles::muted()),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                key_for(app, KeybindingCommand::MoveDown).to_string(),
                styles::keybind(),
            ),
            Span::raw("/"),
            Span::styled(
                key_for(app, KeybindingCommand::MoveUp).to_string(),
                styles::keybind(),
            ),
            Span::styled(" move", styles::muted()),
            Span::raw("  "),
            Span::styled("Enter", styles::keybind()),
            Span::styled(" rebind", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" back", styles::muted()),
        ])
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().bg(styles::surface_raised())),
        sections[2],
    );
}

fn keybinding_picker_items(app: &App) -> Vec<ListItem<'static>> {
    KEYBINDING_COMMANDS
        .iter()
        .copied()
        .enumerate()
        .map(|(index, command)| {
            let selected = index == app.keybinding_cursor;
            let capturing = app.keybinding_capture == Some(command);
            let style = if selected || capturing {
                Style::default()
                    .fg(styles::text_primary())
                    .bg(styles::accent_dim())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(styles::text_muted())
            };
            let marker = if capturing {
                "?"
            } else if selected {
                ">"
            } else {
                " "
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} {:<24}", command_label(command)), style),
                Span::styled(" ", style),
                Span::styled(
                    command_binding(&app.settings.keybindings, command).to_string(),
                    styles::keybind(),
                ),
            ]))
        })
        .collect()
}
