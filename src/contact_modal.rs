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

pub struct Contact {
    /// The group it was opened from, for the admin's actions.
    pub group: Option<ConversationId>,
    pub aci: String,
    confirm: Option<Confirm>,
    /// Their about, number and verification, for the About view.
    about: Option<langis_core::ContactDetails>,
    about_open: bool,
    /// Add to a group: the groups' search, while picking.
    add_to_group: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    Admin,
    Remove,
}

pub fn open(app: &mut App, group: Option<ConversationId>, aci: String) {
    if aci == app.aci {
        return;
    }
    app.panes.contact = Some(Contact { group, aci: aci.clone(), confirm: None, about: None, about_open: false, add_to_group: None });
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

    if let Some(which) = confirm {
        // `ContactModal`'s confirmations, over it.
        let (title, body, action) = match which {
            Confirm::Admin if is_admin => (format!("Remove {name} as group admin?"), String::new(), "Remove as admin"),
            Confirm::Admin => (String::new(), format!("{name} will be able to edit this group and its members."), "Make admin"),
            Confirm::Remove => (String::new(), format!("Remove \"{name}\" from the group?"), "Remove from group"),
        };
        let variant = if which == Confirm::Remove || is_admin { Variant::StrongDestructive } else { Variant::StrongPrimary };
        if let Some(answer) = dialog::show(ctx, egui::Id::unique("contact-confirm"), &title, &body, Some("Cancel"), &[(action, variant)]) {
            if let (Answer::Action(_), Some(g)) = (answer, group_id.clone()) {
                match which {
                    Confirm::Admin => app.send(Command::SetGroupAdmin { conversation: g, member: aci.clone(), admin: !is_admin }),
                    Confirm::Remove => app.send(Command::RemoveFromGroup { conversation: g, service_id: aci.clone() }),
                }
            }
            if let Some(c) = app.panes.contact.as_mut() {
                c.confirm = None;
            }
        }
        return;
    }

    let mut todo = None;
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
                Some(p) => widgets::paint_avatar(ui, rect, &widgets::AvatarSpec::of(p)),
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
                ui.add_space(4.);
            }

            // `AxoStackedButton.Row`: Message, Video, Voice.
            ui.add_space(16.);
            let buttons = [("message", "Message"), ("videocamera", "Video"), ("phone", "Voice")];
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
            let mut items: Vec<(&str, String, Do)> = vec![("pencil", "Nickname".into(), Do::Nickname)];
            if !blocked {
                items.push(("block", "Block".into(), Do::Block));
            }
            items.push(("shield-check", "View safety number".into(), Do::Safety));
            if we_admin && is_member {
                items.push(("key", if is_admin { "Remove as admin" } else { "Make admin" }.into(), Do::Confirm(Confirm::Admin)));
                items.push(("minus-circle", "Remove from group".into(), Do::Confirm(Confirm::Remove)));
            }
            // `ContactModal--add-to-group`, when some group can take them.
            let can_add = app.conversations.iter().any(|g| g.kind == langis_store::ConversationKind::Group && g.can_add_members && !g.left && !g.members.contains(&aci));
            if can_add {
                items.push(("group", "Add to a group".into(), Do::AddToGroup));
            }
            // In a call with them: mute them, or (a call link's admin) remove.
            if let Some(call) = app.call.as_ref()
                && let Some(p) = call.participants.iter().find(|p| p.aci == aci)
            {
                if p.audio {
                    items.push(("mic-slash", "Mute audio".into(), Do::MuteInCall(p.demux_id)));
                }
                if call.conversation.as_call_link().is_some() {
                    items.push(("minus-circle", "Remove from call".into(), Do::RemoveFromCall(p.demux_id)));
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
        Do::MuteInCall(demux_id) => app.send(Command::RemoteMute { demux_id }),
        Do::RemoveFromCall(demux_id) => app.send(Command::RemoveFromCall { demux_id }),
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
        Do::Call(video) => app.send(Command::StartCall { conversation: direct, video }),
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
        let details = contact.about.clone();
        let mut done = false;
        egui::Modal::new(egui::Id::unique("about-contact")).frame(egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24)).show(ctx, |ui| {
            ui.set_width(340.);
            ui.label(egui::RichText::new("About").font(theme::semibold(18.)));
            ui.add_space(12.);
            let row = |ui: &mut egui::Ui, symbol: &str, text: &str| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(24.), Sense::hover());
                    theme::paint_symbol(ui.painter(), symbol, r.center(), 18., palette.text_secondary);
                    ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(14.))).wrap());
                });
                ui.add_space(6.);
            };
            row(ui, "person", name);
            if let Some(d) = &details {
                if d.verified {
                    row(ui, "shield-check", "Verified");
                }
                if let Some(p) = &d.profile {
                    if !p.about.is_empty() {
                        row(ui, "pencil", &format!("{} {}", p.about_emoji, p.about).trim().to_owned());
                    }
                    if let Some(u) = &p.username {
                        row(ui, "at", u);
                    }
                }
                if let Some(note) = d.nickname.as_ref().map(|n| n.note.as_str()).filter(|n| !n.is_empty()) {
                    row(ui, "note", note);
                }
            }
            if let Some(phone) = app.conversation(&ConversationId::direct(aci)).and_then(|c| c.phone.clone()) {
                row(ui, "phone", &phone);
            }
            ui.add_space(8.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                done = widgets::pill_button(ui, "Done", true).clicked();
            });
        });
        if done && let Some(c) = app.panes.contact.as_mut() {
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
        widgets::text_field(ui, &mut query, "Search", false);
        ui.add_space(6.);
        let q = query.trim().to_lowercase();
        egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
            for g in app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && g.can_add_members && !g.left && !g.members.iter().any(|m| m == aci) && (q.is_empty() || g.title.to_lowercase().contains(&q))) {
                let r = crate::panes::contact_row(ui, &palette, &g.title, Some(&format!("{} members", g.members.len() + 1)), g.color, None);
                if r.clicked() {
                    picked = Some(g.id.clone());
                }
            }
        });
        ui.add_space(8.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel = widgets::pill_button(ui, "Cancel", false).clicked();
        });
    });
    if let Some(group) = picked {
        app.send(Command::AddGroupMembers { conversation: group, members: vec![aci.to_owned()] });
        app.toast(format!("Adding {name} to the group"));
    } else if !cancel && let Some(c) = app.panes.contact.as_mut() {
        c.add_to_group = Some(query);
    }
}
