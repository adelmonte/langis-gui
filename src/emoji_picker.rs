//! Desktop's emoji picker: a search field over a grid, with the groups as
//! tabs along the bottom.

use egui::{Align2, CornerRadius, Pos2, Rect, RichText, Sense, Vec2};
use langis_emoji::Group;

use crate::theme::{self, Palette};

#[derive(Default)]
pub struct Picker {
    query: String,
    group: Option<Group>,
}

const CELL: f32 = 36.;
const COLUMNS: usize = 9;

impl Picker {
    /// Shows the picker above `anchor`; returns the emoji picked.
    pub fn show(&mut self, ctx: &egui::Context, anchor: Pos2, open: &mut bool) -> Option<&'static str> {
        let palette = Palette::of(ctx);
        let mut picked = None;
        let size = Vec2::new(CELL * COLUMNS as f32 + 24., 400.);
        let area = egui::Area::new(egui::Id::unique("emoji-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(anchor.x, (anchor.y - size.y - 8.).max(8.)))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.popover)
                    .corner_radius(CornerRadius::same(12))
                    .stroke(egui::Stroke::new(1., palette.border))
                    .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: egui::Color32::from_black_alpha(50) })
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(size.x - 24.);
                        crate::widgets::text_field(ui, &mut self.query, "Search emoji", false).request_focus();
                        ui.add_space(6.);
                        let emoji: Vec<&'static langis_emoji::Emoji> = if self.query.trim().is_empty() {
                            let group = self.group.unwrap_or(Group::SmileysEmotion);
                            ui.label(RichText::new(group.label()).font(theme::semibold(13.)).color(palette.text_secondary));
                            langis_emoji::in_group(group).collect()
                        } else {
                            let options = langis_emoji::SearchOptions { limit: 90, ..Default::default() };
                            langis_emoji::search(self.query.trim(), &options).into_iter().map(|m| m.emoji).collect()
                        };
                        egui::ScrollArea::vertical().max_height(270.).auto_shrink([false, false]).show(ui, |ui| {
                            for row in emoji.chunks(COLUMNS) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.;
                                    for e in row {
                                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(CELL), Sense::click());
                                        if r.hovered() {
                                            ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                                        }
                                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, e.glyph, theme::regular(24.), palette.text);
                                        if r.on_hover_text(e.shortcode().map(|s| format!(":{s}:")).unwrap_or_default()).clicked() {
                                            picked = Some(e.glyph);
                                        }
                                    }
                                });
                            }
                        });
                        ui.add_space(4.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            for group in Group::PICKER {
                                let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                                let selected = self.query.is_empty() && self.group.unwrap_or(Group::SmileysEmotion) == group;
                                if selected || r.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(8), if selected { palette.fill_pressed } else { palette.fill });
                                }
                                ui.painter().text(rect.center(), Align2::CENTER_CENTER, group.icon(), theme::regular(18.), palette.text);
                                if r.on_hover_text(group.label()).clicked() {
                                    self.group = Some(group);
                                    self.query.clear();
                                }
                            }
                        });
                    });
            });
        let outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !Rect::from_center_size(anchor, Vec2::splat(64.)).contains(p));
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            *open = false;
        }
        picked
    }
}
