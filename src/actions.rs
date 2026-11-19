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
    /// Desktop's `DeleteMessagesModal`, for one message or a selection.
    Delete { messages: Vec<i64> },
    Pin { message: i64, choice: usize },
    Forward { messages: Vec<i64>, query: String, chosen: Vec<ConversationId> },
    Info { message: i64, details: Option<Details> },
    Poll(crate::polls::NewPoll),
    PollVotes { message: i64 },
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
    /// Select mode, starting with this message.
    Select(i64),
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
                if offer.react && ui.item(Some("heart-plus"), "React") {
                    out.push(Do::ReactBar(id));
                }
                if m.direction == Direction::Outgoing && m.poll.as_ref().is_some_and(|p| !p.ended) && !m.deleted && ui.item(Some("stop-circle"), "End poll") {
                    out.push(Do::Command(Command::EndPoll { message_id: id }));
                }
                if offer.forward && ui.item(Some("forward"), "Forward") {
                    out.push(Do::Open(Dialog::Forward { messages: vec![id], query: String::new(), chosen: Vec::new() }));
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
                if let Some(picked) = picker.show(ctx, *at + Vec2::new(0., 420.), &mut open) {
                    out.push(Do::Command(react(picked)));
                    out.push(Do::Close);
                } else if !open {
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
            let for_everyone = chosen.iter().all(|m| langis_core::deletes::can_send(m, note_to_self, now));
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
        Dialog::Forward { messages, query, chosen } => {
            let modal = egui::Modal::new(egui::Id::unique("forward-messages")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(380.);
                ui.label(RichText::new("Forward to").font(theme::semibold(18.)));
                ui.add_space(10.);
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
                ui.add_space(12.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if chosen.len() > 1 { format!("Forward to {}", chosen.len()) } else { "Forward".to_owned() };
                    if widgets::pill_button(ui, &label, true).clicked() && !chosen.is_empty() {
                        out.push(Do::Command(Command::Forward { message_ids: messages.clone(), conversations: chosen.clone() }));
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
        Dialog::PollVotes { message } => {
            let Some(poll) = chat.iter().find(|m| m.id == *message).and_then(|m| m.poll.clone()) else { return vec![Do::Close] };
            let modal = egui::Modal::new(egui::Id::unique("poll-votes")).frame(frame).show(ui.ctx(), |ui| {
                crate::polls::votes_dialog(app, ui, &palette, &poll);
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
        Dialog::Poll(poll) => {
            let modal = egui::Modal::new(egui::Id::unique("new-poll")).frame(frame).show(ui.ctx(), |ui| {
                crate::polls::dialog(ui, &palette, poll, &c.id, &mut out);
            });
            if modal.should_close() {
                out.push(Do::Close);
            }
        }
        Dialog::Info { message, details } => {
            let id = *message;
            let Some(m) = chat.iter().find(|m| m.id == id) else { return vec![Do::Close] };
            let modal = egui::Modal::new(egui::Id::unique("message-info")).frame(frame).show(ui.ctx(), |ui| {
                ui.set_width(400.);
                ui.label(RichText::new("Message details").font(theme::semibold(18.)));
                ui.add_space(12.);
                egui::ScrollArea::vertical().max_height(420.).auto_shrink([false, true]).show(ui, |ui| {
                    info_body(app, ui, &palette, m, details.as_ref());
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
        ui.spinner();
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
                    if let Some(at) = state.updated_at {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(crate::format::short_timestamp(at)).font(theme::regular(12.)).color(palette.text_secondary));
                        });
                    }
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
                ui.label(RichText::new(&edit.body).font(theme::regular(14.)));
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
    let name = a.file_name.clone().unwrap_or_else(|| format!("signal-{}.{ext}", chrono::Local::now().format("%Y-%m-%d-%H%M%S")));
    let Some(target) = rfd::FileDialog::new().set_title("Save").set_file_name(&name).save_file() else { return };
    match std::fs::copy(&path, &target) {
        Ok(_) => app.toast(format!("Saved to {}", target.display())),
        Err(error) => app.toast(format!("Couldn't save: {error}")),
    }
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
        let name = a.file_name.clone().unwrap_or_else(|| format!("signal-{stamp}-{}.{ext}", n + 1));
        // Never overwrite: number a name that's taken.
        let mut target = folder.join(&name);
        let mut k = 1;
        while target.exists() {
            let stem = std::path::Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_owned();
            let ext = std::path::Path::new(&name).extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
            target = folder.join(format!("{stem} ({k}){ext}"));
            k += 1;
        }
        if std::fs::copy(path, &target).is_ok() {
            saved += 1;
        }
    }
    let note = if missing > 0 { format!("; {missing} still downloading") } else { String::new() };
    app.toast(format!("Saved {saved} of {} to {}{note}", files.len(), folder.display()));
}
