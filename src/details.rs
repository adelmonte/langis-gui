//! Desktop's `ConversationDetails`: the pane that replaces a chat when its
//! header is clicked. Members, admins, permissions, the group link and
//! requests, the disappearing timer, mute, block, safety numbers, the
//! nickname, and leaving or deleting.

use egui::{Rect, Sense, Vec2, CornerRadius, RichText, Ui};
use langis_core::group_admin::{LinkChange, Permission};
use langis_core::{ChatChange, Command, SafetyNumber};
use langis_store::{ContactNickname, Conversation, ConversationId, ConversationKind};

use crate::app::{App, Loaded};
use crate::icons::Icon;
use crate::panes::{ChatConfirm, body, buttons, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    Safety,
    GroupLink,
    Requests,
    Permissions,
    ChatColor,
    AllMedia,
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
    /// The AvatarEditor for the group's photo.
    pub group_avatar: Option<crate::avatar_editor::AvatarEditor>,
    pub label: Option<String>,
    /// The chat's media, for All media and whether to offer it.
    pub media: Option<Vec<langis_store::Message>>,
    pub media_tab: crate::media_gallery::Tab,
    /// Opened for the nickname dialog, shown once their details arrive.
    pub nickname_on_load: bool,
    /// The member list's search, and whether it shows everyone.
    pub member_query: String,
    pub all_members: bool,
    /// The group link's Share: chats chosen and the search, or its QR code.
    pub share_link: Option<(Vec<ConversationId>, String)>,
    pub link_qr: bool,
    /// The member label's emoji, and its picker while open.
    pub label_emoji: String,
    pub label_picker: Option<crate::emoji_picker::Picker>,
    /// A join request waiting on Approve or Deny's confirmation.
    pub answer_request: Option<(String, bool)>,
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
            group_avatar: None,
            label: None,
            media: None,
            media_tab: Default::default(),
            nickname_on_load: false,
            member_query: String::new(),
            all_members: false,
            share_link: None,
            link_qr: false,
            label_emoji: String::new(),
            label_picker: None,
            answer_request: None,
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
    crate::media_gallery::load(app, id.clone());
    app.panes.details = Some(Details::new(id));
}

/// A person's safety number (from `ContactModal`).
pub fn open_safety(app: &mut App, id: ConversationId) {
    open(app, id.clone());
    app.send(Command::LoadSafetyNumber { conversation: id });
    set_page(app, Page::Safety);
}

/// A person's nickname and note (from `ContactModal`).
pub fn open_nickname(app: &mut App, id: ConversationId) {
    open(app, id);
    if let Some(d) = app.panes.details.as_mut() {
        d.nickname_on_load = true;
    }
}

/// Desktop's All media, from the header menu.
pub fn open_media(app: &mut App, id: ConversationId) {
    open(app, id);
    set_page(app, Page::AllMedia);
}

fn load_contact(app: &App, sid: String) {
    app.load(move |db| async move {
        let details = langis_core::contact_details(&db, &sid).await;
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(d) = app.panes.details.as_mut() {
                if std::mem::take(&mut d.nickname_on_load) {
                    d.nickname = Some(details.nickname.clone().unwrap_or_default());
                }
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
pub fn timer_picker(ui: &mut Ui, id: &str, seconds: &mut u32, custom: &mut bool) -> bool {
    let before = *seconds;
    let label = TIMERS.iter().find(|(s, _)| s == seconds).map(|(_, l)| tr(l).to_owned()).unwrap_or_else(|| crate::format::timer_long(*seconds));
    egui::ComboBox::from_id_salt(id).selected_text(label).show_ui(ui, |ui| {
        for (s, l) in TIMERS.iter().filter(|(s, _)| *s != u32::MAX) {
            ui.selectable_value(seconds, *s, tr(l));
        }
        // Desktop's `DisappearingTimerSelect` ends with Custom time.
        if ui.selectable_label(false, tr("Custom time…")).clicked() {
            *custom = true;
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
                if widgets::icon_button(ui, Icon::ChevronLeft, 20., tr("Back")).clicked() {
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
                            tr("Group settings")
                        } else {
                            tr("Chat settings")
                        }
                    }
                    Page::Safety => "Safety number",
                    Page::GroupLink => tr("Group link"),
                    Page::Requests => "Requests and invites",
                    Page::Permissions => tr("Permissions"),
                    Page::ChatColor => tr("Chat Color"),
                    Page::AllMedia => "",
                };
                ui.label(RichText::new(heading).font(theme::semibold(14.)));
                if page == Page::AllMedia
                    && let Some(d) = app.panes.details.as_mut()
                {
                    crate::media_gallery::tabs(ui, rect.shrink2(egui::vec2(60., 0.)), &mut d.media_tab);
                }
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
                    Page::ChatColor => crate::chat_colors::picker(app, ui, Some(c.id.clone())),
                    Page::AllMedia => crate::media_gallery::show(app, ui, &palette, &c.id),
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

fn main(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let group = c.kind == ConversationKind::Group;
    let me = app.aci.clone();
    let admin = c.admins.iter().any(|a| *a == me);
    // The header: photo, name, about or description, counts.
    ui.vertical_centered(|ui| {
        let photo = widgets::avatar(ui, &widgets::AvatarSpec::of(c), 80.);
        if let Some(badge) = c.id.as_direct().and_then(|sid| app.badge_for(sid)) {
            widgets::paint_badge(ui, photo.rect, badge);
        }
        // Desktop's AvatarLightbox: a photo opens large.
        if !c.avatar_withheld
            && let Some(path) = c.avatar_path.clone()
            && ui.interact(photo.rect, ui.scope_id().with("details-photo"), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        {
            app.panes.avatar_lightbox = Some(path);
        }
        // A photo held back in a message request: Desktop's tap to download.
        if c.avatar_withheld {
            if photo.on_hover_text("Download photo").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                app.send(Command::DownloadPhoto { conversation: c.id.clone() });
            }
            if ui.link("Download photo").clicked() {
                app.send(Command::DownloadPhoto { conversation: c.id.clone() });
            }
        }
        ui.add_space(10.);
        let name = ui.add(egui::Label::new(RichText::new(&c.title).font(theme::semibold(20.))).sense(egui::Sense::click()));
        // Desktop's InContactsIcon after the name.
        if !group && c.system_contact {
            let at = egui::Rect::from_min_size(egui::pos2(name.rect.right() + 6., name.rect.center().y - 7.), egui::vec2(14., 14.));
            let color = if palette.dark { theme::rgb(0xb9b9b9) } else { theme::rgb(0x5e5e5e) };
            theme::paint_symbol(ui.painter(), "person-circle", at.center(), 14., color);
            ui.interact(at, ui.scope_id().with("in-contacts"), egui::Sense::hover()).on_hover_text("This person is in your contacts.");
        }
        // A person's name opens Desktop's AboutContactModal (its chevron).
        if !group && c.kind == ConversationKind::Direct {
            let at = egui::pos2(name.rect.right() + if c.system_contact { 26. } else { 10. }, name.rect.center().y);
            crate::icons::Icon::ChevronRight.paint(ui, egui::Rect::from_center_size(at, egui::vec2(16., 16.)), 16., palette.text_secondary);
            if name.clone().on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                && let Some(sid) = c.id.as_direct()
            {
                crate::contact_modal::open(app, None, sid.to_owned());
                if let Some(contact) = app.panes.contact.as_mut() {
                    contact.about_open = true;
                }
            }
        }
        if c.can_edit_info && group && name.on_hover_text(tr("Edit")).clicked()
            && let Some(d) = app.panes.details.as_mut()
        {
            d.edit_info = Some((c.title.clone(), c.description.clone().unwrap_or_default()));
        }
        if group {
            // Desktop's subtitle: the description, or Add group
            // description..., or the member count.
            match c.description.as_deref().filter(|d| !d.trim().is_empty()) {
                Some(d) => {
                    ui.add_space(4.);
                    let w = ui.available_width().min(360.);
                    widgets::group_description(ui, &c.title, d, w, true);
                }
                // Desktop's "Add group description..." for those who can.
                None if c.can_edit_info => {
                    ui.add_space(4.);
                    if ui.add(egui::Label::new(RichText::new(tr("Add group description...")).font(theme::regular(14.)).color(palette.text_secondary)).sense(egui::Sense::click())).clicked()
                        && let Some(d) = app.panes.details.as_mut()
                    {
                        d.edit_info = Some((c.title.clone(), String::new()));
                    }
                }
                None => {
                    let n = c.members.len() + 1;
                    body(ui, palette, &crate::i18n::tf("icu:ConversationDetailsHeader--members", &format!("{n} members"), &[("number", crate::i18n::Arg::from(n))]));
                }
            }
        } else if let Some(contact) = app.panes.details.as_ref().and_then(|d| d.contact.as_ref())
            && let Some(p) = &contact.profile
            && !p.about.is_empty()
        {
            body(ui, palette, &format!("{} {}", p.about_emoji, p.about).trim().to_owned());
        }
        ui.add_space(16.);
        // Desktop's banner for an ended group.
        if c.ended {
            egui::Frame::new().fill(palette.fill).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (at, _) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::hover());
                    theme::paint_symbol(ui.painter(), "group-x", at.center(), 16., palette.text);
                    ui.label(RichText::new(tr("This group has ended")).font(theme::regular(13.)));
                });
            });
            ui.add_space(12.);
        }
    });
    header_buttons(app, ui, palette, c);
    ui.add_space(20.);
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    use crate::axo_list::{self, End, Item};

    // Disappearing messages, nickname, chat color, media, safety number.
    axo_list::list(ui, None, None, |ui| {
        let info = if group {
            tr("When enabled, messages sent and received in this group will disappear after they've been seen.")
        } else {
            tr("When enabled, messages sent and received in this 1:1 chat will disappear after they've been seen.")
        };
        let mut values: Vec<u32> = TIMERS.iter().map(|(v, _)| *v).filter(|v| *v != u32::MAX).collect();
        values.sort_by_key(|v| if *v == 0 { 0 } else { u32::MAX - v });
        if !values.contains(&c.expire_timer) {
            values.push(c.expire_timer);
        }
        let mut labels: Vec<String> = values.iter().map(|v| if *v == 0 { tr("Off").to_owned() } else { crate::format::timer_long(*v) }).collect();
        // Desktop's `DisappearingTimerSelect` ends with Custom time.
        labels.push(tr("Custom time…").to_owned());
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        let chosen = values.iter().position(|v| *v == c.expire_timer);
        let item = Item::new(tr("Disappearing messages")).symbol("timer-slash").description(info).disabled(group && !c.can_edit_info);
        if let Some(i) = axo_list::select(ui, egui::Id::unique("details-timer").with(&c.id.0), item, &options, chosen) {
            if i == values.len() {
                let (value, unit) = crate::panes::custom_timer_start(if c.expire_timer > 0 { c.expire_timer } else { 3_600 });
                app.panes.custom_timer = Some((crate::panes::TimerTarget::Chat(c.id.clone()), value, unit));
            } else {
                app.send(Command::SetExpireTimer { conversation: c.id.clone(), seconds: values[i] });
            }
        }
        if c.kind == ConversationKind::Direct && axo_list::item(ui, Item::new(tr("Nickname")).symbol("pencil"), End::Click).clicked() {
            let current = app.panes.details.as_ref().and_then(|d| d.contact.as_ref()).and_then(|ct| ct.nickname.clone()).unwrap_or_default();
            if let Some(d) = app.panes.details.as_mut() {
                d.nickname = Some(current);
            }
        }
        let bubble = crate::chat_colors::for_chat(app, &c.id);
        if axo_list::item(ui, Item::new(tr("Chat color")).symbol("palette"), End::Swatch(bubble)).clicked() {
            set_page(app, Page::ChatColor);
        }
        let has_media = app.panes.details.as_ref().and_then(|d| d.media.as_ref()).is_some_and(|m| !m.is_empty());
        if has_media && axo_list::item(ui, Item::new(tr("Media, links, and files")).symbol("album"), End::Arrow(None)).clicked() {
            set_page(app, Page::AllMedia);
        }
        if c.kind == ConversationKind::Direct {
            let verified = app.panes.details.as_ref().and_then(|d| d.contact.as_ref()).is_some_and(|ct| ct.verified);
            let item = Item::new(tr("View Safety Number")).symbol("shield-check");
            if axo_list::item(ui, item, End::Arrow(verified.then_some(tr("Verified")))).clicked() {
                app.send(Command::LoadSafetyNumber { conversation: c.id.clone() });
                if let Some(d) = app.panes.details.as_mut() {
                    d.page = Page::Safety;
                    d.safety = None;
                    d.key_transparency = None;
                }
            }
        }
    });

    // `ConversationNotificationsSettings`: mute, and until when.
    if c.kind != ConversationKind::NoteToSelf {
        axo_list::gap(ui);
        axo_list::list(ui, Some(tr("Notifications")), None, |ui| {
            let state = if !c.muted {
                tr("Not muted").to_owned()
            } else if c.muted_until == i64::MAX {
                tr("Muted always").to_owned()
            } else {
                crate::i18n::tf("icu:muteExpirationLabel", &format!("Muted until {}", crate::format::short_timestamp(c.muted_until)), &[("duration", crate::i18n::Arg::from(&crate::format::short_timestamp(c.muted_until)))])
            };
            let r = axo_list::item(ui, Item::new(tr("Mute notifications")).symbol(if c.muted { "bell-slash" } else { "bell" }), End::Arrow(Some(&state)));
            let menu = egui::Id::unique("details-mute").with(&c.id.0);
            if r.clicked() {
                crate::menu::open_at(ui.ctx(), menu, r.row.rect.right_bottom() - egui::vec2(220., 0.));
            }
            crate::menu::show(ui.ctx(), menu, |m| {
                if c.muted {
                    if m.item(Some("bell"), tr("Unmute")) {
                        app.send(Command::SetChat { conversation: c.id.clone(), change: ChatChange::MutedUntil(0) });
                    }
                } else {
                    crate::panes::mute_items(app, m, &c.id);
                }
            });
            // Desktop's "Mentions" for a group: while muted, notify for
            // mentions anyway or not.
            if c.kind == ConversationKind::Group {
                let options = ["Always notify", "Don't notify"];
                let item = Item::new(tr("Mentions")).symbol("at").description("Receive notifications when you're mentioned in muted chats");
                if let Some(i) = axo_list::select(ui, egui::Id::unique(("details-mentions", c.id.0.as_str())), item, &options, Some(c.dont_notify_mentions as usize)) {
                    let off = i == 1;
                    if off != c.dont_notify_mentions {
                        app.send(Command::SetChat { conversation: c.id.clone(), change: ChatChange::DontNotifyMentions(off) });
                    }
                }
            }
        });
    }

    // `ConversationDetailsGroups`: the groups they're in with us.
    if c.kind == ConversationKind::Direct
        && let Some(them) = c.id.as_direct()
    {
        let common: Vec<Conversation> = app.conversations.iter().filter(|g| g.kind == ConversationKind::Group && !g.left && g.members.iter().any(|m| m == them)).cloned().collect();
        axo_list::gap(ui);
        let n = common.len();
        let title = if n == 0 {
            tr("No groups in common").to_owned()
        } else {
            crate::i18n::tf("icu:ConversationDetailsGroups--title", &if n == 1 { "1 group in common".to_owned() } else { format!("{n} groups in common") }, &[("count", crate::i18n::Arg::from(n))])
        };
        let see_all_id = egui::Id::unique(("common-see-all", c.id.0.as_str()));
        let all = ui.ctx().data(|d| d.get_temp::<bool>(see_all_id)).unwrap_or(false);
        axo_list::list(ui, Some(&title), None, |ui| {
            // Desktop's "Add to a group", then the first five and See all.
            if axo_list::item(ui, Item::new(tr("Add to a group")).symbol("plus"), End::Click).clicked() {
                crate::contact_modal::open(app, None, them.to_owned());
                if let Some(contact) = app.panes.contact.as_mut() {
                    contact.add_to_group = Some(String::new());
                }
            }
            for g in common.iter().take(if all { usize::MAX } else { 5 }) {
                let row = group_row(ui, palette, g);
                if row.clicked() {
                    app.panes.details = None;
                    app.open(g.id.clone());
                }
            }
            if !all && n > 5 && axo_list::item(ui, Item::new(tr("See all")).symbol("chevron-down"), End::Click).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(see_all_id, true));
            }
        });
    }

    // Recent calls with this chat (Desktop's `CallHistoryGroupPanelSection`),
    // from its loaded messages.
    let calls: Vec<(String, i64)> = app
        .chat
        .as_ref()
        .filter(|chat| chat.id == c.id)
        .map(|chat| {
            chat.messages
                .iter()
                .rev()
                .filter_map(|m| match &m.notice {
                    Some(n @ (langis_store::Notice::Call { .. } | langis_store::Notice::GroupCall { .. })) => Some((crate::notices::notice_text(n, |sid| app.name(sid)), m.sent_at)),
                    _ => None,
                })
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    if !calls.is_empty() {
        axo_list::gap(ui);
        axo_list::list(ui, Some(tr("Calls")), None, |ui| {
            for (text, at) in &calls {
                let when = crate::format::short_timestamp(*at);
                axo_list::item(ui, Item::new(text).symbol(tr("phone")), End::Arrow(Some(&when)));
            }
        });
    }

    if group {
        // `ConversationDetailsMembershipList`.
        axo_list::gap(ui);
        let count = c.members.len() + 1;
        let title = if c.ended {
            if count == 1 { "1 former member".to_owned() } else { crate::i18n::tf("icu:ConversationDetailsMembershipList--terminated-title", &format!("{count} former members"), &[("number", crate::i18n::Arg::from(&count))]) }
        } else if count == 1 {
            "1 member".to_owned()
        } else {
            crate::i18n::tf("icu:GroupListItem__message-default", &format!("{count} members"), &[("count", crate::i18n::Arg::from(&count))])
        };
        axo_list::list(ui, Some(&title), None, |ui| {
            if c.can_add_members && !c.left && axo_list::item(ui, Item::new(tr("Add members")).symbol("plus"), End::Click).clicked()
                && let Some(d) = app.panes.details.as_mut()
            {
                d.add_members = Some((String::new(), Vec::new()));
            }
            // Desktop's order: you, then admins, then everyone else.
            let mut everyone: Vec<String> = c.members.iter().filter(|m| c.admins.contains(m)).cloned().collect();
            everyone.extend(c.members.iter().filter(|m| !c.admins.contains(m)).cloned());
            everyone.retain(|m| *m != me);
            everyone.insert(0, me.clone());
            // Desktop's member search (`GroupMembersSearchDialog`) once the
            // list is long, and its first few until View all.
            const SHOWN: usize = 5;
            let long = everyone.len() > SHOWN;
            if long && let Some(d) = app.panes.details.as_mut() {
                ui.horizontal(|ui| {
                    ui.add_space(12.);
                    ui.set_width(ui.available_width() - 12.);
                    widgets::text_field(ui, &mut d.member_query, tr("Search members"), false);
                });
                ui.add_space(4.);
            }
            let (query, all) = app.panes.details.as_ref().map(|d| (d.member_query.trim().to_lowercase(), d.all_members)).unwrap_or_default();
            let names: Vec<(String, String)> = everyone.into_iter().map(|m| { let n = if m == me { tr("You").to_owned() } else { app.name(&m) }; (m, n) }).collect();
            let matching: Vec<(String, String)> = names.into_iter().filter(|(_, n)| query.is_empty() || n.to_lowercase().contains(&query)).collect();
            // "Show all" only when it would show more than one more.
            let hidden = if query.is_empty() && !all && matching.len() > SHOWN + 1 { matching.len() - SHOWN } else { 0 };
            for (m, name) in matching.into_iter().take(if hidden > 0 { SHOWN } else { usize::MAX }) {
                let you = m == me;
                let is_admin = c.admins.contains(&m);
                let label = c.member_labels.get(&m).map(|l| format!("{} {}", l.emoji.clone().unwrap_or_default(), l.text).trim().to_owned());
                let color = m.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                let photo = if you { app.me.1.clone() } else { app.conversation(&ConversationId::direct(&m)).and_then(|p| p.avatar_path.clone()) };
                let label_color = theme::name_color(&m, palette.dark);
                let row = member_row(ui, palette, &name, label.as_deref().map(|l| (l, label_color)), color, photo.as_deref(), is_admin.then_some(tr("Admin")));
                if !you && row.clicked() {
                    crate::contact_modal::open(app, Some(c.id.clone()), m.clone());
                }
                if !you {
                    crate::menu::context(ui, &row, egui::Id::unique("details-member").with(&m), |menu| {
                        if menu.item(Some("message"), tr("Message")) {
                            app.panes.details = None;
                            app.open(ConversationId::direct(&m));
                        }
                        if admin {
                            if menu.item(Some("key"), if is_admin { tr("Remove as admin") } else { tr("Make admin") }) {
                                crate::contact_modal::confirm_member(app, c.id.clone(), m.clone(), true);
                            }
                            if menu.item(Some("minus-circle"), tr("Remove from group")) {
                                crate::contact_modal::confirm_member(app, c.id.clone(), m.clone(), false);
                            }
                        }
                    });
                }
            }
            if hidden > 0 && axo_list::item(ui, Item::new(tr("See all")).symbol("chevron-down"), End::Click).clicked()
                && let Some(d) = app.panes.details.as_mut()
            {
                d.all_members = true;
            }
        });
    }

    if group && !c.ended {
        axo_list::gap(ui);
        axo_list::list(ui, None, None, |ui| {
            let state = &c.group_state;
            if (admin || state.link.is_some()) && axo_list::item(ui, Item::new(tr("Group link")).symbol("link"), End::Arrow(Some(if state.link.is_some() { tr("On") } else { tr("Off") }))).clicked() {
                set_page(app, Page::GroupLink);
            }
            if !c.left && c.can_label {
                let mine = c.member_labels.get(&me).map(|l| l.text.clone()).unwrap_or_default();
                let emoji = c.member_labels.get(&me).and_then(|l| l.emoji.clone()).unwrap_or_default();
                if axo_list::item(ui, Item::new(tr("Member label")).symbol("label"), End::Arrow(None)).clicked()
                    && let Some(d) = app.panes.details.as_mut()
                {
                    d.label = Some(mine);
                    d.label_emoji = emoji;
                }
            }
            let waiting = state.invites.len() + state.requests.len();
            let count = waiting.to_string();
            if axo_list::item(ui, Item::new(tr("Requests & Invites")).symbol("group"), End::Arrow((waiting > 0).then_some(count.as_str()))).clicked() {
                set_page(app, Page::Requests);
            }
            if admin && axo_list::item(ui, Item::new(tr("Permissions")).symbol("key"), End::Arrow(None)).clicked() {
                set_page(app, Page::Permissions);
            }
        });
    }

    // `ConversationDetailsActions`.
    if c.kind != ConversationKind::NoteToSelf {
        let mut confirm = None;
        axo_list::gap(ui);
        axo_list::list(ui, None, None, |ui| {
            let red = |item: Item<'static>| Item { destructive: true, ..item };
            // Desktop's ConversationDetailsActions: the last admin of a group
            // with others in it can't leave or block it until they pick one.
            let last_admin = group && c.admins.len() == 1 && c.admins.contains(&app.aci) && !c.members.is_empty();
            let tip = tr("Before you leave, you must choose at least one new admin for this group.");
            if group && !c.left && !c.ended {
                let r = axo_list::item(ui, red(Item::new(tr("Leave group")).symbol("leave")).disabled(last_admin), End::Click);
                if last_admin {
                    r.row.on_hover_text(tip);
                } else if r.clicked() {
                    confirm = Some(ChatConfirm::Leave);
                }
            }
            if !c.ended {
                let label = match (group, c.blocked) {
                    (true, false) => tr("Block group"),
                    (true, true) => tr("Unblock group"),
                    (false, false) => tr("Block"),
                    (false, true) => tr("Unblock"),
                };
                let blocked_out = last_admin && !c.blocked;
                let r = axo_list::item(ui, red(Item::new(label).symbol("block")).disabled(blocked_out), End::Click);
                if blocked_out {
                    r.row.on_hover_text(tip);
                } else if r.clicked() {
                    confirm = Some(if c.blocked { ChatConfirm::Unblock } else { ChatConfirm::Block });
                }
            }
            // An ended group can only be archived or deleted.
            if group && c.ended {
                if axo_list::item(ui, Item::new(if c.archived { tr("Unarchive") } else { tr("Archive") }).symbol("archive"), End::Click).clicked() {
                    app.set_archived(c.id.clone(), !c.archived);
                }
                if axo_list::item(ui, red(Item::new(tr("Delete")).symbol("trash")), End::Click).clicked() {
                    app.panes.delete_chat = Some((c.id.clone(), false));
                }
            }
            if !c.blocked && axo_list::item(ui, red(Item::new(tr("Report spam")).symbol("error-octagon")), End::Click).clicked() {
                confirm = Some(ChatConfirm::ReportSpam);
            }
        });
        if group && admin && !c.left && !c.ended {
            axo_list::gap(ui);
            axo_list::list(ui, None, None, |ui| {
                if axo_list::item(ui, Item { destructive: true, ..Item::new(tr("End group")).symbol("x-circle") }, End::Click).clicked() {
                    confirm = Some(ChatConfirm::End);
                }
            });
        }
        if let Some(confirm) = confirm {
            app.panes.chat_confirm = Some((c.id.clone(), confirm));
        }
    }
}

/// A member: their avatar, name and label, and Admin at the end.
/// A group in Groups in common: its photo, name and member count.
fn group_row(ui: &mut Ui, palette: &Palette, g: &Conversation) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 52.), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(14), palette.fill);
    }
    let avatar = egui::Rect::from_min_size(egui::pos2(rect.left() + 12., rect.center().y - 18.), egui::vec2(36., 36.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(g));
    let x = avatar.right() + 12.;
    ui.painter().text(egui::pos2(x, rect.center().y - 1.), egui::Align2::LEFT_BOTTOM, &g.title, theme::regular(13.), palette.text);
    ui.painter().text(egui::pos2(x, rect.center().y + 1.), egui::Align2::LEFT_TOP, crate::i18n::tf("icu:GroupListItem__message-default", &format!("{} members", g.members.len() + 1), &[("count", crate::i18n::Arg::from(&g.members.len() + 1))]), theme::regular(12.), palette.text_secondary);
    response
}

#[allow(clippy::too_many_arguments)]
fn member_row(ui: &mut Ui, palette: &Palette, name: &str, label: Option<(&str, egui::Color32)>, color: u8, photo: Option<&str>, end: Option<&str>) -> egui::Response {
    let width = ui.available_width();
    let h = if label.is_some() { 52. } else { 48. };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(14), palette.fill);
    }
    let avatar = egui::Rect::from_min_size(egui::pos2(rect.left() + 12., rect.center().y - 18.), egui::vec2(36., 36.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo, ..widgets::AvatarSpec::person(name, color) });
    let x = avatar.right() + 12.;
    match label {
        // Desktop's GroupMemberLabel: a pill in the member's name color.
        Some((label, tint)) => {
            ui.painter().text(egui::pos2(x, rect.center().y - 1.), egui::Align2::LEFT_BOTTOM, name, theme::regular(13.), palette.text);
            let g = ui.painter().layout_no_wrap(label.to_owned(), theme::regular(11.), tint);
            let pill = egui::Rect::from_min_size(egui::pos2(x, rect.center().y + 2.), g.size() + egui::vec2(12., 4.));
            ui.painter().rect_filled(pill, CornerRadius::same(8), tint.gamma_multiply(0.15));
            ui.painter().galley(pill.min + egui::vec2(6., 2.), g, tint);
        }
        None => {
            ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, name, theme::regular(13.), palette.text);
        }
    }
    if let Some(end) = end {
        ui.painter().text(egui::pos2(rect.right() - 12., rect.center().y), egui::Align2::RIGHT_CENTER, end, theme::regular(13.), palette.text_secondary);
    }
    response
}

/// `AxoStackedButton.Row`: video, audio (1:1 chats), mute, search. Each an
/// 18px symbol in a pill, its label under it.
fn header_buttons(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let group = c.kind == ConversationKind::Group;
    let callable = c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked && !c.ended;
    let mut buttons: Vec<(&str, &str)> = Vec::new();
    if c.kind != ConversationKind::NoteToSelf && !c.ended {
        buttons.push(("videocamera", "Video"));
        if !group {
            buttons.push(("phone", "Audio"));
        }
    }
    buttons.push(if c.muted { ("bell-slash", "Unmute") } else { ("bell", "Mute") });
    buttons.push(("search", "Search"));
    let w = 56.;
    let total = buttons.len() as f32 * w + (buttons.len() as f32 - 1.) * 4.;
    let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36. + 6. + 14.), egui::Sense::hover());
    let left = row.center().x - total / 2.;
    for (i, (symbol, label)) in buttons.iter().enumerate() {
        let cell = egui::Rect::from_min_size(egui::pos2(left + i as f32 * (w + 4.), row.top()), egui::vec2(w, row.height()));
        let pill = egui::Rect::from_center_size(egui::pos2(cell.center().x, cell.top() + 18.), egui::vec2(48., 36.));
        let response = ui.interact(cell, egui::Id::unique("details-stacked").with(i), egui::Sense::click());
        let fill = if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
        ui.painter().rect_filled(pill, CornerRadius::same(18), fill);
        let disabled = matches!(*label, "Video" | "Audio") && !callable;
        let color = if disabled { palette.text_secondary } else { palette.text };
        theme::paint_symbol(ui.painter(), symbol, pill.center(), 18., color);
        ui.painter().text(egui::pos2(cell.center().x, pill.bottom() + 6.), egui::Align2::CENTER_TOP, tr(label), theme::medium(11.), color);
        let clicked = response.clicked();
        match *label {
            "Video" if clicked && callable => start_call(app, c, true),
            "Audio" if clicked && callable => start_call(app, c, false),
            "Unmute" if clicked => app.send(Command::SetChat { conversation: c.id.clone(), change: ChatChange::MutedUntil(0) }),
            "Mute" => {
                let id = c.id.clone();
                crate::menu::dropdown(ui, &response, egui::Id::unique("details-mute"), |m| crate::panes::mute_items(app, m, &id));
            }
            "Search" if clicked => {
                app.panes.details = None;
                app.find_requested = true;
            }
            _ => {}
        }
    }
}

fn start_call(app: &mut App, c: &Conversation, video: bool) {
    // Desktop's lobby comes first, for 1:1 calls too.
    crate::panes::open_lobby(app, c.id.clone(), video);
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
                crate::widgets::spinner(ui, 24.);
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
                    ui.label(RichText::new(tr("Verified")).font(theme::medium(14.)).color(palette.green));
                }
                ui.add_space(8.);
                let label = if number.verified { tr("Clear verification") } else { tr("Mark as verified") };
                if widgets::pill_button(ui, label, !number.verified).clicked() {
                    app.send(Command::SetVerified { conversation: c.id.clone(), verified: !number.verified });
                }
                if number.auto_verify == langis_core::AutoVerify::Available {
                    ui.add_space(8.);
                    match kt {
                        None => {
                            if widgets::pill_button(ui, tr("Verify automatically"), false).clicked() {
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

/// Desktop's `GroupLinkManagement`.
fn group_link(app: &mut App, ui: &mut Ui, _palette: &Palette, c: &Conversation) {
    use crate::axo_list::{self, End, Item};
    let admin = c.admins.contains(&app.aci);
    let link = c.group_state.link.clone();
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    axo_list::list(ui, None, None, |ui| {
        let mut on = link.is_some();
        let item = Item::new(tr("Group link")).disabled(!admin);
        if axo_list::switch(ui, item, &mut on) {
            app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: if on { LinkChange::On } else { LinkChange::Off } });
        }
        // Desktop's link under the switch copies itself when clicked.
        if let Some(url) = link.as_ref().map(|l| l.url.clone()) {
            ui.horizontal(|ui| {
                ui.add_space(16.);
                let r = ui.add(egui::Label::new(RichText::new(&url).font(theme::regular(12.)).color(_palette.text_secondary)).sense(egui::Sense::click()));
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Copy signal.group link to clipboard")).clicked() {
                    ui.ctx().copy_text(url.clone());
                    app.toast(tr("Copied group link"));
                }
            });
            ui.add_space(8.);
        }
    });
    let Some(link) = link else { return };
    let ask = egui::Id::unique("reset-group-link");
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        if axo_list::item(ui, Item::new(tr("Copy link")).symbol("copy"), End::Click).clicked() {
            ui.ctx().copy_text(link.url.clone());
            app.toast(tr("Copied group link"));
        }
        if axo_list::item(ui, Item::new(tr("Reset link")).symbol("arrow-clockwise"), End::Click).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(ask, true));
        }
    });
    share_link_dialogs(app, ui, &link.url, c);
    if admin {
        axo_list::gap(ui);
        axo_list::list(ui, None, None, |ui| {
            let mut approval = link.approval;
            let item = Item::new(tr("Require admin approval")).description(tr("Require an admin to approve new members joining via the group link"));
            if axo_list::switch(ui, item, &mut approval) {
                app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: LinkChange::Approval(approval) });
            }
        });
    }
    if ui.ctx().data(|d| d.get_temp::<bool>(ask)).unwrap_or(false) {
        let body = tr("Are you sure you want to reset the group link? People will no longer be able to join the group using the current link.");
        let answer = crate::dialog::confirm(ui.ctx(), ask.with("dialog"), body, "", tr("Reset link"), crate::dialog::Variant::StrongDestructive);
        if answer == Some(crate::dialog::Choice::Action) {
            app.send(Command::ChangeGroupLink { conversation: c.id.clone(), change: LinkChange::Reset });
        }
        if answer.is_some() {
            ui.ctx().data_mut(|d| d.remove::<bool>(ask));
        }
    }
}

/// The group link sent to chats picked from a list, or shown as a QR code.
fn share_link_dialogs(app: &mut App, ui: &mut Ui, url: &str, c: &Conversation) {
    let palette = Palette::of(ui.ctx());
    if app.panes.details.as_ref().is_some_and(|d| d.link_qr) {
        let mut done = false;
        let closed = crate::panes::modal(ui, "group-link-qr", 340., |ui| {
            crate::panes::title(ui, &c.title);
            ui.vertical_centered(|ui| {
                if let Some(qr) = crate::startup::qr_texture_of(ui.ctx(), &format!("group-link:{url}"), url.as_bytes()) {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(240., 240.), egui::Sense::hover());
                    ui.painter().rect_filled(rect.expand(8.), CornerRadius::same(12), egui::Color32::WHITE);
                    egui::Image::new((qr.id(), rect.size())).paint_at(ui, rect);
                }
                ui.add_space(12.);
                body(ui, &palette, "Anyone with a Signal account can scan this to ask to join the group.");
            });
            done = crate::panes::buttons(ui, &[tr("Done")]).is_some();
        });
        if (done || closed)
            && let Some(d) = app.panes.details.as_mut()
        {
            d.link_qr = false;
        }
    }
    let Some((mut chosen, mut query)) = app.panes.details.as_mut().and_then(|d| d.share_link.take()) else { return };
    let mut answer = None;
    let closed = crate::panes::modal(ui, "share-group-link", 400., |ui| {
        crate::panes::title(ui, "Share group link");
        widgets::text_field(ui, &mut query, tr("Search"), false);
        ui.add_space(6.);
        let q = query.trim().to_lowercase();
        egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
            for chat in app.conversations.iter().filter(|x| !x.left && !x.blocked && x.id != c.id && (q.is_empty() || x.title.to_lowercase().contains(&q))) {
                let mut on = chosen.contains(&chat.id);
                if crate::widgets::checkbox(ui, &mut on, &chat.title).changed() {
                    if on {
                        chosen.push(chat.id.clone());
                    } else {
                        chosen.retain(|x| *x != chat.id);
                    }
                }
            }
        });
        answer = crate::panes::buttons(ui, &[tr("Send"), tr("Cancel")]);
    });
    match answer {
        Some(0) if !chosen.is_empty() => {
            for to in &chosen {
                app.send(Command::SendText { conversation: to.clone(), body: url.to_owned(), quote: None, preview: None, mentions: Vec::new(), styles: Vec::new() });
            }
            app.toast(if chosen.len() == 1 { "Link sent.".to_owned() } else { format!("Link sent to {} chats.", chosen.len()) });
        }
        Some(_) => {}
        None if closed => {}
        None => {
            if let Some(d) = app.panes.details.as_mut() {
                d.share_link = Some((chosen, query));
            }
        }
    }
}

/// Desktop's `PendingInvites`: tabs for Requests and Invites; a request
/// with Approve and Deny (asked first), invites ours one by one and others'
/// by who sent them, each revoked after asking.
fn requests(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    use crate::i18n::{Arg, tf};
    let admin = c.admins.contains(&app.aci);
    let state = c.group_state.clone();
    let tab_id = egui::Id::unique(("requests-tab", c.id.0.as_str()));
    let mut tab: usize = ui.ctx().data(|d| d.get_temp(tab_id)).unwrap_or(if state.requests.is_empty() && !state.invites.is_empty() { 1 } else { 0 });
    ui.horizontal(|ui| {
        let half = ui.available_width() / 2.;
        let labels = [
            tf("icu:PendingInvites--tab-requests", &format!("Requests ({})", state.requests.len()), &[("count", Arg::from(state.requests.len()))]),
            tf("icu:PendingInvites--tab-invites", &format!("Invites ({})", state.invites.len()), &[("count", Arg::from(state.invites.len()))]),
        ];
        for (i, label) in labels.iter().enumerate() {
            let (r, hit) = ui.allocate_exact_size(Vec2::new(half - 4., 36.), Sense::click());
            let on = tab == i;
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, theme::semibold(14.), if on { palette.text } else { palette.text_secondary });
            if on {
                ui.painter().hline(r.x_range().shrink(half / 4.), r.bottom() - 1., egui::Stroke::new(2., palette.text));
            }
            if hit.clicked() {
                tab = i;
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(tab_id, tab));
    ui.add_space(10.);
    // An icon button for a row's end.
    let icon = |ui: &mut Ui, rect: Rect, symbol: &str, tip: &str| -> bool {
        let hit = ui.interact(rect, ui.scope_id().with((symbol, rect.min.x as i32, rect.min.y as i32)), Sense::click());
        ui.painter().circle_filled(rect.center(), 15., if hit.hovered() { palette.fill_pressed } else { palette.fill });
        theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., palette.text);
        hit.on_hover_text(tip).clicked()
    };
    if tab == 0 {
        section(ui, palette, |ui| {
            for r in &state.requests {
                let name = app.name(&r.aci);
                let row = crate::panes::contact_row(ui, palette, &name, None, 2, None);
                if admin {
                    let deny = Rect::from_center_size(egui::pos2(row.rect.right() - 22., row.rect.center().y), Vec2::splat(30.));
                    let approve = deny.translate(Vec2::new(-38., 0.));
                    if icon(ui, approve, "check", tr("Approve Request")) {
                        let title = tf("icu:PendingRequests--approve-for", &format!("Approve request from \u{201c}{name}\u{201d}?"), &[("name", Arg::from(&name))]);
                        crate::panes::ask(app, "", title, tr("Approve Request"), false, vec![Command::AnswerJoinRequest { conversation: c.id.clone(), member: r.aci.clone(), approve: true }]);
                    }
                    if icon(ui, deny, "x", tr("Deny Request")) {
                        let title = if state.link.is_some() {
                            tf("icu:PendingRequests--deny-for--with-link", &format!("Deny request from \u{201c}{name}\u{201d}? They will not be able to request to join via the group link again."), &[("name", Arg::from(&name))])
                        } else {
                            tf("icu:PendingRequests--deny-for", &format!("Deny request from \u{201c}{name}\u{201d}?"), &[("name", Arg::from(&name))])
                        };
                        crate::panes::ask(app, "", title, tr("Deny Request"), true, vec![Command::AnswerJoinRequest { conversation: c.id.clone(), member: r.aci.clone(), approve: false }]);
                    }
                }
            }
        });
        let info = tf("icu:PendingRequests--info", &format!("People on this list are attempting to join \u{201c}{}\u{201d} via the group link.", c.title), &[("name", Arg::from(&c.title))]);
        body(ui, palette, &info);
        return;
    }
    let (ours, theirs): (Vec<_>, Vec<_>) = state.invites.iter().partition(|i| i.by == app.aci);
    if !ours.is_empty() {
        ui.label(RichText::new(tr("Invited by you")).font(theme::semibold(14.)));
        ui.add_space(6.);
        section(ui, palette, |ui| {
            for invite in &ours {
                let name = app.name(&invite.service_id);
                let row = crate::panes::contact_row(ui, palette, &name, None, 5, None);
                let x = Rect::from_center_size(egui::pos2(row.rect.right() - 22., row.rect.center().y), Vec2::splat(30.));
                if icon(ui, x, "x", tr("Revoke group invite")) {
                    let title = tf("icu:PendingInvites--revoke-for", &format!("Revoke group invite for \u{201c}{name}\u{201d}?"), &[("name", Arg::from(&name))]);
                    crate::panes::ask(app, "", title, tr("Revoke"), true, vec![Command::RevokeGroupInvites { conversation: c.id.clone(), invitees: vec![invite.service_id.clone()] }]);
                }
            }
        });
    }
    if !theirs.is_empty() {
        ui.label(RichText::new(tr("Invited by others")).font(theme::semibold(14.)));
        ui.add_space(6.);
        section(ui, palette, |ui| {
            let mut inviters: Vec<&str> = theirs.iter().map(|i| i.by.as_str()).collect();
            inviters.sort_unstable();
            inviters.dedup();
            for by in inviters {
                let invitees: Vec<String> = theirs.iter().filter(|i| i.by == by).map(|i| i.service_id.clone()).collect();
                let who = app.name(by);
                let count = tf("icu:PendingInvites--invited-count", &format!("Invited {}", invitees.len()), &[("number", Arg::from(invitees.len()))]);
                let row = crate::panes::contact_row(ui, palette, &who, Some(&count), 5, None);
                if admin {
                    let x = Rect::from_center_size(egui::pos2(row.rect.right() - 22., row.rect.center().y), Vec2::splat(30.));
                    if icon(ui, x, "x", tr("Revoke group invite")) {
                        let n = invitees.len();
                        let english = if n == 1 { format!("Revoke 1 invite sent by \u{201c}{who}\u{201d}?") } else { format!("Revoke {n} invites sent by \u{201c}{who}\u{201d}?") };
                        let title = tf("icu:PendingInvites--revoke-from", &english, &[("number", Arg::from(n)), ("name", Arg::from(&who))]);
                        crate::panes::ask(app, "", title, tr("Revoke"), true, vec![Command::RevokeGroupInvites { conversation: c.id.clone(), invitees: invitees.clone() }]);
                    }
                }
            }
        });
    }
    if state.invites.is_empty() {
        body(ui, palette, tr("No pending invites"));
        ui.add_space(8.);
    }
    body(ui, palette, tr("Details about people invited to this group aren\u{2019}t shown until they join. Invitees will only see messages after they join the group."));
}

fn permissions(app: &mut App, ui: &mut Ui, _palette: &Palette, c: &Conversation) {
    let ask = egui::Id::unique("confirm-label-access");
    if ui.ctx().data(|d| d.get_temp::<bool>(ask)).unwrap_or(false) {
        let body_text = "Members who aren't admins will lose their member labels, and only admins will be able to set them.";
        match crate::dialog::confirm(ui.ctx(), ask.with("dialog"), "Only admins can set labels?", body_text, tr("Change"), crate::dialog::Variant::StrongPrimary) {
            Some(crate::dialog::Choice::Action) => {
                app.send(Command::SetGroupPermission { conversation: c.id.clone(), permission: Permission::SetMemberLabels { admins_only: true } });
                ui.ctx().data_mut(|d| d.remove::<bool>(ask));
            }
            Some(crate::dialog::Choice::Cancel) => ui.ctx().data_mut(|d| d.remove::<bool>(ask)),
            None => {}
        }
    }
    let state = &c.group_state;
    crate::axo_list::list(ui, None, None, |ui| {
        // Desktop's GroupV2Permissions rows: a select of All members or Only
        // admins, with what it's for under it.
        let choices: [(&str, &str, bool, fn(bool) -> Permission, bool); 4] = [
            (tr("Who can add members"), tr("Choose who can add members to this group."), state.members_admins_only, |a| Permission::AddMembers { admins_only: a }, false),
            (tr("Who can edit group info"), tr("Choose who can edit group name, photo, description, disappearing messages timer, and pinned messages."), state.info_admins_only, |a| Permission::EditInfo { admins_only: a }, false),
            (tr("Who can send messages"), tr("Choose who can send messages to the group."), state.announcements_only, |a| Permission::SendMessages { admins_only: a }, false),
            (tr("Who can add member labels"), tr("Choose who can add member labels in this group."), state.labels_admins_only, |a| Permission::SetMemberLabels { admins_only: a }, true),
        ];
        for (label, info, admins_only, make, labels) in choices {
            let options = [tr("All members"), tr("Only admins")];
            if let Some(i) = crate::axo_list::select(ui, egui::Id::unique(("group-permission", label)), crate::axo_list::Item::new(label).description(info), &options, Some(admins_only as usize)) {
                let value = i == 1;
                if value != admins_only {
                    // Desktop warns that admins-only labels takes away the
                    // labels non-admins have.
                    let labelled = c.member_labels.keys().any(|m| !c.admins.contains(m));
                    if labels && value && labelled {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("confirm-label-access"), true));
                    } else {
                        app.send(Command::SetGroupPermission { conversation: c.id.clone(), permission: make(value) });
                    }
                }
            }
        }
    });
}

/// The details pane's dialogs: edit info, nickname, add members, confirms.
fn dialogs(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    let Some(mut d) = app.panes.details.take() else { return };
    if let Some((mut name, mut description)) = d.edit_info.take() {
        let mut answer = None;
        let mut open_avatar_editor = false;
        let closed = modal(ui, "edit-group", 400., |ui| {
            title(ui, tr("Edit group"));
            // Desktop's AvatarPreview: a click opens the AvatarEditor.
            ui.vertical_centered(|ui| {
                let r = widgets::avatar(ui, &widgets::AvatarSpec::of(c), 80.).interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                if r.on_hover_text(tr("Edit photo")).clicked() {
                    open_avatar_editor = true;
                }
            });
            ui.add_space(12.);
            // The name (32) and description (480), with their counters as
            // they near the end.
            let field = |ui: &mut Ui, text: &mut String, hint: &str, max: usize, rows: usize| {
                ui.add(egui::TextEdit::multiline(text).hint_text(hint).char_limit(max).desired_rows(rows).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10, 8)));
                let left = max.saturating_sub(text.chars().count());
                if left <= if max > 100 { 100 } else { 10 } {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.label(RichText::new(left.to_string()).font(theme::regular(12.)).color(palette.text_secondary));
                    });
                }
            };
            field(ui, &mut name, tr("Group name (required)"), 32, 1);
            ui.add_space(8.);
            field(ui, &mut description, tr("Description"), 480, 3);
            ui.add_space(6.);
            body(ui, palette, tr("Group descriptions will be visible to members of this group and people who have been invited."));
            answer = buttons(ui, &[tr("Save"), tr("Cancel")]);
        });
        let current = (name.clone(), description.clone());
        let leave = crate::panes::discard_guard(ui.ctx(), "edit-group", &current, answer == Some(1) || (closed && answer.is_none()), answer == Some(0));
        match answer {
            Some(0) => {
                let title = (name.trim() != c.title && !name.trim().is_empty()).then(|| name.trim().to_owned());
                let desc = (Some(description.trim()) != c.description.as_deref().map(str::trim)).then(|| description.trim().to_owned());
                if title.is_some() || desc.is_some() {
                    app.send(Command::UpdateGroup { conversation: c.id.clone(), title, description: desc });
                }
            }
            _ if leave => {}
            _ => d.edit_info = Some((name, description)),
        }
        if open_avatar_editor {
            d.group_avatar = Some(crate::avatar_editor::AvatarEditor::new(c.avatar_path.clone(), &c.title));
        }
    }
    // The group's AvatarEditor.
    if let Some(editor) = d.group_avatar.as_mut() {
        let mut toast = None;
        if let Some(change) = editor.show(ui, &mut toast) {
            d.group_avatar = None;
            match change {
                langis_core::profiles::AvatarChange::New(bytes) => app.send(Command::SetGroupAvatar { conversation: c.id.clone(), image: Some(bytes.to_vec()) }),
                langis_core::profiles::AvatarChange::Remove => app.send(Command::SetGroupAvatar { conversation: c.id.clone(), image: None }),
                langis_core::profiles::AvatarChange::Keep => {}
            }
        }
        if let Some(t) = toast {
            app.toast(t);
        }
    }
    if let Some(mut nick) = d.nickname.take() {
        let mut answer = None;
        let closed = modal(ui, "nickname", 380., |ui| {
            title(ui, tr("Nickname"));
            body(ui, palette, tr("Nicknames & notes are stored with Signal and end-to-end encrypted. They are only visible to you."));
            ui.add_space(8.);
            widgets::text_field(ui, &mut nick.given, tr("First name"), false);
            ui.add_space(6.);
            widgets::text_field(ui, &mut nick.family, tr("Last name"), false);
            ui.add_space(6.);
            widgets::text_field(ui, &mut nick.note, tr("Note"), false);
            answer = buttons(ui, &[tr("Save"), tr("Cancel"), tr("Delete")]);
        });
        let leave = crate::panes::discard_guard(ui.ctx(), "nickname", &nick, answer == Some(1) || (closed && answer.is_none()), matches!(answer, Some(0 | 2)));
        match answer {
            Some(0) => app.send(Command::SetNickname { conversation: c.id.clone(), nickname: nick }),
            Some(2) => crate::panes::ask(
                app,
                tr("Delete nickname?"),
                tr("This will permanently delete this nickname and note."),
                tr("Delete"),
                true,
                vec![Command::SetNickname { conversation: c.id.clone(), nickname: ContactNickname::default() }],
            ),
            _ if leave => {}
            _ => d.nickname = Some(nick),
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
            // Desktop's `GroupMemberLabelEditor`: an emoji, then the text.
            ui.horizontal(|ui| {
                let face = if d.label_emoji.is_empty() { "🙂".to_owned() } else { d.label_emoji.clone() };
                let mut job = egui::text::LayoutJob::default();
                crate::chat::append_with_emoji(&mut job, &face, egui::TextFormat { font_id: theme::emoji(18.), color: palette.text, ..Default::default() });
                if ui.add(egui::Button::new(job).corner_radius(CornerRadius::same(8))).on_hover_text("Choose an emoji").clicked() {
                    d.label_picker = if d.label_picker.is_some() { None } else { Some(Default::default()) };
                }
                widgets::text_field(ui, &mut text, "Label", false);
            });
            if !d.label_emoji.is_empty() && ui.link("Remove emoji").clicked() {
                d.label_emoji.clear();
            }
            if let Some(picker) = d.label_picker.as_mut() {
                ui.add_space(6.);
                if let Some(e) = picker.body(ui, &app.recent_emoji) {
                    d.label_emoji = e.to_owned();
                    d.label_picker = None;
                }
            }
            answer = buttons(ui, &[tr("Save"), tr("Cancel")]);
        });
        let emoji = Some(d.label_emoji.clone()).filter(|e| !e.is_empty());
        match answer {
            Some(0) => {
                d.label_picker = None;
                app.send(Command::SetMemberLabel { conversation: c.id.clone(), emoji, text });
            }
            Some(_) => {}
            None if closed => {}
            None => d.label = Some(text),
        }
    }
    if let Some((mut query, mut chosen)) = d.add_members.take() {
        let mut answer = None;
        let closed = modal(ui, "add-members", 400., |ui| {
            title(ui, tr("Add members"));
            widgets::text_field(ui, &mut query, tr("Search"), false);
            ui.add_space(6.);
            let q = query.trim().to_lowercase();
            let people: Vec<langis_store::Conversation> = app.conversations.iter().filter(|p| p.kind == ConversationKind::Direct && !p.blocked && (q.is_empty() || p.title.to_lowercase().contains(&q))).cloned().collect();
            egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                for p in &people {
                    let Some(sid) = p.id.as_direct() else { continue };
                    // Desktop's ChooseGroupMembersModal lists members too,
                    // checked and disabled: "Already a member".
                    let member = c.members.iter().any(|m| m == sid);
                    let on = member || chosen.iter().any(|m| m == sid);
                    let row = ui.add_enabled_ui(!member, |ui| {
                        let r = crate::panes::contact_row(ui, palette, &p.title, if member { Some(tr("Already a member")) } else { p.phone.as_deref() }, p.color, p.avatar_path.as_deref());
                        crate::panes::round_check(ui, palette, egui::pos2(r.rect.right() - 20., r.rect.center().y), on);
                        r
                    });
                    if !member && row.inner.clicked() {
                        if on {
                            chosen.retain(|m| m != sid);
                        } else if crate::panes::may_add(app, c.members.len() + chosen.len() + 2) {
                            chosen.push(sid.to_owned());
                        }
                    }
                }
            });
            answer = buttons(ui, &[&format!("Add ({})", chosen.len()), tr("Cancel")]);
        });
        match answer {
            Some(0) if !chosen.is_empty() => app.send(Command::AddGroupMembers { conversation: c.id.clone(), members: chosen }),
            Some(_) => {}
            None if closed => {}
            None => d.add_members = Some((query, chosen)),
        }
    }
    if app.panes.details.is_none() {
        app.panes.details = Some(d);
    }
}
