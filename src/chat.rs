//! The open chat: Desktop's conversation header, timeline and composition
//! area.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use langis_core::Command;
use langis_store::{ConversationId, ConversationKind, DeliveryStatus, Direction, Message};

use crate::app::{App, PendingSend};
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

/// Messages per page of the timeline.
pub const PAGE: u32 = 60;

/// Messages from one person this close together are drawn as one group
/// (Desktop's `areMessagesInSameGroup`: three minutes).
const GROUP_GAP_MS: i64 = 3 * 60 * 1000;

/// How long after the last keystroke "typing" stops (Desktop's
/// `TYPING_IDLE_TIMEOUT`, a little under its 15s pause timer).
const TYPING_IDLE: Duration = Duration::from_secs(3);

pub struct ChatView {
    pub id: ConversationId,
    /// Oldest first.
    pub messages: Vec<Message>,
    pub loaded: bool,
    pub loading_older: bool,
    pub at_start: bool,
    pub composer: String,
    pub quote: Option<i64>,
    pub typing_sent: bool,
    last_typed: Option<Instant>,
    pub emoji_open: bool,
    pub emoji: crate::emoji_picker::Picker,
    revealed: HashSet<i64>,
    /// Content height before older messages were added on top, to keep
    /// the view where it was.
    keep_from_height: Option<f32>,
    last_height: f32,
    /// Frames left to hold the view at the bottom (the content's height
    /// settles over a frame or two as images load).
    pub scroll_to_bottom: u8,
    draft_at_open: String,
    /// Scrolled near the top: load the page before.
    want_older: bool,
    /// Where to scroll to on the next frame.
    pending_offset: Option<f32>,
}

/// What the timeline asked for, applied after it's drawn.
enum Action {
    Retry(i64),
    Fetch { message_id: i64, index: usize },
    React { message_id: i64, emoji: String, remove: bool },
    Reply(i64),
    Open(String),
}

impl ChatView {
    pub fn new(id: ConversationId, draft: String) -> Self {
        Self {
            id,
            messages: Vec::new(),
            loaded: false,
            loading_older: false,
            at_start: false,
            composer: draft.clone(),
            quote: None,
            typing_sent: false,
            last_typed: None,
            emoji_open: false,
            emoji: Default::default(),
            revealed: HashSet::new(),
            keep_from_height: None,
            last_height: 0.,
            scroll_to_bottom: 10,
            draft_at_open: draft,
            want_older: false,
            pending_offset: None,
        }
    }

    /// A page from the store (oldest first, though the frontend guide says
    /// newest; see CORE-WISHLIST.md).
    pub fn take_page(&mut self, mut page: Vec<Message>, older: bool) {
        let full = page.len() as u32 >= PAGE;
        if older {
            self.loading_older = false;
            self.at_start = !full;
            page.retain(|m| !self.messages.iter().any(|have| have.id == m.id));
            if !page.is_empty() {
                self.keep_from_height = Some(self.last_height);
            }
            page.append(&mut self.messages);
            self.messages = page;
        } else {
            self.at_start = !full;
            self.messages = page;
            self.loaded = true;
            self.scroll_to_bottom = 10;
        }
    }

    /// A message that's new or changed.
    pub fn upsert(&mut self, message: Message) {
        match self.messages.iter_mut().find(|m| m.id == message.id) {
            Some(have) => *have = message,
            None => {
                let at = self.messages.partition_point(|m| (m.sent_at, m.id) <= (message.sent_at, message.id));
                if message.direction == Direction::Outgoing {
                    self.scroll_to_bottom = 10;
                }
                self.messages.insert(at, message);
            }
        }
    }

    /// Keeps the composer's text as the chat's draft (a frontend's own
    /// write, as the frontend guide allows).
    pub fn save_draft(&self, app: &App) {
        if self.composer == self.draft_at_open {
            return;
        }
        let (id, text) = (self.id.clone(), self.composer.clone());
        app.load(move |db| async move {
            let draft = Some(text.as_str()).filter(|t| !t.trim().is_empty());
            let _ = db.set_draft(&id, draft).await;
            db.conversations().await.ok().map(crate::app::Loaded::Conversations)
        });
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let Some(mut chat) = app.chat.take() else { return };
    let Some(conversation) = app.conversation(&chat.id).cloned() else {
        app.chat = Some(chat);
        return;
    };
    header(app, ui, &palette, &conversation);
    composer(app, ui, &palette, &mut chat, &conversation);
    let mut actions = Vec::new();
    egui::CentralPanel::no_frame().show(ui, |ui| {
        timeline(app, ui, &palette, &mut chat, &conversation, &mut actions);
    });
    let id = chat.id.clone();
    let older = std::mem::take(&mut chat.want_older);
    app.chat = Some(chat);
    if older {
        app.load_older();
    }
    for action in actions {
        match action {
            Action::Retry(message_id) => app.send_checked(id.clone(), PendingSend::Retry(message_id)),
            Action::Fetch { message_id, index } => app.send(Command::FetchAttachment { message_id, index }),
            Action::React { message_id, emoji, remove } => app.send(Command::React { message_id, emoji, remove }),
            Action::Reply(message_id) => {
                if let Some(chat) = app.chat.as_mut() {
                    chat.quote = Some(message_id);
                }
            }
            Action::Open(path) => open_externally(&path),
        }
    }
}

/// Opens a file or web link with the desktop's default app.
pub fn open_externally(target: &str) {
    if let Err(error) = std::process::Command::new("xdg-open").arg(target).spawn() {
        tracing::warn!(%error, "couldn't run xdg-open");
    }
}

/// Desktop's `ConversationHeader`: avatar and name, then call buttons.
fn header(app: &mut App, ui: &mut Ui, palette: &Palette, c: &langis_store::Conversation) {
    egui::Panel::top("conversation-header")
        .exact_size(52.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1., palette.border));
            ui.horizontal_centered(|ui| {
                ui.add_space(16.);
                widgets::avatar(ui, &widgets::AvatarSpec::of(c), 32.);
                ui.add_space(4.);
                ui.vertical(|ui| {
                    ui.add_space(if c.kind == ConversationKind::Group || c.expire_timer > 0 { 8. } else { 16. });
                    ui.label(RichText::new(&c.title).font(theme::semibold(14.)));
                    let mut sub = Vec::new();
                    if c.kind == ConversationKind::Group {
                        sub.push(format!("{} members", c.members.len() + 1));
                    }
                    if c.expire_timer > 0 {
                        sub.push(crate::format::timer_long(c.expire_timer));
                    }
                    if !sub.is_empty() {
                        ui.add_space(-4.);
                        ui.label(RichText::new(sub.join(" · ")).font(theme::regular(12.)).color(palette.text_secondary));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.);
                    widgets::icon_button(ui, Icon::More, 20., "More");
                    if c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked {
                        if widgets::icon_button(ui, Icon::Phone, 20., "Start a voice call").clicked() {
                            app.send(Command::StartCall { conversation: c.id.clone(), video: false });
                        }
                        if widgets::icon_button(ui, Icon::Video, 20., "Start a video call").clicked() {
                            app.send(Command::StartCall { conversation: c.id.clone(), video: true });
                        }
                    }
                });
            });
        });
}

fn timeline(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation, actions: &mut Vec<Action>) {
    let group = c.kind == ConversationKind::Group;
    let typing: Vec<String> = app.typing.get(&chat.id).map(|t| t.iter().cloned().collect()).unwrap_or_default();
    let mut scroll = egui::ScrollArea::vertical().id_salt(("timeline", &chat.id.0)).auto_shrink(false).stick_to_bottom(true);
    let to_bottom = chat.scroll_to_bottom > 0;
    if to_bottom {
        if chat.loaded {
            chat.scroll_to_bottom -= 1;
        }
        ui.ctx().request_repaint();
    } else if let Some(offset) = chat.pending_offset.take() {
        scroll = scroll.vertical_scroll_offset(offset);
    }
    let output = scroll.show(ui, |ui| {
        let width = ui.available_width();
        ui.set_width(width);
        ui.add_space(12.);
        if chat.at_start && chat.loaded {
            chat_intro(ui, palette, c);
        } else if !chat.loaded || chat.loading_older {
            ui.vertical_centered(|ui| ui.spinner());
        }
        let ctx = Ctx { app, palette, group, width, chat_id: &chat.id };
        let messages = &chat.messages;
        for (i, m) in messages.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| &messages[p]);
            let next = messages.get(i + 1);
            if prev.is_none_or(|p| !crate::format::same_day(p.sent_at, m.sent_at)) {
                date_header(ui, palette, m.sent_at);
            }
            let revealed = chat.revealed.contains(&m.id);
            if let Some(reveal) = message(ui, &ctx, m, prev, next, revealed, actions) {
                if reveal {
                    chat.revealed.insert(m.id);
                }
            }
        }
        for author in &typing {
            typing_bubble(ui, &ctx, author);
        }
        ui.add_space(16.);
        let end = ui.allocate_response(Vec2::new(1., 1.), Sense::hover());
        if to_bottom {
            end.scroll_to_me(Some(egui::Align::BOTTOM));
        }
    });
    chat.last_height = output.content_size.y;
    if let Some(before) = chat.keep_from_height.take() {
        chat.pending_offset = Some(output.state.clamped_offset().y + (output.content_size.y - before).max(0.));
        ui.ctx().request_repaint();
    }
    if output.state.clamped_offset().y < 80. && chat.loaded && !chat.at_start && !chat.loading_older && !chat.messages.is_empty() {
        chat.want_older = true;
    }
}

/// What the timeline's drawing needs from the app.
struct Ctx<'a> {
    app: &'a App,
    palette: &'a Palette,
    group: bool,
    width: f32,
    #[allow(dead_code)]
    chat_id: &'a ConversationId,
}

/// The top of a chat's history: Desktop's `ConversationHero`.
fn chat_intro(ui: &mut Ui, palette: &Palette, c: &langis_store::Conversation) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.);
        widgets::avatar(ui, &widgets::AvatarSpec::of(c), 80.);
        ui.add_space(8.);
        ui.label(RichText::new(&c.title).font(theme::semibold(18.)));
        if let Some(phone) = &c.phone {
            ui.label(RichText::new(phone).font(theme::regular(13.)).color(palette.text_secondary));
        }
        if c.kind == ConversationKind::Group {
            ui.label(RichText::new(format!("{} members", c.members.len() + 1)).font(theme::regular(13.)).color(palette.text_secondary));
        }
        ui.add_space(24.);
    });
}

fn date_header(ui: &mut Ui, palette: &Palette, at: i64) {
    ui.add_space(12.);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(crate::format::day_label(at)).font(theme::medium(12.)).color(palette.text_secondary));
    });
    ui.add_space(8.);
}

/// One timeline item. Returns `Some(true)` when a spoiler was clicked.
fn message(
    ui: &mut Ui,
    ctx: &Ctx<'_>,
    m: &Message,
    prev: Option<&Message>,
    next: Option<&Message>,
    revealed: bool,
    actions: &mut Vec<Action>,
) -> Option<bool> {
    let palette = ctx.palette;
    if let Some(notice) = &m.notice {
        let text = crate::notices::notice_text(notice, |sid| ctx.app.name(sid));
        ui.add_space(10.);
        ui.vertical_centered(|ui| {
            ui.set_max_width((ctx.width - 80.).max(200.));
            ui.label(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary));
        });
        ui.add_space(10.);
        return None;
    }
    let outgoing = m.direction == Direction::Outgoing;
    let same = |o: &Message| o.notice.is_none() && o.direction == m.direction && o.author == m.author && (o.sent_at - m.sent_at).abs() < GROUP_GAP_MS && crate::format::same_day(o.sent_at, m.sent_at);
    let collapsed_above = prev.is_some_and(same);
    let collapsed_below = next.is_some_and(same);
    ui.add_space(if collapsed_above { 2. } else { 10. });

    let show_avatar = ctx.group && !outgoing;
    let avatar_col = if show_avatar { 36. } else { 0. };
    let side = 16.;
    let max_bubble = ((ctx.width - side * 2. - avatar_col) * 0.75).clamp(220., 480.);
    let in_color = if outgoing { palette.bubble_out_text() } else { palette.text };
    let meta_color = if outgoing { palette.bubble_out_meta() } else { palette.text_secondary };

    let author_name = (!outgoing && ctx.group && !collapsed_above).then(|| {
        m.author.as_deref().map(|a| ctx.app.name(a)).or_else(|| m.author_name.clone()).unwrap_or_else(|| "Unknown".into())
    });

    // What goes in the bubble, top to bottom.
    let sticker = m.attachments.iter().find(|a| a.sticker);
    let images: Vec<(usize, &langis_store::AttachmentMeta)> = m
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.sticker && (a.content_type.starts_with("image/") || a.content_type.starts_with("video/")))
        .collect();
    let files: Vec<(usize, &langis_store::AttachmentMeta)> =
        m.attachments.iter().enumerate().filter(|(_, a)| !a.sticker && !a.content_type.starts_with("image/") && !a.content_type.starts_with("video/")).collect();
    let body_text = if m.deleted { String::new() } else { m.body.clone() };
    let jumbo = !m.deleted && m.attachments.is_empty() && m.quote.is_none() && langis_emoji::jumbo_count(&body_text).is_some_and(|n| n <= 5);

    let rich = if m.deleted {
        let mut job = egui::text::LayoutJob::default();
        job.append("This message was deleted.", 0., egui::TextFormat { font_id: theme::italic(14.), color: in_color, line_height: Some(20.), ..Default::default() });
        Some(Rich { job, links: Vec::new(), spoilers: Vec::new() })
    } else if body_text.is_empty() {
        None
    } else {
        let link_color = if outgoing { Color32::WHITE } else { palette.accent_text };
        let font_size = if jumbo { 32. } else { 14. };
        Some(rich_text(m, &|sid| ctx.app.name(sid), in_color, link_color, revealed, font_size))
    };

    let inner_max = max_bubble - 24.;
    let galley = rich.as_ref().map(|r| {
        let mut job = r.job.clone();
        job.wrap.max_width = inner_max;
        ui.fonts_mut(|f| f.layout_job(job))
    });

    // Metadata: time, "Edited", status.
    let time = crate::format::message_time(m.sent_at);
    let meta_text = if m.edited { format!("Edited  {time}") } else { time };
    let meta = ui.painter().layout_no_wrap(meta_text, theme::regular(11.), Color32::PLACEHOLDER);
    let status_w = if outgoing { 18. } else { 0. };
    let meta_w = meta.size().x + status_w;
    let show_meta = !collapsed_below || m.status == DeliveryStatus::Failed || next.is_some_and(|n| n.status != m.status);

    // Sticker: large, no bubble.
    if let Some(s) = sticker {
        let size = Vec2::splat(144.);
        let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, size.y + 18.), Sense::hover());
        let x = if outgoing { row.right() - side - size.x } else { row.left() + side + avatar_col };
        let rect = Rect::from_min_size(Pos2::new(x, row.top()), size);
        if let Some(path) = s.local_path.as_deref().or(s.thumbnail_path.as_deref()) {
            egui::Image::new(format!("file://{path}")).maintain_aspect_ratio(true).paint_at(ui, rect);
        }
        let at = Pos2::new(if outgoing { rect.right() - meta_w } else { rect.left() }, rect.bottom() + 2.);
        ui.painter().galley(at, meta, palette.text_secondary);
        if show_avatar && !collapsed_below {
            avatar_for(ui, ctx, m, Pos2::new(row.left() + side, rect.bottom() - 28.));
        }
        return None;
    }

    // Sizes of each part.
    let pad = Vec2::new(12., 8.);
    let media_w = images.first().map(|(_, a)| media_size(a, max_bubble).x).unwrap_or(0.);
    let text_w = galley.as_ref().map(|g| g.size().x).unwrap_or(0.);
    let last_row_w = galley.as_ref().and_then(|g| g.rows.last()).map(|r| r.rect().width()).unwrap_or(0.);
    let meta_inline = show_meta && galley.is_some() && last_row_w + meta_w + 8. <= inner_max;
    let name_galley = author_name.as_ref().map(|n| {
        let color = m.author.as_deref().map(|a| theme::name_color(a, palette.dark)).unwrap_or(palette.text);
        ui.painter().layout_no_wrap(n.clone(), theme::semibold(13.), color)
    });
    let mut content_w = text_w.max(if meta_inline { last_row_w + meta_w + 8. } else if show_meta { meta_w } else { 0. });
    if let Some(n) = &name_galley {
        content_w = content_w.max(n.size().x.min(inner_max));
    }
    if m.quote.is_some() || m.link_preview.is_some() || !files.is_empty() {
        content_w = content_w.max(inner_max.min(300.));
    }
    let bubble_w = (content_w + pad.x * 2.).max(media_w).min(max_bubble).max(if jumbo { 0. } else { 48. });

    // Heights.
    let name_h = name_galley.as_ref().map(|g| g.size().y + 2.).unwrap_or(0.);
    let quote_h = if m.quote.is_some() { 58. } else { 0. };
    let media_h: f32 = images.iter().map(|(_, a)| media_size(a, bubble_w).y + 2.).sum();
    let files_h = files.len() as f32 * 48.;
    let preview_h = if m.link_preview.is_some() { 84. } else { 0. };
    let text_h = galley.as_ref().map(|g| g.size().y).unwrap_or(0.);
    let meta_h = if show_meta && !meta_inline { 16. } else { 0. };
    let media_only = !images.is_empty() && galley.is_none() && m.quote.is_none() && name_galley.is_none() && files.is_empty();
    let top_pad = if media_only || (images.first().is_some() && name_galley.is_none() && m.quote.is_none()) { 0. } else { pad.y };
    let text_block = name_h + quote_h + files_h + preview_h + text_h + meta_h;
    let bubble_h = top_pad + media_h + if media_only && meta_h > 0. { 0. } else if text_block > 0. { text_block + pad.y + if media_h > 0. { pad.y } else { 0. } } else { 0. };
    let bubble_h = bubble_h.max(if media_only { media_h } else { 36. });

    let reactions = !m.reactions.is_empty();
    let reactions_h = if reactions { 18. } else { 0. };
    let failed_h = if m.status == DeliveryStatus::Failed { 18. } else { 0. };
    let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, bubble_h + reactions_h + failed_h), Sense::hover());
    let x = if outgoing { row.right() - side - bubble_w } else { row.left() + side + avatar_col };
    let bubble = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(bubble_w, bubble_h));
    let response = ui.interact(bubble, ui.scope_id().with(("bubble", m.id)), Sense::click());

    // The bubble, with Desktop's smaller corners inside a group.
    let (r, small) = (18u8, 4u8);
    let radius = if outgoing {
        CornerRadius { nw: r, sw: r, ne: if collapsed_above { small } else { r }, se: if collapsed_below { small } else { r } }
    } else {
        CornerRadius { ne: r, se: r, nw: if collapsed_above { small } else { r }, sw: if collapsed_below { small } else { r } }
    };
    let fill = if jumbo { Color32::TRANSPARENT } else if outgoing { palette.bubble_out } else { palette.bubble_in };
    ui.painter().rect_filled(bubble, radius, fill);
    let mut y = bubble.top() + top_pad;
    let left = bubble.left() + pad.x;

    if let Some(n) = name_galley {
        ui.painter().galley(Pos2::new(left, y), n, palette.text);
        y += name_h;
    }
    if let Some(q) = &m.quote {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., 52.));
        quote(ui, palette, outgoing, q, rect, ctx);
        y += quote_h;
    }
    for (index, a) in &images {
        let size = media_size(a, bubble_w);
        let rect = Rect::from_min_size(Pos2::new(bubble.left(), y), Vec2::new(bubble_w, size.y));
        let mr = if media_only { radius } else if y <= bubble.top() + 1. { CornerRadius { nw: radius.nw, ne: radius.ne, sw: 0, se: 0 } } else { CornerRadius::ZERO };
        if media(ui, palette, a, rect, mr, ui.scope_id().with(("media", m.id, *index))) {
            match &a.local_path {
                Some(path) => actions.push(Action::Open(path.clone())),
                None => actions.push(Action::Fetch { message_id: m.id, index: *index }),
            }
        }
        y += size.y + 2.;
    }
    if !images.is_empty() && !media_only {
        y += pad.y - 2.;
    }
    for (index, a) in &files {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., 44.));
        if file_row(ui, palette, a, rect, in_color, meta_color, ui.scope_id().with(("file", m.id, *index))) {
            match &a.local_path {
                Some(path) => actions.push(Action::Open(path.clone())),
                None => actions.push(Action::Fetch { message_id: m.id, index: *index }),
            }
        }
        y += 48.;
    }
    if let Some(p) = &m.link_preview {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., 78.));
        if link_preview(ui, palette, outgoing, p, rect, in_color, ui.scope_id().with(("preview", m.id))) {
            actions.push(Action::Open(p.url.clone()));
        }
        y += preview_h;
    }
    let mut revealed_now = None;
    if let (Some(g), Some(rich)) = (galley, rich.as_ref()) {
        let pos = Pos2::new(left, y);
        let text_rect = Rect::from_min_size(pos, g.size());
        let text_response = ui.interact(text_rect, ui.scope_id().with(("text", m.id)), Sense::click());
        if let Some(pointer) = text_response.hover_pos() {
            let cursor = g.cursor_from_pos(pointer - pos);
            let at = cursor.index.0;
            if rich.links.iter().any(|(r, _)| r.contains(&at)) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if text_response.clicked() {
                if let Some((_, url)) = rich.links.iter().find(|(r, _)| r.contains(&at)) {
                    actions.push(Action::Open(url.clone()));
                } else if !revealed && rich.spoilers.iter().any(|r| r.contains(&at)) {
                    revealed_now = Some(true);
                }
            }
        }
        ui.painter().galley(pos, g, in_color);
        y += text_h;
    }
    if show_meta {
        let meta_y = if meta_inline { bubble.bottom() - pad.y - 14. } else { y + 2. };
        let meta_y = if media_only { bubble.bottom() - 22. } else { meta_y };
        let right = bubble.right() - pad.x;
        let meta_x = right - meta_w;
        let color = if media_only { Color32::WHITE } else { meta_color };
        if media_only {
            let shade = Rect::from_min_max(Pos2::new(meta_x - 6., meta_y - 2.), Pos2::new(right + 6., meta_y + 16.));
            ui.painter().rect_filled(shade, CornerRadius::same(9), Color32::from_black_alpha(100));
        }
        ui.painter().galley(Pos2::new(meta_x, meta_y + 1.), meta, color);
        if outgoing {
            let icon = crate::sidebar::status_icon(m.status);
            let at = Rect::from_min_size(Pos2::new(right - 16., meta_y), Vec2::splat(16.));
            let tint = if m.status == DeliveryStatus::Failed { palette.destructive } else { color };
            icon.paint(ui, at, 14., tint);
        }
    }
    let _ = y;

    if reactions {
        reaction_pills(ui, ctx, m, bubble, outgoing, actions);
    }
    if m.status == DeliveryStatus::Failed {
        let at = Pos2::new(bubble.right(), bubble.bottom() + reactions_h + 2.);
        let label = ui.painter().layout_no_wrap("Send failed. Click to retry.".into(), theme::regular(12.), palette.destructive);
        let rect = Rect::from_min_size(Pos2::new(at.x - label.size().x, at.y), label.size());
        ui.painter().galley(rect.min, label, palette.destructive);
        if ui.interact(rect, ui.scope_id().with(("retry", m.id)), Sense::click()).clicked() {
            actions.push(Action::Retry(m.id));
        }
    }
    if show_avatar && !collapsed_below {
        avatar_for(ui, ctx, m, Pos2::new(row.left() + side, bubble.bottom() - 28.));
    }

    // Desktop's hover actions: react and reply, beside the bubble.
    let hover_zone = Rect::from_min_max(Pos2::new(row.left(), bubble.top()), Pos2::new(row.right(), bubble.bottom()));
    if !m.deleted && ui.rect_contains_pointer(hover_zone) {
        let bx = if outgoing { bubble.left() - 64. } else { bubble.right() + 4. };
        let react = Rect::from_min_size(Pos2::new(bx, bubble.center().y - 14.), Vec2::splat(28.));
        let reply = react.translate(Vec2::new(30., 0.));
        for (rect, icon, tip, what) in [(react, Icon::Emoji, "React", 0), (reply, Icon::Reply, "Reply", 1)] {
            let r = ui.interact(rect, ui.scope_id().with(("hover-action", m.id, what)), Sense::click()).on_hover_text(tip);
            if r.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(14), palette.fill);
            }
            icon.paint(ui, rect, 18., palette.text_secondary);
            if r.clicked() {
                actions.push(if what == 0 {
                    let mine = m.reactions.iter().find(|r| r.author.is_none()).map(|r| r.emoji.clone());
                    let remove = mine.as_deref() == Some("❤️");
                    Action::React { message_id: m.id, emoji: "❤️".into(), remove }
                } else {
                    Action::Reply(m.id)
                });
            }
        }
    }
    let _ = response;
    revealed_now
}

fn avatar_for(ui: &Ui, ctx: &Ctx<'_>, m: &Message, at: Pos2) {
    let name = m.author.as_deref().map(|a| ctx.app.name(a)).unwrap_or_default();
    let color = m.author.as_deref().map(|a| a.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b))).unwrap_or(0);
    let photo = m.author.as_deref().and_then(|a| ctx.app.conversation(&ConversationId::direct(a))).and_then(|c| c.avatar_path.clone());
    let spec = widgets::AvatarSpec { name: &name, color, photo: photo.as_deref(), group: false, note_to_self: false };
    widgets::paint_avatar(ui, Rect::from_min_size(at, Vec2::splat(28.)), &spec);
}

/// An image or video's size in a bubble `width` wide.
fn media_size(a: &langis_store::AttachmentMeta, width: f32) -> Vec2 {
    let (w, h) = (a.width.unwrap_or(4).max(1) as f32, a.height.unwrap_or(3).max(1) as f32);
    let w_fit = width.min(320.);
    let h_fit = (w_fit * h / w).clamp(80., 360.);
    Vec2::new(w_fit, h_fit)
}

/// A photo or video. True when clicked.
fn media(ui: &Ui, palette: &Palette, a: &langis_store::AttachmentMeta, rect: Rect, radius: CornerRadius, id: egui::Id) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let image = a.content_type.starts_with("image/");
    match a.local_path.as_deref().filter(|_| image).or(a.thumbnail_path.as_deref()) {
        Some(path) => {
            egui::Image::new(format!("file://{path}")).corner_radius(radius).fit_to_exact_size(rect.size()).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, radius, if palette.dark { Color32::from_gray(40) } else { Color32::from_gray(200) });
        }
    }
    let center = Rect::from_center_size(rect.center(), Vec2::splat(48.));
    if a.local_path.is_none() {
        ui.painter().circle_filled(center.center(), 24., Color32::from_black_alpha(140));
        Icon::ArrowDown.paint(ui, center, 22., Color32::WHITE);
        let size = ui.painter().layout_no_wrap(crate::format::file_size(a.size), theme::regular(11.), Color32::WHITE);
        let at = Rect::from_min_size(rect.left_top() + Vec2::new(8., 8.), size.size() + Vec2::new(12., 4.));
        ui.painter().rect_filled(at, CornerRadius::same(9), Color32::from_black_alpha(140));
        ui.painter().galley(at.min + Vec2::new(6., 2.), size, Color32::WHITE);
    } else if !image {
        ui.painter().circle_filled(center.center(), 24., Color32::from_black_alpha(140));
        let tri = [center.center() + Vec2::new(-6., -9.), center.center() + Vec2::new(-6., 9.), center.center() + Vec2::new(10., 0.)];
        ui.painter().add(egui::Shape::convex_polygon(tri.to_vec(), Color32::WHITE, Stroke::NONE));
    }
    response.clicked()
}

/// A file attachment: Desktop's generic file icon, name and size.
fn file_row(ui: &Ui, palette: &Palette, a: &langis_store::AttachmentMeta, rect: Rect, text: Color32, meta: Color32, id: egui::Id) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let audio = a.voice || a.content_type.starts_with("audio/");
    if audio {
        // Desktop's audio player: a round play button.
        let circle = Rect::from_min_size(rect.min + Vec2::new(0., 4.), Vec2::splat(36.));
        let (fill, mark) = if text == Color32::WHITE { (Color32::WHITE, palette.accent) } else { (palette.accent, Color32::WHITE) };
        ui.painter().circle_filled(circle.center(), 18., fill);
        let c = circle.center();
        let tri = vec![c + Vec2::new(-5., -8.), c + Vec2::new(-5., 8.), c + Vec2::new(9., 0.)];
        ui.painter().add(egui::Shape::convex_polygon(tri, mark, Stroke::NONE));
        let name = if a.voice { "Voice message".to_owned() } else { a.file_name.clone().unwrap_or_else(|| "Audio".into()) };
        ui.painter().text(rect.min + Vec2::new(46., 3.), Align2::LEFT_TOP, name, theme::medium(14.), text);
        let detail = if a.local_path.is_some() { crate::format::file_size(a.size) } else { format!("{} · Download", crate::format::file_size(a.size)) };
        ui.painter().text(rect.min + Vec2::new(46., 24.), Align2::LEFT_TOP, detail, theme::regular(12.), meta);
        return response.clicked();
    }
    let icon = Rect::from_min_size(rect.min + Vec2::new(0., 4.), Vec2::new(30., 36.));
    ui.painter().rect_filled(icon, CornerRadius::same(4), if text == Color32::WHITE { Color32::from_white_alpha(230) } else { palette.popover });
    let ext = a
        .file_name
        .as_deref()
        .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_uppercase()))
        .filter(|e| e.len() <= 4)
        .unwrap_or_default();
    ui.painter().text(icon.center() + Vec2::new(0., 6.), Align2::CENTER_CENTER, ext, theme::semibold(8.), palette.accent);
    let name = a.file_name.clone().unwrap_or_else(|| if a.voice { "Voice message".into() } else { "File".into() });
    let mut job = egui::text::LayoutJob::simple_singleline(name, theme::medium(14.), text);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 42.);
    let name = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(rect.min + Vec2::new(40., 3.), name, text);
    let detail = if a.local_path.is_some() { crate::format::file_size(a.size) } else { format!("{} · Download", crate::format::file_size(a.size)) };
    ui.painter().text(rect.min + Vec2::new(40., 24.), Align2::LEFT_TOP, detail, theme::regular(12.), meta);
    response.clicked()
}

fn quote(ui: &Ui, palette: &Palette, outgoing: bool, q: &langis_store::Quote, rect: Rect, ctx: &Ctx<'_>) {
    let fill = if outgoing {
        Color32::from_white_alpha(56)
    } else if palette.dark {
        Color32::from_black_alpha(60)
    } else {
        Color32::from_white_alpha(150)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let bar = Rect::from_min_size(rect.min, Vec2::new(4., rect.height()));
    let bar_color = if outgoing { Color32::WHITE } else { q.author.as_deref().map(|a| theme::name_color(a, palette.dark)).unwrap_or(palette.accent) };
    ui.painter().rect_filled(bar, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, bar_color);
    let text_color = if outgoing { Color32::WHITE } else { palette.text };
    let author = match &q.author {
        None => "You".to_owned(),
        Some(a) => ctx.app.names.get(a).cloned().unwrap_or_else(|| q.author_name.clone()),
    };
    let mut thumb_w = 0.;
    if let Some(path) = &q.thumbnail {
        thumb_w = 52.;
        let at = Rect::from_min_size(Pos2::new(rect.right() - 52., rect.top()), Vec2::splat(52.));
        egui::Image::new(format!("file://{path}")).corner_radius(CornerRadius { ne: 8, se: 8, nw: 0, sw: 0 }).paint_at(ui, at);
    }
    let inner = rect.shrink2(Vec2::new(10., 6.));
    ui.painter().text(Pos2::new(inner.left() + 2., inner.top()), Align2::LEFT_TOP, author, theme::semibold(13.), text_color);
    let body = if q.text.is_empty() { q.attachment.as_ref().map(|a| a.label()).unwrap_or_default() } else { crate::format::one_line(&q.text) };
    let mut job = egui::text::LayoutJob::simple_singleline(body, theme::regular(13.), text_color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width() - thumb_w - 4.);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(Pos2::new(inner.left() + 2., inner.top() + 19.), galley, text_color);
}

fn link_preview(ui: &Ui, palette: &Palette, outgoing: bool, p: &langis_store::LinkPreview, rect: Rect, text: Color32, id: egui::Id) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let fill = if outgoing { Color32::from_white_alpha(40) } else if palette.dark { Color32::from_black_alpha(60) } else { Color32::from_white_alpha(150) };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let mut left = rect.left() + 10.;
    if let Some(path) = p.image.as_ref().and_then(|i| i.local_path.as_deref()) {
        let at = Rect::from_min_size(rect.min, Vec2::new(78., rect.height()));
        egui::Image::new(format!("file://{path}")).corner_radius(CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }).fit_to_exact_size(at.size()).paint_at(ui, at);
        left = at.right() + 10.;
    }
    let width = rect.right() - 10. - left;
    let mut y = rect.top() + 8.;
    if let Some(title) = &p.title {
        let mut job = egui::text::LayoutJob::simple_singleline(title.clone(), theme::semibold(13.), text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(width);
        let g = ui.fonts_mut(|f| f.layout_job(job));
        let h = g.size().y;
        ui.painter().galley(Pos2::new(left, y), g, text);
        y += h + 1.;
    }
    if let Some(desc) = &p.description {
        let mut job = egui::text::LayoutJob::simple(crate::format::one_line(desc), theme::regular(12.), text, width);
        job.wrap.max_rows = 2;
        job.wrap.overflow_character = Some('…');
        let g = ui.fonts_mut(|f| f.layout_job(job));
        let h = g.size().y;
        ui.painter().galley(Pos2::new(left, y), g, text);
        y += h + 1.;
    }
    let domain = p.url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(&p.url).to_owned();
    ui.painter().text(Pos2::new(left, y.min(rect.bottom() - 18.)), Align2::LEFT_TOP, domain, theme::regular(12.), text.gamma_multiply(0.75));
    response.clicked()
}

fn reaction_pills(ui: &Ui, ctx: &Ctx<'_>, m: &Message, bubble: Rect, outgoing: bool, actions: &mut Vec<Action>) {
    let palette = ctx.palette;
    // Each emoji once, most used first, as Desktop groups them.
    let mut counts: Vec<(String, usize, bool)> = Vec::new();
    for r in &m.reactions {
        match counts.iter_mut().find(|(e, ..)| *e == r.emoji) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 |= r.author.is_none();
            }
            None => counts.push((r.emoji.clone(), 1, r.author.is_none())),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.truncate(3);
    let mut pills = Vec::new();
    for (emoji, count, mine) in &counts {
        let mut job = egui::text::LayoutJob::default();
        append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::emoji(14.), color: palette.text, ..Default::default() });
        if *count > 1 {
            job.append(&format!(" {count}"), 0., egui::TextFormat { font_id: theme::semibold(11.), color: palette.text, valign: egui::Align::Center, ..Default::default() });
        }
        let g = ui.fonts_mut(|f| f.layout_job(job));
        let w = (g.size().x + 14.).max(28.);
        pills.push((g, w, *mine, emoji.clone()));
    }
    let total: f32 = pills.iter().map(|p| p.1 + 4.).sum();
    let mut x = if outgoing { bubble.left() + 8. } else { bubble.right() - 8. - total };
    let y = bubble.bottom() - 4.;
    for (i, (g, w, mine, emoji)) in pills.into_iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 22.));
        let fill = if mine { if palette.dark { Color32::from_gray(80) } else { Color32::from_gray(214) } } else if palette.dark { Color32::from_gray(52) } else { Color32::from_gray(240) };
        ui.painter().rect_filled(rect, CornerRadius::same(11), fill);
        ui.painter().rect_stroke(rect, CornerRadius::same(11), Stroke::new(1.5, palette.background), StrokeKind::Outside);
        ui.painter().galley(rect.center() - g.size() / 2., g, palette.text);
        if ui.interact(rect, ui.scope_id().with(("reaction", m.id, i)), Sense::click()).clicked() && mine {
            actions.push(Action::React { message_id: m.id, emoji, remove: true });
        }
        x += w + 4.;
    }
}

/// Desktop's typing bubble: three dots pulsing in an incoming bubble.
fn typing_bubble(ui: &mut Ui, ctx: &Ctx<'_>, author: &str) {
    let palette = ctx.palette;
    ui.add_space(10.);
    let avatar_col = if ctx.group { 36. } else { 0. };
    let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, 36.), Sense::hover());
    let bubble = Rect::from_min_size(Pos2::new(row.left() + 16. + avatar_col, row.top()), Vec2::new(60., 36.));
    if ctx.group {
        let name = ctx.app.name(author);
        let color = author.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
        widgets::paint_avatar(ui, Rect::from_min_size(Pos2::new(row.left() + 16., row.top() + 8.), Vec2::splat(28.)), &widgets::AvatarSpec::person(&name, color));
    }
    ui.painter().rect_filled(bubble, CornerRadius::same(18), palette.bubble_in);
    let t = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((t * 4. - i as f32 * 0.6).sin() + 1.) / 2.;
        let c = palette.text_secondary.gamma_multiply(0.4 + 0.6 * phase);
        ui.painter().circle_filled(Pos2::new(bubble.left() + 18. + i as f32 * 12., bubble.center().y), 4., c);
    }
    ui.ctx().request_repaint_after(Duration::from_millis(50));
}

/// The text of a message, laid out with its styles, mentions and links.
pub struct Rich {
    pub job: egui::text::LayoutJob,
    /// Char ranges of the laid-out text and where each goes.
    pub links: Vec<(std::ops::Range<usize>, String)>,
    pub spoilers: Vec<std::ops::Range<usize>>,
}

pub fn rich_text(m: &Message, name: &dyn Fn(&str) -> String, color: Color32, link_color: Color32, revealed: bool, size: f32) -> Rich {
    use langis_store::TextStyle;
    let mention_fill = if color == Color32::WHITE { Color32::from_white_alpha(56) } else { Color32::from_black_alpha(20) };
    let body = m.long_text.as_ref().map(|_| m.body.as_str()).unwrap_or(&m.body);
    let links = find_links(body);
    let mut cuts = vec![0, body.len()];
    for s in &m.styles {
        cuts.extend([s.start, s.start + s.len]);
    }
    for mention in &m.mentions {
        cuts.extend([mention.start, mention.start + mention.len]);
    }
    for (r, _) in &links {
        cuts.extend([r.start, r.end]);
    }
    cuts.retain(|&c| c <= body.len() && body.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();

    let mut job = egui::text::LayoutJob::default();
    let mut out_links = Vec::new();
    let mut spoilers = Vec::new();
    let mut chars = 0usize;
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a >= b {
            continue;
        }
        let mention = m.mentions.iter().find(|mn| mn.start <= a && a < mn.start + mn.len);
        let text = match mention {
            Some(mn) if mn.start == a => format!("@{}", name(&mn.aci)),
            Some(_) => continue,
            None => body[a..b].to_owned(),
        };
        let active = |style: TextStyle| m.styles.iter().any(|s| s.style == style && s.start <= a && b <= s.start + s.len);
        let mut format = egui::TextFormat { font_id: theme::regular(size), color, line_height: Some(size * 1.43), ..Default::default() };
        if active(TextStyle::Bold) || mention.is_some() {
            format.font_id = theme::semibold(size);
        }
        if mention.is_some() {
            format.background = mention_fill;
        }
        if active(TextStyle::Italic) {
            format.font_id = theme::italic(size);
        }
        if active(TextStyle::Monospace) {
            format.font_id = egui::FontId::monospace(size - 1.);
        }
        if active(TextStyle::Strikethrough) {
            format.strikethrough = Stroke::new(1., color);
        }
        let link = links.iter().find(|(r, _)| r.start <= a && b <= r.end);
        if link.is_some() {
            format.color = link_color;
            format.underline = Stroke::new(1., link_color);
        }
        let n = text.chars().count();
        if active(TextStyle::Spoiler) {
            spoilers.push(chars..chars + n);
            if !revealed {
                format.background = color.gamma_multiply(0.85);
                format.color = Color32::TRANSPARENT;
                format.underline = Stroke::NONE;
            }
        }
        if let Some((_, url)) = link {
            out_links.push((chars..chars + n, url.clone()));
        }
        append_with_emoji(&mut job, &text, format);
        chars += n;
    }
    Rich { job, links: out_links, spoilers }
}

/// Appends `text`, with its emoji in the color emoji font: Inter has
/// plain glyphs for some (❤ ☀ ✔) that would otherwise win.
pub fn append_with_emoji(job: &mut egui::text::LayoutJob, text: &str, format: egui::TextFormat) {
    let chars: Vec<char> = text.chars().collect();
    let is_emoji = |i: usize| {
        let c = chars[i] as u32;
        let next_vs16 = chars.get(i + 1) == Some(&'\u{FE0F}');
        c >= 0x1F000 || (next_vs16 && (c < 0x80 || (0x2000..=0x2BFF).contains(&c) || c == 0xA9 || c == 0xAE || (0x3000..=0x33FF).contains(&c)))
    };
    let mut run = String::new();
    let mut run_emoji = false;
    let emoji_format = egui::TextFormat { font_id: theme::emoji(format.font_id.size), ..format.clone() };
    for i in 0..chars.len() {
        let c = chars[i];
        // Joiners, variation selectors and keycaps stay with what they follow.
        let joins = matches!(c as u32, 0x200D | 0xFE0F | 0x20E3) || (run_emoji && (0x1F3FB..=0x1F3FF).contains(&(c as u32)));
        let emoji = if joins { run_emoji } else { is_emoji(i) || (run_emoji && i > 0 && chars[i - 1] == '\u{200D}') };
        if emoji != run_emoji && !run.is_empty() {
            job.append(&run, 0., if run_emoji { emoji_format.clone() } else { format.clone() });
            run.clear();
        }
        run_emoji = emoji;
        run.push(c);
    }
    if !run.is_empty() {
        job.append(&run, 0., if run_emoji { emoji_format } else { format });
    }
}

/// Web links in `text`: http(s) URLs and bare `www.` hosts, as Desktop
/// makes clickable. Nothing else opens, so a message can't make a `file:`
/// link.
pub fn find_links(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out = Vec::new();
    let mut start = 0;
    for word in text.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("www.") {
            let clean = trimmed.trim_end_matches(['.', ',', '!', '?', ')', ';', ':', '"', '\'']);
            if clean.len() > 8 {
                let url = if lower.starts_with("www.") { format!("https://{clean}") } else { clean.to_owned() };
                out.push((start..start + clean.len(), url));
            }
        }
        start += word.len();
    }
    out
}

/// Desktop's composition area: emoji, the rounded input, attach, send.
fn composer(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let blocked = c.blocked || c.left || c.ended;
    egui::Panel::bottom("composer")
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin { left: 16, right: 16, top: 10, bottom: 12 }))
        .show(ui, |ui| {
            if blocked {
                ui.vertical_centered(|ui| {
                    let text = if c.blocked { "You blocked this chat." } else { "You can't send messages to this group." };
                    ui.label(RichText::new(text).color(palette.text_secondary));
                });
                return;
            }
            if let Some(q) = chat.quote.and_then(|id| chat.messages.iter().find(|m| m.id == id)) {
                let who = match &q.author {
                    None => "You".to_owned(),
                    Some(a) => app.name(a),
                };
                let text = crate::format::one_line(&q.body);
                let mut close = false;
                egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Replying to {who}")).font(theme::semibold(13.)));
                            ui.add(egui::Label::new(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary)).truncate());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            close = widgets::icon_button(ui, Icon::XCompact, 16., "Cancel reply").clicked();
                        });
                    });
                });
                if close {
                    chat.quote = None;
                }
                ui.add_space(6.);
            }
            let mut send = false;
            ui.horizontal(|ui| {
                let emoji = widgets::icon_button(ui, Icon::Emoji, 20., "Emoji");
                if emoji.clicked() {
                    chat.emoji_open = !chat.emoji_open;
                }
                let has_text = !chat.composer.trim().is_empty();
                let right_w = 2. * 32. + 8.;
                let width = (ui.available_width() - right_w).max(100.);
                let text_color = palette.text;
                let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                    let mut job = egui::text::LayoutJob::default();
                    let format = egui::TextFormat { font_id: theme::regular(14.), color: text_color, line_height: Some(20.), ..Default::default() };
                    append_with_emoji(&mut job, text.as_str(), format);
                    job.wrap.max_width = wrap;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let response = egui::Frame::new()
                    .fill(palette.bubble_in)
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(width - 24.);
                        ui.add(
                            egui::TextEdit::multiline(&mut chat.composer)
                                .hint_text("Message")
                                .frame(egui::Frame::NONE)
                                .desired_rows(1)
                                .desired_width(width - 24.)
                                .font(theme::regular(14.))
                                .layouter(&mut layouter)
                                .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter))
                                .id(egui::Id::unique(("composer", &c.id.0))),
                        )
                    })
                    .inner;
                if response.changed() {
                    chat.last_typed = Some(Instant::now());
                    if !chat.typing_sent && !chat.composer.is_empty() {
                        chat.typing_sent = true;
                        app.send(Command::Typing { conversation: c.id.clone(), started: true });
                    }
                }
                if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
                    send = true;
                }
                if !response.has_focus() && app.safety.is_none() && ui.memory(|m| m.focused().is_none()) {
                    response.request_focus();
                }
                if widgets::icon_button(ui, Icon::Plus, 20., "Add attachment").clicked() {
                    attach(app, chat, c);
                }
                if has_text {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                    ui.painter().circle_filled(rect.center(), 16., palette.badge);
                    Icon::SendFill.paint(ui, rect, 18., Color32::WHITE);
                    if r.on_hover_text("Send").clicked() {
                        send = true;
                    }
                } else {
                    widgets::icon_button(ui, Icon::Mic, 20., "Voice message (not available yet)");
                }
                if chat.emoji_open {
                    let anchor = emoji.rect.left_top();
                    if let Some(picked) = chat.emoji.show(ui.ctx(), anchor, &mut chat.emoji_open) {
                        chat.composer.push_str(picked);
                    }
                }
            });
            if send {
                send_text(app, chat, c);
            }
            if chat.typing_sent && chat.last_typed.is_some_and(|t| t.elapsed() > TYPING_IDLE) {
                chat.typing_sent = false;
                app.send(Command::Typing { conversation: c.id.clone(), started: false });
            }
            if chat.typing_sent {
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            }
        });
}

fn send_text(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    let body = chat.composer.trim().to_owned();
    if body.is_empty() {
        return;
    }
    let command = Command::SendText {
        conversation: c.id.clone(),
        body,
        quote: chat.quote.take(),
        preview: None,
        mentions: Vec::new(),
        styles: Vec::new(),
    };
    chat.composer.clear();
    chat.scroll_to_bottom = 10;
    if chat.typing_sent {
        chat.typing_sent = false;
        app.send(Command::Typing { conversation: c.id.clone(), started: false });
    }
    // The draft went out with the message.
    if !chat.draft_at_open.is_empty() {
        chat.draft_at_open.clear();
        let id = c.id.clone();
        app.load(move |db| async move {
            let _ = db.set_draft(&id, None).await;
            db.conversations().await.ok().map(crate::app::Loaded::Conversations)
        });
    }
    app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
}

/// Picks files to send: images go optimized as photos, the rest as files,
/// with the composer's text as the caption.
fn attach(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    let Some(paths) = rfd::FileDialog::new().set_title("Add attachment").pick_files() else { return };
    let caption = std::mem::take(&mut chat.composer).trim().to_owned();
    let quote = chat.quote.take();
    let mut images = Vec::new();
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            app.toast(format!("Couldn't read {}", path.display()));
            continue;
        };
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "heic") {
            images.push(langis_core::OutgoingImage { bytes: std::sync::Arc::new(bytes), file_name });
        } else {
            let command = Command::SendFile {
                conversation: c.id.clone(),
                file: langis_core::OutgoingFile { bytes: std::sync::Arc::new(bytes), file_name: file_name.unwrap_or_else(|| "file".into()), voice: false },
                caption: caption.clone(),
                quote,
                mentions: Vec::new(),
                styles: Vec::new(),
            };
            app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
        }
    }
    if !images.is_empty() {
        let command = Command::SendImages { conversation: c.id.clone(), images, caption, quote, mentions: Vec::new(), styles: Vec::new() };
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
    chat.scroll_to_bottom = 10;
}

#[cfg(test)]
mod tests {
    use super::find_links;

    #[test]
    fn only_web_links() {
        let links = find_links("see https://example.org/a, www.example.com. not file:///etc/passwd");
        let urls: Vec<_> = links.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(urls, ["https://example.org/a", "https://www.example.com"]);
    }
}
