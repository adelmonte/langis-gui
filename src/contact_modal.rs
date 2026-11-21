//! Desktop's `ContactModal`: a person, clicked in a group (their avatar or
//! name on a message, or a member in the group's details). Their photo and
//! name, Message, Video and Voice, then nickname, block, safety number and
//! the admin's actions.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Vec2};
use langis_core::Command;
use langis_store::ConversationId;

use crate::app::App;
use crate::dialog::{self, Answer, Variant};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

pub struct Contact {
    /// The group it was opened from, for the admin's actions.
    pub group: Option<ConversationId>,
    pub aci: String,
    confirm: Option<Confirm>,
    /// Opened only for a confirmation (from the details' member menu):
    /// closes with it.
    confirm_only: bool,
    /// Their about, number and verification, for the About view.
    about: Option<langis_core::ContactDetails>,
    pub about_open: bool,
    /// Add to a group: the groups' search, while picking.
    pub add_to_group: Option<String>,
    /// Desktop's `GroupMemberLabelInfoModal`, from the label's pill.
    pub label_info: bool,
    /// Ourselves (About You): our profile.
    pub own: Option<langis_core::OwnProfile>,
}

#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    Admin,
    Remove,
    /// Desktop's call link "Remove {name} from the call?": Block from call
    /// or Remove (a demux id).
    Call(u32),
}

/// The member menu's Make admin / Remove as admin (`admin`) or Remove from
/// group, asked as `ContactModal` asks, without the modal itself.
pub fn confirm_member(app: &mut App, group: ConversationId, aci: String, admin: bool) {
    app.panes.contact = Some(Contact {
        group: Some(group),
        aci,
        confirm: Some(if admin { Confirm::Admin } else { Confirm::Remove }),
        confirm_only: true,
        about: None,
        about_open: false,
        add_to_group: None,
        label_info: false,
        own: None,
    });
}

/// A call tile's Remove from call (Desktop's `CallingParticipantMenu`):
/// its Block from call or Remove, without the modal itself.
pub fn confirm_remove_from_call(app: &mut App, aci: String, demux_id: u32) {
    app.panes.contact = Some(Contact {
        group: None,
        aci,
        confirm: Some(Confirm::Call(demux_id)),
        confirm_only: true,
        about: None,
        about_open: false,
        add_to_group: None,
        label_info: false,
        own: None,
    });
}

pub fn open(app: &mut App, group: Option<ConversationId>, aci: String) {
    // Signal isn't a person: Desktop has no ContactModal or About for its
    // chat, whose name opens the chat's details.
    if aci == langis_store::SIGNAL_ACI {
        crate::details::open(app, ConversationId::signal());
        return;
    }
    if aci == app.aci {
        // Desktop's About You.
        app.panes.contact = Some(Contact { group, aci: aci.clone(), confirm: None, confirm_only: false, about: None, about_open: true, add_to_group: None, label_info: false, own: None });
        app.load(move |db| async move {
            let p = langis_core::own_profile(&db, &aci).await;
            Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
                if let Some(c) = app.panes.contact.as_mut().filter(|c| c.aci == aci) {
                    c.own = Some(p);
                }
            })))
        });
        return;
    }
    app.panes.contact = Some(Contact { group, aci: aci.clone(), confirm: None, confirm_only: false, about: None, about_open: false, add_to_group: None, label_info: false, own: None });
    app.load(move |db| async move {
        let details = langis_core::contact_details(&db, &aci).await;
        Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(c) = app.panes.contact.as_mut().filter(|c| c.aci == aci) {
                c.about = Some(details);
            }
        })))
    });
}

enum Do {
    Close,
    About,
    AddToGroup,
    MuteInCall(u32),
    RemoveFromCall(u32),
    Message,
    Call(bool),
    Nickname,
    Block,
    Safety,
    Confirm(Confirm),
    /// A row that only says something ("{name} is blocked").
    Nothing,
    LabelInfo,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(contact) = app.panes.contact.as_ref() else { return };
    let (aci, group_id, confirm) = (contact.aci.clone(), contact.group.clone(), contact.confirm);
    let palette = Palette::of(ctx);
    let direct = ConversationId::direct(&aci);
    let name = app.name(&aci);
    let person = app.conversation(&direct).cloned();
    let group = group_id.as_ref().and_then(|g| app.conversation(g)).cloned();
    let we_admin = group.as_ref().is_some_and(|g| g.admins.contains(&app.aci) && !g.left && !g.ended);
    let is_member = group.as_ref().is_some_and(|g| g.members.contains(&aci));
    let is_admin = group.as_ref().is_some_and(|g| g.admins.contains(&aci));
    let in_call = app.call.is_some();

    if let Some(Confirm::Call(demux_id)) = confirm {
        let body = crate::i18n::tf("icu:CallingAdhocCallInfo__RemoveClientDialogBody", &format!("Remove {name} from the call?"), &[("name", crate::i18n::Arg::from(&name))]);
        let actions = [(tr("Block from call"), Variant::StrongDestructive), (tr("Remove"), Variant::StrongDestructive)];
        match dialog::show(ctx, egui::Id::unique("contact-call-confirm"), "", &body, Some(tr("Cancel")), &actions) {
            Some(Answer::Action(i)) => {
                app.send(Command::RemoveFromCall { demux_id, block: i == 0 });
                app.panes.contact = None;
            }
            Some(Answer::Cancel) if app.panes.contact.as_ref().is_some_and(|c| c.confirm_only) => app.panes.contact = None,
            Some(Answer::Cancel) => {
                if let Some(c) = app.panes.contact.as_mut() {
                    c.confirm = None;
                }
            }
            None => {}
        }
        return;
    }
    if let Some(which) = confirm {
        // `ContactModal`'s confirmations, over it.
        let (title, body, action) = match which {
            Confirm::Admin if is_admin => {
                // Desktop: with admin-only labels, losing admin loses the label.
                let labelled = group.as_ref().is_some_and(|g| g.group_state.labels_admins_only && g.member_labels.contains_key(&aci));
                let body = if labelled { tr("This will also clear their member label.").to_owned() } else { String::new() };
                (crate::i18n::tf("icu:ContactModal--rm-admin-info", &format!("Remove {name} as group admin?"), &[("contact", crate::i18n::Arg::from(&name))]), body, tr("Remove as admin"))
            }
            Confirm::Admin => (String::new(), crate::i18n::tf("icu:ContactModal--make-admin-info", &format!("{name} will be able to edit this group and its members."), &[("contact", crate::i18n::Arg::from(&name))]), tr("Make admin")),
            Confirm::Remove => (String::new(), format!("Remove \"{name}\" from the group?"), tr("Remove from group")),
            Confirm::Call(_) => return,
        };
        let variant = if which == Confirm::Remove || is_admin { Variant::StrongDestructive } else { Variant::StrongPrimary };
        if let Some(answer) = dialog::show(ctx, egui::Id::unique("contact-confirm"), &title, &body, Some(tr("Cancel")), &[(action, variant)]) {
            if let (Answer::Action(_), Some(g)) = (answer, group_id.clone()) {
                match which {
                    Confirm::Admin => app.send(Command::SetGroupAdmin { conversation: g, member: aci.clone(), admin: !is_admin }),
                    Confirm::Remove => app.send(Command::RemoveFromGroup { conversation: g, service_id: aci.clone() }),
                    Confirm::Call(_) => {}
                }
            }
            if app.panes.contact.as_ref().is_some_and(|c| c.confirm_only) {
                app.panes.contact = None;
            } else if let Some(c) = app.panes.contact.as_mut() {
                c.confirm = None;
            }
        }
        return;
    }

    if aci == app.aci {
        // About You alone, without the contact modal.
        about_and_add(app, ctx, &aci, &name);
        if app.panes.contact.as_ref().is_some_and(|c| !c.about_open) {
            app.panes.contact = None;
        }
        return;
    }
    if app.panes.contact.as_ref().is_some_and(|c| c.label_info) {
        label_info(app, ctx, &palette, group.as_ref());
        return;
    }

    let mut todo = None;
    let mut lightbox: Option<String> = None;
    let mut story: Option<String> = None;
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let modal = egui::Modal::new(egui::Id::unique("contact-modal"))
        .frame(egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).inner_margin(egui::Margin { left: 0, right: 0, top: 12, bottom: 16 }))
        .show(ctx, |ui| {
            // AxoDialog `sm`: 320 wide, the header floating over the body.
            let width = 320f32.min(ctx.content_rect().width() - 32.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (head, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            let x = Rect::from_center_size(Pos2::new(head.right() - 32., head.center().y), Vec2::splat(32.));
            let response = ui.interact(x, ui.scope_id().with("close"), Sense::click());
            if response.hovered() {
                ui.painter().circle_filled(x.center(), 16., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "x", x.center(), 20., palette.text);
            if response.on_hover_text("Close").clicked() {
                todo = Some(Do::Close);
            }

            // The avatar, 80, and the name in `font-title-1` (20px), regular.
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 80.), Sense::hover());
            let rect = Rect::from_center_size(row.center(), Vec2::splat(80.));
            match &person {
                Some(p) => {
                    widgets::paint_avatar(ui, rect, &widgets::AvatarSpec::of(p));
                    // Desktop's ring for their stories (a click views them),
                    // their badge, and a held-back photo downloading on click.
                    let ring = crate::stories::of_chat(app, p);
                    if let Some((_, unread)) = &ring {
                        ui.painter().circle_stroke(rect.center(), 43., egui::Stroke::new(2.5, if *unread { palette.badge } else { palette.text_secondary }));
                    }
                    if let Some(badge) = app.badge_for(&aci) {
                        widgets::paint_badge(ui, rect, badge);
                    }
                    let hit = ui.interact(rect, ui.scope_id().with("contact-photo"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if hit.clicked() {
                        if let Some((k, _)) = ring {
                            story = Some(k);
                        } else if p.avatar_withheld {
                            app.send(Command::DownloadPhoto { conversation: p.id.clone() });
                        } else if let Some(path) = p.avatar_path.clone() {
                            lightbox = Some(path);
                        }
                    }
                }
                None => {
                    let color = aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                    widgets::paint_avatar(ui, rect, &widgets::AvatarSpec::person(&name, color));
                }
            }
            ui.add_space(12.);
            let mut job = egui::text::LayoutJob::simple_singleline(name.clone(), theme::regular(20.), palette.text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(width - 48.);
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, galley.size().y), Sense::hover());
            let name_rect = Rect::from_min_size(Pos2::new(row.center().x - galley.size().x / 2., row.top()), galley.size());
            ui.painter().galley(name_rect.min, galley, palette.text);
            // After the name: the contacts icon, then the chevron to About.
            let mut x = name_rect.right() + 6.;
            if person.as_ref().is_some_and(|p| p.system_contact) {
                theme::paint_symbol(ui.painter(), "person-circle", Pos2::new(x + 7., name_rect.center().y), 14., palette.text_secondary);
                x += 18.;
            }
            crate::icons::Icon::ChevronRight.paint(ui, Rect::from_center_size(Pos2::new(x + 8., name_rect.center().y), Vec2::splat(16.)), 16., palette.text_secondary);
            let name_rect = name_rect.union(Rect::from_min_size(Pos2::new(x, name_rect.top()), Vec2::new(16., name_rect.height())));
            // Desktop's name opens `AboutContactModal`.
            if ui.interact(name_rect, ui.scope_id().with("contact-name"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                todo = Some(Do::About);
            }
            if let Some(label) = group.as_ref().and_then(|g| g.member_labels.get(&aci)) {
                // The member's label in the group, a pill under the name.
                ui.add_space(6.);
                let text = format!("{} {}", label.emoji.clone().unwrap_or_default(), label.text).trim().to_owned();
                let color = theme::name_color(&aci, palette.dark);
                let galley = ui.painter().layout_no_wrap(text, theme::medium(12.), color);
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, 18.), Sense::hover());
                let pill = Rect::from_center_size(row.center(), galley.size() + Vec2::new(12., 2.));
                ui.painter().rect_filled(pill, CornerRadius::same(9), color.gamma_multiply(0.15));
                ui.painter().galley(pill.center() - galley.size() / 2., galley, color);
                // Desktop: the label explains member labels.
                if ui.interact(pill, ui.scope_id().with("label-pill"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    todo = Some(Do::LabelInfo);
                }
                ui.add_space(4.);
            }

            // `AxoStackedButton.Row`: Message, Video, Voice.
            ui.add_space(16.);
            let buttons = [("message", tr("Message")), ("videocamera", tr("Video")), ("phone", tr("Voice"))];
            let cell_w = 72.;
            let total = buttons.len() as f32 * cell_w + (buttons.len() - 1) as f32 * 8.;
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 36. + 6. + 14.), Sense::hover());
            for (i, (symbol, label)) in buttons.into_iter().enumerate() {
                let cell = Rect::from_min_size(Pos2::new(row.center().x - total / 2. + i as f32 * (cell_w + 8.), row.top()), Vec2::new(cell_w, row.height()));
                let pill = Rect::from_center_size(Pos2::new(cell.center().x, cell.top() + 18.), Vec2::new(cell_w, 36.));
                let response = ui.interact(cell, ui.scope_id().with(("quick", i)), Sense::click());
                let discouraged = i > 0 && in_call;
                let fill = if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
                ui.painter().rect_filled(pill, CornerRadius::same(18), fill);
                let color = if discouraged { palette.text_secondary } else { palette.text };
                theme::paint_symbol(ui.painter(), symbol, pill.center(), 18., color);
                ui.painter().text(Pos2::new(cell.center().x, pill.bottom() + 6.), Align2::CENTER_TOP, label, theme::medium(11.), color);
                let response = if discouraged { response.on_hover_text("You're already in a call") } else { response };
                if response.clicked() && !discouraged {
                    todo = Some(match i {
                        0 => Do::Message,
                        1 => Do::Call(true),
                        _ => Do::Call(false),
                    });
                }
            }

            // `ContactModal__divider`, then the buttons.
            ui.add_space(16.);
            let (line, _) = ui.allocate_exact_size(Vec2::new(width, 1.), Sense::hover());
            ui.painter().hline(line.shrink2(Vec2::new(24., 0.)).x_range(), line.center().y, egui::Stroke::new(1., palette.border));
            ui.add_space(12.);
            let blocked = person.as_ref().is_some_and(|p| p.blocked);
            let mut items: Vec<(&str, String, Do)> = vec![("pencil", tr("Nickname").into(), Do::Nickname)];
            if blocked {
                items.push(("block", crate::i18n::tf("icu:AboutContactModal__blocked", &format!("{name} is blocked"), &[("name", crate::i18n::Arg::from(&name))]), Do::Nothing));
            } else {
                items.push(("block", tr("Block").into(), Do::Block));
            }
            items.push(("shield-check", tr("View Safety Number").into(), Do::Safety));
            if we_admin && is_member {
                items.push(("key", if is_admin { tr("Remove as admin") } else { tr("Make admin") }.into(), Do::Confirm(Confirm::Admin)));
                items.push(("minus-circle", tr("Remove from group").into(), Do::Confirm(Confirm::Remove)));
            }
            // `ContactModal--add-to-group`, when some group can take them.
            let can_add = app.conversations.iter().any(|g| g.kind == langis_store::ConversationKind::Group && g.can_add_members && !g.left && !g.members.contains(&aci));
            if can_add {
                items.push(("group", tr("Add to another group").into(), Do::AddToGroup));
            }
            // In a call with them: mute them, or (a call link's admin) remove.
            if let Some(call) = app.call.as_ref()
                && let Some(p) = call.participants.iter().find(|p| p.aci == aci)
            {
                if p.audio {
                    items.push(("mic-slash", tr("Mute audio").into(), Do::MuteInCall(p.demux_id)));
                }
                // Desktop's `isRemoveFromCallVisible`: a link we administer.
                let link_admin = call.link.as_ref().is_some_and(|l| l.admin);
                if call.conversation.as_call_link().is_some() && link_admin {
                    items.push(("minus-circle", tr("Remove from call").into(), Do::RemoveFromCall(p.demux_id)));
                }
            }
            for (symbol, label, action) in items {
                let (row, response) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::click());
                if response.hovered() {
                    let hover = if palette.dark { theme::rgb(0x2e2e2e) } else { theme::rgb(0xf6f6f6) };
                    ui.painter().rect_filled(row, 0, hover);
                }
                theme::paint_symbol(ui.painter(), symbol, Pos2::new(row.left() + 24. + 10., row.center().y), 20., palette.text);
                ui.painter().text(Pos2::new(row.left() + 24. + 32., row.center().y), Align2::LEFT_CENTER, label, theme::regular(14.), palette.text);
                if response.clicked() {
                    todo = Some(action);
                }
            }
        });
    if modal.should_close() && todo.is_none() {
        todo = Some(Do::Close);
    }
    about_and_add(app, ctx, &aci, &name);
    let Some(todo) = todo else { return };
    if lightbox.is_some() {
        app.panes.avatar_lightbox = lightbox;
    }
    if let Some(k) = story {
        app.panes.contact = None;
        crate::stories::view(app, k);
        return;
    }
    match todo {
        Do::About => {
            if let Some(c) = app.panes.contact.as_mut() {
                c.about_open = true;
            }
            return;
        }
        Do::AddToGroup => {
            if let Some(c) = app.panes.contact.as_mut() {
                c.add_to_group = Some(String::new());
            }
            return;
        }
        Do::MuteInCall(demux_id) => {
            let body = crate::i18n::tf("icu:ContactModal--confirm-mute-body", &format!("Are you sure you want to mute {name}?"), &[("contact", crate::i18n::Arg::from(&name))]);
            app.panes.contact = None;
            crate::panes::ask(app, "", body, tr("Mute"), false, vec![Command::RemoteMute { demux_id }]);
            return;
        }
        Do::RemoveFromCall(demux_id) => {
            if let Some(c) = app.panes.contact.as_mut() {
                c.confirm = Some(Confirm::Call(demux_id));
            }
            return;
        }
        Do::Nothing => {}
        Do::Confirm(which) => {
            if let Some(c) = app.panes.contact.as_mut() {
                c.confirm = Some(which);
            }
            return;
        }
        Do::Close => {}
        Do::Message => {
            app.panes.details = None;
            app.call_view.pip = app.call.is_some();
            app.open(direct);
        }
        Do::Call(video) => crate::panes::open_lobby(app, direct, video),
        Do::LabelInfo => {
            if let Some(c) = app.panes.contact.as_mut() {
                c.label_info = true;
            }
        }
        Do::Nickname => {
            app.open(direct.clone());
            crate::details::open_nickname(app, direct);
        }
        Do::Block => app.panes.chat_confirm = Some((direct, crate::panes::ChatConfirm::Block)),
        Do::Safety => {
            app.open(direct.clone());
            crate::details::open_safety(app, direct);
        }
    }
    app.panes.contact = None;
}

/// Desktop's `AboutContactModal` and the Add to a group picker, over the
/// contact card.
fn about_and_add(app: &mut App, ctx: &egui::Context, aci: &str, name: &str) {
    let palette = Palette::of(ctx);
    let Some(contact) = app.panes.contact.as_ref() else { return };
    if contact.about_open {
        // Desktop's AboutContactModal: the photo large, then a row for each
        // thing known about them.
        let details = contact.about.clone();
        let person = app.conversation(&ConversationId::direct(aci)).cloned();
        let mut safety = false;
        let mut lightbox = None;
        let you = aci == app.aci;
        let own = contact.own.clone();
        let group = contact.group.as_ref().and_then(|g| app.conversation(g)).cloned();
        let mut go: Option<u8> = None;
        let mut connections = false;
        let closed = crate::panes::modal(&egui::Ui::new(ctx.clone(), egui::Id::unique("about-host"), egui::UiBuilder::new()), "about-contact", 360., |ui| {
            crate::panes::title(ui, if you { tr("You") } else { tr("About") });
            ui.vertical_centered(|ui| {
                let own_photo = own.as_ref().and_then(|p| p.photo.as_ref()).map(|p| p.to_string_lossy().into_owned()).or_else(|| app.me.1.clone());
                let spec = match &person {
                    _ if you => widgets::AvatarSpec { name, color: 0, photo: own_photo.as_deref(), group: false, note_to_self: false },
                    Some(p) => widgets::AvatarSpec::of(p),
                    None => widgets::AvatarSpec::person(name, 0),
                };
                let r = widgets::avatar(ui, &spec, 216.).interact(Sense::click());
                // Our own photo opens large too.
                let photo = if you { own_photo.clone() } else { person.as_ref().and_then(|p| p.avatar_path.clone()).filter(|_| person.as_ref().is_some_and(|p| !p.avatar_withheld)) };
                if let Some(path) = photo
                    && r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                {
                    lightbox = Some(path);
                }
            });
            ui.add_space(16.);
            let row = |ui: &mut egui::Ui, symbol: &str, text: &str, tip: Option<String>| -> egui::Response {
                let r = ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(24.), Sense::hover());
                    theme::paint_symbol(ui.painter(), symbol, r.center(), 18., palette.text);
                    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(14.))).wrap());
                });
                ui.add_space(8.);
                let r = ui.interact(r.response.rect, ui.scope_id().with(("about-row", symbol)), Sense::click());
                match tip {
                    Some(t) => r.on_hover_text(t),
                    None => r,
                }
            };
            if you {
                // About You: the name opens the profile editor; our label in
                // this group; Your QR code with a username.
                let mine = own.as_ref().map(|p| format!("{} {}", p.given_name, p.family_name).trim().to_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| app.me.0.clone());
                if row(ui, "person", &mine, None).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    go = Some(0);
                }
                if let Some(g) = group.as_ref().filter(|g| !g.left && !g.ended) {
                    match g.member_labels.get(&app.aci) {
                        Some(l) => {
                            let text = format!("{} {}", l.emoji.clone().unwrap_or_default(), l.text).trim().to_owned();
                            let r = row(ui, "label", &text, None);
                            if g.can_label && r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                go = Some(1);
                            }
                        }
                        None if g.can_label => {
                            if row(ui, "label", tr("Add a member label"), None).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                go = Some(1);
                            }
                        }
                        None => {}
                    }
                }
                if own.as_ref().is_some_and(|p| p.username.is_some()) && row(ui, "qrcode", tr("Your QR code"), None).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    go = Some(2);
                }
                return;
            }
            // The name: with a nickname, the nickname then the real name.
            let nickname = details.as_ref().and_then(|d| d.nickname.as_ref()).map(|n| format!("{} {}", n.given, n.family).trim().to_owned()).filter(|n| !n.is_empty());
            let profile_name = details.as_ref().and_then(|d| d.profile.as_ref()).map(|p| format!("{} {}", p.given_name, p.family_name).trim().to_owned()).filter(|n| !n.is_empty());
            match (&nickname, &profile_name) {
                (Some(nick), Some(real)) if nick != real => {
                    let tip = crate::i18n::tf("icu:AboutContactModal__TitleWithoutNickname__Tooltip", &format!("\u{201c}{real}\u{201d} is the profile name this person set for themselves in Signal."), &[("title", crate::i18n::Arg::from(real))]);
                    row(ui, "person", &format!("{nick} ({real})"), Some(tip));
                }
                _ => {
                    row(ui, "person", name, None);
                }
            }
            if let Some(d) = &details {
                if d.verified && row(ui, "shield-check", tr("Verified"), None).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    safety = true;
                }
                if (d.profile_sharing || person.as_ref().is_some_and(|p| p.accepted)) && row(ui, "connections", tr("Signal Connection"), None).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    connections = true;
                }
                if d.in_system_contacts {
                    row(ui, "person-circle", &crate::i18n::tf("icu:AboutContactModal__system-contact", &format!("{name} is in your system contacts"), &[("name", crate::i18n::Arg::from(name))]), None);
                }
            }
            if let Some(phone) = person.as_ref().and_then(|c| c.phone.clone()) {
                row(ui, "phone", &langis_core::share_contact::format_phone(&phone, app.phone.as_deref()), None);
            }
            if let Some(p) = details.as_ref().and_then(|d| d.profile.as_ref()).filter(|p| !p.about.is_empty()) {
                row(ui, "edit", &format!("{} {}", p.about_emoji, p.about).trim().to_owned(), None);
            }
            if let Some(p) = &person {
                if p.blocked {
                    row(ui, "block", &crate::i18n::tf("icu:AboutContactModal__blocked", &format!("{name} is blocked"), &[("name", crate::i18n::Arg::from(name))]), None);
                } else if !p.accepted {
                    row(ui, "message", tr("Pending message request"), None);
                } else if p.last_message_at == 0 {
                    row(ui, "message", &crate::i18n::tf("icu:AboutContactModal__no-dms", &format!("No direct messages with {name}"), &[("name", crate::i18n::Arg::from(name))]), None);
                }
            }
            let groups: Vec<String> = app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && !g.left && g.members.iter().any(|m| m == aci)).map(|g| g.title.clone()).collect();
            if !groups.is_empty() {
                row(ui, "group", &crate::chat::shared_groups_text(&groups, true), None);
            }
            if let Some(note) = details.as_ref().and_then(|d| d.nickname.as_ref()).map(|n| n.note.clone()).filter(|n| !n.is_empty()) {
                row(ui, "note", &note, None);
            }
        });
        if let Some(path) = lightbox {
            app.panes.avatar_lightbox = Some(path);
        }
        if connections {
            app.panes.connections_modal = true;
        }
        if let Some(to) = go {
            let group = app.panes.contact.take().and_then(|c| c.group);
            match to {
                0 => {
                    app.tab = crate::app::Tab::Settings;
                    app.settings.page = crate::settings::Page::Profile;
                }
                1 => {
                    if let Some(g) = group {
                        crate::details::open_label_editor(app, g);
                    }
                }
                _ => crate::profile::open_link_screen(app),
            }
            return;
        }
        if safety {
            crate::details::open_safety(app, ConversationId::direct(aci));
        }
        if (closed || safety) && let Some(c) = app.panes.contact.as_mut() {
            c.about_open = false;
        }
    }
    let Some(mut query) = app.panes.contact.as_mut().and_then(|c| c.add_to_group.take()) else { return };
    let mut picked = None;
    let mut cancel = false;
    egui::Modal::new(egui::Id::unique("add-to-group")).frame(egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24)).show(ctx, |ui| {
        ui.set_width(360.);
        ui.label(egui::RichText::new(format!("Add {name} to a group")).font(theme::semibold(18.)));
        ui.add_space(8.);
        widgets::text_field(ui, &mut query, tr("Search"), false);
        ui.add_space(6.);
        let q = query.trim().to_lowercase();
        egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
            for g in app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && g.can_add_members && !g.left && !g.members.iter().any(|m| m == aci) && (q.is_empty() || g.title.to_lowercase().contains(&q))) {
                let r = crate::panes::contact_row(ui, &palette, &g.title, Some(&crate::i18n::tf("icu:GroupListItem__message-default", &format!("{} members", g.members.len() + 1), &[("count", crate::i18n::Arg::from(&g.members.len() + 1))])), g.color, None);
                if r.clicked() {
                    picked = Some(g.id.clone());
                }
            }
        });
        ui.add_space(8.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel = widgets::pill_button(ui, tr("Cancel"), false).clicked();
        });
    });
    if let Some(group) = picked {
        app.send(Command::AddGroupMembers { conversation: group, members: vec![aci.to_owned()] });
        app.toast(format!("Adding {name} to the group"));
    } else if !cancel && let Some(c) = app.panes.contact.as_mut() {
        c.add_to_group = Some(query);
    }
}

/// Desktop's `GroupMemberLabelInfoModal`: what member labels are, and Set a
/// label (or Edit your label) when we may set ours.
fn label_info(app: &mut App, ctx: &egui::Context, palette: &Palette, group: Option<&langis_store::Conversation>) {
    let can = group.is_some_and(|g| g.can_label && !g.left && !g.ended);
    let has = group.is_some_and(|g| g.member_labels.contains_key(&app.aci));
    let set = if has { tr("Edit your label") } else { tr("Set a label") };
    let mut answer = None;
    let ui = &egui::Ui::new(ctx.clone(), egui::Id::unique("label-info"), egui::UiBuilder::new());
    let closed = crate::panes::modal(ui, "member-label-info", 320., |ui| {
        ui.vertical_centered(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(32.), Sense::hover());
            theme::paint_symbol(ui.painter(), "label", r.center(), 32., palette.text);
            ui.add_space(10.);
            ui.label(egui::RichText::new(tr("Member labels")).font(theme::semibold(16.)));
            ui.add_space(6.);
            ui.label(egui::RichText::new(tr("Use a member label to describe yourself or your role in this group. Member labels are only visible within this group.")).font(theme::regular(14.)).color(palette.text_secondary));
        });
        let labels: Vec<&str> = if can { vec![tr("OK"), set] } else { vec![tr("OK")] };
        answer = crate::panes::buttons(ui, &labels);
    });
    if answer == Some(1)
        && let Some(g) = group
    {
        let id = g.id.clone();
        app.panes.contact = None;
        crate::details::open_label_editor(app, id);
    } else if answer.is_some() || closed {
        if let Some(c) = app.panes.contact.as_mut() {
            c.label_info = false;
        }
    }
}
