use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui_core::style::Style;

use crate::ui::styles;

use super::{
    App, Overlay, centered_rect, new_github_token_input_with_value, save_settings,
    to_textarea_input,
};

pub(super) fn open_github_token_prompt(app: &mut App) {
    app.github_token_input =
        new_github_token_input_with_value(app.settings.github.token.as_deref().unwrap_or_default());
    app.overlay = Overlay::GitHubTokenPrompt;
    app.status = "Enter a GitHub token for HTTPS publishing.".to_string();
}

pub(super) fn handle_github_token_prompt_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.github_token_input = new_github_token_input_with_value(
                app.settings.github.token.as_deref().unwrap_or_default(),
            );
            app.overlay = Overlay::Settings;
            app.status = "GitHub token unchanged.".to_string();
        }
        KeyCode::Enter => {
            let token = app.github_token_input.lines().join("").trim().to_string();
            app.settings.github.token = if token.is_empty() { None } else { Some(token) };
            save_settings(app);
            app.github_token_input = new_github_token_input_with_value(
                app.settings.github.token.as_deref().unwrap_or_default(),
            );
            app.overlay = Overlay::Settings;
            app.status = if app.settings.github.token.is_some() {
                "GitHub token saved.".to_string()
            } else {
                "GitHub token cleared.".to_string()
            };
        }
        _ => {
            app.github_token_input.input(to_textarea_input(key));
        }
    }
}

pub(super) fn draw_github_token_prompt(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(64, 34, area);
    frame.render_widget(Clear, modal);
    let inner = modal.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Block::default()
            .title("GitHub Token")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(styles::border_muted()))
            .style(Style::default().bg(styles::surface_raised())),
        modal,
    );
    frame.render_widget(
        Paragraph::new("Used only for HTTPS git push. Stored locally and hidden in the UI.")
            .style(styles::muted()),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new("Create a fine-grained token with repository Contents read/write access.")
            .style(styles::subtle())
            .wrap(Wrap { trim: true }),
        sections[1],
    );
    frame.render_widget(&app.github_token_input, sections[2]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", styles::keybind()),
            Span::styled(" save", styles::muted()),
            Span::raw("  "),
            Span::styled("Esc", styles::keybind()),
            Span::styled(" cancel", styles::muted()),
        ]))
        .style(Style::default().bg(styles::surface_raised())),
        sections[3],
    );
}
