//! The open chat: Desktop's conversation header, timeline and composition
//! area.

use std::collections::{HashMap, HashSet};
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

/// Messages each side of one jumped to.
pub const AROUND: u32 = 30;

/// Messages from one person this close together are drawn as one group
/// (Desktop's `areMessagesInSameGroup`: three minutes).
const GROUP_GAP_MS: i64 = 3 * 60 * 1000;


pub struct ChatView {
    pub id: ConversationId,
    /// Desktop's select mode: the chosen messages, in the order chosen.
    pub selected: Option<Vec<i64>>,
    /// Oldest first.
    pub messages: Vec<Message>,
    pub loaded: bool,
    pub loading_older: bool,
    pub at_start: bool,
    pub composer: String,
    pub quote: Option<i64>,
    pub typing_sent: bool,
    /// When "typing" last went out; resent every 10s while typing.
    pub typing_refreshed: Option<Instant>,
    pub last_typed: Option<Instant>,
    pub emoji_open: bool,
    pub emoji: crate::emoji_picker::Picker,
    pub revealed: HashSet<i64>,
    /// Desktop's `displayLimit`: how much of a long message's text shows,
    /// raised by Read more.
    pub expanded: HashMap<i64, usize>,
    /// Content height before older messages were added on top, to keep
    /// the view where it was.
    keep_from_height: Option<f32>,
    last_height: f32,
    /// Frames left to hold the view at the bottom (the content's height
    /// settles over a frame or two as images load).
    pub scroll_to_bottom: u8,
    pub draft_at_open: String,
    /// Scrolled near the top: load the page before.
    want_older: bool,
    /// Where to scroll to on the next frame.
    pending_offset: Option<f32>,
    /// The context menu or reaction bar, if open.
    pub popup: Option<crate::actions::Popup>,
    pub dialog: Option<crate::actions::Dialog>,
    /// Pinned messages, newest pin first.
    pub pinned: Vec<Message>,
    /// Which pin the pinned bar shows.
    pinned_index: usize,
    /// Our message being edited in the composer.
    pub editing: Option<i64>,
    /// The newest messages are loaded (false after jumping back in time,
    /// until scrolling down reaches them).
    pub at_end: bool,
    loading_newer: bool,
    want_newer: bool,
    /// A message to bring into view, and one to flash once it is.
    pub scroll_target: Option<i64>,
    /// Desktop's `LastSeenIndicator`: how many were unread when the chat
    /// opened, the message the divider goes above, and whether the chat
    /// still has to scroll to it.
    pub unread_at_open: u32,
    /// Unread messages that mention us, oldest first: the @ button's.
    pub unread_mentions: Vec<i64>,
    pub unread_marker: Option<(i64, u32)>,
    /// `TimelineFloatingHeader`: the last scroll offset, and when it moved.
    last_offset: f32,
    scrolled_at: Option<Instant>,
    scroll_to_unread: bool,
    highlight: Option<(i64, Instant)>,
    /// Scrolled up from the bottom (for the jump-down button).
    pub scrolled_up: bool,
    /// Reload the newest page (the jump-down button, away from the end).
    want_latest: bool,
    /// A message to load around and jump to, once the frame is drawn.
    pending_jump: Option<i64>,
    /// The composer's formatting and mentions.
    pub spans: crate::composer::Spans,
    /// The composer's link preview.
    pub preview: crate::composer::Preview,
    /// Which @mention suggestion is selected.
    pub mention_index: usize,
    pub attach_menu: bool,
    /// A voice message being recorded.
    pub recording: Option<crate::voice::Recording>,
    /// Search in this chat, while open.
    pub find: Option<crate::search::Find>,
}

/// Store reads for the open chat.
pub enum ChatLoaded {
    Details { message: i64, details: crate::actions::Details },
    Pinned { conversation: ConversationId, messages: Vec<Message> },
    /// Messages around `target`, after a jump to it.
    Around { conversation: ConversationId, target: i64, messages: Vec<Message> },
    /// The page after the newest loaded.
    Newer { conversation: ConversationId, messages: Vec<Message> },
    /// A message found to jump to (a quote's original).
    Jump { conversation: ConversationId, message_id: i64 },
}

/// What the timeline asked for, applied after it's drawn.
enum Action {
    Retry(i64),
    Fetch { message_id: i64, index: usize },
    React { message_id: i64, emoji: String, remove: bool },
    Reply(i64),
    Open(String),
    /// A person's `ContactModal`.
    Contact(String),
    /// Opens a 1:1 chat (a shared contact's Message).
    Chat(langis_store::ConversationId),
    /// Read more: 3000 more characters of a long message.
    ReadMore(i64),
    /// Play or pause a message's audio, or (at, 0 to 1) play from a point.
    Audio { message_id: i64, title: String, path: String, duration: f32, at: Option<f32>, voice: bool },
    AudioRate,
    /// The context menu for a message, at a point.
    Menu { message_id: i64, at: Pos2 },
    /// The reaction bar for a message, at a point.
    ReactBar { message_id: i64, at: Pos2 },
    /// Scroll to the message a quote quotes.
    JumpToQuote { author: Option<String>, sent_at: i64 },
    /// Scrolls to a message by id.
    Jump(i64),
    /// Show a photo or video in the viewer.
    Lightbox { message_id: i64, index: usize },
    Vote { message_id: i64, options: Vec<u32> },
    /// A sticker clicked: open its pack.
    StickerPack(i64),
    PollVotes(i64),
    /// Their view-once message, its one viewing.
    ViewOnce(i64),
    FetchLongText(i64),
}

impl ChatView {
    pub fn new(id: ConversationId, draft: String) -> Self {
        Self {
            id,
            selected: None,
            messages: Vec::new(),
            loaded: false,
            loading_older: false,
            at_start: false,
            composer: draft.clone(),
            quote: None,
            typing_sent: false,
            typing_refreshed: None,
            last_typed: None,
            emoji_open: false,
            emoji: Default::default(),
            revealed: HashSet::new(),
            expanded: HashMap::new(),
            keep_from_height: None,
            last_height: 0.,
            scroll_to_bottom: 10,
            draft_at_open: draft,
            want_older: false,
            pending_offset: None,
            popup: None,
            dialog: None,
            pinned: Vec::new(),
            pinned_index: 0,
            editing: None,
            at_end: true,
            loading_newer: false,
            want_newer: false,
            scroll_target: None,
            unread_at_open: 0,
            unread_mentions: Vec::new(),
            unread_marker: None,
            last_offset: 0.,
            scrolled_at: None,
            scroll_to_unread: false,
            highlight: None,
            scrolled_up: false,
            want_latest: false,
            pending_jump: None,
            spans: Default::default(),
            preview: Default::default(),
            mention_index: 0,
            attach_menu: false,
            recording: None,
            find: None,
        }
    }

    /// Loads around `message_id` and shows it, after this frame.
    pub fn jump_later(&mut self, message_id: i64) {
        self.pending_jump = Some(message_id);
    }

    pub fn on_loaded(&mut self, loaded: ChatLoaded) {
        match loaded {
            ChatLoaded::Details { message, details } => {
                if let Some(crate::actions::Dialog::Info { message: open, details: slot }) = self.dialog.as_mut()
                    && *open == message
                {
                    *slot = Some(details);
                }
            }
            ChatLoaded::Pinned { conversation, messages } => {
                if conversation == self.id {
                    self.pinned = messages;
                    self.pinned_index = self.pinned_index.min(self.pinned.len().saturating_sub(1));
                }
            }
            ChatLoaded::Around { conversation, target, messages } => {
                if conversation != self.id || messages.is_empty() {
                    return;
                }
                let before = messages.iter().position(|m| m.id == target).unwrap_or(0);
                let after = messages.len() - before - 1;
                self.at_start = (before as u32) < AROUND;
                self.at_end = (after as u32) < AROUND;
                self.messages = messages;
                self.loaded = true;
                self.loading_older = false;
                self.loading_newer = false;
                self.scroll_to_bottom = 0;
                self.scroll_target = Some(target);
            }
            // Handled by the app (it needs to load around the message).
            ChatLoaded::Jump { .. } => {}
            ChatLoaded::Newer { conversation, mut messages } => {
                if conversation != self.id {
                    return;
                }
                self.loading_newer = false;
                self.at_end = (messages.len() as u32) < PAGE;
                messages.retain(|m| !self.messages.iter().any(|have| have.id == m.id));
                self.messages.append(&mut messages);
            }
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
            // The divider above the oldest unread: that many incoming
            // messages back from the newest (all loaded ones, if fewer).
            if self.unread_at_open > 0 && self.unread_marker.is_none() {
                let incoming: Vec<i64> = self.messages.iter().rev().filter(|m| m.direction == Direction::Incoming && m.notice.is_none()).take(self.unread_at_open as usize).map(|m| m.id).collect();
                if let Some(oldest) = incoming.last() {
                    self.unread_marker = Some((*oldest, self.unread_at_open));
                    self.scroll_to_unread = true;
                    self.scroll_to_bottom = 0;
                }
            }
        }
    }

    /// A message that's new or changed.
    pub fn upsert(&mut self, message: Message) {
        // Past what's loaded: it comes with the next page down.
        let beyond = !self.at_end && self.messages.last().is_some_and(|l| (message.sent_at, message.id) > (l.sent_at, l.id));
        match self.messages.iter_mut().find(|m| m.id == message.id) {
            Some(have) => *have = message,
            None if beyond => {}
            None => {
                let at = self.messages.partition_point(|m| (m.sent_at, m.id) <= (message.sent_at, message.id));
                if message.direction == Direction::Outgoing {
                    self.scroll_to_bottom = 10;
                    // Desktop drops the divider once we send.
                    self.unread_marker = None;
                }
                self.messages.insert(at, message);
            }
        }
    }

    /// Puts our message in the composer to edit (Desktop's edit mode).
    pub fn start_editing(&mut self, app: &App, m: Message) {
        let _ = app;
        let (text, spans) = crate::composer::editable(&m);
        self.composer = text;
        self.spans = spans;
        self.preview = Default::default();
        self.editing = Some(m.id);
        self.quote = None;
    }

    /// Keeps the composer's text as the chat's draft (a frontend's own
    /// write, as the frontend guide allows).
    pub fn save_draft(&self, app: &App) {
        if self.composer == self.draft_at_open || self.editing.is_some() {
            return;
        }
        let (id, text, styles) = (self.id.clone(), self.composer.clone(), self.spans.styles.clone());
        app.load(move |db| async move {
            let draft = Some(text.as_str()).filter(|t| !t.trim().is_empty());
            let _ = db.set_draft_with_styles(&id, draft, &styles).await;
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
    if std::mem::take(&mut app.find_requested) {
        chat.find = Some(crate::search::Find::open());
    }
    if std::mem::take(&mut app.select_requested) {
        chat.selected = Some(Vec::new());
    }
    if chat.selected.is_some() && chat.dialog.is_none() && chat.popup.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        chat.selected = None;
    }
    crate::search::find_bar(app, ui, &palette, &mut chat);
    // Desktop's header shows the mini player, while there is one, in
    // place of the pinned messages.
    if !crate::audio::mini_player(app, ui, &palette) {
        pinned_bar(app, ui, &palette, &mut chat);
    }
    if chat.selected.is_some() {
        select_bar(app, ui, &palette, &mut chat);
    } else if !crate::panes::message_request_bar(app, ui, &palette, &conversation) {
        crate::composer::show(app, ui, &palette, &mut chat, &conversation);
    }
    let mut actions = Vec::new();
    egui::CentralPanel::no_frame().show(ui, |ui| {
        timeline(app, ui, &palette, &mut chat, &conversation, &mut actions);
    });
    let id = chat.id.clone();
    let older = std::mem::take(&mut chat.want_older);
    let newer = std::mem::take(&mut chat.want_newer);
    let latest = std::mem::take(&mut chat.want_latest);
    let jump = chat.pending_jump.take();
    if newer && let Some(last) = chat.messages.last().map(|m| m.id) {
        chat.loading_newer = true;
        let conversation = id.clone();
        app.load(move |db| async move {
            let messages = db.messages_after(&conversation, last, PAGE).await.ok()?;
            Some(crate::app::Loaded::Chat(ChatLoaded::Newer { conversation, messages }))
        });
    }
    app.chat = Some(chat);
    if older {
        app.load_older();
    }
    if latest {
        app.open(id.clone());
    }
    if let Some(message_id) = jump {
        app.open_at(id.clone(), message_id);
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
            Action::Open(path) => open_link(app, &path),
            Action::Audio { message_id, title, path, duration, at, voice } => {
                let starting = app.audio.active(message_id).is_none_or(|p| !p.playing());
                app.audio.toggle(message_id, &id, title, &path, duration, at);
                // A voice message played: viewed, as Desktop marks it.
                if voice && starting {
                    app.send(Command::MarkViewed { message_id });
                }
            }
            Action::AudioRate => app.audio.cycle_rate(),
            Action::Contact(aci) => crate::contact_modal::open(app, Some(id.clone()), aci),
            Action::Chat(chat) => app.open(chat),
            Action::ReadMore(message_id) => {
                if let Some(chat) = app.chat.as_mut() {
                    let limit = chat.expanded.entry(message_id).or_insert(READ_MORE_LENGTH);
                    *limit += 3000;
                }
            }
            Action::Menu { message_id, at } => {
                if let Some(chat) = app.chat.as_mut() {
                    chat.popup = Some(crate::actions::Popup::Menu { message: message_id, at });
                }
            }
            Action::Lightbox { message_id, index } => {
                if let Some(m) = app.chat.as_ref().and_then(|c| c.messages.iter().find(|m| m.id == message_id)).cloned() {
                    app.open_lightbox(&m, index);
                }
            }
            Action::ViewOnce(message_id) => app.send(Command::OpenViewOnce { message_id }),
            Action::Vote { message_id, options } => app.send(Command::VotePoll { message_id, options }),
            Action::StickerPack(message_id) => app.send(Command::FindMessageStickerPack { message_id }),
            Action::PollVotes(message) => {
                if let Some(chat) = app.chat.as_mut() {
                    chat.dialog = Some(crate::actions::Dialog::PollVotes { message });
                }
            }
            Action::FetchLongText(message_id) => app.send(Command::FetchLongText { message_id }),
            Action::Jump(message_id) => app.open_at(id.clone(), message_id),
            Action::JumpToQuote { author, sent_at } => {
                let here = app.chat.as_ref().and_then(|c| c.messages.iter().find(|m| m.sent_at == sent_at && m.author == author).map(|m| m.id));
                match here {
                    Some(message_id) => app.open_at(id.clone(), message_id),
                    None => {
                        let conversation = id.clone();
                        app.load(move |db| async move {
                            let m = db.find_message(&conversation, author.as_deref(), sent_at).await.ok().flatten()?;
                            Some(crate::app::Loaded::Chat(ChatLoaded::Jump { conversation, message_id: m.id }))
                        });
                    }
                }
            }
            Action::ReactBar { message_id, at } => {
                if let Some(chat) = app.chat.as_mut() {
                    chat.popup = Some(crate::actions::Popup::Reactions { message: message_id, at, full: false, picker: Default::default() });
                }
            }
        }
    }
    popups(app, ui);
}

/// The open chat's context menu, reaction bar and dialogs.
fn popups(app: &mut App, ui: &mut Ui) {
    use crate::actions::{Do, Popup};
    let Some(mut chat) = app.chat.take() else { return };
    let Some(conversation) = app.conversation(&chat.id).cloned() else {
        app.chat = Some(chat);
        return;
    };
    let mut todo = Vec::new();
    let pinned: Vec<i64> = chat.pinned.iter().map(|m| m.id).collect();
    let mut popup_at: Option<Pos2> = None;
    if let Some(popup) = chat.popup.as_mut() {
        popup_at = Some(match popup {
            Popup::Menu { at, .. } | Popup::Reactions { at, .. } => *at,
        });
        for d in crate::actions::show_popup(app, ui.ctx(), popup, &chat.messages, &pinned, &conversation) {
            todo.push((true, d));
        }
    }
    if let Some(dialog) = chat.dialog.as_mut() {
        for d in crate::actions::show_dialog(app, ui, dialog, &chat.messages, &conversation) {
            todo.push((false, d));
        }
    }
    // The menu's "React" opens the reaction bar in its place.
    let mut replaced = false;
    for (from_popup, d) in todo {
        match d {
            Do::Close => {
                if from_popup {
                    if !replaced {
                        chat.popup = None;
                    }
                } else {
                    chat.dialog = None;
                }
            }
            Do::Reply(id) => {
                chat.quote = Some(id);
                chat.editing = None;
            }
            Do::Edit(id) => {
                if let Some(m) = chat.messages.iter().find(|m| m.id == id) {
                    chat.start_editing(app, m.clone());
                }
            }
            Do::Command(command) => {
                // A selection's delete or forward is done with it.
                if !from_popup {
                    chat.selected = None;
                }
                app.send(command);
            }
            Do::Copy(text) => {
                ui.ctx().copy_text(text);
                app.toast("Copied to clipboard");
            }
            Do::Save(id, index) => {
                if let Some(m) = chat.messages.iter().find(|m| m.id == id).cloned() {
                    crate::actions::save_attachment(app, &m, index);
                }
            }
            Do::ReactBar(id) => {
                let at = popup_at.unwrap_or_default();
                chat.popup = Some(Popup::Reactions { message: id, at, full: false, picker: Default::default() });
                replaced = true;
            }
            Do::Select(id) => chat.selected = Some(vec![id]),
            Do::Open(dialog) => {
                if let crate::actions::Dialog::Info { message, .. } = &dialog {
                    crate::actions::load_details(app, *message);
                }
                chat.dialog = Some(dialog);
            }
        }
    }
    app.chat = Some(chat);
}

/// A file extension for a content type, for naming saved attachments.
pub fn extension_for(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "audio/aac" => "m4a",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "application/pdf" => "pdf",
        _ => "bin",
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
                if widgets::avatar(ui, &widgets::AvatarSpec::of(c), 32.).on_hover_text("Chat settings").clicked() {
                    crate::details::open(app, c.id.clone());
                }
                ui.add_space(4.);
                ui.vertical(|ui| {
                    ui.add_space(if c.kind == ConversationKind::Group || c.expire_timer > 0 { 8. } else { 16. });
                    if ui.add(egui::Label::new(RichText::new(&c.title).font(theme::semibold(14.))).sense(egui::Sense::click())).clicked() {
                        crate::details::open(app, c.id.clone());
                    }
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
                    let more = widgets::icon_button(ui, Icon::More, 20., "More");
                    header_menu(app, ui, &more, c);
                    if widgets::icon_button(ui, Icon::Search, 20., "Search in chat").clicked() {
                        app.find_requested = true;
                    }
                    if c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked {
                        let group = c.kind == ConversationKind::Group;
                        if widgets::icon_button(ui, Icon::Phone, 20., "Start a voice call").clicked() {
                            if group {
                                crate::panes::open_lobby(app, c.id.clone(), false);
                            } else {
                                app.send(Command::StartCall { conversation: c.id.clone(), video: false });
                            }
                        }
                        if widgets::icon_button(ui, Icon::Video, 20., "Start a video call").clicked() {
                            if group {
                                crate::panes::open_lobby(app, c.id.clone(), true);
                            } else {
                                app.send(Command::StartCall { conversation: c.id.clone(), video: true });
                            }
                        }
                    }
                });
            });
        });
}

/// Desktop's `PinnedMessagesBar`: the pinned message under the header;
/// clicking shows it, and with several, the next click shows the next.
fn pinned_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    if chat.pinned.is_empty() {
        return;
    }
    let index = chat.pinned_index.min(chat.pinned.len() - 1);
    let m = chat.pinned[index].clone();
    let mut jump = false;
    let mut unpin = false;
    egui::Panel::top("pinned-bar")
        .exact_size(48.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1., palette.border));
            let r = ui.interact(rect, ui.scope_id().with("pinned-bar"), Sense::click());
            if r.hovered() {
                ui.painter().rect_filled(rect, 0, palette.fill);
            }
            // Which pin of how many: Desktop's stacked marks on the left.
            let n = chat.pinned.len().min(3);
            for i in 0..n {
                let h = (rect.height() - 16.) / n as f32;
                let mark = Rect::from_min_size(Pos2::new(rect.left() + 16., rect.top() + 8. + i as f32 * h), Vec2::new(2., h - 2.));
                let on = i == index.min(n - 1);
                ui.painter().rect_filled(mark, 1, if on { palette.text } else { palette.text_secondary.gamma_multiply(0.4) });
            }
            crate::glyphs::pin(ui, Rect::from_center_size(Pos2::new(rect.left() + 38., rect.center().y), Vec2::splat(18.)), palette.text);
            let who = match &m.author {
                None => "You".to_owned(),
                Some(a) => app.name(a),
            };
            ui.painter().text(Pos2::new(rect.left() + 56., rect.top() + 8.), Align2::LEFT_TOP, who, theme::semibold(13.), palette.text);
            let text = if m.body.is_empty() { m.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_default() } else { crate::format::one_line(&m.body) };
            let mut job = egui::text::LayoutJob::simple_singleline(text, theme::regular(13.), palette.text_secondary);
            job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 120.);
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter().galley(Pos2::new(rect.left() + 56., rect.top() + 26.), g, palette.text_secondary);
            let close = Rect::from_center_size(Pos2::new(rect.right() - 28., rect.center().y), Vec2::splat(28.));
            let cr = ui.interact(close, ui.scope_id().with("unpin"), Sense::click()).on_hover_text("Unpin");
            if cr.hovered() {
                ui.painter().rect_filled(close, CornerRadius::same(8), palette.fill_pressed);
            }
            Icon::XCompact.paint(ui, close, 16., palette.text_secondary);
            if cr.clicked() {
                unpin = true;
            } else if r.clicked() {
                jump = true;
            }
        });
    if unpin {
        app.send(Command::UnpinMessage { message_id: m.id });
    } else if jump {
        chat.pinned_index = (index + 1) % chat.pinned.len();
        let id = chat.id.clone();
        let _ = id;
        if chat.messages.iter().any(|x| x.id == m.id) {
            chat.scroll_target = Some(m.id);
        } else {
            chat.pending_jump = Some(m.id);
        }
    }
}

/// Desktop's `LastSeenIndicator`: a 1px gray-45 line, then "N Unread
/// Messages" in bold, 25 above and 35 below.
fn last_seen_indicator(ui: &mut Ui, palette: &Palette, count: u32) -> Rect {
    let text = if count == 1 { "1 Unread Message".to_owned() } else { format!("{count} Unread Messages") };
    let galley = ui.painter().layout_no_wrap(text, theme::semibold(13.), palette.text);
    let height = 25. + 1. + 3. + galley.size().y + 35.;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let line = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + 25.), Vec2::new(rect.width(), 1.));
    ui.painter().rect_filled(line, 0, theme::rgb(0x848484));
    let color = if palette.dark { theme::rgb(0xe9e9e9) } else { theme::rgb(0x1b1b1b) };
    ui.painter().galley(Pos2::new(rect.center().x - galley.size().x / 2., line.bottom() + 3.), galley, color);
    rect
}

fn timeline(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation, actions: &mut Vec<Action>) {
    let group = c.kind == ConversationKind::Group;
    let typing: Vec<String> = app.typing.get(&chat.id).map(|t| t.iter().cloned().collect()).unwrap_or_default();
    let mut scroll = egui::ScrollArea::vertical().id_salt(("timeline", &chat.id.0)).auto_shrink(false).stick_to_bottom(chat.at_end);
    let to_bottom = chat.scroll_to_bottom > 0;
    if to_bottom {
        if chat.loaded {
            chat.scroll_to_bottom -= 1;
        }
        // Straight to the end, as Desktop opens a chat: no scroll animation.
        scroll = scroll.vertical_scroll_offset((chat.last_height - ui.available_height()).max(0.));
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
        let menu_for = chat.popup.as_ref().map(|p| match p {
            crate::actions::Popup::Menu { message, .. } | crate::actions::Popup::Reactions { message, .. } => *message,
        });
        let selecting = chat.selected.is_some();
        let ctx = Ctx { app, palette, group, width, chat_id: &chat.id, menu_for, selecting, expanded: &chat.expanded };
        let messages = &chat.messages;
        let mut reached = None;
        let view_top = ui.clip_rect().top();
        let mut oldest_visible: Option<(i64, i64)> = None;
        for (i, m) in messages.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| &messages[p]);
            let next = messages.get(i + 1);
            if prev.is_none_or(|p| !crate::format::same_day(p.sent_at, m.sent_at)) {
                date_header(ui, palette, m.sent_at);
            }
            if let Some((marker, count)) = chat.unread_marker
                && marker == m.id
            {
                let rect = last_seen_indicator(ui, palette, count);
                if chat.scroll_to_unread {
                    chat.scroll_to_unread = false;
                    ui.scroll_to_rect_animation(rect, Some(egui::Align::Min), egui::style::ScrollAnimation::none());
                }
            }
            let revealed = chat.revealed.contains(&m.id);
            let top = ui.cursor().top();
            if let Some(reveal) = message(ui, &ctx, m, prev, next, revealed, actions) {
                if reveal {
                    chat.revealed.insert(m.id);
                }
            }
            let area = Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
            if oldest_visible.is_none() && area.bottom() > view_top {
                oldest_visible = Some((m.id, m.sent_at));
            }
            if let Some(selected) = chat.selected.as_mut()
                && m.notice.is_none()
            {
                select_row(ui, palette, area, m.id, selected);
            }
            if chat.scroll_target == Some(m.id) {
                ui.scroll_to_rect_animation(area, Some(egui::Align::Center), egui::style::ScrollAnimation::none());
                reached = Some(m.id);
            }
            if let Some((id, since)) = chat.highlight
                && id == m.id
            {
                // Desktop's flash on a message jumped to.
                let t = since.elapsed().as_secs_f32();
                if t < 2. {
                    let alpha = ((1. - t / 2.) * 60.) as u8;
                    ui.painter().rect_filled(area, 0, palette.accent.gamma_multiply_u8(alpha));
                    ui.ctx().request_repaint();
                }
            }
        }
        if let Some(id) = reached {
            chat.scroll_target = None;
            chat.highlight = Some((id, Instant::now()));
        }
        if !chat.at_end && chat.loaded {
            ui.vertical_centered(|ui| ui.spinner());
        }
        if !typing.is_empty() {
            typing_bubble(ui, &ctx, &typing);
        }
        ui.add_space(16.);
        ui.allocate_response(Vec2::new(1., 1.), Sense::hover());
        oldest_visible
    });
    let oldest_visible = output.inner;
    chat.last_height = output.content_size.y;
    if let Some(before) = chat.keep_from_height.take() {
        chat.pending_offset = Some(output.state.clamped_offset().y + (output.content_size.y - before).max(0.));
        ui.ctx().request_repaint();
    }
    if output.state.clamped_offset().y < 80. && chat.loaded && !chat.at_start && !chat.loading_older && !chat.messages.is_empty() {
        chat.want_older = true;
    }
    let from_bottom = output.content_size.y - output.inner_rect.height() - output.state.clamped_offset().y;
    if from_bottom < 80. && chat.loaded && !chat.at_end && !chat.loading_newer && chat.scroll_target.is_none() {
        chat.want_newer = true;
    }
    let was_up = chat.scrolled_up;
    chat.scrolled_up = from_bottom > 400. || !chat.at_end;
    // Back at the newest messages: what arrived meanwhile is read now.
    if was_up && !chat.scrolled_up && app.focused && (c.unread_count > 0 || c.marked_unread) {
        app.send(Command::MarkRead { conversation: c.id.clone() });
    }
    // `TimelineFloatingHeader`: the oldest visible message's date, in a
    // pill 10 from the top, for 3 seconds after scrolling (not while the
    // chat's very first message shows).
    let offset = output.state.clamped_offset().y;
    if (offset - chat.last_offset).abs() > 0.5 && chat.loaded {
        if chat.last_offset != 0. || offset != 0. {
            chat.scrolled_at = Some(Instant::now());
        }
        chat.last_offset = offset;
    }
    let recent = chat.scrolled_at.is_some_and(|t| t.elapsed().as_secs_f32() < 3.);
    let first_shown = chat.at_start && oldest_visible.map(|(id, _)| id) == chat.messages.first().map(|m| m.id);
    let shown = ui.ctx().animate_bool_with_time(ui.scope_id().with("floating-header"), recent && !first_shown && chat.scroll_to_bottom == 0, 0.25);
    if recent {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    if shown > 0.
        && let Some((_, at)) = oldest_visible
    {
        let text = crate::format::day_label(at);
        let galley = ui.painter().layout_no_wrap(text, theme::regular(13.), palette.text_secondary);
        let pill = Rect::from_center_size(
            Pos2::new(output.inner_rect.center().x - 7., output.inner_rect.top() + 10. + (galley.size().y + 12.) / 2.),
            galley.size() + Vec2::new(20., 12.),
        );
        let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Middle, ui.scope_id().with("floating-header")));
        let shadow = egui::epaint::Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha((51. * shown) as u8) };
        painter.add(shadow.as_shape(pill, CornerRadius::same(255)));
        let fill = if palette.dark { theme::rgb(0x2e2e2e) } else { Color32::WHITE };
        painter.rect_filled(pill, CornerRadius::same(255), fill.gamma_multiply(shown));
        painter.galley(pill.center() - galley.size() / 2., galley, palette.text_secondary.gamma_multiply(shown));
    }
    // Desktop's `ScrollDownButton`s: unread mentions above, then to the
    // bottom, each with its count.
    let mut next_above = output.inner_rect.bottom() - 56.;
    if chat.scrolled_up {
        next_above -= 48.;
    }
    if let Some(&mention) = chat.unread_mentions.first() {
        let rect = Rect::from_min_size(Pos2::new(output.inner_rect.right() - 56., next_above), Vec2::splat(40.));
        let r = ui.interact(rect, ui.scope_id().with("to-mention"), Sense::click());
        ui.painter().circle_filled(rect.center(), 20., palette.popover);
        ui.painter().circle_stroke(rect.center(), 20., Stroke::new(1., palette.border));
        Icon::At.paint(ui, rect, 20., palette.text);
        widgets::count_badge(ui, Pos2::new(rect.right() + 4., rect.top() + 2.), chat.unread_mentions.len() as u32, false);
        if r.on_hover_text("Go to next unread mention").clicked() {
            chat.unread_mentions.remove(0);
            actions.push(Action::Jump(mention));
        }
    }
    if chat.scrolled_up {
        let rect = Rect::from_min_size(output.inner_rect.right_bottom() - Vec2::new(56., 56.), Vec2::splat(40.));
        let r = ui.interact(rect, ui.scope_id().with("to-bottom"), Sense::click());
        ui.painter().circle_filled(rect.center(), 20., palette.popover);
        ui.painter().circle_stroke(rect.center(), 20., Stroke::new(1., palette.border));
        Icon::ChevronShallowDown.paint(ui, rect, 20., palette.text);
        let unread = c.unread_count + c.marked_unread as u32;
        if unread > 0 {
            widgets::count_badge(ui, Pos2::new(rect.right() + 4., rect.top() + 2.), unread, false);
        }
        if r.on_hover_text("Scroll to bottom").clicked() {
            if chat.at_end {
                chat.scroll_to_bottom = 10;
            } else {
                chat.want_latest = true;
            }
        }
    }
}

/// What the timeline's drawing needs from the app.
struct Ctx<'a> {
    app: &'a App,
    palette: &'a Palette,
    group: bool,
    width: f32,
    chat_id: &'a ConversationId,
    /// The message whose menu or reaction bar is open: its hover bar stays.
    menu_for: Option<i64>,
    /// In select mode: the bubbles take no input, and incoming ones make
    /// room for the checkbox.
    selecting: bool,
    expanded: &'a HashMap<i64, usize>,
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

/// Desktop's `TimelineDateHeader`: the day, 13px, with 20 around it.
fn date_header(ui: &mut Ui, palette: &Palette, at: i64) {
    ui.add_space(20.);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(crate::format::day_label(at)).font(theme::regular(13.)).color(palette.text_secondary));
    });
    ui.add_space(20.);
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
    if let Some(state) = m.view_once {
        view_once_bubble(ui, ctx, m, state, actions);
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
    // Desktop's `translate: 16px + 22px` for incoming bubbles in select mode.
    let lead = side + if ctx.selecting && !outgoing { 38. } else { 0. };
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
    // A poll's body is its question, drawn by the poll.
    let body_text = if m.deleted || m.poll.is_some() { String::new() } else { m.body.clone() };
    let jumbo = !m.deleted && m.attachments.is_empty() && m.quote.is_none() && langis_emoji::jumbo_count(&body_text).is_some_and(|n| n <= 5);

    let rich = if m.deleted {
        let mut job = egui::text::LayoutJob::default();
        // Desktop's `Message` names who deleted it: the admin, else its author.
        let text = match &m.deleted_by_admin {
            Some(langis_store::DeletedByAdmin { by: Some(admin) }) => format!("Admin {} deleted this message", ctx.app.name(admin)),
            Some(_) => "You deleted this message".to_owned(),
            None if outgoing => "You deleted this message".to_owned(),
            None => {
                let name = m.author.as_deref().map(|a| ctx.app.name(a)).or_else(|| m.author_name.clone()).unwrap_or_else(|| "Unknown".into());
                format!("{name} deleted this message")
            }
        };
        job.append(&text, 0., egui::TextFormat { font_id: theme::italic(14.), color: in_color, line_height: Some(20.), ..Default::default() });
        Some(Rich { job, links: Vec::new(), spoilers: Vec::new() })
    } else if body_text.is_empty() {
        None
    } else {
        let link_color = if outgoing { Color32::WHITE } else { palette.accent_text };
        let font_size = if jumbo { 32. } else { 14. };
        let limit = ctx.expanded.get(&m.id).copied().unwrap_or(READ_MORE_LENGTH);
        // Desktop's `areLinksEnabled`: no links in a message request not
        // accepted yet; they read as plain text.
        let links_on = ctx.app.conversation(ctx.chat_id).is_none_or(|c| c.accepted);
        let mut rich = rich_text(m, &|sid| ctx.app.name(sid), in_color, if links_on { link_color } else { in_color }, revealed, font_size, limit);
        if !links_on {
            rich.links.retain(|(_, url)| url == READ_MORE);
            for section in &mut rich.job.sections {
                section.format.underline = Stroke::NONE;
            }
        }
        Some(rich)
    };

    // Desktop's `getWidth`: a photo's bubble is as wide as the photo, and
    // its text wraps inside.
    let media_w = images.first().map(|(_, a)| media_size(a, None).x.min(max_bubble)).unwrap_or(0.);
    let inner_max = if images.is_empty() { max_bubble - 24. } else { media_w - 24. };
    let poll_view = m.poll.as_ref().filter(|_| !m.deleted).map(|p| crate::polls::layout(ui, p, inner_max.min(300.), in_color));
    let content = m.content.as_ref().filter(|_| !m.deleted);
    let galley = rich.as_ref().map(|r| {
        let mut job = r.job.clone();
        job.wrap.max_width = inner_max;
        ui.fonts_mut(|f| f.layout_job(job))
    });

    // Metadata: time, "Edited", status.
    let time = crate::format::message_time(m.sent_at);
    let meta_text = if m.edited { format!("Edited  {time}") } else { time };
    let meta = ui.painter().layout_no_wrap(meta_text, theme::regular(11.), Color32::PLACEHOLDER);
    let timer = expiry_left(m);
    if timer.is_some() {
        // The icon steps down as it runs out.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    // Desktop's status icon: 12px tall (18 wide when delivered or read), 6px after the time.
    let status_w = if outgoing { 6. + 12. * crate::sidebar::status_icon(m.status).aspect() } else { 0. } + if timer.is_some() { 16. } else { 0. };
    let meta_w = meta.size().x + status_w;
    let show_meta = !collapsed_below || m.status == DeliveryStatus::Failed || next.is_some_and(|n| n.status != m.status);

    // Sticker: large, no bubble.
    if let Some(s) = sticker {
        let size = Vec2::splat(144.);
        let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, size.y + 18.), Sense::hover());
        let x = if outgoing { row.right() - side - size.x } else { row.left() + lead + avatar_col };
        let rect = Rect::from_min_size(Pos2::new(x, row.top()), size);
        if let Some(path) = s.local_path.as_deref().or(s.thumbnail_path.as_deref())
            && crate::thumbs::visible(ui, rect)
        {
            egui::Image::new(format!("file://{path}")).maintain_aspect_ratio(true).paint_at(ui, rect);
        }
        let r = ui.interact(rect, ui.scope_id().with(("sticker", m.id)), Sense::click());
        if r.clicked() {
            actions.push(Action::StickerPack(m.id));
        }
        if r.secondary_clicked() {
            actions.push(Action::Menu { message_id: m.id, at: r.interact_pointer_pos().unwrap_or(rect.center()) });
        }
        if (ui.rect_contains_pointer(rect) || ctx.menu_for == Some(m.id))
            && let Some((what, at)) = crate::actions::hover_bar(ui, palette, m.id, rect, outgoing)
        {
            actions.push(match what {
                0 => Action::ReactBar { message_id: m.id, at },
                1 => Action::Reply(m.id),
                _ => Action::Menu { message_id: m.id, at },
            });
        }
        let at = Pos2::new(if outgoing { rect.right() - meta_w } else { rect.left() }, rect.bottom() + 2.);
        ui.painter().galley(at, meta, palette.text_secondary);
        if show_avatar && !collapsed_below {
            if let Some(action) = avatar_for(ui, ctx, m, Pos2::new(row.left() + lead, rect.bottom() - 28.)) {
                actions.push(action);
            }
        }
        return None;
    }

    // Sizes of each part.
    let pad = Vec2::new(12., 8.);
    let text_w = galley.as_ref().map(|g| g.size().x).unwrap_or(0.);
    let last_row_w = galley.as_ref().and_then(|g| g.rows.last()).map(|r| r.rect().width()).unwrap_or(0.);
    let meta_inline = show_meta && galley.is_some() && last_row_w + meta_w + 8. <= inner_max;
    let name_galley = author_name.as_ref().map(|n| {
        let color = m.author.as_deref().map(|a| theme::name_color(a, palette.dark)).unwrap_or(palette.text);
        ui.painter().layout_no_wrap(n.clone(), theme::semibold(13.), color)
    });
    let mut content_w = text_w.max(if meta_inline { last_row_w + meta_w + 8. } else if show_meta { meta_w } else { 0. });
    if let Some(n) = &name_galley {
        // Room for the member label's pill beside the name.
        let label_w = m
            .author
            .as_deref()
            .and_then(|a| ctx.app.conversation(ctx.chat_id)?.member_labels.get(a).cloned())
            .map(|l| ui.painter().layout_no_wrap(format!("{} {}", l.emoji.unwrap_or_default(), l.text), theme::regular(12.), palette.text).size().x + 18.)
            .unwrap_or(0.);
        content_w = content_w.max((n.size().x + label_w).min(inner_max));
    }
    let all_audio = !files.is_empty() && files.iter().all(|(_, a)| is_audio(a));
    // Desktop draws no preview on a message with attachments.
    let preview = m.link_preview.as_ref().filter(|_| m.attachments.is_empty());
    if m.quote.is_some() || preview.is_some() || (!files.is_empty() && !all_audio) || poll_view.is_some() || content.is_some() {
        content_w = content_w.max(inner_max.min(300.));
    }
    if all_audio {
        content_w = content_w.max(AUDIO_W.min(inner_max));
    }
    let bubble_w = if images.is_empty() { (content_w + pad.x * 2.).min(max_bubble).max(if jumbo { 0. } else { 48. }) } else { media_w };

    // Heights.
    let name_h = name_galley.as_ref().map(|g| g.size().y + 2.).unwrap_or(0.);
    let quote_h = if m.quote.is_some() { 58. } else { 0. };
    let media_h: f32 = images.iter().map(|(_, a)| media_size(a, Some(bubble_w)).y + 2.).sum();
    // `MessageAudio`'s countdown shares the line with the time when the
    // audio is all there is.
    let audio_shares_meta = files.len() == 1 && all_audio && galley.is_none() && preview.is_none() && poll_view.is_none() && show_meta && !meta_inline;
    let audio_h = AUDIO_ROW_H + if audio_shares_meta { 0. } else { 20. };
    let files_h: f32 = files.iter().map(|(_, a)| if is_audio(a) { audio_h } else { 48. }).sum();
    // The preview runs edge to edge; flush with the bubble's top when
    // nothing comes above it, else 4px under it, and 5px over what follows.
    let preview_first = name_galley.is_none() && m.quote.is_none() && files.is_empty();
    let preview_card = preview.map(|p| PreviewCard::layout(ui, palette, p, bubble_w));
    let preview_h = preview_card.as_ref().map(|c| c.height + 5. + if preview_first { 0. } else { 4. }).unwrap_or(0.);
    let poll_h = poll_view.as_ref().map(|p| p.height).unwrap_or(0.);
    let content_h = content.map(content_height).unwrap_or(0.);
    // Desktop's line under a long message's start, while the rest isn't here.
    let long_line = m.long_text.as_ref().filter(|_| !m.deleted).map(|l| match l.state {
        langis_store::LongTextState::Downloading => ("Downloading the rest…", false),
        langis_store::LongTextState::Failed => ("Download Full Message", true),
        langis_store::LongTextState::TooBig => ("Message too long to display more", false),
    });
    let long_h = if long_line.is_some() { 20. } else { 0. };
    let text_h = galley.as_ref().map(|g| g.size().y).unwrap_or(0.) + long_h;
    let meta_h = if show_meta && !meta_inline { 16. } else { 0. };
    let media_only = !images.is_empty() && galley.is_none() && m.quote.is_none() && name_galley.is_none() && files.is_empty();
    let top_pad = if media_only || (images.first().is_some() && name_galley.is_none() && m.quote.is_none()) || (preview_card.is_some() && preview_first) { 0. } else { pad.y };
    let text_block = name_h + quote_h + files_h + preview_h + poll_h + content_h + text_h + meta_h;
    let bubble_h = top_pad + media_h + if media_only && meta_h > 0. { 0. } else if text_block > 0. { text_block + pad.y + if media_h > 0. { pad.y } else { 0. } } else { 0. };
    let bubble_h = bubble_h.max(if media_only { media_h } else { 36. });

    let reactions = !m.reactions.is_empty();
    let reactions_h = if reactions { 18. } else { 0. };
    let failed_h = if m.status == DeliveryStatus::Failed { 18. } else { 0. };
    let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, bubble_h + reactions_h + failed_h), Sense::hover());
    let x = if outgoing { row.right() - side - bubble_w } else { row.left() + lead + avatar_col };
    let bubble = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(bubble_w, bubble_h));
    let response = ui.interact(bubble, ui.scope_id().with(("bubble", m.id)), Sense::click());

    // The bubble, with Desktop's smaller corners inside a group.
    let (r, small) = (18u8, 4u8);
    let radius = if outgoing {
        CornerRadius { nw: r, sw: r, ne: if collapsed_above { small } else { r }, se: if collapsed_below { small } else { r } }
    } else {
        CornerRadius { ne: r, se: r, nw: if collapsed_above { small } else { r }, sw: if collapsed_below { small } else { r } }
    };
    if jumbo {
    } else if outgoing {
        crate::chat_colors::paint(ui.painter(), bubble, radius, crate::chat_colors::for_chat(ctx.app, ctx.chat_id), ui.ctx().content_rect());
    } else {
        ui.painter().rect_filled(bubble, radius, palette.bubble_in);
    }
    let mut y = bubble.top() + top_pad;
    let left = bubble.left() + pad.x;

    if let Some(n) = name_galley {
        // `ContactName`: clicking opens their `ContactModal`.
        let rect = Rect::from_min_size(Pos2::new(left, y), n.size());
        let response = ui.interact(rect, ui.scope_id().with(("author", m.id)), Sense::click());
        if !ctx.selecting {
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked()
                && let Some(aci) = m.author.clone()
            {
                actions.push(Action::Contact(aci));
            }
        }
        let name_w = n.size().x;
        ui.painter().galley(Pos2::new(left, y), n, palette.text);
        // Desktop's `ContactName` label pill: their member label, if any.
        let label = m.author.as_deref().and_then(|a| ctx.app.conversation(ctx.chat_id)?.member_labels.get(a).cloned());
        if let Some(label) = label {
            let color = m.author.as_deref().map(|a| theme::name_color(a, palette.dark)).unwrap_or(palette.text);
            let mut job = egui::text::LayoutJob::default();
            if let Some(emoji) = label.emoji.as_deref().filter(|e| !e.is_empty()) {
                append_with_emoji(&mut job, &format!("{emoji} "), egui::TextFormat { font_id: theme::emoji(12.), color, ..Default::default() });
            }
            job.append(&label.text, 0., egui::TextFormat { font_id: theme::regular(12.), color, ..Default::default() });
            job.wrap = egui::text::TextWrapping::truncate_at_width((bubble_w - pad.x * 2. - name_w - 20.).max(30.));
            let g = ui.fonts_mut(|f| f.layout_job(job));
            let pill = Rect::from_min_size(Pos2::new(left + name_w + 6., y - 1.), Vec2::new(g.size().x + 12., 18.));
            ui.painter().rect_filled(pill, CornerRadius::same(9), color.gamma_multiply(0.14));
            ui.painter().galley(Pos2::new(pill.left() + 6., pill.center().y - g.size().y / 2.), g, color);
        }
        y += name_h;
    }
    if let Some(q) = &m.quote {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., 52.));
        quote(ui, palette, outgoing, q, rect, ctx);
        if !q.story && ui.interact(rect, ui.scope_id().with(("quote", m.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            actions.push(Action::JumpToQuote { author: q.author.clone(), sent_at: q.sent_at });
        }
        y += quote_h;
    }
    for (index, a) in &images {
        let size = media_size(a, Some(bubble_w));
        let rect = Rect::from_min_size(Pos2::new(bubble.left(), y), Vec2::new(bubble_w, size.y));
        let mr = if media_only { radius } else if y <= bubble.top() + 1. { CornerRadius { nw: radius.nw, ne: radius.ne, sw: 0, se: 0 } } else { CornerRadius::ZERO };
        if media(ui, palette, a, rect, mr, ui.scope_id().with(("media", m.id, *index))) {
            match &a.local_path {
                Some(_) => actions.push(Action::Lightbox { message_id: m.id, index: *index }),
                None => actions.push(Action::Fetch { message_id: m.id, index: *index }),
            }
        }
        y += size.y + 2.;
    }
    if !images.is_empty() && !media_only {
        y += pad.y - 2.;
    }
    for (index, a) in &files {
        if is_audio(a) {
            let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., audio_h));
            if let Some(action) = audio_row(ui, ctx, m, a, *index, rect, outgoing) {
                actions.push(action);
            }
            y += audio_h;
            continue;
        }
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., 44.));
        if file_row(ui, palette, a, rect, in_color, meta_color, ui.scope_id().with(("file", m.id, *index))) {
            match &a.local_path {
                Some(path) => actions.push(Action::Open(path.clone())),
                None => actions.push(Action::Fetch { message_id: m.id, index: *index }),
            }
        }
        y += 48.;
    }
    if let (Some(p), Some(card)) = (preview, preview_card) {
        if !preview_first {
            y += 4.;
        }
        let rect = Rect::from_min_size(Pos2::new(bubble.left(), y), Vec2::new(bubble_w, card.height));
        let top = if preview_first { CornerRadius { nw: radius.nw, ne: radius.ne, sw: 0, se: 0 } } else { CornerRadius::ZERO };
        if card.paint(ui, palette, p, rect, top, ui.scope_id().with(("preview", m.id))) {
            actions.push(Action::Open(p.url.clone()));
        }
        y += card.height + 5.;
    }
    if let (Some(view), Some(poll)) = (poll_view, m.poll.as_ref()) {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., poll_h));
        match crate::polls::paint(ui, view, poll, m.id, rect, outgoing, palette) {
            Some(Some(options)) => actions.push(Action::Vote { message_id: m.id, options }),
            Some(None) => actions.push(Action::PollVotes(m.id)),
            None => {}
        }
        y += poll_h;
    }
    if let Some(content) = content {
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(bubble_w - pad.x * 2., content_h));
        let sender = (!outgoing).then(|| m.author.as_deref().map(|a| ctx.app.name(a)).or_else(|| m.author_name.clone()).unwrap_or_else(|| "Unknown".into()));
        if let Some(action) = content_block(ui, palette, content, sender.as_deref(), rect, outgoing, in_color, meta_color, ui.scope_id().with(("content", m.id))) {
            actions.push(action);
        }
        y += content_h;
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
                    actions.push(if url == READ_MORE { Action::ReadMore(m.id) } else { Action::Open(url.clone()) });
                } else if !revealed && rich.spoilers.iter().any(|r| r.contains(&at)) {
                    revealed_now = Some(true);
                }
            }
        }
        let g_h = g.size().y;
        ui.painter().galley(pos, g, in_color);
        if let Some((label, clickable)) = long_line {
            let color = if clickable { if outgoing { Color32::WHITE } else { palette.accent_text } } else { meta_color };
            let at = Pos2::new(left, y + g_h + 2.);
            let lg = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), color);
            let r = Rect::from_min_size(at, lg.size());
            ui.painter().galley(at, lg, color);
            if clickable && ui.interact(r, ui.scope_id().with(("long-text", m.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                actions.push(Action::FetchLongText(m.id));
            }
        }
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
        let meta_end = meta_x + meta.size().x;
        ui.painter().galley(Pos2::new(meta_x, meta_y + 1.), meta, color);
        if let Some(left) = timer {
            let at = Rect::from_min_size(Pos2::new(meta_end + 2., meta_y + 1.), Vec2::splat(14.));
            Icon::timer(left).paint(ui, at, 12., color);
        }
        if outgoing {
            let icon = crate::sidebar::status_icon(m.status);
            let at = Rect::from_min_size(Pos2::new(right - 12., meta_y + 2.), Vec2::splat(12.));
            let tint = if m.status == DeliveryStatus::Failed { palette.destructive } else { color };
            icon.paint(ui, at, 12., tint);
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
        if let Some(action) = avatar_for(ui, ctx, m, Pos2::new(row.left() + lead, bubble.bottom() - 28.)) {
            actions.push(action);
        }
    }

    // Desktop's hover bar: react, reply and more, beside the bubble; and
    // its context menu on right-click.
    let hover_zone = Rect::from_min_max(Pos2::new(row.left(), bubble.top()), Pos2::new(row.right(), bubble.bottom()));
    let menu_open = ctx.menu_for == Some(m.id);
    if !ctx.selecting
        && (ui.rect_contains_pointer(hover_zone) || menu_open)
        && let Some((what, at)) = crate::actions::hover_bar(ui, palette, m.id, bubble, outgoing)
    {
        actions.push(match what {
            0 => Action::ReactBar { message_id: m.id, at },
            1 => Action::Reply(m.id),
            _ => Action::Menu { message_id: m.id, at },
        });
    }
    if !ctx.selecting && response.secondary_clicked() {
        let at = response.interact_pointer_pos().unwrap_or(bubble.center());
        actions.push(Action::Menu { message_id: m.id, at });
    }
    revealed_now
}

/// Desktop's `TapToViewMessage`: a small bubble standing for the media.
fn view_once_bubble(ui: &mut Ui, ctx: &Ctx<'_>, m: &Message, state: langis_store::ViewOnce, actions: &mut Vec<Action>) {
    use langis_store::ViewOnce;
    let palette = ctx.palette;
    let outgoing = m.direction == Direction::Outgoing;
    let a = m.attachments.first();
    let kind = match a.map(|a| a.content_type.as_str()) {
        Some(t) if t.starts_with("video/") => "Video",
        Some(t) if t.starts_with("image/") => "Photo",
        _ => "Media",
    };
    let ready = !outgoing && state == ViewOnce::Ready;
    let downloaded = a.is_some_and(|a| a.local_path.is_some());
    let (icon, label) = match state {
        ViewOnce::Ready if outgoing => (Icon::ViewOnceDashBold, "Media"),
        ViewOnce::Ready if downloaded => (Icon::ViewOnce, kind),
        ViewOnce::Ready => (Icon::ArrowDown, kind),
        ViewOnce::Viewed | ViewOnce::Gone => (Icon::ViewOnceViewedBold, if outgoing { "Media" } else { "Viewed" }),
        ViewOnce::Invalid => (Icon::ViewOnceDashBold, "Media"),
    };
    let text_color = if outgoing { palette.bubble_out_text() } else if ready { palette.text } else { palette.text_secondary };
    let meta_color = if outgoing { palette.bubble_out_meta() } else { palette.text_secondary };
    ui.add_space(10.);
    let show_avatar = ctx.group && !outgoing;
    let avatar_col = if show_avatar { 36. } else { 0. };
    let label_g = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(14.), text_color);
    let meta = ui.painter().layout_no_wrap(crate::format::message_time(m.sent_at), theme::regular(11.), meta_color);
    let status_w = if outgoing { 6. + 12. * crate::sidebar::status_icon(m.status).aspect() } else { 0. };
    let w = 12. + 22. + label_g.size().x + 12. + meta.size().x + status_w + 12.;
    let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, 40.), Sense::hover());
    let x = if outgoing { row.right() - 16. - w } else { row.left() + 16. + avatar_col };
    let bubble = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(w, 40.));
    if outgoing {
        crate::chat_colors::paint(ui.painter(), bubble, CornerRadius::same(18), crate::chat_colors::for_chat(ctx.app, ctx.chat_id), ui.ctx().content_rect());
    } else {
        ui.painter().rect_filled(bubble, CornerRadius::same(18), palette.bubble_in);
    }
    let icon_rect = Rect::from_min_size(Pos2::new(bubble.left() + 12., bubble.center().y - 10.), Vec2::splat(20.));
    icon.paint(ui, icon_rect, 18., text_color);
    ui.painter().galley(Pos2::new(icon_rect.right() + 4., bubble.center().y - label_g.size().y / 2.), label_g, text_color);
    let meta_x = bubble.right() - 12. - meta.size().x - status_w;
    ui.painter().galley(Pos2::new(meta_x, bubble.center().y - 6.), meta, meta_color);
    if outgoing {
        let at = Rect::from_min_size(Pos2::new(bubble.right() - 24., bubble.center().y - 6.), Vec2::splat(12.));
        crate::sidebar::status_icon(m.status).paint(ui, at, 12., meta_color);
    }
    if show_avatar {
        if let Some(action) = avatar_for(ui, ctx, m, Pos2::new(row.left() + 16., bubble.bottom() - 28.)) {
            actions.push(action);
        }
    }
    let r = ui.interact(bubble, ui.scope_id().with(("view-once", m.id)), Sense::click());
    if ready {
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            actions.push(if downloaded { Action::ViewOnce(m.id) } else { Action::Fetch { message_id: m.id, index: 0 } });
        }
    }
    if r.secondary_clicked() {
        actions.push(Action::Menu { message_id: m.id, at: r.interact_pointer_pos().unwrap_or(bubble.center()) });
    }
}

/// The author's avatar beside a group message; clicking opens their
/// `ContactModal`.
fn avatar_for(ui: &Ui, ctx: &Ctx<'_>, m: &Message, at: Pos2) -> Option<Action> {
    let name = m.author.as_deref().map(|a| ctx.app.name(a)).unwrap_or_default();
    let color = m.author.as_deref().map(|a| a.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b))).unwrap_or(0);
    let photo = m.author.as_deref().and_then(|a| ctx.app.conversation(&ConversationId::direct(a))).and_then(|c| c.avatar_path.clone());
    let spec = widgets::AvatarSpec { name: &name, color, photo: photo.as_deref(), group: false, note_to_self: false };
    let rect = Rect::from_min_size(at, Vec2::splat(28.));
    widgets::paint_avatar(ui, rect, &spec);
    let response = ui.interact(rect, ui.scope_id().with(("avatar", m.id)), Sense::click());
    if response.hovered() && !ctx.selecting {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (response.clicked() && !ctx.selecting).then(|| m.author.clone().map(Action::Contact)).flatten()
}

/// Desktop's `getImageDimensionsForTimeline`: the photo's own width within
/// 200 to 300 (or `width`), and the height that keeps its shape within 50 to
/// 450. Unknown sizes get the smallest box.
fn media_size(a: &langis_store::AttachmentMeta, width: Option<f32>) -> Vec2 {
    let (Some(w), Some(h)) = (a.width.filter(|w| *w > 0), a.height.filter(|h| *h > 0)) else {
        return Vec2::new(width.unwrap_or(200.), 50.);
    };
    let target = width.unwrap_or((w as f32).clamp(200., 300.));
    Vec2::new(target, (target * h as f32 / w as f32).round().clamp(50., 450.))
}

/// A photo or video. True when clicked.
fn media(ui: &Ui, palette: &Palette, a: &langis_store::AttachmentMeta, rect: Rect, radius: CornerRadius, id: egui::Id) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let image = a.content_type.starts_with("image/");
    match a.local_path.as_deref().filter(|_| image).or(a.thumbnail_path.as_deref()) {
        Some(path) => {
            // `object-fit: cover`: fills the box, cropped evenly, never stretched.
            let mut uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
            if let (Some(w), Some(h)) = (a.width.filter(|w| *w > 0), a.height.filter(|h| *h > 0)) {
                let (photo, frame) = (w as f32 / h as f32, rect.width() / rect.height());
                if photo > frame {
                    let keep = frame / photo;
                    uv = Rect::from_min_max(Pos2::new((1. - keep) / 2., 0.), Pos2::new((1. + keep) / 2., 1.));
                } else if photo < frame {
                    let keep = photo / frame;
                    uv = Rect::from_min_max(Pos2::new(0., (1. - keep) / 2.), Pos2::new(1., (1. + keep) / 2.));
                }
            }
            if crate::thumbs::visible(ui, rect) {
                egui::Image::new(crate::thumbs::uri(ui.ctx(), path, rect.size())).corner_radius(radius).uv(uv).fit_to_exact_size(rect.size()).paint_at(ui, rect);
            }
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
fn is_audio(a: &langis_store::AttachmentMeta) -> bool {
    a.voice || a.content_type.starts_with("audio/")
}

/// `MessageAudio`: the 36px button, 12 apart from the 47-bar waveform.
const AUDIO_W: f32 = 36. + 12. + crate::audio::BAR_COUNT as f32 * 4. - 2.;
/// 2 above, the button row and 5 under it; the countdown line follows.
const AUDIO_ROW_H: f32 = 2. + 36. + 5.;

/// Desktop's `MessageAudio`: play (or download), the waveform to click
/// into, then the time left and, while playing, the speed.
fn audio_row(ui: &Ui, ctx: &Ctx<'_>, m: &Message, a: &langis_store::AttachmentMeta, index: usize, rect: Rect, outgoing: bool) -> Option<Action> {
    let palette = ctx.palette;
    let dark = palette.dark;
    let id = ui.scope_id().with(("audio", m.id, index));
    let peaks = a.local_path.as_deref().and_then(|p| ctx.app.audio.peaks(ui.ctx(), p));
    let active = ctx.app.audio.active(m.id);
    let playing = active.is_some_and(|p| p.playing());
    let title = if outgoing { "You".to_owned() } else { m.author.as_deref().map(|a| ctx.app.name(a)).unwrap_or_default() };
    let mut action = None;

    // `PlaybackButton`, variant message.
    let button = Rect::from_min_size(rect.min + Vec2::new(0., 2.), Vec2::splat(36.));
    let response = ui.interact(button, id.with("button"), Sense::click());
    let (fill, hover, icon_color) = match (outgoing, dark) {
        (true, _) => (Color32::from_white_alpha(51), Color32::from_white_alpha(77), Color32::WHITE),
        (false, true) => (Color32::from_white_alpha(51), Color32::from_white_alpha(77), palette.text),
        (false, false) => (Color32::from_white_alpha(204), Color32::from_white_alpha(153), palette.text),
    };
    ui.painter().circle_filled(button.center(), 18., if response.hovered() { hover } else { fill });
    let icon_rect = Rect::from_center_size(button.center(), Vec2::splat(20.));
    match (&a.local_path, &peaks) {
        (None, _) => {
            Icon::ArrowDown.paint(ui, icon_rect, 20., icon_color);
            if response.on_hover_text("Download").clicked() {
                action = Some(Action::Fetch { message_id: m.id, index });
            }
        }
        (Some(_), None) => {
            egui::Spinner::new().size(16.).color(icon_color).paint_at(ui, Rect::from_center_size(button.center(), Vec2::splat(16.)));
        }
        (Some(path), Some(peaks)) => {
            Icon::paint(if playing { Icon::PauseFill } else { Icon::PlayFill }, ui, icon_rect, 20., icon_color);
            if response.on_hover_text(if playing { "Pause audio" } else { "Play audio" }).clicked() {
                action = Some(Action::Audio { message_id: m.id, title: title.clone(), path: path.clone(), duration: peaks.duration, at: None, voice: a.voice && !outgoing });
            }
        }
    }

    // `Waveform`: 2px bars, 2 apart, 4 to 20 tall (2 until it's here), the
    // played part brighter and the bar it's at full height.
    let wave = Rect::from_min_size(Pos2::new(button.right() + 12., button.top() + 8.), Vec2::new(crate::audio::BAR_COUNT as f32 * 4. - 2., 20.));
    let (bar, bar_active) = match (outgoing, dark) {
        (true, _) => (Color32::from_white_alpha(102), Color32::from_white_alpha(204)),
        (false, true) => (Color32::from_white_alpha(102), Color32::from_white_alpha(179)),
        (false, false) => (Color32::from_black_alpha(102), Color32::from_black_alpha(204)),
    };
    let duration = peaks.as_ref().map(|p| p.duration).unwrap_or(0.);
    let position = active.map(|p| p.position()).unwrap_or(0.);
    let at_bar = crate::audio::BAR_COUNT as f32 * position / duration.max(1e-6);
    let min_h = if peaks.is_some() { 4. } else { 2. };
    for i in 0..crate::audio::BAR_COUNT {
        let value = peaks.as_ref().and_then(|p| p.bars.get(i).copied()).unwrap_or(0.);
        let highlight = (i as f32) < at_bar;
        let h = if highlight && i as f32 + 1. >= at_bar { 20. } else { (20. * value).max(min_h) };
        let x = wave.left() + i as f32 * 4.;
        let r = Rect::from_center_size(Pos2::new(x + 1., wave.center().y), Vec2::new(2., h));
        ui.painter().rect_filled(r, CornerRadius::same(1), if highlight { bar_active } else { bar });
    }
    if let (Some(path), Some(peaks)) = (&a.local_path, &peaks) {
        let response = ui.interact(wave.expand2(Vec2::new(0., 8.)), id.with("wave"), Sense::click());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let at = ((p.x - wave.left()) / wave.width()).clamp(0., 1.);
            action = Some(Action::Audio { message_id: m.id, title, path: path.clone(), duration: peaks.duration, at: Some(at), voice: a.voice && !outgoing });
        }
    }

    // The countdown, under the button, and the speed beside it.
    let line = rect.top() + AUDIO_ROW_H + 1.;
    let count_color = match (outgoing, dark) {
        (true, _) | (false, true) => Color32::from_white_alpha(204),
        (false, false) => Color32::from_black_alpha(153),
    };
    let count = if a.local_path.is_none() || duration < 1. { crate::format::file_size(a.size) } else { crate::audio::playback_text(duration - position) };
    ui.painter().text(Pos2::new(button.center().x, line), Align2::CENTER_TOP, count, theme::regular(12.), count_color);
    if playing && let Some(player) = active {
        let pill = Rect::from_min_size(Pos2::new(button.right() + 12., line - 1.), Vec2::new(38., 18.));
        let response = ui.interact(pill, id.with("rate"), Sense::click());
        let (fill, text) = match (outgoing, dark) {
            (true, _) => (Color32::from_white_alpha(51), Color32::from_white_alpha(204)),
            (false, true) => (Color32::from_white_alpha(20), theme::rgb(0xb9b9b9)),
            (false, false) => (Color32::from_black_alpha(20), theme::rgb(0x5e5e5e)),
        };
        ui.painter().rect_filled(pill, CornerRadius::same(4), fill);
        ui.painter().text(pill.center(), Align2::CENTER_CENTER, crate::audio::rate_label(player.rate), theme::semibold(11.), text);
        if response.on_hover_text("Playback speed").clicked() {
            action = Some(Action::AudioRate);
        }
    }
    action
}

fn file_row(ui: &Ui, palette: &Palette, a: &langis_store::AttachmentMeta, rect: Rect, text: Color32, meta: Color32, id: egui::Id) -> bool {
    let response = ui.interact(rect, id, Sense::click());
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
    // `Quote.scss`: the chat's color, shaded, fixed to the window like the
    // bubbles; the bar is its start color, white in our own bubbles.
    let color = crate::chat_colors::for_chat(ctx.app, ctx.chat_id);
    let radius = CornerRadius { nw: 8, ne: 8, sw: 4, se: 4 };
    crate::chat_colors::paint(ui.painter(), rect, radius, crate::chat_colors::quote_fill(color, palette.dark), ui.ctx().content_rect());
    let bar = Rect::from_min_size(rect.min, Vec2::new(4., rect.height()));
    let bar_color = match color {
        _ if outgoing => Color32::WHITE,
        crate::chat_colors::Bubble::Solid(c) | crate::chat_colors::Bubble::Gradient(_, c, _) => c,
    };
    ui.painter().rect_filled(bar, CornerRadius { nw: 8, sw: 4, ne: 0, se: 0 }, bar_color);
    let text_color = if palette.dark { theme::rgb(0xe9e9e9) } else { theme::rgb(0x1b1b1b) };
    let author = match &q.author {
        None => "You".to_owned(),
        Some(a) => ctx.app.names.get(a).cloned().unwrap_or_else(|| q.author_name.clone()),
    };
    let mut thumb_w = 0.;
    if let Some(path) = &q.thumbnail {
        thumb_w = 52.;
        let at = Rect::from_min_size(Pos2::new(rect.right() - 52., rect.top()), Vec2::splat(52.));
        if crate::thumbs::visible(ui, at) {
            egui::Image::new(crate::thumbs::uri(ui.ctx(), path, at.size())).corner_radius(CornerRadius { ne: 8, se: 8, nw: 0, sw: 0 }).paint_at(ui, at);
        }
    }
    let inner = rect.shrink2(Vec2::new(10., 6.));
    ui.painter().text(Pos2::new(inner.left() + 2., inner.top()), Align2::LEFT_TOP, author, theme::semibold(13.), text_color);
    let body = if q.text.is_empty() { q.attachment.as_ref().map(|a| a.label()).unwrap_or_default() } else { crate::format::one_line(&q.text) };
    let mut job = egui::text::LayoutJob::simple_singleline(body, theme::regular(13.), text_color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width() - thumb_w - 4.);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(Pos2::new(inner.left() + 2., inner.top() + 19.), galley, text_color);
}

/// How tall a message's contact card, payment or gift badge is.
fn content_height(content: &langis_store::MessageContent) -> f32 {
    use langis_store::MessageContent;
    match content {
        // `EmbeddedContact`: 4 above, the 52px row, 12, the button, 6 under.
        MessageContent::Contact(c) => 4. + 52. + if c.aci.is_some() { 12. + 32. } else { 0. } + 6.,
        MessageContent::Payment(_) => 48.,
        MessageContent::GiftBadge(_) => 64.,
    }
}

/// Desktop's `EmbeddedContact` (the photo, the name and a chevron, then
/// Message for someone on Signal), its payment notification and its gift
/// badge, which are redeemed on the phone.
#[allow(clippy::too_many_arguments)]
fn content_block(
    ui: &Ui,
    palette: &Palette,
    content: &langis_store::MessageContent,
    sender: Option<&str>,
    rect: Rect,
    outgoing: bool,
    text: Color32,
    secondary: Color32,
    id: egui::Id,
) -> Option<Action> {
    use langis_store::{GiftBadgeState, MessageContent};
    let mut action = None;
    let line = |y: f32, s: String, font: egui::FontId, color: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(s, font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
        let g = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(Pos2::new(rect.left(), y), g, color);
    };
    match content {
        MessageContent::Contact(c) => {
            let name = c.display_nickname().or_else(|| c.display_name()).or_else(|| c.organization.clone()).unwrap_or_else(|| "Unknown contact".into());
            let row = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + 4.), Vec2::new(rect.width(), 52.));
            let spec = widgets::AvatarSpec { name: &name, color: 0, photo: c.avatar.as_ref().and_then(|a| a.local_path.as_deref()), group: false, note_to_self: false };
            widgets::paint_avatar(ui, Rect::from_min_size(row.min, Vec2::splat(52.)), &spec);
            let mut job = egui::text::LayoutJob::simple_singleline(name.clone(), theme::medium(16.), text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(row.width() - 52. - 12. - 32.);
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter().galley(Pos2::new(row.left() + 64., row.center().y - g.size().y / 2.), g, text);
            theme::paint_symbol(ui.painter(), "chevron-[end]", Pos2::new(row.right() - 10., row.center().y), 20., secondary);
            // The row opens who they are: their Signal profile when known.
            let response = ui.interact(row, id.with("row"), Sense::click());
            if let Some(aci) = &c.aci {
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    action = Some(Action::Contact(aci.clone()));
                }
                let button = Rect::from_min_size(Pos2::new(rect.left(), row.bottom() + 12.), Vec2::new(rect.width(), 32.));
                let b = ui.interact(button, id.with("message"), Sense::click());
                let fill = if outgoing { Color32::from_white_alpha(if b.hovered() { 70 } else { 50 }) } else if b.hovered() { palette.fill_pressed } else { palette.fill };
                ui.painter().rect_filled(button, CornerRadius::same(16), fill);
                ui.painter().text(button.center(), Align2::CENTER_CENTER, "Message", theme::semibold(14.), text);
                if b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    action = Some(Action::Chat(langis_store::ConversationId::direct(aci)));
                }
            } else {
                // Without a Signal account, the card lists how to reach them.
                let how: Vec<String> = c.numbers.iter().chain(&c.emails).map(|m| m.value.clone()).collect();
                if !how.is_empty() {
                    response.on_hover_text(how.join("\n"));
                }
            }
        }
        MessageContent::Payment(p) => {
            line(rect.top() + 4., "Payment".into(), theme::semibold(14.), text);
            let note = match p {
                langis_store::PaymentEvent::Notification { note: Some(n) } if !n.is_empty() => n.clone(),
                _ => p.description(sender, None),
            };
            line(rect.top() + 24., note, theme::regular(13.), secondary);
        }
        MessageContent::GiftBadge(g) => {
            line(rect.top() + 4., if outgoing { "Gift badge you sent".into() } else { "Gift badge".into() }, theme::semibold(14.), text);
            let state = match g.state {
                GiftBadgeState::Unopened if outgoing => "Sent",
                GiftBadgeState::Unopened => "Open it on your phone to redeem it",
                GiftBadgeState::Opened => "Opened",
                GiftBadgeState::Redeemed => "Redeemed",
                GiftBadgeState::Failed => "Couldn't be redeemed",
            };
            line(rect.top() + 24., state.into(), theme::regular(13.), secondary);
            let left = chrono::DateTime::from_timestamp_millis(g.expiration).map(|t| t.with_timezone(&chrono::Local).format("Expires %b %-d, %Y").to_string());
            if let Some(left) = left {
                line(rect.top() + 42., left, theme::regular(12.), secondary);
            }
        }
    }
    action
}

/// Desktop's `module-message__link-preview`: a full-width image on top when
/// it's large and not square (`shouldUseFullSizeLinkPreviewImage`: both
/// sides 200 or more), else a 72px thumbnail beside the text; the title in
/// up to 2 lines, the description in up to 5, then the domain and date.
struct PreviewCard {
    height: f32,
    full_image_h: f32,
    thumb: bool,
    title: Option<std::sync::Arc<egui::Galley>>,
    description: Option<std::sync::Arc<egui::Galley>>,
    footer: std::sync::Arc<egui::Galley>,
}

const PREVIEW_PAD: Vec2 = Vec2::new(12., 10.);
const PREVIEW_THUMB: f32 = 72.;

impl PreviewCard {
    fn layout(ui: &Ui, palette: &Palette, p: &langis_store::LinkPreview, width: f32) -> Self {
        let image = p.image.as_ref().filter(|i| i.content_type.starts_with("image/"));
        let full = image.is_some_and(|i| {
            let (w, h) = (i.width.unwrap_or(0) as f32, i.height.unwrap_or(0) as f32);
            w >= 200. && h >= 200. && (1. - w / h).abs() >= 0.05
        });
        let full_image_h = match image.filter(|_| full) {
            Some(i) => (width * i.height.unwrap_or(1) as f32 / i.width.unwrap_or(1).max(1) as f32).round().clamp(50., 450.),
            None => 0.,
        };
        let thumb = image.is_some() && !full;
        let text_w = (width - PREVIEW_PAD.x * 2. - if thumb { PREVIEW_THUMB + 8. } else { 0. }).max(40.);
        let (strong, secondary) = preview_colors(palette);
        let wrapped = |text: String, font: egui::FontId, color: Color32, rows: usize| {
            let mut job = egui::text::LayoutJob::simple(text, font, color, text_w);
            job.wrap.max_rows = rows;
            job.wrap.break_anywhere = false;
            job.wrap.overflow_character = Some('…');
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let title = p.title.as_ref().map(|t| wrapped(crate::format::one_line(t), theme::semibold(14.), strong, 2));
        let description = p.description.as_ref().map(|d| wrapped(crate::format::one_line(d), theme::regular(13.), strong, 5));
        let mut footer = preview_domain(&p.url);
        if let Some(date) = p.date.and_then(preview_date) {
            footer = format!("{footer} · {date}");
        }
        let footer = wrapped(footer, theme::regular(13.), secondary, 1);
        let text_h = title.as_ref().map(|g| g.size().y).unwrap_or(0.)
            + description.as_ref().map(|g| g.size().y + 4.).unwrap_or(0.)
            + footer.size().y + 2.;
        let content_h = text_h.max(if thumb { PREVIEW_THUMB } else { 0. }) + PREVIEW_PAD.y * 2.;
        Self { height: full_image_h + content_h, full_image_h, thumb, title, description, footer }
    }

    /// Draws it in `rect`, its top corners `top`; true when clicked.
    fn paint(&self, ui: &Ui, palette: &Palette, p: &langis_store::LinkPreview, rect: Rect, top: CornerRadius, id: egui::Id) -> bool {
        let response = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let fill = if palette.dark { theme::rgb(0x2e2e2e) } else { theme::rgb(0xf6f6f6) };
        let content = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + self.full_image_h), rect.max);
        let content_radius = if self.full_image_h > 0. { CornerRadius::ZERO } else { top };
        ui.painter().rect_filled(content, content_radius, fill);
        if let Some(image) = p.image.as_ref() {
            if self.full_image_h > 0. {
                let at = Rect::from_min_size(rect.min, Vec2::new(rect.width(), self.full_image_h));
                preview_image(ui, palette, image, at, top);
            } else if self.thumb {
                let at = Rect::from_min_size(Pos2::new(content.left() + PREVIEW_PAD.x, content.center().y - PREVIEW_THUMB / 2.), Vec2::splat(PREVIEW_THUMB));
                preview_image(ui, palette, image, at, CornerRadius::same(8));
            }
        }
        let text_h = self.title.as_ref().map(|g| g.size().y).unwrap_or(0.)
            + self.description.as_ref().map(|g| g.size().y + 4.).unwrap_or(0.)
            + self.footer.size().y + 2.;
        let left = content.left() + PREVIEW_PAD.x + if self.thumb { PREVIEW_THUMB + 8. } else { 0. };
        let mut y = content.center().y - text_h / 2.;
        let (strong, secondary) = preview_colors(palette);
        if let Some(g) = &self.title {
            ui.painter().galley(Pos2::new(left, y), g.clone(), strong);
            y += g.size().y;
        }
        if let Some(g) = &self.description {
            y += 4.;
            ui.painter().galley(Pos2::new(left, y), g.clone(), strong);
            y += g.size().y;
        }
        ui.painter().galley(Pos2::new(left, y + 2.), self.footer.clone(), secondary);
        response.clicked()
    }
}

/// gray-90 and gray-60 in light, gray-05 and gray-25 in dark.
fn preview_colors(palette: &Palette) -> (Color32, Color32) {
    if palette.dark { (theme::rgb(0xe9e9e9), theme::rgb(0xb9b9b9)) } else { (theme::rgb(0x1b1b1b), theme::rgb(0x5e5e5e)) }
}

/// A preview image, cropped to fill; a gray box until it's downloaded.
fn preview_image(ui: &Ui, palette: &Palette, image: &langis_store::AttachmentMeta, at: Rect, radius: CornerRadius) {
    if !crate::thumbs::visible(ui, at) {
        return;
    }
    let Some(path) = image.local_path.as_deref() else {
        let gray = if palette.dark { theme::rgb(0x3b3b3b) } else { theme::rgb(0xe9e9e9) };
        ui.painter().rect_filled(at, radius, gray);
        return;
    };
    let mut uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
    if let (Some(w), Some(h)) = (image.width.filter(|w| *w > 0), image.height.filter(|h| *h > 0)) {
        let (photo, frame) = (w as f32 / h as f32, at.width() / at.height());
        if photo > frame {
            let keep = frame / photo;
            uv = Rect::from_min_max(Pos2::new((1. - keep) / 2., 0.), Pos2::new((1. + keep) / 2., 1.));
        } else {
            let keep = photo / frame;
            uv = Rect::from_min_max(Pos2::new(0., (1. - keep) / 2.), Pos2::new(1., (1. + keep) / 2.));
        }
    }
    egui::Image::new(crate::thumbs::uri(ui.ctx(), path, at.size())).uv(uv).corner_radius(radius).paint_at(ui, at);
}

/// The link's host, lowercase (Desktop's `getDomain`).
fn preview_domain(url: &str) -> String {
    let host = url.split("://").nth(1).and_then(|r| r.split(['/', '?', '#']).next()).unwrap_or(url);
    let host = host.rsplit('@').next().unwrap_or(host);
    host.to_lowercase()
}

/// The page's date as Desktop's `LinkPreviewDate` writes it ("Nov 19, 2026"),
/// unless it's more than a day ahead (`isLinkPreviewDateValid`).
fn preview_date(millis: i64) -> Option<String> {
    let latest = chrono::Utc::now().timestamp_millis() + 86_400_000;
    if millis <= 0 || millis >= latest {
        return None;
    }
    let date = chrono::DateTime::from_timestamp_millis(millis)?.with_timezone(&chrono::Local);
    Some(date.format("%b %-d, %Y").to_string())
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
    // Over 3 kinds, Desktop shows 2 and a "+N" pill counting the rest.
    let more = (counts.len() > 3).then(|| counts[2..].iter().map(|c| c.1).sum::<usize>());
    counts.truncate(if more.is_some() { 2 } else { 3 });
    if let Some(rest) = more {
        counts.push((String::new(), rest, false));
    }
    let mut pills = Vec::new();
    for (emoji, count, mine) in &counts {
        let mut job = egui::text::LayoutJob::default();
        if emoji.is_empty() {
            job.append(&format!("+{count}"), 0., egui::TextFormat { font_id: theme::semibold(11.), color: palette.text, valign: egui::Align::Center, ..Default::default() });
        } else {
            append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::emoji(14.), color: palette.text, ..Default::default() });
        }
        if *count > 1 && !emoji.is_empty() {
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
fn typing_bubble(ui: &mut Ui, ctx: &Ctx<'_>, authors: &[String]) {
    let palette = ctx.palette;
    ui.add_space(10.);
    // Desktop's `TypingBubble`: one bubble for everyone, after the last 3
    // typists' avatars overlapping, and a +N for the rest.
    let shown: Vec<&String> = authors.iter().rev().take(3).rev().collect();
    let more = authors.len().saturating_sub(3);
    let step = 20.;
    let avatars = if ctx.group { shown.len() + (more > 0) as usize } else { 0 };
    let avatar_col = if avatars > 0 { 28. + (avatars - 1) as f32 * step + 8. } else { 0. };
    let (row, _) = ui.allocate_exact_size(Vec2::new(ctx.width, 36.), Sense::hover());
    let bubble = Rect::from_min_size(Pos2::new(row.left() + 16. + avatar_col, row.top()), Vec2::new(60., 36.));
    if ctx.group {
        let mut x = row.left() + 16.;
        for author in &shown {
            let name = ctx.app.name(author);
            let color = author.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
            let at = Rect::from_min_size(Pos2::new(x, row.top() + 8.), Vec2::splat(28.));
            ui.painter().circle_filled(at.center(), 15.5, palette.background);
            widgets::paint_avatar(ui, at, &widgets::AvatarSpec::person(&name, color));
            x += step;
        }
        if more > 0 {
            let at = Rect::from_min_size(Pos2::new(x, row.top() + 8.), Vec2::splat(28.));
            ui.painter().circle_filled(at.center(), 15.5, palette.background);
            ui.painter().circle_filled(at.center(), 14., palette.fill);
            ui.painter().text(at.center(), Align2::CENTER_CENTER, format!("+{more}"), theme::semibold(11.), palette.text);
        }
        let names: Vec<String> = authors.iter().map(|a| ctx.app.name(a)).collect();
        ui.interact(row, ui.scope_id().with("typing"), Sense::hover()).on_hover_text(format!("{} typing", names.join(", ")));
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

/// Desktop's `MessageBodyReadMore`: text past 800 characters (when there
/// are over 900) waits behind Read more, cut where no link is split.
pub const READ_MORE_LENGTH: usize = 800;
const READ_MORE: &str = "langis:read-more";

/// Where `body` is cut to show `limit` characters, if it is.
fn read_more_cut(body: &str, links: &[(std::ops::Range<usize>, String)], limit: usize) -> Option<usize> {
    if body.chars().count() <= limit.saturating_add(100) {
        return None;
    }
    let mut end = body.char_indices().nth(limit).map_or(body.len(), |(i, _)| i);
    if let Some((r, _)) = links.iter().find(|(r, _)| r.start < end && end < r.end) {
        end = r.end;
    }
    (end < body.len()).then_some(end)
}

pub fn rich_text(m: &Message, name: &dyn Fn(&str) -> String, color: Color32, link_color: Color32, revealed: bool, size: f32, limit: usize) -> Rich {
    use langis_store::TextStyle;
    let mention_fill = if color == Color32::WHITE { Color32::from_white_alpha(56) } else { Color32::from_black_alpha(20) };
    let full = m.long_text.as_ref().map(|_| m.body.as_str()).unwrap_or(&m.body);
    let links = find_links(full);
    let cut = read_more_cut(full, &links, limit);
    let body = &full[..cut.unwrap_or(full.len())];
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
    if cut.is_some() {
        let format = egui::TextFormat { font_id: theme::regular(size), color, line_height: Some(size * 1.43), ..Default::default() };
        job.append("... ", 0., format.clone());
        chars += 4;
        job.append("Read more", 0., egui::TextFormat { font_id: theme::semibold(size), ..format });
        out_links.push((chars..chars + 9, READ_MORE.to_owned()));
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

/// Links in a message, as Desktop's Linkify finds them (linkify-it): web
/// links with or without `https://`, bare domains like `example.com`, and
/// email addresses. Each comes with what opening it opens.
pub fn find_links(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut finder = linkify::LinkFinder::new();
    finder.url_must_have_scheme(false);
    finder
        .links(text)
        .filter_map(|link| {
            let found = link.as_str();
            let lower = found.to_ascii_lowercase();
            let url = match link.kind() {
                linkify::LinkKind::Email => format!("mailto:{found}"),
                linkify::LinkKind::Url if lower.starts_with("https://") || lower.starts_with("http://") => found.to_owned(),
                // Any other scheme (file:, javascript:) isn't a link.
                linkify::LinkKind::Url if lower.contains("://") || lower.starts_with("mailto:") => return None,
                linkify::LinkKind::Url if lower.starts_with("www.") => format!("https://{found}"),
                linkify::LinkKind::Url if bare_domain(&lower) => format!("http://{found}"),
                _ => return None,
            };
            Some((link.start()..link.end(), url))
        })
        .collect()
}

/// Whether a link written without a scheme ends its host in a real
/// top-level domain, as linkify-it's list requires: "example.com" is a
/// link, "notes.txt" isn't.
fn bare_domain(link: &str) -> bool {
    const COMMON: &[&str] = &[
        "com", "org", "net", "edu", "gov", "mil", "int", "info", "biz", "name", "pro", "app", "dev", "io", "ai", "co", "me", "tv",
        "xyz", "online", "site", "store", "tech", "blog", "news", "shop", "cloud", "page", "art", "link", "live", "life", "space",
        "website", "club", "design", "email", "media", "social", "world", "today", "zone", "rocks", "wiki", "eu", "asia",
    ];
    let host = link.split(['/', '?', '#', ':']).next().unwrap_or(link);
    let Some(tld) = host.rsplit('.').next().filter(|_| host.contains('.')) else { return false };
    // Two letters is a country (.uk, .de); a file's extension like .txt
    // or .md... .md is Moldova, as linkify-it also has it.
    (tld.len() == 2 && tld.chars().all(|c| c.is_ascii_alphabetic())) || COMMON.contains(&tld)
}

/// Opens a link from a message: Signal's own links in the app, as Desktop
/// routes them (`ts/util/signalRoutes`), anything else in the browser.
pub fn open_link(app: &mut App, url: &str) {
    use langis_core::routes::Route;
    match langis_core::routes::parse(url) {
        Some(Route::GroupInvite(_)) => app.send(Command::OpenGroupLink { url: url.to_owned() }),
        Some(Route::LinkCall(_)) => app.send(Command::OpenCallLink { url: url.to_owned() }),
        Some(Route::AddStickers { .. }) => match langis_core::stickers::parse_pack_link(url) {
            Some((id, key)) => {
                app.send(Command::PreviewStickerPack { id: id.clone(), key: key.clone() });
                app.fun.preview = Some((id, key, None));
            }
            None => app.toast("This sticker pack link isn't valid."),
        },
        // signal.me: looked up as New chat does, which offers to message them.
        Some(Route::ContactByPhoneNumber(_) | Route::ContactByEncryptedUsername(_)) => {
            app.send(Command::FindRecipient { query: url.to_owned() });
            app.panes.compose = Some(crate::panes::Compose::Chat { query: url.to_owned(), searching: true, found: None });
        }
        _ => open_externally(url),
    }
}

/// How much of a disappearing message's time is left (1 to 0), or `None`
/// for one that doesn't disappear. Its countdown starts once sent or read.
pub fn expiry_left(m: &Message) -> Option<f32> {
    if m.expires_in == 0 {
        return None;
    }
    let Some(start) = m.expire_start else { return Some(1.) };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(start);
    let total = i64::from(m.expires_in) * 1000;
    Some((1. - (now - start) as f32 / total as f32).clamp(0., 1.))
}

#[cfg(test)]
mod tests {
    use super::{find_links, read_more_cut};

    #[test]
    fn read_more_cuts_long_text_without_splitting_links() {
        let short = "a".repeat(900);
        assert_eq!(read_more_cut(&short, &find_links(&short), 800), None);
        let long = "a".repeat(901);
        assert_eq!(read_more_cut(&long, &find_links(&long), 800), Some(800));
        let linked = format!("{} https://example.org/{} {}", "a".repeat(790), "b".repeat(40), "c".repeat(200));
        let links = find_links(&linked);
        let end = read_more_cut(&linked, &links, 800).unwrap();
        assert_eq!(end, links[0].0.end);
    }

    #[test]
    fn only_web_links() {
        let links = find_links("see https://example.org/a, www.example.com. not file:///etc/passwd");
        let urls: Vec<_> = links.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(urls, ["https://example.org/a", "https://www.example.com"]);
    }

    #[test]
    fn bare_domains_and_emails_are_links_but_file_names_arent() {
        let text = "try signal.org/blog or mail maya@example.org, not notes.txt";
        let links = find_links(text);
        let urls: Vec<_> = links.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(urls, ["http://signal.org/blog", "mailto:maya@example.org"]);
        assert_eq!(&text[links[0].0.clone()], "signal.org/blog");
    }
}

/// A message in select mode: the whole row toggles it, with Desktop's
/// checkbox (`module-message__select-checkbox`) and tint.
fn select_row(ui: &mut Ui, palette: &Palette, area: Rect, id: i64, selected: &mut Vec<i64>) {
    let on = selected.contains(&id);
    let response = ui.interact(area, egui::Id::unique("select-row").with(id), Sense::click());
    if on {
        // `rgba($color-ultramarine, 8%)`.
        ui.painter().rect_filled(area, 0, Color32::from_rgba_unmultiplied(0x2c, 0x6b, 0xed, 20));
    }
    let center = Pos2::new(area.left() + 16. + 10., area.center().y);
    if on {
        ui.painter().circle_filled(center, 10., theme::rgb(0x2c6bed));
        theme::paint_symbol(ui.painter(), "check", center, 14., Color32::WHITE);
    } else {
        ui.painter().circle_stroke(center, 9.5, egui::Stroke::new(1., if palette.dark { theme::rgb(0x5e5e5e) } else { theme::rgb(0xdedede) }));
    }
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        if on {
            selected.retain(|x| *x != id);
        } else {
            selected.push(id);
        }
    }
}

/// Desktop's `SelectModeActions`, in the composer's place: exit, the
/// count, delete and forward.
fn select_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    let chosen: Vec<i64> = chat.selected.clone().unwrap_or_default();
    egui::Panel::bottom("select-mode")
        .exact_size(54.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let icon = if palette.dark { theme::rgb(0xe9e9e9) } else { theme::rgb(0x3b3b3b) };
            let disabled = if palette.dark { theme::rgb(0x5e5e5e) } else { theme::rgb(0xb9b9b9) };
            let button = |ui: &mut Ui, x: f32, symbol: &str, label: &str, enabled: bool| {
                let r = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(44.));
                let response = ui.interact(r, egui::Id::unique("select-bar").with(label), Sense::click());
                theme::paint_symbol(ui.painter(), symbol, r.center(), 20., if enabled { icon } else { disabled });
                response.on_hover_text(label).clicked() && enabled
            };
            if button(ui, rect.left() + 4. + 22., "x", "Exit select mode", true) {
                chat.selected = None;
            }
            let forwardable = !chosen.is_empty()
                && chosen.iter().all(|id| chat.messages.iter().find(|m| m.id == *id).is_some_and(crate::actions::can_forward));
            if button(ui, rect.right() - 4. - 22., "forward", "Forward selected messages", forwardable) {
                if chosen.len() > 30 {
                    app.toast("You can only forward up to 30 messages");
                } else {
                    chat.dialog = Some(crate::actions::Dialog::Forward { messages: chosen.clone(), query: String::new(), chosen: Vec::new() });
                }
            }
            if button(ui, rect.right() - 4. - 44. - 22., "trash", "Delete selected messages", !chosen.is_empty()) {
                chat.dialog = Some(crate::actions::Dialog::Delete { messages: chosen.clone() });
            }
            let text = format!("{} selected", chosen.len());
            ui.painter().text(Pos2::new(rect.left() + 4. + 44. + 10., rect.center().y), egui::Align2::LEFT_CENTER, text, theme::regular(14.), if palette.dark { theme::rgb(0xbebebe) } else { theme::rgb(0x5e5e5e) });
        });
}

/// Desktop's `HeaderDropdownMenuContent` for an accepted chat.
fn header_menu(app: &mut App, ui: &Ui, more: &egui::Response, c: &langis_store::Conversation) {
    use langis_core::ChatChange;
    let id = c.id.clone();
    let group = c.kind == ConversationKind::Group;
    crate::menu::dropdown(ui, more, egui::Id::unique("chat-header-menu"), |m| {
        if !group || c.can_edit_info {
            m.sub(Some("timer"), "Disappearing messages", |m| {
                // expirationTimer.DEFAULT_DURATIONS_IN_SECONDS
                const DEFAULTS: [u32; 8] = [0, 2_419_200, 604_800, 86_400, 28_800, 3_600, 300, 30];
                for seconds in DEFAULTS {
                    let label = if seconds == 0 { "Off".to_owned() } else { crate::format::timer_long(seconds) };
                    if m.check(c.expire_timer == seconds, &label) {
                        app.send(Command::SetExpireTimer { conversation: id.clone(), seconds });
                    }
                }
                let custom = !DEFAULTS.contains(&c.expire_timer);
                let label = if custom { format!("Custom time ({})", crate::format::timer_long(c.expire_timer)) } else { "Custom time...".to_owned() };
                if m.check(custom, &label) {
                    let (value, unit) = crate::panes::custom_timer_start(if custom { c.expire_timer } else { 3_600 });
                    app.panes.custom_timer = Some((id.clone(), value, unit));
                }
            });
        }
        if c.muted {
            if m.item(Some("bell"), "Unmute") {
                app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::MutedUntil(0) });
            }
        } else {
            m.sub(Some("bell-slash"), "Mute notifications", |m| crate::panes::mute_items(app, m, &id));
        }
        if m.item(Some("settings"), if group { "Group settings" } else { "Chat settings" }) {
            crate::details::open(app, id.clone());
        }
        if m.item(Some("album"), "All media") {
            crate::details::open_media(app, id.clone());
        }
        m.separator();
        if m.item(Some("check-circle"), "Select messages") {
            app.select_requested = true;
        }
        m.separator();
        if !c.marked_unread && m.item(Some("message-badge"), "Mark as unread") {
            app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::MarkedUnread(true) });
        }
        if m.item(Some(if c.pinned { "pin-slash" } else { "pin" }), if c.pinned { "Unpin chat" } else { "Pin chat" }) {
            app.send(Command::SetPinned { conversation: id.clone(), pinned: !c.pinned });
        }
        if m.item(Some(if c.archived { "archive-up" } else { "archive" }), if c.archived { "Unarchive" } else { "Archive" }) {
            app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::Archived(!c.archived) });
        }
        if !c.ended {
            if !c.blocked && m.item(Some("block"), "Block") {
                app.panes.chat_confirm = Some((id.clone(), crate::panes::ChatConfirm::Block));
            }
            if c.blocked && m.item(Some("message-thread"), "Unblock") {
                app.panes.chat_confirm = Some((id.clone(), crate::panes::ChatConfirm::Unblock));
            }
        }
        if m.item(Some("trash"), "Delete") {
            let member = group && !c.left && !c.ended;
            app.panes.delete_chat = Some((id.clone(), member));
        }
        if group && !c.ended && !c.left && m.item(Some("leave"), "Leave group") {
            app.panes.chat_confirm = Some((id.clone(), crate::panes::ChatConfirm::Leave));
        }
    });
}

