use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{TextArea, WrapMode};

pub(super) fn new_commit_message_input() -> TextArea<'static> {
    let mut commit_message = TextArea::default();
    commit_message.set_placeholder_text("Write the commit message for accepted changes");
    commit_message.set_wrap_mode(WrapMode::WordOrGlyph);
    commit_message
}

pub(super) fn new_github_token_input() -> TextArea<'static> {
    new_github_token_input_with_value("")
}

pub(super) fn new_github_token_input_with_value(value: &str) -> TextArea<'static> {
    let mut input = TextArea::new(vec![value.to_string()]);
    input.set_placeholder_text("Paste a GitHub token with repository write access");
    input.set_mask_char('*');
    input
}

pub(super) fn to_textarea_input(key: KeyEvent) -> ratatui_textarea::Input {
    ratatui_textarea::Input {
        key: match key.code {
            KeyCode::Backspace => ratatui_textarea::Key::Backspace,
            KeyCode::Enter => ratatui_textarea::Key::Enter,
            KeyCode::Left => ratatui_textarea::Key::Left,
            KeyCode::Right => ratatui_textarea::Key::Right,
            KeyCode::Up => ratatui_textarea::Key::Up,
            KeyCode::Down => ratatui_textarea::Key::Down,
            KeyCode::Home => ratatui_textarea::Key::Home,
            KeyCode::End => ratatui_textarea::Key::End,
            KeyCode::PageUp => ratatui_textarea::Key::PageUp,
            KeyCode::PageDown => ratatui_textarea::Key::PageDown,
            KeyCode::Delete => ratatui_textarea::Key::Delete,
            KeyCode::Char(ch) => ratatui_textarea::Key::Char(ch),
            KeyCode::Tab => ratatui_textarea::Key::Tab,
            _ => ratatui_textarea::Key::Null,
        },
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}
