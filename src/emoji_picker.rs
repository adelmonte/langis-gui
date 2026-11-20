//! Desktop's emoji panel (`FunPanelEmojis`): the search field with the
//! skin tone button, the sections along the top (`FunSubNav`), then every
//! group in one scroll, 8 across in 40px cells, each section's title held
//! at the top while it scrolls. Arrow keys move through the grid.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Vec2};
use langis_emoji::{Emoji, Group, SkinTone};

use crate::theme::{self, Palette};
use crate::i18n::tr;

/// Desktop's default skin tone (Settings, Chats), for emoji that take one.
static SKIN_TONE: std::sync::Mutex<Option<SkinTone>> = std::sync::Mutex::new(None);
/// A tone chosen in the picker, for the app to save.
static TONE_CHANGED: std::sync::Mutex<Option<Option<SkinTone>>> = std::sync::Mutex::new(None);

pub fn set_skin_tone(tone: Option<SkinTone>) {
    *SKIN_TONE.lock().unwrap() = tone;
}

pub fn skin_tone() -> Option<SkinTone> {
    *SKIN_TONE.lock().unwrap()
}

/// A skin tone chosen in a picker since last asked, to be saved.
pub fn take_tone_change() -> Option<Option<SkinTone>> {
    TONE_CHANGED.lock().unwrap().take()
}

fn choose_tone(tone: Option<SkinTone>) {
    set_skin_tone(tone);
    *TONE_CHANGED.lock().unwrap() = Some(tone);
}

#[derive(Default)]
pub struct Picker {
    query: String,
    /// The keyboard's place in the grid (an index into what's shown).
    focus: Option<usize>,
    /// A section chosen in the sub-nav, to scroll to.
    jump: Option<usize>,
    /// The skin tone popover: for one emoji (picked when a tone is), or
    /// for the default (`None`).
    tones_for: Option<Option<&'static Emoji>>,
    /// The emoji that asked for a default tone, picked in the tone chosen.
    then_pick: Option<&'static Emoji>,
    /// Desktop's "This Message" section: the reactions a message has.
    pub this_message: Vec<String>,
}

pub const CELL: f32 = 40.;
pub const COLUMNS: usize = 8;
/// The panel's width: the grid and 16px either side.
pub const WIDTH: f32 = CELL * COLUMNS as f32 + 32.;
const HEADER: f32 = 32.;

/// A section of the scroll: its title, sub-nav icon (a symbol, or an emoji
/// where Desktop's icon set has none here) and emoji.
struct Section {
    title: &'static str,
    symbol: Option<&'static str>,
    glyph: &'static str,
    emoji: Vec<&'static Emoji>,
}

fn group_symbol(g: Group) -> Option<&'static str> {
    match g {
        Group::SmileysEmotion => Some("face-smiling"),
        Group::PeopleBody => Some("person"),
        Group::Activities => Some("soccerball"),
        Group::TravelPlaces => Some("car"),
        Group::Objects => Some("lightbulb"),
        Group::Symbols => Some("heart"),
        Group::Flags => Some("flag"),
        _ => None,
    }
}

fn glyph_of(e: &Emoji) -> &'static str {
    if e.supports_skin_tones() { e.glyph_with(skin_tone()) } else { e.glyph }
}

impl Picker {
    fn sections(&self, ui: &egui::Ui, recent: &[String]) -> Vec<Section> {
        // Desktop draws every emoji from its own sheet; here the system font
        // does, so leave out what it is too old to have.
        let font = theme::emoji(24.);
        let keep = |list: Vec<&'static Emoji>| -> Vec<&'static Emoji> {
            ui.ctx().fonts_mut(|f| list.into_iter().filter(|e| e.glyph.chars().filter(|c| !matches!(c, '\u{200d}' | '\u{fe0f}' | '\u{fe0e}' | '\u{20e3}')).all(|c| f.has_glyph(&font, c))).collect())
        };
        let q = self.query.trim();
        if !q.is_empty() {
            let options = langis_emoji::SearchOptions { limit: 120, ..Default::default() };
            let found = keep(langis_emoji::search(q, &options).into_iter().map(|m| m.emoji).collect());
            return vec![Section { title: tr("Search Results"), symbol: Some("search"), glyph: "", emoji: found }];
        }
        let mut out = Vec::new();
        let here: Vec<&'static Emoji> = self.this_message.iter().filter_map(|e| langis_emoji::by_glyph(e).map(|(e, _)| e)).collect();
        if !here.is_empty() {
            out.push(Section { title: tr("This Message"), symbol: Some("heart"), glyph: "", emoji: keep(here) });
        }
        let recents: Vec<&'static Emoji> = recent.iter().filter_map(|e| langis_emoji::by_glyph(e).map(|(e, _)| e)).collect();
        if !recents.is_empty() {
            out.push(Section { title: tr("Recently Used"), symbol: Some("recent"), glyph: "", emoji: keep(recents) });
        }
        for g in Group::PICKER {
            out.push(Section { title: g.label(), symbol: group_symbol(g), glyph: g.icon(), emoji: keep(langis_emoji::in_group(g).collect()) });
        }
        out
    }

    /// The panel: search, sections, the grid. Returns the emoji picked.
    pub fn body(&mut self, ui: &mut egui::Ui, recent: &[String]) -> Option<&'static str> {
        let palette = Palette::of(ui.ctx());
        let mut picked = None;
        let sections = self.sections(ui, recent);
        // Where each section starts in the scroll.
        let mut tops = Vec::with_capacity(sections.len());
        let mut y = 0.;
        for s in &sections {
            tops.push(y);
            y += HEADER + (s.emoji.len().div_ceil(COLUMNS)) as f32 * CELL;
        }
        let total = y;
        let flat: Vec<&'static Emoji> = sections.iter().flat_map(|s| s.emoji.iter().copied()).collect();

        // Search, and the default skin tone's button.
        let mut tone_button = Rect::NOTHING;
        ui.horizontal(|ui| {
            let field = ui.add_sized(
                Vec2::new(ui.available_width() - 40., 32.),
                egui::TextEdit::singleline(&mut self.query).hint_text("Search emoji").margin(egui::Margin::symmetric(10, 7)),
            );
            if field.changed() {
                self.focus = None;
            }
            if self.tones_for.is_none() && ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
            if r.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
            }
            let hand = langis_emoji::by_glyph("✋").map(|(e, _)| e.glyph_with(skin_tone())).unwrap_or("✋");
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, hand, theme::emoji(20.), palette.text);
            if r.on_hover_text("Skin tone").clicked() {
                self.tones_for = Some(None);
            }
            tone_button = rect;
        });
        ui.add_space(6.);

        // The sub-nav: which section is in view, and a click to go there.
        let offset_id = ui.scope_id().with("emoji-scroll");
        let offset: f32 = ui.ctx().data(|d| d.get_temp(offset_id)).unwrap_or(0.);
        let current = tops.iter().rposition(|t| *t <= offset + 1.).unwrap_or(0);
        if sections.len() > 1 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let w = ui.available_width() / sections.len() as f32;
                for (i, s) in sections.iter().enumerate() {
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 32.), Sense::click());
                    let on = i == current;
                    let cell = Rect::from_center_size(rect.center(), Vec2::splat(30.));
                    if on || r.hovered() {
                        ui.painter().rect_filled(cell, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
                    }
                    let color = if on { palette.text } else { palette.text_secondary };
                    match s.symbol {
                        Some(symbol) => theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., color),
                        None => {
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, s.glyph, theme::emoji(16.), palette.text);
                        }
                    }
                    if r.on_hover_text(s.title).clicked() {
                        self.jump = Some(i);
                    }
                }
            });
            ui.add_space(4.);
        }

        // The keyboard: arrows from the search field into the grid.
        let keys = ui.input(|i| {
            (i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter))
        });
        if !flat.is_empty() && self.tones_for.is_none() {
            let last = flat.len() - 1;
            let moved = match (self.focus, keys) {
                (None, (_, _, true, _, _)) => Some(0),
                (Some(f), (true, _, _, _, _)) => Some((f + 1).min(last)),
                (Some(f), (_, true, _, _, _)) => Some(f.saturating_sub(1)),
                (Some(f), (_, _, true, _, _)) => Some((f + COLUMNS).min(last)),
                (Some(f), (_, _, _, true, _)) if f < COLUMNS => None,
                (Some(f), (_, _, _, true, _)) => Some(f - COLUMNS),
                (f, _) => f,
            };
            if moved != self.focus {
                self.focus = moved;
                if let Some(f) = moved {
                    // Scroll to keep it in view.
                    let (row_top, _) = position(&sections, &tops, f);
                    self.jump = None;
                    ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("emoji-want"), row_top));
                }
            }
            if keys.4
                && let Some(f) = self.focus
                && let Some(e) = flat.get(f)
            {
                picked = self.pick(e);
            }
        }

        let height = ui.available_height().clamp(120., 360.);
        let mut area = egui::ScrollArea::vertical().id_salt("emoji-grid").max_height(height).auto_shrink([false, false]);
        if let Some(i) = self.jump.take() {
            area = area.vertical_scroll_offset(tops[i]);
        }
        let want_id = ui.scope_id().with("emoji-want");
        if let Some(row_top) = ui.ctx().data_mut(|d| d.remove_temp::<f32>(want_id)) {
            if row_top < offset + HEADER {
                area = area.vertical_scroll_offset((row_top - HEADER).max(0.));
            } else if row_top + CELL > offset + height {
                area = area.vertical_scroll_offset(row_top + CELL - height);
            }
        }
        let tone = skin_tone();
        let output = area.show_viewport(ui, |ui, viewport| {
            ui.set_height(total);
            let origin = ui.min_rect().min;
            let mut index = 0;
            for (si, s) in sections.iter().enumerate() {
                let top = tops[si];
                let rows = s.emoji.len().div_ceil(COLUMNS);
                let bottom = top + HEADER + rows as f32 * CELL;
                if bottom < viewport.top() || top > viewport.bottom() {
                    index += s.emoji.len();
                    continue;
                }
                ui.painter().text(origin + Vec2::new(4., top + HEADER / 2.), Align2::LEFT_CENTER, s.title, theme::semibold(13.), palette.text_secondary);
                for (i, e) in s.emoji.iter().enumerate() {
                    let at = origin + Vec2::new((i % COLUMNS) as f32 * CELL, top + HEADER + (i / COLUMNS) as f32 * CELL);
                    let rect = Rect::from_min_size(at, Vec2::splat(CELL));
                    let n = index + i;
                    if rect.bottom() < origin.y + viewport.top() || rect.top() > origin.y + viewport.bottom() {
                        continue;
                    }
                    let r = ui.interact(rect, ui.scope_id().with(("emoji", n)), Sense::click());
                    let focused = self.focus == Some(n);
                    if r.hovered() || focused {
                        ui.painter().rect_filled(rect.shrink(2.), CornerRadius::same(8), palette.fill);
                    }
                    if focused {
                        ui.painter().rect_stroke(rect.shrink(2.), CornerRadius::same(8), egui::Stroke::new(2., palette.badge), egui::StrokeKind::Inside);
                    }
                    let glyph = if e.supports_skin_tones() { e.glyph_with(tone) } else { e.glyph };
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, theme::emoji(28.), palette.text);
                    let r = r.on_hover_text(e.shortcode().map(|s| format!(":{s}:")).unwrap_or_default());
                    if r.clicked() {
                        picked = self.pick(e);
                    }
                    // Right-click: this one in another tone.
                    if r.secondary_clicked() && e.supports_skin_tones() {
                        self.tones_for = Some(Some(e));
                    }
                }
                index += s.emoji.len();
            }
            if sections.iter().all(|s| s.emoji.is_empty()) {
                ui.painter().text(origin + Vec2::new(ui.available_width() / 2., 60.), Align2::CENTER_CENTER, tr("No emoji found"), theme::regular(14.), palette.text_secondary);
            }
        });
        let offset = output.state.clamped_offset().y;
        ui.ctx().data_mut(|d| d.insert_temp(offset_id, offset));
        // The current section's title held at the top (sticky), over a
        // backing so the grid passes under it.
        if let Some(i) = tops.iter().rposition(|t| *t <= offset)
            && offset > tops[i] + 0.5
        {
            let bar = Rect::from_min_size(output.inner_rect.min, Vec2::new(output.inner_rect.width(), HEADER));
            ui.painter().rect_filled(bar, 0, palette.popover);
            ui.painter().text(bar.left_center() + Vec2::new(4., 0.), Align2::LEFT_CENTER, sections[i].title, theme::semibold(13.), palette.text_secondary);
        }

        // The skin tone popover.
        if let Some(target) = self.tones_for {
            let anchor = if target.is_some() { output.inner_rect.center_top() + Vec2::new(-130., 40.) } else { tone_button.left_bottom() + Vec2::new(-230., 4.) };
            let area = egui::Area::new(ui.scope_id().with("emoji-tones")).order(egui::Order::Tooltip).fixed_pos(anchor).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(8)).inner_margin(8).shadow(egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: Color32::from_black_alpha(60) }).show(ui, |ui| {
                    if target.is_none() {
                        ui.label(RichText::new("Choose default skin tone").font(theme::semibold(13.)));
                        ui.add_space(4.);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let base = target.or(self.then_pick).or_else(|| langis_emoji::by_glyph("✋").map(|(e, _)| e));
                        let options: Vec<Option<SkinTone>> = std::iter::once(None).chain(SkinTone::ALL.into_iter().map(Some)).collect();
                        for t in options {
                            let (rect, r) = ui.allocate_exact_size(Vec2::splat(CELL), Sense::click());
                            let on = target.is_none() && t == skin_tone();
                            if r.hovered() || on {
                                ui.painter().rect_filled(rect, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
                            }
                            let glyph = base.map(|e| e.glyph_with(t)).unwrap_or("✋");
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, theme::emoji(26.), palette.text);
                            if r.clicked() {
                                match target {
                                    Some(e) => picked = Some(e.glyph_with(t)),
                                    None => {
                                        choose_tone(t);
                                        if let Some(e) = self.then_pick.take() {
                                            picked = Some(e.glyph_with(t));
                                        }
                                    }
                                }
                                self.tones_for = None;
                            }
                        }
                    });
                });
            });
            let outside = ui.input(|i| i.pointer.any_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !tone_button.contains(p));
            if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.tones_for = None;
                self.then_pick = None;
            }
        }
        picked
    }

    /// An emoji clicked: with no default tone yet, one that takes a tone
    /// asks for it first (Desktop's skin tone popover).
    fn pick(&mut self, e: &'static Emoji) -> Option<&'static str> {
        if e.supports_skin_tones() && skin_tone().is_none() && !TONE_ASKED.load(std::sync::atomic::Ordering::Relaxed) {
            TONE_ASKED.store(true, std::sync::atomic::Ordering::Relaxed);
            self.tones_for = Some(None);
            self.then_pick = Some(e);
            return None;
        }
        Some(glyph_of(e))
    }

    /// Shows the picker above `anchor`; returns the emoji picked.
    pub fn show(&mut self, ctx: &egui::Context, anchor: Pos2, open: &mut bool, recent: &[String]) -> Option<&'static str> {
        let palette = Palette::of(ctx);
        let mut picked = None;
        let size = Vec2::new(WIDTH, 440.);
        let area = egui::Area::new(egui::Id::unique("emoji-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(anchor.x, (anchor.y - size.y - 8.).max(8.)))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.popover)
                    .corner_radius(CornerRadius::same(8))
                    .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 90 } else { 50 }) })
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(size.x - 32.);
                        ui.set_height(size.y - 24.);
                        picked = self.body(ui, recent);
                    });
            });
        let outside = ctx.input(|i| i.pointer.any_pressed())
            && self.tones_for.is_none()
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !Rect::from_center_size(anchor, Vec2::splat(64.)).contains(p));
        if outside || (self.tones_for.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
            *open = false;
        }
        picked
    }
}

/// The tone popover comes up by itself once a run; after that a click
/// uses the plain emoji (right-click still offers the tones).
static TONE_ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the `n`th emoji shown sits: its row's top in the scroll.
fn position(sections: &[Section], tops: &[f32], mut n: usize) -> (f32, usize) {
    for (i, s) in sections.iter().enumerate() {
        if n < s.emoji.len() {
            return (tops[i] + HEADER + (n / COLUMNS) as f32 * CELL, i);
        }
        n -= s.emoji.len();
    }
    (0., 0)
}
