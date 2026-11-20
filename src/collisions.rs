//! Desktop's name-collision warnings (`TimelineWarnings`,
//! `ContactSpoofingReviewDialog`): a message request from someone with the
//! same name as a contact, and group members who share a name.

use std::collections::BTreeMap;

use egui::{Color32, RichText, Sense, Ui, Vec2};
use langis_core::Command;
use langis_store::{Conversation, ConversationId, ConversationKind};

use crate::app::App;
use crate::panes::{body, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

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
        ("Review requests carefully. Signal found another contact with the same name.".to_owned(), tr("Review request"), false)
    } else {
        let groups = group_collisions(app, c);
        if groups.is_empty() {
            return;
        }
        let n: usize = groups.iter().map(Vec::len).sum();
        (format!("{n} group members have the same name."), tr("Review members"), true)
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
                    if widgets::icon_button(ui, crate::icons::Icon::X, 16., tr("Close")).clicked() {
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

/// What was clicked on a person in the review dialog.
enum PersonClick {
    Name,
    Connection,
}

/// Desktop's `ContactSpoofingReviewDialogPerson`: the 48px avatar, the
/// name (opens their contact modal), then their number, Signal
/// Connection, and the groups we share; `extra` adds its buttons under.
fn person(app: &App, ui: &mut Ui, palette: &Palette, sid: &str, extra: impl FnOnce(&mut Ui)) -> Option<PersonClick> {
    let name = app.name(sid);
    let c = app.conversation(&ConversationId::direct(sid));
    let mut click = None;
    ui.horizontal_top(|ui| {
        let (avatar, hit) = ui.allocate_exact_size(Vec2::splat(48.), Sense::click());
        match c {
            Some(c) => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c)),
            None => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::person(&name, 0)),
        }
        if hit.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            click = Some(PersonClick::Name);
        }
        ui.add_space(12.);
        ui.vertical(|ui| {
            let r = ui.add(egui::Label::new(RichText::new(&name).font(theme::semibold(14.)).color(palette.text)).sense(Sense::click()));
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                click = Some(PersonClick::Name);
            }
            let property = |ui: &mut Ui, symbol: &str, text: &str, link: bool| -> bool {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(16.), Sense::hover());
                    theme::paint_symbol(ui.painter(), symbol, r.center(), 14., palette.text_secondary);
                    let color = if link { palette.text } else { palette.text_secondary };
                    let mut text = RichText::new(text).font(theme::regular(13.)).color(color);
                    if link {
                        text = text.underline();
                    }
                    let r = ui.add(egui::Label::new(text).wrap().sense(if link { Sense::click() } else { Sense::hover() }));
                    link && r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                })
                .inner
            };
            if let Some(p) = c.and_then(|c| c.phone.clone()) {
                property(ui, "phone", &langis_core::share_contact::format_phone(&p, app.phone.as_deref()), false);
            }
            if c.is_some_and(|c| c.accepted && !c.blocked) && property(ui, "connections", tr("Signal Connection"), true) {
                click = Some(PersonClick::Connection);
            }
            let groups: Vec<String> = app.conversations.iter().filter(|g| g.kind == ConversationKind::Group && !g.left && g.members.iter().any(|m| m == sid)).map(|g| g.title.clone()).collect();
            let shared = if groups.is_empty() { tr("No other groups in common").to_owned() } else { crate::chat::shared_groups_text(&groups, true) };
            property(ui, "group", &shared, false);
            extra(ui);
        });
    });
    click
}

/// Desktop's `SignalConnectionsModal`.
pub fn connections_modal(app: &mut App, ui: &mut Ui) {
    if !app.panes.connections_modal {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let mut ok = false;
    let closed = modal(ui, "signal-connections", 360., |ui| {
        title(ui, tr("Signal Connections"));
        ui.vertical_centered(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(36.), Sense::hover());
            theme::paint_symbol(ui.painter(), "connections", r.center(), 36., palette.text);
        });
        ui.add_space(12.);
        body(ui, &palette, &crate::i18n::tf("icu:SignalConnectionsModal__header", "Signal Connections are people you've chosen to trust, either by:", &[("connections", crate::i18n::Arg::from(&tr("Signal Connections")))]));
        ui.add_space(8.);
        for (symbol, text) in [("message", tr("Starting a chat")), ("check-circle", tr("Accepting a message request")), ("person-circle", tr("Having them in your system contacts"))] {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(20.), Sense::hover());
                theme::paint_symbol(ui.painter(), symbol, r.center(), 16., palette.text);
                ui.label(RichText::new(text).font(theme::regular(14.)));
            });
            ui.add_space(4.);
        }
        ui.add_space(8.);
        body(ui, &palette, tr("Your connections can see your name and photo, and can see posts to \"My Story\" unless you hide it from them"));
        ok = crate::panes::buttons(ui, &[tr("OK")]).is_some();
    });
    if ok || closed {
        app.panes.connections_modal = false;
    }
}

/// Desktop's `ContactSpoofingReviewDialog`.
pub fn review_dialog(app: &mut App, ui: &mut Ui) {
    connections_modal(app, ui);
    let Some(id) = app.panes.collision_review.clone() else { return };
    let Some(c) = app.conversation(&id).cloned() else {
        app.panes.collision_review = None;
        return;
    };
    let palette = Palette::of(ui.ctx());
    let mut close = false;
    let mut open: Option<String> = None;
    let mut connections = false;
    let me = app.aci.clone();
    let admin = c.admins.iter().any(|a| *a == me);
    let mut todo: Vec<Box<dyn FnOnce(&mut App)>> = Vec::new();
    let closed = modal(ui, "collision-review", 420., |ui| {
        let mut handle = |click: Option<PersonClick>, sid: &str| match click {
            Some(PersonClick::Name) => open = Some(sid.to_owned()),
            Some(PersonClick::Connection) => connections = true,
            None => {}
        };
        if let Some(other) = direct_collision(app, &c).cloned() {
            title(ui, tr("Review request"));
            body(ui, &palette, tr("If you're not sure who the request is from, review the contacts below and take action."));
            ui.add_space(12.);
            ui.label(RichText::new(tr("Request")).font(theme::semibold(13.)).color(palette.text_secondary));
            ui.add_space(6.);
            if let Some(sid) = c.id.as_direct() {
                let click = person(app, ui, &palette, sid, |ui| {
                    ui.add_space(6.);
                    ui.horizontal(|ui| {
                        if widgets::axo_button(ui, tr("Delete"), widgets::Axo::SubtleSecondary).clicked() {
                            let id = c.id.clone();
                            todo.push(Box::new(move |app| app.panes.delete_chat = Some((id, false))));
                            close = true;
                        }
                        if widgets::axo_button(ui, tr("Block"), widgets::Axo::SubtleDestructive).clicked() {
                            let id = c.id.clone();
                            todo.push(Box::new(move |app| app.panes.chat_confirm = Some((id, crate::panes::ChatConfirm::Block))));
                            close = true;
                        }
                    });
                });
                handle(click, sid);
            }
            ui.add_space(16.);
            ui.label(RichText::new(tr("Your contact")).font(theme::semibold(13.)).color(palette.text_secondary));
            ui.add_space(6.);
            if let Some(sid) = other.id.as_direct() {
                let click = person(app, ui, &palette, sid, |_| {});
                handle(click, sid);
            }
        } else {
            title(ui, tr("Review members"));
            let sets = group_collisions(app, &c);
            let total: usize = sets.iter().map(Vec::len).sum();
            let description = if sets.len() > 1 {
                crate::i18n::tf("icu:ContactSpoofingReviewDialog__group__multiple-conflicts__description", &format!("{} name conflicts were found in this group. Review the members below or choose to take action.", sets.len()), &[("count", crate::i18n::Arg::from(&sets.len()))])
            } else {
                crate::i18n::tf("icu:ContactSpoofingReviewDialog__group__description", &format!("{total} group members have the same name, review the members below or choose to take action."), &[("count", crate::i18n::Arg::from(&total))])
            };
            body(ui, &palette, &description);
            ui.add_space(10.);
            egui::ScrollArea::vertical().max_height(420.).show(ui, |ui| {
                let all: Vec<String> = sets.into_iter().flatten().collect();
                let n = all.len();
                for (i, sid) in all.into_iter().enumerate() {
                    let them = app.conversation(&ConversationId::direct(&sid)).cloned();
                    let click = person(app, ui, &palette, &sid, |ui| {
                        ui.add_space(6.);
                        if admin {
                            if widgets::axo_button(ui, tr("Remove"), widgets::Axo::SubtleDestructive).clicked() {
                                let who = app.name(&sid);
                                let body = crate::i18n::tf("icu:RemoveGroupMemberConfirmation__description", &format!("Remove \u{201c}{who}\u{201d} from the group?"), &[("name", crate::i18n::Arg::from(&who))]);
                                let (g, s) = (c.id.clone(), sid.clone());
                                todo.push(Box::new(move |app| crate::panes::ask(app, "", body, crate::i18n::tr("Remove"), true, vec![Command::RemoveFromGroup { conversation: g, service_id: s }])));
                            }
                        } else if them.as_ref().is_some_and(|t| t.blocked) {
                            if widgets::axo_button(ui, tr("Unblock"), widgets::Axo::SubtleSecondary).clicked() {
                                let id = ConversationId::direct(&sid);
                                todo.push(Box::new(move |app| app.panes.chat_confirm = Some((id, crate::panes::ChatConfirm::Unblock))));
                                close = true;
                            }
                        } else if !them.as_ref().is_some_and(|t| t.system_contact) && widgets::axo_button(ui, tr("Block"), widgets::Axo::SubtleDestructive).clicked() {
                            let id = ConversationId::direct(&sid);
                            todo.push(Box::new(move |app| app.panes.chat_confirm = Some((id, crate::panes::ChatConfirm::Block))));
                            close = true;
                        }
                    });
                    handle(click, &sid);
                    if i + 1 < n {
                        ui.add_space(10.);
                        ui.separator();
                        ui.add_space(10.);
                    }
                }
            });
        }
    });
    for f in todo {
        f(app);
    }
    if connections {
        app.panes.connections_modal = true;
    }
    if let Some(sid) = open {
        let group = (c.kind == ConversationKind::Group).then(|| c.id.clone());
        app.panes.collision_review = None;
        crate::contact_modal::open(app, group, sid);
        return;
    }
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
