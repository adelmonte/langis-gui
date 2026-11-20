//! Signal Desktop's look: its colors (the Axo design tokens in
//! `ts/axo/_tailwind-theme/colors.css`), Inter, and egui's widgets restyled
//! to match.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Theme};

/// The app's colors for one appearance.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Nav tabs and the chat list (`surface-secondary`).
    pub sidebar: Color32,
    /// The conversation and its composer (`surface-primary`).
    pub background: Color32,
    /// Menus and dialogs.
    pub popover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    /// Accent-colored text and links (`label-accent`).
    pub accent_text: Color32,
    pub bubble_in: Color32,
    pub bubble_out: Color32,
    /// Hover and pressed fills (`fill-primary`, `fill-primary-pressed`).
    pub fill: Color32,
    pub fill_pressed: Color32,
    pub fill_secondary: Color32,
    pub destructive: Color32,
    /// Desktop's unread badge (`fill-accent`).
    pub badge: Color32,
    /// Calls' background (`$calling-background-color`).
    pub calling: Color32,
    pub green: Color32,
}

pub const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// `rgb` at `alpha` percent, premultiplied as egui wants.
fn alpha(hex: u32, percent: u8) -> Color32 {
    let c = rgb(hex);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (percent as u32 * 255 / 100) as u8)
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            sidebar: rgb(0xf5f5f5),
            background: rgb(0xfafafa),
            popover: rgb(0xffffff),
            border: alpha(0x000000, 6),
            text: alpha(0x000000, 90),
            text_secondary: alpha(0x000000, 60),
            accent: rgb(0x2267f5),
            accent_text: rgb(0x030ffc),
            bubble_in: rgb(0xebebeb),
            bubble_out: rgb(0x2267f5),
            fill: alpha(0x7d7d7d, 8),
            fill_pressed: alpha(0x7d7d7d, 12),
            fill_secondary: alpha(0x7d7d7d, 16),
            destructive: rgb(0xf21602),
            badge: rgb(0x4655ff),
            calling: rgb(0x1b1b1b),
            green: rgb(0x4caf50),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            sidebar: rgb(0x1e1e1e),
            background: rgb(0x191919),
            popover: rgb(0x2a2a2a),
            border: alpha(0xffffff, 6),
            text: alpha(0xffffff, 90),
            text_secondary: alpha(0xffffff, 60),
            accent: rgb(0x2267f5),
            accent_text: rgb(0x99a1ff),
            bubble_in: rgb(0x323232),
            bubble_out: rgb(0x2267f5),
            fill: alpha(0x969696, 12),
            fill_pressed: alpha(0x969696, 16),
            fill_secondary: alpha(0x969696, 20),
            destructive: rgb(0xff4a3a),
            badge: rgb(0x5563ff),
            calling: rgb(0x1b1b1b),
            green: rgb(0x4caf50),
        }
    }

    pub fn of(ctx: &egui::Context) -> Self {
        if ctx.theme() == Theme::Dark { Self::dark() } else { Self::light() }
    }

    /// Text on an outgoing bubble, and its time.
    /// `.module-message__text--outgoing`: white at 90%.
    pub fn bubble_out_text(&self) -> Color32 {
        Color32::from_white_alpha(230)
    }

    /// label-secondary-oncolor: white at 80% (70% dark).
    pub fn bubble_out_meta(&self) -> Color32 {
        Color32::from_white_alpha(if self.dark { 179 } else { 204 })
    }
}

/// Desktop's avatar colors (`AxoTokens.Avatar`, A100 to A210): background
/// and initials.
pub const AVATAR_COLORS: [(u32, u32); 12] = [
    (0xe3e3fe, 0x3838f5),
    (0xdde7fc, 0x1251d3),
    (0xd8e8f0, 0x086da0),
    (0xcde4cd, 0x067906),
    (0xeae0fd, 0x661aff),
    (0xf5e3fe, 0x9f00f0),
    (0xf6d8ec, 0xb8057c),
    (0xf5d7d7, 0xbe0404),
    (0xfef5d0, 0x836b01),
    (0xeae6d5, 0x7d6f40),
    (0xd2d2dc, 0x4f4f6d),
    (0xd7d7d9, 0x5c5c5c),
];

pub fn avatar_colors(index: u8) -> (Color32, Color32) {
    let (bg, fg) = AVATAR_COLORS[index as usize % AVATAR_COLORS.len()];
    (rgb(bg), rgb(fg))
}

/// A group member's name color (Desktop's `ContactNameColors`, which
/// share the avatar hues): stable per person.
pub fn name_color(service_id: &str, dark: bool) -> Color32 {
    let hash = service_id.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let (bg, fg) = AVATAR_COLORS[hash as usize % 10];
    // The dark avatar background reads better on a dark bubble.
    if dark { rgb(bg) } else { rgb(fg) }
}

pub const MEDIUM: &str = "Inter Medium";
pub const SEMIBOLD: &str = "Inter SemiBold";
pub const ITALIC: &str = "Inter Italic";
/// Color emoji first, for emoji shown on their own (reactions, the picker)
/// and emoji runs in text, where Inter's plain hearts and stars would win.
pub const EMOJI: &str = "Emoji";
/// Desktop's SignalSymbols, the glyphs `AxoSymbol` draws (`symbols.rs`).
pub const SYMBOLS: &str = "Symbols";

pub fn emoji(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(EMOJI.into()))
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn symbols(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SYMBOLS.into()))
}

/// `AxoSymbol.Icon`: the named symbol, `size` square, centered on `center`.
pub fn paint_symbol(painter: &egui::Painter, name: &str, center: egui::Pos2, size: f32, color: Color32) {
    let Some(glyph) = crate::symbols::icon(name) else {
        debug_assert!(false, "no symbol {name}");
        return;
    };
    painter.text(center, egui::Align2::CENTER_CENTER, glyph, symbols(size), color);
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ITALIC.into()))
}

/// Where Linux distributions put a color emoji font.
const EMOJI_FONTS: [&str; 4] = [
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/NotoColorEmoji.ttf",
    "/usr/share/fonts/noto-emoji/NotoColorEmoji.ttf",
];

/// Inter (Desktop's font) first, then the system's color emoji if there
/// is one, then egui's own (monochrome emoji and symbols).
pub fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(bytes) = EMOJI_FONTS.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("Color Emoji".into(), FontData::from_owned(bytes).into());
        fallbacks.insert(0, "Color Emoji".into());
    }
    let faces: [(&str, &'static [u8]); 4] = [
        ("Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        (MEDIUM, include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        (SEMIBOLD, include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        (ITALIC, include_bytes!("../assets/fonts/Inter-Italic.ttf")),
    ];
    let mut emoji_family: Vec<String> = Vec::new();
    if fonts.font_data.contains_key("Color Emoji") {
        emoji_family.push("Color Emoji".into());
    }
    emoji_family.push("Inter".into());
    emoji_family.extend(fallbacks.iter().filter(|f| f.as_str() != "Color Emoji").cloned());
    fonts.families.insert(FontFamily::Name(EMOJI.into()), emoji_family);
    fonts.font_data.insert(SYMBOLS.into(), FontData::from_static(include_bytes!("../assets/fonts/SignalSymbols-Regular.ttf")).into());
    fonts.families.insert(FontFamily::Name(SYMBOLS.into()), vec![SYMBOLS.into()]);
    for (name, bytes) in faces {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
        let mut family = vec![name.to_owned()];
        family.extend(fallbacks.iter().cloned());
        if name == "Inter" {
            fonts.families.insert(FontFamily::Proportional, family);
        } else {
            fonts.families.insert(FontFamily::Name(name.into()), family);
        }
    }
    fonts
}

/// egui's widgets in Desktop's colors, for both appearances.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    for (theme, palette) in [(Theme::Light, Palette::light()), (Theme::Dark, Palette::dark())] {
        ctx.style_mut_of(theme, |style| {
            use egui::{FontFamily::Proportional, TextStyle};
            style.text_styles = [
                (TextStyle::Small, FontId::new(11., Proportional)),
                (TextStyle::Body, FontId::new(14., Proportional)),
                (TextStyle::Button, FontId::new(14., Proportional)),
                (TextStyle::Heading, FontId::new(18., FontFamily::Name(SEMIBOLD.into()))),
                (TextStyle::Monospace, FontId::monospace(13.)),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(8., 6.);
            style.spacing.button_padding = egui::vec2(12., 6.);
            style.spacing.interact_size.y = 28.;
            style.interaction.selectable_labels = false;
            let v = &mut style.visuals;
            v.dark_mode = palette.dark;
            v.override_text_color = Some(palette.text);
            v.panel_fill = palette.background;
            v.window_fill = palette.popover;
            v.extreme_bg_color = palette.bubble_in;
            v.faint_bg_color = palette.fill;
            v.window_stroke = Stroke::new(1., palette.border);
            v.window_corner_radius = CornerRadius::same(12);
            v.menu_corner_radius = CornerRadius::same(10);
            v.hyperlink_color = palette.accent_text;
            v.selection.bg_fill = palette.accent.gamma_multiply(0.35);
            // Selected text keeps its color, as in Chromium.
            v.selection.stroke = Stroke::new(1., palette.text);
            v.text_cursor.stroke = Stroke::new(1.5, palette.accent);
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.corner_radius = CornerRadius::same(8);
                w.bg_stroke = Stroke::NONE;
                w.fg_stroke = Stroke::new(1., palette.text);
            }
            v.widgets.noninteractive.fg_stroke = Stroke::new(1., palette.text);
            v.widgets.noninteractive.bg_stroke = Stroke::new(1., palette.border);
            v.widgets.inactive.weak_bg_fill = palette.fill;
            v.widgets.inactive.bg_fill = palette.fill_secondary;
            v.widgets.hovered.weak_bg_fill = palette.fill_pressed;
            v.widgets.hovered.bg_fill = palette.fill_secondary;
            v.widgets.active.weak_bg_fill = palette.fill_secondary;
            v.widgets.active.bg_fill = palette.accent;
            v.widgets.open.weak_bg_fill = palette.fill_pressed;
        });
    }
}
