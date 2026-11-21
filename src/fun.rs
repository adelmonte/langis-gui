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
use crate::i18n::tr;

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
    pub asked: bool,
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
    /// "Search stickers".
    sticker_query: String,
    /// A sticker taken out of Recents from its menu, to save.
    forget_recent: Option<(Vec<u8>, u32)>,
    /// The Favorites button was clicked with none: Desktop's hint toast.
    favorites_hint: bool,
    /// "Hide sticker pack?" for this received pack (id, key).
    confirm_hide: Option<(Vec<u8>, Vec<u8>)>,
    /// GIFs sent lately, newest first (Desktop's Recents), kept in the
    /// `ui.recent_gifs` setting.
    pub recent_gifs: Vec<(Gif, PathBuf)>,
    /// Showing Recents rather than a search.
    gif_recents: bool,
    pub manager: Option<StickerManagerView>,
    pub manager_open: bool,
    /// Desktop's sticker pack creator, while open.
    pub creator: Option<crate::sticker_creator::Creator>,
    /// Why an upload failed, for a toast.
    pub creator_error: Option<String>,
    /// The manager's tab: All (0) or My Stickers (1).
    manager_tab: usize,
    /// Desktop's "Sticker details" for one of a previewed pack's stickers.
    sticker_details: Option<(Vec<u8>, Vec<u8>, StickerView)>,
    /// "Uninstall sticker pack?" for this pack (id, key).
    confirm_uninstall: Option<(Vec<u8>, Vec<u8>)>,
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
            Event::StickerUpload(upload) => {
                if let Some(why) = self.creator.as_mut().and_then(|c| c.on_upload(upload)) {
                    self.creator_error = Some(why);
                }
            }
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
    // Desktop's FunPicker: 352 by 524, radius 8, the tabs 48px tall.
    let size = Vec2::new(crate::emoji_picker::WIDTH, 524.);
    let area = egui::Area::new(egui::Id::unique("fun-picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(anchor.x, (anchor.y - size.y - 8.).max(8.)))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette.popover)
                .corner_radius(CornerRadius::same(8))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 90 } else { 50 }) })
                .inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 12 })
                .show(ui, |ui| {
                    ui.set_width(size.x - 32.);
                    ui.set_height(size.y - 12.);
                    tabs(ui, &palette, &mut fun.tab);
                    match fun.tab {
                        Tab::Emoji => picked = chat.emoji.body(ui, &app.recent_emoji),
                        Tab::Stickers => send = stickers(app, ui, &palette, &mut fun),
                        Tab::Gifs => send = gifs(app, ui, &palette, &mut fun),
                    }
                });
        });
    let outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !Rect::from_center_size(anchor, Vec2::splat(64.)).contains(p));
    // A hold that ended without a click leaves nothing behind.
    if ctx.input(|i| !i.pointer.any_down() && !i.pointer.any_released()) {
        ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("fun-held")));
    }
    let manager = fun.manager_open;
    let forget = fun.forget_recent.take();
    let hint = std::mem::take(&mut fun.favorites_hint);
    app.fun = fun;
    if hint {
        app.toast(tr("Right click a sticker to add to Favorites"));
    }
    if let Some((pack, sticker)) = forget {
        app.fun.recent_stickers.retain(|(p, s)| !(*p == pack && *s == sticker));
        save_recent_stickers(app);
    }
    if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) || manager {
        chat.emoji_open = false;
    }
    if let Some(what) = send {
        match what {
            Send::Sticker(pack, sticker) => {
                remember_sticker(app, &pack.id, sticker.id);
                let command = Command::SendSticker { conversation: c.id.clone(), pack, sticker, quote: chat.quote.take() };
                chat.scroll_to_bottom = 10;
                app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
            }
            // Desktop stages a GIF in the composer, to go with the text,
            // asking first when it would replace an attachment.
            Send::Gif(gif, path) => {
                remember_gif(app, &gif);
                if chat.staged.is_empty() {
                    chat.gif_draft = Some((gif, path));
                } else {
                    chat.gif_replacing = Some((gif, path));
                }
            }
        }
        chat.emoji_open = false;
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
    ("Thumbs Up", "thumbs up"),
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
    save_recent_stickers(app);
}

fn save_recent_stickers(app: &mut App) {
    let list = &app.fun.recent_stickers;
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
            if held(ui, &r) {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("fun-held"), true));
                let file = path.to_string_lossy().into_owned();
                let shape = Vec2::new(gif.preview.width.max(1) as f32, gif.preview.height.max(1) as f32);
                lightbox(ui, |ui, big| {
                    let fit = Rect::from_center_size(big.center(), shape * (big.width() / shape.x).min(big.height() / shape.y));
                    if !crate::video::inline(ui, &file, fit, CornerRadius::same(12)) {
                        ui.painter().rect_filled(fit, CornerRadius::same(12), Color32::from_gray(40));
                    }
                });
            }
            if r.on_hover_text(&gif.title).clicked() && !ui.ctx().data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("fun-held"))).unwrap_or(false) {
                send = Some(Send::Gif(gif.clone(), path.clone()));
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
    Gif(Gif, PathBuf),
}

/// Desktop's `FunTabs`: the three, centered in a 48px row.
fn tabs(ui: &mut Ui, palette: &Palette, tab: &mut Tab) {
    let labels = [(Tab::Emoji, tr("Emoji")), (Tab::Stickers, tr("Stickers")), (Tab::Gifs, tr("GIFs"))];
    let widths: Vec<f32> = labels.iter().map(|(_, l)| ui.painter().layout_no_wrap((*l).to_owned(), theme::medium(13.), palette.text).size().x + 24.).collect();
    let inner: f32 = widths.iter().sum::<f32>() + 4. * (labels.len() - 1) as f32;
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 48.), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(48.);
        ui.add_space(((ui.available_width() - inner) / 2.).max(0.));
        ui.spacing_mut().item_spacing.x = 4.;
        for (t, label) in labels {
            let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), palette.text);
            let (rect, r) = ui.allocate_exact_size(Vec2::new(galley.size().x + 24., 28.), Sense::click());
            let on = *tab == t;
            if on || r.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(14), if on { palette.fill_pressed } else { palette.fill });
            }
            ui.painter().galley(rect.center() - galley.size() / 2., galley, if on { palette.text } else { palette.text_secondary });
            if r.clicked() {
                *tab = t;
            }
        }
    });
}

/// Desktop's `FunLightbox`: pressing and holding a sticker or GIF shows
/// it large until let go (and then it isn't sent). True while it shows.
fn held(ui: &Ui, r: &egui::Response) -> bool {
    r.is_pointer_button_down_on() && ui.input(|i| i.pointer.press_start_time().is_some_and(|t| i.time - t > 0.4))
}

fn lightbox(ui: &Ui, paint: impl FnOnce(&Ui, Rect)) {
    let screen = ui.ctx().content_rect();
    egui::Area::new(egui::Id::unique("fun-lightbox")).order(egui::Order::Tooltip).fixed_pos(screen.min).interactable(false).show(ui.ctx(), |ui| {
        ui.painter().rect_filled(screen, 0, Color32::from_black_alpha(160));
        let side = (screen.width().min(screen.height()) * 0.5).min(360.);
        paint(ui, Rect::from_center_size(screen.center(), Vec2::splat(side)));
    });
    ui.ctx().request_repaint();
}

/// A sticker's image in `rect`.
fn sticker_image(ui: &Ui, path: &std::path::Path, rect: Rect) {
    // Animated stickers play while hovered in the picker, as Desktop's do.
    let file = path.to_string_lossy();
    if ui.rect_contains_pointer(rect) && crate::anim::paint(ui, &file, rect) {
        return;
    }
    egui::Image::new(crate::thumbs::uri(ui.ctx(), &file, rect.size())).maintain_aspect_ratio(true).fit_to_exact_size(rect.size()).paint_at(ui, rect);
}

/// Desktop's `FunPanelStickers`: Search stickers, the sub-nav (Recently
/// Used, Favorites, each pack, then add), and one scroll through them all,
/// four across, each section titled.
fn stickers(app: &App, ui: &mut Ui, palette: &Palette, fun: &mut Fun) -> Option<Send> {
    let mut send = None;
    widgets::text_field(ui, &mut fun.sticker_query, tr("Search stickers"), false);
    ui.add_space(6.);
    let query = fun.sticker_query.trim().to_owned();
    type Item = (Arc<StickerPackView>, StickerView);
    // The sections: (title, sub-nav cover, stickers).
    let mut sections: Vec<(String, Option<std::path::PathBuf>, &'static str, Vec<Item>)> = Vec::new();
    if !query.is_empty() {
        let options = langis_emoji::SearchOptions { limit: 60, ..Default::default() };
        let mut wanted: Vec<&str> = langis_emoji::search(&query, &options).into_iter().map(|m| m.emoji.glyph).collect();
        wanted.push(query.as_str());
        let strip = |e: &str| e.chars().filter(|c| !matches!(c, '\u{fe0f}' | '\u{fe0e}')).collect::<String>();
        let wanted: Vec<String> = wanted.into_iter().map(strip).collect();
        let found: Vec<Item> = fun.packs.iter().flat_map(|p| p.stickers.iter().filter(|s| s.emoji.as_deref().is_some_and(|e| wanted.contains(&strip(e)))).map(|s| (p.clone(), s.clone()))).collect();
        sections.push((tr("Search Results").to_owned(), None, "search", found));
    } else {
        let recents: Vec<Item> = fun
            .recent_stickers
            .iter()
            .filter_map(|(pack, sticker)| {
                let p = fun.packs.iter().find(|p| &p.id == pack)?;
                Some((p.clone(), p.stickers.iter().find(|s| s.id == *sticker)?.clone()))
            })
            .collect();
        if !recents.is_empty() {
            sections.push((tr("Recently Used").to_owned(), None, "recent", recents));
        }
        let favorites: Vec<Item> = fun
            .favorites
            .iter()
            .map(|f| {
                let pack = fun.packs.iter().find(|p| p.id == f.pack_id).cloned().unwrap_or_else(|| {
                    Arc::new(StickerPackView { id: f.pack_id.clone(), key: f.pack_key.clone(), title: String::new(), author: String::new(), stickers: vec![f.sticker.clone()] })
                });
                (pack, f.sticker.clone())
            })
            .collect();
        // Desktop keeps the Favorites button while favorites are on, empty
        // or not (`FunPanelStickers`, commit 5b083e565).
        if !favorites.is_empty() || langis_core::remote_config::current().sticker_favorites() {
            sections.push((tr("Favorites").to_owned(), None, "star", favorites));
        }
        for p in &fun.packs {
            let title = if p.title.is_empty() { tr("Sticker Pack").to_owned() } else { p.title.clone() };
            sections.push((title, p.stickers.first().map(|s| s.path.clone()), "", p.stickers.iter().map(|s| (p.clone(), s.clone())).collect()));
        }
    }
    let cell = (ui.available_width() / 4.).floor();
    const HEADER: f32 = 32.;
    let mut tops = Vec::new();
    let mut y = 0.;
    for (_, _, _, items) in &sections {
        tops.push(y);
        y += HEADER + items.len().div_ceil(4) as f32 * cell;
    }
    let total = y;
    let offset_id = ui.scope_id().with("sticker-scroll");
    let offset: f32 = ui.ctx().data(|d| d.get_temp(offset_id)).unwrap_or(0.);
    let current = tops.iter().rposition(|t| *t <= offset + 1.).unwrap_or(0);
    let mut jump = None;
    // The sub-nav: a section's symbol or its pack's cover, then add.
    if query.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            egui::ScrollArea::horizontal().id_salt("sticker-subnav").max_width(ui.available_width() - 40.).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, (title, cover, symbol, _)) in sections.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
                        let on = i == current;
                        if on || r.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
                        }
                        match cover {
                            Some(path) => sticker_image(ui, path, rect.shrink(4.)),
                            None => theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., palette.text),
                        }
                        if r.on_hover_text(title.as_str()).clicked() {
                            if *symbol == "star" && sections[i].3.is_empty() {
                                fun.favorites_hint = true;
                            } else {
                                jump = Some(i);
                            }
                        }
                    }
                });
            });
            if widgets::icon_button(ui, Icon::Plus, 18., tr("Add stickers")).clicked() {
                fun.manager_open = true;
                app.send(Command::LoadStickerManager);
            }
        });
        ui.add_space(6.);
    }
    if sections.iter().all(|(_, _, _, items)| items.is_empty()) {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            let text = if !query.is_empty() { tr("No stickers found") } else { tr("No sticker packs, add stickers to send to your friends.") };
            ui.label(RichText::new(text).color(palette.text_secondary));
            if query.is_empty() {
                ui.add_space(8.);
                if widgets::pill_button(ui, tr("Add stickers"), true).clicked() {
                    fun.manager_open = true;
                    app.send(Command::LoadStickerManager);
                }
            }
        });
        return None;
    }
    let mut area = egui::ScrollArea::vertical().id_salt("sticker-grid").auto_shrink([false, false]);
    if let Some(i) = jump {
        area = area.vertical_scroll_offset(tops[i]);
    }
    let output = area.show_viewport(ui, |ui, viewport| {
        ui.set_height(total);
        let origin = ui.min_rect().min;
        for (si, (title, _, _, items)) in sections.iter().enumerate() {
            let top = tops[si];
            let bottom = top + HEADER + items.len().div_ceil(4) as f32 * cell;
            if bottom < viewport.top() || top > viewport.bottom() {
                continue;
            }
            ui.painter().text(origin + Vec2::new(4., top + HEADER / 2.), Align2::LEFT_CENTER, title, theme::semibold(13.), palette.text_secondary);
            for (i, (pack, sticker)) in items.iter().enumerate() {
                let rect = Rect::from_min_size(origin + Vec2::new((i % 4) as f32 * cell, top + HEADER + (i / 4) as f32 * cell), Vec2::splat(cell));
                if rect.bottom() < origin.y + viewport.top() || rect.top() > origin.y + viewport.bottom() {
                    continue;
                }
                let r = ui.interact(rect, ui.scope_id().with(("sticker", si, i)), Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                }
                sticker_image(ui, &sticker.path, rect.shrink(6.));
                let favorite = fun.favorite_ids.iter().any(|(p, s)| *p == pack.id && *s == sticker.id)
                    || fun.favorites.iter().any(|f| f.pack_id == pack.id && f.sticker.id == sticker.id);
                let show_recents_menu = title == tr("Recently Used");
                let was_recents = fun.sticker_recents;
                fun.sticker_recents = show_recents_menu;
                let r = r.on_hover_text(sticker.emoji.clone().unwrap_or_default());
                // Its menu: favorite it or not, its pack, out of Recents.
                let menu = egui::Id::unique(("sticker-menu", &pack.id, sticker.id));
                if r.secondary_clicked()
                    && let Some(at) = r.interact_pointer_pos()
                {
                    crate::menu::open_at(ui.ctx(), menu, at);
                }
                crate::menu::show(ui.ctx(), menu, |m| {
                    if m.item(Some(if favorite { "star-slash" } else { "star" }), if favorite { "Remove from favorites" } else { "Add to favorites" }) {
                        app.send(Command::SetStickerFavorite { pack_id: pack.id.clone(), pack_key: pack.key.clone(), sticker_id: sticker.id, favorite: !favorite });
                    }
                    if !pack.title.is_empty() && m.item(Some("stickerpack"), "View sticker pack") {
                        fun.sticker_query.clear();
                        fun.pack = Some(pack.id.clone());
                        fun.sticker_recents = false;
                        fun.chose_pack = true;
                    }
                    if fun.sticker_recents && m.item(Some("x-circle"), "Remove from recents") {
                        fun.forget_recent = Some((pack.id.clone(), sticker.id));
                    }
                });
                if held(ui, &r) {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("fun-held"), true));
                    lightbox(ui, |ui, big| sticker_image(ui, &sticker.path, big));
                }
                if r.clicked() && !ui.ctx().data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("fun-held"))).unwrap_or(false) {
                    send = Some(Send::Sticker(pack.clone(), sticker.clone()));
                }
                fun.sticker_recents = was_recents;
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(offset_id, output.state.clamped_offset().y));
    send
}

fn gifs(app: &App, ui: &mut Ui, palette: &Palette, fun: &mut Fun) -> Option<Send> {
    let mut send = None;
    let field = widgets::text_field(ui, &mut fun.gif_query, tr("Search GIFs"), false);
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
    // Desktop's GIF sub-nav: Recents, Trending, then its categories (each
    // a search), as icons.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        let mut items: Vec<(&str, Option<&str>, &str, bool)> = Vec::new();
        if !fun.recent_gifs.is_empty() {
            items.push(("Recently Used", Some("recent"), "", fun.gif_recents));
        }
        items.push(("Trending", Some("trending"), "", !fun.gif_recents && wanted.is_empty()));
        for (label, term) in GIF_CATEGORIES {
            let symbol = match term {
                "love" => Some("heart"),
                "thumbs up" => Some("thumbsup"),
                "excited" => Some("face-excited"),
                "sad" => Some("face-sad"),
                "angry" => Some("face-angry"),
                _ => None,
            };
            items.push((label, symbol, term, !fun.gif_recents && wanted == term));
        }
        let w = ui.available_width() / items.len() as f32;
        for (label, symbol, term, on) in items {
            let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 32.), Sense::click());
            let cell = Rect::from_center_size(rect.center(), Vec2::splat(30.));
            if on || r.hovered() {
                ui.painter().rect_filled(cell, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
            }
            let color = if on { palette.text } else { palette.text_secondary };
            match symbol {
                Some(symbol) => theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., color),
                None => {
                    let glyph = if term == "celebrate" { "🎉" } else { "😮" };
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, theme::emoji(16.), palette.text);
                }
            }
            if r.on_hover_text(tr(label)).clicked() {
                match label {
                    "Recently Used" => fun.gif_recents = true,
                    "Trending" => {
                        fun.gif_recents = false;
                        fun.gif_query.clear();
                        fun.gif_typed = None;
                    }
                    _ => {
                        fun.gif_recents = false;
                        fun.gif_query = term.to_owned();
                        fun.gif_typed = None;
                    }
                }
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
    ui.label(RichText::new(if wanted.is_empty() { tr("Trending") } else { tr("Search Results") }).font(theme::semibold(13.)).color(palette.text_secondary));
    if let Some(error) = fun.gif_error.clone() {
        ui.add_space(30.);
        ui.vertical_centered(|ui| {
            // Desktop's words, unless the key's what's missing (langis's own).
            let text = if error.contains("API key") { error } else { tr("Couldn\u{2019}t load GIFs. Check your connection and try again").to_owned() };
            ui.label(RichText::new(text).color(palette.text_secondary));
            ui.add_space(8.);
            if widgets::pill_button(ui, tr("Retry"), false).clicked() {
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
                crate::widgets::spinner(ui, 24.);
            } else {
                ui.label(RichText::new(tr("No GIFs found")).color(palette.text_secondary));
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
            if held(ui, &r) {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("fun-held"), true));
                let file = path.to_string_lossy().into_owned();
                let shape = Vec2::new(gif.preview.width.max(1) as f32, gif.preview.height.max(1) as f32);
                lightbox(ui, |ui, big| {
                    let fit = Rect::from_center_size(big.center(), shape * (big.width() / shape.x).min(big.height() / shape.y));
                    if !crate::video::inline(ui, &file, fit, CornerRadius::same(12)) {
                        ui.painter().rect_filled(fit, CornerRadius::same(12), Color32::from_gray(40));
                    }
                });
            }
            if r.on_hover_text(&gif.title).clicked() && !ui.ctx().data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("fun-held"))).unwrap_or(false) {
                send = Some(Send::Gif(gif.clone(), path.clone()));
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
    ui.painter().text(ui.max_rect().right_bottom() - Vec2::new(4., 4.), Align2::RIGHT_BOTTOM, tr("Powered by GIPHY"), theme::regular(10.), palette.text_secondary);
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
                    ui.label(RichText::new(tr("Sticker Pack")).font(theme::semibold(18.)));
                    ui.add_space(30.);
                    ui.vertical_centered(|ui| crate::widgets::spinner(ui, 24.));
                    ui.add_space(30.);
                }
                Some(Err(error)) => {
                    ui.label(RichText::new(tr("Sticker Pack")).font(theme::semibold(18.)));
                    ui.add_space(12.);
                    let _ = error;
                    ui.label(RichText::new(tr("Error opening sticker pack. Check your internet connection and try again.")).color(palette.text_secondary));
                }
                Some(Ok(preview)) => {
                    ui.label(RichText::new(if preview.pack.title.is_empty() { tr("Sticker Pack") } else { &preview.pack.title }).font(theme::semibold(18.)));
                    if !preview.pack.author.is_empty() {
                        ui.label(RichText::new(&preview.pack.author).color(palette.text_secondary));
                    }
                    let n = preview.stickers.len();
                    ui.label(RichText::new(crate::i18n::tf("icu:stickers--StickerPreview--StickerCount", &if n == 1 { "1 sticker".to_owned() } else { format!("{n} stickers") }, &[("count", crate::i18n::Arg::from(n))])).font(theme::regular(13.)).color(palette.text_secondary));
                    ui.add_space(10.);
                    let cell = (ui.available_width() / 5.).floor();
                    egui::ScrollArea::vertical().max_height(320.).auto_shrink([false, true]).show(ui, |ui| {
                        for row in preview.stickers.chunks(5) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                for s in row {
                                    let (rect, hit) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                                    if hit.hovered() {
                                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                                    }
                                    sticker_image(ui, &s.path, rect.shrink(6.));
                                    // Desktop's "See sticker details".
                                    if hit.on_hover_text(tr("See sticker details")).clicked() {
                                        app.fun.sticker_details = Some((preview.pack.id.clone(), preview.pack.key.clone(), s.clone()));
                                    }
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
                        if widgets::pill_button(ui, tr("Remove"), false).clicked() {
                            app.fun.confirm_uninstall = Some((id.clone(), key.clone()));
                        }
                    } else if widgets::pill_button(ui, tr("Add Stickers"), true).clicked() {
                        app.send(Command::InstallStickerPack { id: id.clone(), key: key.clone() });
                        close = true;
                    }
                    // Desktop's share menu's Copy link.
                    if widgets::pill_button(ui, tr("Link"), false).clicked() {
                        ui.ctx().copy_text(pack_link(&id, &key));
                        app.toast(tr("Link copied"));
                    }
                }
                if widgets::pill_button(ui, tr("Close"), false).clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            app.fun.preview = None;
        }
        uninstall_confirm(app, ui);
        sticker_details(app, ui);
        return;
    }
    if !app.fun.manager_open {
        return;
    }
    let mut close = false;
    let mut open = None;
    let manager = app.fun.manager.clone().unwrap_or_default();
    let mut tab = app.fun.manager_tab;
    let modal = egui::Modal::new(egui::Id::unique("sticker-manager")).frame(frame).show(ui.ctx(), |ui| {
        ui.set_width(440.);
        ui.label(RichText::new("Sticker packs").font(theme::semibold(18.)));
        ui.add_space(8.);
        // Desktop's StickerManager tabs.
        ui.horizontal(|ui| {
            let half = ui.available_width() / 2.;
            for (i, label) in [tr("All"), tr("My Stickers")].iter().enumerate() {
                let (r, hit) = ui.allocate_exact_size(Vec2::new(half - 4., 32.), Sense::click());
                let on = tab == i;
                ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, theme::semibold(14.), if on { palette.text } else { palette.text_secondary });
                if on {
                    ui.painter().hline(r.x_range().shrink(half / 4.), r.bottom() - 1., Stroke::new(2., palette.text));
                }
                if hit.clicked() {
                    tab = i;
                }
            }
        });
        ui.add_space(4.);
        // Desktop's sections: (title, packs, shown when empty, description).
        let sections: Vec<(&str, &Vec<StickerPackSummary>, &str, &str)> = if tab == 1 {
            vec![(tr("My Stickers"), &manager.installed, tr("No sticker packs, add stickers to send to your friends."), "")]
        } else {
            vec![
                (tr("Signal Artist Series"), &manager.blessed, tr("No Signal Artist stickers available"), ""),
                (tr("Shared With You"), &manager.received, "", tr("When you receive a sticker from someone, the sticker pack will appear here.")),
            ]
        };
        egui::ScrollArea::vertical().max_height(460.).min_scrolled_height(300.).auto_shrink([false, true]).show(ui, |ui| {
            for (title, packs, empty, description) in sections {
                if packs.is_empty() && empty.is_empty() {
                    continue;
                }
                ui.add_space(10.);
                ui.label(RichText::new(title).font(theme::semibold(14.)));
                if !description.is_empty() {
                    ui.label(RichText::new(description).font(theme::regular(12.)).color(palette.text_secondary));
                }
                if packs.is_empty() {
                    ui.label(RichText::new(empty).font(theme::regular(13.)).color(palette.text_secondary));
                }
                let installed_list = std::ptr::eq(packs, &manager.installed);
                // Desktop's Hide, for packs shared with us (commits 5edf85d87,
                // 8049395d9).
                let hideable = std::ptr::eq(packs, &manager.received) && langis_core::remote_config::current().sticker_pack_hiding();
                let mut rows = Vec::new();
                for (i, pack) in packs.iter().enumerate() {
                    let top = ui.cursor().top();
                    let action = pack_row(ui, &palette, pack, title, (false, false), hideable);
                    rows.push(Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top()));
                    if let Some(action) = action {
                        match action {
                            PackAction::Move(up) => {
                                let mut ids: Vec<Vec<u8>> = packs.iter().map(|p| p.id.clone()).collect();
                                ids.swap(i, if up { i - 1 } else { i + 1 });
                                app.send(Command::OrderStickerPacks(ids));
                            }
                            PackAction::Open => open = Some((pack.id.clone(), pack.key.clone())),
                            PackAction::Install => app.send(Command::InstallStickerPack { id: pack.id.clone(), key: pack.key.clone() }),
                            PackAction::Uninstall => app.fun.confirm_uninstall = Some((pack.id.clone(), pack.key.clone())),
                            PackAction::Hide => app.fun.confirm_hide = Some((pack.id.clone(), pack.key.clone())),
                            PackAction::Copy => {
                                ui.ctx().copy_text(pack_link(&pack.id, &pack.key));
                                app.toast("Link copied");
                            }
                        }
                    }
                }
                // Desktop drags installed packs into order.
                if installed_list
                    && let Some((from, to)) = widgets::reorder_rows(ui, egui::Id::unique("sticker-pack-reorder"), &rows, palette.accent)
                {
                    let mut ids: Vec<Vec<u8>> = packs.iter().map(|p| p.id.clone()).collect();
                    let moved = ids.remove(from);
                    ids.insert(to, moved);
                    app.send(Command::OrderStickerPacks(ids));
                }
            }
        });
        ui.add_space(12.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pill_button(ui, tr("Done"), true).clicked() {
                close = true;
            }
        });
    });
    app.fun.manager_tab = tab;
    if (close || modal.should_close()) && app.fun.confirm_uninstall.is_none() && app.fun.confirm_hide.is_none() {
        app.fun.manager_open = false;
    }
    uninstall_confirm(app, ui);
    hide_confirm(app, ui);
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
    Copy,
    Hide,
}

/// Desktop's `HidePackDialog` before a received pack is hidden.
fn hide_confirm(app: &mut App, ui: &Ui) {
    let Some((id, key)) = app.fun.confirm_hide.clone() else { return };
    match crate::dialog::confirm(
        ui.ctx(),
        egui::Id::unique("sticker-hide"),
        tr("Hide Sticker Pack?"),
        tr("This will remove the sticker pack. You can't add it again without a link or sticker message."),
        tr("Hide"),
        crate::dialog::Variant::SubtleDestructive,
    ) {
        Some(crate::dialog::Choice::Action) => {
            app.fun.confirm_hide = None;
            app.send(Command::SetStickerPackHidden { id, key, hidden: true });
        }
        Some(crate::dialog::Choice::Cancel) => app.fun.confirm_hide = None,
        None => {}
    }
}

/// Desktop's sticker details: the sticker large, its emoji, and the
/// favorite button.
fn sticker_details(app: &mut App, ui: &Ui) {
    let Some((pack_id, pack_key, sticker)) = app.fun.sticker_details.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let favorite = app.fun.favorite_ids.iter().any(|(p, s)| *p == pack_id && *s == sticker.id);
    // Desktop offers Add only for an installed pack, and names the pack
    // (`StickerPreviewModal`'s details popover, commit 4cbe3539e).
    let pack = app.fun.preview.as_ref().and_then(|(_, _, r)| r.as_ref()?.as_ref().ok()).map(|p| p.pack.clone());
    let installed = pack.as_ref().is_some_and(|p| p.installed) || app.fun.packs.iter().any(|p| p.id == pack_id);
    let mut toggle = false;
    let closed = crate::panes::modal(ui, "sticker-details", 300., |ui| {
        crate::panes::title(ui, tr("Sticker details"));
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(160.), Sense::hover());
            sticker_image(ui, &sticker.path, rect);
            if let Some(e) = &sticker.emoji {
                let mut job = egui::text::LayoutJob::default();
                crate::chat::append_with_emoji(&mut job, e, egui::TextFormat { font_id: theme::regular(28.), color: palette.text, ..Default::default() });
                ui.label(job);
            }
            if let Some(pack) = pack.as_ref().filter(|p| !p.title.is_empty() || !p.author.is_empty()) {
                let line = [pack.title.as_str(), pack.author.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" · ");
                ui.label(RichText::new(line).font(theme::regular(12.)).color(palette.text_secondary));
            }
            ui.add_space(8.);
            if favorite || installed {
                toggle = widgets::pill_button(ui, if favorite { tr("Remove from Favorites") } else { tr("Add to Favorites") }, false).clicked();
            }
        });
    });
    if toggle {
        app.send(Command::SetStickerFavorite { pack_id, pack_key, sticker_id: sticker.id, favorite: !favorite });
        // (At the limit the backend says why instead.)
        if favorite || app.fun.favorite_ids.len() < 500 {
            app.toast(if favorite { tr("Sticker removed from Favorites") } else { tr("Sticker added to Favorites") });
        }
    }
    if closed || toggle {
        app.fun.sticker_details = None;
    }
}

/// A pack's signal.art link.
fn pack_link(id: &[u8], key: &[u8]) -> String {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    format!("https://signal.art/addstickers/#pack_id={}&pack_key={}", hex(id), hex(key))
}

/// Desktop's "Uninstall sticker pack?" before it goes.
fn uninstall_confirm(app: &mut App, ui: &Ui) {
    let Some((id, key)) = app.fun.confirm_uninstall.clone() else { return };
    match crate::dialog::confirm(
        ui.ctx(),
        egui::Id::unique("sticker-uninstall"),
        "",
        tr("You may not be able to re-install this sticker pack if you no longer have the source message."),
        tr("Uninstall"),
        crate::dialog::Variant::StrongDestructive,
    ) {
        Some(crate::dialog::Choice::Action) => {
            app.fun.confirm_uninstall = None;
            app.fun.preview = None;
            app.send(Command::UninstallStickerPack { id, key });
        }
        Some(crate::dialog::Choice::Cancel) => app.fun.confirm_uninstall = None,
        None => {}
    }
}

fn pack_row(ui: &mut Ui, palette: &Palette, pack: &StickerPackSummary, section: &str, moves: (bool, bool), hideable: bool) -> Option<PackAction> {
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
    let t = ui.painter().text(Pos2::new(rect.left() + 60., rect.top() + 10.), Align2::LEFT_TOP, title, theme::medium(14.), palette.text);
    // Desktop's Signal Artist badge after a blessed pack's title.
    if pack.blessed {
        theme::paint_symbol(ui.painter(), "badge-fill", Pos2::new(t.right() + 10., t.center().y), 14., palette.badge);
    }
    let sub = if pack.author.is_empty() { crate::i18n::tf("icu:stickers--StickerPreview--StickerCount", &format!("{} stickers", pack.sticker_count), &[("count", crate::i18n::Arg::from(&pack.sticker_count))]) } else { format!("{} · {} stickers", pack.author, pack.sticker_count) };
    ui.painter().text(Pos2::new(rect.left() + 60., rect.top() + 30.), Align2::LEFT_TOP, sub, theme::regular(12.), palette.text_secondary);
    let label = if pack.installed { tr("Uninstall") } else { tr("Install") };
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
    let menu = egui::Id::unique(("pack-menu", section, &pack.id));
    if r.secondary_clicked()
        && let Some(at) = r.interact_pointer_pos()
    {
        crate::menu::open_at(ui.ctx(), menu, at);
    }
    let (mut copy, mut hide) = (false, false);
    crate::menu::show(ui.ctx(), menu, |m| {
        if m.item(Some("link"), tr("Copy link")) {
            copy = true;
        }
        if hideable && m.item(Some("visible-slash"), tr("Hide")) {
            hide = true;
        }
    });
    if copy {
        return Some(PackAction::Copy);
    }
    if hide {
        return Some(PackAction::Hide);
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
