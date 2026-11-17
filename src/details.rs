//! Desktop's `ConversationDetails`: the pane that replaces a chat when its
//! header is clicked. Members, admins, permissions, the group link and
//! requests, the disappearing timer, mute, block, safety numbers, the
//! nickname, and leaving or deleting.

use egui::{CornerRadius, RichText, Ui};
use langis_core::group_admin::{LinkChange, Permission};
use langis_core::{ChatChange, Command, SafetyNumber};
use langis_store::{ContactNickname, Conversation, ConversationId, ConversationKind};

use crate::app::{App, Loaded};
use crate::icons::Icon;
use crate::panes::{body, buttons, menu_row, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    Safety,
    GroupLink,
    Requests,
    Permissions,
}

pub struct Details {
    pub conversation: ConversationId,
    pub page: Page,
    pub safety: Option<Result<SafetyNumber, String>>,
    pub key_transparency: Option<bool>,
    /// Their contact details (1:1): profile, nickname, verified.
    pub contact: Option<langis_core::ContactDetails>,
    /// Dialogs over the pane.
    pub edit_info: Option<(String, String)>,
    pub nickname: Option<ContactNickname>,
    pub add_members: Option<(String, Vec<String>)>,
    pub confirm: Option<Confirm>,
    pub label: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Leave,
    End,
    Delete,
    Block,
    Unblock,
}

impl Details {
    pub fn new(conversation: ConversationId) -> Self {
        Self {
            conversation,
            page: Page::Main,
            safety: None,
            key_transparency: None,
            contact: None,
            edit_info: None,
            nickname: None,
            add_members: None,
            confirm: None,
            label: None,
        }
    }
}

/// Opens a chat's details: refreshes the group, or their profile.
pub fn open(app: &mut App, id: ConversationId) {
    if id.is_group() {
        app.send(Command::RefreshGroup { conversation: id.clone() });
    } else if let Some(sid) = id.as_direct() {
        app.send(Command::LoadProfile { service_id: sid.to_owned() });
        load_contact(app, sid.to_owned());
    }
    app.panes.details = Some(Details::new(id));
}

fn load_contact(app: &App, sid: String) {
    app.load(move |db| async move {
        let details = langis_core::contact_details(&db, &sid).await;
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(d) = app.panes.details.as_mut() {
                d.contact = Some(details);
            }
        })))
    });
}

/// Desktop's disappearing message choices.
pub const TIMERS: [(u32, &str); 9] = [
    (0, "Off"),
    (30, "30 seconds"),
    (5 * 60, "5 minutes"),
    (60 * 60, "1 hour"),
    (8 * 60 * 60, "8 hours"),
    (24 * 60 * 60, "1 day"),
    (7 * 24 * 60 * 60, "1 week"),
    (28 * 24 * 60 * 60, "4 weeks"),
    (u32::MAX, ""),
];

/// A drop-down of [`TIMERS`]; true when it changed.
pub fn timer_picker(ui: &mut Ui, id: &str, seconds: &mut u32) -> bool {
    let before = *seconds;
    let label = TIMERS.iter().find(|(s, _)| s == seconds).map(|(_, l)| (*l).to_owned()).unwrap_or_else(|| crate::format::timer_long(*seconds));
    egui::ComboBox::from_id_salt(id).selected_text(label).show_ui(ui, |ui| {
        for (s, l) in TIMERS.iter().filter(|(s, _)| *s != u32::MAX) {
            ui.selectable_value(seconds, *s, *l);
        }
    });
    *seconds != before
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let Some(details) = app.panes.details.as_ref() else { return };
    let Some(c) = app.conversation(&details.conversation).cloned() else {
        app.panes.details = None;
        return;
    };
    let page = details.page;
    // A top bar with the back arrow.
    egui::Panel::top("details-header")
        .exact_size(52.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1., palette.border));
            ui.horizontal_centered(|ui| {
                ui.add_space(12.);
                if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back").clicked() {
                    match page {
                        Page::Main => app.panes.details = None,
                        _ => {
                            if let Some(d) = app.panes.details.as_mut() {
                                d.page = Page::Main;
                            }
                            app.send(Command::CancelKeyTransparency);
                        }
                    }
                }
                let heading = match page {
                    Page::Main => {
                        if c.kind == ConversationKind::Group {
                            "Group settings"
                        } else {
                            "Chat settings"
                        }
                    }
                    Page::Safety => "Safety number",
                    Page::GroupLink => "Group link",
                    Page::Requests => "Requests and invites",
                    Page::Permissions => "Permissions",
                };
                ui.label(RichText::new(heading).font(theme::semibold(14.)));
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let width = ui.available_width().min(640.);
            let margin = ((ui.available_width() - width) / 2.).max(16.);
            egui::Frame::new().inner_margin(egui::Margin { left: margin as i8, right: margin as i8, top: 24, bottom: 32 }).show(ui, |ui| {
                ui.set_max_width(width);
                match page {
                    Page::Main => main(app, ui, &palette, &c),
                    Page::Safety => safety(app, ui, &palette, &c),
                    Page::GroupLink => group_link(app, ui, &palette, &c),
                    Page::Requests => requests(app, ui, &palette, &c),
                    Page::Permissions => permissions(app, ui, &palette, &c),
                }
            });
        });
    });
    dialogs(app, ui, &palette, &c);
}

fn section(ui: &mut Ui, palette: &Palette, contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical(contents);
    });
    ui.add_space(12.);
}

/// A row with an icon, a label and (on the right) a value.
fn row(ui: &mut Ui, palette: &Palette, icon: Icon, label: &str, value: Option<&str>) -> egui::Response {
    let response = menu_row(ui, Some(icon), label, false);
    if let Some(value) = value {
        ui.painter().text(response.rect.right_center() - egui::vec2(10., 0.), egui::Align2::RIGHT_CENTER, value, theme::regular(13.), palette.text_secondary);
    }
    response
}

fn main(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let group = c.kind == ConversationKind::Group;
    let me = app.aci.clone();
    let admin = c.admins.iter().any(|a| *a == me);
    // The header: photo, name, about or description, counts.
    ui.vertical_centered(|ui| {
        widgets::avatar(ui, &widgets::AvatarSpec::of(c), 80.);
        ui.add_space(10.);
        let name = ui.add(egui::Label::new(RichText::new(&c.title).font(theme::semibold(20.))).sense(egui::Sense::click()));
        if c.can_edit_info && group && name.on_hover_text("Edit").clicked()
            && let Some(d) = app.panes.details.as_mut()
        {
            d.edit_info = Some((c.title.clone(), c.description.clone().unwrap_or_default()));
        }
        if group {
            body(ui, palette, &format!("Group · {} members", c.members.len() + 1));
            if let Some(d) = c.description.as_deref().filter(|d| !d.is_empty()) {
                ui.add_space(4.);
                body(ui, palette, d);
            }
        } else if let Some(contact) = app.panes.details.as_ref().and_then(|d| d.contact.as_ref()) {
            if let Some(p) = &contact.profile {
                if !p.about.is_empty() {
                    body(ui, palette, &format!("{} {}", p.about_emoji, p.about).trim().to_owned());
                }
                if let Some(username) = &p.username {
                    body(ui, palette, username);
                }
            }
            if let Some(phone) = &c.phone {
                body(ui, palette, phone);
            }
        }
        ui.add_space(14.);
        // Desktop's quick buttons: video, audio, mute.
        ui.horizontal(|ui| {
            let n = 3.;
            ui.add_space(((ui.available_width() - n * 76.) / 2.).max(0.));
            let quick = |ui: &mut Ui, icon: Icon, label: &str| {
                let (rect, response) = ui.allocate_exact_size(egui::vec2(72., 56.), egui::Sense::click());
                let fill = if response.hovered() { palette.fill_pressed } else { palette.fill };
                ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
                icon.paint(ui, egui::Rect::from_min_size(rect.min + egui::vec2(26., 8.), egui::vec2(20., 20.)), 20., palette.text);
                ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 9.), egui::Align2::CENTER_BOTTOM, label, theme::regular(12.), palette.text);
                ui.add_space(4.);
                response.clicked()
            };
            let callable = c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked;
            if quick(ui, Icon::Video, "Video") && callable {
                start_call(app, c, true);
            }
            if quick(ui, Icon::Phone, "Audio") && callable {
                start_call(app, c, false);
            }
            let muted = c.muted;
            let mute = ui.scope(|ui| quick(ui, Icon::BellSlashFill, if muted { "Unmute" } else { "Mute" })).inner;
            if mute {
                if muted {
                    app.send(Command::SetChat { conversation: c.id.clone(), change: ChatChange::MutedUntil(0) });
                } else {
                    egui::Popup::open_id(ui.ctx(), egui::Id::unique("details-mute"));
                }
            }
        });
    });
    egui::Popup::new(egui::Id::unique("details-mute"), ui.ctx().clone(), egui::PopupAnchor::PointerFixed, egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("details-mute-layer")))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            for (label, until) in crate::panes::mute_choices(false) {
                if menu_row(ui, None, label, false).clicked() {
                    app.send(Command::SetChat { conversation: c.id.clone(), change: ChatChange::MutedUntil(until) });
                }
            }
        });
    ui.add_space(20.);

    // Chat settings.
    section(ui, palette, |ui| {
        let mut timer = c.expire_timer;
        ui.horizontal(|ui| {
            Icon::TimerCompact.paint(ui, egui::Rect::from_min_size(ui.cursor().min + egui::vec2(10., 8.), egui::vec2(20., 20.)), 18., palette.text);
            ui.add_space(40.);
            ui.label(RichText::new("Disappearing messages").font(theme::regular(14.)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!group || c.can_edit_info, |ui| {
                    if timer_picker(ui, "details-timer", &mut timer) {
                        app.send(Command::SetExpireTimer { conversation: c.id.clone(), seconds: timer });
                    }
                });
            });
        });
        ui.add_space(4.);
        if c.kind == ConversationKind::Direct {
            if row(ui, palette, Icon::EditCompact, "Nickname", None).clicked() {
                let current = app.panes.details.as_ref().and_then(|d| d.contact.as_ref()).and_then(|ct| ct.nickname.clone()).unwrap_or_default();
                if let Some(d) = app.panes.details.as_mut() {
                    d.nickname = Some(current);
                }
            }
            let verified = app.panes.details.as_ref().and_then(|d| d.contact.as_ref()).is_some_and(|ct| ct.verified);
            if row(ui, palette, Icon::SafetyNumber, "View safety number", verified.then_some("Verified")).clicked() {
                app.send(Command::LoadSafetyNumber { conversation: c.id.clone() });
                if let Some(d) = app.panes.details.as_mut() {
                    d.page = Page::Safety;
                    d.safety = None;
                    d.key_transparency = None;
                }
            }
        }
        if group && !c.left {
            let mine = c.member_labels.get(&me).map(|l| l.text.clone()).unwrap_or_default();
            if c.can_label && row(ui, palette, Icon::EditCompact, "Your member label", Some(&mine)).clicked()
                && let Some(d) = app.panes.details.as_mut()
            {
                d.label = Some(mine);
            }
        }
    });

    if group && !c.left {
        // Members.
        ui.label(RichText::new(format!("{} members", c.members.len() + 1)).font(theme::semibold(14.)));
        ui.add_space(6.);
        section(ui, palette, |ui| {
            if c.can_add_members && menu_row(ui, Some(Icon::PersonPlus), "Add members", false).clicked()
                && let Some(d) = app.panes.details.as_mut()
            {
                d.add_members = Some((String::new(), Vec::new()));
            }
            let mut everyone: Vec<String> = c.members.clone();
            everyone.insert(0, me.clone());
            for m in everyone {
                let you = m == me;
                let name = if you { "You".to_owned() } else { app.name(&m) };
                let is_admin = c.admins.contains(&m);
                let label = c.member_labels.get(&m).map(|l| format!("{} {}", l.emoji.clone().unwrap_or_default(), l.text).trim().to_owned());
                let color = m.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                let response = crate::panes::contact_row(ui, palette, &name, label.as_deref(), color, None);
                if is_admin {
                    ui.painter().text(response.rect.right_center() - egui::vec2(10., 0.), egui::Align2::RIGHT_CENTER, "Admin", theme::regular(12.), palette.text_secondary);
                }
                if !you {
                    response.context_menu(|ui| {
                        if menu_row(ui, None, "Message", false).clicked() {
                            app.panes.details = None;
                            app.open(ConversationId::direct(&m));
                            ui.close();
                        }
                        if admin {
                            if menu_row(ui, None, if is_admin { "Remove as admin" } else { "Make admin" }, false).clicked() {
                                app.send(Command::SetGroupAdmin { conversation: c.id.clone(), member: m.clone(), admin: !is_admin });
                                ui.close();
                            }
                            if menu_row(ui, None, "Remove from group", true).clicked() {
                                app.send(Command::RemoveFromGroup { conversation: c.id.clone(), service_id: m.clone() });
                                ui.close();
                            }
                        }
                    });
                }
            }
        });
        // Administration.
        section(ui, palette, |ui| {
            let state = &c.group_state;
            let link_value = if state.link.is_some() { "On" } else { "Off" };
            if (admin || state.link.is_some()) && row(ui, palette, Icon::Link, "Group link", Some(link_value)).clicked() {
                set_page(app, Page::GroupLink);
            }
            let waiting = state.invites.len() + state.requests.len();
            if row(ui, palette, Icon::PersonPlus, "Requests and invites", Some(&waiting.to_string())).clicked() {
                set_page(app, Page::Requests);
            }
            if admin && row(ui, palette, Icon::Key, "Permissions", None).clicked() {
                set_page(app, Page::Permissions);
            }
        });
    }

    // Leaving, blocking, deleting.
    section(ui, palette, |ui| {
        let mut confirm = None;
        if group && !c.left && menu_row(ui, Some(Icon::Leave), "Leave group", true).clicked() {
            confirm = Some(Confirm::Leave);
        }
        if group && admin && !c.left && !c.ended && menu_row(ui, Some(Icon::MinusCircle), "End group", true).clicked() {
            confirm = Some(Confirm::End);
        }
        if c.kind != ConversationKind::NoteToSelf {
            let (label, which) = if c.blocked { ("Unblock", Confirm::Unblock) } else if group { ("Block group", Confirm::Block) } else { ("Block", Confirm::Block) };
            if menu_row(ui, Some(Icon::Block), label, !c.blocked).clicked() {
                confirm = Some(which);
            }
            if !c.blocked && !group && menu_row(ui, Some(Icon::Spam), "Report spam", true).clicked() {
                app.send(Command::AnswerMessageRequest { conversation: c.id.clone(), answer: langis_store::MessageRequestResponse::Spam });
            }
        }
        if menu_row(ui, Some(Icon::Trash), "Delete chat", true).clicked() {
            confirm = Some(Confirm::Delete);
        }
        if let Some(confirm) = confirm
            && let Some(d) = app.panes.details.as_mut()
        {
            d.confirm = Some(confirm);
        }
    });
}

fn start_call(app: &mut App, c: &Conversation, video: bool) {
    if c.kind == ConversationKind::Group {
        crate::panes::open_lobby(app, c.id.clone(), video);
    } else {
        app.send(Command::StartCall { conversation: c.id.clone(), video });
    }
}

fn set_page(app: &mut App, page: Page) {
    if let Some(d) = app.panes.details.as_mut() {
        d.page = page;
    }
}

fn safety(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let state = app.panes.details.as_ref().and_then(|d| d.safety.clone());
    let kt = app.panes.details.as_ref().and_then(|d| d.key_transparency);
    ui.vertical_centered(|ui| {
        match state {
            None => {
                ui.spinner();
            }
            Some(Err(why)) => body(ui, palette, &why),
            Some(Ok(number)) => {
                ui.add_space(8.);
                // The code they scan (Desktop's `SafetyNumberViewer`), kept
                // while the same fingerprint shows.
                let key = egui::Id::unique(("safety-qr", &number.qr));
                let texture = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(key)).or_else(|| {
                    let made = crate::startup::qr_texture_of(ui.ctx(), "safety-qr", &number.qr)?;
                    ui.ctx().data_mut(|d| d.insert_temp(key, made.clone()));
                    Some(made)
                });
                if let Some(texture) = texture {
                    egui::Frame::new().fill(egui::Color32::WHITE).corner_radius(CornerRadius::same(12)).inner_margin(12).show(ui, |ui| {
                        ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::Vec2::splat(180.)));
                    });
                    ui.add_space(12.);
                }
                // Twelve groups of five, in three rows of four.
                let groups: Vec<String> = number.digits.as_bytes().chunks(5).map(|g| String::from_utf8_lossy(g).into_owned()).collect();
                egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(12)).inner_margin(16).show(ui, |ui| {
                    for line in groups.chunks(4) {
                        ui.label(RichText::new(line.join("  ")).font(egui::FontId::monospace(18.)));
                    }
                });
                ui.add_space(12.);
                body(ui, palette, &format!("To verify end-to-end encryption with {}, compare the numbers above with their device. They can also scan your code with their device.", c.title));
                ui.add_space(16.);
                if number.verified {
                    ui.label(RichText::new("Verified").font(theme::medium(14.)).color(palette.green));
                }
                ui.add_space(8.);
                let label = if number.verified { "Clear verification" } else { "Mark as verified" };
                if widgets::pill_button(ui, label, !number.verified).clicked() {
                    app.send(Command::SetVerified { conversation: c.id.clone(), verified: !number.verified });
                }
                if number.auto_verify == langis_core::AutoVerify::Available {
                    ui.add_space(8.);
                    match kt {
                        None => {
                            if widgets::pill_button(ui, "Verify automatically", false).clicked() {
                                app.send(Command::CheckKeyTransparency { conversation: c.id.clone() });
                                if let Some(d) = app.panes.details.as_mut() {
                                    d.key_transparency = Some(false);
                                    d.key_transparency = None;
                                }
                            }
                        }
                        Some(true) => {
                            ui.label(RichText::new("Encryption was automatically verified for this chat.").color(palette.green));
                        }
                        Some(false) => body(ui, palette, "Automatic verification isn't available for this chat."),
                    }
                }
            }
        }
    });
}

fn group_link(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let admin = c.admins.contains(&app.aci);
    let link = c.group_state.link.clone();
    section(ui, palette, |ui| {
        let mut on = link.is_some();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Group link").font(theme::regular(14.)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(admin, |ui| {
                    if widgets::switch(ui, &mut on).changed() {
                        app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: if on { LinkChange::On } else { LinkChange::Off } });
                    }
                });
            });
        });
        if let Some(link) = &link {
            ui.add_space(8.);
            ui.label(RichText::new(&link.url).font(theme::regular(13.)).color(palette.accent_text));
            ui.add_space(8.);
            if menu_row(ui, Some(Icon::Copy), "Copy link", false).clicked() {
                ui.ctx().copy_text(link.url.clone());
                app.toast("Link copied.");
            }
            if admin {
                if menu_row(ui, Some(Icon::Recent), "Reset link", false).clicked() {
                    app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: LinkChange::Reset });
                }
                let mut approval = link.approval;
                ui.horizontal(|ui| {
                    ui.label("Approve new members");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::switch(ui, &mut approval).changed() {
                            app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: LinkChange::Approval(approval) });
                        }
                    });
                });
            }
        }
    });
    body(ui, palette, "Anyone with the link can view the group's name and photo and ask to join. Share it with people you trust.");
}

fn requests(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let admin = c.admins.contains(&app.aci);
    let state = c.group_state.clone();
    ui.label(RichText::new("Requests").font(theme::semibold(14.)));
    ui.add_space(6.);
    section(ui, palette, |ui| {
        if state.requests.is_empty() {
            body(ui, palette, "No pending requests.");
        }
        for r in &state.requests {
            let name = app.name(&r.aci);
            ui.horizontal(|ui| {
                crate::panes::contact_row(ui, palette, &name, None, 2, None);
            });
            if admin {
                ui.horizontal(|ui| {
                    ui.add_space(56.);
                    if widgets::pill_button(ui, "Approve", true).clicked() {
                        app.send(Command::AnswerJoinRequest { conversation: c.id.clone(), member: r.aci.clone(), approve: true });
                    }
                    if widgets::pill_button(ui, "Deny", false).clicked() {
                        app.send(Command::AnswerJoinRequest { conversation: c.id.clone(), member: r.aci.clone(), approve: false });
                    }
                });
            }
        }
    });
    ui.label(RichText::new("Invites").font(theme::semibold(14.)));
    ui.add_space(6.);
    section(ui, palette, |ui| {
        if state.invites.is_empty() {
            body(ui, palette, "No pending invites.");
        }
        for invite in &state.invites {
            let name = app.name(&invite.service_id);
            let by = format!("Invited by {}", if invite.by == app.aci { "you".to_owned() } else { app.name(&invite.by) });
            let response = crate::panes::contact_row(ui, palette, &name, Some(&by), 5, None);
            if admin || invite.by == app.aci {
                response.context_menu(|ui| {
                    if menu_row(ui, None, "Revoke invite", true).clicked() {
                        app.send(Command::RevokeGroupInvites { conversation: c.id.clone(), invitees: vec![invite.service_id.clone()] });
                        ui.close();
                    }
                });
            }
        }
    });
    body(ui, palette, "People on this list are not able to see messages in the group until they join.");
}

fn permissions(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let state = &c.group_state;
    section(ui, palette, |ui| {
        let choices: [(&str, bool, fn(bool) -> Permission); 3] = [
            ("Add members", state.members_admins_only, |a| Permission::AddMembers { admins_only: a }),
            ("Edit group info", state.info_admins_only, |a| Permission::EditInfo { admins_only: a }),
            ("Send messages", state.announcements_only, |a| Permission::SendMessages { admins_only: a }),
        ];
        for (label, admins_only, make) in choices {
            ui.horizontal(|ui| {
                ui.set_min_height(36.);
                ui.label(RichText::new(label).font(theme::regular(14.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut value = admins_only;
                    egui::ComboBox::from_id_salt(label).selected_text(if value { "Only admins" } else { "All members" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut value, false, "All members");
                        ui.selectable_value(&mut value, true, "Only admins");
                    });
                    if value != admins_only {
                        app.send(Command::SetGroupPermission { conversation: c.id.clone(), permission: make(value) });
                    }
                });
            });
        }
    });
}

/// The details pane's dialogs: edit info, nickname, add members, confirms.
fn dialogs(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let Some(mut d) = app.panes.details.take() else { return };
    if let Some((mut name, mut description)) = d.edit_info.take() {
        let mut answer = None;
        let closed = modal(ui, "edit-group", 400., |ui| {
            title(ui, "Edit group");
            ui.horizontal(|ui| {
                widgets::avatar(ui, &widgets::AvatarSpec::of(c), 64.);
                ui.vertical(|ui| {
                    if ui.link("Choose photo").clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file()
                        && let Ok(bytes) = std::fs::read(path)
                    {
                        app.send(Command::SetGroupAvatar { conversation: c.id.clone(), image: Some(bytes) });
                    }
                    if c.avatar_path.is_some() && ui.link("Remove photo").clicked() {
                        app.send(Command::SetGroupAvatar { conversation: c.id.clone(), image: None });
                    }
                });
            });
            ui.add_space(8.);
            widgets::text_field(ui, &mut name, "Group name", false);
            ui.add_space(6.);
            widgets::text_field(ui, &mut description, "Group description", false);
            answer = buttons(ui, &["Save", "Cancel"]);
        });
        match answer {
            Some(0) => {
                let title = (name.trim() != c.title && !name.trim().is_empty()).then(|| name.trim().to_owned());
                let desc = (Some(description.trim()) != c.description.as_deref().map(str::trim)).then(|| description.trim().to_owned());
                if title.is_some() || desc.is_some() {
                    app.send(Command::UpdateGroup { conversation: c.id.clone(), title, description: desc });
                }
            }
            Some(_) => {}
            None if closed => {}
            None => d.edit_info = Some((name, description)),
        }
    }
    if let Some(mut nick) = d.nickname.take() {
        let mut answer = None;
        let closed = modal(ui, "nickname", 380., |ui| {
            title(ui, "Nickname");
            body(ui, palette, "Nicknames and notes are stored with Signal and end-to-end encrypted. They are only visible to you.");
            ui.add_space(8.);
            widgets::text_field(ui, &mut nick.given, "First name", false);
            ui.add_space(6.);
            widgets::text_field(ui, &mut nick.family, "Last name", false);
            ui.add_space(6.);
            widgets::text_field(ui, &mut nick.note, "Note", false);
            answer = buttons(ui, &["Save", "Cancel", "Delete"]);
        });
        match answer {
            Some(0) => app.send(Command::SetNickname { conversation: c.id.clone(), nickname: nick }),
            Some(2) => app.send(Command::SetNickname { conversation: c.id.clone(), nickname: ContactNickname::default() }),
            Some(_) => {}
            None if closed => {}
            None => d.nickname = Some(nick),
        }
        if answer.is_some_and(|a| a != 1)
            && let Some(sid) = c.id.as_direct()
        {
            load_contact(app, sid.to_owned());
        }
    }
    if let Some(mut text) = d.label.take() {
        let mut answer = None;
        let closed = modal(ui, "member-label", 360., |ui| {
            title(ui, "Your member label");
            body(ui, palette, "A label shown next to your name in this group.");
            ui.add_space(8.);
            widgets::text_field(ui, &mut text, "Label", false);
            answer = buttons(ui, &["Save", "Cancel"]);
        });
        match answer {
            Some(0) => app.send(Command::SetMemberLabel { conversation: c.id.clone(), emoji: None, text }),
            Some(_) => {}
            None if closed => {}
            None => d.label = Some(text),
        }
    }
    if let Some((mut query, mut chosen)) = d.add_members.take() {
        let mut answer = None;
        let closed = modal(ui, "add-members", 400., |ui| {
            title(ui, "Add members");
            widgets::text_field(ui, &mut query, "Search", false);
            ui.add_space(6.);
            let q = query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                for p in app.conversations.iter().filter(|p| p.kind == ConversationKind::Direct && (q.is_empty() || p.title.to_lowercase().contains(&q))) {
                    let Some(sid) = p.id.as_direct() else { continue };
                    if c.members.iter().any(|m| m == sid) {
                        continue;
                    }
                    let on = chosen.iter().any(|m| m == sid);
                    let label = if on { format!("{} (added)", p.title) } else { p.title.clone() };
                    if crate::panes::contact_row(ui, palette, &label, p.phone.as_deref(), p.color, p.avatar_path.as_deref()).clicked() {
                        if on {
                            chosen.retain(|m| m != sid);
                        } else {
                            chosen.push(sid.to_owned());
                        }
                    }
                }
            });
            answer = buttons(ui, &[&format!("Add ({})", chosen.len()), "Cancel"]);
        });
        match answer {
            Some(0) if !chosen.is_empty() => app.send(Command::AddGroupMembers { conversation: c.id.clone(), members: chosen }),
            Some(_) => {}
            None if closed => {}
            None => d.add_members = Some((query, chosen)),
        }
    }
    if let Some(confirm) = d.confirm.take() {
        let group = c.kind == ConversationKind::Group;
        let (heading, text, action) = match confirm {
            Confirm::Leave => ("Leave group?", "You will no longer be able to send or receive messages in this group.", "Leave"),
            Confirm::End => ("End group?", "This ends the group for everyone. No one will be able to send messages in it.", "End group"),
            Confirm::Delete => ("Delete chat?", "This chat will be deleted from all your devices.", "Delete"),
            Confirm::Block if group => ("Block and leave this group?", "You will no longer receive messages or updates from this group.", "Block"),
            Confirm::Block => ("Block this person?", "Blocked people won't be able to call you or send you messages.", "Block"),
            Confirm::Unblock => ("Unblock?", "You will be able to message and call each other.", "Unblock"),
        };
        let mut answer = None;
        let closed = modal(ui, "details-confirm", 360., |ui| {
            title(ui, heading);
            body(ui, palette, text);
            answer = buttons(ui, &[action, "Cancel"]);
        });
        match answer {
            Some(0) => {
                let id = c.id.clone();
                match confirm {
                    Confirm::Leave => app.send(Command::LeaveGroup { conversation: id }),
                    Confirm::End => app.send(Command::EndGroup { conversation: id }),
                    Confirm::Delete => {
                        app.send(Command::DeleteConversation { conversation: id });
                        return;
                    }
                    Confirm::Block => app.send(Command::SetChat { conversation: id, change: ChatChange::Blocked(true) }),
                    Confirm::Unblock => app.send(Command::SetChat { conversation: id, change: ChatChange::Blocked(false) }),
                }
            }
            Some(_) => {}
            None if closed => {}
            None => d.confirm = Some(confirm),
        }
    }
    if app.panes.details.is_none() {
        app.panes.details = Some(d);
    }
}
