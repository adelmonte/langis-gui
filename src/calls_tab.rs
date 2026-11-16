//! Desktop's Calls tab (`CallsTab`): the call history and call links on the
//! left, a link's details (or a prompt) on the right.

use egui::{CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::Command;
use langis_store::{AdhocCallStatus, CallLinkRestrictions, CallOutcome, ConversationId, ConversationKind, GroupCallStatus, Notice};

use crate::app::App;
use crate::icons::Icon;
use crate::panes::{body, menu_row};
use crate::theme::{self, Palette};
use crate::widgets;

/// One row of the history.
struct Row {
    conversation: ConversationId,
    title: String,
    /// What happened: "Incoming voice call", "Missed video call"…
    detail: String,
    missed: bool,
    video: bool,
    at: i64,
}

fn rows(app: &App) -> Vec<Row> {
    let mut out = Vec::new();
    for m in &app.panes.calls {
        let Some(notice) = &m.notice else { continue };
        let title = app.conversation(&m.conversation_id).map(|c| c.title.clone()).unwrap_or_else(|| "Unknown".into());
        let (detail, missed, video) = match notice {
            Notice::Call { incoming, video, outcome, .. } => {
                let kind = if *video { "video call" } else { "voice call" };
                let detail = match (incoming, outcome) {
                    (_, CallOutcome::Missed) => format!("Missed {kind}"),
                    (_, CallOutcome::Declined) => format!("Declined {kind}"),
                    (true, _) => format!("Incoming {kind}"),
                    (false, CallOutcome::Unanswered) => format!("Unanswered {kind}"),
                    (false, _) => format!("Outgoing {kind}"),
                };
                (detail, matches!(outcome, CallOutcome::Missed), *video)
            }
            Notice::GroupCall { status, incoming, .. } => {
                let detail = match status {
                    Some(GroupCallStatus::Missed) => "Missed group call",
                    Some(GroupCallStatus::Declined) => "Declined group call",
                    Some(GroupCallStatus::OutgoingRing) => "Outgoing group call",
                    _ if *incoming => "Incoming group call",
                    _ => "Group call",
                };
                (detail.to_owned(), matches!(status, Some(GroupCallStatus::Missed)), true)
            }
            _ => continue,
        };
        out.push(Row { conversation: m.conversation_id.clone(), title, detail, missed, video, at: m.sent_at });
    }
    for call in &app.panes.adhoc {
        let Some(link) = app.panes.links.iter().find(|l| l.room_id == call.room_id) else { continue };
        let detail = match call.status {
            AdhocCallStatus::Pending => "Call link",
            AdhocCallStatus::Joined => "Joined call link",
            AdhocCallStatus::Generic => "Call link",
        };
        out.push(Row {
            conversation: ConversationId::call_link(&link.room_id),
            title: link_title(&link.name),
            detail: detail.into(),
            missed: false,
            video: true,
            at: call.at,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.at));
    out
}

fn link_title(name: &str) -> String {
    if name.is_empty() { "Signal call".into() } else { name.to_owned() }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    if !app.panes.calls_loaded {
        app.panes.calls_loaded = true;
        crate::panes::load_calls(app);
        // Desktop marks every call seen when the tab is viewed.
        app.send(Command::MarkCallsRead { conversation: None });
    }
    egui::Panel::left("calls-list")
        .resizable(true)
        .default_size(300.)
        .min_size(240.)
        .max_size(420.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.sidebar))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1., palette.border));
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(24.);
                ui.label(RichText::new("Calls").font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.);
                    let more = widgets::icon_button(ui, Icon::More, 20., "More");
                    egui::Popup::menu(&more).show(|ui| {
                        if menu_row(ui, Some(Icon::Trash), "Clear call history", true).clicked() {
                            app.send(Command::ClearCallHistory);
                            app.panes.calls.clear();
                            app.panes.adhoc.clear();
                            ui.close();
                        }
                    });
                    let new_call = widgets::icon_button(ui, Icon::PhonePlus, 20., "New call");
                    egui::Popup::menu(&new_call).show(|ui| {
                        ui.set_min_width(240.);
                        let people: Vec<_> = app.conversations.iter().filter(|c| c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked).take(30).cloned().collect();
                        egui::ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                            for c in people {
                                if crate::panes::contact_row(ui, &palette, &c.title, None, c.color, c.avatar_path.as_deref()).clicked() {
                                    call(app, &c.id, false);
                                    ui.close();
                                }
                            }
                        });
                    });
                });
            });
            ui.add_space(6.);
            ui.horizontal(|ui| {
                ui.add_space(16.);
                ui.add(egui::TextEdit::singleline(&mut app.panes.call_search).hint_text("Search").desired_width(ui.available_width() - 16.));
            });
            ui.add_space(8.);
            // "Create a call link", as Desktop's first row.
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.), Sense::click());
            let inner = rect.shrink2(Vec2::new(10., 2.));
            if response.hovered() {
                ui.painter().rect_filled(inner, CornerRadius::same(12), palette.fill);
            }
            let circle = Rect::from_min_size(inner.min + Vec2::new(14., 6.), Vec2::splat(40.));
            ui.painter().circle_filled(circle.center(), 20., palette.fill_secondary);
            Icon::Link.paint(ui, circle, 20., palette.text);
            ui.painter().text(Pos2::new(circle.right() + 12., circle.center().y), egui::Align2::LEFT_CENTER, "Create a Call Link", theme::medium(14.), palette.text);
            if response.clicked() {
                app.send(Command::CreateCallLink);
                app.toast("Creating a call link…");
            }
            let query = app.panes.call_search.trim().to_lowercase();
            let list: Vec<Row> = rows(app).into_iter().filter(|r| query.is_empty() || r.title.to_lowercase().contains(&query)).collect();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if list.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| body(ui, &palette, if query.is_empty() { "No recent calls" } else { "No results" }));
                }
                for (i, row) in list.iter().enumerate() {
                    if call_row(app, ui, &palette, row, i) {
                        if let Some(room) = row.conversation.as_call_link() {
                            app.panes.link_name = app.panes.links.iter().find(|l| l.room_id == room).map(|l| l.name.clone()).unwrap_or_default();
                            app.panes.link_details = Some(room.to_owned());
                        } else {
                            app.panes.link_details = None;
                            app.open(row.conversation.clone());
                        }
                    }
                }
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        match app.panes.link_details.clone() {
            Some(room) => link_details(app, ui, &palette, &room),
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Click on a call to see details, or start a new call").font(theme::regular(14.)).color(palette.text_secondary));
                });
            }
        }
    });
}

/// Calls a chat (a group's lobby first) or joins a link's call.
fn call(app: &mut App, conversation: &ConversationId, video: bool) {
    if conversation.is_group() || conversation.as_call_link().is_some() {
        crate::panes::open_lobby(app, conversation.clone(), video);
    } else {
        app.send(Command::StartCall { conversation: conversation.clone(), video });
    }
}

/// One history row: avatar, name, what happened, when, and a call button.
/// True when the row (not the button) was clicked.
fn call_row(app: &mut App, ui: &mut Ui, palette: &Palette, row: &Row, i: usize) -> bool {
    let width = ui.available_width();
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, 64.), Sense::click());
    let rect = Rect::from_min_max(outer.min + Vec2::new(10., 2.), outer.max - Vec2::new(10., 2.));
    let selected = row.conversation.as_call_link().is_some_and(|r| app.panes.link_details.as_deref() == Some(r));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { palette.fill_pressed } else { palette.fill });
    }
    let avatar = Rect::from_min_size(rect.min + Vec2::new(14., 8.), Vec2::splat(40.));
    match app.conversation(&row.conversation) {
        Some(c) => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c)),
        None => {
            ui.painter().circle_filled(avatar.center(), 20., palette.fill_secondary);
            Icon::Link.paint(ui, avatar, 20., palette.text);
        }
    }
    let x = avatar.right() + 12.;
    let painter = ui.painter_at(rect);
    painter.text(Pos2::new(x, rect.top() + 12.), egui::Align2::LEFT_TOP, &row.title, theme::semibold(14.), palette.text);
    let color = if row.missed { palette.destructive } else { palette.text_secondary };
    let detail = format!("{} · {}", row.detail, crate::format::short_timestamp(row.at));
    painter.text(Pos2::new(x, rect.top() + 32.), egui::Align2::LEFT_TOP, detail, theme::regular(12.), color);
    // The call-back button on the right.
    let button = Rect::from_center_size(Pos2::new(rect.right() - 28., rect.center().y), Vec2::splat(32.));
    let hit = ui.interact(button, ui.scope_id().with(("call-back", i)), Sense::click());
    if hit.hovered() {
        ui.painter().circle_filled(button.center(), 16., palette.fill_pressed);
    }
    let icon = if row.conversation.as_call_link().is_some() { Icon::Video } else if row.video { Icon::Video } else { Icon::Phone };
    icon.paint(ui, button, 18., palette.text);
    if hit.on_hover_text(if row.conversation.as_call_link().is_some() { "Join" } else { "Call back" }).clicked() {
        call(app, &row.conversation.clone(), row.video);
        return false;
    }
    response.clicked()
}

/// Desktop's `CallLinkDetails`.
fn link_details(app: &mut App, ui: &mut Ui, palette: &Palette, room: &str) {
    let Some(link) = app.panes.links.iter().find(|l| l.room_id == room).cloned() else {
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
        return;
    };
    let admin = link.admin_key.is_some();
    let url = langis_core::call_link_url(&link.root_key).unwrap_or_default();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 40, right: 40, top: 32, bottom: 32 }).show(ui, |ui| {
            ui.set_max_width(560.);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.), Sense::hover());
                ui.painter().circle_filled(rect.center(), 32., palette.fill_secondary);
                Icon::Link.paint(ui, rect, 28., palette.text);
                ui.add_space(12.);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 90.).max(120.));
                    ui.add_space(8.);
                    ui.label(RichText::new(link_title(&link.name)).font(theme::semibold(20.)));
                    ui.add(egui::Label::new(RichText::new(&url).font(theme::regular(13.)).color(palette.text_secondary)).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, "Join", true).clicked() {
                        crate::panes::open_lobby(app, ConversationId::call_link(room), true);
                    }
                });
            });
            ui.add_space(20.);
            egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if admin {
                    ui.horizontal(|ui| {
                        ui.add_space(10.);
                        ui.label("Call name");
                        let field = ui.add(egui::TextEdit::singleline(&mut app.panes.link_name).hint_text("Signal call").desired_width(260.));
                        if field.lost_focus() && app.panes.link_name.trim() != link.name {
                            app.send(Command::UpdateCallLink { room_id: room.to_owned(), name: Some(app.panes.link_name.trim().to_owned()), restrictions: None });
                        }
                    });
                    ui.add_space(6.);
                    let mut approve = link.restrictions == CallLinkRestrictions::AdminApproval;
                    ui.horizontal(|ui| {
                        ui.add_space(10.);
                        ui.label("Require admin approval");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(10.);
                            if widgets::switch(ui, &mut approve).changed() {
                                let restrictions = if approve { CallLinkRestrictions::AdminApproval } else { CallLinkRestrictions::None };
                                app.send(Command::UpdateCallLink { room_id: room.to_owned(), name: None, restrictions: Some(restrictions) });
                            }
                        });
                    });
                    ui.add_space(6.);
                }
                if menu_row(ui, Some(Icon::Copy), "Copy link", false).clicked() {
                    ui.ctx().copy_text(url.clone());
                    app.toast("Call link copied.");
                }
                let label = if admin { "Delete link" } else { "Remove link" };
                if menu_row(ui, Some(Icon::Trash), label, true).clicked() {
                    app.send(Command::DeleteCallLink { room_id: room.to_owned() });
                    app.panes.link_details = None;
                }
            });
            ui.add_space(12.);
            body(ui, palette, "Anyone with this link can join the call. Share it only with people you trust.");
        });
    });
}

/// The in-call extras: raise hand, reactions, the participants list and
/// people waiting to join a link's call. Drawn over the call screen.
pub fn in_call_extras(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(call) = app.call.clone() else { return };
    if !call.group {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let white = egui::Color32::WHITE;
    // Recent reactions float up from the bottom left, for four seconds.
    let now = std::time::Instant::now();
    app.panes.call_reactions.retain(|(_, at)| now.duration_since(*at).as_secs_f32() < 4.);
    for (i, (r, at)) in app.panes.call_reactions.iter().enumerate() {
        let age = now.duration_since(*at).as_secs_f32();
        let name = if Some(r.demux_id) == call.local_demux_id { "You".to_owned() } else { app.name(&r.aci) };
        let pos = Pos2::new(area.left() + 24., area.bottom() - 220. - i as f32 * 40. - age * 12.);
        let job = {
            let mut job = egui::text::LayoutJob::default();
            crate::chat::append_with_emoji(&mut job, &r.emoji, egui::TextFormat { font_id: theme::regular(24.), color: white, ..Default::default() });
            job.append(&format!("  {name}"), 0., egui::TextFormat { font_id: theme::medium(13.), color: white, ..Default::default() });
            job
        };
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(pos, galley, white);
    }
    // Raised hands, top left.
    if !call.raised_hands.is_empty() {
        let names: Vec<String> = call
            .raised_hands
            .iter()
            .map(|d| if Some(*d) == call.local_demux_id { "You".to_owned() } else { call.participants.iter().find(|p| p.demux_id == *d).map(|p| app.name(&p.aci)).unwrap_or_default() })
            .collect();
        let text = format!("✋ {}", names.join(", "));
        let galley = ui.painter().layout_no_wrap(text, theme::medium(13.), white);
        let rect = Rect::from_min_size(area.min + Vec2::new(16., 16.), galley.size() + Vec2::new(20., 12.));
        ui.painter().rect_filled(rect, CornerRadius::same(10), egui::Color32::from_white_alpha(30));
        ui.painter().galley(rect.min + Vec2::new(10., 6.), galley, white);
    }
    // Buttons, top right: participants, raise hand, react.
    let mut x = area.right() - 16.;
    let mut top_button = |ui: &mut Ui, icon: Icon, on: bool, tip: &str| {
        let rect = Rect::from_min_size(Pos2::new(x - 40., area.top() + 12.), Vec2::splat(40.));
        x -= 48.;
        let response = ui.interact(rect, ui.scope_id().with(("call-top", tip)), Sense::click()).on_hover_text(tip);
        let fill = if on { egui::Color32::from_white_alpha(70) } else if response.hovered() { egui::Color32::from_white_alpha(40) } else { egui::Color32::from_white_alpha(20) };
        ui.painter().circle_filled(rect.center(), 20., fill);
        icon.paint(ui, rect, 20., white);
        response
    };
    let people = top_button(ui, Icon::Group, app.panes.participants_open, "Participants");
    let raised = call.local_demux_id.is_some_and(|d| call.raised_hands.contains(&d));
    let hand = top_button(ui, Icon::RaiseHandLight, raised, if raised { "Lower hand" } else { "Raise hand" });
    let react = top_button(ui, Icon::HeartPlus, app.panes.reactions_open, "React");
    if people.clicked() {
        app.panes.participants_open = !app.panes.participants_open;
    }
    if hand.clicked() {
        app.send(Command::RaiseHand(!raised));
    }
    if react.clicked() {
        app.panes.reactions_open = !app.panes.reactions_open;
    }
    if app.panes.reactions_open {
        egui::Area::new(egui::Id::unique("call-reactions")).fixed_pos(react.rect.left_bottom() + Vec2::new(-200., 8.)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(egui::Color32::from_gray(40)).corner_radius(CornerRadius::same(20)).inner_margin(8).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for emoji in ["❤️", "👍", "👎", "😂", "😮", "😢"] {
                        let mut job = egui::text::LayoutJob::default();
                        crate::chat::append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::regular(24.), color: white, ..Default::default() });
                        if ui.add(egui::Button::new(job).frame(false)).clicked() {
                            app.send(Command::CallReact { emoji: emoji.to_owned() });
                            app.panes.reactions_open = false;
                        }
                    }
                });
            });
        });
    }
    // The participants list (and, for our link, who's waiting).
    if app.panes.participants_open {
        egui::Area::new(egui::Id::unique("call-participants")).fixed_pos(Pos2::new(area.right() - 316., area.top() + 64.)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(egui::Color32::from_gray(38)).corner_radius(CornerRadius::same(16)).inner_margin(12).show(ui, |ui| {
                ui.set_width(280.);
                ui.label(RichText::new(format!("In this call · {}", call.participants.len() + 1)).font(theme::semibold(14.)).color(white));
                ui.add_space(6.);
                ui.label(RichText::new("You").color(white));
                let admin = call.link.as_ref().is_some_and(|l| l.admin);
                for p in &call.participants {
                    ui.horizontal(|ui| {
                        let mut name = app.name(&p.aci);
                        if !p.audio {
                            name.push_str(" (muted)");
                        }
                        ui.label(RichText::new(name).color(white));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if admin && ui.small_button("Remove").clicked() {
                                app.send(Command::RemoveFromCall { demux_id: p.demux_id });
                            }
                            if p.audio && ui.small_button("Mute").on_hover_text("Ask them to mute").clicked() {
                                app.send(Command::RemoteMute { demux_id: p.demux_id });
                            }
                        });
                    });
                }
                if let Some(link) = call.link.as_ref().filter(|l| l.admin && !l.pending.is_empty()) {
                    ui.add_space(8.);
                    ui.label(RichText::new("Waiting to join").font(theme::semibold(13.)).color(white));
                    for aci in &link.pending {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(app.name(aci)).color(white));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Deny").clicked() {
                                    app.send(Command::ApproveCallJoin { aci: aci.clone(), approve: false });
                                }
                                if ui.small_button("Approve").clicked() {
                                    app.send(Command::ApproveCallJoin { aci: aci.clone(), approve: true });
                                }
                            });
                        });
                    }
                }
            });
        });
    }
    // Someone waiting, for a link we administer: a banner even with the list closed.
    if let Some(link) = call.link.as_ref().filter(|l| l.admin && !l.pending.is_empty() && !app.panes.participants_open) {
        let text = format!("{} would like to join", app.name(&link.pending[0]));
        let rect = Rect::from_center_size(Pos2::new(area.center().x, area.top() + 80.), Vec2::new(360., 44.));
        ui.painter().rect_filled(rect, CornerRadius::same(12), egui::Color32::from_gray(45));
        ui.painter().text(rect.left_center() + Vec2::new(14., 0.), egui::Align2::LEFT_CENTER, text, theme::medium(13.), white);
        let approve = Rect::from_min_size(Pos2::new(rect.right() - 84., rect.top() + 8.), Vec2::new(74., 28.));
        let response = ui.interact(approve, ui.scope_id().with("approve-first"), Sense::click());
        ui.painter().rect_filled(approve, CornerRadius::same(14), palette.green);
        ui.painter().text(approve.center(), egui::Align2::CENTER_CENTER, "Approve", theme::medium(12.), white);
        if response.clicked() {
            app.send(Command::ApproveCallJoin { aci: link.pending[0].clone(), approve: true });
        }
    }
    if call.link.as_ref().is_some_and(|l| l.waiting) {
        ui.painter().text(area.center() + Vec2::new(0., 80.), egui::Align2::CENTER_CENTER, "Waiting to be let in…", theme::medium(15.), white);
    }
}
