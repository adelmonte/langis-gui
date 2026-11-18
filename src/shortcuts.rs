//! Desktop's keyboard shortcuts (`ShortcutGuide`), and the guide itself
//! (Ctrl+/). The guide lists only the ones handled here and in the
//! composer.

use egui::{Align2, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Vec2};
use langis_core::{CallStatus, ChatChange, Command};

use crate::app::{App, Tab};
use crate::theme::{self, Palette};

const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers { command: true, shift: true, ..Modifiers::NONE };
/// Desktop's `ctrlOrAlt` is Alt away from macOS.
const ALT_SHIFT: Modifiers = Modifiers { alt: true, shift: true, ..Modifiers::NONE };

/// The guide's sections: (what, keys).
const GUIDE: &[(&str, &[(&str, &[&str])])] = &[
    (
        "Navigation",
        &[
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
            ("Archive chat", &["Ctrl", "Shift", "A"]),
            ("Unarchive chat", &["Ctrl", "Shift", "U"]),
            ("Close chat", &["Ctrl", "Shift", "C"]),
            ("Show keyboard shortcuts", &["Ctrl", "/"]),
        ],
    ),
    (
        "Composer",
        &[
            ("Add newline to message", &["Shift", "Enter"]),
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
                app.send(Command::AcceptCall { video: true });
            } else if pressed(ctx, ALT_SHIFT, Key::A) {
                app.send(Command::AcceptCall { video: false });
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
                    app.call_view.video_on = Some(on);
                    app.send(Command::SetCallVideo(on));
                }
                if call.group && pressed(ctx, Modifiers::SHIFT, Key::H) {
                    let raised = call.local_demux_id.is_some_and(|d| call.raised_hands.contains(&d));
                    app.send(Command::RaiseHand(!raised));
                }
            }
        }
    } else if let Some(id) = app.selected.clone() {
        let callable = app.conversation(&id).is_some_and(|c| c.kind == langis_store::ConversationKind::Direct && !c.blocked);
        for (key, video) in [(Key::C, false), (Key::Y, true)] {
            if pressed(ctx, ALT_SHIFT, key) && callable {
                app.send(Command::StartCall { conversation: id.clone(), video });
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
            app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::Archived(archive) });
        }
    }
    if pressed(ctx, CTRL_SHIFT, Key::C) {
        app.close_chat();
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
            ui.painter().text(head.center(), Align2::CENTER_CENTER, "Keyboard shortcuts", theme::semibold(14.), palette.text);
            let x = Rect::from_center_size(Pos2::new(head.right() - 32., head.center().y), Vec2::splat(32.));
            let response = ui.interact(x, ui.scope_id().with("close"), Sense::click());
            if response.hovered() {
                ui.painter().circle_filled(x.center(), 16., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "x", x.center(), 20., palette.text);
            close |= response.on_hover_text("Close").clicked();
            ui.add_space(8.);
            egui::ScrollArea::vertical().max_height((screen.height() - 160.).max(200.)).show(ui, |ui| {
                for (section, rows) in GUIDE {
                    ui.add_space(12.);
                    let (title, _) = ui.allocate_exact_size(Vec2::new(width, 20.), Sense::hover());
                    ui.painter().text(Pos2::new(title.left() + 24., title.center().y), Align2::LEFT_CENTER, *section, theme::semibold(14.), palette.text);
                    ui.add_space(4.);
                    for (what, keys) in *rows {
                        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
                        ui.painter().text(Pos2::new(row.left() + 24., row.center().y), Align2::LEFT_CENTER, *what, theme::regular(14.), palette.text);
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
