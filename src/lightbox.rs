//! Desktop's `Lightbox`: a chat's photos and videos full size over the
//! window, with the sender and date on top, arrows (and arrow keys) to go
//! through the chat's media, thumbnails below, and Save. View-once media
//! opens in it too, alone, with no Save, and its copy is deleted on close.

use std::path::PathBuf;

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use langis_core::Command;
use langis_store::{ConversationId, Message};

use crate::app::{App, Loaded};
use crate::icons::Icon;
use crate::theme;
use crate::i18n::tr;

/// One photo or video in the viewer.
#[derive(Clone, Debug)]
pub struct Item {
    pub message_id: i64,
    pub index: usize,
    pub path: Option<String>,
    pub thumbnail: Option<String>,
    pub content_type: String,
    pub author: Option<String>,
    pub author_name: Option<String>,
    pub sent_at: i64,
    pub caption: String,
    pub size: Option<(u32, u32)>,
}

pub struct Lightbox {
    pub conversation: Option<ConversationId>,
    pub items: Vec<Item>,
    pub index: usize,
    /// A view-once copy, deleted when the viewer closes.
    pub view_once: Option<PathBuf>,
    /// Desktop's click to zoom: the photo at 3 times, panned by the pointer.
    pub zoomed: bool,
    /// The video shown, playing in the viewer (Desktop's `<video>`).
    pub player: Option<crate::video::Player>,
}

/// Photos and videos of a message, as viewer items.
pub fn items_of(m: &Message) -> Vec<Item> {
    m.attachments
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.sticker && (a.content_type.starts_with("image/") || a.content_type.starts_with("video/")))
        .map(|(index, a)| Item {
            message_id: m.id,
            index,
            path: a.local_path.clone(),
            thumbnail: a.thumbnail_path.clone(),
            content_type: a.content_type.clone(),
            author: m.author.clone(),
            author_name: m.author_name.clone(),
            sent_at: m.sent_at,
            caption: m.body.clone(),
            size: a.width.zip(a.height),
        })
        .collect()
}

impl App {
    /// Opens the viewer on attachment `index` of `m`, then fills in the
    /// rest of the chat's media around it.
    pub fn open_lightbox(&mut self, m: &Message, index: usize) {
        let items = items_of(m);
        let at = items.iter().position(|i| i.index == index).unwrap_or(0);
        self.lightbox = Some(Lightbox { conversation: Some(m.conversation_id.clone()), items, index: at, view_once: None, zoomed: false, player: None });
        let (conversation, message_id) = (m.conversation_id.clone(), m.id);
        self.load(move |db| async move {
            // Every page of the chat's media, not only the newest.
            let mut messages = crate::media_gallery::all_media(&db, &conversation).await?;
            messages.reverse();
            let items: Vec<Item> = messages.iter().flat_map(items_of).collect();
            Some(Loaded::Lightbox { conversation, message_id, index, items })
        });
    }

    /// The whole chat's media came: keep showing the same item.
    pub fn lightbox_loaded(&mut self, conversation: ConversationId, message_id: i64, index: usize, items: Vec<Item>) {
        let Some(lightbox) = self.lightbox.as_mut().filter(|l| l.conversation.as_ref() == Some(&conversation)) else { return };
        let current = lightbox.items.get(lightbox.index).map(|i| (i.message_id, i.index)).unwrap_or((message_id, index));
        if let Some(at) = items.iter().position(|i| (i.message_id, i.index) == current) {
            lightbox.items = items;
            lightbox.index = at;
        }
    }

    /// A message changed: an attachment may have finished downloading.
    pub fn lightbox_update(&mut self, m: &Message) {
        let Some(lightbox) = self.lightbox.as_mut() else { return };
        let fresh = items_of(m);
        for item in lightbox.items.iter_mut().filter(|i| i.message_id == m.id) {
            if let Some(new) = fresh.iter().find(|f| f.index == item.index) {
                *item = new.clone();
            }
        }
    }

    /// Their view-once media, opened for its one viewing.
    pub fn open_view_once(&mut self, opened: langis_core::OpenedViewOnce) {
        let item = Item {
            message_id: opened.message_id,
            index: 0,
            path: Some(opened.path.to_string_lossy().into_owned()),
            thumbnail: None,
            content_type: opened.content_type,
            author: None,
            author_name: None,
            sent_at: 0,
            caption: String::new(),
            size: opened.size,
        };
        self.lightbox = Some(Lightbox { conversation: None, items: vec![item], index: 0, view_once: Some(opened.path), zoomed: false, player: None });
    }

    pub fn close_lightbox(&mut self) {
        let Some(lightbox) = self.lightbox.take() else { return };
        // Full size only while it's open.
        for item in &lightbox.items {
            if let Some(path) = &item.path {
                self.ctx.forget_image(&crate::thumbs::full_uri(path));
            }
        }
        if let Some(path) = lightbox.view_once {
            self.ctx.forget_image(&crate::thumbs::full_uri(&path.to_string_lossy()));
            if let Err(error) = std::fs::remove_file(&path) {
                tracing::warn!(%error, "couldn't delete the view-once copy");
            }
        }
    }
}

const ZOOM_SCALE: f32 = 3.;

/// Desktop's `IMAGE_SPRING_PROPS` (react-spring: tension 350, friction 20,
/// mass 0.5, clamped so it stops at the target instead of overshooting) for
/// the scale and the two offsets.
#[derive(Clone, Copy)]
struct Spring {
    index: usize,
    at: [f32; 3],
    velocity: [f32; 3],
}

fn spring(ctx: &egui::Context, id: egui::Id, index: usize, target: [f32; 3]) -> [f32; 3] {
    let mut z = ctx.data(|d| d.get_temp::<Spring>(id)).filter(|z| z.index == index).unwrap_or(Spring { index, at: [1., 0., 0.], velocity: [0.; 3] });
    let moving = z.step(target, ctx.input(|i| i.stable_dt).min(0.05));
    ctx.data_mut(|d| d.insert_temp(id, z));
    if moving {
        ctx.request_repaint();
    }
    z.at
}

impl Spring {
    /// Moves `dt` seconds towards `target`; true while still moving.
    fn step(&mut self, target: [f32; 3], dt: f32) -> bool {
        let steps = (dt / 0.002).ceil().max(1.) as usize;
        let h = dt / steps as f32;
        let mut moving = false;
        for k in 0..3 {
            for _ in 0..steps {
                let before = self.at[k] - target[k];
                if before == 0. && self.velocity[k] == 0. {
                    break;
                }
                let accel = (-350. * before - 20. * self.velocity[k]) / 0.5;
                self.velocity[k] += accel * h;
                self.at[k] += self.velocity[k] * h;
                if (self.at[k] - target[k]) * before < 0. {
                    self.at[k] = target[k];
                    self.velocity[k] = 0.;
                }
            }
            let precision = if k == 0 { 0.001 } else { 0.1 };
            if (self.at[k] - target[k]).abs() < precision && self.velocity[k].abs() < precision * 10. {
                self.at[k] = target[k];
                self.velocity[k] = 0.;
            } else {
                moving = true;
            }
        }
        moving
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(lightbox) = app.lightbox.as_ref() else { return };
    let ctx = ui.ctx().clone();
    let screen = ctx.content_rect();
    let view_once = lightbox.view_once.is_some();
    let count = lightbox.items.len();
    let index = lightbox.index.min(count.saturating_sub(1));
    let zoomed = lightbox.zoomed;
    let mut toggle_zoom = false;
    let Some(item) = lightbox.items.get(index).cloned() else {
        app.lightbox = None;
        return;
    };
    let mut close = false;
    let mut go: Option<usize> = None;
    let mut save = false;
    let mut forward = false;
    let mut fetch = false;
    let neighbors: Vec<(usize, Item)> = lightbox
        .items
        .iter()
        .enumerate()
        .skip(index.saturating_sub(10))
        .take(21)
        .map(|(i, it)| (i, it.clone()))
        .collect();
    // The video's player, when the item is a downloaded video: started
    // when it's shown (Desktop autoplays in the viewer), kept across frames.
    let video = !item.content_type.starts_with("image/") && item.path.is_some();
    let mut player = app.lightbox.as_mut().and_then(|l| l.player.take()).filter(|p| video && Some(p.path()) == item.path.as_deref());
    if video && player.is_none()
        && let Some(path) = item.path.as_deref()
    {
        let ppp = ctx.pixels_per_point();
        let max = ((screen.width() * ppp) as u32, (screen.height() * ppp) as u32);
        player = Some(crate::video::Player::new(path, max, false, false));
    }
    egui::Area::new(egui::Id::unique("lightbox")).order(egui::Order::Foreground).fixed_pos(screen.min).show(&ctx, |ui| {
        let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(rect, 0, Color32::from_black_alpha(242));

        // Desktop's `Lightbox__container--zoom`: the header and footer fade
        // out while zoomed.
        let chrome = 1. - ctx.animate_bool_with_time_and_easing(egui::Id::unique("lightbox-chrome"), zoomed, 0.15, egui::emath::easing::cubic_out);
        let previous = ui.opacity();
        ui.multiply_opacity(chrome);
        // Top bar: who and when, then Save and Close.
        let top = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 56.));
        if !view_once {
            let who = match &item.author {
                None => tr("You").to_owned(),
                Some(a) => app.names.get(a).cloned().or(item.author_name.clone()).unwrap_or_else(|| tr("Unknown").into()),
            };
            ui.painter().text(top.left_center() + Vec2::new(20., -8.), Align2::LEFT_CENTER, who, theme::semibold(14.), Color32::WHITE);
            let when = chrono::DateTime::from_timestamp_millis(item.sent_at)
                .map(|t| crate::format::localized(&t.with_timezone(&chrono::Local), "%b %-d, %Y, %-I:%M %p"))
                .unwrap_or_default();
            ui.painter().text(top.left_center() + Vec2::new(20., 10.), Align2::LEFT_CENTER, when, theme::regular(12.), Color32::from_gray(190));
        }
        let mut x = top.right() - 52.;
        if button(ui, Rect::from_min_size(Pos2::new(x, top.top() + 10.), Vec2::splat(36.)), Icon::X, tr("Close")) {
            close = true;
        }
        x -= 44.;
        if !view_once && button(ui, Rect::from_min_size(Pos2::new(x, top.top() + 10.), Vec2::splat(36.)), Icon::Save, tr("Save")) {
            save = true;
        }
        // Desktop's Lightbox Forward, for a message langis will forward.
        let forwardable = !view_once && forward_target(app, &item).is_some();
        x -= 44.;
        if forwardable && button(ui, Rect::from_min_size(Pos2::new(x, top.top() + 10.), Vec2::splat(36.)), Icon::Forward, tr("Forward")) {
            forward = true;
        }
        ui.set_opacity(previous);

        // The media, as large as fits.
        let strip_h = if count > 1 && !view_once { 72. } else { 0. };
        let caption_h = if item.caption.is_empty() || view_once { 0. } else { 40. };
        let area = Rect::from_min_max(Pos2::new(rect.left() + 72., top.bottom() + 8.), Pos2::new(rect.right() - 72., rect.bottom() - strip_h - caption_h - 16.));
        // A video's own shape once read (one we sent has no size stored).
        let size = player.as_ref().and_then(|p| p.probe()).map(|p| (p.width, p.height)).or(item.size);
        let (w, h) = size.map(|(w, h)| (w.max(1) as f32, h.max(1) as f32)).unwrap_or((4., 3.));
        let scale = (area.width() / w).min(area.height() / h).min(if size.is_some() { 2. } else { f32::MAX });
        let media = Rect::from_center_size(area.center(), Vec2::new(w * scale, h * scale));
        let image = item.content_type.starts_with("image/");
        let zoomable = image && item.path.is_some() && item.size.is_some() && !view_once;
        // Desktop's `positionImage`: 3 times the fitted size, offset by
        // three times the pointer's distance from the window's center, at
        // most the photo's own size, on the axes it overflows.
        let target = if zoomable && zoomed {
            let pointer = ctx.pointer_hover_pos().unwrap_or(rect.center());
            let pan = |overflows: bool, offset: f32, max: f32| if overflows { (offset * ZOOM_SCALE).clamp(-max, max) } else { 0. };
            [
                ZOOM_SCALE,
                pan(media.width() * ZOOM_SCALE > rect.width(), rect.center().x - pointer.x, media.width()),
                pan(media.height() * ZOOM_SCALE > rect.height(), rect.center().y - pointer.y, media.height()),
            ]
        } else {
            [1., 0., 0.]
        };
        let [s, tx, ty] = spring(&ctx, egui::Id::unique("lightbox-spring"), index, target);
        match item.path.as_deref().filter(|_| image).or(item.thumbnail.as_deref()) {
            Some(path) if zoomable && (s - 1.).abs() > 0.001 => {
                // `translate(x, y) scale(s)` about the photo's center, over
                // the header and footer.
                let shown = Rect::from_center_size(media.center() + Vec2::new(tx, ty), media.size() * s);
                ui.scope(|ui| {
                    ui.set_clip_rect(rect);
                    egui::Image::new(crate::thumbs::full_uri(path)).fit_to_exact_size(shown.size()).paint_at(ui, shown);
                });
            }
            Some(path) => {
                egui::Image::new(crate::thumbs::full_uri(path)).fit_to_exact_size(media.size()).paint_at(ui, media);
            }
            None => {
                ui.painter().rect_filled(media, CornerRadius::same(4), Color32::from_gray(40));
            }
        }
        if zoomable {
            let target = if zoomed { area } else { media };
            let r = ui.interact(target, ui.scope_id().with("lightbox-zoom"), Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(if zoomed { egui::CursorIcon::ZoomOut } else { egui::CursorIcon::ZoomIn });
            }
            if r.clicked() {
                toggle_zoom = true;
            }
            // Electron's menu on an image: Copy image (not for view-once).
            if let Some(path) = &item.path {
                crate::interaction::image_menu(ui, &r, std::path::Path::new(path));
            }
        }
        let center = Rect::from_center_size(media.center(), Vec2::splat(64.));
        let progress = app.downloads.get(&(item.message_id, item.index)).copied();
        if item.path.is_none()
            && let Some((received, total)) = progress
        {
            // Desktop's `Lightbox__toast-container`: "Downloading X of Y".
            ui.painter().circle_filled(center.center(), 32., Color32::from_black_alpha(160));
            crate::widgets::paint_spinner(ui, center.center(), 36., Color32::WHITE);
            let text = crate::i18n::tf(
                "icu:lightBoxDownloading",
                &format!("Downloading {} of {}", crate::format::file_size(received), crate::format::file_size(total)),
                &[("downloaded", crate::i18n::Arg::from(&crate::format::file_size(received))), ("total", crate::i18n::Arg::from(&crate::format::file_size(total)))],
            );
            let g = ui.painter().layout_no_wrap(text, theme::regular(13.), Color32::WHITE);
            let pill = Rect::from_center_size(Pos2::new(media.center().x, media.top() + 28.), g.size() + Vec2::new(24., 14.));
            ui.painter().rect_filled(pill, CornerRadius::same(8), Color32::from_rgba_unmultiplied(0x3b, 0x3b, 0x3b, 235));
            ui.painter().galley(pill.center() - g.size() / 2., g, Color32::WHITE);
        } else if item.path.is_none() {
            ui.painter().circle_filled(center.center(), 32., Color32::from_black_alpha(160));
            Icon::ArrowDown.paint(ui, center, 30., Color32::WHITE);
            if ui.interact(center, ui.scope_id().with("lightbox-fetch"), Sense::click()).on_hover_text("Download").clicked() {
                fetch = true;
            }
        } else if let Some(player) = player.as_mut() {
            // The video over its poster, then Desktop's controls on it.
            if let Some(texture) = player.texture(&ctx) {
                egui::Image::new((texture.id(), media.size())).paint_at(ui, media);
            }
            let r = ui.interact(media, ui.scope_id().with("lightbox-play"), Sense::click());
            if r.clicked() {
                player.toggle();
            }
            if !player.playing() {
                ui.painter().circle_filled(center.center(), 32., Color32::from_black_alpha(160));
                Icon::PlayFill.paint(ui, center, 30., Color32::WHITE);
            }
            if view_once {
                // Desktop's view-once video: no controls, the time left
                // under it.
                let left = (player.duration() - player.position()).max(0.);
                if player.duration() > 0. {
                    ui.painter().text(Pos2::new(media.center().x, media.bottom() + 20.), egui::Align2::CENTER_CENTER, crate::audio::playback_text(left), theme::regular(14.), Color32::WHITE);
                }
            } else {
                video_controls(ui, player, media, view_once);
            }
        }

        ui.multiply_opacity(chrome);
        if caption_h > 0. {
            let mut job = egui::text::LayoutJob::simple(item.caption.clone(), theme::regular(14.), Color32::WHITE, area.width());
            job.halign = egui::Align::Center;
            job.wrap.max_rows = 2;
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter().galley(Pos2::new(area.center().x, area.bottom() + 12.), g, Color32::WHITE);
        }

        // Arrows, gone while zoomed.
        if count > 1 && !zoomed {
            if index > 0 && button(ui, Rect::from_center_size(Pos2::new(rect.left() + 36., area.center().y), Vec2::splat(44.)), Icon::ChevronLeft, "Previous") {
                go = Some(index - 1);
            }
            if index + 1 < count && button(ui, Rect::from_center_size(Pos2::new(rect.right() - 36., area.center().y), Vec2::splat(44.)), Icon::ChevronRight, tr("Next")) {
                go = Some(index + 1);
            }
        }

        // Thumbnails of the neighbors, the current one outlined.
        if strip_h > 0. {
            let size = 52.;
            let total = neighbors.len() as f32 * (size + 6.);
            let mut tx = rect.center().x - total / 2.;
            let ty = rect.bottom() - strip_h;
            for (i, it) in &neighbors {
                let r = Rect::from_min_size(Pos2::new(tx, ty), Vec2::splat(size));
                let image = it.content_type.starts_with("image/");
                // A video without a thumbnail shows its first frame.
                let poster = (!image && it.thumbnail.is_none()).then(|| it.path.as_deref().and_then(|p| crate::video::poster(ui.ctx(), p))).flatten();
                match it.path.as_deref().filter(|_| image).or(it.thumbnail.as_deref()).or(poster.as_deref()) {
                    Some(path) => crate::chat::cover_image(ui, path, r, CornerRadius::same(6)),
                    None => {
                        ui.painter().rect_filled(r, CornerRadius::same(6), Color32::from_gray(50));
                    }
                }
                if *i == index {
                    ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(2., Color32::WHITE), egui::StrokeKind::Outside);
                } else {
                    ui.painter().rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(90));
                }
                if ui.interact(r, ui.scope_id().with(("lightbox-thumb", *i)), Sense::click()).clicked() {
                    go = Some(*i);
                }
                tx += size + 6.;
            }
        }

        ui.set_opacity(previous);
        if backdrop.clicked() && !zoomed && !media.contains(backdrop.interact_pointer_pos().unwrap_or_default()) {
            close = true;
        }
    });
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            close = true;
        }
        // Desktop's `$mod+S`.
        if i.modifiers.command && i.key_pressed(egui::Key::S) && !view_once {
            save = true;
        }
        if i.key_pressed(egui::Key::ArrowLeft) && index > 0 {
            go = Some(index - 1);
        }
        if i.key_pressed(egui::Key::ArrowRight) && index + 1 < count {
            go = Some(index + 1);
        }
    });
    // A view-once video closes when it ends, as Desktop's does.
    if view_once && player.as_ref().is_some_and(|p| p.ended()) {
        close = true;
    }
    if let Some(lightbox) = app.lightbox.as_mut()
        && go.is_none()
    {
        lightbox.player = player;
    }
    if fetch {
        app.send(Command::FetchAttachment { message_id: item.message_id, index: item.index });
    }
    if save {
        save_item(app, &item);
    }
    if let Some(lightbox) = app.lightbox.as_mut() {
        if toggle_zoom {
            lightbox.zoomed = !lightbox.zoomed;
        }
        if let Some(to) = go {
            lightbox.index = to;
            lightbox.zoomed = false;
        }
    }
    if forward && let Some(chat) = forward_target(app, &item) {
        let messages = vec![item.message_id];
        app.close_lightbox();
        if let Some(view) = app.chat.as_mut().filter(|_| app.selected.as_ref() == Some(&chat)) {
            view.dialog = Some(crate::actions::Dialog::Forward { messages, query: String::new(), chosen: Vec::new(), editing: None });
        }
        return;
    }
    if close {
        app.close_lightbox();
    }
}

/// The open chat the item's message is in, if langis will forward it.
fn forward_target(app: &App, item: &Item) -> Option<ConversationId> {
    let chat = app.selected.clone()?;
    let view = app.chat.as_ref()?;
    let m = view.messages.iter().find(|m| m.id == item.message_id)?;
    (m.conversation_id == chat && crate::actions::can_forward(m)).then_some(chat)
}

/// Play or pause, the time, a bar to seek on, and mute, along the video's
/// bottom (a view-once video can't be sought).
fn video_controls(ui: &Ui, player: &mut crate::video::Player, media: Rect, view_once: bool) {
    let bar = Rect::from_min_max(Pos2::new(media.left(), media.bottom() - 40.), media.max);
    ui.painter().rect_filled(bar, CornerRadius::ZERO, Color32::from_black_alpha(110));
    let play = Rect::from_center_size(Pos2::new(bar.left() + 22., bar.center().y), Vec2::splat(28.));
    if ui.interact(play, ui.scope_id().with("video-play"), Sense::click()).on_hover_text(if player.playing() { tr("Pause") } else { tr("Play") }).clicked() {
        player.toggle();
    }
    theme::paint_symbol(ui.painter(), if player.playing() { "pause-fill" } else { "play-fill" }, play.center(), 18., Color32::WHITE);
    let (at, total) = (player.position(), player.duration());
    let time = format!("{} / {}", crate::audio::playback_text(at), crate::audio::playback_text(total));
    let tg = ui.painter().layout_no_wrap(time, theme::regular(12.), Color32::WHITE);
    let mute = Rect::from_center_size(Pos2::new(bar.right() - 22., bar.center().y), Vec2::splat(28.));
    if ui.interact(mute, ui.scope_id().with("video-mute"), Sense::click()).on_hover_text(if player.muted { tr("Unmute") } else { tr("Mute") }).clicked() {
        let muted = !player.muted;
        player.set_muted(muted);
    }
    theme::paint_symbol(ui.painter(), if player.muted { "speaker-slash" } else { "speaker" }, mute.center(), 18., Color32::WHITE);
    let time_x = mute.left() - 8. - tg.size().x;
    ui.painter().galley(Pos2::new(time_x, bar.center().y - tg.size().y / 2.), tg, Color32::WHITE);
    let track = Rect::from_min_max(Pos2::new(play.right() + 10., bar.center().y - 2.), Pos2::new(time_x - 12., bar.center().y + 2.));
    ui.painter().rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(70));
    let done = if total > 0. { (at / total).clamp(0., 1.) } else { 0. };
    let filled = Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * done, track.bottom()));
    ui.painter().rect_filled(filled, CornerRadius::same(2), Color32::WHITE);
    if !view_once {
        let hit = track.expand2(Vec2::new(0., 8.));
        let r = ui.interact(hit, ui.scope_id().with("video-seek"), Sense::click_and_drag());
        if (r.clicked() || r.drag_stopped())
            && let Some(p) = r.interact_pointer_pos()
            && total > 0.
        {
            player.seek(((p.x - track.left()) / track.width()).clamp(0., 1.) * total);
        }
    }
}

fn save_item(app: &mut App, item: &Item) {
    let Some(path) = item.path.clone() else {
        app.send(Command::FetchAttachment { message_id: item.message_id, index: item.index });
        app.toast("Downloading. Save it again once it's here.");
        return;
    };
    let name = format!("signal-{}.{}", chrono::Local::now().format("%Y-%m-%d-%H%M%S"), crate::chat::extension_for(&item.content_type));
    let Some(target) = rfd::FileDialog::new().set_title(tr("Save")).set_file_name(&name).save_file() else { return };
    match std::fs::copy(&path, &target) {
        Ok(_) => app.toast(format!("Saved to {}", target.display())),
        Err(error) => app.toast(format!("Couldn't save: {error}")),
    }
}

/// A round button on the dark backdrop. True when clicked.
fn button(ui: &Ui, rect: Rect, icon: Icon, tip: &str) -> bool {
    let r = ui.interact(rect, ui.scope_id().with(("lightbox-button", tip)), Sense::click()).on_hover_text(tip);
    let fill = if r.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(16) };
    ui.painter().circle_filled(rect.center(), rect.width() / 2., fill);
    icon.paint(ui, rect, rect.width() * 0.5, Color32::WHITE);
    r.clicked()
}

#[cfg(test)]
mod tests {
    use super::Spring;

    #[test]
    fn zoom_eases_in_without_overshoot() {
        let mut z = Spring { index: 0, at: [1., 0., 0.], velocity: [0.; 3] };
        let target = [3., -400., 120.];
        let mut frames = 0;
        let mut half = None;
        while z.step(target, 1. / 60.) {
            frames += 1;
            assert!(z.at[0] <= 3. && z.at[1] >= -400. && z.at[2] <= 120.);
            if half.is_none() && z.at[0] >= 2. {
                half = Some(frames);
            }
            assert!(frames < 120);
        }
        eprintln!("halfway at frame {half:?}, settled after {frames} frames");
        assert!(half.unwrap() >= 3 && frames >= 6, "animates over several frames");
        assert_eq!(z.at, target);
    }
}
