//! Small pieces drawn the way Desktop draws them: avatars, icon buttons,
//! pill buttons, badges and text fields.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Ui, Vec2};
use langis_store::{Conversation, ConversationKind};

use crate::icons::Icon;
use crate::theme::{self, Palette};

/// What an avatar shows.
pub struct AvatarSpec<'a> {
    pub name: &'a str,
    pub color: u8,
    pub photo: Option<&'a str>,
    pub group: bool,
    pub note_to_self: bool,
}

impl<'a> AvatarSpec<'a> {
    pub fn of(c: &'a Conversation) -> Self {
        Self {
            name: &c.title,
            color: c.color,
            photo: c.avatar_path.as_deref(),
            group: c.kind == ConversationKind::Group,
            note_to_self: c.kind == ConversationKind::NoteToSelf,
        }
    }

    pub fn person(name: &'a str, color: u8) -> Self {
        Self { name, color, photo: None, group: false, note_to_self: false }
    }
}

/// A round avatar `size` across: the photo, else initials on the
/// person's color, else Desktop's person or group symbol.
pub fn avatar(ui: &mut Ui, spec: &AvatarSpec<'_>, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    paint_avatar(ui, rect, spec);
    response
}

/// Desktop's `BadgeImage` on an avatar: the badge at its bottom right,
/// about a third of its size.
pub fn paint_badge(ui: &Ui, avatar: Rect, path: &str) {
    let side = (avatar.width() * 0.375).max(14.);
    let at = Rect::from_min_size(avatar.right_bottom() - Vec2::splat(side * 0.85), Vec2::splat(side));
    egui::Image::new(crate::thumbs::uri(ui.ctx(), path, at.size())).fit_to_exact_size(at.size()).paint_at(ui, at);
}

pub fn paint_avatar(ui: &Ui, rect: Rect, spec: &AvatarSpec<'_>) {
    let size = rect.width();
    if let Some(path) = spec.photo {
        if ui.is_rect_visible(rect) {
            egui::Image::new(crate::thumbs::uri(ui.ctx(), path, rect.size())).corner_radius(CornerRadius::same((size / 2.) as u8)).paint_at(ui, rect);
        }
        return;
    }
    let (bg, fg) = theme::avatar_colors(if spec.note_to_self { 0 } else { spec.color });
    ui.painter().circle_filled(rect.center(), size / 2., bg);
    let initials = crate::format::initials(spec.name);
    // Desktop's Avatar: icons at 62.5%, initials ceil(size * 0.45) regular.
    if spec.note_to_self {
        Icon::Note.paint(ui, rect, size * 0.625, fg);
    } else if spec.group {
        Icon::Group.paint(ui, rect, size * 0.625, fg);
    } else if initials.is_empty() {
        Icon::Person.paint(ui, rect, size * 0.625, fg);
    } else {
        let font = theme::regular((size * 0.45).ceil());
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, initials, font, fg);
    }
}

/// A square icon button with Desktop's rounded hover fill.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, tip: &str) -> Response {
    let palette = Palette::of(ui.ctx());
    let box_size = size + 12.;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    if response.hovered() {
        let fill = if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    }
    icon.paint(ui, rect, size, palette.text);
    response.on_hover_text(tip)
}

/// `AxoIconButton` size `md`, variant `implied-secondary`: a 20px symbol
/// in a 32px circle that fills on hover and press.
pub fn axo_icon_button(ui: &mut Ui, symbol: &str, label: &str) -> Response {
    let palette = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
    if response.is_pointer_button_down_on() {
        ui.painter().circle_filled(rect.center(), 16., palette.fill_pressed);
    } else if response.hovered() {
        ui.painter().circle_filled(rect.center(), 16., palette.fill);
    }
    theme::paint_symbol(ui.painter(), symbol, rect.center(), 20., palette.text);
    response.on_hover_text(label)
}

/// `AxoIconButton` size `sm`: an 18px symbol in a 28px circle; `pressed`
/// (a toggle that's on) fills it with the accent.
pub fn axo_icon_button_sm(ui: &mut Ui, symbol: &str, label: &str, pressed: bool) -> Response {
    let palette = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.), Sense::click());
    if pressed {
        ui.painter().circle_filled(rect.center(), 14., palette.badge);
    } else if response.is_pointer_button_down_on() {
        ui.painter().circle_filled(rect.center(), 14., palette.fill_pressed);
    } else if response.hovered() {
        ui.painter().circle_filled(rect.center(), 14., palette.fill);
    }
    theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., if pressed { Color32::WHITE } else { palette.text });
    response.on_hover_text(label)
}

/// Desktop's round button (`AxoButton`): accent-filled or quiet.
pub fn pill_button(ui: &mut Ui, text: &str, primary: bool) -> Response {
    let palette = Palette::of(ui.ctx());
    let font = theme::medium(14.);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let size = Vec2::new(galley.size().x + 32., 32.);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (mut fill, text_color) = if primary { (palette.badge, Color32::WHITE) } else { (palette.fill_secondary, palette.text) };
    if response.hovered() {
        fill = fill.gamma_multiply(if primary { 0.9 } else { 1.4 });
    }
    ui.painter().rect_filled(rect, CornerRadius::same(16), fill);
    ui.painter().galley(rect.center() - galley.size() / 2., galley, text_color);
    response
}

/// The unread count: a pill of white digits on the accent.
pub fn count_badge(ui: &Ui, right_center: egui::Pos2, count: u32, muted: bool) -> Rect {
    let palette = Palette::of(ui.ctx());
    let text = if count > 999 { "999+".to_owned() } else { count.to_string() };
    let galley = ui.painter().layout_no_wrap(text, theme::semibold(11.), Color32::WHITE);
    let width = (galley.size().x + 10.).max(18.);
    let rect = Rect::from_min_size(right_center - Vec2::new(width, 9.), Vec2::new(width, 18.));
    let fill = if muted { palette.text_secondary } else { palette.badge };
    ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    ui.painter().galley(rect.center() - galley.size() / 2., galley, Color32::WHITE);
    rect
}

/// A rounded text field on Desktop's input fill.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, password: bool) -> Response {
    let palette = Palette::of(ui.ctx());
    egui::Frame::new()
        .fill(palette.fill)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .password(password)
                    .frame(egui::Frame::NONE)
                    .align(egui::Align2::LEFT_CENTER)
                    .min_size(Vec2::new(0., ui.spacing().interact_size.y))
                    .desired_width(ui.available_width()),
            )
        })
        .inner
}

/// Desktop's on/off switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let palette = Palette::of(ui.ctx());
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(40., 24.), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool(response.id, *on);
    let fill = if *on { palette.badge } else { palette.fill_secondary };
    ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12., t);
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 9., Color32::WHITE);
    response
}
