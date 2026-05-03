use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui_core::style::Style;
use ratatui_textarea::TextArea;

use crate::ui::styles;

use super::{
    App, Overlay, centered_rect, new_commit_message_input, refresh_review_files, to_textarea_input,
};

pub(super) fn open_commit_prompt(app: &mut App) -> TextArea<'static> {
    app.overlay = Overlay::CommitPrompt;
    app.status = "Write a commit message for the accepted changes.".to_string();

    new_commit_message_input()
}

pub(super) async fn handle_commit_prompt_key(
    app: &mut App,
    key: KeyEvent,
    commit_message: &mut TextArea<'static>,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.status = "Commit cancelled. Review remains active.".to_string();
        }
        KeyCode::Enter => {
            submit_commit_message(app, commit_message).await?;
        }
        _ => {
            commit_message.input(to_textarea_input(key));
        }
    }
    Ok(())
}

pub(super) async fn submit_commit_message(
    app: &mut App,
    commit_message: &mut TextArea<'static>,
) -> Result<()> {
    let message = commit_message.lines().join("\n").trim().to_string();
    if message.is_empty() {
        app.status = "Write a commit message first.".to_string();
        return Ok(());
    }

    if !app.git.has_staged_changes().await? {
        app.status = "No accepted changes are staged yet.".to_string();
        return Ok(());
    }

    if app.had_staged_changes_on_open {
        app.status =
            "Cannot commit from better-review because the app opened with unrelated staged changes."
                .to_string();
        return Ok(());
    }

    app.git.commit_staged(&message).await?;
    refresh_review_files(app).await?;
    app.overlay = Overlay::PublishPrompt;
    app.publish_cursor = 0;
    app.status = "Committed accepted changes. Publish when ready.".to_string();
    *commit_message = new_commit_message_input();

    Ok(())
}

pub(super) fn draw_commit_prompt(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &App,
    commit_message: &TextArea<'_>,
) {
    let modal = centered_rect(60, 35, area);
    frame.render_widget(Clear, modal);
    let inner = modal.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(inner);

    let counts = app.review_counts();
    let block = Block::default()
        .title("Commit Accepted Changes")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(styles::border_muted()))
        .style(Style::default().bg(styles::surface_raised()));
    frame.render_widget(block, modal);
    frame.render_widget(
        Paragraph::new(format!(
            "Accepted {}  |  Rejected {}  |  Unreviewed {}",
            counts.accepted, counts.rejected, counts.unreviewed
        ))
        .style(styles::title()),
        lines[0],
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::raw("Commit prompt active  |  "),
            Span::styled("Enter", styles::keybind()),
            Span::raw(" commit  |  "),
            Span::styled("Esc", styles::keybind()),
            Span::raw(" close"),
        ])])
        .style(styles::muted()),
        lines[1],
    );
    frame.render_widget(commit_message, lines[2]);
    frame.render_widget(
        Paragraph::new("Only accepted staged changes are committed.").style(styles::muted()),
        lines[3],
    );
}
