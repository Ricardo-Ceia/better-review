use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui_core::style::{Modifier, Style};
use ratatui_interact::components::AnimatedTextState;

use crate::services::opencode::WhyAnswer;
use crate::ui::styles;

#[cfg(test)]
use super::explain_history::current_explain_run;
use super::explain_history::{
    ExplainRun, ExplainRunStatus, explain_run_status_label, explain_run_status_style,
    selected_history_run,
};
use super::{
    App, KeybindingCommand, ReviewFocus, centered_rect, key_for, key_label, key_status_label,
    why_model_display_label,
};

pub(super) fn draw_explain_menu(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(64, 46, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(explain_menu_lines(app))
            .block(
                Block::default()
                    .title(Line::from(Span::styled("Explain", styles::title())))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(styles::accent_bright_color()))
                    .style(Style::default().bg(styles::surface_raised())),
            )
            .style(Style::default().bg(styles::surface_raised()))
            .wrap(Wrap { trim: true }),
        modal,
    );
}

pub(super) fn draw_explain_history(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let modal = centered_rect(70, 56, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(explain_history_lines(app))
            .block(
                Block::default()
                    .title(Line::from(Span::styled("Explain History", styles::title())))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(styles::accent_bright_color()))
                    .style(Style::default().bg(styles::surface_raised())),
            )
            .style(Style::default().bg(styles::surface_raised()))
            .wrap(Wrap { trim: true }),
        modal,
    );
}

#[cfg(test)]
pub(super) fn explain_panel_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = explain_context_lines(app);

    let Some(run) = current_explain_run(app) else {
        lines.extend(explain_empty_lines(app));
        return lines;
    };

    lines.push(Line::from(Span::raw("")));
    lines.extend(render_explain_run_lines(app, run, &app.logo_animation));
    lines.push(Line::from(Span::raw("")));
    lines.extend(explain_footer_lines(app));
    lines
}

fn explain_context_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        explain_context_source_line(app),
        styles::soft_accent(),
    ))];
    lines.push(Line::from(Span::styled(
        format!("model: {}", why_model_display_label(app)),
        styles::muted(),
    )));
    if let Some(scope_preview) = explain_scope_preview(app) {
        lines.push(Line::from(Span::styled(
            format!("scope: {scope_preview}"),
            styles::muted(),
        )));
    }
    lines
}

pub(super) fn explain_menu_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Review focus decides the scope.",
            styles::soft_accent(),
        )),
        Line::from(Span::raw("")),
        explain_menu_detail_line(
            "Scope",
            explain_scope_preview(app).unwrap_or_else(|| "nothing selected".to_string()),
        ),
        explain_menu_detail_line("Context", explain_context_source_label(app)),
        explain_menu_detail_line("Model", why_model_display_label(app)),
        explain_menu_detail_line(
            "History",
            format!("{} run(s) this session", app.why_this.runs.len()),
        ),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("Enter", styles::keybind()),
            Span::styled(" run explain", styles::muted()),
        ]),
        Line::from(vec![
            key_hint_span(app, KeybindingCommand::ExplainContext),
            Span::styled(" choose context", styles::muted()),
        ]),
        Line::from(vec![
            key_hint_span(app, KeybindingCommand::ExplainModel),
            Span::styled(" choose model", styles::muted()),
        ]),
        Line::from(vec![
            key_hint_span(app, KeybindingCommand::ExplainHistory),
            Span::styled(" open history", styles::muted()),
        ]),
        Line::from(vec![
            key_hint_span(app, KeybindingCommand::ExplainRetry),
            Span::styled(" retry current run", styles::muted()),
        ]),
        Line::from(vec![
            key_hint_span(app, KeybindingCommand::ExplainCancel),
            Span::styled(" cancel current run", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled("Esc", styles::keybind()),
            Span::styled(" close", styles::muted()),
        ]),
    ];

    if app.active_session().is_none() {
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            "Choose a context source before you run Explain.",
            Style::default()
                .fg(styles::danger())
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines
}

fn explain_menu_detail_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<7}"), styles::title()),
        Span::styled(value, styles::muted()),
    ])
}

#[cfg(test)]
fn explain_empty_lines(app: &App) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::raw("")),
        Line::from(Span::styled("Explain the current change", styles::title())),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::Explain)),
                styles::keybind(),
            ),
            Span::styled("open the Explain menu", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::ExplainModel)),
                styles::keybind(),
            ),
            Span::styled("choose model", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::ExplainContext)),
                styles::keybind(),
            ),
            Span::styled("choose context source", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::ExplainHistory)),
                styles::keybind(),
            ),
            Span::styled("open explain history", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::ExplainCancel)),
                styles::keybind(),
            ),
            Span::styled("cancel current run", styles::muted()),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", key_for(app, KeybindingCommand::ExplainRetry)),
                styles::keybind(),
            ),
            Span::styled("retry current run", styles::muted()),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Tip: file focus explains the file; hunk focus explains the current hunk.",
            styles::subtle(),
        )),
    ]
}

#[cfg(test)]
fn explain_footer_lines(app: &App) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        key_hint_span(app, KeybindingCommand::Explain),
        Span::styled(" menu", styles::muted()),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::Settings),
        Span::styled(" settings", styles::muted()),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::ExplainHistory),
        Span::styled(
            format!(" history ({})", app.why_this.runs.len()),
            styles::muted(),
        ),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::ExplainRetry),
        Span::styled(" retry", styles::muted()),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::ExplainCancel),
        Span::styled(" cancel", styles::muted()),
    ])]
}

fn explain_history_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = explain_context_lines(app);
    lines.push(Line::from(Span::raw("")));

    if app.why_this.runs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No explain runs yet.",
            styles::title(),
        )));
        return lines;
    }

    lines.push(Line::from(Span::styled(
        format!("{} run(s) this session", app.why_this.runs.len()),
        styles::title(),
    )));
    lines.extend(render_explain_history_list_lines(app));
    lines.push(Line::from(Span::raw("")));
    if let Some(run) = selected_history_run(app) {
        lines.extend(render_explain_run_lines(app, run, &app.logo_animation));
    }
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(vec![
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
        Span::styled(" focus", styles::muted()),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::ExplainRetry),
        Span::styled(" retry", styles::muted()),
        Span::raw("  "),
        key_hint_span(app, KeybindingCommand::ExplainCancel),
        Span::styled(" cancel", styles::muted()),
        Span::raw("  "),
        Span::styled("Del", styles::keybind()),
        Span::styled(" clear", styles::muted()),
    ]));
    lines
}

fn render_explain_history_list_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for (index, run) in app.why_this.runs.iter().enumerate() {
        let selected = app.why_this.history_cursor == index;
        let marker = if selected { ">" } else { " " };
        let style = if selected {
            Style::default()
                .fg(styles::text_primary())
                .bg(styles::accent_dim())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(styles::text_muted())
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{marker} #{} ", run.id), style),
            Span::styled(
                explain_run_status_label(&run.status),
                explain_run_status_style(&run.status),
            ),
            Span::styled(format!(" {}", run.label), style),
        ]));
    }

    lines
}

fn render_explain_run_lines(
    app: &App,
    run: &ExplainRun,
    animation: &AnimatedTextState,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(run.label.clone(), styles::title())),
        Line::from(Span::styled(
            format!("status: {}", explain_run_status_label(&run.status)),
            explain_run_status_style(&run.status),
        )),
        Line::from(Span::styled(
            format!("context: {}", run.context_source_label),
            styles::muted(),
        )),
        Line::from(Span::styled(
            format!("model: {}", run.model_label),
            styles::muted(),
        )),
    ];

    match &run.status {
        ExplainRunStatus::Running => {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled(
                loading_thinking_label(animation),
                styles::accent_bold(),
            )));
            lines.push(Line::from(Span::styled(
                "Using a fork of the selected context source so the live coding thread stays clean.",
                styles::muted(),
            )));
        }
        ExplainRunStatus::Ready => {
            let Some(answer) = &run.result else {
                return lines;
            };
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled(
                format!("forked session: {}", answer.fork_session_id),
                styles::subtle(),
            )));
            lines.push(Line::from(Span::raw("")));
            lines.extend(render_why_answer_lines(answer));
        }
        ExplainRunStatus::Failed => {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled(
                "Explain could not produce a valid answer.",
                Style::default()
                    .fg(styles::danger())
                    .add_modifier(Modifier::BOLD),
            )));
            if let Some(error) = &run.error {
                lines.push(Line::from(Span::raw(error.clone())));
            }
            lines.push(Line::from(Span::styled(
                format!(
                    "Press {} to retry, or press {} to switch models.",
                    key_status_label(app, KeybindingCommand::ExplainRetry),
                    key_status_label(app, KeybindingCommand::ExplainModel)
                ),
                styles::muted(),
            )));
        }
        ExplainRunStatus::Cancelled => {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled(
                "This explain run was cancelled before completion.",
                styles::muted(),
            )));
        }
    }

    lines
}

fn explain_context_source_label(app: &App) -> String {
    app.active_session()
        .map(|session| format!("{} ({})", session.title, session.id))
        .unwrap_or_else(|| "none selected".to_string())
}

pub(super) fn explain_scope_preview(app: &App) -> Option<String> {
    let file = app.review.files.get(app.review.cursor_file)?;
    if app.review.focus == ReviewFocus::Files || file.hunks.is_empty() {
        return Some(format!("file {}", file.display_label()));
    }

    let hunk = file.hunks.get(app.review.cursor_hunk)?;
    Some(format!("hunk {} {}", file.display_label(), hunk.header))
}

pub(super) fn explain_context_source_line(app: &App) -> String {
    app.active_session()
        .map(|session| format!("context: {} ({})", session.title, session.id))
        .unwrap_or_else(|| "context: none selected".to_string())
}

pub(super) fn loading_thinking_label(animation: &AnimatedTextState) -> String {
    let phase = (animation.frame / 24) % 4;
    let dots = ".".repeat(phase as usize);
    format!("Thinking{dots}")
}

pub(super) fn render_why_answer_lines(answer: &WhyAnswer) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.extend(render_why_section(
        "Summary:",
        styles::accent_bold(),
        &answer.summary,
    ));
    lines.extend(render_why_section(
        "Purpose:",
        styles::title(),
        &answer.purpose,
    ));
    lines.extend(render_why_section(
        "Change:",
        styles::title(),
        &answer.change,
    ));
    lines.extend(render_why_section(
        &format!("Risk ({}):", answer.risk_level.label()),
        Style::default()
            .fg(styles::danger())
            .add_modifier(Modifier::BOLD),
        &answer.risk_reason,
    ));
    lines
}

fn render_why_section(label: &str, label_style: Style, body: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(label.to_string(), label_style))];
    for line in body.lines() {
        lines.push(Line::from(Span::raw(line.to_string())));
    }
    lines.push(Line::from(Span::raw("")));
    lines
}

fn key_hint_span(app: &App, command: KeybindingCommand) -> Span<'static> {
    Span::styled(key_label(key_for(app, command)), styles::keybind())
}
