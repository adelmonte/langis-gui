//! Desktop's keyboard shortcuts (`ShortcutGuide`), and the guide itself
//! (Ctrl+/). The guide lists only the ones handled here and in the
//! composer.

use egui::{Align2, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Vec2};
use langis_core::{CallStatus, Command};

use crate::app::{App, Tab};
use crate::theme::{self, Palette};
use crate::i18n::tr;

const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers { command: true, shift: true, ..Modifiers::NONE };
/// Desktop's `ctrlOrAlt` is Alt away from macOS.
const ALT_SHIFT: Modifiers = Modifiers { alt: true, shift: true, ..Modifiers::NONE };

/// The guide's sections: (what, keys).
const GUIDE: &[(&str, &[(&str, &[&str])])] = &[
    (
        "Navigation",
        &[
            ("Navigate by section", &["Ctrl", "T"]),
            ("Go to previous chat", &["Alt", "↑"]),
            ("Go to next chat", &["Alt", "↓"]),
            ("Go to previous unread chat", &["Alt", "Shift", "↑"]),
            ("Go to next unread chat", &["Alt", "Shift", "↓"]),
            ("Go to chat by index", &["Ctrl", "1 to 9"]),
            ("Open Settings", &["Ctrl", ","]),
            ("Start new chat", &["Ctrl", "N"]),
            ("Search", &["Ctrl", "F"]),
            ("Search in chat", &["Ctrl", "Shift", "F"]),
            ("Focus composer", &["Ctrl", "Shift", "T"]),
            ("Open all media view", &["Ctrl", "Shift", "M"]),
            ("Open emoji chooser", &["Ctrl", "Shift", "J"]),
            ("Open sticker chooser", &["Ctrl", "Shift", "O"]),
            ("Open GIF chooser", &["Ctrl", "Shift", "G"]),
            ("Begin recording voice note", &["Ctrl", "Shift", "Y"]),
            ("Open chat menu", &["Ctrl", "Shift", "L"]),
            ("Focus oldest unread or last message", &["Ctrl", "J"]),
            ("Scroll to top of list", &["Ctrl", "↑"]),
            ("Scroll to bottom of list", &["Ctrl", "↓"]),
            ("Archive chat", &["Ctrl", "Shift", "A"]),
            ("Unarchive chat", &["Ctrl", "Shift", "U"]),
            ("Close chat", &["Ctrl", "Shift", "C"]),
            ("Show keyboard shortcuts", &["Ctrl", "/"]),
        ],
    ),
    (
        "Message",
        &[
            ("Open context menu for selected message", &["Shift", "F10"]),
            ("Default action for selected message", &["Enter"]),
            ("View selected message details", &["Ctrl", "D"]),
            ("Toggle reply to selected message", &["Ctrl", "Shift", "R"]),
            ("Toggle emoji-reaction picker for selected message", &["Ctrl", "Shift", "E"]),
            ("Save attachment from selected message", &["Ctrl", "S"]),
            ("Delete selected messages", &["Ctrl", "Shift", "D"]),
            ("Forward selected messages", &["Ctrl", "Shift", "S"]),
        ],
    ),
    (
        "Composer",
        &[
            ("Add newline to message", &["Shift", "Enter"]),
            ("Expand composer", &["Ctrl", "Shift", "K"]),
            ("Send (in expanded composer)", &["Ctrl", "Enter"]),
            ("Attach file", &["Ctrl", "U"]),
            ("Remove draft link preview", &["Ctrl", "P"]),
            ("Remove all draft attachments", &["Ctrl", "Shift", "P"]),
            ("Edit last message", &["↑"]),
            ("Bold text", &["Ctrl", "B"]),
            ("Italicize text", &["Ctrl", "I"]),
            ("Strikethrough text", &["Ctrl", "Shift", "X"]),
            ("Monospace text", &["Ctrl", "E"]),
            ("Spoiler text", &["Ctrl", "Shift", "B"]),
        ],
    ),
    (
        "Calls",
        &[
            ("Mute and unmute", &["Shift", "M"]),
            ("Turn camera on and off", &["Shift", "V"]),
            ("Toggle expanded preview on and off", &["Shift", "P"]),
            ("Raise or lower hand", &["Shift", "H"]),
            ("Accept call with video", &["Alt", "Shift", "V"]),
            ("Accept call without video", &["Alt", "Shift", "A"]),
            ("Decline call", &["Alt", "Shift", "D"]),
            ("Start voice call", &["Alt", "Shift", "C"]),
            ("Start video call", &["Alt", "Shift", "Y"]),
            ("End call", &["Alt", "Shift", "E"]),
        ],
    ),
];

fn pressed(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
}

/// The chat list as shown, top to bottom.
fn listed(app: &App) -> Vec<langis_store::ConversationId> {
    app.conversations.iter().filter(|c| crate::panes::visible(app, c)).map(|c| c.id.clone()).collect()
}

/// The chat `step` places from the open one (wrapping), unread only if
/// asked.
fn neighbor(app: &App, step: i32, unread: bool) -> Option<langis_store::ConversationId> {
    let list = listed(app);
    if list.is_empty() {
        return None;
    }
    let here = app.selected.as_ref().and_then(|s| list.iter().position(|c| c == s));
    let n = list.len() as i32;
    let mut at = here.map(|h| h as i32).unwrap_or(if step > 0 { -1 } else { n });
    for _ in 0..n {
        at = (at + step).rem_euclid(n);
        let id = &list[at as usize];
        let is_unread = app.conversation(id).is_some_and(|c| c.unread_count > 0 || c.marked_unread);
        if !unread || is_unread {
            return Some(id.clone());
        }
    }
    None
}

pub fn handle(app: &mut App, ctx: &egui::Context) {
    if pressed(ctx, CTRL, Key::Slash) {
        app.panes.shortcuts = !app.panes.shortcuts;
    }
    // Calls first: Desktop's call keys win while one is up.
    if let Some(call) = app.call.clone() {
        let ringing = call.incoming && call.status == CallStatus::Ringing;
        if ringing {
            if pressed(ctx, ALT_SHIFT, Key::V) {
                if crate::permissions::allowed(app, crate::permissions::Need::Answer(true)) {
                    app.send(Command::AcceptCall { video: true });
                }
            } else if pressed(ctx, ALT_SHIFT, Key::A) {
                if crate::permissions::allowed(app, crate::permissions::Need::Answer(false)) {
                    app.send(Command::AcceptCall { video: false });
                }
            } else if pressed(ctx, ALT_SHIFT, Key::D) {
                app.send(Command::DeclineCall);
            }
        } else {
            if pressed(ctx, ALT_SHIFT, Key::E) {
                app.send(Command::HangUp);
            }
            // Plain Shift letters only while nothing takes typing.
            let typing = ctx.memory(|m| m.focused().is_some());
            if !typing {
                if pressed(ctx, Modifiers::SHIFT, Key::M) {
                    let on = !app.call_view.audio_on;
                    app.call_view.audio_on = on;
                    app.send(Command::SetCallAudio(on));
                }
                if pressed(ctx, Modifiers::SHIFT, Key::V) {
                    let on = !app.call_view.video_on.unwrap_or(call.video);
                    if !on || crate::permissions::allowed(app, crate::permissions::Need::Camera) {
                        app.call_view.video_on = Some(on);
                        app.send(Command::SetCallVideo(on));
                    }
                }
                // Desktop's Shift+P: the self-view large or small.
                if pressed(ctx, Modifiers::SHIFT, Key::P) {
                    app.call_view.self_large = !app.call_view.self_large;
                }
                if call.group && pressed(ctx, Modifiers::SHIFT, Key::H) {
                    let raised = call.local_demux_id.is_some_and(|d| call.raised_hands.contains(&d));
                    app.send(Command::RaiseHand(!raised));
                }
            }
        }
    } else if let Some(id) = app.selected.clone() {
        let callable = app.conversation(&id).is_some_and(|c| c.kind == langis_store::ConversationKind::Direct && !c.blocked && !c.ever_unregistered());
        for (key, video) in [(Key::C, false), (Key::Y, true)] {
            if pressed(ctx, ALT_SHIFT, key) && callable {
                crate::panes::open_lobby(app, id.clone(), video);
            }
        }
    }

    for (key, step, unread) in [(Key::ArrowUp, -1, false), (Key::ArrowDown, 1, false)] {
        if pressed(ctx, Modifiers::ALT, key)
            && let Some(id) = neighbor(app, step, unread)
        {
            app.open(id);
        }
    }
    // Desktop's other keys for them: Ctrl+Tab and Ctrl+Shift+Tab.
    for (modifiers, step) in [(CTRL_SHIFT, -1), (CTRL, 1)] {
        if pressed(ctx, modifiers, Key::Tab)
            && let Some(id) = neighbor(app, step, false)
        {
            app.open(id);
        }
    }
    for (key, step) in [(Key::ArrowUp, -1), (Key::ArrowDown, 1)] {
        if pressed(ctx, ALT_SHIFT, key)
            && let Some(id) = neighbor(app, step, true)
        {
            app.open(id);
        }
    }
    let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
    for (i, key) in digits.into_iter().enumerate() {
        if pressed(ctx, CTRL, key)
            && let Some(id) = listed(app).get(i).cloned()
        {
            app.open(id);
        }
    }
    if pressed(ctx, CTRL, Key::Comma) {
        app.tab = Tab::Settings;
    }
    if pressed(ctx, CTRL, Key::N) {
        app.tab = Tab::Chats;
        app.panes.compose = Some(crate::panes::Compose::Chat { query: String::new(), searching: false, found: None });
    }
    if pressed(ctx, CTRL, Key::F) {
        app.tab = Tab::Chats;
        ctx.memory_mut(|m| m.request_focus(crate::sidebar::search_id()));
    }
    // Desktop's Navigate by section (Ctrl+T, F6): the chat list's search,
    // then the composer, round again.
    if pressed(ctx, CTRL, Key::T) || pressed(ctx, CTRL, Key::F6) || pressed(ctx, Modifiers::NONE, Key::F6) {
        let search = crate::sidebar::search_id();
        let composer = app.selected.as_ref().filter(|_| app.chat.is_some()).map(crate::composer::input_id);
        let in_search = ctx.memory(|m| m.has_focus(search));
        match composer {
            Some(c) if in_search => ctx.memory_mut(|m| m.request_focus(c)),
            _ => ctx.memory_mut(|m| m.request_focus(search)),
        }
    }
    let Some(id) = app.selected.clone().filter(|_| app.chat.is_some()) else { return };
    if pressed(ctx, CTRL_SHIFT, Key::F) {
        app.find_requested = true;
    }
    if pressed(ctx, CTRL_SHIFT, Key::T) {
        ctx.memory_mut(|m| m.request_focus(crate::composer::input_id(&id)));
    }
    if pressed(ctx, CTRL_SHIFT, Key::M) {
        crate::details::open_media(app, id.clone());
    }
    if pressed(ctx, CTRL_SHIFT, Key::J)
        && let Some(chat) = app.chat.as_mut()
    {
        chat.emoji_open = !chat.emoji_open;
    }
    for (key, archive) in [(Key::A, true), (Key::U, false)] {
        if pressed(ctx, CTRL_SHIFT, key) && app.conversation(&id).is_some_and(|c| c.archived != archive) {
            app.set_archived(id.clone(), archive);
        }
    }
    if pressed(ctx, CTRL_SHIFT, Key::C) {
        app.close_chat();
        return;
    }
    // The fun picker's tabs, and a voice note.
    for (key, tab) in [(Key::O, crate::fun::Tab::Stickers), (Key::G, crate::fun::Tab::Gifs)] {
        if pressed(ctx, CTRL_SHIFT, key)
            && let Some(chat) = app.chat.as_mut()
        {
            chat.emoji_open = true;
            app.fun.tab = tab;
        }
    }
    if pressed(ctx, CTRL_SHIFT, Key::Y)
        && let Some(mut chat) = app.chat.take()
    {
        crate::voice::start(app, &mut chat);
        app.chat = Some(chat);
    }
    if pressed(ctx, CTRL_SHIFT, Key::L) {
        let at = ctx.content_rect().right_top() + egui::vec2(-240., 52.);
        crate::menu::open_at(ctx, egui::Id::unique("chat-header-menu"), at);
    }
    if pressed(ctx, CTRL, Key::U)
        && let Some(mut chat) = app.chat.take()
    {
        crate::composer::attach_file(app, &mut chat);
        app.chat = Some(chat);
    }
    // The viewer has its own keys (Ctrl+S saves what it shows).
    if app.lightbox.is_some() {
        return;
    }
    let Some(chat) = app.chat.as_mut() else { return };
    if pressed(ctx, CTRL, Key::P) {
        crate::composer::remove_preview(chat);
    }
    if pressed(ctx, CTRL_SHIFT, Key::P) {
        chat.staged.clear();
        chat.view_once = false;
    }
    // Ctrl+Up and Ctrl+Down: the top and bottom of the timeline.
    if pressed(ctx, CTRL, Key::ArrowUp)
        && let Some(first) = chat.messages.first()
    {
        chat.scroll_target = Some(first.id);
    }
    if pressed(ctx, CTRL, Key::ArrowDown) {
        if chat.at_end {
            chat.scroll_to_bottom = 10;
        } else {
            chat.want_latest = true;
        }
    }
    // Desktop's selected message: Ctrl+J picks the oldest unread one (or
    // the last), the arrows move it while nothing takes typing.
    if pressed(ctx, CTRL, Key::J) {
        let unread = app.conversation(&id).map(|c| c.unread_count).unwrap_or(0) as usize;
        let chat = app.chat.as_mut().unwrap();
        let incoming: Vec<i64> = chat.messages.iter().filter(|m| m.direction == langis_store::Direction::Incoming && m.notice.is_none()).map(|m| m.id).collect();
        let pick = if unread > 0 && unread <= incoming.len() { incoming.get(incoming.len() - unread).copied() } else { chat.messages.iter().rev().find(|m| m.notice.is_none()).map(|m| m.id) };
        chat.kb_selected = pick;
        chat.scroll_target = pick;
        ctx.memory_mut(|m| m.surrender_focus(crate::composer::input_id(&id)));
    }
    let Some(chat) = app.chat.as_mut() else { return };
    let Some(selected) = chat.kb_selected else { return };
    let typing = ctx.memory(|m| m.focused().is_some());
    if !typing {
        for (key, step) in [(Key::ArrowUp, -1i64), (Key::ArrowDown, 1)] {
            if pressed(ctx, Modifiers::NONE, key) {
                let list: Vec<i64> = chat.messages.iter().filter(|m| m.notice.is_none()).map(|m| m.id).collect();
                if let Some(i) = list.iter().position(|m| *m == selected) {
                    let to = (i as i64 + step).clamp(0, list.len() as i64 - 1) as usize;
                    chat.kb_selected = Some(list[to]);
                    chat.scroll_target = Some(list[to]);
                }
            }
        }
        if pressed(ctx, Modifiers::NONE, Key::Escape) && chat.popup.is_none() && chat.dialog.is_none() {
            chat.kb_selected = None;
            return;
        }
    }
    let Some(m) = chat.messages.iter().find(|m| m.id == selected).cloned() else { return };
    // Desktop's Shift+F10: the selected message's context menu.
    if !typing && pressed(ctx, Modifiers::SHIFT, Key::F10) {
        let at = ctx.content_rect().center();
        if let Some(chat) = app.chat.as_mut() {
            chat.popup = Some(crate::actions::Popup::Menu { message: m.id, at });
        }
        return;
    }
    if !typing && pressed(ctx, Modifiers::NONE, Key::Enter) {
        // The default action: a photo or video opens in the viewer.
        if let Some(index) = m.attachments.iter().position(|a| !a.sticker && a.local_path.is_some() && (a.content_type.starts_with("image/") || a.content_type.starts_with("video/"))) {
            app.open_lightbox(&m, index);
            return;
        }
    }
    let chat = app.chat.as_mut().unwrap();
    if pressed(ctx, CTRL, Key::D) {
        crate::actions::load_details(app, m.id);
        if let Some(chat) = app.chat.as_mut() {
            chat.dialog = Some(crate::actions::Dialog::Info { message: m.id, details: None });
        }
        return;
    }
    if pressed(ctx, CTRL_SHIFT, Key::R) {
        chat.quote = if chat.quote == Some(m.id) { None } else { Some(m.id) };
        ctx.memory_mut(|mem| mem.request_focus(crate::composer::input_id(&id)));
    }
    if pressed(ctx, CTRL_SHIFT, Key::E) {
        chat.popup = match chat.popup {
            Some(crate::actions::Popup::Reactions { message, .. }) if message == m.id => None,
            _ => Some(crate::actions::Popup::Reactions { message: m.id, at: ctx.content_rect().center(), full: false, picker: Default::default() }),
        };
    }
    if pressed(ctx, CTRL_SHIFT, Key::D) {
        chat.dialog = Some(crate::actions::Dialog::Delete { messages: vec![m.id] });
    }
    if pressed(ctx, CTRL_SHIFT, Key::S) && crate::actions::can_forward(&m) {
        chat.dialog = Some(crate::actions::Dialog::Forward { messages: vec![m.id], query: String::new(), chosen: Vec::new(), editing: None });
    }
    if pressed(ctx, CTRL, Key::S)
        && let Some(index) = m.attachments.iter().position(|a| !a.sticker)
    {
        crate::actions::save_attachment(app, &m, index);
    }
}

/// Desktop's `ShortcutGuideModal`: each section's shortcuts, their keys
/// as caps at the end.
pub fn guide(app: &mut App, ctx: &egui::Context) {
    if !app.panes.shortcuts {
        return;
    }
    let palette = Palette::of(ctx);
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("shortcut-guide"))
        .frame(egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).inner_margin(egui::Margin { left: 0, right: 0, top: 12, bottom: 16 }))
        .show(ctx, |ui| {
            let screen = ctx.content_rect();
            let width = 440f32.min(screen.width() - 32.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (head, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            ui.painter().text(head.center(), Align2::CENTER_CENTER, tr("Keyboard shortcuts"), theme::semibold(14.), palette.text);
            let x = Rect::from_center_size(Pos2::new(head.right() - 32., head.center().y), Vec2::splat(32.));
            let response = ui.interact(x, ui.scope_id().with("close"), Sense::click());
            if response.hovered() {
                ui.painter().circle_filled(x.center(), 16., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "x", x.center(), 20., palette.text);
            close |= response.on_hover_text(tr("Close")).clicked();
            ui.add_space(8.);
            egui::ScrollArea::vertical().max_height((screen.height() - 160.).max(200.)).show(ui, |ui| {
                for (section, rows) in GUIDE {
                    ui.add_space(12.);
                    let (title, _) = ui.allocate_exact_size(Vec2::new(width, 20.), Sense::hover());
                    ui.painter().text(Pos2::new(title.left() + 24., title.center().y), Align2::LEFT_CENTER, tr(section), theme::semibold(14.), palette.text);
                    ui.add_space(4.);
                    for (what, keys) in *rows {
                        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
                        ui.painter().text(Pos2::new(row.left() + 24., row.center().y), Align2::LEFT_CENTER, tr(what), theme::regular(14.), palette.text);
                        let mut right = row.right() - 24.;
                        for key in keys.iter().rev() {
                            let galley = ui.painter().layout_no_wrap((*key).to_owned(), theme::medium(12.), palette.text);
                            let cap = Rect::from_min_max(Pos2::new(right - (galley.size().x + 12.).max(24.), row.center().y - 12.), Pos2::new(right, row.center().y + 12.));
                            ui.painter().rect_filled(cap, CornerRadius::same(4), palette.fill);
                            ui.painter().galley(cap.center() - galley.size() / 2., galley, palette.text);
                            right = cap.left() - 4.;
                        }
                    }
                }
            });
        });
    if close || modal.should_close() {
        app.panes.shortcuts = false;
    }
}
