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
    app.panes.contact = Some(Contact { group, aci, confirm: None });
}

enum Do {
    Close,
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
            ui.painter().galley(Pos2::new(row.center().x - galley.size().x / 2., row.top()), galley, palette.text);
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
    let Some(todo) = todo else { return };
    match todo {
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
