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
    /// Desktop's click to zoom: the photo at full size, panned by the pointer.
    pub zoomed: bool,
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
        self.lightbox = Some(Lightbox { conversation: Some(m.conversation_id.clone()), items, index: at, view_once: None, zoomed: false });
        let (conversation, message_id) = (m.conversation_id.clone(), m.id);
        self.load(move |db| async move {
            let mut messages = db.media_messages(&conversation, 500).await.ok()?;
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
        self.lightbox = Some(Lightbox { conversation: None, items: vec![item], index: 0, view_once: Some(opened.path), zoomed: false });
    }

    pub fn close_lightbox(&mut self) {
        let Some(lightbox) = self.lightbox.take() else { return };
        if let Some(path) = lightbox.view_once {
            self.ctx.forget_image(&format!("file://{}", path.display()));
            if let Err(error) = std::fs::remove_file(&path) {
                tracing::warn!(%error, "couldn't delete the view-once copy");
            }
        }
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
    let mut fetch = false;
    let mut play = None;
    let neighbors: Vec<(usize, Item)> = lightbox
        .items
        .iter()
        .enumerate()
        .skip(index.saturating_sub(10))
        .take(21)
        .map(|(i, it)| (i, it.clone()))
        .collect();
    egui::Area::new(egui::Id::unique("lightbox")).order(egui::Order::Foreground).fixed_pos(screen.min).show(&ctx, |ui| {
        let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(rect, 0, Color32::from_black_alpha(242));

        // Top bar: who and when, then Save and Close.
        let top = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 56.));
        if !view_once {
            let who = match &item.author {
                None => "You".to_owned(),
                Some(a) => app.names.get(a).cloned().or(item.author_name.clone()).unwrap_or_else(|| "Unknown".into()),
            };
            ui.painter().text(top.left_center() + Vec2::new(20., -8.), Align2::LEFT_CENTER, who, theme::semibold(14.), Color32::WHITE);
            let when = chrono::DateTime::from_timestamp_millis(item.sent_at)
                .map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y, %-I:%M %p").to_string())
                .unwrap_or_default();
            ui.painter().text(top.left_center() + Vec2::new(20., 10.), Align2::LEFT_CENTER, when, theme::regular(12.), Color32::from_gray(190));
        }
        let mut x = top.right() - 52.;
        if button(ui, Rect::from_min_size(Pos2::new(x, top.top() + 10.), Vec2::splat(36.)), Icon::X, "Close") {
            close = true;
        }
        x -= 44.;
        if !view_once && button(ui, Rect::from_min_size(Pos2::new(x, top.top() + 10.), Vec2::splat(36.)), Icon::Save, "Save") {
            save = true;
        }

        // The media, as large as fits.
        let strip_h = if count > 1 && !view_once { 72. } else { 0. };
        let caption_h = if item.caption.is_empty() || view_once { 0. } else { 40. };
        let area = Rect::from_min_max(Pos2::new(rect.left() + 72., top.bottom() + 8.), Pos2::new(rect.right() - 72., rect.bottom() - strip_h - caption_h - 16.));
        let (w, h) = item.size.map(|(w, h)| (w.max(1) as f32, h.max(1) as f32)).unwrap_or((4., 3.));
        let scale = (area.width() / w).min(area.height() / h).min(if item.size.is_some() { 2. } else { f32::MAX });
        let media = Rect::from_center_size(area.center(), Vec2::new(w * scale, h * scale));
        let image = item.content_type.starts_with("image/");
        // Full size is the photo's pixels, at least twice the fitted size.
        let full = (1. / ui.ctx().pixels_per_point()).max(scale * 2.);
        let zoomable = image && item.path.is_some() && item.size.is_some() && !view_once;
        match item.path.as_deref().filter(|_| image).or(item.thumbnail.as_deref()) {
            Some(path) if zoomable && zoomed => {
                let size = Vec2::new(w * full, h * full);
                let pointer = ui.ctx().pointer_hover_pos().unwrap_or(area.center());
                let along = ((pointer - area.min) / area.size()).clamp(Vec2::ZERO, Vec2::splat(1.));
                let slack = (size - area.size()).max(Vec2::ZERO);
                let min = Pos2::new(
                    if slack.x > 0. { area.left() - slack.x * along.x } else { area.center().x - size.x / 2. },
                    if slack.y > 0. { area.top() - slack.y * along.y } else { area.center().y - size.y / 2. },
                );
                ui.scope(|ui| {
                    ui.set_clip_rect(area);
                    egui::Image::new(format!("file://{path}")).fit_to_exact_size(size).paint_at(ui, Rect::from_min_size(min, size));
                });
            }
            Some(path) => {
                egui::Image::new(format!("file://{path}")).fit_to_exact_size(media.size()).paint_at(ui, media);
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
        }
        let center = Rect::from_center_size(media.center(), Vec2::splat(64.));
        if item.path.is_none() {
            ui.painter().circle_filled(center.center(), 32., Color32::from_black_alpha(160));
            Icon::ArrowDown.paint(ui, center, 30., Color32::WHITE);
            if ui.interact(center, ui.scope_id().with("lightbox-fetch"), Sense::click()).on_hover_text("Download").clicked() {
                fetch = true;
            }
        } else if !image {
            ui.painter().circle_filled(center.center(), 32., Color32::from_black_alpha(160));
            Icon::PlayFill.paint(ui, center, 30., Color32::WHITE);
            if ui.interact(media, ui.scope_id().with("lightbox-play"), Sense::click()).on_hover_text("Play").clicked() {
                play = item.path.clone();
            }
        }

        if caption_h > 0. {
            let mut job = egui::text::LayoutJob::simple(item.caption.clone(), theme::regular(14.), Color32::WHITE, area.width());
            job.halign = egui::Align::Center;
            job.wrap.max_rows = 2;
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter().galley(Pos2::new(area.center().x, area.bottom() + 12.), g, Color32::WHITE);
        }

        // Arrows.
        if count > 1 {
            if index > 0 && button(ui, Rect::from_center_size(Pos2::new(rect.left() + 36., area.center().y), Vec2::splat(44.)), Icon::ChevronLeft, "Previous") {
                go = Some(index - 1);
            }
            if index + 1 < count && button(ui, Rect::from_center_size(Pos2::new(rect.right() - 36., area.center().y), Vec2::splat(44.)), Icon::ChevronRight, "Next") {
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
                match it.thumbnail.as_deref().or(it.path.as_deref().filter(|_| it.content_type.starts_with("image/"))) {
                    Some(path) => {
                        egui::Image::new(format!("file://{path}")).corner_radius(CornerRadius::same(6)).fit_to_exact_size(r.size()).paint_at(ui, r);
                    }
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

        if backdrop.clicked() && !zoomed && !media.contains(backdrop.interact_pointer_pos().unwrap_or_default()) {
            close = true;
        }
    });
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            close = true;
        }
        if i.key_pressed(egui::Key::ArrowLeft) && index > 0 {
            go = Some(index - 1);
        }
        if i.key_pressed(egui::Key::ArrowRight) && index + 1 < count {
            go = Some(index + 1);
        }
    });
    if let Some(path) = play {
        crate::chat::open_externally(&path);
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
    if close {
        app.close_lightbox();
    }
}

fn save_item(app: &mut App, item: &Item) {
    let Some(path) = item.path.clone() else {
        app.send(Command::FetchAttachment { message_id: item.message_id, index: item.index });
        app.toast("Downloading. Save it again once it's here.");
        return;
    };
    let name = format!("signal-{}.{}", chrono::Local::now().format("%Y-%m-%d-%H%M%S"), crate::chat::extension_for(&item.content_type));
    let Some(target) = rfd::FileDialog::new().set_title("Save").set_file_name(&name).save_file() else { return };
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
