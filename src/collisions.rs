//! Desktop's name-collision warnings (`TimelineWarnings`,
//! `ContactSpoofingReviewDialog`): a message request from someone with the
//! same name as a contact, and group members who share a name.

use std::collections::BTreeMap;

use egui::{Color32, Pos2, Rect, RichText, Sense, Ui, Vec2};
use langis_core::Command;
use langis_store::{Conversation, ConversationId, ConversationKind};

use crate::app::App;
use crate::panes::{body, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;

fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    !a.trim().is_empty() && norm(a) == norm(b)
}

/// A message request whose name is a contact's too: that contact.
pub fn direct_collision<'a>(app: &'a App, c: &Conversation) -> Option<&'a Conversation> {
    if c.kind != ConversationKind::Direct || c.accepted {
        return None;
    }
    app.conversations.iter().find(|o| o.kind == ConversationKind::Direct && o.id != c.id && o.accepted && same(&o.title, &c.title))
}

/// A group's members who share a name with another, by name (not us).
pub fn group_collisions(app: &App, c: &Conversation) -> Vec<Vec<String>> {
    if c.kind != ConversationKind::Group {
        return Vec::new();
    }
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in c.members.iter().filter(|m| **m != app.aci) {
        let name = app.name(m);
        if name.trim().is_empty() {
            continue;
        }
        by.entry(name.trim().to_lowercase()).or_default().push(m.clone());
    }
    by.into_values().filter(|v| v.len() > 1).collect()
}

/// The bar under the header, while there's a collision not dismissed.
pub fn bar(app: &mut App, ui: &mut Ui, palette: &Palette, c: &Conversation) {
    if app.panes.collision_dismissed.contains(&c.id) {
        return;
    }
    let (text, link, dismissable) = if direct_collision(app, c).is_some() {
        ("Review requests carefully. Signal found another contact with the same name.".to_owned(), "Review request", false)
    } else {
        let groups = group_collisions(app, c);
        if groups.is_empty() {
            return;
        }
        let n: usize = groups.iter().map(Vec::len).sum();
        (format!("{n} group members have the same name."), "Review members", true)
    };
    let fill = if palette.dark { Color32::from_rgb(0x3b, 0x33, 0x1c) } else { Color32::from_rgb(0xfe, 0xf5, 0xd6) };
    let mut review = false;
    let mut dismiss = false;
    egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (at, _) = ui.allocate_exact_size(Vec2::splat(18.), Sense::hover());
            theme::paint_symbol(ui.painter(), "error-circle", at.center(), 16., palette.text);
            ui.add_space(6.);
            ui.label(RichText::new(text).font(theme::regular(13.)));
            if ui.link(RichText::new(link).font(theme::semibold(13.))).clicked() {
                review = true;
            }
            if dismissable {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, crate::icons::Icon::X, 16., "Close").clicked() {
                        dismiss = true;
                    }
                });
            }
        });
    });
    if review {
        app.panes.collision_review = Some(c.id.clone());
    }
    if dismiss {
        app.panes.collision_dismissed.insert(c.id.clone());
    }
}

/// A person in the review dialog: avatar, name, number.
fn person(app: &App, ui: &mut Ui, palette: &Palette, sid: &str) {
    let name = app.name(sid);
    let c = app.conversation(&ConversationId::direct(sid));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.), Sense::hover());
    let avatar = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 20.), Vec2::splat(40.));
    match c {
        Some(c) => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c)),
        None => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::person(&name, 0)),
    }
    let phone = c.and_then(|c| c.phone.clone());
    let x = avatar.right() + 12.;
    match phone {
        Some(p) => {
            ui.painter().text(Pos2::new(x, rect.center().y - 9.), egui::Align2::LEFT_CENTER, &name, theme::semibold(14.), palette.text);
            ui.painter().text(Pos2::new(x, rect.center().y + 10.), egui::Align2::LEFT_CENTER, langis_core::share_contact::format_phone(&p, app.phone.as_deref()), theme::regular(13.), palette.text_secondary);
        }
        None => {
            ui.painter().text(Pos2::new(x, rect.center().y), egui::Align2::LEFT_CENTER, &name, theme::semibold(14.), palette.text);
        }
    }
}

/// Desktop's `ContactSpoofingReviewDialog`.
pub fn review_dialog(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.panes.collision_review.clone() else { return };
    let Some(c) = app.conversation(&id).cloned() else {
        app.panes.collision_review = None;
        return;
    };
    let palette = Palette::of(ui.ctx());
    let mut close = false;
    let me = app.aci.clone();
    let admin = c.admins.iter().any(|a| *a == me);
    let closed = modal(ui, "collision-review", 420., |ui| {
        if let Some(other) = direct_collision(app, &c).cloned() {
            title(ui, "Review request");
            body(ui, &palette, "If you're not sure who the request is from, review the contacts below and take action.");
            ui.add_space(12.);
            ui.label(RichText::new("Request").font(theme::semibold(13.)).color(palette.text_secondary));
            if let Some(sid) = c.id.as_direct() {
                person(app, ui, &palette, sid);
            }
            ui.horizontal(|ui| {
                if widgets::axo_button(ui, "Delete", widgets::Axo::SubtleSecondary).clicked() {
                    app.panes.delete_chat = Some((c.id.clone(), false));
                    close = true;
                }
                if widgets::axo_button(ui, "Block", widgets::Axo::SubtleDestructive).clicked() {
                    app.panes.chat_confirm = Some((c.id.clone(), crate::panes::ChatConfirm::Block));
                    close = true;
                }
            });
            ui.add_space(16.);
            ui.label(RichText::new("Your contact").font(theme::semibold(13.)).color(palette.text_secondary));
            if let Some(sid) = other.id.as_direct() {
                person(app, ui, &palette, sid);
            }
        } else {
            title(ui, "Review members");
            body(ui, &palette, "If you're not sure who a request is from, review the members below and take action.");
            egui::ScrollArea::vertical().max_height(420.).show(ui, |ui| {
                for set in group_collisions(app, &c) {
                    ui.add_space(12.);
                    let name = app.name(&set[0]);
                    ui.label(RichText::new(format!("{} members named \u{201c}{name}\u{201d}", set.len())).font(theme::semibold(13.)).color(palette.text_secondary));
                    for sid in set {
                        person(app, ui, &palette, &sid);
                        if admin && ui.add(egui::Button::new(RichText::new("Remove from group").color(palette.destructive).font(theme::medium(13.))).frame(false)).clicked() {
                            app.send(Command::RemoveFromGroup { conversation: c.id.clone(), service_id: sid.clone() });
                        }
                    }
                }
            });
        }
    });
    if close || closed {
        app.panes.collision_review = None;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_match_ignoring_case_and_spaces() {
        assert!(super::same("Maya  Chen", "maya chen"));
        assert!(!super::same("Maya Chen", "Maya Cohen"));
        assert!(!super::same(" ", " "));
    }
}
