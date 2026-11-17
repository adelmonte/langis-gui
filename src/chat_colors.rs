//! Chat colors: what our bubbles are filled with. Desktop's presets
//! (`ts/axo/AxoTokens.std.ts`, `ChatBubble`), custom colors from their hue
//! and saturation (`ts/sql/migrations/1870-account-and-chat-colors`), and
//! gradients drawn as `background-attachment: fixed` does
//! (`stylesheets/_modules.scss`): one gradient across the window, each
//! bubble showing its part.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect};
use langis_store::{ChatColor, ChatColors, ConversationId, CustomColor, HueSaturation};

use langis_core::Command;

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

/// `SampleMessageBubbles`, on the page's color in a bordered card. Returns
/// the card.
fn sample_bubbles(ui: &mut egui::Ui, palette: &crate::theme::Palette, selected: Bubble) -> Rect {
    use egui::{Sense, Stroke, Vec2};

    use crate::theme;
    egui::Frame::new()
        .fill(palette.background)
        .stroke(Stroke::new(0.5, palette.border))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(egui::Margin::symmetric(0, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let width = ui.available_width();
            let samples = [
                (true, "Here's a preview of the chat color.", "4h"),
                (false, "Another bubble.", "2h"),
                (true, "The color is visible to only you.", "Now"),
            ];
            let fixed = ui.max_rect();
            for (i, (outgoing, text, time)) in samples.iter().enumerate() {
                if i > 0 {
                    ui.add_space(12.);
                }
                let color = if *outgoing { palette.bubble_out_text() } else { palette.text };
                let meta = if *outgoing { palette.bubble_out_meta() } else { palette.text_secondary };
                let g = ui.painter().layout((*text).to_owned(), theme::regular(14.), color, width * 0.6);
                let t = ui.painter().layout_no_wrap((*time).to_owned(), theme::regular(11.), meta);
                let w = g.size().x + 8. + t.size().x + 24.;
                let h = g.size().y + 16.;
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
                let x = if *outgoing { row.right() - 16. - w } else { row.left() + 16. };
                let bubble = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(w, h));
                if *outgoing {
                    paint(ui.painter(), bubble, CornerRadius::same(18), selected, fixed);
                } else {
                    ui.painter().rect_filled(bubble, CornerRadius::same(18), palette.bubble_in);
                }
                let gh = g.size().y;
                ui.painter().galley(Pos2::new(bubble.left() + 12., bubble.top() + 8.), g, color);
                ui.painter().galley(Pos2::new(bubble.right() - 12. - t.size().x, bubble.top() + 8. + gh - t.size().y), t, meta);
            }
        })
        .response
        .rect
}

/// Desktop's `ChatColorPicker`: a preview, the presets and custom colors,
/// and resetting. For one chat (`chat`), or the default for all.
pub fn picker(app: &mut App, ui: &mut egui::Ui, chat: Option<ConversationId>) {
    use egui::{Sense, Stroke, Vec2};
    use langis_store::ChatColorEdit;

    use crate::axo_list::{self, End, Item};
    use crate::theme;

    let palette = crate::theme::Palette::of(ui.ctx());
    let colors = app.chat_colors.clone();
    let current = match &chat {
        Some(chat) => colors.of(chat).clone(),
        None => colors.default.clone(),
    };
    let selected = of(&current, &colors);
    sample_bubbles(ui, &palette, selected);
    ui.add_space(16.);
    let choose = |app: &mut App, color: ChatColor| {
        let edit = match &chat {
            Some(chat) => ChatColorEdit::Chat(chat.clone(), Some(color)),
            None => ChatColorEdit::Default(color),
        };
        app.send(Command::EditChatColors(edit));
    };
    axo_list::list(ui, None, None, |ui| {
        // `ChatColorPicker__bubbles`: 52px swatches 24px apart, centered.
        let mut swatches: Vec<(ChatColor, Bubble)> =
            langis_store::CHAT_COLOR_PRESETS.iter().map(|name| (ChatColor::preset(name), preset(name).unwrap_or(DEFAULT))).collect();
        for (id, value) in &colors.custom.colors {
            swatches.push((ChatColor::Custom { id: id.clone(), value: *value }, custom(value)));
        }
        let width = ui.available_width();
        let per_row = (((width + 24.) / (52. + 24.)).floor() as usize).max(1);
        // The swatches, then Desktop's "+" for a new custom color.
        let count = swatches.len() + 1;
        let rows = count.div_ceil(per_row);
        ui.add_space(20.);
        let (area, _) = ui.allocate_exact_size(Vec2::new(width, rows as f32 * 52. + (rows - 1) as f32 * 24.), Sense::hover());
        ui.add_space(20.);
        let row_w = per_row as f32 * 52. + (per_row - 1) as f32 * 24.;
        let left = area.left() + (width - row_w) / 2.;
        let ring = if palette.dark { Color32::WHITE } else { theme::rgb(0x3b3b3b) };
        let at = |i: usize| Rect::from_min_size(Pos2::new(left + (i % per_row) as f32 * 76., area.top() + (i / per_row) as f32 * 76.), Vec2::splat(52.));
        for (i, (color, bubble)) in swatches.iter().enumerate() {
            let rect = at(i);
            let response = ui.interact(rect, egui::Id::unique("chat-color-swatch").with(i), Sense::click());
            let chosen = match (color, &current) {
                (ChatColor::Preset { name: a }, ChatColor::Preset { name: b }) => a == b,
                (ChatColor::Custom { id: a, .. }, ChatColor::Custom { id: b, .. }) => a == b,
                _ => false,
            };
            if chosen {
                ui.painter().circle_stroke(rect.center(), 24., Stroke::new(4., ring));
            }
            paint_swatch(ui.painter(), rect.center(), 20., *bubble);
            if response.clicked() && !chosen {
                choose(app, color.clone());
            }
            // `CustomColorBubble`'s menu.
            if let ChatColor::Custom { id, value } = color {
                crate::menu::context(ui, &response, egui::Id::unique("custom-color-menu").with(id), |m| {
                    if m.item(Some("pencil"), "Edit color") {
                        app.color_editor = Some(Editor::new(Some(id.clone()), chat.clone(), *value));
                    }
                    if m.item(Some("copy"), "Duplicate") {
                        app.send(Command::EditChatColors(ChatColorEdit::AddCustom { id: crate::settings::new_id(), color: *value, chat: chat.clone() }));
                    }
                    if m.item(Some("trash"), "Delete") {
                        let used = colors.chats_with(id) + usize::from(matches!(&colors.default, ChatColor::Custom { id: d, .. } if d == id));
                        if used == 0 {
                            app.send(Command::EditChatColors(ChatColorEdit::RemoveCustom { id: id.clone() }));
                        } else {
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("custom-color-delete"), (id.clone(), used)));
                        }
                    }
                });
            }
            response.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        let plus = at(swatches.len());
        let response = ui.interact(plus, egui::Id::unique("chat-color-add"), Sense::click());
        ui.painter().circle_filled(plus.center(), 26., if response.hovered() { palette.fill_secondary } else { palette.fill });
        theme::paint_symbol(ui.painter(), "plus", plus.center(), 24., palette.text);
        if response.on_hover_text("Show custom color editor").clicked() {
            app.color_editor = Some(Editor::new(None, chat.clone(), Editor::INITIAL));
        }
    });
    let delete = egui::Id::unique("custom-color-delete");
    if let Some((id, used)) = ui.ctx().data(|d| d.get_temp::<(String, usize)>(delete)) {
        use crate::dialog::{self, Choice, Variant};
        let body = if used == 1 { "This custom color is used in 1 chat. Do you want to delete it for all chats?".to_owned() } else { format!("This custom color is used in {used} chats. Do you want to delete it for all chats?") };
        let answer = dialog::confirm(ui.ctx(), delete.with("dialog"), "Delete color", &body, "Delete", Variant::StrongDestructive);
        if answer == Some(Choice::Action) {
            app.send(Command::EditChatColors(ChatColorEdit::RemoveCustom { id }));
        }
        if answer.is_some() {
            ui.ctx().data_mut(|d| d.remove::<(String, usize)>(delete));
        }
    }
    axo_list::gap(ui);
    let ask = egui::Id::unique("chat-color-reset");
    axo_list::list(ui, None, None, |ui| {
        if let Some(chat) = &chat
            && axo_list::item(ui, Item::new("Reset chat color"), End::Click).clicked()
        {
            app.send(Command::EditChatColors(ChatColorEdit::Chat(chat.clone(), None)));
        }
        if axo_list::item(ui, Item::new("Reset all chat colors"), End::Click).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(ask, true));
        }
    });
    if ui.ctx().data(|d| d.get_temp::<bool>(ask)).unwrap_or(false) {
        use crate::dialog::{self, Answer, Variant};
        let message = "Would you like to override all chat colors?";
        let answer = if chat.is_none() {
            // Desktop asks whether the default goes too.
            dialog::show(ui.ctx(), ask.with("dialog"), "Reset Chat Colors", message, Some("Cancel"), &[("Reset default colors", Variant::StrongPrimary), ("Reset all chat colors", Variant::StrongPrimary)])
        } else {
            dialog::show(ui.ctx(), ask.with("dialog"), "Reset all chat colors", message, Some("Cancel"), &[("Reset", Variant::StrongPrimary)])
        };
        match (answer, chat.is_none()) {
            (Some(Answer::Action(0)), true) => app.send(Command::EditChatColors(ChatColorEdit::ResetDefault)),
            (Some(Answer::Action(_)), true) => {
                app.send(Command::EditChatColors(ChatColorEdit::ResetDefault));
                app.send(Command::EditChatColors(ChatColorEdit::ResetChats));
            }
            (Some(Answer::Action(_)), false) => app.send(Command::EditChatColors(ChatColorEdit::ResetChats)),
            _ => {}
        }
        if answer.is_some() {
            ui.ctx().data_mut(|d| d.remove::<bool>(ask));
        }
    }
}

/// A color's hue and saturation (lightness comes from the hue, as
/// Desktop's `withCalculatedLightness` does).
#[cfg(test)]
fn hue_saturation(c: Color32) -> HueSaturation {
    let (r, g, b) = (c.r() as f32 / 255., c.g() as f32 / 255., c.b() as f32 / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.;
    let d = max - min;
    if d == 0. {
        return HueSaturation { hue: 0., saturation: 0., lightness: None };
    }
    let s = d / (1. - (2. * l - 1.).abs());
    let h = if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    };
    HueSaturation { hue: h, saturation: (s * 100.).clamp(0., 100.), lightness: None }
}

/// Desktop's `CustomColorEditorDialog`, open.
#[derive(Clone, Debug)]
pub struct Editor {
    /// The custom color being changed, or none for a new one.
    id: Option<String>,
    /// The chat whose picker it came from, which a new color is chosen for.
    chat: Option<ConversationId>,
    gradient: bool,
    start: HueSaturation,
    end: HueSaturation,
    /// Desktop's dial angle: counterclockwise from the right.
    deg: f32,
    /// Which knob the sliders change.
    end_knob: bool,
}

impl Editor {
    /// `INITIAL_CUSTOM_CHAT_COLOR`: ultramarine's ends, at `270 - 180`.
    const INITIAL: CustomColor = CustomColor {
        start: HueSaturation { hue: 220.5, saturation: 95.9, lightness: None },
        end: Some(HueSaturation { hue: 220.4, saturation: 84.3, lightness: None }),
        deg: Some(90.),
    };

    fn new(id: Option<String>, chat: Option<ConversationId>, c: CustomColor) -> Self {
        let fallback = Self::INITIAL.end.unwrap_or(c.start);
        Editor { id, chat, gradient: c.end.is_some(), start: c.start, end: c.end.unwrap_or(fallback), deg: c.deg.unwrap_or(90.), end_knob: false }
    }

    /// A new color's editor, for the screenshots.
    #[cfg(test)]
    pub fn sample() -> Self {
        let mut ed = Editor::new(None, None, Editor::INITIAL);
        ed.end = HueSaturation { hue: 300., saturation: 80., lightness: None };
        ed.deg = 45.;
        ed.end_knob = true;
        ed
    }

    fn color(&self) -> CustomColor {
        let plain = |c: HueSaturation| HueSaturation { lightness: None, ..c };
        if self.gradient {
            CustomColor { start: plain(self.start), end: Some(plain(self.end)), deg: Some(self.deg) }
        } else {
            CustomColor { start: plain(self.start), end: None, deg: None }
        }
    }
}

/// `AxoColorSlider`: a 10px track and a 20px thumb with a white ring.
/// Returns the new value, 0 to 1, while dragged.
fn color_slider(ui: &mut egui::Ui, id: egui::Id, value: f32, track: &[Color32], thumb: Color32) -> Option<f32> {
    use egui::{Sense, Stroke, Vec2};
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.), Sense::click_and_drag());
    let _ = id;
    let bar = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 20., 10.));
    let n = track.len().max(2) - 1;
    ui.painter().circle_filled(Pos2::new(bar.left(), bar.center().y), 5., track[0]);
    ui.painter().circle_filled(Pos2::new(bar.right(), bar.center().y), 5., track[n]);
    for i in 0..n {
        let x0 = bar.left() + bar.width() * i as f32 / n as f32;
        let x1 = bar.left() + bar.width() * (i + 1) as f32 / n as f32;
        let seg = Rect::from_min_max(Pos2::new(x0, bar.top()), Pos2::new(x1 + 0.5, bar.bottom()));
        paint(ui.painter(), seg, CornerRadius::ZERO, Bubble::Gradient(90., track[i], track[i + 1]), seg);
    }
    let x = bar.left() + bar.width() * value.clamp(0., 1.);
    let center = Pos2::new(x, bar.center().y);
    ui.painter().circle_filled(center + Vec2::new(0., 1.), 10.5, Color32::from_black_alpha(30));
    ui.painter().circle_filled(center, 10., thumb);
    ui.painter().circle_stroke(center, 9., Stroke::new(2., Color32::WHITE));
    if (response.dragged() || response.clicked())
        && let Some(p) = response.interact_pointer_pos()
    {
        return Some(((p.x - bar.left()) / bar.width()).clamp(0., 1.));
    }
    None
}

/// The open custom color editor, and what Save does.
pub fn editor(app: &mut App, ctx: &egui::Context) {
    use egui::{Align2, Order, Sense, Stroke, Vec2};
    use langis_store::ChatColorEdit;

    use crate::theme::{self, Palette};

    let Some(mut ed) = app.color_editor.clone() else { return };
    let palette = Palette::of(ctx);
    let screen = ctx.content_rect();
    let overlay = if palette.dark { Color32::from_black_alpha(122) } else { Color32::from_black_alpha(61) };
    egui::Area::new(egui::Id::unique("color-editor-overlay")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0, overlay);
    });
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut save = false;
    // AxoDialog `md`: 400 wide.
    let width = 400f32.min(screen.width() - 32.);
    let id = egui::Id::unique("color-editor");
    let last_h = ctx.data(|d| d.get_temp::<f32>(id.with("height"))).unwrap_or(560.);
    let pos = Pos2::new(screen.center().x - width / 2., (screen.center().y - last_h / 2.).max(16.));
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let shadow = egui::epaint::Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 61 } else { 31 }) };
    let shown = egui::Area::new(id).order(Order::Tooltip).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).shadow(shadow).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // Header: the title, centered, and the close button.
            ui.add_space(12.);
            let (head, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            ui.painter().text(head.center(), Align2::CENTER_CENTER, "Custom Color", theme::semibold(14.), palette.text);
            let x = Rect::from_center_size(Pos2::new(head.right() - 16. - 16., head.center().y), Vec2::splat(32.));
            let xr = ui.interact(x, id.with("close"), Sense::click());
            if xr.hovered() {
                ui.painter().circle_filled(x.center(), 16., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "x", x.center(), 20., palette.text);
            close |= xr.on_hover_text("Close").clicked();
            ui.add_space(10.);
            // `AxoDialog.TabList`: a segmented control on a track.
            let (track, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            let track = track.shrink2(Vec2::new(16., 0.));
            ui.painter().rect_filled(track, CornerRadius::same(16), palette.fill);
            for (i, label) in ["Solid", "Gradient"].iter().enumerate() {
                let half = Rect::from_min_size(Pos2::new(track.left() + i as f32 * track.width() / 2., track.top()), Vec2::new(track.width() / 2., 32.)).shrink(2.);
                let r = ui.interact(half, id.with(("tab", i)), Sense::click());
                let on = ed.gradient == (i == 1);
                if on {
                    ui.painter().rect_filled(half, CornerRadius::same(14), if palette.dark { theme::rgb(0x5b5b5b) } else { Color32::WHITE });
                }
                ui.painter().text(half.center(), Align2::CENTER_CENTER, *label, theme::medium(13.), palette.text);
                if r.clicked() {
                    ed.gradient = i == 1;
                    if !ed.gradient {
                        ed.end_knob = false;
                    }
                }
            }
            ui.add_space(8.);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
                ui.set_width(width - 32.);
                let bubble = custom(&ed.color());
                let card = sample_bubbles(ui, &palette, bubble);
                if ed.gradient {
                    // `GradientDial`: a bar through the middle at the angle,
                    // a knob at each end, 32 in from the card's edge.
                    let inset = card.shrink(32.);
                    let c = inset.center();
                    let rad = ed.deg.to_radians();
                    let dir = Vec2::new(rad.cos(), -rad.sin());
                    let dist = (inset.width() / 2. / dir.x.abs()).min(inset.height() / 2. / dir.y.abs());
                    let (a, b) = (c + dir * dist, c - dir * dist);
                    let bar = if palette.dark { Color32::from_white_alpha(60) } else { Color32::from_white_alpha(150) };
                    ui.painter().with_clip_rect(card).line_segment([c + dir * 2000., c - dir * 2000.], Stroke::new(6., bar));
                    for (end, at, color) in [(false, a, hsl(ed.start)), (true, b, hsl(ed.end))] {
                        let knob = Rect::from_center_size(at, Vec2::splat(40.));
                        let r = ui.interact(knob, id.with(("knob", end)), Sense::click_and_drag());
                        let selected = ed.end_knob == end;
                        ui.painter().circle_filled(at + Vec2::new(0., 1.), 20.5, Color32::from_black_alpha(40));
                        ui.painter().circle_filled(at, 20., bar);
                        ui.painter().circle_filled(at, 15., color);
                        if selected {
                            ui.painter().circle_stroke(at, 18.5, Stroke::new(3., palette.accent));
                        }
                        if r.clicked() || r.drag_started() {
                            ed.end_knob = end;
                        }
                        if r.dragged()
                            && let Some(p) = r.interact_pointer_pos()
                        {
                            let v = p - c;
                            let mut deg = (-v.y).atan2(v.x).to_degrees();
                            if end {
                                deg += 180.;
                            }
                            ed.deg = deg.rem_euclid(360.).round();
                        }
                    }
                }
                ui.add_space(10.);
                let chosen = if ed.end_knob { ed.end } else { ed.start };
                ui.label(egui::RichText::new("Hue").font(theme::regular(14.)).color(palette.text));
                ui.add_space(4.);
                let stops: Vec<Color32> = (0..13).map(|i| hsl(HueSaturation { hue: i as f32 * 30., saturation: 100., lightness: None })).collect();
                let thumb = hsl(HueSaturation { hue: chosen.hue, saturation: 100., lightness: None });
                let mut changed = chosen;
                if let Some(t) = color_slider(ui, id.with("hue"), chosen.hue / 360., &stops, thumb) {
                    changed.hue = (t * 360.).min(359.);
                }
                ui.add_space(8.);
                ui.label(egui::RichText::new("Saturation").font(theme::regular(14.)).color(palette.text));
                ui.add_space(4.);
                let ends = [hsl(HueSaturation { saturation: 0., ..chosen }), hsl(HueSaturation { saturation: 100., ..chosen })];
                if let Some(t) = color_slider(ui, id.with("saturation"), chosen.saturation / 100., &ends, hsl(chosen)) {
                    changed.saturation = t * 100.;
                }
                changed.lightness = None;
                if ed.end_knob {
                    ed.end = changed;
                } else {
                    ed.start = changed;
                }
            });
            // `AxoDialog.Actions`: Cancel and Save at the end.
            ui.add_space(8.);
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            let button = |ui: &egui::Ui, rect: Rect, label: &str, fill: Color32, text: Color32, key: &str| {
                let r = ui.interact(rect, id.with(key), Sense::click());
                let bg = if r.is_pointer_button_down_on() { fill.gamma_multiply(0.9) } else { fill };
                ui.painter().rect_filled(rect, CornerRadius::same(16), bg);
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, theme::medium(13.), text);
                r.clicked()
            };
            let save_r = Rect::from_min_size(Pos2::new(row.right() - 16. - 80., row.top()), Vec2::new(80., 32.));
            let cancel_r = Rect::from_min_size(Pos2::new(save_r.left() - 8. - 80., row.top()), Vec2::new(80., 32.));
            close |= button(ui, cancel_r, "Cancel", palette.fill, palette.text, "cancel");
            save |= button(ui, save_r, "Save", palette.accent, Color32::WHITE, "save");
            ui.add_space(16.);
        });
    });
    let h = shown.response.rect.height();
    if (h - last_h).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(id.with("height"), h));
        ctx.request_repaint();
    }
    if save {
        let color = ed.color();
        let edit = match &ed.id {
            Some(id) => ChatColorEdit::EditCustom { id: id.clone(), color },
            None => ChatColorEdit::AddCustom { id: crate::settings::new_id(), color, chat: ed.chat.clone() },
        };
        app.send(Command::EditChatColors(edit));
        app.color_editor = None;
    } else if close {
        app.color_editor = None;
    } else {
        app.color_editor = Some(ed);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn initial_custom_color_is_ultramarine() {
        let start = super::hue_saturation(crate::theme::rgb(0x0552f0));
        let end = super::hue_saturation(crate::theme::rgb(0x2c6bed));
        let initial = super::Editor::INITIAL;
        assert!((start.hue - initial.start.hue).abs() < 0.5 && (start.saturation - initial.start.saturation).abs() < 0.5, "{start:?}");
        let e = initial.end.unwrap();
        assert!((end.hue - e.hue).abs() < 0.5 && (end.saturation - e.saturation).abs() < 0.5, "{end:?}");
    }

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
