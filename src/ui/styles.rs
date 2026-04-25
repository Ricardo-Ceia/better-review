use ratatui_core::style::{Color, Modifier, Style};

pub const BASE_BG: Color = Color::Rgb(0, 0, 0);
pub const SURFACE: Color = Color::Rgb(10, 8, 18);
pub const SURFACE_RAISED: Color = Color::Rgb(21, 16, 39);
pub const BORDER_MUTED: Color = Color::Rgb(47, 47, 47);
pub const TEXT_PRIMARY: Color = Color::Rgb(205, 205, 205);
pub const TEXT_MUTED: Color = Color::Rgb(133, 133, 133);
pub const TEXT_SUBTLE: Color = Color::Rgb(85, 85, 85);
pub const ACCENT: Color = Color::Rgb(105, 48, 199);
pub const ACCENT_BRIGHT: Color = Color::Rgb(221, 181, 248);
pub const ACCENT_DIM: Color = Color::Rgb(58, 47, 102);
pub const SUCCESS: Color = Color::Rgb(184, 184, 184);
pub const DANGER: Color = Color::Rgb(147, 147, 147);

pub fn title() -> Style {
    Style::default()
        .fg(TEXT_PRIMARY)
        .add_modifier(Modifier::BOLD)
}

pub fn accent_bold() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn keybind() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bold(style: Style) {
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn title_style_matches_palette() {
        let style = title();
        assert_eq!(style.fg, Some(TEXT_PRIMARY));
        assert_bold(style);
    }

    #[test]
    fn accent_bold_style_matches_palette() {
        let style = accent_bold();
        assert_eq!(style.fg, Some(ACCENT));
        assert_bold(style);
    }

    #[test]
    fn keybind_style_matches_palette() {
        let style = keybind();
        assert_eq!(style.fg, Some(ACCENT));
        assert_bold(style);
    }

    #[test]
    fn soft_accent_style_matches_palette() {
        let style = soft_accent();
        assert_eq!(style.fg, Some(ACCENT));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn muted_style_matches_palette() {
        let style = muted();
        assert_eq!(style.fg, Some(TEXT_MUTED));
    }

    #[test]
    fn subtle_style_matches_palette() {
        let style = subtle();
        assert_eq!(style.fg, Some(TEXT_SUBTLE));
    }
}
