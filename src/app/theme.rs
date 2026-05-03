use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui_core::style::{Modifier, Style};

use crate::settings::ThemePreset;
use crate::ui::styles::{self, Palette};

use super::{App, KeybindingCommand, Overlay, centered_rect, key_for, key_matches, save_settings};

pub(super) fn open_theme_picker(app: &mut App) {
    app.overlay = Overlay::ThemePicker;
    app.theme_cursor = theme_picker_cursor(app.settings.theme);
    app.status = "Choose a UI theme.".to_string();
}

pub(super) fn handle_theme_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::Settings;
            app.status = "Back to settings.".to_string();
        }
        KeyCode::Up => {
            app.theme_cursor = app.theme_cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.theme_cursor = app.theme_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.theme_cursor + 1 < ThemePreset::ALL.len() => {
            app.theme_cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.theme_cursor + 1 < ThemePreset::ALL.len() =>
        {
            app.theme_cursor += 1;
        }
        KeyCode::Enter => {
            let theme = selected_theme(app);
            app.settings.theme = theme;
            app.palette = Palette::from_theme(theme);
            save_settings(app);
            app.overlay = Overlay::Settings;
            app.status = format!("Theme set to {}.", theme.label());
        }
        _ => {}
    }
}

fn selected_theme(app: &App) -> ThemePreset {
    ThemePreset::ALL[app.theme_cursor.min(ThemePreset::ALL.len() - 1)]
}

pub(super) fn theme_picker_cursor(theme: ThemePreset) -> usize {
    ThemePreset::ALL
        .iter()
        .position(|candidate| *candidate == theme)
        .unwrap_or(0)
}

pub(super) fn draw_theme_picker(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    styles::set_palette(app.palette);
    let modal = centered_rect(54, 42, area);
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
            Span::styled("Theme", styles::title()),
            Span::styled("  Pick the UI color palette.", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[0],
    );

    let rows = theme_picker_items(app);
    let mut state = ListState::default().with_selected(Some(app.theme_cursor));
    frame.render_stateful_widget(
        List::new(rows)
            .block(Block::default().style(Style::default().bg(styles::surface_raised()))),
        sections[1],
        &mut state,
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
            Span::styled(" select", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" back", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[2],
    );
}

fn theme_picker_items(app: &App) -> Vec<ListItem<'static>> {
    ThemePreset::ALL
        .iter()
        .copied()
        .enumerate()
        .map(|(index, theme)| {
            let style = if index == app.theme_cursor {
                Style::default()
                    .fg(styles::text_primary())
                    .bg(styles::accent_dim())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(styles::text_muted())
            };
            let marker = if app.settings.theme == theme {
                "[✓]"
            } else {
                "[ ]"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), style),
                Span::styled(theme.label().to_string(), style),
            ]))
        })
        .collect()
}
