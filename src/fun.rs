//! Desktop's fun picker (`FunPicker`): emoji, stickers and GIFs in one
//! popover over the composer, and its sticker manager
//! (`StickerManager`, `StickerPreviewModal`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::giphy::Gif;
use langis_core::stickers::{StickerManagerView, StickerPackPreview, StickerPackSummary};
use langis_core::{Command, Event, FavoriteStickerView, StickerPackView, StickerView};

use crate::app::{App, PendingSend};
use crate::chat::ChatView;
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Emoji,
    Stickers,
    Gifs,
}

/// How long the GIF search rests before searching.
const GIF_DELAY: Duration = Duration::from_millis(400);

/// Stickers and GIFs, kept across chats.
#[derive(Default)]
pub struct Fun {
    pub tab: Tab,
    pub packs: Vec<Arc<StickerPackView>>,
    pub favorites: Vec<FavoriteStickerView>,
    /// Every favorite by (pack id, sticker id), image here or not.
    pub favorite_ids: Vec<(Vec<u8>, u32)>,
    asked: bool,
    /// The pack shown; `None` for favorites.
    pack: Option<Vec<u8>>,
    /// Favorites were picked by hand (not left as the default).
    chose_pack: bool,
    gif_query: String,
    gif_typed: Option<Instant>,
    /// The search the results are for.
    gif_shown: Option<String>,
    gifs: Vec<(Gif, PathBuf)>,
    gif_next: Option<u32>,
    gif_error: Option<String>,
    gif_loading: bool,
    /// Stickers sent lately, newest first (Desktop's Recents), by pack id
    /// and sticker id, kept in the `ui.recent_stickers` setting.
    pub recent_stickers: Vec<(Vec<u8>, u32)>,
    /// Showing the sticker Recents.
    sticker_recents: bool,
    /// GIFs sent lately, newest first (Desktop's Recents), kept in the
    /// `ui.recent_gifs` setting.
    pub recent_gifs: Vec<(Gif, PathBuf)>,
    /// Showing Recents rather than a search.
    gif_recents: bool,
    pub manager: Option<StickerManagerView>,
    pub manager_open: bool,
    /// A pack opened for its preview, and what came for it.
    pub preview: Option<(Vec<u8>, Vec<u8>, Option<Result<StickerPackPreview, String>>)>,
}

impl Fun {
    /// Takes the backend's sticker and GIF events; false for any other.
    pub fn on_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Stickers(packs) => {
                self.packs = packs.iter().cloned().map(Arc::new).collect();
                if self.pack.as_ref().is_some_and(|p| !self.packs.iter().any(|x| &x.id == p)) {
                    self.pack = None;
                }
            }
            Event::FavoriteStickers(favorites) => self.favorites = favorites.clone(),
            Event::FavoriteStickerIds(ids) => self.favorite_ids = ids.clone(),
            Event::StickerManager(view) => self.manager = Some(view.clone()),
            Event::StickerPackPreview { id, result } => {
                if let Some((open, _, slot)) = self.preview.as_mut()
                    && open == id
                {
                    *slot = Some(result.clone());
                }
            }
            Event::Gifs { query, offset, result } => {
                if self.gif_shown.as_deref() != Some(query.as_str()) {
                    return true;
                }
                self.gif_loading = false;
                match result {
                    Ok(found) => {
                        if *offset == 0 {
                            self.gifs.clear();
                        }
                        self.gifs.extend(found.gifs.iter().cloned());
                        self.gif_next = found.next;
                        self.gif_error = None;
                    }
                    Err(error) => self.gif_error = Some(error.clone()),
                }
            }
            _ => return false,
        }
        true
    }

    fn search_gifs(&mut self, app: &App, offset: u32) {
        let query = self.gif_query.trim().to_owned();
        self.gif_shown = Some(query.clone());
        self.gif_loading = true;
        if offset == 0 {
            self.gif_next = None;
        }
        app.send(Command::SearchGifs { query, offset });
    }
}

/// Shows the picker above `anchor`. An emoji picked comes back to go in
/// the text; a sticker or GIF is sent from here.
pub fn show(app: &mut App, ctx: &egui::Context, anchor: Pos2, chat: &mut ChatView, c: &langis_store::Conversation) -> Option<&'static str> {
    let palette = Palette::of(ctx);
    let mut fun = std::mem::take(&mut app.fun);
    if !fun.asked {
        fun.asked = true;
        app.send(Command::LoadStickers);
    }
    let mut picked = None;
    let mut send = None;
    let size = Vec2::new(36. * 9. + 24., 452.);
    let area = egui::Area::new(egui::Id::unique("fun-picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(anchor.x, (anchor.y - size.y - 8.).max(8.)))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette.popover)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1., palette.border))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(50) })
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(size.x - 24.);
                    ui.set_height(size.y - 24.);
                    tabs(ui, &palette, &mut fun.tab);
                    ui.add_space(8.);
                    match fun.tab {
                        Tab::Emoji => picked = chat.emoji.body(ui, &app.recent_emoji),
                        Tab::Stickers => send = stickers(app, ui, &palette, &mut fun),
                        Tab::Gifs => send = gifs(app, ui, &palette, &mut fun),
                    }
                });
        });
    let outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !Rect::from_center_size(anchor, Vec2::splat(64.)).contains(p));
    let manager = fun.manager_open;
    app.fun = fun;
    if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) || manager {
        chat.emoji_open = false;
    }
    if let Some(what) = send {
        let quote = chat.quote.take();
        let command = match what {
            Send::Sticker(pack, sticker) => {
                remember_sticker(app, &pack.id, sticker.id);
                Command::SendSticker { conversation: c.id.clone(), pack, sticker, quote }
            }
            Send::Gif(gif) => {
                remember_gif(app, &gif);
                Command::SendGif { conversation: c.id.clone(), gif, quote }
            }
        };
        chat.emoji_open = false;
        chat.scroll_to_bottom = 10;
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
    if let Some(e) = picked {
        app.use_emoji(e);
    }
    picked
}

/// Desktop's GIF categories (`FunPanelGifs`), each a search.
const GIF_CATEGORIES: [(&str, &str); 7] = [
    ("Celebrate", "celebrate"),
    ("Love", "love"),
    ("Thumbs up", "thumbs up"),
    ("Surprised", "surprised"),
    ("Excited", "excited"),
    ("Sad", "sad"),
    ("Angry", "angry"),
];

/// Most stickers kept in Recents.
const RECENT_STICKERS: usize = 24;

/// Puts a sent sticker first in Recents and saves the list.
fn remember_sticker(app: &mut App, pack: &[u8], sticker: u32) {
    let list = &mut app.fun.recent_stickers;
    list.retain(|(p, s)| !(p == pack && *s == sticker));
    list.insert(0, (pack.to_vec(), sticker));
    list.truncate(RECENT_STICKERS);
    let saved = serde_json::Value::Array(list.iter().map(|(p, s)| serde_json::json!({ "pack": p.iter().map(|b| format!("{b:02x}")).collect::<String>(), "sticker": s })).collect()).to_string();
    app.load(move |db| async move {
        let _ = db.set_setting("ui.recent_stickers", &saved).await;
        None
    });
}

pub fn parse_recent_stickers(text: &str) -> Vec<(Vec<u8>, u32)> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    items
        .iter()
        .filter_map(|v| {
            let hex = v["pack"].as_str()?;
            let pack: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect::<Option<_>>()?;
            Some((pack, v["sticker"].as_u64()? as u32))
        })
        .collect()
}

/// Most GIFs kept in Recents.
const RECENT_GIFS: usize = 30;

/// Recents, two columns like the results; a click sends one.
fn recent_gifs(ui: &mut Ui, palette: &Palette, fun: &mut Fun) -> Option<Send> {
    let mut send = None;
    let col = (ui.available_width() - 6.) / 2.;
    egui::ScrollArea::vertical().id_salt("recent-gifs").auto_shrink([false, false]).show(ui, |ui| {
        let top = ui.cursor().top();
        let mut heights = [0f32; 2];
        for (gif, path) in &fun.recent_gifs {
            let column = if heights[0] <= heights[1] { 0 } else { 1 };
            let h = (col * gif.preview.height.max(1) as f32 / gif.preview.width.max(1) as f32).clamp(60., 240.);
            let rect = Rect::from_min_size(Pos2::new(ui.max_rect().left() + column as f32 * (col + 6.), top + heights[column]), Vec2::new(col, h));
            let r = ui.interact(rect, ui.scope_id().with(("recent-gif", &gif.id)), Sense::click());
            if ui.is_rect_visible(rect) {
                let file = path.to_string_lossy();
                let radius = CornerRadius::same(8);
                if !(r.hovered() && crate::video::inline(ui, &file, rect, radius)) {
                    match crate::video::poster(ui.ctx(), &file) {
                        Some(still) => crate::chat::cover_image(ui, &still, rect, radius),
                        None => {
                            ui.painter().rect_filled(rect, radius, palette.fill);
                        }
                    }
                }
            }
            if r.on_hover_text(&gif.title).clicked() {
                send = Some(Send::Gif(gif.clone()));
            }
            heights[column] += h + 6.;
        }
        ui.allocate_space(Vec2::new(ui.available_width(), heights[0].max(heights[1])));
    });
    send
}

/// Puts a sent GIF first in Recents and saves the list.
fn remember_gif(app: &mut App, gif: &Gif) {
    let path = app.fun.gifs.iter().chain(&app.fun.recent_gifs).find(|(g, _)| g.id == gif.id).map(|(_, p)| p.clone());
    let Some(path) = path else { return };
    app.fun.recent_gifs.retain(|(g, _)| g.id != gif.id);
    app.fun.recent_gifs.insert(0, (gif.clone(), path));
    app.fun.recent_gifs.truncate(RECENT_GIFS);
    let saved = serde_json::Value::Array(app.fun.recent_gifs.iter().map(|(g, p)| gif_json(g, p)).collect()).to_string();
    app.load(move |db| async move {
        let _ = db.set_setting("ui.recent_gifs", &saved).await;
        None
    });
}

fn gif_json(g: &Gif, path: &std::path::Path) -> serde_json::Value {
    let media = |m: &langis_core::giphy::GifMedia| serde_json::json!({ "url": m.url, "width": m.width, "height": m.height });
    serde_json::json!({ "id": g.id, "title": g.title, "preview": media(&g.preview), "original": media(&g.original), "path": path.to_string_lossy() })
}

/// The saved Recents whose previews are still on disk.
pub fn parse_recent_gifs(text: &str) -> Vec<(Gif, PathBuf)> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let media = |v: &serde_json::Value| langis_core::giphy::GifMedia {
        url: v["url"].as_str().unwrap_or_default().to_owned(),
        width: v["width"].as_u64().unwrap_or(0) as u32,
        height: v["height"].as_u64().unwrap_or(0) as u32,
    };
    items
        .iter()
        .filter_map(|v| {
            let path = PathBuf::from(v["path"].as_str()?);
            path.exists().then(|| {
                let gif = Gif { id: v["id"].as_str().unwrap_or_default().to_owned(), title: v["title"].as_str().unwrap_or_default().to_owned(), preview: media(&v["preview"]), original: media(&v["original"]) };
                (gif, path)
            })
        })
        .collect()
}

enum Send {
    Sticker(Arc<StickerPackView>, StickerView),
    Gif(Gif),
}

fn tabs(ui: &mut Ui, palette: &Palette, tab: &mut Tab) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        for (t, label) in [(Tab::Emoji, "Emoji"), (Tab::Stickers, "Stickers"), (Tab::Gifs, "GIFs")] {
            let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), palette.text);
            let (rect, r) = ui.allocate_exact_size(Vec2::new(galley.size().x + 24., 30.), Sense::click());
            let on = *tab == t;
            if on || r.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(15), if on { palette.fill_pressed } else { palette.fill });
            }
            ui.painter().galley(rect.center() - galley.size() / 2., galley, palette.text);
            if r.clicked() {
                *tab = t;
            }
        }
    });
}

/// A sticker's image in `rect`.
fn sticker_image(ui: &Ui, path: &std::path::Path, rect: Rect) {
    // Animated stickers play while hovered in the picker, as Desktop's do.
    let file = path.to_string_lossy();
    if ui.rect_contains_pointer(rect) && crate::anim::paint(ui, &file, rect) {
        return;
    }
    egui::Image::new(format!("file://{}", path.display())).maintain_aspect_ratio(true).fit_to_exact_size(rect.size()).paint_at(ui, rect);
}

fn stickers(app: &App, ui: &mut Ui, palette: &Palette, fun: &mut Fun) -> Option<Send> {
    let mut send = None;
    // With no favorites yet, open on the first pack.
    if fun.pack.is_none() && fun.favorites.is_empty() && !fun.chose_pack {
        fun.pack = fun.packs.first().map(|p| p.id.clone());
    }
    // The packs along the top: favorites, each installed pack, then "+".
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        // Desktop's Recents first, when there are any.
        if !fun.recent_stickers.is_empty() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
            if fun.sticker_recents || r.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(8), if fun.sticker_recents { palette.fill_pressed } else { palette.fill });
            }
            theme::paint_symbol(ui.painter(), "recent", rect.center(), 18., palette.text);
            if r.on_hover_text("Recents").clicked() {
                fun.sticker_recents = true;
                fun.chose_pack = true;
            }
        }
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
        let favorites_on = fun.pack.is_none() && !fun.sticker_recents;
        if favorites_on || r.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(8), if favorites_on { palette.fill_pressed } else { palette.fill });
        }
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "★", theme::regular(18.), palette.text);
        if r.on_hover_text("Favorites").clicked() {
            fun.pack = None;
            fun.sticker_recents = false;
            fun.chose_pack = true;
        }
        egui::ScrollArea::horizontal().max_width(ui.available_width() - 40.).show(ui, |ui| {
            ui.horizontal(|ui| {
                for pack in &fun.packs {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
                    let on = !fun.sticker_recents && fun.pack.as_ref() == Some(&pack.id);
                    if on || r.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
                    }
                    if let Some(cover) = pack.stickers.first() {
                        sticker_image(ui, &cover.path, rect.shrink(4.));
                    }
                    if r.on_hover_text(&pack.title).clicked() {
                        fun.pack = Some(pack.id.clone());
                        fun.sticker_recents = false;
                    }
                }
            });
        });
        if widgets::icon_button(ui, Icon::Plus, 18., "Add stickers").clicked() {
            fun.manager_open = true;
            app.send(Command::LoadStickerManager);
        }
    });
    ui.add_space(6.);
    let shown: Vec<(Arc<StickerPackView>, StickerView)> = match &fun.pack {
        // Recents: those still in an installed pack.
        _ if fun.sticker_recents => fun
            .recent_stickers
            .iter()
            .filter_map(|(pack, sticker)| {
                let p = fun.packs.iter().find(|p| &p.id == pack)?;
                Some((p.clone(), p.stickers.iter().find(|s| s.id == *sticker)?.clone()))
            })
            .collect(),
        Some(id) => fun.packs.iter().find(|p| &p.id == id).map(|p| p.stickers.iter().map(|s| (p.clone(), s.clone())).collect()).unwrap_or_default(),
        None => fun
            .favorites
            .iter()
            .map(|f| {
                let pack = fun.packs.iter().find(|p| p.id == f.pack_id).cloned().unwrap_or_else(|| {
                    Arc::new(StickerPackView { id: f.pack_id.clone(), key: f.pack_key.clone(), title: String::new(), author: String::new(), stickers: vec![f.sticker.clone()] })
                });
                (pack, f.sticker.clone())
            })
            .collect(),
    };
    let title = match &fun.pack {
        _ if fun.sticker_recents => "Recents".to_owned(),
        Some(id) => fun.packs.iter().find(|p| &p.id == id).map(|p| p.title.clone()).unwrap_or_default(),
        None => "Favorites".to_owned(),
    };
    ui.label(RichText::new(title).font(theme::semibold(13.)).color(palette.text_secondary));
    if shown.is_empty() {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            let text = if fun.pack.is_none() { "Stickers you favorite show up here." } else { "No stickers in this pack yet." };
            ui.label(RichText::new(text).color(palette.text_secondary));
        });
        return None;
    }
    let cell = (ui.available_width() / 4.).floor();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for row in shown.chunks(4) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                for (pack, sticker) in row {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                    if r.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                    }
                    sticker_image(ui, &sticker.path, rect.shrink(6.));
                    let favorite = fun.favorite_ids.iter().any(|(p, s)| *p == pack.id && *s == sticker.id)
                        || fun.favorites.iter().any(|f| f.pack_id == pack.id && f.sticker.id == sticker.id);
                    if favorite {
                        ui.painter().text(rect.right_top() + Vec2::new(-8., 6.), Align2::RIGHT_TOP, "★", theme::regular(13.), palette.badge);
                    }
                    let tip = format!("{}  (right-click to {} favorites)", sticker.emoji.clone().unwrap_or_default(), if favorite { "remove from" } else { "add to" });
                    let r = r.on_hover_text(tip);
                    if r.secondary_clicked() {
                        app.send(Command::SetStickerFavorite { pack_id: pack.id.clone(), pack_key: pack.key.clone(), sticker_id: sticker.id, favorite: !favorite });
                    }
                    if r.clicked() {
                        send = Some(Send::Sticker(pack.clone(), sticker.clone()));
                    }
                }
            });
        }
    });
    send
}

fn gifs(app: &App, ui: &mut Ui, palette: &Palette, fun: &mut Fun) -> Option<Send> {
    let mut send = None;
    let field = widgets::text_field(ui, &mut fun.gif_query, "Search GIPHY", false);
    if field.changed() {
        fun.gif_typed = Some(Instant::now());
    }
    if !field.has_focus() && ui.memory(|m| m.focused().is_none()) {
        field.request_focus();
    }
    let wanted = fun.gif_query.trim().to_owned();
    let rested = fun.gif_typed.is_none_or(|t| t.elapsed() >= GIF_DELAY);
    if rested && fun.gif_shown.as_deref() != Some(wanted.as_str()) {
        fun.search_gifs(app, 0);
    } else if !rested {
        ui.ctx().request_repaint_after(GIF_DELAY);
    }
    ui.add_space(6.);
    // Desktop's sections: Recents, Trending, then its categories, which
    // are searches.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4., 4.);
        let chip = |ui: &mut Ui, label: &str, on: bool| {
            let fill = if on { palette.fill_pressed } else { palette.fill };
            ui.add(egui::Button::new(RichText::new(label).font(theme::regular(12.))).fill(fill).corner_radius(CornerRadius::same(12))).clicked()
        };
        if !fun.recent_gifs.is_empty() && chip(ui, "Recents", fun.gif_recents) {
            fun.gif_recents = true;
        }
        if chip(ui, "Trending", !fun.gif_recents && wanted.is_empty()) {
            fun.gif_recents = false;
            fun.gif_query.clear();
            fun.gif_typed = None;
        }
        for (label, term) in GIF_CATEGORIES {
            if chip(ui, label, !fun.gif_recents && wanted == term) {
                fun.gif_recents = false;
                fun.gif_query = term.to_owned();
                fun.gif_typed = None;
            }
        }
    });
    if field.changed() {
        fun.gif_recents = false;
    }
    if fun.gif_recents {
        return recent_gifs(ui, palette, fun);
    }
    ui.add_space(4.);
    ui.label(RichText::new(if wanted.is_empty() { "Trending" } else { "Search Results" }).font(theme::semibold(13.)).color(palette.text_secondary));
    if let Some(error) = fun.gif_error.clone() {
        ui.add_space(30.);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(error).color(palette.text_secondary));
            ui.add_space(8.);
            if widgets::pill_button(ui, "Retry", false).clicked() {
                fun.gif_error = None;
                fun.search_gifs(app, 0);
            }
        });
        return None;
    }
    if fun.gifs.is_empty() {
        ui.add_space(30.);
        ui.vertical_centered(|ui| {
            if fun.gif_loading {
                ui.spinner();
            } else {
                ui.label(RichText::new("No GIFs found").color(palette.text_secondary));
            }
        });
        return None;
    }
    let col = (ui.available_width() - 6.) / 2.;
    let mut more = false;
    let output = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Two columns, each GIF at its shape, Desktop's masonry.
        let top = ui.cursor().top();
        let mut heights = [0f32; 2];
        for (gif, path) in &fun.gifs {
            let column = if heights[0] <= heights[1] { 0 } else { 1 };
            let h = (col * gif.preview.height.max(1) as f32 / gif.preview.width.max(1) as f32).clamp(60., 240.);
            let rect = Rect::from_min_size(Pos2::new(ui.max_rect().left() + column as f32 * (col + 6.), top + heights[column]), Vec2::new(col, h));
            let r = ui.interact(rect, ui.scope_id().with(("gif", &gif.id)), Sense::click());
            // Giphy's previews are MP4s: the first frame still, playing
            // while hovered (Desktop's `FunGif` plays them in <video>).
            if ui.is_rect_visible(rect) {
                let file = path.to_string_lossy();
                let radius = CornerRadius::same(8);
                let played = r.hovered() && crate::video::inline(ui, &file, rect, radius);
                if !played {
                    match crate::video::poster(ui.ctx(), &file) {
                        Some(still) => crate::chat::cover_image(ui, &still, rect, radius),
                        None => {
                            ui.painter().rect_filled(rect, radius, palette.fill);
                        }
                    }
                }
            }
            if r.hovered() {
                ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(2., palette.badge), egui::StrokeKind::Inside);
            }
            if r.on_hover_text(&gif.title).clicked() {
                send = Some(Send::Gif(gif.clone()));
            }
            heights[column] += h + 6.;
        }
        ui.allocate_space(Vec2::new(ui.available_width(), heights[0].max(heights[1])));
    });
    let from_bottom = output.content_size.y - output.inner_rect.height() - output.state.clamped_offset().y;
    if from_bottom < 200. && !fun.gif_loading && fun.gif_next.is_some() {
        more = true;
    }
    if more && let Some(next) = fun.gif_next {
        fun.search_gifs(app, next);
    }
    ui.painter().text(ui.max_rect().right_bottom() - Vec2::new(4., 4.), Align2::RIGHT_BOTTOM, "Powered by GIPHY", theme::regular(10.), palette.text_secondary);
    send
}

/// The sticker manager, and a pack's preview, as dialogs.
pub fn dialogs(app: &mut App, ui: &Ui) {
    let palette = Palette::of(ui.ctx());
    let frame = egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24);
    if let Some((id, key, result)) = app.fun.preview.clone() {
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("sticker-pack")).frame(frame).show(ui.ctx(), |ui| {
            ui.set_width(420.);
            match &result {
                None => {
                    ui.label(RichText::new("Sticker pack").font(theme::semibold(18.)));
                    ui.add_space(30.);
                    ui.vertical_centered(|ui| ui.spinner());
                    ui.add_space(30.);
                }
                Some(Err(error)) => {
                    ui.label(RichText::new("Sticker pack").font(theme::semibold(18.)));
                    ui.add_space(12.);
                    ui.label(RichText::new(format!("This pack couldn't be opened: {error}")).color(palette.text_secondary));
                }
                Some(Ok(preview)) => {
                    ui.label(RichText::new(if preview.pack.title.is_empty() { "Sticker pack" } else { &preview.pack.title }).font(theme::semibold(18.)));
                    if !preview.pack.author.is_empty() {
                        ui.label(RichText::new(&preview.pack.author).color(palette.text_secondary));
                    }
                    ui.add_space(10.);
                    let cell = (ui.available_width() / 5.).floor();
                    egui::ScrollArea::vertical().max_height(320.).auto_shrink([false, true]).show(ui, |ui| {
                        for row in preview.stickers.chunks(5) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                for s in row {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(cell), Sense::hover());
                                    sticker_image(ui, &s.path, rect.shrink(6.));
                                }
                            });
                        }
                    });
                }
            }
            ui.add_space(14.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(Ok(preview)) = &result {
                    let installed = preview.pack.installed || app.fun.packs.iter().any(|p| p.id == id);
                    if installed {
                        if widgets::pill_button(ui, "Uninstall", false).clicked() {
                            app.send(Command::UninstallStickerPack { id: id.clone(), key: key.clone() });
                            close = true;
                        }
                    } else if widgets::pill_button(ui, "Install", true).clicked() {
                        app.send(Command::InstallStickerPack { id: id.clone(), key: key.clone() });
                        close = true;
                    }
                }
                if widgets::pill_button(ui, "Close", false).clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            app.fun.preview = None;
        }
        return;
    }
    if !app.fun.manager_open {
        return;
    }
    let mut close = false;
    let mut open = None;
    let manager = app.fun.manager.clone().unwrap_or_default();
    let modal = egui::Modal::new(egui::Id::unique("sticker-manager")).frame(frame).show(ui.ctx(), |ui| {
        ui.set_width(440.);
        ui.label(RichText::new("Sticker packs").font(theme::semibold(18.)));
        ui.add_space(8.);
        egui::ScrollArea::vertical().max_height(460.).auto_shrink([false, true]).show(ui, |ui| {
            for (title, packs, empty) in [
                ("Installed stickers", &manager.installed, "No stickers installed"),
                ("Signal Artist Series", &manager.blessed, ""),
                ("Stickers you received", &manager.received, "Stickers from incoming messages will appear here"),
            ] {
                if packs.is_empty() && empty.is_empty() {
                    continue;
                }
                ui.add_space(10.);
                ui.label(RichText::new(title).font(theme::semibold(14.)));
                if packs.is_empty() {
                    ui.label(RichText::new(empty).font(theme::regular(13.)).color(palette.text_secondary));
                }
                let installed_list = std::ptr::eq(packs, &manager.installed);
                for (i, pack) in packs.iter().enumerate() {
                    // Desktop drags installed packs into order; here they
                    // move a step from their row.
                    let moves = if installed_list { (i > 0, i + 1 < packs.len()) } else { (false, false) };
                    if let Some(action) = pack_row(ui, &palette, pack, title, moves) {
                        match action {
                            PackAction::Move(up) => {
                                let mut ids: Vec<Vec<u8>> = packs.iter().map(|p| p.id.clone()).collect();
                                ids.swap(i, if up { i - 1 } else { i + 1 });
                                app.send(Command::OrderStickerPacks(ids));
                            }
                            PackAction::Open => open = Some((pack.id.clone(), pack.key.clone())),
                            PackAction::Install => app.send(Command::InstallStickerPack { id: pack.id.clone(), key: pack.key.clone() }),
                            PackAction::Uninstall => app.send(Command::UninstallStickerPack { id: pack.id.clone(), key: pack.key.clone() }),
                        }
                    }
                }
            }
        });
        ui.add_space(12.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pill_button(ui, "Done", true).clicked() {
                close = true;
            }
        });
    });
    if close || modal.should_close() {
        app.fun.manager_open = false;
    }
    if let Some((id, key)) = open {
        app.send(Command::PreviewStickerPack { id: id.clone(), key: key.clone() });
        app.fun.preview = Some((id, key, None));
    }
}

enum PackAction {
    Move(bool),
    Open,
    Install,
    Uninstall,
}

fn pack_row(ui: &mut Ui, palette: &Palette, pack: &StickerPackSummary, section: &str, moves: (bool, bool)) -> Option<PackAction> {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
    }
    let cover = Rect::from_min_size(rect.min + Vec2::new(6., 6.), Vec2::splat(44.));
    match &pack.cover {
        Some(path) => sticker_image(ui, path, cover),
        None => {
            ui.painter().rect_filled(cover, CornerRadius::same(8), palette.fill_secondary);
        }
    }
    let title = if pack.title.is_empty() { "Sticker pack" } else { &pack.title };
    ui.painter().text(Pos2::new(rect.left() + 60., rect.top() + 10.), Align2::LEFT_TOP, title, theme::medium(14.), palette.text);
    let sub = if pack.author.is_empty() { format!("{} stickers", pack.sticker_count) } else { format!("{} · {} stickers", pack.author, pack.sticker_count) };
    ui.painter().text(Pos2::new(rect.left() + 60., rect.top() + 30.), Align2::LEFT_TOP, sub, theme::regular(12.), palette.text_secondary);
    let label = if pack.installed { "Uninstall" } else { "Install" };
    let g = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), palette.text);
    let button = Rect::from_min_size(Pos2::new(rect.right() - g.size().x - 30., rect.center().y - 14.), Vec2::new(g.size().x + 24., 28.));
    let br = ui.interact(button, ui.scope_id().with(("pack-button", section, &pack.id)), Sense::click());
    ui.painter().rect_filled(button, CornerRadius::same(14), if br.hovered() { palette.fill_pressed } else { palette.fill_secondary });
    ui.painter().galley(button.center() - g.size() / 2., g, palette.text);
    if br.clicked() {
        return Some(if pack.installed { PackAction::Uninstall } else { PackAction::Install });
    }
    // Up and down, before the button, for installed packs.
    let mut x = button.left() - 8.;
    for (show, up, symbol) in [(moves.1, false, "arrow-down"), (moves.0, true, "arrow-up")] {
        let at = Rect::from_center_size(Pos2::new(x - 14., rect.center().y), Vec2::splat(28.));
        x -= 32.;
        if !show {
            continue;
        }
        let mr = ui.interact(at, ui.scope_id().with(("pack-move", up, &pack.id)), Sense::click());
        if mr.hovered() {
            ui.painter().rect_filled(at, CornerRadius::same(14), palette.fill_pressed);
        }
        theme::paint_symbol(ui.painter(), symbol, at.center(), 16., palette.text_secondary);
        if mr.on_hover_text(if up { "Move up" } else { "Move down" }).clicked() {
            return Some(PackAction::Move(up));
        }
    }
    r.clicked().then_some(PackAction::Open)
}

#[cfg(test)]
mod gif_tests {
    #[test]
    fn recent_gifs_round_trip_and_drop_missing_previews() {
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().join("a.mp4");
        std::fs::write(&here, b"x").unwrap();
        let media = |w| langis_core::giphy::GifMedia { url: "https://example.org/a.mp4".into(), width: w, height: 100 };
        let gif = |id: &str| langis_core::giphy::Gif { id: id.into(), title: "t".into(), preview: media(200), original: media(480) };
        let saved = serde_json::Value::Array(vec![super::gif_json(&gif("a"), &here), super::gif_json(&gif("b"), &dir.path().join("gone.mp4"))]).to_string();
        let back = super::parse_recent_gifs(&saved);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, gif("a"));
        assert_eq!(back[0].1, here);
    }
}
