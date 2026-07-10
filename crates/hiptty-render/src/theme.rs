use ratatui::style::{Color, Modifier, Style};

/// Synthwave-inspired neon palette for dark terminals (no app-drawn background).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub foreground: Color,
    pub secondary: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_bg: Color,
    /// Near-black wash behind modal overlays; darker than [`accent_bg`].
    pub modal_backdrop: Color,
    pub link: Color,
    pub success: Color,
    pub warn: Color,
    pub error: Color,
    pub logo_hi: Color,
    pub logo_lo: Color,
}

impl Palette {
    pub fn foreground_style(self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub fn secondary_style(self) -> Style {
        Style::default().fg(self.secondary)
    }

    pub fn muted_style(self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn accent_style(self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn link_style(self) -> Style {
        Style::default().fg(self.link)
    }

    pub fn success_style(self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn warn_style(self) -> Style {
        Style::default().fg(self.warn)
    }

    pub fn error_style(self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn title_style(self, title_color: Option<&str>) -> Style {
        let base = title_color
            .and_then(parse_hex_color)
            .unwrap_or(self.foreground);
        Style::default().fg(base)
    }

    pub fn selected_style(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn modal_backdrop_style(self) -> Style {
        Style::default()
            .fg(self.modal_backdrop)
            .bg(self.modal_backdrop)
    }

    pub fn modal_surface_style(self) -> Style {
        Style::default().bg(self.accent_bg)
    }

    /// Muted copy for page chrome drawn behind an open overlay (simulates translucency).
    pub fn dimmed(self) -> Self {
        const FACTOR: f32 = 0.28;
        Self {
            foreground: scale_color(self.foreground, FACTOR),
            secondary: scale_color(self.secondary, FACTOR),
            muted: scale_color(self.muted, FACTOR),
            accent: scale_color(self.accent, FACTOR * 1.1),
            accent_bg: self.accent_bg,
            modal_backdrop: self.modal_backdrop,
            link: scale_color(self.link, FACTOR),
            success: scale_color(self.success, FACTOR),
            warn: scale_color(self.warn, FACTOR),
            error: scale_color(self.error, FACTOR),
            logo_hi: scale_color(self.logo_hi, FACTOR),
            logo_lo: scale_color(self.logo_lo, FACTOR),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            foreground: Color::Rgb(224, 232, 240),
            secondary: Color::Rgb(132, 139, 189),
            muted: Color::Rgb(73, 84, 149),
            accent: Color::Rgb(255, 126, 219),
            accent_bg: Color::Rgb(54, 27, 47),
            modal_backdrop: Color::Rgb(22, 12, 28),
            link: Color::Rgb(54, 249, 246),
            success: Color::Rgb(114, 241, 184),
            warn: Color::Rgb(254, 222, 93),
            error: Color::Rgb(254, 68, 80),
            logo_hi: Color::Rgb(234, 0, 217),
            logo_lo: Color::Rgb(10, 189, 198),
        }
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

/// Logo breathing cycle in ticks (50ms each). ~3.0s full cycle (slow breath, not flash).
const LOGO_CYCLE: u64 = 60;
/// Per-character phase lag in ticks; small vs cycle so the wave is a soft shimmer.
const LOGO_CHAR_OFFSET: u64 = 2;

pub fn logo_color(tick: u64, palette: Palette) -> Color {
    let phase = (tick % LOGO_CYCLE) as f32 / LOGO_CYCLE as f32;
    let t = logo_wave(phase);
    lerp_color(palette.logo_hi, palette.logo_lo, t)
}

/// Per-character hue shift for title logo breathing effect.
pub fn logo_char_color(index: usize, tick: u64, palette: Palette) -> Color {
    let phase = ((tick + index as u64 * LOGO_CHAR_OFFSET) % LOGO_CYCLE) as f32 / LOGO_CYCLE as f32;
    let t = logo_wave(phase);
    lerp_color(palette.logo_hi, palette.logo_lo, t)
}

fn logo_wave(phase: f32) -> f32 {
    let t = phase.fract() * 2.0;
    if t < 1.0 {
        t
    } else {
        2.0 - t
    }
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
        _ => (234, 0, 217),
    }
}

fn scale_color(color: Color, factor: f32) -> Color {
    let (r, g, b) = color_to_rgb(color);
    let factor = factor.clamp(0.0, 1.0);
    Color::Rgb(
        (r as f32 * factor).round() as u8,
        (g as f32 * factor).round() as u8,
        (b as f32 * factor).round() as u8,
    )
}
