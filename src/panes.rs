//! What sits around the chats: the state of the Calls and Stories tabs,
//! chat details, the new chat and new group dialogs, chat list menus and
//! folders, message requests, and the account's alerts (megaphones,
//! server alerts, captcha). Events the app doesn't handle itself come here.

use std::time::Instant;

use egui::{CornerRadius, RichText, Ui};
use langis_core::{CallReaction, Command, Event, GroupLinkPreview};
use langis_store::{CallLink, ChatFolder, Conversation, ConversationId, ConversationKind, DirectoryEntry, Message, MessageRequestResponse};

use crate::app::{App, Loaded, Tab};
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

#[derive(Default)]
pub struct Panes {
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
                lobby.users = users;
            }
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
            if let Some(Compose::Chat { query: q, searching, found }) = app.panes.compose.as_mut()
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
    compose_dialog(app, ui);
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
    if c.unread_count > 0 && m.item(Some("message-check"), "Mark read") {
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
                } else if group {
                    "Do you want to join this group? They won't know you've seen their messages until you accept.".to_owned()
                } else {
                    format!("Let {} message you and share your name and photo with them? They won't know you've seen their messages until you accept.", c.title)
                };
                ui.label(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary));
                if c.left || c.ended {
                    if ui.link("Delete chat").clicked() {
                        app.send(Command::DeleteConversation { conversation: c.id.clone() });
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
                            app.send(answer(*response));
                        }
                    }
                });
            });
        });
    true
}

fn compose_dialog(app: &mut App, ui: &mut Ui) {
    let Some(mut compose) = app.panes.compose.take() else { return };
    let palette = Palette::of(ui.ctx());
    let mut keep = true;
    match &mut compose {
        Compose::Chat { query, searching, found } => {
            let mut open = None;
            let mut opened_link = None;
            let closed = modal(ui, "new-chat", 380., |ui| {
                title(ui, "New chat");
                let field = widgets::text_field(ui, query, "Name, username or number", false);
                field.request_focus();
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(8.);
                let q = query.trim().to_lowercase();
                egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                    if menu_row(ui, Some(Icon::Group), "New group", false).clicked() {
                        open = Some(None);
                    }
                    for c in app.conversations.iter().filter(|c| c.kind != ConversationKind::Group && !q.is_empty() && c.title.to_lowercase().contains(&q)).take(20) {
                        if contact_row(ui, &palette, &c.title, c.phone.as_deref(), c.color, c.avatar_path.as_deref()).clicked() {
                            open = Some(Some(c.id.clone()));
                        }
                    }
                    match found {
                        Some(Ok(entry)) => {
                            if contact_row(ui, &palette, &entry.title, entry.phone.as_deref(), 0, None).clicked() {
                                open = Some(Some(entry.id.clone()));
                            }
                        }
                        Some(Err(why)) => body(ui, &palette, why),
                        None if *searching => {
                            ui.spinner();
                        }
                        None => {}
                    }
                });
                // A pasted group or call link opens it (Desktop opens these
                // from the system; here they're pasted).
                let link = signal_link(query.trim());
                let is_link = link.is_some();
                if let Some((label, command)) = link {
                    ui.add_space(6.);
                    if menu_row(ui, Some(Icon::Link), label, false).clicked() || enter {
                        opened_link = Some(command);
                    }
                }
                let lookup = !is_link && !q.is_empty() && (q.starts_with('+') || q.contains('.') || q.chars().all(|ch| ch.is_ascii_digit() || " -()".contains(ch)));
                if lookup && !*searching {
                    ui.add_space(6.);
                    let label = if q.starts_with('+') || q.chars().next().is_some_and(|ch| ch.is_ascii_digit()) { "Find by phone number" } else { "Find by username" };
                    if menu_row(ui, Some(Icon::SearchCompactLight), label, false).clicked() || enter {
                        *searching = true;
                        *found = None;
                        app.send(Command::FindRecipient { query: query.trim().to_owned() });
                    }
                }
                ui.add_space(16.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, "Cancel", false).clicked() {
                        keep = false;
                    }
                });
            });
            if closed {
                keep = false;
            }
            if let Some(command) = opened_link {
                app.send(command);
                keep = false;
            }
            match open {
                Some(Some(id)) => {
                    keep = false;
                    app.open(id);
                    app.tab = Tab::Chats;
                }
                Some(None) => {
                    compose = Compose::Group { picking: true, query: String::new(), members: Vec::new(), title: String::new(), timer: 0, avatar: None };
                }
                None => {}
            }
        }
        Compose::Group { picking, query, members, title: name, timer, avatar } => {
            let closed = modal(ui, "new-group", 420., |ui| {
                if *picking {
                    title(ui, "Add members");
                    widgets::text_field(ui, query, "Name, username or number", false);
                    ui.add_space(6.);
                    // Chosen people, as chips.
                    ui.horizontal_wrapped(|ui| {
                        let mut remove = None;
                        for m in members.iter() {
                            if ui.button(format!("{}  ×", app.name(m))).clicked() {
                                remove = Some(m.clone());
                            }
                        }
                        if let Some(m) = remove {
                            members.retain(|x| *x != m);
                        }
                    });
                    let q = query.trim().to_lowercase();
                    egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                        let people = app.conversations.iter().filter(|c| c.kind == ConversationKind::Direct && (q.is_empty() || c.title.to_lowercase().contains(&q)));
                        for c in people.take(60) {
                            let Some(sid) = c.id.as_direct() else { continue };
                            let chosen = members.iter().any(|m| m == sid);
                            let row = ui.horizontal(|ui| {
                                let mut on = chosen;
                                ui.checkbox(&mut on, "");
                                contact_row(ui, &palette, &c.title, c.phone.as_deref(), c.color, c.avatar_path.as_deref())
                            });
                            if row.inner.clicked() || row.response.clicked() {
                                if chosen {
                                    members.retain(|m| m != sid);
                                } else {
                                    members.push(sid.to_owned());
                                }
                            }
                        }
                        if !q.is_empty() && (q.starts_with('+') || q.contains('.')) && menu_row(ui, Some(Icon::SearchCompactLight), "Find and add", false).clicked() {
                            app.send(Command::FindRecipient { query: query.trim().to_owned() });
                        }
                    });
                    match buttons(ui, &[if members.is_empty() { "Skip" } else { "Next" }, "Cancel"]) {
                        Some(0) => *picking = false,
                        Some(_) => keep = false,
                        None => {}
                    }
                } else {
                    title(ui, "Name this group");
                    ui.horizontal(|ui| {
                        let spec = widgets::AvatarSpec { name: name.as_str(), color: 3, photo: None, group: true, note_to_self: false };
                        if widgets::avatar(ui, &spec, 64.).on_hover_text("Choose a photo").clicked()
                            && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file()
                        {
                            *avatar = std::fs::read(path).ok();
                        }
                        if avatar.is_some() {
                            ui.label(RichText::new("Photo chosen").color(palette.text_secondary));
                        }
                    });
                    ui.add_space(8.);
                    widgets::text_field(ui, name, "Group name (required)", false);
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        ui.label("Disappearing messages");
                        crate::details::timer_picker(ui, "new-group-timer", timer);
                    });
                    ui.add_space(8.);
                    body(ui, &palette, &format!("{} members", members.len() + 1));
                    match buttons(ui, &["Create", "Back"]) {
                        Some(0) if !name.trim().is_empty() => {
                            app.send(Command::CreateGroup { title: name.trim().to_owned(), members: members.clone(), avatar: avatar.clone(), expire_timer: *timer });
                        }
                        Some(0) => app.toast("Give the group a name."),
                        Some(_) => *picking = true,
                        None => {}
                    }
                }
            });
            if closed {
                keep = false;
            }
        }
    }
    if keep && app.panes.compose.is_none() {
        app.panes.compose = Some(compose);
    }
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

