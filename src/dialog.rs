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
}

/// What the person chose this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Cancel,
    Action,
}

pub fn confirm(ctx: &egui::Context, id: Id, title: &str, description: &str, action: &str, variant: Variant) -> Option<Choice> {
    let palette = Palette::of(ctx);
    let screen = ctx.content_rect();
    let overlay = if palette.dark { Color32::from_black_alpha(122) } else { Color32::from_black_alpha(61) };
    egui::Area::new(id.with("overlay")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0, overlay);
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(Choice::Cancel);
    }
    let width = 288.;
    let last_h = ctx.data(|d| d.get_temp::<f32>(id.with("height"))).unwrap_or(160.);
    let pos = Pos2::new(screen.center().x - width / 2., screen.center().y - last_h / 2.);
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 61 } else { 31 }) };
    let mut choice = None;
    let shown = egui::Area::new(id.with("dialog")).order(Order::Tooltip).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).shadow(shadow).show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let inner = width - 48.;
            ui.add_space(20.);
            let painter = ui.painter().clone();
            let title_g = painter.layout(title.to_owned(), theme::semibold(14.), palette.text, inner);
            let desc_g = painter.layout(description.to_owned(), theme::regular(13.), palette.text_secondary, inner);
            let (r, _) = ui.allocate_exact_size(Vec2::new(width, title_g.size().y.max(20.)), Sense::hover());
            painter.galley(Pos2::new(r.left() + 24., r.top()), title_g, palette.text);
            ui.add_space(4.);
            let (r, _) = ui.allocate_exact_size(Vec2::new(width, desc_g.size().y), Sense::hover());
            painter.galley(Pos2::new(r.left() + 24., r.top()), desc_g, palette.text_secondary);
            ui.add_space(16.);
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 30.), Sense::hover());
            ui.add_space(16.);
            let w = (width - 32. - 8.) / 2.;
            let cancel = Rect::from_min_size(Pos2::new(row.left() + 16., row.top()), Vec2::new(w, 30.));
            let act = Rect::from_min_size(Pos2::new(cancel.right() + 8., row.top()), Vec2::new(w, 30.));
            if button(ui, id.with("cancel"), cancel, "Cancel", palette.fill, palette.fill_pressed, palette.text) {
                choice = Some(Choice::Cancel);
            }
            let (bg, pressed) = match variant {
                Variant::StrongDestructive if palette.dark => (theme::rgb(0xee382c), theme::rgb(0xef493e)),
                Variant::StrongDestructive => (theme::rgb(0xf21c0d), theme::rgb(0xe41a0c)),
                Variant::StrongPrimary => (palette.accent, palette.accent.gamma_multiply(0.9)),
            };
            if button(ui, id.with("action"), act, action, bg, pressed, Color32::WHITE) {
                choice = Some(Choice::Action);
            }
        });
    });
    let h = shown.response.rect.height();
    if (h - last_h).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(id.with("height"), h));
        ctx.request_repaint();
    }
    choice
}

/// `AxoButton` size `md`: 6px by 12px padding, 13px medium, pill shaped.
fn button(ui: &egui::Ui, id: Id, rect: Rect, label: &str, fill: Color32, pressed: Color32, text: Color32) -> bool {
    let r = ui.interact(rect, id, Sense::click());
    let bg = if r.is_pointer_button_down_on() { pressed } else { fill };
    ui.painter().rect_filled(rect, CornerRadius::same(15), bg);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, theme::medium(13.), text);
    r.clicked()
}
