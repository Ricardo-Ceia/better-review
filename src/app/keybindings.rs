use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::settings::KeybindingsSettings;

use super::{App, KEYBINDING_COMMANDS, KeybindingCommand, key_label};

pub(super) fn command_binding(settings: &KeybindingsSettings, command: KeybindingCommand) -> char {
    command_binding_value(settings, command)
        .chars()
        .next()
        .filter(|ch| is_valid_keybinding_char(*ch))
        .unwrap_or_else(|| {
            command_binding_value(&KeybindingsSettings::default(), command)
                .chars()
                .next()
                .expect("default keybinding must not be empty")
        })
}

fn command_binding_value(settings: &KeybindingsSettings, command: KeybindingCommand) -> &str {
    match command {
        KeybindingCommand::Refresh => &settings.refresh,
        KeybindingCommand::Commit => &settings.commit,
        KeybindingCommand::Settings => &settings.settings,
        KeybindingCommand::Accept => &settings.accept,
        KeybindingCommand::Reject => &settings.reject,
        KeybindingCommand::Unreview => &settings.unreview,
        KeybindingCommand::Explain => &settings.explain,
        KeybindingCommand::ExplainContext => &settings.explain_context,
        KeybindingCommand::ExplainModel => &settings.explain_model,
        KeybindingCommand::ExplainHistory => &settings.explain_history,
        KeybindingCommand::ExplainRetry => &settings.explain_retry,
        KeybindingCommand::ExplainCancel => &settings.explain_cancel,
        KeybindingCommand::MoveDown => &settings.move_down,
        KeybindingCommand::MoveUp => &settings.move_up,
    }
}

pub(super) fn set_command_binding(
    settings: &mut KeybindingsSettings,
    command: KeybindingCommand,
    key: char,
) {
    let key = key.to_string();
    match command {
        KeybindingCommand::Refresh => settings.refresh = key,
        KeybindingCommand::Commit => settings.commit = key,
        KeybindingCommand::Settings => settings.settings = key,
        KeybindingCommand::Accept => settings.accept = key,
        KeybindingCommand::Reject => settings.reject = key,
        KeybindingCommand::Unreview => settings.unreview = key,
        KeybindingCommand::Explain => settings.explain = key,
        KeybindingCommand::ExplainContext => settings.explain_context = key,
        KeybindingCommand::ExplainModel => settings.explain_model = key,
        KeybindingCommand::ExplainHistory => settings.explain_history = key,
        KeybindingCommand::ExplainRetry => settings.explain_retry = key,
        KeybindingCommand::ExplainCancel => settings.explain_cancel = key,
        KeybindingCommand::MoveDown => settings.move_down = key,
        KeybindingCommand::MoveUp => settings.move_up = key,
    }
}

pub(super) fn command_label(command: KeybindingCommand) -> &'static str {
    match command {
        KeybindingCommand::Refresh => "Refresh changes",
        KeybindingCommand::Commit => "Commit accepted",
        KeybindingCommand::Settings => "Open settings",
        KeybindingCommand::Accept => "Accept change",
        KeybindingCommand::Reject => "Reject change",
        KeybindingCommand::Unreview => "Move to unreviewed",
        KeybindingCommand::Explain => "Open Explain",
        KeybindingCommand::ExplainContext => "Choose Explain context",
        KeybindingCommand::ExplainModel => "Choose Explain model",
        KeybindingCommand::ExplainHistory => "Open Explain history",
        KeybindingCommand::ExplainRetry => "Retry Explain",
        KeybindingCommand::ExplainCancel => "Cancel Explain",
        KeybindingCommand::MoveDown => "Move down",
        KeybindingCommand::MoveUp => "Move up",
    }
}

pub(super) fn key_for(app: &App, command: KeybindingCommand) -> char {
    command_binding(&app.settings.keybindings, command)
}

pub(super) fn key_status_label(app: &App, command: KeybindingCommand) -> String {
    key_label(key_for(app, command))
}

pub(super) fn key_matches(app: &App, key: KeyEvent, command: KeybindingCommand) -> bool {
    key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char(key_for(app, command))
}

pub(super) fn is_valid_keybinding_char(ch: char) -> bool {
    ch.is_ascii_lowercase()
}

pub(super) fn keybinding_conflict(
    settings: &KeybindingsSettings,
    command: KeybindingCommand,
    key: char,
) -> Option<KeybindingCommand> {
    KEYBINDING_COMMANDS
        .iter()
        .copied()
        .find(|candidate| *candidate != command && command_binding(settings, *candidate) == key)
}

pub(super) fn selected_keybinding_command(app: &App) -> KeybindingCommand {
    KEYBINDING_COMMANDS[app.keybinding_cursor.min(KEYBINDING_COMMANDS.len() - 1)]
}
