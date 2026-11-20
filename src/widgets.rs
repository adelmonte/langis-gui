//! Small pieces drawn the way Desktop draws them: avatars, icon buttons,
//! pill buttons, badges and text fields.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};
use langis_store::{Conversation, ConversationKind};

use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::i18n::tr;

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

/// Text with its links clickable (Desktop's `Linkify`: http, https and
/// mailto, not sneaky ones), wrapped at `width` and cut to `max_rows`
/// (0: all) with an ellipsis. Returns whether it was cut. Centered when
/// `center`.
pub fn linked_text(ui: &mut Ui, text: &str, size: f32, color: Color32, width: f32, max_rows: usize, center: bool) -> bool {
    let links = crate::chat::find_links(text);
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::TextFormat { font_id: theme::regular(size), color, line_height: Some(size * 1.43), ..Default::default() };
    let mut at = 0;
    for (range, _) in &links {
        job.append(&text[at..range.start], 0., plain.clone());
        job.append(&text[range.clone()], 0., egui::TextFormat { underline: egui::Stroke::new(1., color), ..plain.clone() });
        at = range.end;
    }
    job.append(&text[at..], 0., plain);
    job.wrap.max_width = width;
    if max_rows > 0 {
        job.wrap.max_rows = max_rows;
        job.wrap.overflow_character = Some('\u{2026}');
    }
    if center {
        job.halign = egui::Align::Center;
    }
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let cut = galley.elided;
    let (rect, r) = ui.allocate_exact_size(Vec2::new(if center { width } else { galley.size().x }, galley.size().y), Sense::click());
    let origin = if center { egui::pos2(rect.center().x, rect.top()) } else { rect.min };
    // The link under the pointer, by character.
    let hovered = r.hover_pos().and_then(|p| {
        let index = galley.cursor_from_pos(p - origin).index;
        let byte = text.char_indices().nth(index.0).map_or(text.len(), |(b, _)| b);
        links.iter().find(|(range, _)| range.contains(&byte)).map(|(_, url)| url.clone())
    });
    ui.painter().galley(origin, galley, color);
    if let Some(url) = hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if r.clicked() {
            crate::chat::open_externally(&url);
        }
    }
    cut
}

/// Desktop's `GroupDescription`: two lines, then "read more" opening the
/// whole of it in a dialog titled with the group's name.
pub fn group_description(ui: &mut Ui, title: &str, text: &str, width: f32, center: bool) {
    let palette = Palette::of(ui.ctx());
    let open_id = egui::Id::unique(("group-description", title));
    let cut = linked_text(ui, text, 14., palette.text, width, 2, center);
    if cut {
        let r = ui.add(egui::Label::new(RichText::new(tr("read more")).font(theme::semibold(14.)).color(palette.text)).sense(Sense::click()));
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(open_id, true));
        }
    }
    if ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
        let closed = crate::panes::modal(ui, "group-description", 400., |ui| {
            crate::panes::title(ui, title);
            egui::ScrollArea::vertical().max_height(400.).show(ui, |ui| {
                linked_text(ui, text, 14., palette.text, 368., 0, false);
            });
        });
        if closed {
            ui.ctx().data_mut(|d| d.remove::<bool>(open_id));
        }
    }
}

/// Desktop's `InContactsIcon`: a 14px person-circle after the name of
/// someone in the phone's contacts, gray-60 / gray-25, with its tooltip.
pub fn in_contacts_icon(ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(14.), Sense::hover());
    let color = if palette.dark { theme::rgb(0xb9b9b9) } else { theme::rgb(0x5e5e5e) };
    theme::paint_symbol(ui.painter(), "person-circle", rect.center(), 14., color);
    r.on_hover_text("This person is in your contacts.");
}

/// `AxoButton` variants used outside dialogs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Axo {
    SubtleSecondary,
    SubtleDestructive,
}

/// `AxoButton` size `md`: 30px tall, 12px a side, 13px medium, a pill.
pub fn axo_button(ui: &mut Ui, label: &str, variant: Axo) -> Response {
    let palette = Palette::of(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(galley.size().x + 24., 30.), Sense::click());
    let down = response.is_pointer_button_down_on();
    let (fill, text) = match variant {
        Axo::SubtleSecondary => (if down { palette.fill_pressed } else { palette.fill_secondary }, palette.text),
        Axo::SubtleDestructive if palette.dark => (Color32::from_rgba_unmultiplied(0xff, 0x4a, 0x3a, if down { 41 } else { 31 }), theme::rgb(0xff4a3a)),
        Axo::SubtleDestructive => (Color32::from_rgba_unmultiplied(0xf2, 0x16, 0x02, if down { 26 } else { 15 }), theme::rgb(0xf21602)),
    };
    ui.painter().rect_filled(rect, CornerRadius::same(15), fill);
    ui.painter().galley(rect.center() - galley.size() / 2., galley, text);
    response
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

/// Desktop's `AxoCheckbox`: a 20px rounded box, its border quiet until
/// checked, then filled with the primary color and a white check.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    let palette = Palette::of(ui.ctx());
    let text: egui::WidgetText = text.into();
    let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
    let size = Vec2::new(20. + 10. + galley.size().x, galley.size().y.max(24.));
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let boxed = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 10.), Vec2::splat(20.));
    if *checked {
        let fill = if response.hovered() { palette.badge.gamma_multiply(0.9) } else { palette.badge };
        ui.painter().rect_filled(boxed, CornerRadius::same(6), fill);
        theme::paint_symbol(ui.painter(), "check", boxed.center(), 16., Color32::WHITE);
    } else {
        if response.hovered() {
            ui.painter().rect_filled(boxed, CornerRadius::same(6), palette.fill);
        }
        ui.painter().rect_stroke(boxed, CornerRadius::same(6), Stroke::new(1.5, palette.text_secondary.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    }
    ui.painter().galley(Pos2::new(boxed.right() + 10., rect.center().y - galley.size().y / 2.), galley, palette.text);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *checked, ""));
    response
}

/// Desktop's `SpinnerV2`: a faint ring with a quarter of it, in `color`,
/// going round.
pub fn paint_spinner(ui: &Ui, center: Pos2, size: f32, color: Color32) {
    let stroke = (size / 9.).clamp(2., 5.);
    let radius = size / 2. - stroke / 2.;
    ui.painter().circle_stroke(center, radius, Stroke::new(stroke, color.gamma_multiply(0.2)));
    let start = ui.input(|i| i.time) as f32 * std::f32::consts::TAU * 0.9;
    let points: Vec<Pos2> = (0..=24).map(|i| {
        let a = start + i as f32 / 24. * std::f32::consts::FRAC_PI_2 * 1.2;
        center + radius * Vec2::angled(a)
    }).collect();
    ui.painter().add(egui::Shape::line(points, Stroke::new(stroke, color)));
    for end in [start, start + std::f32::consts::FRAC_PI_2 * 1.2] {
        ui.painter().circle_filled(center + radius * Vec2::angled(end), stroke / 2., color);
    }
    ui.ctx().request_repaint();
}

/// A spinner in the layout, `size` across, in the secondary text color.
pub fn spinner(ui: &mut Ui, size: f32) -> Response {
    let color = Palette::of(ui.ctx()).text_secondary;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_spinner(ui, rect.center(), size, color);
    }
    response
}
