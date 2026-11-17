//! Desktop's Axo lists (`ts/axo/items/AxoList`, `_AxoBaseItem.css`):
//! a card of items with an optional heading above and a note below, each
//! item a symbol, a label with a description under it, and a value or
//! control at the end. Also `AxoPanel`'s header and `AxoSwitch`.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shadow, Stroke, Ui, Vec2};

use crate::theme::{self, Palette};

/// `surface-card`: white, or 8% gray over the dark page.
pub fn card_fill(p: &Palette) -> Color32 {
    if p.dark { theme::rgb(0x232323) } else { Color32::WHITE }
}

/// `fill-secondary` and its pressed shade, over the card.
fn item_fill(p: &Palette, pressed: bool) -> Color32 {
    match (p.dark, pressed) {
        (true, false) => theme::rgb(0x3a3a3a),
        (true, true) => theme::rgb(0x3f3f3f),
        (false, false) => theme::rgb(0xeaeaea),
        (false, true) => theme::rgb(0xe5e5e5),
    }
}

/// `AxoPanel.Header`: 52px, the title centered, an optional back button.
/// Returns whether back was clicked.
pub fn panel_header(ui: &mut Ui, title: &str, back: bool) -> bool {
    let palette = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, title, theme::semibold(14.), palette.text);
    if !back {
        return false;
    }
    let button = Rect::from_center_size(Pos2::new(rect.left() + 16. + 16., rect.center().y), Vec2::splat(32.));
    let r = ui.interact(button, egui::Id::unique("panel-back").with(title), Sense::click());
    if r.hovered() {
        ui.painter().circle_filled(button.center(), 16., if r.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill });
    }
    theme::paint_symbol(ui.painter(), "chevron-[start]", button.center(), 20., palette.text);
    r.on_hover_text("Go back").clicked()
}

/// `AxoContainer`: up to 750px wide, centered, 16px in, 8px down.
pub fn container(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let width = ui.available_width().min(750.);
    let side = ((ui.available_width() - width) / 2.).max(0.);
    ui.horizontal(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 16 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                add(ui)
            });
        });
    });
}

/// `PreferencesList`: a heading, the card, a note. Lists in a group sit 16px
/// apart; call [`gap`] between them.
pub fn list(ui: &mut Ui, label: Option<&str>, footer: Option<&str>, add: impl FnOnce(&mut Ui)) {
    let palette = Palette::of(ui.ctx());
    let width = ui.available_width();
    if let Some(label) = label {
        let g = ui.painter().layout(label.to_owned(), theme::semibold(13.), palette.text, width - 32.);
        let (r, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y.max(18.) + 16.), Sense::hover());
        ui.painter().galley(r.min + Vec2::new(16., 8.), g, palette.text);
    }
    let shadow = Shadow { offset: [0, 1], blur: 2, spread: 0, color: Color32::from_black_alpha(if palette.dark { 41 } else { 20 }) };
    let outline = if palette.dark { Stroke::new(0.5, Color32::from_white_alpha(20)) } else { Stroke::new(0.5, Color32::from_black_alpha(31)) };
    egui::Frame::new()
        .fill(card_fill(&palette))
        .corner_radius(CornerRadius::same(18))
        .shadow(shadow)
        .stroke(outline)
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(width - 8.);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            add(ui);
        });
    if let Some(footer) = footer {
        let g = ui.painter().layout(footer.to_owned(), theme::regular(12.), palette.text_secondary, width - 32.);
        let (r, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y + 16.), Sense::hover());
        ui.painter().galley(r.min + Vec2::new(16., 8.), g, palette.text_secondary);
    }
}

pub fn gap(ui: &mut Ui) {
    ui.add_space(16.);
}

/// An item's text and symbol.
#[derive(Clone, Copy, Default)]
pub struct Item<'a> {
    pub symbol: Option<&'a str>,
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub disabled: bool,
    pub destructive: bool,
}

impl<'a> Item<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, ..Default::default() }
    }
    pub fn symbol(mut self, symbol: &'a str) -> Self {
        self.symbol = Some(symbol);
        self
    }
    pub fn description(mut self, text: &'a str) -> Self {
        self.description = Some(text);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// What an item ends with.
pub enum End<'a> {
    None,
    /// Nothing at the end, the row clickable (`AxoClickableItem` without
    /// an arrow).
    Click,
    /// `value` in secondary text.
    Value(&'a str),
    /// A value, then the `next` chevron (`AxoClickableItem` with `arrow`).
    Arrow(Option<&'a str>),
    Switch(bool),
    /// `AxoSelect.Trigger`: the chosen option and a chevron.
    Select(&'a str),
    /// `AxoItem.Action`, an `md` button: subtle, or subtle destructive.
    Action(&'a str, bool),
    /// A swatch (the chat color), then the chevron.
    Swatch(crate::chat_colors::Bubble),
}

/// What happened to an item this frame.
pub struct ItemResponse {
    pub row: Response,
    /// The select's trigger or the action's button.
    pub control: Option<Response>,
}

impl ItemResponse {
    /// The item was used: a click on the row or its control.
    pub fn clicked(&self) -> bool {
        self.row.clicked() || self.control.as_ref().is_some_and(|c| c.clicked())
    }
}

/// One item. Rows with an arrow, a switch or a swatch are clickable as a
/// whole; a select or an action only on its control.
pub fn item(ui: &mut Ui, item: Item<'_>, end: End<'_>) -> ItemResponse {
    let palette = Palette::of(ui.ctx());
    let width = ui.available_width();
    let leading = if item.symbol.is_some() { 18. + 12. } else { 0. };
    let primary = if item.disabled { palette.text_secondary.gamma_multiply(0.6) } else if item.destructive { palette.destructive } else { palette.text };
    let secondary = if item.disabled { palette.text_secondary.gamma_multiply(0.6) } else { palette.text_secondary };

    let painter = ui.painter().clone();
    let (end_w, end_h, end_text) = match &end {
        End::None | End::Click => (0., 0., None),
        End::Value(v) => {
            let g = painter.layout_no_wrap((*v).to_owned(), theme::regular(13.), secondary);
            (12. + g.size().x, 18., Some(g))
        }
        End::Arrow(v) => {
            let g = v.map(|v| painter.layout_no_wrap(v.to_owned(), theme::regular(13.), secondary));
            (g.as_ref().map(|g| 12. + g.size().x).unwrap_or(0.) + 6. + 14., 18., g)
        }
        End::Switch(_) => (12. + 32., 18., None),
        End::Select(v) => {
            let g = painter.layout_no_wrap((*v).to_owned(), theme::medium(13.), primary);
            (12. + 12. + g.size().x + 8. + 14. + 10., 28., Some(g))
        }
        End::Action(label, _) => {
            let g = painter.layout_no_wrap((*label).to_owned(), theme::medium(13.), primary);
            (12. + (g.size().x + 24.).max(56.), 30., Some(g))
        }
        End::Swatch(_) => (12. + 20. + 6. + 14., 20., None),
    };
    let text_w = (width - 24. - leading - end_w).max(80.);
    let label = painter.layout(item.label.to_owned(), theme::regular(13.), primary, text_w);
    let desc = item.description.map(|d| painter.layout(d.to_owned(), theme::regular(12.), secondary, text_w));
    let text_h = label.size().y.max(18.) + desc.as_ref().map(|d| 2. + d.size().y).unwrap_or(0.);
    let height = 8. + text_h.max(end_h) + 8.;
    let (rect, row) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let whole_row = matches!(end, End::Click | End::Arrow(_) | End::Switch(_) | End::Swatch(_));
    let row = if item.disabled || !whole_row { ui.interact(rect, row.id, Sense::hover()) } else { row };
    if whole_row && !item.disabled && row.hovered() {
        painter.rect_filled(rect, CornerRadius::same(14), item_fill(&palette, row.is_pointer_button_down_on()));
    }
    // Leading and trailing center on the first 38px (`axo-item-centering-to-max-height`).
    let mid = rect.top() + 8. + text_h.max(end_h).min(38.) / 2.;
    let mut x = rect.left() + 12.;
    if let Some(symbol) = item.symbol {
        theme::paint_symbol(&painter, symbol, Pos2::new(x + 9., mid), 18., primary);
        x += leading;
    }
    let label_h = label.size().y;
    let text_top = if desc.is_none() { rect.top() + (height - label_h) / 2. } else { rect.top() + 8. };
    painter.galley(Pos2::new(x, text_top), label, primary);
    if let Some(d) = desc {
        painter.galley(Pos2::new(x, text_top + label_h + 2.), d, secondary);
    }

    let right = rect.right() - 12.;
    let mut control = None;
    match end {
        End::None | End::Click => {}
        End::Value(_) => {
            let g = end_text.unwrap();
            painter.galley(Pos2::new(right - g.size().x, mid - g.size().y / 2.), g, secondary);
        }
        End::Arrow(_) => {
            theme::paint_symbol(&painter, "chevron-[end]", Pos2::new(right - 7., mid), 14., placeholder(&palette));
            if let Some(g) = end_text {
                painter.galley(Pos2::new(right - 20. - g.size().x, mid - g.size().y / 2.), g, secondary);
            }
        }
        End::Switch(on) => {
            let r = Rect::from_center_size(Pos2::new(right - 16., mid), Vec2::new(32., 18.));
            paint_switch(ui, r, on, item.disabled, row.id);
        }
        End::Select(_) => {
            let g = end_text.unwrap();
            let w = 12. + g.size().x + 8. + 14. + 10.;
            let r = Rect::from_min_size(Pos2::new(right - w, mid - 14.), Vec2::new(w, 28.));
            let c = ui.interact(r, row.id.with("select"), if item.disabled { Sense::hover() } else { Sense::click() });
            let fill = if c.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
            painter.rect_filled(r, CornerRadius::same(14), fill);
            painter.galley(Pos2::new(r.left() + 12., mid - g.size().y / 2.), g, primary);
            theme::paint_symbol(&painter, "chevron-down", Pos2::new(r.right() - 10. - 7., mid), 14., primary);
            control = Some(c);
        }
        End::Action(_, destructive) => {
            let g = end_text.unwrap();
            let w = (g.size().x + 24.).max(56.);
            let r = Rect::from_min_size(Pos2::new(right - w, mid - 15.), Vec2::new(w, 30.));
            let c = ui.interact(r, row.id.with("action"), if item.disabled { Sense::hover() } else { Sense::click() });
            let pressed = c.is_pointer_button_down_on();
            let (fill, text) = if destructive {
                (destructive_tint(&palette, pressed), palette.destructive)
            } else {
                (if pressed { palette.fill_pressed } else { palette.fill }, palette.text)
            };
            painter.rect_filled(r, CornerRadius::same(15), fill);
            painter.text(r.center(), Align2::CENTER_CENTER, galley_text(&g), theme::medium(13.), text);
            control = Some(c);
        }
        End::Swatch(bubble) => {
            theme::paint_symbol(&painter, "chevron-[end]", Pos2::new(right - 7., mid), 14., placeholder(&palette));
            crate::chat_colors::paint_swatch(&painter, Pos2::new(right - 20. - 10., mid), 10., bubble);
        }
    }
    ItemResponse { row, control }
}

fn galley_text(g: &egui::Galley) -> String {
    g.job.text.clone()
}

/// `label-placeholder`: 35% of the text color.
fn placeholder(p: &Palette) -> Color32 {
    if p.dark { Color32::from_white_alpha(89) } else { Color32::from_black_alpha(89) }
}

/// `destructive-tint`: the destructive red at 10%, 14% pressed.
fn destructive_tint(p: &Palette, pressed: bool) -> Color32 {
    p.destructive.gamma_multiply(if pressed { 0.14 } else { 0.1 })
}

/// `AxoSwitch`: an 18 by 32 track, the accent sliding in from the start
/// when on, a 16px white thumb.
fn paint_switch(ui: &Ui, r: Rect, on: bool, disabled: bool, id: egui::Id) {
    let palette = Palette::of(ui.ctx());
    let painter = ui.painter();
    let t = ui.ctx().animate_bool_with_time(id.with("switch"), on, 0.2);
    let radius = CornerRadius::same(9);
    painter.rect_filled(r, radius, palette.fill);
    if t > 0. {
        let mut accent = palette.badge;
        if disabled {
            accent = accent.gamma_multiply(0.25);
        }
        let w = egui::lerp(0.0..=r.width(), t);
        painter.with_clip_rect(Rect::from_min_size(r.min, Vec2::new(w, r.height()))).rect_filled(r, radius, accent.gamma_multiply(t.min(1.)));
    }
    let border = if palette.dark { Color32::from_white_alpha(31) } else { Color32::from_black_alpha(31) };
    painter.rect_stroke(r, radius, Stroke::new(1., border), egui::StrokeKind::Inside);
    let x = egui::lerp(r.left() + 9.0..=r.right() - 9., t);
    let thumb = Pos2::new(x, r.center().y);
    painter.circle_filled(thumb + Vec2::new(0., 0.5), 8.5, Color32::from_black_alpha(31));
    painter.circle_filled(thumb, 8., Color32::WHITE);
}

/// A switch item: the row toggles `on`. Returns whether it changed.
pub fn switch(ui: &mut Ui, item: Item<'_>, on: &mut bool) -> bool {
    let r = self::item(ui, item, End::Switch(*on));
    if r.row.clicked() && !item.disabled {
        *on = !*on;
        return true;
    }
    false
}

/// A select item: its options in Desktop's select menu, the chosen one
/// checked. Returns the index picked.
pub fn select(ui: &mut Ui, id: egui::Id, item: Item<'_>, options: &[&str], chosen: Option<usize>) -> Option<usize> {
    let current = chosen.and_then(|i| options.get(i)).copied().unwrap_or("");
    let r = self::item(ui, item, End::Select(current));
    let mut picked = None;
    if let Some(trigger) = &r.control {
        crate::menu::dropdown(ui, trigger, id, |m| {
            for (i, option) in options.iter().enumerate() {
                if m.check(chosen == Some(i), option) {
                    picked = Some(i);
                }
            }
        });
    }
    picked
}
