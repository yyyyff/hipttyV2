use hiptty_core::Theme;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub warn: Color,
    pub error: Color,
    pub quote_bg: Color,
    pub dim: Color,
    pub logo_hi: Color,
    pub logo_pda: Color,
}

impl Palette {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                primary: Color::Rgb(212, 212, 216),
                secondary: Color::Rgb(161, 161, 170),
                accent: Color::Rgb(129, 140, 248),
                warn: Color::Rgb(251, 191, 36),
                error: Color::Rgb(248, 113, 113),
                quote_bg: Color::Rgb(63, 63, 70),
                dim: Color::Rgb(113, 113, 122),
                logo_hi: Color::Rgb(167, 139, 250),
                logo_pda: Color::Rgb(156, 163, 175),
            },
            Theme::Light => Self {
                primary: Color::Rgb(39, 39, 42),
                secondary: Color::Rgb(82, 82, 91),
                accent: Color::Rgb(99, 102, 241),
                warn: Color::Rgb(217, 119, 6),
                error: Color::Rgb(220, 38, 38),
                quote_bg: Color::Rgb(228, 228, 231),
                dim: Color::Rgb(161, 161, 170),
                logo_hi: Color::Rgb(124, 58, 237),
                logo_pda: Color::Rgb(107, 114, 128),
            },
        }
    }

    pub fn primary_style(self) -> Style {
        Style::default().fg(self.primary)
    }

    pub fn secondary_style(self) -> Style {
        Style::default().fg(self.secondary)
    }

    pub fn accent_style(self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn dim_style(self) -> Style {
        Style::default().fg(self.dim)
    }

    pub fn error_style(self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn warn_style(self) -> Style {
        Style::default().fg(self.warn)
    }

    pub fn title_style(self, title_color: Option<&str>) -> Style {
        let base = title_color
            .and_then(parse_hex_color)
            .unwrap_or(self.primary);
        Style::default().fg(base)
    }

    pub fn selected_style(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
}

pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Logo breathing cycle in ticks (50ms each). ~0.8s full cycle for visible flow.
const LOGO_CYCLE: u64 = 16;
const LOGO_CHAR_OFFSET: u64 = 2;

pub fn logo_color(tick: u64, palette: Palette) -> Color {
    let phase = (tick % LOGO_CYCLE) as f32 / LOGO_CYCLE as f32;
    let t = logo_wave(phase);
    lerp_color(palette.logo_hi, palette.logo_pda, t)
}

/// Per-character hue shift for title logo breathing effect.
pub fn logo_char_color(index: usize, tick: u64, palette: Palette) -> Color {
    let phase = ((tick + index as u64 * LOGO_CHAR_OFFSET) % LOGO_CYCLE) as f32
        / LOGO_CYCLE as f32;
    let t = logo_wave(phase);
    lerp_color(palette.logo_hi, palette.logo_pda, t)
}

fn logo_wave(phase: f32) -> f32 {
    ((phase * std::f32::consts::PI * 2.0).sin() + 1.0) / 2.0
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = color_to_rgb(a);
    let (br, bg, bb) = color_to_rgb(b);
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (ar as f32 + (br as f32 - ar as f32) * t) as u8,
        (ag as f32 + (bg as f32 - ag as f32) * t) as u8,
        (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
    )
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (167, 139, 250),
    }
}
