//! Desktop's `AxoConfirmDialog` (on `AxoAlertDialog` and `AxoBaseDialog`):
//! a 288px card, `curved-30` corners, `material-dialog`, over a dimmed
//! window (`fill-overlay`). Title `type-title-small`, description
//! `type-body-medium` secondary, then a footer of equal width `md` buttons:
//! Cancel and the action. Escape cancels; the overlay ignores clicks.

use egui::{Align2, Color32, CornerRadius, Id, Order, Pos2, Rect, Sense, Shadow, Vec2};

use crate::theme::{self, Palette};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    StrongPrimary,
    StrongDestructive,
    SubtleDestructive,
}

/// What the person chose this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Cancel,
    Action,
}

pub fn confirm(ctx: &egui::Context, id: Id, title: &str, description: &str, action: &str, variant: Variant) -> Option<Choice> {
    match show(ctx, id, title, description, Some("Cancel"), &[(action, variant)]) {
        Some(Answer::Cancel) => Some(Choice::Cancel),
        Some(Answer::Action(_)) => Some(Choice::Action),
        None => None,
    }
}

/// An `AxoAlertDialog` with one button: closing it, by the button or
/// Escape, is the answer.
pub fn alert(ctx: &egui::Context, id: Id, title: &str, description: &str, action: &str) -> bool {
    show(ctx, id, title, description, None, &[(action, Variant::StrongPrimary)]).is_some()
}

/// What a dialog with several actions answered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    Cancel,
    Action(usize),
}

/// The dialog with a footer of `cancel` (when given) and `actions`: two
/// buttons share a row; more stack, full width.
pub fn show(ctx: &egui::Context, id: Id, title: &str, description: &str, cancel: Option<&str>, actions: &[(&str, Variant)]) -> Option<Answer> {
    show_with(ctx, id, title, description, cancel, actions, |_| {})
}

/// `show`, with more under the description (`AxoDialog.Body`), 24 in.
pub fn show_with(ctx: &egui::Context, id: Id, title: &str, description: &str, cancel: Option<&str>, actions: &[(&str, Variant)], body: impl FnOnce(&mut egui::Ui)) -> Option<Answer> {
    let palette = Palette::of(ctx);
    let screen = ctx.content_rect();
    let overlay = if palette.dark { Color32::from_black_alpha(122) } else { Color32::from_black_alpha(61) };
    egui::Area::new(id.with("overlay")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0, overlay);
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(Answer::Cancel);
    }
    let width = 288.;
    let last_h = ctx.data(|d| d.get_temp::<f32>(id.with("height"))).unwrap_or(160.);
    let pos = Pos2::new(screen.center().x - width / 2., screen.center().y - last_h / 2.);
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 61 } else { 31 }) };
    let mut answer = None;
    let mut buttons: Vec<(&str, Option<Variant>)> = Vec::new();
    if let Some(cancel) = cancel {
        buttons.push((cancel, None));
    }
    buttons.extend(actions.iter().map(|(label, v)| (*label, Some(*v))));
    let shown = egui::Area::new(id.with("dialog")).order(Order::Tooltip).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).shadow(shadow).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let inner = width - 48.;
            ui.add_space(20.);
            let painter = ui.painter().clone();
            let title_g = painter.layout(title.to_owned(), theme::semibold(14.), palette.text, inner);
            let (r, _) = ui.allocate_exact_size(Vec2::new(width, title_g.size().y.max(20.)), Sense::hover());
            painter.galley(Pos2::new(r.left() + 24., r.top()), title_g, palette.text);
            if !description.is_empty() {
                let desc_g = painter.layout(description.to_owned(), theme::regular(13.), palette.text_secondary, inner);
                ui.add_space(4.);
                let (r, _) = ui.allocate_exact_size(Vec2::new(width, desc_g.size().y), Sense::hover());
                painter.galley(Pos2::new(r.left() + 24., r.top()), desc_g, palette.text_secondary);
            }
            egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 0, bottom: 0 }).show(ui, |ui| {
                ui.set_width(inner);
                body(ui);
            });
            ui.add_space(16.);
            let side_by_side = buttons.len() <= 2;
            let rows = if side_by_side { 1 } else { buttons.len() };
            let (area, _) = ui.allocate_exact_size(Vec2::new(width, rows as f32 * 30. + (rows - 1) as f32 * 8.), Sense::hover());
            ui.add_space(16.);
            let n = buttons.len() as f32;
            for (i, (label, variant)) in buttons.iter().enumerate() {
                let rect = if side_by_side {
                    let w = (width - 32. - 8. * (n - 1.)) / n;
                    Rect::from_min_size(Pos2::new(area.left() + 16. + i as f32 * (w + 8.), area.top()), Vec2::new(w, 30.))
                } else {
                    Rect::from_min_size(Pos2::new(area.left() + 16., area.top() + i as f32 * 38.), Vec2::new(width - 32., 30.))
                };
                let (bg, pressed, text) = match variant {
                    None => (palette.fill, palette.fill_pressed, palette.text),
                    Some(Variant::StrongDestructive) if palette.dark => (theme::rgb(0xee382c), theme::rgb(0xef493e), Color32::WHITE),
                    Some(Variant::StrongDestructive) => (theme::rgb(0xf21c0d), theme::rgb(0xe41a0c), Color32::WHITE),
                    Some(Variant::StrongPrimary) => (palette.accent, palette.accent.gamma_multiply(0.9), Color32::WHITE),
                    // `fill-destructive-tint`, `label-destructive`.
                    Some(Variant::SubtleDestructive) if palette.dark => (Color32::from_rgba_unmultiplied(0xff, 0x4a, 0x3a, 31), Color32::from_rgba_unmultiplied(0xff, 0x4a, 0x3a, 41), theme::rgb(0xff4a3a)),
                    Some(Variant::SubtleDestructive) => (Color32::from_rgba_unmultiplied(0xf2, 0x16, 0x02, 15), Color32::from_rgba_unmultiplied(0xf2, 0x16, 0x02, 26), theme::rgb(0xf21602)),
                };
                if button(ui, id.with(("button", i)), rect, label, bg, pressed, text) {
                    answer = Some(match (cancel.is_some(), i) {
                        (true, 0) => Answer::Cancel,
                        (true, i) => Answer::Action(i - 1),
                        (false, i) => Answer::Action(i),
                    });
                }
            }
        });
    });
    let h = shown.response.rect.height();
    if (h - last_h).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(id.with("height"), h));
        ctx.request_repaint();
    }
    answer
}

/// `AxoButton` size `md`: 6px by 12px padding, 13px medium, pill shaped.
fn button(ui: &egui::Ui, id: Id, rect: Rect, label: &str, fill: Color32, pressed: Color32, text: Color32) -> bool {
    let r = ui.interact(rect, id, Sense::click());
    let bg = if r.is_pointer_button_down_on() { pressed } else { fill };
    ui.painter().rect_filled(rect, CornerRadius::same(15), bg);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, theme::medium(13.), text);
    r.clicked()
}
