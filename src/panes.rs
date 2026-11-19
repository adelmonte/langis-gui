//! What sits around the chats: the state of the Calls and Stories tabs,
//! chat details, the new chat and new group dialogs, chat list menus and
//! folders, message requests, and the account's alerts (megaphones,
//! server alerts, captcha). Events the app doesn't handle itself come here.

use std::time::Instant;

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Ui, Vec2};
use langis_core::{CallReaction, Command, Event, GroupLinkPreview};
use langis_store::{CallLink, ChatFolder, Conversation, ConversationId, ConversationKind, DirectoryEntry, Message, MessageRequestResponse};

use crate::app::{App, Loaded, Tab};
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

#[derive(Default)]
pub struct Panes {
    /// Desktop's `ShortcutGuideModal`.
    pub shortcuts: bool,
    /// Desktop's `ContactModal`.
    pub contact: Option<crate::contact_modal::Contact>,
    // The Calls tab.
    pub calls: Vec<Message>,
    pub adhoc: Vec<langis_store::AdhocCall>,
    pub links: Vec<CallLink>,
    pub calls_loaded: bool,
    pub call_search: String,
    /// Desktop's new call view in place of the calls list, and its query.
    pub new_call: Option<String>,
    /// A chat action waiting on its confirmation (`chat_confirm_dialog`).
    pub chat_confirm: Option<(ConversationId, ChatConfirm)>,
    /// Desktop's `DisappearingTimeDialog`: the chat, the number and unit.
    pub custom_timer: Option<(ConversationId, u32, usize)>,
    /// Desktop's `MuteUntilDialog`: the chat, and the date and time typed.
    pub mute_until: Option<(ConversationId, String, String)>,
    /// Asking before clearing the call history.
    pub clear_calls: bool,
    /// A call link's details, by room id.
    pub link_details: Option<String>,
    pub link_name: String,
    /// Desktop's lobby before joining a group's (or a link's) call.
    pub lobby: Option<Lobby>,
    /// Reactions sent in the group call, with when they came.
    pub call_reactions: Vec<(CallReaction, Instant)>,
    pub participants_open: bool,
    pub reactions_open: bool,

    // The Stories tab.
    pub stories: crate::stories::StoriesView,

    // Dialogs.
    pub compose: Option<Compose>,
    pub group_link: Option<GroupLinkPreview>,
    pub details: Option<crate::details::Details>,
    pub profile: Option<crate::profile::ProfileEditor>,

    // The chat list.
    pub archived: bool,
    pub folders: Vec<ChatFolder>,
    /// The folder shown (`None`: all chats).
    pub folder: Option<String>,
    /// A chat waiting on "Delete chat?", and whether we're still in it.
    pub delete_chat: Option<(ConversationId, bool)>,
    pub menu_for: Option<ConversationId>,
    /// A message request answer waiting on its confirmation.
    pub confirm_answer: Option<(ConversationId, MessageRequestResponse)>,
    /// "Cancel request" to join a group, waiting on its confirmation.
    pub confirm_cancel_join: Option<ConversationId>,
    /// Who's in each group's call, from its last peek (empty: no call).
    pub active_calls: std::collections::HashMap<ConversationId, Vec<String>>,

    // The account's alerts.
    pub megaphones: Vec<langis_core::megaphones::MegaphoneView>,
    pub server_alerts: langis_core::server_alerts::ServerAlerts,
    pub critical_modal: bool,
    pub captcha: Option<String>,
    pub key_transparency_failed: bool,
    pub clock_skew: bool,
}

/// Desktop's `CallingLobby`.
pub struct Lobby {
    pub conversation: ConversationId,
    /// Who's in the call now (ACIs).
    pub users: Vec<String>,
    pub video: bool,
}

/// Desktop's "New chat" and "New group" flows.
pub enum Compose {
    Chat { query: String, searching: bool, found: Option<Result<DirectoryEntry, String>> },
    /// Desktop's Find by username or Find by phone number step.
    Find { phone: bool, query: String, searching: bool, found: Option<Result<DirectoryEntry, String>> },
    Group { picking: bool, query: String, members: Vec<String>, title: String, timer: u32, avatar: Option<Vec<u8>> },
}

/// Called once the account is ready.
pub fn on_ready(app: &mut App) {
    load_folders(app);
    crate::stories::load(app);
    crate::chat_colors::load(app);
    crate::settings::load_zoom(app);
}

pub fn load_folders(app: &App) {
    app.load(|db| async move {
        let folders = db.chat_folders().await.ok()?;
        Some(Loaded::Apply(Box::new(move |app: &mut App| app.panes.folders = folders)))
    });
}

pub fn load_calls(app: &App) {
    app.load(|db| async move {
        let calls = db.call_history(200).await.ok()?;
        let adhoc = db.adhoc_call_history(200).await.unwrap_or_default();
        let links = db.call_links().await.unwrap_or_default();
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            app.panes.calls = calls;
            app.panes.adhoc = adhoc;
            app.panes.links = links;
            app.panes.calls_loaded = true;
        })))
    });
}

/// Events the app itself doesn't take.
pub fn on_event(app: &mut App, event: Event) {
    match event {
        Event::Megaphones(list) => app.panes.megaphones = list,
        Event::StoriesChanged | Event::StorySettingsChanged => crate::stories::load(app),
        Event::ChatFoldersChanged => load_folders(app),
        Event::ChatColorsChanged => crate::chat_colors::load(app),
        Event::CallLinksChanged => load_calls(app),
        Event::CallLinkCreated(room) => {
            load_calls(app);
            app.panes.link_name.clear();
            app.panes.link_details = Some(room);
            app.tab = Tab::Calls;
        }
        Event::CallLinkOpened(room) => {
            load_calls(app);
            let conversation = ConversationId::call_link(&room);
            app.send(Command::PeekGroupCall { conversation: conversation.clone() });
            app.panes.lobby = Some(Lobby { conversation, users: Vec::new(), video: false });
        }
        Event::CallLinkRefused { removed } => {
            app.toast(if removed { "You were removed from the call." } else { "Your request to join this call was declined." });
        }
        Event::GroupPeek { conversation, users } => {
            if let Some(lobby) = app.panes.lobby.as_mut().filter(|l| l.conversation == conversation) {
                lobby.users = users.clone();
            }
            app.panes.active_calls.insert(conversation, users);
        }
        Event::CallReactions(list) => {
            let now = Instant::now();
            app.panes.call_reactions.extend(list.into_iter().map(|r| (r, now)));
        }
        Event::CallMuted { by, target } => {
            let by = app.name(&by);
            match target {
                None => {
                    app.call_view.audio_on = false;
                    app.toast(format!("{by} muted you"));
                }
                Some(target) => {
                    let target = app.name(&target);
                    app.toast(format!("{by} muted {target}"));
                }
            }
        }
        Event::RecipientFound { query, result } => {
            if let Some(Compose::Chat { query: q, searching, found } | Compose::Find { query: q, searching, found, .. }) = app.panes.compose.as_mut()
                && q.trim() == query.trim()
            {
                *searching = false;
                *found = Some(result);
            } else if let Some(Compose::Group { query: q, members, .. }) = app.panes.compose.as_mut()
                && q.trim() == query.trim()
            {
                match result {
                    Ok(entry) => {
                        if let Some(sid) = entry.id.as_direct()
                            && !members.iter().any(|m| m == sid)
                        {
                            members.push(sid.to_owned());
                            app.names.insert(sid.to_owned(), entry.title.clone());
                        }
                        q.clear();
                    }
                    Err(why) => app.toast(why),
                }
            }
        }
        Event::GroupCreated(conversation) => {
            app.panes.compose = None;
            app.reload_conversations();
            app.open(conversation);
        }
        Event::GroupLinkPreview(preview) => app.panes.group_link = Some(preview),
        Event::GroupOpened { conversation, toast } => {
            app.panes.group_link = None;
            app.reload_conversations();
            app.open(conversation);
            if let Some(toast) = toast {
                app.toast(toast);
            }
        }
        Event::GroupLinkFailed { title, description } => {
            app.panes.group_link = None;
            app.toast(format!("{title}: {description}"));
        }
        Event::MessageRequestAnswered { answer, .. } => {
            let text = match answer {
                MessageRequestResponse::Block | MessageRequestResponse::BlockAndDelete => Some("Blocked."),
                MessageRequestResponse::Spam => Some("Reported as spam."),
                MessageRequestResponse::BlockAndSpam => Some("Reported as spam and blocked."),
                _ => None,
            };
            if let Some(text) = text {
                app.toast(text);
            }
        }
        Event::SafetyNumber { conversation, result } => {
            if let Some(d) = app.panes.details.as_mut().filter(|d| d.conversation == conversation) {
                d.safety = Some(result);
            }
        }
        Event::KeyTransparency { conversation, verified } => {
            if let Some(d) = app.panes.details.as_mut().filter(|d| d.conversation == conversation) {
                d.key_transparency = Some(verified);
            }
        }
        Event::KeyTransparencyFailed => app.panes.key_transparency_failed = true,
        Event::ProfileSaved(result) => match result {
            Ok(()) => {
                app.panes.profile = None;
                app.load_me();
            }
            Err(why) => app.toast(format!("Your profile could not be updated. {why}")),
        },
        Event::UsernameReserved { nickname, discriminator, result } => {
            if let Some(p) = app.panes.profile.as_mut() {
                p.on_reserved(nickname, discriminator, result);
            }
        }
        Event::UsernameSaved(result) => {
            if let Some(p) = app.panes.profile.as_mut() {
                p.on_saved(result);
            }
            app.load_me();
        }
        Event::UsernameLinkReset(result) => match result {
            Ok(()) => app.toast("Your username link was reset."),
            Err(why) => app.toast(format!("Couldn't reset the link: {why}")),
        },
        Event::CaptchaRequired => app.panes.captcha = Some(String::new()),
        Event::Captcha(result) => match result {
            Ok(()) => {
                app.panes.captcha = None;
                app.toast("Verification complete.");
            }
            Err(why) => app.toast(format!("Verification failed: {why}")),
        },
        Event::ServerAlerts(alerts) => app.panes.server_alerts = alerts,
        Event::CriticalAlertModal => app.panes.critical_modal = true,
        Event::ClockSkew(skewed) => app.panes.clock_skew = skewed,
        Event::ViewOnceOpened(_) | Event::Reacted { .. } | Event::PollActivity { .. } => {}
        _ => {}
    }
}

/// A dialog in Desktop's style: a rounded card over a dimmed window.
/// Returns whether it was dismissed (Escape, or a click outside).
pub fn modal(ui: &Ui, id: &str, width: f32, contents: impl FnOnce(&mut Ui)) -> bool {
    let palette = Palette::of(ui.ctx());
    let response = egui::Modal::new(egui::Id::unique(("langis-modal", id)))
        .frame(egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24))
        .show(ui.ctx(), |ui| {
            ui.set_width(width);
            contents(ui);
        });
    response.should_close()
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::semibold(18.)));
    ui.add_space(8.);
}

pub fn body(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::regular(14.)).color(palette.text_secondary));
}

/// Buttons along a dialog's bottom, right to left: the first is primary.
pub fn buttons(ui: &mut Ui, labels: &[&str]) -> Option<usize> {
    let mut clicked = None;
    ui.add_space(16.);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        for (i, label) in labels.iter().enumerate() {
            if widgets::pill_button(ui, label, i == 0).clicked() {
                clicked = Some(i);
            }
            ui.add_space(4.);
        }
    });
    clicked
}

/// A row in a menu or list: an icon and a label, the width of the area.
pub fn menu_row(ui: &mut Ui, icon: Option<Icon>, label: &str, danger: bool) -> egui::Response {
    let palette = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(200.), 36.), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), palette.fill);
    }
    let color = if danger { palette.destructive } else { palette.text };
    let mut x = rect.left() + 10.;
    if let Some(icon) = icon {
        icon.paint(ui, egui::Rect::from_min_size(egui::pos2(x, rect.top() + 8.), egui::vec2(20., 20.)), 18., color);
        x += 30.;
    }
    ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, label, theme::regular(14.), color);
    response
}

/// Everything that floats over the window: dialogs and banners.
pub fn overlays(app: &mut App, ui: &mut Ui) {
    group_link_dialog(app, ui);
    lobby_dialog(app, ui);
    crate::profile::show(app, ui);
    captcha_dialog(app, ui);
    critical_alert(app, ui);
    key_transparency_dialog(app, ui);
    crate::stories::overlays(app, ui);
}

/// Desktop's left pane notices: megaphones and server alerts, above the
/// chat list.
pub fn list_notices(app: &mut App, ui: &mut Ui, palette: &Palette) {
    use langis_core::server_alerts::ServerAlert;
    let mut pressed = None;
    if let Some(m) = app.panes.megaphones.first().cloned() {
        card(ui, palette, |ui| {
            ui.label(RichText::new(&m.title).font(theme::semibold(14.)));
            ui.label(RichText::new(&m.body).font(theme::regular(13.)).color(palette.text_secondary));
            ui.horizontal(|ui| {
                for (cta, label) in [m.secondary.clone(), m.primary.clone()].into_iter().flatten() {
                    if ui.link(RichText::new(label).font(theme::medium(13.))).clicked() {
                        pressed = Some((m.id.clone(), cta));
                    }
                }
            });
        });
    }
    if let Some((id, cta)) = pressed {
        if cta == langis_core::megaphones::MegaphoneCta::Donate {
            crate::chat::open_externally("https://signal.org/donate/");
        }
        app.panes.megaphones.retain(|m| m.id != id);
        app.send(Command::PressMegaphone { id, cta });
    }
    if app.panes.server_alerts.contains_key(&ServerAlert::IdlePrimaryDevice) || app.panes.server_alerts.contains_key(&ServerAlert::CriticalIdlePrimaryDevice) {
        let critical = app.panes.server_alerts.contains_key(&ServerAlert::CriticalIdlePrimaryDevice);
        let mut dismiss = false;
        card(ui, palette, |ui| {
            ui.label(RichText::new("Open Signal on your phone").font(theme::semibold(14.)));
            ui.label(
                RichText::new("Your phone hasn't been used in a while. Open Signal on it to keep this device linked.")
                    .font(theme::regular(13.))
                    .color(palette.text_secondary),
            );
            if !critical && ui.link("Dismiss").clicked() {
                dismiss = true;
            }
        });
        if dismiss {
            app.panes.server_alerts.remove(&ServerAlert::IdlePrimaryDevice);
            app.send(Command::DismissServerAlert(ServerAlert::IdlePrimaryDevice));
        }
    }
    if app.panes.clock_skew {
        card(ui, palette, |ui| {
            ui.label(RichText::new("Your computer's clock is wrong").font(theme::semibold(14.)));
            ui.label(RichText::new("Set it to the correct time, or messages may not send.").font(theme::regular(13.)).color(palette.text_secondary));
        });
    }
}

fn card(ui: &mut Ui, palette: &Palette, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(16.);
        egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width() - 16.);
            ui.vertical(contents);
        });
    });
    ui.add_space(6.);
}

/// Whether a chat belongs in the folder (Desktop's `isConversationInChatFolder`).
pub fn in_folder(folder: &ChatFolder, c: &Conversation) -> bool {
    if folder.excluded.contains(&c.id) {
        return false;
    }
    if !folder.show_muted_chats && c.muted {
        return false;
    }
    if folder.show_only_unread && c.unread_count == 0 && !c.marked_unread {
        return false;
    }
    if folder.folder_type == langis_store::ChatFolderType::All {
        return true;
    }
    folder.included.contains(&c.id)
        || (folder.include_all_individual_chats && matches!(c.kind, ConversationKind::Direct | ConversationKind::NoteToSelf))
        || (folder.include_all_group_chats && c.kind == ConversationKind::Group)
}

/// Desktop's folder tabs above the chat list, when there are folders.
pub fn folder_tabs(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let custom: Vec<ChatFolder> = app.panes.folders.iter().filter(|f| f.folder_type == langis_store::ChatFolderType::Custom).cloned().collect();
    if custom.is_empty() {
        return;
    }
    egui::ScrollArea::horizontal().id_salt("folder-tabs").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(16.);
            let tab = |ui: &mut Ui, label: &str, selected: bool| {
                let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::medium(13.), palette.text);
                let (rect, response) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20., 28.), egui::Sense::click());
                let fill = if selected { palette.fill_pressed } else if response.hovered() { palette.fill } else { egui::Color32::TRANSPARENT };
                ui.painter().rect_filled(rect, CornerRadius::same(14), fill);
                ui.painter().galley(rect.center() - galley.size() / 2., galley, palette.text);
                response.clicked()
            };
            if tab(ui, "All chats", app.panes.folder.is_none()) {
                app.panes.folder = None;
            }
            for f in &custom {
                if tab(ui, &f.name, app.panes.folder.as_ref() == Some(&f.id)) {
                    app.panes.folder = Some(f.id.clone());
                }
            }
        });
    });
    ui.add_space(4.);
}

/// Which chats the list shows: archived or not, in the chosen folder.
pub fn visible(app: &App, c: &Conversation) -> bool {
    if c.archived != app.panes.archived {
        return false;
    }
    match app.panes.folder.as_ref().and_then(|id| app.panes.folders.iter().find(|f| &f.id == id)) {
        Some(folder) => in_folder(folder, c),
        None => true,
    }
}

/// A chat's right-click menu: Desktop's
/// `LeftPaneConversationListItemContextMenu`.
pub fn chat_menu(app: &mut App, m: &mut crate::menu::Menu, c: &Conversation) {
    use langis_core::ChatChange;
    let id = c.id.clone();
    let unread = c.unread_count > 0 || c.marked_unread;
    if unread && m.item(Some("message-check"), "Mark read") {
        app.send(Command::MarkRead { conversation: id.clone() });
        if c.marked_unread {
            app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::MarkedUnread(false) });
        }
    }
    if !unread && m.item(Some("message-badge"), "Mark as unread") {
        app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::MarkedUnread(true) });
    }
    if m.item(Some(if c.pinned { "pin-slash" } else { "pin" }), if c.pinned { "Unpin chat" } else { "Pin chat" }) {
        app.send(Command::SetPinned { conversation: id.clone(), pinned: !c.pinned });
    }
    if c.muted {
        if m.item(Some("bell"), "Unmute") {
            app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::MutedUntil(0) });
        }
    } else {
        m.sub(Some("bell-slash"), "Mute notifications", |m| mute_items(app, m, &id));
    }
    let all_chats = app.panes.folder.is_none();
    let custom: Vec<ChatFolder> = app.panes.folders.iter().filter(|f| f.folder_type == langis_store::ChatFolderType::Custom).cloned().collect();
    if all_chats && !custom.is_empty() {
        m.sub(Some("folder-plus"), "Add to folder", |m| {
            for f in &custom {
                let inside = f.included.contains(&id);
                if m.check(inside, &f.name) {
                    app.send(Command::EditChatFolders(langis_store::ChatFolderEdit::ToggleChat { id: f.id.clone(), chat: id.clone(), add: !inside }));
                }
            }
            if m.item(Some("plus"), "Create folder") {
                let params = langis_store::ChatFolderParams { included: vec![id.clone()], ..langis_store::ChatFolderParams::defaults() };
                app.settings.folder = Some((crate::settings::new_id(), true, params));
                app.settings.page = crate::settings::Page::ChatFolders;
                app.tab = crate::app::Tab::Settings;
            }
        });
    }
    if let Some(folder) = app.panes.folder.clone()
        && m.item(Some("folder-minus"), "Remove from folder")
    {
        app.send(Command::EditChatFolders(langis_store::ChatFolderEdit::ToggleChat { id: folder, chat: id.clone(), add: false }));
    }
    if m.item(Some(if c.archived { "archive-up" } else { "archive" }), if c.archived { "Unarchive" } else { "Archive" }) {
        app.send(Command::SetChat { conversation: id.clone(), change: ChatChange::Archived(!c.archived) });
    }
    if m.item(Some("trash"), "Delete") {
        let member = c.kind == langis_store::ConversationKind::Group && !c.left && !c.ended;
        app.panes.delete_chat = Some((id, member));
    }
}

/// Desktop's `MuteNotificationsMenuItems` for a chat that isn't muted.
pub fn mute_items(app: &mut App, m: &mut crate::menu::Menu, id: &ConversationId) {
    m.label("Mute this chat for…");
    for (label, until) in mute_choices(false) {
        if until == i64::MAX && m.item(None, "Until…") {
            let at = chrono::Local::now() + chrono::Duration::hours(1);
            app.panes.mute_until = Some((id.clone(), at.format("%Y-%m-%d").to_string(), at.format("%H:%M").to_string()));
        }
        if m.item(None, label) {
            app.send(Command::SetChat { conversation: id.clone(), change: langis_core::ChatChange::MutedUntil(until) });
        }
    }
}

/// The chat actions Desktop confirms first (`ConversationDetailsActions`
/// and the header menu).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatConfirm {
    Leave,
    Block,
    Unblock,
    ReportSpam,
    End,
    /// Desktop asks a second time before ending a group.
    EndForSure,
}

/// The open chat confirmation, and what its answer does.
pub fn chat_confirm_dialog(app: &mut App, ctx: &egui::Context) {
    use crate::dialog::{self, Answer, Variant};
    let Some((id, which)) = app.panes.chat_confirm.clone() else { return };
    let Some(c) = app.conversation(&id).cloned() else {
        app.panes.chat_confirm = None;
        return;
    };
    let group = c.kind == ConversationKind::Group;
    let name = c.title.clone();
    let (title, body, actions): (String, &str, Vec<(&str, Variant)>) = match which {
        ChatConfirm::Leave => ("Do you really want to leave?".into(), "You will no longer be able to send or receive messages in this group.", vec![("Leave", Variant::StrongDestructive)]),
        ChatConfirm::Block if group => (format!("Block and Leave the \"{name}\" Group?"), "You will no longer receive messages or updates from this group.", vec![("Block", Variant::StrongDestructive)]),
        ChatConfirm::Block => (format!("Block {name}?"), "Blocked people won't be able to call you or send you messages.", vec![("Block", Variant::StrongDestructive)]),
        ChatConfirm::Unblock if group => (format!("Unblock the \"{name}\" Group?"), "Your contacts will be able to add you to this group.", vec![("Unblock", Variant::StrongDestructive)]),
        ChatConfirm::Unblock => (format!("Unblock {name}?"), "You will be able to message and call each other.", vec![("Unblock", Variant::StrongPrimary)]),
        ChatConfirm::ReportSpam => (
            "Report as spam?".into(),
            if group {
                "Signal will be notified that the person who invited you to this group may be sending spam. Signal can't see the content of any chats."
            } else {
                "Signal will be notified that this person may be sending spam. Signal can\u{2019}t see the content of any chats."
            },
            vec![("Report spam", Variant::StrongDestructive), ("Report and block", Variant::StrongDestructive)],
        ),
        ChatConfirm::End => (
            format!("End \"{name}\"?"),
            "Members will no longer be able to send messages or start calls in the group. They will be notified that you ended the group, and will still have access to message history.",
            vec![("End group", Variant::StrongDestructive)],
        ),
        ChatConfirm::EndForSure => ("This will end the group permanently. Are you sure you want to proceed?".into(), "", vec![("End group", Variant::StrongDestructive)]),
    };
    let key = egui::Id::unique("chat-confirm").with((&id.0, which as u8));
    let Some(answer) = dialog::show(ctx, key, &title, body, Some("Cancel"), &actions) else { return };
    app.panes.chat_confirm = None;
    let Answer::Action(index) = answer else { return };
    let conversation = id.clone();
    match which {
        ChatConfirm::Leave => app.send(Command::LeaveGroup { conversation }),
        ChatConfirm::Block => app.send(Command::SetChat { conversation, change: langis_core::ChatChange::Blocked(true) }),
        ChatConfirm::Unblock => app.send(Command::SetChat { conversation, change: langis_core::ChatChange::Blocked(false) }),
        ChatConfirm::ReportSpam => {
            let answer = if index == 0 { langis_store::MessageRequestResponse::Spam } else { langis_store::MessageRequestResponse::BlockAndSpam };
            app.send(Command::AnswerMessageRequest { conversation, answer });
        }
        ChatConfirm::End => app.panes.chat_confirm = Some((id, ChatConfirm::EndForSure)),
        ChatConfirm::EndForSure => app.send(Command::EndGroup { conversation }),
    }
}

/// Desktop's `DisappearingTimeDialog` units, in seconds, and how many of
/// each it offers (`RANGES`, the end excluded).
const TIME_UNITS: [(&str, u32, u32); 5] = [("Seconds", 1, 60), ("Minutes", 60, 60), ("Hours", 3600, 24), ("Days", 86_400, 7), ("Weeks", 604_800, 5)];

/// A timer as Desktop's dialog starts it: the largest unit it fills.
pub fn custom_timer_start(seconds: u32) -> (u32, usize) {
    let mut out = (1, 0);
    for (i, (_, unit, _)) in TIME_UNITS.iter().enumerate() {
        if seconds < *unit {
            break;
        }
        out = (seconds / unit, i);
    }
    out
}

/// The custom disappearing time and mute until dialogs.
pub fn custom_time_dialogs(app: &mut App, ctx: &egui::Context) {
    use crate::dialog::{self, Answer, Variant};
    if let Some((id, mut value, mut unit)) = app.panes.custom_timer.clone() {
        let answer = dialog::show_with(ctx, egui::Id::unique("custom-timer"), "Custom Time", "Choose a custom time for disappearing messages.", Some("Cancel"), &[("Set", Variant::StrongPrimary)], |ui| {
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8., 0.);
                let (_, _, max) = TIME_UNITS[unit];
                let numbers: Vec<String> = (1..max).map(|n| n.to_string()).collect();
                let options: Vec<&str> = numbers.iter().map(String::as_str).collect();
                if let Some(i) = crate::axo_list::select_pill(ui, egui::Id::unique("custom-timer-value"), 80., &options, (value as usize).saturating_sub(1)) {
                    value = i as u32 + 1;
                }
                let units: Vec<&str> = TIME_UNITS.iter().map(|(n, _, _)| *n).collect();
                if let Some(i) = crate::axo_list::select_pill(ui, egui::Id::unique("custom-timer-unit"), ui.available_width(), &units, unit) {
                    unit = i;
                    value = value.clamp(1, TIME_UNITS[i].2 - 1);
                }
            });
        });
        match answer {
            Some(Answer::Action(_)) => {
                app.send(Command::SetExpireTimer { conversation: id, seconds: value * TIME_UNITS[unit].1 });
                app.panes.custom_timer = None;
            }
            Some(Answer::Cancel) => app.panes.custom_timer = None,
            None => app.panes.custom_timer = Some((id, value, unit)),
        }
    }
    if let Some((id, mut date, mut time)) = app.panes.mute_until.clone() {
        use chrono::TimeZone as _;
        let at = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
            .ok()
            .zip(chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M").ok())
            .and_then(|(d, t)| chrono::Local.from_local_datetime(&d.and_time(t)).single())
            .filter(|at| *at > chrono::Local::now());
        // Desktop's `Intl` zone name, as the system links it.
        let zone = std::env::var("TZ")
            .ok()
            .filter(|z| z.contains('/'))
            .or_else(|| std::fs::read_link("/etc/localtime").ok().and_then(|p| p.to_str().and_then(|p| p.split_once("zoneinfo/")).map(|(_, z)| z.to_owned())))
            .unwrap_or_else(|| chrono::Local::now().format("%:z").to_string());
        let answer = dialog::show_with(ctx, egui::Id::unique("mute-until"), "Mute this chat until…", "", Some("Cancel"), &[("Mute", Variant::StrongPrimary)], |ui| {
            ui.add_space(12.);
            let palette = Palette::of(ui.ctx());
            for (label, value, hint) in [("Date", &mut date, "YYYY-MM-DD"), ("Time", &mut time, "HH:MM")] {
                ui.label(RichText::new(label).font(theme::medium(13.)).color(palette.text));
                ui.add_space(4.);
                ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(ui.available_width()));
                ui.add_space(8.);
            }
            ui.label(RichText::new(format!("All times in {zone}")).font(theme::regular(12.)).color(palette.text_secondary));
        });
        match answer {
            Some(Answer::Action(_)) => match at {
                Some(at) => {
                    app.send(Command::SetChat { conversation: id, change: langis_core::ChatChange::MutedUntil(at.timestamp_millis()) });
                    app.panes.mute_until = None;
                }
                None => {
                    app.toast("Choose a time in the future.");
                    app.panes.mute_until = Some((id, date, time));
                }
            },
            Some(Answer::Cancel) => app.panes.mute_until = None,
            None => app.panes.mute_until = Some((id, date, time)),
        }
    }
}

/// Desktop's `DeleteMessagesConfirmationDialog`, for a chat deleted from a
/// menu.
pub fn delete_chat_dialog(app: &mut App, ctx: &egui::Context) {
    let Some((id, member)) = app.panes.delete_chat.clone() else { return };
    let description = if member {
        "All messages in this chat will be deleted from all your devices. You will still be a member of this group."
    } else {
        "All messages in this chat will be deleted from all your devices."
    };
    match crate::dialog::confirm(ctx, egui::Id::unique("delete-chat"), "Delete chat?", description, "Delete", crate::dialog::Variant::StrongDestructive) {
        Some(crate::dialog::Choice::Action) => {
            app.panes.delete_chat = None;
            if app.panes.details.as_ref().is_some_and(|d| d.conversation == id) {
                app.panes.details = None;
            }
            app.send(Command::DeleteConversation { conversation: id });
        }
        Some(crate::dialog::Choice::Cancel) => app.panes.delete_chat = None,
        None => {}
    }
}

/// Desktop's mute durations; `0` unmutes and `i64::MAX` is "Always".
pub fn mute_choices(muted: bool) -> Vec<(&'static str, i64)> {
    if muted {
        return vec![("Unmute", 0)];
    }
    let now = chrono::Utc::now().timestamp_millis();
    let hour = 3_600_000;
    // getMuteValuesOptions, without "Until…" for now.
    vec![("1 hour", now + hour), ("8 hours", now + 8 * hour), ("1 day", now + 24 * hour), ("1 week", now + 7 * 24 * hour), ("Always", i64::MAX)]
}

/// Desktop's "More actions" menu over the chat list.
pub fn list_menu(app: &mut App, m: &mut crate::menu::Menu) {
    if m.item(Some("archive"), "View Archive") {
        app.panes.archived = true;
    }
    let custom = app.panes.folders.iter().any(|f| f.folder_type == langis_store::ChatFolderType::Custom);
    if !custom && m.item(Some("folder-plus"), "Add chat folder") {
        app.tab = crate::app::Tab::Settings;
        app.settings.page = crate::settings::Page::ChatFolders;
        app.settings.folder = Some((crate::settings::new_id(), true, langis_store::ChatFolderParams::defaults()));
    }
    if custom && m.item(Some("folder-settings"), "Folder settings") {
        app.tab = crate::app::Tab::Settings;
        app.settings.page = crate::settings::Page::ChatFolders;
    }
}

/// Desktop's message request bar, in place of the composer: true when it
/// was shown.
pub fn message_request_bar(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) -> bool {
    let blocked = c.blocked;
    message_request_confirms(app, ui.ctx(), c);
    // Desktop's `GroupV2PendingApprovalActions`: waiting on an admin.
    if matches!(c.group_state.membership, langis_store::Membership::Requesting { .. }) && !c.left && !c.ended {
        egui::Panel::bottom("join-requested")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin::symmetric(24, 16)))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Your request to join has been sent to the group admin. You'll be notified when they take action.").font(theme::regular(13.)).color(palette.text_secondary));
                    ui.add_space(10.);
                    if widgets::pill_button(ui, "Cancel request", false).clicked() {
                        app.panes.confirm_cancel_join = Some(c.id.clone());
                    }
                });
            });
        return true;
    }
    if c.accepted && !blocked && !c.left && !c.ended {
        return false;
    }
    egui::Panel::bottom("message-request")
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin::symmetric(24, 16)))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let group = c.kind == ConversationKind::Group;
                let text = if c.ended {
                    "This group was ended by an admin.".to_owned()
                } else if c.left {
                    "You are no longer a member of this group.".to_owned()
                } else if blocked {
                    if group { "You have blocked this group. Unblock it to send messages." } else { "You have blocked this person. Unblock them to send messages." }
                        .to_owned()
                } else if let langis_store::Membership::Invited { by: Some(by) } = &c.group_state.membership {
                    format!("{} invited you to join this group. Do you want to join? They won't know you've seen their messages until you accept.", app.name(by))
                } else if group {
                    "Do you want to join this group? They won't know you've seen their messages until you accept.".to_owned()
                } else {
                    format!("Let {} message you and share your name and photo with them? They won't know you've seen their messages until you accept.", c.title)
                };
                ui.label(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary));
                if c.left || c.ended {
                    if ui.link("Delete chat").clicked() {
                        app.panes.delete_chat = Some((c.id.clone(), false));
                    }
                    return;
                }
                ui.add_space(10.);
                let answer = |a| Command::AnswerMessageRequest { conversation: c.id.clone(), answer: a };
                ui.horizontal(|ui| {
                    let labels: &[(&str, MessageRequestResponse)] = if blocked {
                        &[("Delete", MessageRequestResponse::Delete), ("Unblock", MessageRequestResponse::Accept)]
                    } else {
                        &[
                            ("Report spam", MessageRequestResponse::Spam),
                            ("Block", MessageRequestResponse::Block),
                            ("Delete", MessageRequestResponse::Delete),
                            ("Accept", MessageRequestResponse::Accept),
                        ]
                    };
                    let width: f32 = labels.iter().map(|(l, _)| l.len() as f32 * 8. + 44.).sum();
                    ui.add_space(((ui.available_width() - width) / 2.).max(0.));
                    for (label, response) in labels {
                        let primary = *response == MessageRequestResponse::Accept;
                        if widgets::pill_button(ui, label, primary).clicked() {
                            // Desktop asks before blocking, deleting or reporting.
                            if primary {
                                app.send(answer(*response));
                            } else {
                                app.panes.confirm_answer = Some((c.id.clone(), *response));
                            }
                        }
                    }
                });
            });
        });
    true
}

/// Desktop's message request confirmations (`MessageRequestActionsConfirmation`,
/// `ReportAndMaybeBlockModal`) and its cancel-join one.
fn message_request_confirms(app: &mut App, ctx: &egui::Context, c: &Conversation) {
    use crate::dialog::{Answer, Variant};
    if app.panes.confirm_cancel_join.as_ref() == Some(&c.id) {
        match crate::dialog::show(ctx, egui::Id::unique("cancel-join"), "Cancel your request to join this group?", "If you cancel, you can ask to join again with the group link.", Some("Not now"), &[("Cancel request", Variant::StrongDestructive)]) {
            Some(Answer::Action(_)) => {
                app.panes.confirm_cancel_join = None;
                app.send(Command::CancelJoinRequest { conversation: c.id.clone() });
            }
            Some(Answer::Cancel) => app.panes.confirm_cancel_join = None,
            None => {}
        }
    }
    let Some((id, answer)) = app.panes.confirm_answer.clone().filter(|(id, _)| *id == c.id) else { return };
    let group = c.kind == ConversationKind::Group;
    let who = c.title.clone();
    let (title, body, actions): (String, String, Vec<(&str, Variant, MessageRequestResponse)>) = match answer {
        MessageRequestResponse::Block => (
            format!("Block {who}?"),
            if group { "You will leave this group and no longer receive its messages or updates.".into() } else { "Blocked people won't be able to call you or send you messages.".into() },
            vec![("Block", Variant::StrongDestructive, MessageRequestResponse::Block)],
        ),
        MessageRequestResponse::Delete if !c.blocked => (
            if group { "Delete and leave group?".into() } else { "Delete chat?".into() },
            if group { "You will leave this group, and its messages will be deleted from all your devices.".into() } else { "This chat will be deleted from all your devices.".into() },
            vec![("Delete", Variant::StrongDestructive, MessageRequestResponse::Delete)],
        ),
        MessageRequestResponse::Delete => (
            "Delete chat?".into(),
            "This chat will be deleted from all your devices.".into(),
            vec![("Delete", Variant::StrongDestructive, MessageRequestResponse::Delete)],
        ),
        MessageRequestResponse::Spam => (
            "Report spam?".into(),
            "Signal will be told this may be spam. Signal can't see the content of any chat.".into(),
            vec![("Report", Variant::SubtleDestructive, MessageRequestResponse::Spam), ("Report and block", Variant::StrongDestructive, MessageRequestResponse::BlockAndSpam)],
        ),
        other => {
            app.panes.confirm_answer = None;
            app.send(Command::AnswerMessageRequest { conversation: id, answer: other });
            return;
        }
    };
    let buttons: Vec<(&str, Variant)> = actions.iter().map(|(l, v, _)| (*l, *v)).collect();
    match crate::dialog::show(ctx, egui::Id::unique("message-request-confirm"), &title, &body, Some("Cancel"), &buttons) {
        Some(Answer::Action(i)) => {
            app.panes.confirm_answer = None;
            app.send(Command::AnswerMessageRequest { conversation: id, answer: actions[i].2 });
        }
        Some(Answer::Cancel) => app.panes.confirm_answer = None,
        None => {}
    }
}

/// Desktop's `isProbablyAUsername`.
fn probably_username(text: &str) -> bool {
    let t = text.trim();
    if t.starts_with('@') {
        return true;
    }
    let like = t.len() >= 3 && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if !like || t.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    t.rsplit_once('.').is_some_and(|(name, digits)| !name.is_empty() && (2..=3).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()))
}

fn probably_phone(text: &str) -> bool {
    let t = text.trim();
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    digits >= 3 && t.chars().all(|c| c.is_ascii_digit() || " -()+.".contains(c)) && !t[1..].contains('+')
}

/// A `ListTile` in the compose pane: 52 high (48 for a step button),
/// inset 10 by 2, rounded 12.
fn tile(ui: &mut Ui, palette: &Palette, height: f32, id: egui::Id) -> (Rect, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let inner = rect.shrink2(Vec2::new(10., 2.));
    let response = ui.interact(inner, id, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(inner, CornerRadius::same(12), if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill });
    }
    (inner, response)
}

/// `ComposeStepButton`: a 32px circle with a 20px symbol, and a title.
fn step_button(ui: &mut Ui, palette: &Palette, symbol: &str, label: &str) -> bool {
    let (inner, response) = tile(ui, palette, 48., egui::Id::unique("compose-step").with(label));
    let center = Pos2::new(inner.left() + 14. + 16., inner.center().y);
    let circle = if palette.dark { Color32::from_white_alpha(31) } else { Color32::from_black_alpha(15) };
    ui.painter().circle_filled(center, 16., circle);
    theme::paint_symbol(ui.painter(), symbol, center, 20., palette.text);
    ui.painter().text(Pos2::new(center.x + 16. + 12., center.y), egui::Align2::LEFT_CENTER, label, theme::regular(14.), palette.text);
    response.clicked()
}

fn conversation_tile(ui: &mut Ui, palette: &Palette, c: &langis_store::Conversation) -> bool {
    let (inner, response) = tile(ui, palette, 52., egui::Id::unique("compose-row").with(&c.id.0));
    let avatar = Rect::from_min_size(Pos2::new(inner.left() + 14., inner.center().y - 18.), Vec2::splat(36.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
    let title = if c.kind == ConversationKind::NoteToSelf { "Note to Self".to_owned() } else { c.title.clone() };
    let g = ui.painter().layout_no_wrap(title, theme::regular(14.), palette.text);
    let clip = Rect::from_min_max(Pos2::new(avatar.right() + 12., inner.top()), Pos2::new(inner.right() - 14., inner.bottom()));
    ui.painter().with_clip_rect(clip).galley(Pos2::new(clip.left(), inner.center().y - g.size().y / 2.), g, palette.text);
    response.clicked()
}

fn list_header(ui: &mut Ui, palette: &Palette, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 24., r.center().y), egui::Align2::LEFT_CENTER, text, theme::semibold(14.), palette.text);
}

/// The left pane's footer: one button at the end.
fn footer_button(ui: &mut Ui, label: &str, enabled: bool, then: impl FnOnce(bool)) {
    ui.add_space(14.);
    let clicked = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.);
                ui.add_enabled_ui(enabled, |ui| widgets::pill_button(ui, label, true).clicked()).inner
            })
            .inner
        })
        .inner;
    then(clicked && enabled);
}

/// A `ListTile.checkbox`'s circle: Desktop's 20px round checkbox.
pub fn round_check(ui: &Ui, palette: &Palette, center: Pos2, on: bool) {
    if on {
        ui.painter().circle_filled(center, 10., theme::rgb(0x2c6bed));
        theme::paint_symbol(ui.painter(), "check", center, 14., Color32::WHITE);
    } else {
        ui.painter().circle_stroke(center, 9.5, egui::Stroke::new(1.5, palette.text_secondary));
    }
}

/// Desktop's "Choose members" step. True on Next or Skip.
fn choose_members(app: &mut App, ui: &mut Ui, palette: &Palette, query: &mut String, members: &mut Vec<String>, keep: &mut bool) -> bool {
    ui.add_space(12.);
    ui.horizontal(|ui| {
        ui.add_space(12.);
        if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *keep = false;
        }
        ui.label(RichText::new("Choose members").font(theme::semibold(18.)));
    });
    ui.add_space(6.);
    let field = ui.horizontal(|ui| {
        ui.add_space(16.);
        ui.add(egui::TextEdit::singleline(query).hint_text("Name, username, or number").desired_width(ui.available_width() - 16.))
    });
    if ui.ctx().memory(|m| m.focused().is_none()) {
        field.inner.request_focus();
    }
    let enter = field.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(8.);
    // `ContactPills`: the chosen, 8 by 12 apart, wrapping.
    if !members.is_empty() {
        let mut remove = None;
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 4, bottom: 4 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12., 8.);
            ui.horizontal_wrapped(|ui| {
                for m in members.iter() {
                    let name = app.name(m);
                    let g = ui.painter().layout_no_wrap(name.clone(), theme::regular(13.), palette.text);
                    let size = Vec2::new(8. + 20. + 6. + g.size().x.min(140.) + 6. + 16. + 8., 28.);
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    let fill = if palette.dark { Color32::from_white_alpha(31) } else { Color32::from_black_alpha(20) };
                    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
                    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 8., rect.center().y - 10.), Vec2::splat(20.));
                    let color = m.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::person(&name, color));
                    let clip = Rect::from_min_max(Pos2::new(avatar.right() + 6., rect.top()), Pos2::new(rect.right() - 8. - 16. - 6., rect.bottom()));
                    ui.painter().with_clip_rect(clip).galley(Pos2::new(clip.left(), rect.center().y - g.size().y / 2.), g, palette.text);
                    let x = Rect::from_center_size(Pos2::new(rect.right() - 8. - 8., rect.center().y), Vec2::splat(16.));
                    let r = ui.interact(x, egui::Id::unique("contact-pill").with(m), Sense::click());
                    theme::paint_symbol(ui.painter(), "x", x.center(), 14., palette.text);
                    if r.on_hover_text(format!("Remove {name}")).clicked() {
                        remove = Some(m.clone());
                    }
                }
            });
        });
        if let Some(m) = remove {
            members.retain(|x| *x != m);
        }
        ui.add_space(10.);
    }
    let q = query.trim().to_lowercase();
    let username = probably_username(&q);
    let phone = !username && probably_phone(&q);
    let mut people: Vec<_> = app.conversations.iter().filter(|c| c.kind == ConversationKind::Direct && !c.blocked && (q.is_empty() || c.title.to_lowercase().contains(&q))).cloned().collect();
    people.sort_by_key(|c| c.title.to_lowercase());
    let footer = 64.;
    let body_h = (ui.available_height() - footer).max(0.);
    egui::ScrollArea::vertical().max_height(body_h).min_scrolled_height(body_h).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if !people.is_empty() && !username {
            list_header(ui, palette, "Contacts");
        }
        for c in people.iter().filter(|_| !username) {
            let Some(sid) = c.id.as_direct() else { continue };
            let chosen = members.iter().any(|m| m == sid);
            let (inner, response) = tile(ui, palette, 52., egui::Id::unique("choose-member").with(&c.id.0));
            let avatar = Rect::from_min_size(Pos2::new(inner.left() + 14., inner.center().y - 18.), Vec2::splat(36.));
            widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
            ui.painter().text(Pos2::new(avatar.right() + 12., inner.center().y), egui::Align2::LEFT_CENTER, &c.title, theme::regular(14.), palette.text);
            round_check(ui, palette, Pos2::new(inner.right() - 14. - 10., inner.center().y), chosen);
            if response.clicked() {
                if chosen {
                    members.retain(|m| m != sid);
                } else {
                    members.push(sid.to_owned());
                }
            }
        }
        if username || phone {
            list_header(ui, palette, if username { "Find by username" } else { "Find by phone number" });
            let shown = query.trim().trim_start_matches('@').to_owned();
            if step_button(ui, palette, if username { "at" } else { "number" }, &shown) || enter {
                app.send(Command::FindRecipient { query: query.trim().to_owned() });
            }
        }
        if people.is_empty() && !username && !phone {
            ui.add_space(28.);
            ui.vertical_centered(|ui| body(ui, palette, "No contacts found"));
        }
    });
    let mut done = false;
    footer_button(ui, if members.is_empty() { "Skip" } else { "Next" }, true, |clicked| done = clicked);
    done
}

/// Desktop's `LeftPaneComposeHelper` and its find steps, in place of the
/// chat list. False when there's no compose step open.
pub fn compose_pane(app: &mut App, ui: &mut Ui) -> bool {
    let Some(mut compose) = app.panes.compose.take() else { return false };
    let palette = Palette::of(ui.ctx());
    let mut keep = true;
    let mut next: Option<Compose> = None;
    let mut open: Option<ConversationId> = None;
    match &mut compose {
        Compose::Group { picking: true, query, members, .. } => {
            if choose_members(app, ui, &palette, query, members, &mut keep) {
                if let Compose::Group { picking, .. } = &mut compose {
                    *picking = false;
                }
            }
            if !keep {
                next = Some(Compose::Chat { query: String::new(), searching: false, found: None });
                keep = true;
            }
        }
        Compose::Group { picking, members, title: name, timer, avatar, .. } => {
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(12.);
                if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back to member selection").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    *picking = true;
                }
                ui.label(RichText::new("Name this group").font(theme::semibold(18.)));
            });
            let footer = 64.;
            let body_h = (ui.available_height() - footer).max(0.);
            egui::ScrollArea::vertical().max_height(body_h).min_scrolled_height(body_h).auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(16.);
                // `AvatarPreview`, 80px, choosing a photo on click.
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.), Sense::hover());
                let rect = Rect::from_center_size(r.center(), Vec2::splat(80.));
                let response = ui.interact(rect, egui::Id::unique("new-group-photo"), Sense::click());
                match avatar.as_ref() {
                    Some(bytes) => {
                        egui::Image::from_bytes("bytes://new-group-photo", bytes.clone()).corner_radius(CornerRadius::same(40)).paint_at(ui, rect);
                    }
                    None => {
                        ui.painter().circle_filled(rect.center(), 40., palette.fill_secondary);
                        theme::paint_symbol(ui.painter(), "camera", rect.center(), 32., palette.text_secondary);
                    }
                }
                if response.on_hover_text("Choose a photo").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file()
                {
                    *avatar = std::fs::read(path).ok();
                }
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    ui.add_space(16.);
                    ui.add(egui::TextEdit::singleline(name).hint_text("Group name (required)").desired_width(ui.available_width() - 16.)).request_focus();
                });
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    ui.add_space(16.);
                    ui.label(RichText::new("Disappearing messages").font(theme::regular(14.)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.);
                        crate::details::timer_picker(ui, "new-group-timer", timer);
                    });
                });
                ui.add_space(8.);
                list_header(ui, &palette, "Members");
                for m in members.iter() {
                    let (inner, _) = tile(ui, &palette, 52., egui::Id::unique("new-group-member").with(m));
                    let avatar = Rect::from_min_size(Pos2::new(inner.left() + 14., inner.center().y - 18.), Vec2::splat(36.));
                    let name = app.name(m);
                    let color = m.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::person(&name, color));
                    ui.painter().text(Pos2::new(avatar.right() + 12., inner.center().y), egui::Align2::LEFT_CENTER, name, theme::regular(14.), palette.text);
                }
            });
            let ready = !name.trim().is_empty();
            footer_button(ui, "Create", ready, |create| {
                if create {
                    app.send(Command::CreateGroup { title: name.trim().to_owned(), members: members.clone(), avatar: avatar.clone(), expire_timer: *timer });
                }
            });
        }
        Compose::Chat { query, searching, found } => {
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(12.);
                if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back to inbox").clicked() {
                    keep = false;
                }
                ui.label(RichText::new("New chat").font(theme::semibold(18.)));
            });
            ui.add_space(6.);
            let field = ui.horizontal(|ui| {
                ui.add_space(16.);
                ui.add(egui::TextEdit::singleline(query).hint_text("Name, username, or number").desired_width(ui.available_width() - 16.))
            });
            if !ui.ctx().memory(|m| m.has_focus(field.inner.id)) && ui.ctx().memory(|m| m.focused().is_none()) {
                field.inner.request_focus();
            }
            if field.inner.changed() {
                *searching = false;
                *found = None;
            }
            let enter = field.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                keep = false;
            }
            ui.add_space(8.);
            let q = query.trim().to_lowercase();
            let username = probably_username(&q);
            let phone = !username && probably_phone(&q);
            let link = signal_link(query.trim());
            let matches = |c: &&langis_store::Conversation| q.is_empty() || c.title.to_lowercase().contains(&q) || c.phone.as_deref().is_some_and(|p| p.contains(&q));
            let mut contacts: Vec<_> = app.conversations.iter().filter(|c| c.kind != ConversationKind::Group && !c.blocked && matches(c)).cloned().collect();
            contacts.sort_by_key(|c| c.title.to_lowercase());
            let mut groups: Vec<_> = app.conversations.iter().filter(|c| c.kind == ConversationKind::Group && !c.left && !c.ended && matches(c)).cloned().collect();
            groups.sort_by_key(|c| c.title.to_lowercase());
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if q.is_empty() {
                    if step_button(ui, &palette, "group", "New group") {
                        next = Some(Compose::Group { picking: true, query: String::new(), members: Vec::new(), title: String::new(), timer: 0, avatar: None });
                    }
                    if step_button(ui, &palette, "at", "Find by username") {
                        next = Some(Compose::Find { phone: false, query: String::new(), searching: false, found: None });
                    }
                    if step_button(ui, &palette, "number", "Find by phone number") {
                        next = Some(Compose::Find { phone: true, query: String::new(), searching: false, found: None });
                    }
                }
                // A pasted group or call link opens it.
                if let Some((label, command)) = &link
                    && (step_button(ui, &palette, "link", label) || enter)
                {
                    app.send(command.clone());
                    keep = false;
                }
                if !username {
                    if !contacts.is_empty() {
                        list_header(ui, &palette, "Contacts");
                        for c in &contacts {
                            if conversation_tile(ui, &palette, c) {
                                open = Some(c.id.clone());
                            }
                        }
                    }
                    if !groups.is_empty() {
                        list_header(ui, &palette, "Groups");
                        for c in &groups {
                            if conversation_tile(ui, &palette, c) {
                                open = Some(c.id.clone());
                            }
                        }
                    }
                }
                if (username || phone) && link.is_none() {
                    list_header(ui, &palette, if username { "Find by username" } else { "Find by phone number" });
                    let shown = if username { query.trim().trim_start_matches('@').to_owned() } else { query.trim().to_owned() };
                    if step_button(ui, &palette, if username { "at" } else { "number" }, &shown) || (enter && !*searching) {
                        *searching = true;
                        *found = None;
                        app.send(Command::FindRecipient { query: query.trim().to_owned() });
                    }
                    match found {
                        Some(Ok(entry)) => open = Some(entry.id.clone()),
                        Some(Err(_)) => {
                            let why = if username {
                                format!("@{shown} is not a Signal user. Make sure you've entered the complete username.")
                            } else {
                                format!("User not found. \"{shown}\" is not a Signal user.")
                            };
                            ui.add_space(8.);
                            egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 16, top: 0, bottom: 0 }).show(ui, |ui| {
                                body(ui, &palette, &why);
                            });
                        }
                        None if *searching => {
                            ui.add_space(8.);
                            ui.vertical_centered(|ui| ui.spinner());
                        }
                        None => {}
                    }
                }
                if !q.is_empty() && contacts.is_empty() && groups.is_empty() && !username && !phone && link.is_none() {
                    ui.add_space(28.);
                    ui.vertical_centered(|ui| body(ui, &palette, "No chats found"));
                }
            });
        }
        Compose::Find { phone, query, searching, found } => {
            let heading = if *phone { "Find by phone number" } else { "Find by username" };
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(12.);
                if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    next = Some(Compose::Chat { query: String::new(), searching: false, found: None });
                }
                ui.label(RichText::new(heading).font(theme::semibold(18.)));
            });
            ui.add_space(6.);
            let field = ui.horizontal(|ui| {
                ui.add_space(16.);
                ui.add(egui::TextEdit::singleline(query).hint_text(if *phone { "Phone number" } else { "Username" }).desired_width(ui.available_width() - 16.))
            });
            if ui.ctx().memory(|m| m.focused().is_none()) {
                field.inner.request_focus();
            }
            if field.inner.changed() {
                *found = None;
            }
            let enter = field.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !*phone {
                ui.add_space(6.);
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 0 }).show(ui, |ui| {
                    body(ui, &palette, "Enter a username followed by a dot and its set of numbers.");
                });
            }
            if let Some(Err(_)) = found {
                let shown = query.trim().trim_start_matches('@');
                let why = if *phone { format!("User not found. \"{shown}\" is not a Signal user.") } else { format!("@{shown} is not a Signal user. Make sure you've entered the complete username.") };
                ui.add_space(8.);
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 0 }).show(ui, |ui| {
                    ui.label(RichText::new(why).font(theme::regular(13.)).color(palette.destructive));
                });
            }
            if let Some(Ok(entry)) = found {
                open = Some(entry.id.clone());
            }
            ui.add_space(16.);
            ui.horizontal(|ui| {
                ui.add_space(16.);
                let ready = !query.trim().is_empty() && !*searching;
                let label = if *searching { "…" } else { "Next" };
                let clicked = ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, label, true).clicked()).inner;
                if (clicked || enter) && ready {
                    *searching = true;
                    *found = None;
                    let q = if *phone { query.trim().to_owned() } else { format!("@{}", query.trim().trim_start_matches('@')) };
                    *query = q.clone();
                    app.send(Command::FindRecipient { query: q });
                }
            });
        }
    }
    if let Some(id) = open {
        app.open(id);
        app.tab = Tab::Chats;
        keep = false;
    }
    if let Some(n) = next {
        app.panes.compose = Some(n);
    } else if keep {
        app.panes.compose = Some(compose);
    }
    true
}

/// A person in a list: avatar, name and (if known) number.
pub fn contact_row(ui: &mut Ui, palette: &Palette, name: &str, phone: Option<&str>, color: u8, photo: Option<&str>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(200.), 48.), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
    }
    let avatar = egui::Rect::from_min_size(rect.min + egui::vec2(8., 6.), egui::vec2(36., 36.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { name, color, photo, group: false, note_to_self: false });
    let x = avatar.right() + 12.;
    if let Some(phone) = phone {
        ui.painter().text(egui::pos2(x, rect.top() + 8.), egui::Align2::LEFT_TOP, name, theme::medium(14.), palette.text);
        ui.painter().text(egui::pos2(x, rect.top() + 27.), egui::Align2::LEFT_TOP, phone, theme::regular(12.), palette.text_secondary);
    } else {
        ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, name, theme::medium(14.), palette.text);
    }
    response
}

fn group_link_dialog(app: &mut App, ui: &mut Ui) {
    let Some(preview) = app.panes.group_link.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let mut answer = None;
    let closed = modal(ui, "group-link", 360., |ui| {
        ui.vertical_centered(|ui| {
            let photo = preview.avatar.as_ref().map(|p| p.to_string_lossy().into_owned());
            widgets::avatar(ui, &widgets::AvatarSpec { name: &preview.title, color: 4, photo: photo.as_deref(), group: true, note_to_self: false }, 80.);
            ui.add_space(8.);
            ui.label(RichText::new(&preview.title).font(theme::semibold(18.)));
            body(ui, &palette, &format!("Group · {} members", preview.member_count));
            if let Some(d) = &preview.description {
                ui.add_space(6.);
                body(ui, &palette, d);
            }
            if preview.approval {
                ui.add_space(6.);
                body(ui, &palette, "An admin of this group must approve your request before you can join.");
            }
        });
        answer = buttons(ui, &[if preview.approval { "Request to join" } else { "Join" }, "Cancel"]);
    });
    match answer {
        Some(0) => app.send(Command::JoinGroupLink { url: preview.url }),
        Some(_) => app.panes.group_link = None,
        None if closed => app.panes.group_link = None,
        None => {}
    }
}

/// Desktop's `CallingLobby`: who's there, our camera choice, Join.
fn lobby_dialog(app: &mut App, ui: &mut Ui) {
    let Some(lobby) = app.panes.lobby.as_ref() else { return };
    if app.call.is_some() {
        app.panes.lobby = None;
        return;
    }
    let palette = Palette::of(ui.ctx());
    let conversation = lobby.conversation.clone();
    let users = lobby.users.clone();
    let mut video = lobby.video;
    let title_text = match conversation.as_call_link() {
        Some(room) => app.panes.links.iter().find(|l| l.room_id == room).map(|l| if l.name.is_empty() { "Signal call".to_owned() } else { l.name.clone() }).unwrap_or_else(|| "Signal call".into()),
        None => app.conversation(&conversation).map(|c| c.title.clone()).unwrap_or_default(),
    };
    let mut answer = None;
    let closed = modal(ui, "lobby", 380., |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(&title_text).font(theme::semibold(18.)));
            ui.add_space(6.);
            let who = match users.len() {
                0 => "No one else is here".to_owned(),
                1 => format!("{} is in this call", app.name(&users[0])),
                2 => format!("{} and {} are in this call", app.name(&users[0]), app.name(&users[1])),
                n => format!("{}, {} and {} others are in this call", app.name(&users[0]), app.name(&users[1]), n - 2),
            };
            body(ui, &palette, &who);
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.label("Camera");
                widgets::switch(ui, &mut video);
            });
        });
        answer = buttons(ui, &[if users.is_empty() { "Start call" } else { "Join call" }, "Cancel"]);
    });
    if let Some(l) = app.panes.lobby.as_mut() {
        l.video = video;
    }
    match answer {
        Some(0) => {
            app.call_view.video_on = Some(video);
            app.send(Command::StartCall { conversation, video });
            app.panes.lobby = None;
        }
        Some(_) => app.panes.lobby = None,
        None if closed => app.panes.lobby = None,
        None => {}
    }
}

/// Opens the lobby for a group's call (Desktop shows it before joining).
pub fn open_lobby(app: &mut App, conversation: ConversationId, video: bool) {
    app.send(Command::PeekGroupCall { conversation: conversation.clone() });
    app.panes.lobby = Some(Lobby { conversation, users: Vec::new(), video });
}

/// Desktop's captcha dialog: the challenge opens in the browser, which
/// hands back a `signalcaptcha://` link to paste here.
fn captcha_dialog(app: &mut App, ui: &mut Ui) {
    let Some(mut token) = app.panes.captcha.take() else { return };
    let palette = Palette::of(ui.ctx());
    let mut answer = None;
    modal(ui, "captcha", 400., |ui| {
        title(ui, "Verification required");
        body(ui, &palette, "To help prevent spam, Signal needs you to complete a verification before sending. Open the challenge, solve it, then paste the signalcaptcha:// link it gives you here.");
        ui.add_space(8.);
        if ui.link("Open the challenge").clicked() {
            crate::chat::open_externally("https://signalcaptchas.org/challenge/generate.html");
        }
        ui.add_space(8.);
        widgets::text_field(ui, &mut token, "signalcaptcha://…", false);
        answer = buttons(ui, &["Continue", "Skip verification"]);
    });
    match answer {
        Some(0) if !token.trim().is_empty() => {
            app.send(Command::SubmitCaptcha(token.trim().to_owned()));
            app.panes.captcha = Some(token);
        }
        Some(1) => app.send(Command::SkipCaptcha),
        _ => app.panes.captcha = Some(token),
    }
}

fn critical_alert(app: &mut App, ui: &mut Ui) {
    if !app.panes.critical_modal {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let mut ok = false;
    modal(ui, "critical-alert", 380., |ui| {
        title(ui, "Open Signal on your phone");
        body(ui, &palette, "Your phone hasn't been used in a long time. Open Signal on it soon, or this device will be unlinked.");
        ok = buttons(ui, &["OK"]).is_some();
    });
    if ok {
        app.panes.critical_modal = false;
        app.send(Command::DismissServerAlert(langis_core::server_alerts::ServerAlert::CriticalIdlePrimaryDevice));
    }
}

fn key_transparency_dialog(app: &mut App, ui: &mut Ui) {
    if !app.panes.key_transparency_failed {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let mut ok = false;
    modal(ui, "kt-failed", 380., |ui| {
        title(ui, "Automatic Key Verification is currently unavailable");
        body(ui, &palette, "You can still verify safety numbers by comparing them with your contacts in person.");
        ok = buttons(ui, &["OK"]).is_some();
    });
    if ok {
        app.panes.key_transparency_failed = false;
    }
}

/// Opens a story list editor's choices: who we know, by name.
pub fn people(app: &App) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = app
        .conversations
        .iter()
        .filter(|c| c.kind == ConversationKind::Direct)
        .filter_map(|c| c.id.as_direct().map(|s| (s.to_owned(), c.title.clone())))
        .collect();
    list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    list
}

/// A group link (`https://signal.group/#…`, or `sgnl://signal.group/…`) or
/// a call link (`https://signal.link/call/#key=…`), with what opening it
/// shows and the command that opens it.
fn signal_link(text: &str) -> Option<(&'static str, Command)> {
    let rest = text.strip_prefix("https://").or_else(|| text.strip_prefix("sgnl://"))?;
    let url = text.to_owned();
    if rest.starts_with("signal.group/") {
        Some(("Open group link", Command::OpenGroupLink { url }))
    } else if rest.starts_with("signal.link/call") {
        Some(("Open call link", Command::OpenCallLink { url }))
    } else {
        None
    }
}

#[cfg(test)]
mod link_tests {
    #[test]
    fn group_and_call_links_are_told_apart() {
        assert_eq!(super::signal_link("https://signal.group/#CjQKIA").map(|l| l.0), Some("Open group link"));
        assert_eq!(super::signal_link("sgnl://signal.group/#CjQKIA").map(|l| l.0), Some("Open group link"));
        assert_eq!(super::signal_link("https://signal.link/call/#key=bcdf").map(|l| l.0), Some("Open call link"));
        assert!(super::signal_link("https://example.com/signal.group/").is_none());
        assert!(super::signal_link("+15555550123").is_none());
    }
}


#[cfg(test)]
mod compose_tests {
    #[test]
    fn username_and_phone_guesses() {
        assert!(super::probably_username("@maya"));
        assert!(super::probably_username("maya.42"));
        assert!(!super::probably_username("maya"));
        assert!(!super::probably_username("12345"));
        assert!(super::probably_phone("+1 555 0100"));
        assert!(super::probably_phone("(555) 010-0100"));
        assert!(!super::probably_phone("ma"));
        assert!(!super::probably_phone("5+5"));
    }
}
