use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui_core::style::{Modifier, Style};

use crate::ui::styles;

use super::{
    App, KEYBINDING_COMMANDS, KeybindingCommand, Overlay, centered_rect, key_for, key_matches,
    open_github_token_prompt, open_keybinding_picker, open_saved_model_picker, open_theme_picker,
    saved_model_label,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsRow {
    Theme,
    DefaultExplainModel,
    GitHubToken,
    Keybindings,
}

const SETTINGS_ROWS: &[SettingsRow] = &[
    SettingsRow::Theme,
    SettingsRow::DefaultExplainModel,
    SettingsRow::GitHubToken,
    SettingsRow::Keybindings,
];

pub(super) fn open_settings(app: &mut App) {
    app.overlay = Overlay::Settings;
    app.settings_cursor = 0;
    app.status = "Settings opened.".to_string();
}

pub(super) fn save_settings(app: &mut App) {
    match app.settings_store.save(&app.settings) {
        Ok(()) => {
            app.apply_saved_settings();
        }
        Err(error) => {
            app.status = format!("Could not save settings: {error}");
        }
    }
}

pub(super) fn handle_settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.status = "Closed settings.".to_string();
        }
        KeyCode::Up => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.settings_cursor + 1 < settings_row_count() => {
            app.settings_cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.settings_cursor + 1 < settings_row_count() =>
        {
            app.settings_cursor += 1;
        }
        KeyCode::Enter | KeyCode::Right => {
            open_selected_settings_row(app);
        }
        _ => {}
    }
}

fn settings_row_count() -> usize {
    SETTINGS_ROWS.len()
}

fn open_selected_settings_row(app: &mut App) {
    match SETTINGS_ROWS[app.settings_cursor.min(SETTINGS_ROWS.len() - 1)] {
        SettingsRow::Theme => open_theme_picker(app),
        SettingsRow::DefaultExplainModel => open_saved_model_picker(app),
        SettingsRow::GitHubToken => open_github_token_prompt(app),
        SettingsRow::Keybindings => open_keybinding_picker(app),
    }
}

pub(super) fn settings_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for (index, row) in SETTINGS_ROWS.iter().copied().enumerate() {
        let (label, value) = settings_row_content(app, row);
        let selected = index == app.settings_cursor;
        let row_style = if selected {
            Style::default()
                .fg(styles::text_primary())
                .bg(styles::accent_dim())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(styles::text_muted())
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {label:<18}"), row_style),
            Span::styled(value, row_style),
        ]));
    }

    lines
}

fn settings_row_content(app: &App, row: SettingsRow) -> (&'static str, String) {
    match row {
        SettingsRow::Theme => ("Theme", app.settings.theme.label().to_string()),
        SettingsRow::DefaultExplainModel => (
            "Default model",
            saved_model_label(&app.settings.explain.default_model),
        ),
        SettingsRow::GitHubToken => (
            "GitHub token",
            if app.settings.github.token.is_some() {
                "Saved".to_string()
            } else {
                "Not set".to_string()
            },
        ),
        SettingsRow::Keybindings => (
            "Keybindings",
            format!("{} shortcuts", KEYBINDING_COMMANDS.len()),
        ),
    }
}

pub(super) fn draw_settings(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(58, 36, area);
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
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Settings", styles::title())),
            Line::from(Span::styled("Saved preferences", styles::muted())),
        ])
        .style(Style::default().bg(styles::surface_raised())),
        sections[0],
    );

    let rows = settings_lines(app);
    frame.render_widget(
        Paragraph::new(rows)
            .style(Style::default().bg(styles::surface_raised()))
            .wrap(Wrap { trim: true }),
        sections[1],
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
            Span::styled(" open", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" close", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[2],
    );
}
