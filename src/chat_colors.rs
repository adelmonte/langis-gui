//! Chat colors: what our bubbles are filled with. Desktop's presets
//! (`ts/axo/AxoTokens.std.ts`, `ChatBubble`), custom colors from their hue
//! and saturation (`ts/sql/migrations/1870-account-and-chat-colors`), and
//! gradients drawn as `background-attachment: fixed` does
//! (`stylesheets/_modules.scss`): one gradient across the window, each
//! bubble showing its part.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect};
use langis_store::{ChatColor, ChatColors, ConversationId, CustomColor, HueSaturation};

use crate::app::{App, Loaded};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bubble {
    Solid(Color32),
    /// A CSS `linear-gradient` angle in degrees, then its two ends.
    Gradient(f32, Color32, Color32),
}

/// Desktop's default, `ultramarine`.
pub const DEFAULT: Bubble = Bubble::Gradient(180., crate::theme::rgb(0x0552f0), crate::theme::rgb(0x2c6bed));

pub fn preset(name: &str) -> Option<Bubble> {
    use crate::theme::rgb;
    let solid = |hex| Some(Bubble::Solid(rgb(hex)));
    let gradient = |deg, a, b| Some(Bubble::Gradient(deg, rgb(a), rgb(b)));
    match name {
        "crimson" => solid(0xcf163e),
        "vermilion" => solid(0xc73f0a),
        "burlap" => solid(0x6f6a58),
        "forest" => solid(0x3b7845),
        "wintergreen" => solid(0x1d8663),
        "teal" => solid(0x077d92),
        "blue" => solid(0x336ba3),
        "indigo" => solid(0x6058ca),
        "violet" => solid(0x9932c8),
        "plum" => solid(0xaa377a),
        "taupe" => solid(0x8f616a),
        "steel" => solid(0x71717f),
        "ultramarine" => gradient(180., 0x0552f0, 0x2c6bed),
        "basil" => gradient(180., 0x2f9373, 0x077343),
        "ember" => gradient(168., 0xe57c00, 0x5e0000),
        "fluorescent" => gradient(192., 0xec13dd, 0x1b36c6),
        "infrared" => gradient(192., 0xf65560, 0x442ced),
        "lagoon" => gradient(180., 0x004066, 0x32867d),
        "midnight" => gradient(180., 0x2c2c3a, 0x787891),
        "sea" => gradient(180., 0x498fd4, 0x2c66a0),
        "sublime" => gradient(180., 0x6281d5, 0x974460),
        "tangerine" => gradient(192., 0xdb7133, 0x911231),
        _ => None,
    }
}

/// Desktop's `calculateLightness`: a custom color's lightness from its hue,
/// in percent.
fn lightness_from_hue(hue: f32) -> f32 {
    let between = |hue: f32, min: f32, max: f32, lo: f32, hi: f32| lo + (hue - min) / (max - min) * (hi - lo);
    if hue < 60. {
        between(hue, 0., 60., 45., 30.)
    } else if hue < 180. {
        30.
    } else if hue < 240. {
        between(hue, 180., 240., 30., 50.)
    } else if hue < 300. {
        between(hue, 240., 300., 50., 40.)
    } else {
        between(hue, 300., 360., 40., 45.)
    }
}

pub fn hsl(c: HueSaturation) -> Color32 {
    let l = c.lightness.map(|l| l * 100.).unwrap_or_else(|| lightness_from_hue(c.hue)) / 100.;
    let s = (c.saturation / 100.).clamp(0., 1.);
    let h = c.hue.rem_euclid(360.) / 60.;
    let chroma = (1. - (2. * l - 1.).abs()) * s;
    let x = chroma * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = l - chroma / 2.;
    let byte = |v: f32| ((v + m).clamp(0., 1.) * 255.).round() as u8;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

/// A custom color: solid, or a gradient at Desktop's own angle, which CSS
/// gets as `270 - deg`.
pub fn custom(c: &CustomColor) -> Bubble {
    match c.end {
        Some(end) => Bubble::Gradient(270. - c.deg.unwrap_or(180.), hsl(c.start), hsl(end)),
        None => Bubble::Solid(hsl(c.start)),
    }
}

pub fn of(color: &ChatColor, all: &ChatColors) -> Bubble {
    match color {
        ChatColor::Preset { name } => preset(name).unwrap_or(DEFAULT),
        ChatColor::Custom { id, value } => custom(&all.custom.get(id).unwrap_or(*value)),
    }
}

/// A chat's bubbles: its own color, else the default.
pub fn for_chat(app: &App, chat: &ConversationId) -> Bubble {
    of(app.chat_colors.of(chat), &app.chat_colors)
}

pub fn load(app: &App) {
    app.load(|db| async move {
        let colors = db.chat_colors().await.ok()?;
        Some(Loaded::Apply(Box::new(move |app: &mut App| app.chat_colors = colors)))
    });
}

/// Fills `rect` with `bubble`. A gradient runs across `fixed` (the window)
/// and the bubble shows its part.
pub fn paint(painter: &Painter, rect: Rect, radius: CornerRadius, bubble: Bubble, fixed: Rect) {
    match bubble {
        Bubble::Solid(color) => {
            painter.rect_filled(rect, radius, color);
        }
        Bubble::Gradient(deg, start, end) => {
            let mut mesh = egui::Mesh::default();
            let ctx = painter.ctx();
            let mut tessellator = egui::epaint::Tessellator::new(
                ctx.pixels_per_point(),
                ctx.tessellation_options(|o| *o),
                ctx.fonts(|f| f.font_image_size()),
                Vec::new(),
            );
            tessellator.tessellate_rect(&egui::epaint::RectShape::filled(rect, radius, Color32::WHITE), &mut mesh);
            let (dir, len) = line(deg, fixed);
            let center = fixed.center();
            for v in &mut mesh.vertices {
                let t = ((v.pos - center).dot(dir) / len + 0.5).clamp(0., 1.);
                let color = lerp(start, end, t);
                // The edge's feathering keeps its fade.
                v.color = color.gamma_multiply(v.color.a() as f32 / 255.);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

/// A CSS gradient line through `rect`: its direction (0deg up, 90deg
/// right) and length, so the corners land on 0 and 1.
fn line(deg: f32, rect: Rect) -> (egui::Vec2, f32) {
    let a = deg.to_radians();
    let dir = egui::vec2(a.sin(), -a.cos());
    let len = (rect.width() * a.sin()).abs() + (rect.height() * a.cos()).abs();
    (dir, len.max(1.))
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// The picker's swatch: the whole gradient inside a circle.
pub fn paint_swatch(painter: &Painter, center: Pos2, radius: f32, bubble: Bubble) {
    let rect = Rect::from_center_size(center, egui::Vec2::splat(radius * 2.));
    paint(painter, rect, CornerRadius::same(radius.round() as u8), bubble, rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_colors_take_desktops_lightness() {
        assert_eq!(lightness_from_hue(0.), 45.);
        assert_eq!(lightness_from_hue(120.), 30.);
        assert_eq!(lightness_from_hue(240.), 50.);
        assert_eq!(hsl(HueSaturation { hue: 0., saturation: 100., lightness: Some(0.5) }), Color32::from_rgb(255, 0, 0));
        assert_eq!(hsl(HueSaturation { hue: 120., saturation: 0., lightness: Some(1.) }), Color32::WHITE);
    }

    #[test]
    fn a_gradients_ends_land_on_the_corners() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100., 50.));
        let (dir, len) = line(180., rect);
        assert!((rect.center_top() - rect.center()).dot(dir) / len + 0.5 < 0.001);
        assert!(((rect.center_bottom() - rect.center()).dot(dir) / len + 0.5 - 1.).abs() < 0.001);
    }
}
