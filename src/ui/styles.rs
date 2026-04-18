use ratatui_core::style::{Color, Modifier, Style};

pub const BASE_BG: Color = Color::Rgb(16, 18, 19);
pub const SURFACE: Color = Color::Rgb(20, 23, 23);
pub const SURFACE_RAISED: Color = Color::Rgb(28, 32, 31);
pub const BORDER_MUTED: Color = Color::Rgb(58, 65, 63);
pub const TEXT_PRIMARY: Color = Color::Rgb(232, 228, 219);
pub const TEXT_MUTED: Color = Color::Rgb(167, 163, 154);
pub const TEXT_SUBTLE: Color = Color::Rgb(109, 107, 101);
pub const ACCENT: Color = Color::Rgb(96, 170, 150);
pub const ACCENT_BRIGHT: Color = Color::Rgb(165, 223, 206);
pub const ACCENT_DIM: Color = Color::Rgb(54, 90, 81);
pub const SUCCESS: Color = Color::Rgb(126, 176, 147);
pub const DANGER: Color = Color::Rgb(172, 116, 118);

pub fn title() -> Style {
    Style::default()
        .fg(TEXT_PRIMARY)
        .add_modifier(Modifier::BOLD)
}

pub fn accent_bold() -> Style {
    Style::default()
        .fg(ACCENT_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

pub fn keybind() -> Style {
    Style::default()
        .fg(ACCENT_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

pub fn soft_accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn muted() -> Style {
    Style::default().fg(TEXT_MUTED)
}

pub fn subtle() -> Style {
    Style::default().fg(TEXT_SUBTLE)
}
