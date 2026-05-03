use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui_core::style::{Modifier, Style};

use crate::ui::styles;

use super::{
    App, KeybindingCommand, MODEL_CACHE_TTL, Message, Overlay, WhyModelChoice, centered_rect,
    close_explain_submenu, key_matches, save_settings,
};

pub(super) fn handle_model_picker_key(app: &mut App, key: KeyEvent) {
    let max_index = app.why_this.model.available.len();
    match key.code {
        KeyCode::Esc => {
            close_explain_submenu(app, "Closed the Explain model picker.");
        }
        KeyCode::Up => {
            app.why_this.model.cursor = app.why_this.model.cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.why_this.model.cursor = app.why_this.model.cursor.saturating_sub(1);
        }
        KeyCode::Down if app.why_this.model.cursor < max_index => {
            app.why_this.model.cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.why_this.model.cursor < max_index =>
        {
            app.why_this.model.cursor += 1;
        }
        KeyCode::Enter => {
            if app.why_this.model.cursor == 0 {
                app.why_this.model_override = Some(WhyModelChoice::Auto);
                app.status = format!("Explain model set to {}.", why_model_display_label(app));
            } else if let Some(model) = app
                .why_this
                .model
                .available
                .get(app.why_this.model.cursor - 1)
                .cloned()
            {
                app.why_this.model_override = Some(WhyModelChoice::Explicit(model.clone()));
                app.status = format!("Explain model set to {model}.");
            }
            if app.why_this.return_to_menu {
                app.overlay = Overlay::ExplainMenu;
            } else {
                app.overlay = Overlay::None;
            }
        }
        _ => {}
    }
}

pub(super) fn handle_saved_model_picker_key(app: &mut App, key: KeyEvent) {
    let max_index = app.why_this.model.available.len();
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::Settings;
            app.status = "Back to settings.".to_string();
        }
        KeyCode::Up => {
            app.saved_model_cursor = app.saved_model_cursor.saturating_sub(1);
        }
        _ if key_matches(app, key, KeybindingCommand::MoveUp) => {
            app.saved_model_cursor = app.saved_model_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.saved_model_cursor < max_index => {
            app.saved_model_cursor += 1;
        }
        _ if key_matches(app, key, KeybindingCommand::MoveDown)
            && app.saved_model_cursor < max_index =>
        {
            app.saved_model_cursor += 1;
        }
        KeyCode::Enter => {
            app.settings.explain.default_model = if app.saved_model_cursor == 0 {
                None
            } else {
                app.why_this
                    .model
                    .available
                    .get(app.saved_model_cursor - 1)
                    .cloned()
            };
            save_settings(app);
            sync_model_picker_cursors(app);
            app.overlay = Overlay::Settings;
            app.status = format!(
                "Default Explain model set to {}.",
                saved_model_label(&app.settings.explain.default_model)
            );
        }
        _ => {}
    }
}

pub(super) async fn open_model_picker(app: &mut App) {
    let Some(opencode) = app.opencode.clone() else {
        app.status =
            "Explain model selection is unavailable because opencode is not ready.".to_string();
        return;
    };

    app.overlay = Overlay::ModelPicker;
    app.why_this.model.cursor =
        model_picker_cursor(&current_model_choice(app), &app.why_this.model.available);

    let is_cache_fresh = app
        .why_this
        .model
        .last_loaded_at
        .is_some_and(|loaded_at| loaded_at.elapsed() < MODEL_CACHE_TTL);
    if is_cache_fresh && !app.why_this.model.available.is_empty() {
        app.status = model_picker_status_message(app.overlay).to_string();
        return;
    }

    if app.why_this.model.loading {
        app.status = "Loading Explain models...".to_string();
        return;
    }

    app.why_this.model.loading = true;
    app.status = "Loading Explain models...".to_string();

    let tx = app.tx.clone();
    tokio::spawn(async move {
        let result = opencode.list_models().await.map_err(|err| err.to_string());
        let _ = tx.send(Message::ModelList { result });
    });
}

pub(super) fn open_saved_model_picker(app: &mut App) {
    if app.opencode.is_none() {
        app.status =
            "Default Explain model selection is unavailable because opencode is not ready."
                .to_string();
        return;
    }

    app.overlay = Overlay::SettingsModelPicker;
    app.saved_model_cursor = saved_model_picker_cursor(
        app.settings.explain.default_model.as_deref(),
        &app.why_this.model.available,
    );

    let is_cache_fresh = app
        .why_this
        .model
        .last_loaded_at
        .is_some_and(|loaded_at| loaded_at.elapsed() < MODEL_CACHE_TTL);
    if is_cache_fresh && !app.why_this.model.available.is_empty() {
        app.status = model_picker_status_message(app.overlay).to_string();
        return;
    }

    if app.why_this.model.loading {
        app.status = "Loading Explain models...".to_string();
        return;
    }

    app.why_this.model.loading = true;
    app.status = "Loading Explain models...".to_string();

    let Some(opencode) = app.opencode.clone() else {
        return;
    };
    let tx = app.tx.clone();
    tokio::spawn(async move {
        let result = opencode.list_models().await.map_err(|err| err.to_string());
        let _ = tx.send(Message::ModelList { result });
    });
}

pub(super) fn draw_model_picker(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    draw_model_picker_modal(
        frame,
        area,
        app,
        Overlay::ModelPicker,
        app.why_this.model.cursor,
        current_model_choice(app),
    );
}

pub(super) fn draw_saved_model_picker(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    draw_model_picker_modal(
        frame,
        area,
        app,
        Overlay::SettingsModelPicker,
        app.saved_model_cursor,
        saved_model_choice(app),
    );
}

fn draw_model_picker_modal(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &App,
    overlay: Overlay,
    cursor: usize,
    selected_choice: WhyModelChoice,
) {
    let modal = centered_rect(62, 48, area);
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
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(inner);

    let mut rows = Vec::with_capacity(app.why_this.model.available.len() + 1);
    let title = match overlay {
        Overlay::ModelPicker => "Choose Explain model",
        Overlay::SettingsModelPicker => "Default Explain model",
        _ => unreachable!(),
    };
    let auto_label = match overlay {
        Overlay::ModelPicker => format!(" Auto ({})", auto_model_label(app)),
        Overlay::SettingsModelPicker => format!(
            " Auto ({})",
            app.why_this
                .model
                .auto_session_model
                .clone()
                .unwrap_or_else(|| "session default".to_string())
        ),
        _ => unreachable!(),
    };
    rows.push(model_picker_item(
        0,
        &auto_label,
        cursor,
        selected_choice == WhyModelChoice::Auto,
    ));

    for (index, model) in app.why_this.model.available.iter().enumerate() {
        rows.push(model_picker_item(
            index + 1,
            model,
            cursor,
            matches!(&selected_choice, WhyModelChoice::Explicit(selected) if selected == model),
        ));
    }

    if app.why_this.model.loading && app.why_this.model.available.is_empty() {
        rows.push(ListItem::new(Line::from(Span::styled(
            " Loading models...",
            styles::muted(),
        ))));
    }

    if let Some(error) = &app.why_this.model.last_error {
        rows.push(ListItem::new(Line::from(Span::styled(
            format!(" Error: {error}"),
            Style::default().fg(styles::danger()),
        ))));
        rows.push(ListItem::new(Line::from(Span::styled(
            " Close and reopen this picker to retry.",
            styles::muted(),
        ))));
    }

    let mut state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(
        List::new(rows).block(
            Block::default()
                .title(Line::from(Span::styled(title, styles::title())))
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

fn model_picker_item(
    index: usize,
    label: &str,
    cursor: usize,
    selected_value: bool,
) -> ListItem<'static> {
    let style = if index == cursor {
        Style::default()
            .fg(styles::text_primary())
            .bg(styles::accent_dim())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(styles::text_muted())
    };
    let marker = if selected_value { "[✓]" } else { "[ ]" };

    ListItem::new(Line::from(vec![
        Span::styled(format!(" {marker} "), style),
        Span::styled(label.to_string(), style),
    ]))
}

pub(super) fn saved_model_label(model: &Option<String>) -> String {
    model.clone().unwrap_or_else(|| "Auto".to_string())
}

fn saved_model_choice(app: &App) -> WhyModelChoice {
    match &app.settings.explain.default_model {
        Some(model) => WhyModelChoice::Explicit(model.clone()),
        None => WhyModelChoice::Auto,
    }
}

pub(super) fn explicit_model_choice(choice: &WhyModelChoice) -> Option<&str> {
    match choice {
        WhyModelChoice::Auto => None,
        WhyModelChoice::Explicit(model) => Some(model.as_str()),
    }
}

pub(super) fn saved_model_picker_cursor(saved_model: Option<&str>, models: &[String]) -> usize {
    match saved_model {
        None => 0,
        Some(model) => models
            .iter()
            .position(|candidate| candidate == model)
            .map_or(0, |index| index + 1),
    }
}

pub(super) fn ensure_model_present(models: &mut Vec<String>, model: Option<&str>) {
    let Some(model) = model else {
        return;
    };
    if !models.iter().any(|candidate| candidate == model) {
        models.insert(0, model.to_string());
    }
}

pub(super) fn sync_model_picker_cursors(app: &mut App) {
    app.why_this.model.cursor =
        model_picker_cursor(&current_model_choice(app), &app.why_this.model.available);
    app.saved_model_cursor = saved_model_picker_cursor(
        app.settings.explain.default_model.as_deref(),
        &app.why_this.model.available,
    );
}

pub(super) fn model_picker_status_message(overlay: Overlay) -> &'static str {
    match overlay {
        Overlay::ModelPicker => "Choose the Explain model, or keep Auto.",
        Overlay::SettingsModelPicker => "Choose the default Explain model, or keep Auto.",
        _ => "Choose a model.",
    }
}

pub(super) fn model_picker_cursor(choice: &WhyModelChoice, models: &[String]) -> usize {
    match choice {
        WhyModelChoice::Auto => 0,
        WhyModelChoice::Explicit(model) => {
            models
                .iter()
                .position(|candidate| candidate == model)
                .unwrap_or(0)
                + 1
        }
    }
}

pub(super) fn current_model_choice(app: &App) -> WhyModelChoice {
    app.why_this
        .model_override
        .clone()
        .unwrap_or(WhyModelChoice::Auto)
}

pub(super) fn resolved_why_model(app: &App) -> Option<String> {
    match current_model_choice(app) {
        WhyModelChoice::Auto => app
            .settings
            .explain
            .default_model
            .clone()
            .or_else(|| app.why_this.model.auto_session_model.clone()),
        WhyModelChoice::Explicit(model) => Some(model.clone()),
    }
}

pub(super) fn auto_model_label(app: &App) -> String {
    app.settings
        .explain
        .default_model
        .clone()
        .or_else(|| app.why_this.model.auto_session_model.clone())
        .unwrap_or_else(|| "session default".to_string())
}

pub(super) fn why_model_display_label(app: &App) -> String {
    match current_model_choice(app) {
        WhyModelChoice::Auto => format!("Auto ({})", auto_model_label(app)),
        WhyModelChoice::Explicit(model) => model,
    }
}
