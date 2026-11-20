//! What can be done to one message: Desktop's hover bar (react, reply,
//! more), its context menu (`MessageContextMenu`), the reaction bar and
//! picker, and the dialogs those open (delete, pin, forward, info).

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::Command;
use langis_store::{ConversationId, ConversationKind, Direction, Message, SendState, SendStatus};

use crate::app::{App, Loaded};
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

/// Desktop's reaction bar when the account has none saved
/// (`DEFAULT_PREFERRED_REACTION_EMOJI`).
pub const DEFAULT_REACTIONS: [&str; 6] = ["❤️", "👍", "👎", "😂", "😮", "😢"];

/// A menu or bar floating over the timeline.
pub enum Popup {
    /// The context menu, opened by right-click or the hover bar's "more".
    Menu { message: i64, at: Pos2 },
    /// The reaction bar; `full` when its "+" opened the whole picker.
    Reactions { message: i64, at: Pos2, full: bool, picker: crate::emoji_picker::Picker },
}

/// A dialog over the chat.
pub enum Dialog {
    /// Desktop's ContactDetail: a shared contact in full, as a panel.
    ContactDetail { message: i64 },
    /// Desktop's `DeleteMessagesModal`, for one message or a selection.
    Delete { messages: Vec<i64> },
    Pin { message: i64, choice: usize },
    /// Desktop's `ForwardMessagesModal`: choose chats, then (for one
    /// message) its text to edit before sending.
    Forward { messages: Vec<i64>, query: String, chosen: Vec<ConversationId>, editing: Option<String> },
    Info { message: i64, details: Option<Details> },
    Poll(crate::polls::NewPoll),
    PollVotes { message: i64 },
    /// Desktop's `EditHistoryMessagesModal`: every version, newest first.
    EditHistory { message: i64, details: Option<Details> },
    /// Desktop's `PinnedMessagesPanel`: every pinned message.
    Pinned { messages: Vec<Message> },
    /// A notice's Learn more: what it means.
    Explain { title: &'static str, body: &'static str },
    /// Desktop's `ShareContactDialog`: pick someone, then what of theirs
    /// to share.
    ShareContact(ShareStep),
    /// Desktop's `ReactionViewer`: who reacted, all or by emoji; your
    /// own row removes yours.
    ReactionViewer { message: i64, tab: Option<String> },
    /// Desktop's `CustomizingPreferredReactionsModal`: the reaction bar's
    /// six, one picked to change from the emoji picker.
    Reactions { slots: Vec<String>, selected: Option<usize>, picker: crate::emoji_picker::Picker },
}

pub enum ShareStep {
    /// "Select contact": who can be shared, once loaded.
    Pick { query: String, candidates: Option<Vec<langis_core::share_contact::ShareCandidate>> },
    /// Loading the picked person's details.
    Loading,
    /// "Share contact": the card and the optional details.
    Share { draft: langis_core::share_contact::ShareContactDraft, selection: langis_core::share_contact::ShareContactSelection },
}

/// Opens the share contact dialog and loads who can be shared.
pub fn open_share_contact(app: &App, chat: &mut crate::chat::ChatView) {
    chat.dialog = Some(Dialog::ShareContact(ShareStep::Pick { query: String::new(), candidates: None }));
    let recipient = chat.id.clone();
    app.load(move |db| async move {
        let list = langis_core::share_contact::candidates(&db, &recipient).await;
        Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(Dialog::ShareContact(ShareStep::Pick { candidates, .. })) = app.chat.as_mut().and_then(|c| c.dialog.as_mut()) {
                *candidates = Some(list);
            }
        })))
    });
}

/// Desktop's `MessageDetail`: each recipient's state and the edits.
pub struct Details {
    pub states: Vec<SendState>,
    pub edits: Vec<langis_store::EditRevision>,
}

/// Desktop's pin durations (`PinMessageDialog`), in seconds.
const PIN_DURATIONS: [(&str, Option<u32>); 4] =
    [("24 hours", Some(24 * 3600)), ("7 days", Some(7 * 24 * 3600)), ("30 days", Some(30 * 24 * 3600)), ("Forever", None)];

/// What a popup or dialog asked for, applied by the chat.
pub enum Do {
    Reply(i64),
    Edit(i64),
    Command(Command),
    Copy(String),
    Save(i64, usize),
    /// The reaction bar for this message, at the menu's place.
    ReactBar(i64),
    Open(Dialog),
    /// Opens a chat (a shared contact's Send message).
    OpenChat(langis_store::ConversationId),
    /// Select mode, starting with this message.
    Select(i64),
    /// Scrolls to a message, loading around it if it isn't here.
    Jump(i64),
    Close,
}

/// A floating box at `at`, kept on screen. Closes on Escape or a click
/// outside it (other than on `keep`).
pub fn popover(ctx: &egui::Context, id: &str, at: Pos2, width: f32, keep: Option<Rect>, body: impl FnOnce(&mut Ui)) -> bool {
    let palette = Palette::of(ctx);
    let screen = ctx.content_rect();
    let area = egui::Area::new(egui::Id::unique(("popover", id)))
        .order(egui::Order::Foreground)
        .constrain_to(screen)
        .fixed_pos(at)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette.popover)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1., palette.border))
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(50) })
                .inner_margin(6)
                .show(ui, |ui| {
                    ui.set_width(width);
                    body(ui);
                });
        });
    let rect = area.response.rect;
    let outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !rect.contains(p) && !keep.is_some_and(|k| k.contains(p)));
    outside || ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

/// The hover bar beside a bubble: react, reply, more. Returns which was
/// clicked (0 react, 1 reply, 2 more) and where.
pub fn hover_bar(ui: &Ui, palette: &Palette, id: i64, bubble: Rect, outgoing: bool) -> Option<(u8, Pos2)> {
    let w = 30.;
    let bx = if outgoing { bubble.left() - 3. * w - 6. } else { bubble.right() + 6. };
    let mut clicked = None;
    let buttons: [(Icon, &str); 3] = if outgoing {
        [(Icon::More, "More actions"), (Icon::Reply, "Reply"), (Icon::Emoji, "React")]
    } else {
        [(Icon::Emoji, "React"), (Icon::Reply, "Reply"), (Icon::More, "More actions")]
    };
    for (i, (icon, tip)) in buttons.into_iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(bx + i as f32 * w, bubble.center().y - 14.), Vec2::splat(28.));
        let r = ui.interact(rect, ui.scope_id().with(("hover-action", id, i)), Sense::click()).on_hover_text(tip);
        if r.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(14), palette.fill);
        }
        icon.paint(ui, rect, 18., palette.text_secondary);
        if r.clicked() {
            let what = match icon {
                Icon::Emoji => 0,
                Icon::Reply => 1,
                _ => 2,
            };
            clicked = Some((what, rect.left_bottom() + Vec2::new(0., 4.)));
        }
    }
    clicked
}

/// What the menu offers for `m` (Desktop's `MessageContextMenu` and
/// `TimelineMessage`'s rules).
struct Offers {
    download: Option<usize>,
    reply: bool,
    react: bool,
    edit: bool,
    copy: bool,
    forward: bool,
    pin: Option<bool>,
    retry_delete: bool,
    delete: bool,
}

fn offers(app: &App, m: &Message, c: &langis_store::Conversation, pinned: bool) -> Offers {
    let now = chrono::Utc::now().timestamp_millis();
    let note_to_self = c.kind == ConversationKind::NoteToSelf;
    let can_send = !c.left && !c.blocked && !c.ended;
    let usable = !m.deleted && m.view_once.is_none();
    let failed_delete = m.deleted && m.direction == Direction::Outgoing && m.status == langis_store::DeliveryStatus::Failed;
    let _ = app;
    Offers {
        download: m.attachments.iter().position(|a| !a.sticker).filter(|_| usable),
        reply: usable && can_send,
        react: !m.deleted && can_send,
        edit: can_send && langis_core::edits::can_edit(m, note_to_self, now),
        copy: usable && !m.body.is_empty() && m.poll.is_none(),
        forward: can_forward(m),
        pin: (usable && can_send && m.notice.is_none()).then_some(pinned),
        retry_delete: failed_delete,
        delete: true,
    }
}

/// A sticker's pack id, key and sticker id, from the hex its message keeps.
fn sticker_ref(pack: &langis_store::StickerPackRef) -> Option<(Vec<u8>, Vec<u8>, u32)> {
    let hex = |s: &str| -> Option<Vec<u8>> {
        (s.len() % 2 == 0).then_some(())?;
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
    };
    Some((hex(&pack.id)?, hex(&pack.key)?, pack.sticker_id?))
}

/// Whether langis will forward `m` (its rule in `Command::Forward`, after
/// Desktop's `canForward`): not deleted, a poll, view-once or a notice, and
/// every attachment downloaded and not a sticker.
pub fn can_forward(m: &Message) -> bool {
    !m.deleted
        && m.view_once.is_none()
        && m.poll.is_none()
        && m.notice.is_none()
        && m.attachments.iter().all(|a| a.local_path.is_some() && !a.sticker)
        && (!m.body.is_empty() || !m.attachments.is_empty())
}

/// Draws the open popup, if any; returns what it asked for.
pub fn show_popup(app: &App, ctx: &egui::Context, popup: &mut Popup, chat: &[Message], pinned: &[i64], c: &langis_store::Conversation) -> Vec<Do> {
    let mut out = Vec::new();
    match popup {
        Popup::Menu { message, at } => {
            let Some(m) = chat.iter().find(|m| m.id == *message) else { return vec![Do::Close] };
            let offer = offers(app, m, c, pinned.contains(message));
            let id = *message;
            let failed_send = m.direction == Direction::Outgoing && !m.deleted && m.status == langis_store::DeliveryStatus::Failed;
            let menu = egui::Id::unique("message-menu").with((id, at.x.to_bits(), at.y.to_bits()));
            if !crate::menu::is_open(ctx, menu) && !ctx.data(|d| d.get_temp::<bool>(menu.with("shown")).unwrap_or(false)) {
                crate::menu::open_at(ctx, menu, *at);
                ctx.data_mut(|d| d.insert_temp(menu.with("shown"), true));
            }
            // Desktop's `MessageContextMenu`, in its order.
            let open = crate::menu::show(ctx, menu, |ui| {
                if let Some(index) = offer.download
                    && ui.item(Some("download"), "Download")
                {
                    out.push(Do::Save(id, index));
                }
                if offer.reply && ui.item(Some("reply"), "Reply") {
                    out.push(Do::Reply(id));
                }
                // Desktop's sticker favorite, from the sticker's pack.
                if let Some((pack_id, pack_key, sticker_id)) = m.attachments.iter().find(|a| a.sticker).and_then(|a| a.sticker_pack.as_ref()).and_then(sticker_ref).filter(|_| !m.deleted) {
                    let favorite = app.fun.favorite_ids.iter().any(|(p, s)| *p == pack_id && *s == sticker_id);
                    let (symbol, label) = if favorite { ("star-slash", "Remove from Favorites") } else { ("star", "Add to Favorites") };
                    if ui.item(Some(symbol), label) {
                        out.push(Do::Command(Command::SetStickerFavorite { pack_id, pack_key, sticker_id, favorite: !favorite }));
                    }
                }
                if offer.react && ui.item(Some("heart-plus"), "React") {
                    out.push(Do::ReactBar(id));
                }
                if m.direction == Direction::Outgoing && m.poll.as_ref().is_some_and(|p| !p.ended) && !m.deleted && ui.item(Some("stop-circle"), "End poll") {
                    out.push(Do::Command(Command::EndPoll { message_id: id }));
                }
                if offer.forward && ui.item(Some("forward"), "Forward") {
                    out.push(Do::Open(Dialog::Forward { messages: vec![id], query: String::new(), chosen: Vec::new(), editing: None }));
                }
                if offer.edit && ui.item(Some("pencil"), "Edit") {
                    out.push(Do::Edit(id));
                }
                if !m.deleted && ui.item(Some("check-circle"), "Select") {
                    out.push(Do::Select(id));
                }
                if offer.copy && ui.item(Some("copy"), "Copy text") {
                    out.push(Do::Copy(plain_text(app, m)));
                }
                match offer.pin {
                    Some(true) if ui.item(Some("pin-slash"), "Unpin") => out.push(Do::Command(Command::UnpinMessage { message_id: id })),
                    Some(false) if ui.item(Some("pin"), "Pin") => out.push(Do::Open(Dialog::Pin { message: id, choice: 0 })),
                    _ => {}
                }
                if ui.item(Some("info-circle"), "Info") {
                    out.push(Do::Open(Dialog::Info { message: id, details: None }));
                }
                if offer.delete && ui.item(Some("trash"), "Delete") {
                    out.push(Do::Open(Dialog::Delete { messages: vec![id] }));
                }
                if failed_send && ui.item(Some("send"), "Retry Send") {
                    out.push(Do::Command(Command::RetrySend { message_id: id }));
                }
                if offer.retry_delete && ui.item(Some("trash"), "Retry Delete for Everyone") {
                    out.push(Do::Command(Command::RetryDeleteForEveryone { message_id: id }));
                }
            });
            if !open || !out.is_empty() {
                crate::menu::close(ctx, menu);
                ctx.data_mut(|d| d.remove::<bool>(menu.with("shown")));
                out.push(Do::Close);
            }
        }
        Popup::Reactions { message, at, full, picker } => {
            let Some(m) = chat.iter().find(|m| m.id == *message) else { return vec![Do::Close] };
            let mine = m.reactions.iter().find(|r| r.author.is_none()).map(|r| r.emoji.clone());
            let id = *message;
            let react = |emoji: &str| {
                let remove = mine.as_deref() == Some(emoji);
                Command::React { message_id: id, emoji: emoji.to_owned(), remove }
            };
            if *full {
                let mut open = true;
                let anchor = *at + Vec2::new(0., 420.);
                // Desktop's reaction picker ends with Customize.
                let palette = Palette::of(ctx);
                let mut customize = false;
                egui::Area::new(egui::Id::unique("customize-reactions"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(Pos2::new(anchor.x, anchor.y - 4.))
                    .show(ctx, |ui| {
                        egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).stroke(egui::Stroke::new(1., palette.border)).inner_margin(6).show(ui, |ui| {
                            customize = widgets::pill_button(ui, "Customize reactions", false).clicked();
                        });
                    });
                if customize {
                    out.push(Do::Close);
                    out.push(Do::Open(Dialog::Reactions { slots: app.reactions.clone(), selected: Some(0), picker: Default::default() }));
                } else if let Some(picked) = picker.show(ctx, anchor, &mut open, &app.recent_emoji) {
                    app.use_emoji(picked);
                    out.push(Do::Command(react(picked)));
                    out.push(Do::Close);
                } else if !open && !ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.y > anchor.y - 8.) {
                    out.push(Do::Close);
                }
            } else {
                let palette = Palette::of(ctx);
                let mut want_full = false;
                let closed = popover(ctx, "reaction-bar", *at, 7. * 40., None, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        for emoji in &app.reactions {
                            let (rect, r) = ui.allocate_exact_size(Vec2::splat(36.), Sense::click());
                            if mine.as_deref() == Some(emoji.as_str()) {
                                ui.painter().circle_filled(rect.center(), 18., palette.fill_pressed);
                            } else if r.hovered() {
                                ui.painter().circle_filled(rect.center(), 18., palette.fill);
                            }
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, emoji, theme::emoji(24.), palette.text);
                            if r.clicked() {
                                out.push(Do::Command(react(emoji)));
                            }
                        }
                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(36.), Sense::click());
                        ui.painter().circle_filled(rect.center(), 18., if r.hovered() { palette.fill_pressed } else { palette.fill });
                        Icon::Plus.paint(ui, rect, 20., palette.text);
                        if r.on_hover_text("More reactions").clicked() {
                            want_full = true;
                        }
                    });
                });
                if want_full {
                    *full = true;
                } else if closed || !out.is_empty() {
                    out.push(Do::Close);
                }
            }
        }
    }
    out
}

/// The message's text as a person reads it: mentions by name.
pub fn plain_text(app: &App, m: &Message) -> String {
    let rich = crate::chat::rich_text(m, &|sid| app.name(sid), Color32::WHITE, Color32::WHITE, true, 14., usize::MAX);
    rich.job.text
}

/// Draws the open dialog; returns what it asked for.
pub fn show_dialog(app: &App, ui: &Ui, dialog: &mut Dialog, chat: &[Message], c: &langis_store::Conversation) -> Vec<Do> {
    let palette = Palette::of(ui.ctx());
    let mut out = Vec::new();
    let frame = egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24);
    match dialog {
        Dialog::Delete { messages } => {
            let chosen: Vec<&Message> = messages.iter().filter_map(|id| chat.iter().find(|m| m.id == *id)).collect();
            if chosen.is_empty() {
                return vec![Do::Close];
            }
            let note_to_self = c.kind == ConversationKind::NoteToSelf;
            let now = chrono::Utc::now().timestamp_millis();
            // Desktop's `canDeleteForEveryone`: our own messages while
            // allowed, or anyone's as a group admin when the remote config
            // allows admin deletes (langis sends the right kind).
            let group = c.kind == ConversationKind::Group;
            let admin = group && !c.left && c.admins.iter().any(|a| *a == app.aci);
            let rules = langis_core::deletes::Chat::new(note_to_self, group, admin, c.ended, &langis_core::remote_config::current());
            let for_everyone = chosen.iter().all(|m| langis_core::deletes::send_kind(m, rules, now).is_some());
            let n = chosen.len();
            let title = if n == 1 { "Delete selected message?".to_owned() } else { format!("Delete selected {n} messages?") };
            let body = match (note_to_self, n) {
                (true, 1) => "This message will be deleted from all your devices.",
                (true, _) => "These messages will be deleted from all your devices.",
                (false, 1) => "Who would you like to delete this message for?",
                (false, _) => "Who would you like to delete these messages for?",
            };
            let mut actions = vec![(if note_to_self { "Delete" } else { "Delete for me" }, crate::dialog::Variant::SubtleDestructive)];
            if for_everyone && !note_to_self {
                actions.push(("Delete for everyone", crate::dialog::Variant::SubtleDestructive));
            }
            let ids: Vec<i64> = chosen.iter().map(|m| m.id).collect();
            match crate::dialog::show(ui.ctx(), egui::Id::unique("delete-messages"), &title, body, Some("Cancel"), &actions) {
                Some(crate::dialog::Answer::Action(0)) => out.push(Do::Command(Command::DeleteForMe { message_ids: ids })),
                Some(crate::dialog::Answer::Action(_)) => {
                    for message_id in ids {
                        out.push(Do::Command(Command::DeleteForEveryone { message_id }));
                    }
                }
                Some(crate::dialog::Answer::Cancel) => {}
                None => return out,
            }
            out.push(Do::Close);
        }
        Dialog::Pin { message, choice } => {
            let id = *message;
            let modal = egui::Modal::new(egui::Id::unique("pin-message")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(320.);
                ui.label(RichText::new("Pin message for...").font(theme::semibold(18.)));
                ui.add_space(12.);
                for (i, (label, _)) in PIN_DURATIONS.iter().enumerate() {
                    if radio_row(ui, *choice == i, label) {
                        *choice = i;
                    }
                }
                ui.add_space(16.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, "Pin", true).clicked() {
                        out.push(Do::Command(Command::PinMessage { message_id: id, duration: PIN_DURATIONS[*choice].1 }));
                    }
                    if widgets::pill_button(ui, "Cancel", false).clicked() {
                        out.push(Do::Close);
                    }
                });
            });
            if modal.should_close() || !out.is_empty() {
                out.push(Do::Close);
            }
        }
        Dialog::Forward { messages, query, chosen, editing } => {
            let mut send = None;
            let modal = egui::Modal::new(egui::Id::unique("forward-messages")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(380.);
                ui.label(RichText::new("Forward To").font(theme::semibold(18.)));
                ui.add_space(10.);
                if let Some(text) = editing {
                    // Desktop's `ForwardMessageEditor`: the message as it
                    // will go, its text editable, then Send.
                    let original = chat.iter().find(|m| Some(m.id) == messages.first().copied());
                    if let Some(m) = original {
                        let files = m.attachments.iter().filter(|a| !a.sticker).count();
                        if files > 0 {
                            let thumb = m.attachments.iter().find_map(|a| a.thumbnail_path.clone().or_else(|| a.local_path.clone().filter(|_| a.content_type.starts_with("image/"))));
                            ui.horizontal(|ui| {
                                if let Some(path) = thumb {
                                    let (at, _) = ui.allocate_exact_size(Vec2::splat(56.), Sense::hover());
                                    crate::chat::cover_image(ui, &path, at, CornerRadius::same(8));
                                }
                                let what = m.attachments.first().map(langis_store::attachment_preview).unwrap_or_default();
                                ui.label(RichText::new(if files > 1 { format!("{what} and {} more", files - 1) } else { what.to_owned() }).color(palette.text_secondary));
                            });
                            ui.add_space(8.);
                        }
                    }
                    egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(18)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).hint_text("Add a message").frame(egui::Frame::NONE).desired_rows(2).desired_width(f32::INFINITY).font(theme::regular(14.)));
                    });
                } else {
                    widgets::text_field(ui, query, "Search", false);
                    ui.add_space(8.);
                    let q = query.trim().to_lowercase();
                    egui::ScrollArea::vertical().max_height(320.).auto_shrink([false, true]).show(ui, |ui| {
                        for conv in app.conversations.iter().filter(|x| !x.left && !x.blocked && !x.ended && (q.is_empty() || x.title.to_lowercase().contains(&q))) {
                            let on = chosen.contains(&conv.id);
                            let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.), Sense::click());
                            if r.hovered() {
                                ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                            }
                            widgets::paint_avatar(ui, Rect::from_min_size(rect.min + Vec2::new(8., 8.), Vec2::splat(32.)), &widgets::AvatarSpec::of(conv));
                            let mut job = egui::text::LayoutJob::simple_singleline(conv.title.clone(), theme::medium(14.), palette.text);
                            job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 90.);
                            let g = ui.fonts_mut(|f| f.layout_job(job));
                            ui.painter().galley(Pos2::new(rect.left() + 50., rect.center().y - g.size().y / 2.), g, palette.text);
                            check_circle(ui, Pos2::new(rect.right() - 20., rect.center().y), on, &palette);
                            if r.clicked() {
                                if on {
                                    chosen.retain(|x| x != &conv.id);
                                } else {
                                    chosen.push(conv.id.clone());
                                }
                            }
                        }
                    });
                    // The chosen chats as pills (Desktop's `ContactPills`).
                    if !chosen.is_empty() {
                        ui.add_space(8.);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(4.);
                            let mut remove = None;
                            for id in chosen.iter() {
                                let Some(conv) = app.conversation(id) else { continue };
                                let g = ui.painter().layout_no_wrap(conv.title.clone(), theme::regular(13.), palette.text);
                                let (pill, r) = ui.allocate_exact_size(Vec2::new(4. + 20. + 6. + g.size().x + 6. + 14. + 6., 28.), Sense::click());
                                ui.painter().rect_filled(pill, CornerRadius::same(14), palette.fill_secondary);
                                widgets::paint_avatar(ui, Rect::from_min_size(pill.min + Vec2::new(4., 4.), Vec2::splat(20.)), &widgets::AvatarSpec::of(conv));
                                ui.painter().galley(Pos2::new(pill.left() + 30., pill.center().y - g.size().y / 2.), g, palette.text);
                                theme::paint_symbol(ui.painter(), "x", Pos2::new(pill.right() - 13., pill.center().y), 12., palette.text_secondary);
                                if r.clicked() {
                                    remove = Some(id.clone());
                                }
                            }
                            if let Some(id) = remove {
                                chosen.retain(|x| *x != id);
                            }
                        });
                    }
                }
                ui.add_space(12.);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 32.), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let single = messages.len() == 1;
                    match editing {
                        Some(text) => {
                            if widgets::pill_button(ui, "Send", true).clicked() {
                                send = Some(Some(text.clone()));
                            }
                        }
                        None if single => {
                            if ui.add_enabled_ui(!chosen.is_empty(), |ui| widgets::pill_button(ui, "Continue", true)).inner.clicked() {
                                let body = chat.iter().find(|m| m.id == messages[0]).map(|m| m.body.clone()).unwrap_or_default();
                                *editing = Some(body);
                            }
                        }
                        None => {
                            if ui.add_enabled_ui(!chosen.is_empty(), |ui| widgets::pill_button(ui, "Send", true)).inner.clicked() {
                                send = Some(None);
                            }
                        }
                    }
                    if widgets::pill_button(ui, "Cancel", false).clicked() {
                        out.push(Do::Close);
                    }
                });
            });
            if let Some(body) = send {
                // The text as it was goes unchanged (keeping its formatting).
                let original = chat.iter().find(|m| Some(m.id) == messages.first().copied()).map(|m| m.body.clone());
                let body = body.filter(|b| Some(b) != original.as_ref());
                out.push(Do::Command(Command::Forward { message_ids: messages.clone(), conversations: chosen.clone(), body }));
            }
            if modal.should_close() || !out.is_empty() {
                out.push(Do::Close);
            }
        }
        Dialog::PollVotes { message } => {
            let Some(m) = chat.iter().find(|m| m.id == *message) else { return vec![Do::Close] };
            let Some(poll) = m.poll.clone() else { return vec![Do::Close] };
            let ours = m.direction == langis_store::Direction::Outgoing;
            let id = m.id;
            let mut end = false;
            let closed = crate::panes::modal(ui, "poll-votes", 400., |ui| {
                end = crate::polls::votes_dialog(app, ui, &palette, &poll, ours);
            });
            if end {
                out.push(Do::Command(Command::EndPoll { message_id: id }));
                out.push(Do::Close);
            }
            if closed {
                out.push(Do::Close);
            }
        }
        Dialog::Poll(poll) => {
            let modal = egui::Modal::new(egui::Id::unique("new-poll")).frame(frame).show(ui.ctx(), |ui| {
                crate::polls::dialog(ui, &palette, poll, &c.id, &mut out);
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::ShareContact(step) => {
            use langis_core::share_contact as share;
            let mut next = None;
            let modal = egui::Modal::new(egui::Id::unique("share-contact")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(360.);
                match step {
                    ShareStep::Pick { query, candidates } => {
                        ui.label(RichText::new("Select contact").font(theme::semibold(18.)));
                        ui.add_space(12.);
                        widgets::text_field(ui, query, "Search", false);
                        ui.add_space(8.);
                        let Some(list) = candidates else {
                            ui.vertical_centered(|ui| crate::widgets::spinner(ui, 24.));
                            return;
                        };
                        let shown: Vec<share::ShareCandidate> = list.iter().filter(|c| share::matches(c, query)).cloned().collect();
                        if shown.is_empty() {
                            ui.add_space(24.);
                            ui.vertical_centered(|ui| {
                                let text = if query.trim().is_empty() { "No contacts".to_owned() } else { format!("No results for \u{201c}{}\u{201d}", query.trim()) };
                                ui.label(RichText::new(text).font(theme::regular(14.)).color(palette.text_secondary));
                            });
                            ui.add_space(24.);
                            return;
                        }
                        egui::ScrollArea::vertical().max_height(420.).min_scrolled_height(320.).auto_shrink([false, false]).show(ui, |ui| {
                            // Desktop groups the list by first letter.
                            for (letter, people) in share::group_by_first_initial(shown, |c| c.title.as_str()) {
                                ui.add_space(6.);
                                ui.label(RichText::new(letter).font(theme::semibold(13.)).color(palette.text_secondary));
                                for person in people {
                                    let sid = person.id.as_direct().unwrap_or_default().to_owned();
                                    let color = sid.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                                    let photo = app.conversation(&person.id).and_then(|c| c.avatar_path.clone());
                                    if crate::panes::contact_row(ui, &palette, &person.title, person.phone.as_deref(), color, photo.as_deref()).clicked() {
                                        next = Some(person.clone());
                                    }
                                }
                            }
                        });
                    }
                    ShareStep::Loading => {
                        ui.label(RichText::new("Share contact").font(theme::semibold(18.)));
                        ui.add_space(24.);
                        ui.vertical_centered(|ui| crate::widgets::spinner(ui, 24.));
                        ui.add_space(24.);
                    }
                    ShareStep::Share { draft, selection } => {
                        ui.label(RichText::new("Share contact").font(theme::semibold(18.)));
                        ui.add_space(16.);
                        let name = draft.name.as_ref().and_then(|n| n.full()).or_else(|| draft.organization.clone()).unwrap_or_default();
                        ui.vertical_centered(|ui| {
                            let color = draft.aci.as_deref().unwrap_or_default().bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                            let photo = draft.avatar.as_ref().map(|a| a.path.clone());
                            let (at, _) = ui.allocate_exact_size(egui::vec2(80., 80.), Sense::hover());
                            widgets::paint_avatar(ui, at, &widgets::AvatarSpec { photo: photo.as_deref(), ..widgets::AvatarSpec::person(&name, color) });
                            ui.add_space(8.);
                            ui.label(RichText::new(&name).font(theme::semibold(16.)));
                        });
                        ui.add_space(12.);
                        // The optional details, each a checkbox as Desktop's.
                        if let Some(e164) = &draft.e164 {
                            crate::widgets::checkbox(ui, &mut selection.e164, RichText::new(format!("Phone  {}", share::format_phone(e164, None))).font(theme::regular(14.)));
                        }
                        if let Some((given, family)) = &draft.nickname {
                            crate::widgets::checkbox(ui, &mut selection.nickname, RichText::new(format!("Nickname  {} {}", given, family).trim().to_owned()).font(theme::regular(14.)));
                        }
                        if let Some(note) = &draft.note {
                            crate::widgets::checkbox(ui, &mut selection.note, RichText::new(format!("Notes  {note}")).font(theme::regular(14.)));
                        }
                        ui.add_space(16.);
                        let shared = share::apply_selection(draft, *selection);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let can = share::can_share(&shared);
                            if ui.add_enabled_ui(can, |ui| widgets::pill_button(ui, "Send contact", true)).inner.clicked() {
                                out.push(Do::Command(Command::ShareContact { conversation: c.id.clone(), share: Box::new(shared.clone()) }));
                                out.push(Do::Close);
                            }
                            if widgets::pill_button(ui, "Cancel", false).clicked() {
                                out.push(Do::Close);
                            }
                        });
                    }
                }
            });
            if let Some(person) = next {
                *step = ShareStep::Loading;
                app.load(move |db| async move {
                    let draft = share::with_e164_as_missing_name(share::read_draft(&db, &person).await, None);
                    let selection = share::default_selection(&draft);
                    Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
                        if let Some(Dialog::ShareContact(step)) = app.chat.as_mut().and_then(|c| c.dialog.as_mut()) {
                            *step = ShareStep::Share { draft, selection };
                        }
                    })))
                });
            }
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::ReactionViewer { message, tab } => {
            let Some(m) = chat.iter().find(|m| m.id == *message).cloned() else { return vec![Do::Close] };
            if m.reactions.is_empty() {
                return vec![Do::Close];
            }
            // Tabs: All, then each emoji by count (Desktop's order).
            let mut kinds: Vec<(String, usize)> = Vec::new();
            for r in &m.reactions {
                match kinds.iter_mut().find(|(e, _)| *e == r.emoji) {
                    Some(k) => k.1 += 1,
                    None => kinds.push((r.emoji.clone(), 1)),
                }
            }
            kinds.sort_by(|a, b| b.1.cmp(&a.1));
            if tab.as_ref().is_some_and(|t| !kinds.iter().any(|(e, _)| e == t)) {
                *tab = None;
            }
            let me = app.aci.clone();
            let modal = egui::Modal::new(egui::Id::unique("reaction-viewer")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(320.);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
                    let pill = |ui: &mut Ui, label: egui::WidgetText, on: bool| {
                        let galley = label.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20., 32.), Sense::click());
                        if on {
                            ui.painter().rect_filled(rect, CornerRadius::same(16), palette.fill_pressed);
                        } else if r.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(16), palette.fill);
                        }
                        ui.painter().galley(rect.center() - galley.size() / 2., galley, palette.text);
                        r.clicked()
                    };
                    if pill(ui, RichText::new(format!("All {}", m.reactions.len())).font(theme::semibold(13.)).into(), tab.is_none()) {
                        *tab = None;
                    }
                    for (emoji, count) in &kinds {
                        let mut job = egui::text::LayoutJob::default();
                        crate::chat::append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::emoji(18.), color: palette.text, valign: egui::Align::Center, ..Default::default() });
                        job.append(&format!(" {count}"), 0., egui::TextFormat { font_id: theme::semibold(13.), color: palette.text, valign: egui::Align::Center, ..Default::default() });
                        if pill(ui, job.into(), tab.as_deref() == Some(emoji.as_str())) {
                            *tab = Some(emoji.clone());
                        }
                    }
                });
                ui.add_space(12.);
                // You first, then newest first.
                let mut rows: Vec<&langis_store::Reaction> = m.reactions.iter().filter(|r| tab.as_ref().is_none_or(|t| *t == r.emoji)).collect();
                rows.sort_by(|a, b| a.author.is_some().cmp(&b.author.is_some()).then(b.sent_at.cmp(&a.sent_at)));
                egui::ScrollArea::vertical().max_height(360.).auto_shrink([false, true]).show(ui, |ui| {
                    for r in rows {
                        let (name, color, photo) = match &r.author {
                            None => ("You".to_owned(), me.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b)), app.me.1.clone()),
                            Some(a) => (app.name(a), a.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b)), app.conversation(&ConversationId::direct(a)).and_then(|p| p.avatar_path.clone())),
                        };
                        let (rect, row) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.), Sense::click());
                        let mine = r.author.is_none();
                        if mine && row.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                        }
                        let avatar = egui::Rect::from_min_size(egui::pos2(rect.left() + 4., rect.center().y - 16.), egui::vec2(32., 32.));
                        let initials_of = if r.author.is_none() { app.me.0.clone() } else { name.clone() };
                        widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo: photo.as_deref(), ..widgets::AvatarSpec::person(&initials_of, color) });
                        ui.painter().text(egui::pos2(avatar.right() + 12., rect.center().y), egui::Align2::LEFT_CENTER, &name, theme::regular(14.), palette.text);
                        let mut job = egui::text::LayoutJob::default();
                        crate::chat::append_with_emoji(&mut job, &r.emoji, egui::TextFormat { font_id: theme::emoji(20.), color: palette.text, ..Default::default() });
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        ui.painter().galley(egui::pos2(rect.right() - 8. - g.size().x, rect.center().y - g.size().y / 2.), g, palette.text);
                        if mine && row.on_hover_text("Click to remove").clicked() {
                            out.push(Do::Command(Command::React { message_id: m.id, emoji: r.emoji.clone(), remove: true }));
                            out.push(Do::Close);
                        }
                    }
                });
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::Pinned { messages } => {
            let modal = egui::Modal::new(egui::Id::unique("pinned-messages")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(420.);
                ui.label(RichText::new("Pinned messages").font(theme::semibold(18.)));
                ui.add_space(12.);
                egui::ScrollArea::vertical().max_height(460.).auto_shrink([false, true]).show(ui, |ui| {
                    for m in messages.iter() {
                        let who = match &m.author {
                            None => "You".to_owned(),
                            Some(a) => app.name(a),
                        };
                        let text = if m.body.is_empty() { m.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_default() } else { langis_store::hide_spoilers(&m.body, &m.styles) };
                        let response = egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(who).font(theme::semibold(13.)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(crate::format::short_timestamp(m.sent_at)).font(theme::regular(12.)).color(palette.text_secondary));
                                });
                            });
                            let mut job = egui::text::LayoutJob::simple(text, theme::regular(14.), palette.text, ui.available_width());
                            job.wrap.max_rows = 4;
                            job.wrap.overflow_character = Some('…');
                            ui.label(job);
                        });
                        let r = ui.interact(response.response.rect, ui.scope_id().with(("pinned", m.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if r.clicked() {
                            out.push(Do::Close);
                            out.push(Do::Jump(m.id));
                        }
                        ui.add_space(6.);
                    }
                });
                ui.add_space(12.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, "Done", true).clicked() {
                        out.push(Do::Close);
                    }
                });
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::Explain { title, body } => {
            let (title, body) = (*title, *body);
            if let Some(answer) = crate::dialog::show(ui.ctx(), egui::Id::unique("notice-explain"), title, body, None, &[("OK", crate::dialog::Variant::StrongPrimary)]) {
                let _ = answer;
                out.push(Do::Close);
            }
        }
        Dialog::Reactions { slots, selected, picker } => {
            let modal = egui::Modal::new(egui::Id::unique("customize-reactions-dialog")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(360.);
                ui.label(RichText::new("Customize reactions").font(theme::semibold(18.)));
                ui.add_space(4.);
                ui.label(RichText::new("Choose a reaction, then pick its replacement.").font(theme::regular(13.)).color(palette.text_secondary));
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.;
                    for (i, emoji) in slots.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(44.), Sense::click());
                        let on = *selected == Some(i);
                        ui.painter().circle_filled(rect.center(), 22., if on { palette.fill_pressed } else if r.hovered() { palette.fill } else { palette.popover });
                        if on {
                            ui.painter().circle_stroke(rect.center(), 22., egui::Stroke::new(2., palette.accent));
                        }
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, emoji, theme::emoji(26.), palette.text);
                        if r.clicked() {
                            *selected = Some(i);
                        }
                    }
                });
                ui.add_space(12.);
                if let Some(picked) = picker.body(ui, &app.recent_emoji)
                    && let Some(i) = *selected
                {
                    slots[i] = picked.to_owned();
                    *selected = Some((i + 1) % slots.len());
                }
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    if widgets::pill_button(ui, "Reset", false).clicked() {
                        *slots = DEFAULT_REACTIONS.iter().map(|e| e.to_string()).collect();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::pill_button(ui, "Save", true).clicked() {
                            out.push(Do::Command(Command::SetPreferredReactions(slots.clone())));
                            out.push(Do::Close);
                        }
                        if widgets::pill_button(ui, "Cancel", false).clicked() {
                            out.push(Do::Close);
                        }
                    });
                });
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::EditHistory { message, details } => {
            let id = *message;
            let Some(m) = chat.iter().find(|m| m.id == id) else { return vec![Do::Close] };
            let modal = egui::Modal::new(egui::Id::unique("edit-history")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(400.);
                ui.label(RichText::new("Edit history").font(theme::semibold(18.)));
                ui.add_space(12.);
                egui::ScrollArea::vertical().max_height(460.).auto_shrink([false, true]).show(ui, |ui| {
                    // The current version, then the older ones, each with
                    // its formatting and mentions (spoilers hidden), and the
                    // message's attachments on the current one.
                    let mut versions: Vec<Message> = vec![m.clone()];
                    if let Some(details) = details {
                        let mut older: Vec<&langis_store::EditRevision> = details.edits.iter().collect();
                        older.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
                        for e in older {
                            if e.body == m.body && e.sent_at == m.sent_at {
                                continue;
                            }
                            let mut v = m.clone();
                            v.body = e.body.clone();
                            v.styles = e.styles.clone();
                            v.mentions = e.mentions.clone();
                            v.sent_at = e.sent_at;
                            v.attachments.clear();
                            versions.push(v);
                        }
                    } else {
                        crate::widgets::spinner(ui, 24.);
                    }
                    let outgoing = m.direction == Direction::Outgoing;
                    for v in versions {
                        ui.with_layout(egui::Layout::top_down(if outgoing { egui::Align::Max } else { egui::Align::Min }), |ui| {
                            let fill = if outgoing { palette.badge } else { palette.bubble_in };
                            let color = if outgoing { palette.bubble_out_text() } else { palette.text };
                            let meta = if outgoing { palette.bubble_out_meta() } else { palette.text_secondary };
                            egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(18)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                                ui.set_max_width(300.);
                                if let Some(a) = v.attachments.iter().find(|a| !a.sticker && a.content_type.starts_with("image/")) {
                                    if let Some(path) = a.local_path.as_deref().or(a.thumbnail_path.as_deref()) {
                                        let (at, _) = ui.allocate_exact_size(Vec2::new(276., 180.), Sense::hover());
                                        crate::chat::cover_image(ui, path, at, CornerRadius::same(12));
                                    }
                                } else if let Some(a) = v.attachments.first() {
                                    ui.label(RichText::new(langis_store::attachment_preview(a)).font(theme::semibold(13.)).color(color));
                                }
                                if !v.body.is_empty() {
                                    let rich = crate::chat::rich_text(&v, &|sid| app.name(sid), color, color, false, 14., usize::MAX);
                                    let mut job = rich.job;
                                    job.wrap.max_width = 276.;
                                    ui.label(job);
                                }
                                ui.label(RichText::new(crate::format::message_time(v.sent_at)).font(theme::regular(11.)).color(meta));
                            });
                        });
                        ui.add_space(8.);
                    }
                });
                ui.add_space(12.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, "Done", true).clicked() {
                        out.push(Do::Close);
                    }
                });
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::ContactDetail { message } => {
            let Some(m) = chat.iter().find(|m| m.id == *message) else { return vec![Do::Close] };
            let Some(langis_store::MessageContent::Contact(contact)) = m.content.as_ref() else { return vec![Do::Close] };
            let contact = contact.clone();
            let mut go = None;
            if panel(ui, &palette, "Contact", |ui| go = contact_detail(app, ui, &palette, &contact)) {
                out.push(Do::Close);
            }
            if let Some(command) = go {
                out.push(command);
                out.push(Do::Close);
            }
        }
        Dialog::Info { message, details } => {
            let id = *message;
            let Some(m) = chat.iter().find(|m| m.id == id) else { return vec![Do::Close] };
            // Desktop's MessageDetail is a panel over the conversation: a
            // back button and its title, then the details.
            let pane = ui.max_rect();
            egui::Area::new(egui::Id::unique("message-info")).order(egui::Order::Middle).fixed_pos(pane.min).show(ui.ctx(), |ui| {
                ui.set_min_size(pane.size());
                ui.set_max_size(pane.size());
                ui.interact(pane, ui.scope_id().with("message-info-bg"), Sense::click());
                ui.painter().rect_filled(pane, 0, palette.background);
                let header = Rect::from_min_size(pane.min, Vec2::new(pane.width(), 52.));
                ui.painter().text(header.center(), Align2::CENTER_CENTER, "Message details", theme::semibold(14.), palette.text);
                ui.painter().hline(header.x_range(), header.bottom(), Stroke::new(1., palette.border));
                let back = Rect::from_center_size(Pos2::new(header.left() + 30., header.center().y), Vec2::splat(32.));
                let hit = ui.interact(back, ui.scope_id().with("message-info-back"), Sense::click());
                if hit.hovered() {
                    ui.painter().circle_filled(back.center(), 16., palette.fill);
                }
                Icon::ChevronLeft.paint(ui, back, 20., palette.text);
                if hit.on_hover_text("Back").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    out.push(Do::Close);
                }
                let body = Rect::from_min_max(Pos2::new(pane.left(), header.bottom() + 1.), pane.max);
                ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        let width = ui.available_width().min(600.);
                        ui.add_space(20.);
                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - width) / 2.).max(16.));
                            ui.vertical(|ui| {
                                ui.set_width(width - 32.);
                                info_body(app, ui, &palette, m, details.as_ref());
                            });
                        });
                        ui.add_space(20.);
                    });
                });
            });
        }
    }
    out
}

fn radio_row(ui: &mut Ui, on: bool, label: &str) -> bool {
    let palette = Palette::of(ui.ctx());
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.), Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
    }
    let c = Pos2::new(rect.left() + 18., rect.center().y);
    ui.painter().circle_stroke(c, 9., Stroke::new(2., if on { palette.badge } else { palette.text_secondary }));
    if on {
        ui.painter().circle_filled(c, 5., palette.badge);
    }
    ui.painter().text(Pos2::new(rect.left() + 38., rect.center().y), Align2::LEFT_CENTER, label, theme::regular(14.), palette.text);
    r.clicked()
}

/// A panel over the conversation (Desktop's `ConversationPanel`): a back
/// button and the title, then `body` in a centered column. True on back.
fn panel(ui: &Ui, palette: &Palette, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let pane = ui.max_rect();
    let mut back = false;
    egui::Area::new(egui::Id::unique(("conversation-panel", title))).order(egui::Order::Middle).fixed_pos(pane.min).show(ui.ctx(), |ui| {
        ui.set_min_size(pane.size());
        ui.set_max_size(pane.size());
        ui.interact(pane, ui.scope_id().with("panel-bg"), Sense::click());
        ui.painter().rect_filled(pane, 0, palette.background);
        let header = Rect::from_min_size(pane.min, Vec2::new(pane.width(), 52.));
        ui.painter().text(header.center(), Align2::CENTER_CENTER, title, theme::semibold(14.), palette.text);
        ui.painter().hline(header.x_range(), header.bottom(), Stroke::new(1., palette.border));
        let b = Rect::from_center_size(Pos2::new(header.left() + 30., header.center().y), Vec2::splat(32.));
        let hit = ui.interact(b, ui.scope_id().with("panel-back"), Sense::click());
        if hit.hovered() {
            ui.painter().circle_filled(b.center(), 16., palette.fill);
        }
        Icon::ChevronLeft.paint(ui, b, 20., palette.text);
        back = hit.on_hover_text("Back").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
        let rest = Rect::from_min_max(Pos2::new(pane.left(), header.bottom() + 1.), pane.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rest), |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let width = ui.available_width().min(600.);
                ui.add_space(20.);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - width) / 2.).max(16.));
                    ui.vertical(|ui| {
                        ui.set_width(width - 32.);
                        body(ui);
                    });
                });
                ui.add_space(20.);
            });
        });
    });
    back
}

/// Desktop's `ContactDetail`: the photo and name, Send message for a
/// Signal account, then each number, email and address with its kind.
fn contact_detail(app: &App, ui: &mut Ui, palette: &Palette, c: &langis_store::SharedContact) -> Option<Do> {
    use langis_store::{AddressKind, ContactMethodKind};
    let mut out = None;
    let name = c.display_nickname().or_else(|| c.display_name()).or_else(|| c.organization.clone()).unwrap_or_else(|| "Unknown contact".into());
    ui.vertical_centered(|ui| {
        let spec = widgets::AvatarSpec { name: &name, color: 0, photo: c.avatar.as_ref().and_then(|a| a.local_path.as_deref()), group: false, note_to_self: false };
        widgets::avatar(ui, &spec, 80.);
        ui.add_space(10.);
        ui.label(RichText::new(&name).font(theme::semibold(20.)));
        if let Some(org) = c.organization.as_ref().filter(|o| !o.is_empty() && **o != name) {
            ui.label(RichText::new(org).font(theme::regular(14.)).color(palette.text_secondary));
        }
        if let Some(aci) = &c.aci {
            ui.add_space(12.);
            if widgets::pill_button(ui, "Send message", true).clicked() {
                out = Some(Do::OpenChat(langis_store::ConversationId::direct(aci)));
            }
        }
    });
    let kind = |k: ContactMethodKind, label: &Option<String>| match k {
        ContactMethodKind::Home => "Home".to_owned(),
        ContactMethodKind::Mobile => "Mobile".to_owned(),
        ContactMethodKind::Work => "Work".to_owned(),
        ContactMethodKind::Custom => label.clone().unwrap_or_else(|| "Other".into()),
    };
    let row = |ui: &mut Ui, label: String, value: String| {
        ui.add_space(14.);
        ui.label(RichText::new(label).font(theme::regular(12.)).color(palette.text_secondary));
        ui.add(egui::Label::new(RichText::new(value).font(theme::regular(14.))).selectable(true));
    };
    ui.add_space(12.);
    for n in &c.numbers {
        row(ui, kind(n.kind, &n.label), langis_core::share_contact::format_phone(&n.value, app.phone.as_deref()));
    }
    for e in &c.emails {
        row(ui, kind(e.kind, &e.label), e.value.clone());
    }
    for a in &c.addresses {
        let label = match a.kind {
            AddressKind::Home => "Home".to_owned(),
            AddressKind::Work => "Work".to_owned(),
            AddressKind::Custom => a.label.clone().unwrap_or_else(|| "Address".into()),
        };
        let lines: Vec<String> = [&a.street, &a.pobox, &a.neighborhood]
            .into_iter()
            .flatten()
            .cloned()
            .chain([[a.city.clone(), a.region.clone(), a.postcode.clone()].into_iter().flatten().collect::<Vec<_>>().join(" ")].into_iter().filter(|s| !s.is_empty()))
            .chain(a.country.clone())
            .filter(|s| !s.trim().is_empty())
            .collect();
        row(ui, label, lines.join("\n"));
    }
    if let Some(note) = c.note.as_ref().filter(|n| !n.is_empty()) {
        row(ui, "Note".into(), note.clone());
    }
    out
}

/// Desktop's round checkbox.
pub fn check_circle(ui: &Ui, center: Pos2, on: bool, palette: &Palette) {
    if on {
        ui.painter().circle_filled(center, 10., palette.badge);
        Icon::Check.paint(ui, Rect::from_center_size(center, Vec2::splat(16.)), 14., Color32::WHITE);
    } else {
        ui.painter().circle_stroke(center, 9., Stroke::new(1.5, palette.text_secondary));
    }
}

fn info_body(app: &App, ui: &mut Ui, palette: &Palette, m: &Message, details: Option<&Details>) {
    // Desktop's `MessageDetail` opens with the message itself.
    let outgoing = m.direction == Direction::Outgoing;
    let preview = if m.deleted {
        "This message was deleted".to_owned()
    } else if !m.body.is_empty() {
        langis_store::hide_spoilers(&m.body, &m.styles)
    } else {
        m.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_default()
    };
    ui.with_layout(egui::Layout::top_down(if outgoing { egui::Align::Max } else { egui::Align::Min }), |ui| {
        let fill = if outgoing { palette.accent } else { palette.bubble_in };
        let color = if outgoing { Color32::WHITE } else { palette.text };
        egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(18)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_max_width(300.);
            let mut job = egui::text::LayoutJob::simple(preview, theme::regular(14.), color, 300.);
            job.wrap.max_rows = 6;
            job.wrap.overflow_character = Some('…');
            ui.label(job);
        });
    });
    ui.add_space(12.);
    let when = |label: &str, at: i64| {
        let time = chrono::DateTime::from_timestamp_millis(at).map(|t| t.with_timezone(&chrono::Local).format("%A, %B %-d, %Y at %-I:%M %p").to_string()).unwrap_or_default();
        (label.to_owned(), time)
    };
    let mut rows = vec![when("Sent", m.sent_at)];
    if m.direction == Direction::Incoming {
        rows.push(when("Received", m.received_at));
        if let Some(author) = &m.author {
            rows.insert(0, ("From".into(), app.name(author)));
        }
    }
    if m.expires_in > 0 {
        let left = m.expire_start.map(|s| (s + m.expires_in as i64 * 1000 - chrono::Utc::now().timestamp_millis()).max(0) / 1000);
        let text = match left {
            Some(secs) => crate::format::duration(secs),
            None => crate::format::timer_long(m.expires_in),
        };
        rows.push(("Disappears in".into(), text));
    }
    for (label, value) in rows {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(110., 18.), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(110.);
                ui.label(RichText::new(label).font(theme::medium(13.)).color(palette.text_secondary));
            });
            ui.label(RichText::new(value).font(theme::regular(13.)));
        });
    }
    let Some(details) = details else {
        ui.add_space(8.);
        crate::widgets::spinner(ui, 24.);
        return;
    };
    if m.direction == Direction::Outgoing && !details.states.is_empty() {
        let groups: [(&str, &dyn Fn(SendStatus) -> bool); 6] = [
            ("Not sent", &|s| s == SendStatus::Failed),
            ("Viewed by", &|s| s == SendStatus::Viewed),
            ("Read by", &|s| s == SendStatus::Read),
            ("Delivered to", &|s| s == SendStatus::Delivered),
            ("Sent to", &|s| s == SendStatus::Sent),
            ("Pending", &|s| matches!(s, SendStatus::Pending)),
        ];
        for (title, pick) in groups {
            let people: Vec<&SendState> = details.states.iter().filter(|s| pick(s.status)).collect();
            if people.is_empty() {
                continue;
            }
            ui.add_space(14.);
            ui.label(RichText::new(title).font(theme::semibold(14.)));
            for state in people {
                ui.horizontal(|ui| {
                    let name = app.name(&state.recipient);
                    let color = state.recipient.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                    widgets::avatar(ui, &widgets::AvatarSpec::person(&name, color), 28.);
                    ui.label(RichText::new(name).font(theme::regular(14.)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Each person's status icon, as Desktop's contact rows have.
                        let status = match state.status {
                            SendStatus::Failed => Some(langis_store::DeliveryStatus::Failed),
                            SendStatus::Pending => Some(langis_store::DeliveryStatus::Sending),
                            SendStatus::Sent => Some(langis_store::DeliveryStatus::Sent),
                            SendStatus::Delivered => Some(langis_store::DeliveryStatus::Delivered),
                            SendStatus::Read => Some(langis_store::DeliveryStatus::Read),
                            SendStatus::Viewed => Some(langis_store::DeliveryStatus::Viewed),
                            SendStatus::Skipped => None,
                        };
                        if let Some(status) = status {
                            let icon = crate::sidebar::status_icon(status);
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(18., 16.), Sense::hover());
                            let tint = if status == langis_store::DeliveryStatus::Failed { palette.destructive } else { palette.text_secondary };
                            icon.paint(ui, rect, 12., tint);
                        }
                        if let Some(at) = state.updated_at {
                            ui.label(RichText::new(crate::format::short_timestamp(at)).font(theme::regular(12.)).color(palette.text_secondary));
                        }
                    });
                });
            }
        }
    }
    if !details.edits.is_empty() {
        ui.add_space(14.);
        ui.label(RichText::new("Edit history").font(theme::semibold(14.)));
        for edit in &details.edits {
            ui.add_space(4.);
            egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(langis_store::hide_spoilers(&edit.body, &edit.styles)).font(theme::regular(14.)));
                ui.label(RichText::new(crate::format::message_time(edit.sent_at)).font(theme::regular(11.)).color(palette.text_secondary));
            });
        }
    }
}

/// Loads a message's details for the info dialog.
pub fn load_details(app: &App, message: i64) {
    app.load(move |db| async move {
        let states = db.send_states(message).await.unwrap_or_default();
        let edits = db.edit_history(message).await.unwrap_or_default();
        Some(Loaded::Chat(crate::chat::ChatLoaded::Details { message, details: Details { states, edits } }))
    });
}

/// Saves attachment `index` of `m` where the user picks; with several,
/// saves them all into a folder and starts every missing download, as
/// Desktop's `TimelineMessage` Download does.
pub fn save_attachment(app: &mut App, m: &Message, index: usize) {
    let files: Vec<&langis_store::AttachmentMeta> = m.attachments.iter().filter(|a| !a.sticker).collect();
    if files.len() > 1 {
        return save_all(app, m, &files);
    }
    let Some(a) = m.attachments.get(index) else { return };
    let Some(path) = a.local_path.clone() else {
        app.send(Command::FetchAttachment { message_id: m.id, index });
        app.toast("Downloading. Save it again once it's here.");
        return;
    };
    let ext = crate::chat::extension_for(&a.content_type);
    let fallback = format!("signal-{}.{ext}", chrono::Local::now().format("%Y-%m-%d-%H%M%S"));
    let name = safe_file_name(a.file_name.as_deref().unwrap_or(""), &fallback);
    let Some(target) = rfd::FileDialog::new().set_title("Save").set_file_name(&name).save_file() else { return };
    match std::fs::copy(&path, &target) {
        Ok(_) => app.toast(format!("Saved to {}", target.display())),
        Err(error) => app.toast(format!("Couldn't save: {error}")),
    }
}

/// A sender's file name made safe to create in a folder: its last path
/// component only, no control characters, no leading dots, never `.`/`..`
/// or empty (then `fallback`).
pub fn safe_file_name(name: &str, fallback: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() { fallback.to_owned() } else { trimmed.chars().take(200).collect() }
}

fn save_all(app: &mut App, m: &Message, files: &[&langis_store::AttachmentMeta]) {
    let missing = files.iter().filter(|a| a.local_path.is_none()).count();
    if missing > 0 {
        app.send(Command::FetchAttachments { message_id: m.id });
    }
    let here: Vec<_> = files.iter().filter_map(|a| a.local_path.as_ref().map(|p| (a, p))).collect();
    if here.is_empty() {
        app.toast("Downloading. Save them again once they're here.");
        return;
    }
    let Some(folder) = rfd::FileDialog::new().set_title("Save to folder").pick_folder() else { return };
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let mut saved = 0;
    for (n, (a, path)) in here.iter().enumerate() {
        let ext = crate::chat::extension_for(&a.content_type);
        let name = safe_file_name(a.file_name.as_deref().unwrap_or(""), &format!("signal-{stamp}-{}.{ext}", n + 1));
        // Never overwrite: number a name that's taken. `create_new` refuses
        // an existing file or a symlink, so nothing is written through one.
        let stem = std::path::Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_owned();
        let dot_ext = std::path::Path::new(&name).extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
        for k in 0..1000 {
            let file = if k == 0 { name.clone() } else { format!("{stem} ({k}){dot_ext}") };
            let target = folder.join(&file);
            if target.parent() != Some(folder.as_path()) {
                break;
            }
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&target) {
                Ok(mut out) => {
                    if std::fs::File::open(path).and_then(|mut src| std::io::copy(&mut src, &mut out)).is_ok() {
                        saved += 1;
                    } else {
                        drop(out);
                        let _ = std::fs::remove_file(&target);
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => break,
            }
        }
    }
    let note = if missing > 0 { format!("; {missing} still downloading") } else { String::new() };
    app.toast(format!("Saved {saved} of {} to {}{note}", files.len(), folder.display()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn sender_file_names_stay_in_the_folder() {
        use super::safe_file_name as f;
        assert_eq!(f("/home/u/.config/autostart/x.desktop", "fb"), "x.desktop");
        assert_eq!(f("../../.bashrc", "fb"), "bashrc");
        assert_eq!(f("..", "fb"), "fb");
        assert_eq!(f("", "fb"), "fb");
        assert_eq!(f("a\\b\\c.txt", "fb"), "c.txt");
        assert_eq!(f("trail map.png", "fb"), "trail map.png");
    }
}
