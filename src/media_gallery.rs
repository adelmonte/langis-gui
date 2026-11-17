//! Desktop's All media (`MediaGallery`): a chat's photos and videos in a
//! grid, and its audio, links and files in lists, each by date.

use chrono::{Datelike as _, Local, TimeZone as _};
use egui::{Align2, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use langis_core::Command;
use langis_store::{AttachmentMeta, ConversationId, Message};

use crate::app::{App, Loaded};
use crate::theme::{self, Palette};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab {
    #[default]
    Media,
    Audio,
    Links,
    Files,
}

impl Tab {
    const ALL: [(Tab, &'static str); 4] = [(Tab::Media, "Media"), (Tab::Audio, "Audio"), (Tab::Links, "Links"), (Tab::Files, "Files")];

    /// `EmptyState`'s title and description.
    fn empty(self) -> (&'static str, &'static str) {
        match self {
            Tab::Media => ("No Media", "Photos, Videos, and GIFs that you send and receive will appear here"),
            Tab::Audio => ("No Audio", "Voice Messages and Audio Files that you send and receive will appear here"),
            Tab::Links => ("No Links", "Links that you send and receive will appear here"),
            Tab::Files => ("No Files", "Files that you send and receive will appear here"),
        }
    }
}

/// Reads a chat's media into its open details.
pub fn load(app: &App, conversation: ConversationId) {
    app.load(move |db| async move {
        let messages = db.media_messages(&conversation, 1000).await.ok()?;
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(d) = app.panes.details.as_mut().filter(|d| d.conversation == conversation) {
                d.media = Some(messages);
            }
        })))
    });
}

fn is_media(a: &AttachmentMeta) -> bool {
    !a.sticker && (a.content_type.starts_with("image/") || a.content_type.starts_with("video/"))
}

fn is_audio(a: &AttachmentMeta) -> bool {
    a.content_type.starts_with("audio/")
}

/// One entry of a tab: a message, and which of its attachments (none for
/// a link).
struct Entry<'a> {
    message: &'a Message,
    index: Option<usize>,
}

fn entries(messages: &[Message], tab: Tab) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    for m in messages {
        if tab == Tab::Links {
            if m.link_preview.is_some() {
                out.push(Entry { message: m, index: None });
            }
            continue;
        }
        for (i, a) in m.attachments.iter().enumerate() {
            let fits = match tab {
                Tab::Media => is_media(a),
                Tab::Audio => is_audio(a),
                Tab::Files => !a.sticker && !is_media(a) && !is_audio(a),
                Tab::Links => false,
            };
            if fits {
                out.push(Entry { message: m, index: Some(i) });
            }
        }
    }
    out
}

/// `groupMediaItemsByDate`: Today, Yesterday, This week, This month, then
/// each month by name. Entries come newest first.
fn section(millis: i64) -> String {
    let now = Local::now();
    let Some(time) = Local.timestamp_millis_opt(millis).single() else { return String::new() };
    let start = |days: i64| (now - chrono::Duration::days(days)).date_naive().and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).single());
    if start(0).is_some_and(|t| time >= t) {
        return "Today".into();
    }
    if start(1).is_some_and(|t| time >= t) {
        return "Yesterday".into();
    }
    if start(7).is_some_and(|t| time >= t) {
        return "This week".into();
    }
    if time.year() == now.year() && time.month() == now.month() {
        return "This month".into();
    }
    time.format("%B %Y").to_string()
}

/// `ExperimentalAxoSegmentedControl` without a track, for the header.
pub fn tabs(ui: &mut Ui, rect: Rect, tab: &mut Tab) {
    let palette = Palette::of(ui.ctx());
    let width = rect.width().min(320.);
    let item = width / Tab::ALL.len() as f32;
    let left = rect.center().x - width / 2.;
    for (i, (t, label)) in Tab::ALL.iter().enumerate() {
        let r = Rect::from_center_size(Pos2::new(left + item * (i as f32 + 0.5), rect.center().y), Vec2::new(item - 4., 28.));
        let response = ui.interact(r, egui::Id::unique("media-tab").with(i), Sense::click());
        if *t == *tab {
            ui.painter().rect_filled(r, CornerRadius::same(14), palette.fill_secondary);
        } else if response.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(14), palette.fill);
        }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, theme::medium(13.), palette.text);
        if response.clicked() {
            *tab = *t;
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui, palette: &Palette, conversation: &ConversationId) {
    let Some(d) = app.panes.details.as_ref() else { return };
    let tab = d.media_tab;
    let Some(messages) = d.media.clone() else {
        ui.add_space(40.);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    let list = entries(&messages, tab);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    if list.is_empty() {
        let (title, description) = tab.empty();
        ui.add_space(80.);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(title).font(theme::semibold(16.)).color(palette.text));
            ui.add_space(6.);
            ui.label(egui::RichText::new(description).font(theme::regular(13.)).color(palette.text_secondary));
        });
        return;
    }
    // Desktop's column, 360 to 660 wide.
    let width = ui.available_width().min(660.);
    let pad = ((ui.available_width() - width) / 2.).max(0.);
    let mut sections: Vec<(String, Vec<&Entry>)> = Vec::new();
    for e in &list {
        let name = section(e.message.sent_at);
        match sections.last_mut() {
            Some((last, items)) if *last == name => items.push(e),
            _ => sections.push((name, vec![e])),
        }
    }
    let count = sections.len();
    for (s, (name, items)) in sections.iter().enumerate() {
        if tab == Tab::Media {
            media_section(app, ui, palette, pad, width, name, items);
        } else {
            list_section(app, ui, palette, pad, width, name, items, tab, conversation);
            if s + 1 < count {
                // `mx-4 my-3`, a half pixel line.
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.5), Sense::hover());
                ui.painter().hline(r.left() + pad + 16.0..=r.left() + pad + width - 16., r.center().y, egui::Stroke::new(0.5, palette.border));
            }
        }
    }
    ui.add_space(16.);
}

fn heading(ui: &mut Ui, palette: &Palette, x: f32, top: f32, bottom: f32, name: &str) {
    let g = ui.painter().layout_no_wrap(name.to_owned(), theme::semibold(13.), palette.text);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), top + g.size().y + bottom), Sense::hover());
    ui.painter().galley(Pos2::new(r.left() + x, r.top() + top), g, palette.text);
}

/// A grid of 3 to 5 square tiles, 100 to 120 each, 4 apart (`px-5`).
fn media_section(app: &mut App, ui: &mut Ui, palette: &Palette, pad: f32, width: f32, name: &str, items: &[&Entry]) {
    heading(ui, palette, pad + 20. + 4., 16., 8., name);
    let inner = width - 40.;
    let columns = if inner >= 560. { 5 } else if inner >= 455. { 4 } else { 3 };
    let tile = ((inner - 4. * (columns as f32 - 1.)) / columns as f32).clamp(60., 120.);
    let rows = items.len().div_ceil(columns);
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), rows as f32 * (tile + 4.)), Sense::hover());
    for (i, e) in items.iter().enumerate() {
        let (row, col) = (i / columns, i % columns);
        let rect = Rect::from_min_size(Pos2::new(area.left() + pad + 20. + col as f32 * (tile + 4.), area.top() + row as f32 * (tile + 4.)), Vec2::splat(tile));
        let Some(index) = e.index else { continue };
        let a = &e.message.attachments[index];
        let response = ui.interact(rect, egui::Id::unique("media-tile").with((e.message.id, index)), Sense::click());
        let radius = CornerRadius::same(7);
        ui.painter().rect_filled(rect, radius, palette.fill);
        let video = a.content_type.starts_with("video/");
        let image = if video { a.thumbnail_path.as_ref() } else { a.local_path.as_ref().or(a.thumbnail_path.as_ref()) };
        if let Some(path) = image {
            // `object-cover`: the middle square of the picture.
            let uv = match (a.width, a.height) {
                (Some(w), Some(h)) if w > 0 && h > 0 => {
                    let (w, h) = (w as f32, h as f32);
                    if w > h {
                        let side = h / w;
                        Rect::from_min_max(Pos2::new((1. - side) / 2., 0.), Pos2::new((1. + side) / 2., 1.))
                    } else {
                        let side = w / h;
                        Rect::from_min_max(Pos2::new(0., (1. - side) / 2.), Pos2::new(1., (1. + side) / 2.))
                    }
                }
                _ => Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            };
            egui::Image::new(format!("file://{path}")).uv(uv).corner_radius(radius).paint_at(ui, rect);
        }
        if video {
            ui.painter().circle_filled(rect.center(), 22., egui::Color32::from_black_alpha(140));
            theme::paint_symbol(ui.painter(), "play-fill", rect.center(), 24., egui::Color32::WHITE);
        } else if a.local_path.is_none() {
            ui.painter().circle_filled(rect.center(), 22., egui::Color32::from_black_alpha(140));
            theme::paint_symbol(ui.painter(), "arrow-down", rect.center(), 24., egui::Color32::WHITE);
        }
        if response.hovered() {
            ui.painter().rect_filled(rect, radius, egui::Color32::from_black_alpha(25));
        }
        if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if a.local_path.is_some() {
                app.open_lightbox(e.message, index);
            } else {
                app.send(Command::FetchAttachment { message_id: e.message.id, index });
            }
        }
    }
    ui.add_space(4.);
}

/// `ListItem`s: a 36px thumbnail, a title and subtitle, the date at the end.
#[allow(clippy::too_many_arguments)]
fn list_section(app: &mut App, ui: &mut Ui, palette: &Palette, pad: f32, width: f32, name: &str, items: &[&Entry], tab: Tab, conversation: &ConversationId) {
    heading(ui, palette, pad + 24., 6., 8., name);
    for e in items {
        let m = e.message;
        let author = match &m.author { None => "You".to_owned(), Some(sid) => m.author_name.clone().unwrap_or_else(|| app.name(sid)) };
        let a = e.index.map(|i| &m.attachments[i]);
        let (title, subtitle) = match (tab, a, &m.link_preview) {
            (Tab::Links, _, Some(p)) => {
                let domain = p.url.split("://").nth(1).unwrap_or(&p.url).split('/').next().unwrap_or("").to_owned();
                (p.title.clone().unwrap_or_default(), format!("{}\n{author} · {domain}", p.url))
            }
            (Tab::Audio, Some(a), _) => {
                let kind = if a.voice { "Voice Message" } else { "Audio" };
                (a.file_name.clone().unwrap_or_else(|| author.clone()), format!("{} · {kind}", crate::format::file_size(a.size)))
            }
            (_, Some(a), _) => {
                let mut title = Vec::new();
                if let Some(f) = &a.file_name {
                    title.push(f.clone());
                }
                title.push(author.clone());
                let size = crate::format::file_size(a.size);
                (title.join(" · "), if a.local_path.is_none() { format!("\u{2193} {size}") } else { size })
            }
            _ => continue,
        };
        let text_w = width - 20. - 28. - 36. - 12. - 12. - 48.;
        let tg = ui.painter().layout(title, theme::regular(14.), palette.text, text_w);
        let sg = ui.painter().layout(subtitle, theme::regular(12.), palette.text_secondary, text_w);
        let h = (16. + tg.size().y.min(20.) + sg.size().y).max(52.);
        let (full, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
        let row = Rect::from_min_max(Pos2::new(full.left() + pad + 10., full.top()), Pos2::new(full.left() + pad + width - 10., full.bottom()));
        let response = ui.interact(row, egui::Id::unique("media-row").with((m.id, e.index)), Sense::click());
        if response.hovered() {
            let fill = if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
            ui.painter().rect_filled(row, CornerRadius::same(9), fill);
        }
        let thumb = Rect::from_min_size(Pos2::new(row.left() + 14., if tab == Tab::Links { row.top() + 8. } else { row.center().y - 18. }), Vec2::splat(36.));
        let radius = CornerRadius::same(5);
        match (tab, a) {
            (Tab::Links, _) => {
                let image = m.link_preview.as_ref().and_then(|p| p.image.as_ref()).and_then(|i| i.local_path.clone());
                match image {
                    Some(path) => {
                        egui::Image::new(format!("file://{path}")).corner_radius(radius).paint_at(ui, thumb);
                    }
                    None => {
                        ui.painter().rect_filled(thumb, radius, palette.fill_secondary);
                        theme::paint_symbol(ui.painter(), "link", thumb.center(), 20., palette.text_secondary);
                    }
                }
            }
            (Tab::Audio, _) => {
                ui.painter().rect_filled(thumb, radius, palette.fill_secondary);
                theme::paint_symbol(ui.painter(), if a.is_some_and(|a| a.voice) { "mic" } else { "audio" }, thumb.center(), 20., palette.text_secondary);
            }
            _ => {
                // `FileThumbnail`: the extension on a page.
                ui.painter().rect_filled(thumb, radius, palette.fill_secondary);
                let ext = a.and_then(|a| a.file_name.as_deref()).and_then(|f| f.rsplit_once('.')).map(|(_, e)| e.to_uppercase()).filter(|e| e.len() <= 4);
                match ext {
                    Some(ext) => {
                        ui.painter().text(thumb.center(), Align2::CENTER_CENTER, ext, theme::semibold(10.), palette.text_secondary);
                    }
                    None => theme::paint_symbol(ui.painter(), "file", thumb.center(), 20., palette.text_secondary),
                }
            }
        }
        let x = thumb.right() + 12.;
        let tgh = tg.size().y;
        let top = if tab == Tab::Links { row.top() + 8. } else { row.center().y - (tgh + sg.size().y) / 2. };
        ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(x, row.top()), Pos2::new(x + text_w, row.bottom()))).galley(Pos2::new(x, top), tg, palette.text);
        ui.painter().galley(Pos2::new(x, top + tgh), sg, palette.text_secondary);
        let date = Local.timestamp_millis_opt(m.sent_at).single().map(|t| t.format("%b %-d").to_string()).unwrap_or_default();
        let date_pos = if tab == Tab::Links { Pos2::new(row.right() - 14., row.top() + 8.) } else { Pos2::new(row.right() - 14., row.center().y) };
        let date_rect = Rect::from_min_max(Pos2::new(row.right() - 62., row.top()), row.max);
        let date_click = ui.interact(date_rect, egui::Id::unique("media-date").with((m.id, e.index)), Sense::click());
        ui.painter().text(date_pos, if tab == Tab::Links { Align2::RIGHT_TOP } else { Align2::RIGHT_CENTER }, date, theme::regular(12.), palette.text_secondary);
        if date_click.on_hover_text("Show message").clicked() {
            // `showMessage`: back to the chat at it.
            app.panes.details = None;
            app.open_at(conversation.clone(), m.id);
            continue;
        }
        if response.clicked() {
            match (tab, a) {
                (Tab::Links, _) => {
                    if let Some(p) = &m.link_preview {
                        crate::chat::open_externally(&p.url);
                    }
                }
                (_, Some(a)) => match &a.local_path {
                    Some(path) => crate::chat::open_externally(path),
                    None => app.send(Command::FetchAttachment { message_id: m.id, index: e.index.unwrap_or(0) }),
                },
                _ => {}
            }
        }
    }
}
