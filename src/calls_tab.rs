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
use crate::i18n::tr;

/// One row of the history.
struct Row {
    conversation: ConversationId,
    title: String,
    /// Desktop's short status (`CallsList__ItemCallInfo--*`): Missed,
    /// Declined, Call link, Outgoing or Incoming.
    status: &'static str,
    missed: bool,
    video: bool,
    at: i64,
    /// The calls grouped in this row (Desktop's call history group), newest
    /// first: what each was, when, and whether missed.
    group: Vec<(String, i64, bool)>,
}

fn rows(app: &App) -> Vec<Row> {
    let mut out = Vec::new();
    for m in &app.panes.calls {
        let Some(notice) = &m.notice else { continue };
        let title = app.conversation(&m.conversation_id).map(|c| c.title.clone()).unwrap_or_else(|| tr("Unknown").into());
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
                    _ => tr("Group call"),
                };
                (detail.to_owned(), matches!(status, Some(GroupCallStatus::Missed)), true)
            }
            _ => continue,
        };
        let status = status_of(&detail);
        out.push(Row { conversation: m.conversation_id.clone(), title, status, missed, video, at: m.sent_at, group: vec![(detail, m.sent_at, missed)] });
    }
    for call in &app.panes.adhoc {
        let Some(link) = app.panes.links.iter().find(|l| l.room_id == call.room_id) else { continue };
        let detail = match call.status {
            AdhocCallStatus::Pending => tr("Call link"),
            AdhocCallStatus::Joined => "Joined call link",
            AdhocCallStatus::Generic => tr("Call link"),
        };
        out.push(Row {
            conversation: ConversationId::call_link(&link.room_id),
            title: link_title(&link.name),
            status: tr("Call link"),
            missed: false,
            video: true,
            at: call.at,
            group: vec![(detail.into(), call.at, false)],
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.at));
    // Desktop's call history groups: neighbours from the same chat, of the
    // same kind and outcome, on the same day, are one row with a count.
    let mut grouped: Vec<Row> = Vec::new();
    for row in out {
        match grouped.last_mut() {
            Some(last)
                if last.conversation == row.conversation
                    && last.status == row.status
                    && last.video == row.video
                    && crate::format::same_day(last.at, row.at) =>
            {
                last.group.extend(row.group);
            }
            _ => grouped.push(row),
        }
    }
    grouped
}

fn status_of(detail: &str) -> &'static str {
    if detail.starts_with("Missed") {
        tr("Missed")
    } else if detail.starts_with("Declined") {
        tr("Declined")
    } else if detail.starts_with("Incoming") {
        tr("Incoming")
    } else {
        tr("Outgoing")
    }
}

fn link_title(name: &str) -> String {
    if name.is_empty() { "Signal call".into() } else { name.to_owned() }
}

/// Desktop's `CallsNewCall`: a search, then contacts and groups, each with
/// its call buttons.
fn new_call(app: &mut App, ui: &mut Ui, palette: &Palette) {
    ui.add_space(12.);
    ui.horizontal(|ui| {
        ui.add_space(12.);
        if widgets::icon_button(ui, Icon::ChevronLeft, 20., tr("Back")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.panes.new_call = None;
        }
        ui.label(RichText::new(tr("New Call")).font(theme::semibold(18.)));
    });
    let Some(mut query) = app.panes.new_call.clone() else { return };
    ui.add_space(6.);
    ui.horizontal(|ui| {
        ui.add_space(16.);
        ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search").desired_width(ui.available_width() - 16.)).request_focus();
    });
    ui.add_space(8.);
    let q = query.trim().to_lowercase();
    let matches = |c: &&langis_store::Conversation| {
        c.kind != ConversationKind::NoteToSelf && !c.left && !c.blocked && !c.ended && c.removal_stage.is_none() && !c.unregistered(chrono::Utc::now().timestamp_millis()) && (q.is_empty() || c.title.to_lowercase().contains(&q))
    };
    let direct: Vec<_> = app.conversations.iter().filter(matches).filter(|c| c.kind == ConversationKind::Direct).cloned().collect();
    let groups: Vec<_> = app.conversations.iter().filter(matches).filter(|c| c.kind == ConversationKind::Group).cloned().collect();
    let in_call = app.call.is_some();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if direct.is_empty() && groups.is_empty() {
            ui.add_space(28.);
            let text = if q.is_empty() { tr("No recent conversations.").to_owned() } else { format!("No results for \u{201c}{}\u{201d}", query.trim()) };
            ui.vertical_centered(|ui| body(ui, palette, &text));
        }
        for (header, list) in [(tr("Contacts"), &direct), (tr("Groups"), &groups)] {
            if list.is_empty() {
                continue;
            }
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), Sense::hover());
            ui.painter().text(Pos2::new(r.left() + 24., r.center().y), egui::Align2::LEFT_CENTER, header, theme::semibold(14.), palette.text);
            for c in list.iter() {
                // `ListTile` compact: 52 high, a 32px avatar.
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.), Sense::hover());
                let avatar = Rect::from_min_size(Pos2::new(rect.left() + 24., rect.center().y - 16.), Vec2::splat(32.));
                widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
                let mut right = rect.right() - 16.;
                let mut button = |ui: &mut Ui, icon: Icon, label: &str| {
                    let b = Rect::from_min_size(Pos2::new(right - 28., rect.center().y - 14.), Vec2::splat(28.));
                    right -= 28. + 12.;
                    let r = ui.interact(b, egui::Id::unique("new-call").with((&c.id.0, label)), Sense::click());
                    if r.hovered() && !in_call {
                        ui.painter().rect_filled(b, CornerRadius::same(4), palette.fill_secondary);
                    }
                    icon.paint(ui, b.shrink(4.), 20., if in_call { palette.text_secondary } else { palette.text });
                    r.on_hover_text(label).clicked() && !in_call
                };
                if button(ui, Icon::Video, tr("Start a video call")) {
                    call(app, &c.id, true);
                    app.panes.new_call = None;
                }
                if c.kind == ConversationKind::Direct && button(ui, Icon::Phone, tr("Start a call")) {
                    call(app, &c.id, false);
                    app.panes.new_call = None;
                }
                let title = ui.painter().layout_no_wrap(c.title.clone(), theme::regular(14.), palette.text);
                let clip = Rect::from_min_max(Pos2::new(avatar.right() + 12., rect.top()), Pos2::new(right, rect.bottom()));
                ui.painter().with_clip_rect(clip).galley(Pos2::new(clip.left(), rect.center().y - title.size().y / 2.), title, palette.text);
            }
        }
    });
    if let Some(q) = app.panes.new_call.as_mut() {
        *q = query;
    }
}

/// Desktop's "Clear call history?" confirmation.
fn clear_dialog(app: &mut App, ctx: &egui::Context) {
    if !app.panes.clear_calls {
        return;
    }
    let ours = app.panes.links.iter().any(|l| l.admin_key.is_some());
    let body = if ours {
        tr("This will permanently delete all call history. Call links you\u{2019}ve created will no longer work for people who have them.")
    } else {
        tr("This will permanently delete all call history.")
    };
    match crate::dialog::confirm(ctx, egui::Id::unique("clear-calls"), tr("Clear call history?"), body, tr("Clear"), crate::dialog::Variant::StrongDestructive) {
        Some(crate::dialog::Choice::Action) => {
            app.send(Command::ClearCallHistory);
            app.panes.calls.clear();
            app.panes.adhoc.clear();
            app.panes.clear_calls = false;
        }
        Some(crate::dialog::Choice::Cancel) => app.panes.clear_calls = false,
        None => {}
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    clear_dialog(app, ui.ctx());
    if !app.panes.calls_loaded {
        app.panes.calls_loaded = true;
        crate::panes::load_calls(app);
        // Desktop marks every call seen when the tab is viewed.
        app.send(Command::MarkCallsRead { conversation: None });
        // Which groups in the list have a call going, for their Join.
        let mut groups: Vec<ConversationId> = app.panes.calls.iter().map(|m| m.conversation_id.clone()).filter(|c| c.as_group().is_some()).collect();
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        groups.dedup();
        for group in groups.into_iter().take(20) {
            app.send(Command::PeekGroupCall { conversation: group });
        }
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
            if app.panes.new_call.is_some() {
                new_call(app, ui, &palette);
                return;
            }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                crate::sidebar::header_start(app, ui);
                ui.label(RichText::new(tr("Calls")).font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.);
                    ui.spacing_mut().item_spacing.x = 8.;
                    let more = widgets::axo_icon_button_sm(ui, "more", tr("More actions"), false);
                    crate::menu::dropdown(ui, &more, egui::Id::unique("calls-more"), |m| {
                        if m.item(Some("trash"), tr("Clear call history")) {
                            app.panes.clear_calls = true;
                        }
                    });
                    if widgets::axo_icon_button_sm(ui, "phone-plus", tr("New Call"), false).clicked() {
                        app.panes.new_call = Some(String::new());
                    }
                });
            });
            ui.add_space(6.);
            ui.horizontal(|ui| {
                ui.add_space(16.);
                let hint = if app.panes.calls_missed_only { tr("Search missed calls") } else { tr("Search") };
                ui.add(egui::TextEdit::singleline(&mut app.panes.call_search).hint_text(hint).desired_width(ui.available_width() - 16. - 36.));
                // Desktop's missed-calls filter.
                let (rect, r) = ui.allocate_exact_size(Vec2::splat(28.), Sense::click());
                let on = app.panes.calls_missed_only;
                if on {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), palette.accent);
                } else if r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), palette.fill_pressed);
                }
                theme::paint_symbol(ui.painter(), "filter", rect.center(), 16., if on { egui::Color32::WHITE } else { palette.text_secondary });
                if r.on_hover_text(if on { "Show all calls" } else { "Show only missed calls" }).clicked() {
                    app.panes.calls_missed_only = !on;
                }
            });
            ui.add_space(8.);
            if app.panes.calls_missed_only {
                // `CallsList__FilteredByMissedHeader`.
                ui.horizontal(|ui| {
                    ui.add_space(27.);
                    ui.label(RichText::new(tr("Filtered by missed")).font(theme::semibold(13.)).color(palette.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.);
                        if crate::widgets::axo_button(ui, tr("Clear filter"), crate::widgets::Axo::SubtleSecondary).clicked() {
                            app.panes.calls_missed_only = false;
                        }
                    });
                });
                ui.add_space(4.);
            }
            // "Create a call link", as Desktop's first row.
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.), Sense::click());
            let inner = rect.shrink2(Vec2::new(10., 2.));
            if response.hovered() {
                ui.painter().rect_filled(inner, CornerRadius::same(12), palette.fill);
            }
            let circle = Rect::from_min_size(inner.min + Vec2::new(14., 6.), Vec2::splat(40.));
            ui.painter().circle_filled(circle.center(), 20., palette.fill_secondary);
            Icon::Link.paint(ui, circle, 20., palette.text);
            ui.painter().text(Pos2::new(circle.right() + 12., circle.center().y), egui::Align2::LEFT_CENTER, tr("Create a Call Link"), theme::medium(14.), palette.text);
            if response.clicked() {
                app.send(Command::CreateCallLink);
                app.toast("Creating a call link…");
            }
            let query = app.panes.call_search.trim().to_lowercase();
            let missed_only = app.panes.calls_missed_only;
            let list: Vec<Row> = rows(app).into_iter().filter(|r| (query.is_empty() || r.title.to_lowercase().contains(&query)) && (!missed_only || r.missed)).collect();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if list.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| {
                        // Desktop's `CallsList__EmptyState--*`.
                        let q = app.panes.call_search.trim();
                        if !q.is_empty() {
                            body(ui, &palette, &if missed_only { format!("No results for \u{201c}{q}\u{201d} in missed calls") } else { format!("No results for \u{201c}{q}\u{201d}") });
                        } else if missed_only {
                            body(ui, &palette, tr("No missed calls"));
                        } else {
                            ui.label(RichText::new(tr("No calls")).font(theme::semibold(16.)).color(palette.text_secondary));
                            body(ui, &palette, tr("Recent calls will appear here."));
                        }
                    });
                }
                for (i, row) in list.iter().enumerate() {
                    if call_row(app, ui, &palette, row, i) {
                        if let Some(room) = row.conversation.as_call_link() {
                            app.panes.link_name = app.panes.links.iter().find(|l| l.room_id == room).map(|l| l.name.clone()).unwrap_or_default();
                            app.panes.link_details = Some(room.to_owned());
                            app.panes.call_details = None;
                        } else {
                            // Desktop's call details: the chat and this group's calls.
                            app.panes.link_details = None;
                            app.panes.call_details = Some((row.conversation.clone(), row.group.clone()));
                        }
                    }
                }
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        match (app.panes.link_details.clone(), app.panes.call_details.clone()) {
            (Some(room), _) => link_details(app, ui, &palette, &room),
            (None, Some((conversation, calls))) => {
                // Desktop shows the chat's details here, with Message and
                // this call group.
                if !app.panes.details.as_ref().is_some_and(|d| d.conversation == conversation && d.calls.as_ref() == Some(&calls)) {
                    crate::details::open(app, conversation);
                    if let Some(d) = app.panes.details.as_mut() {
                        d.calls = Some(calls);
                    }
                }
                crate::details::show(app, ui);
            }
            (None, None) => {
                // `CallsTab__EmptyStateText--with-icon-2`.
                ui.centered_and_justified(|ui| {
                    let mut job = egui::text::LayoutJob::default();
                    let f = egui::TextFormat { font_id: theme::regular(14.), color: palette.text_secondary, ..Default::default() };
                    job.append("Click ", 0., f.clone());
                    job.append("\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}", 0., f.clone());
                    job.append(" to start a new voice or video call.", 0., f);
                    let r = ui.label(job);
                    let x = r.rect.left() + ui.painter().layout_no_wrap("Click ".into(), theme::regular(14.), palette.text_secondary).size().x;
                    Icon::PhonePlus.paint(ui, Rect::from_min_size(Pos2::new(x + 2., r.rect.center().y - 9.), Vec2::splat(18.)), 18., palette.text_secondary);
                });
            }
        }
    });
}

/// Calls a chat (a group's lobby first) or joins a link's call.
fn call(app: &mut App, conversation: &ConversationId, video: bool) {
    crate::panes::open_lobby(app, conversation.clone(), video);
}

/// One history row: avatar, name, what happened, when, and a call button.
/// True when the row (not the button) was clicked.
fn call_row(app: &mut App, ui: &mut Ui, palette: &Palette, row: &Row, i: usize) -> bool {
    let width = ui.available_width();
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, 64.), Sense::click());
    let rect = Rect::from_min_max(outer.min + Vec2::new(10., 2.), outer.max - Vec2::new(10., 2.));
    let selected = row.conversation.as_call_link().is_some_and(|r| app.panes.link_details.as_deref() == Some(r))
        || app.panes.call_details.as_ref().is_some_and(|(c, g)| *c == row.conversation && g.first().map(|x| x.1) == row.group.first().map(|x| x.1));
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { palette.fill_pressed } else { palette.fill });
    }
    let avatar = Rect::from_min_size(rect.min + Vec2::new(14., 10.), Vec2::splat(36.));
    match app.conversation(&row.conversation) {
        Some(c) => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c)),
        None => {
            ui.painter().circle_filled(avatar.center(), 18., palette.fill_secondary);
            Icon::Link.paint(ui, avatar, 18., palette.text);
        }
    }
    let x = avatar.right() + 12.;
    let painter = ui.painter_at(rect);
    let title = painter.text(Pos2::new(x, rect.top() + 12.), egui::Align2::LEFT_TOP, &row.title, theme::semibold(14.), palette.text);
    // Desktop's bell-slash after a muted chat's name.
    if app.conversation(&row.conversation).is_some_and(|c| c.muted) {
        theme::paint_symbol(&painter, "bell-slash", Pos2::new(title.right() + 10., title.center().y), 14., palette.text_secondary);
    }
    // A group's or link's call going on now: Desktop's `ItemCallInfo--Active`.
    let active = (row.conversation.as_group().is_some() || row.conversation.as_call_link().is_some())
        && app.panes.active_calls.get(&row.conversation).is_some_and(|u| !u.is_empty())
        && app.call.as_ref().is_none_or(|c| c.conversation != row.conversation);
    let color = if row.missed && !active { palette.destructive } else { palette.text_secondary };
    // Desktop's subtitle: "(n) " for a group, the status, then the time or
    // Active.
    let count = if row.group.len() > 1 { format!("({}) ", row.group.len()) } else { String::new() };
    let when = if active { tr("Active").to_owned() } else { crate::format::short_timestamp(row.at) };
    let detail = format!("{count}{} \u{b7} {when}", row.status);
    painter.text(Pos2::new(x, rect.top() + 32.), egui::Align2::LEFT_TOP, detail, theme::regular(12.), color);
    if active {
        let label = ui.painter().layout_no_wrap(tr("Join").into(), theme::semibold(13.), egui::Color32::WHITE);
        let pill = Rect::from_center_size(Pos2::new(rect.right() - 12. - (label.size().x + 24.) / 2., rect.center().y), label.size() + Vec2::new(24., 12.));
        let hit = ui.interact(pill, ui.scope_id().with(("join-active", i)), Sense::click());
        ui.painter().rect_filled(pill, CornerRadius::same(255), if hit.hovered() { palette.accent.gamma_multiply(0.85) } else { palette.accent });
        ui.painter().galley(pill.center() - label.size() / 2., label, egui::Color32::WHITE);
        if hit.clicked() {
            crate::panes::open_lobby(app, row.conversation.clone(), true);
            return false;
        }
        return response.clicked();
    }
    // The call-back button on the right.
    let button = Rect::from_center_size(Pos2::new(rect.right() - 28., rect.center().y), Vec2::splat(32.));
    let hit = ui.interact(button, ui.scope_id().with(("call-back", i)), Sense::click());
    if hit.hovered() {
        ui.painter().circle_filled(button.center(), 16., palette.fill_pressed);
    }
    let icon = if row.conversation.as_call_link().is_some() { Icon::Video } else if row.video { Icon::Video } else { Icon::Phone };
    icon.paint(ui, button, 18., palette.text);
    if hit.on_hover_text(if row.conversation.as_call_link().is_some() { tr("Join") } else { tr("Call back") }).clicked() {
        call(app, &row.conversation.clone(), row.video);
        return false;
    }
    response.clicked()
}

/// Desktop's `CallLinkDetails`.
/// Desktop's CallLinkAddNameModal: the name, Save.
fn link_name_dialog(app: &mut App, ui: &mut Ui, room: &str, current: &str) {
    if !app.panes.link_name_open {
        return;
    }
    let mut answer = None;
    let closed = crate::panes::modal(ui, "call-link-name", 360., |ui| {
        crate::panes::title(ui, if current.is_empty() { tr("Add call name") } else { tr("Edit call name") });
        widgets::text_field(ui, &mut app.panes.link_name, tr("Call name"), false);
        answer = crate::panes::buttons(ui, &[tr("Save"), tr("Cancel")]);
    });
    match answer {
        Some(0) => {
            app.panes.link_name_open = false;
            if app.panes.link_name.trim() != current {
                app.send(Command::UpdateCallLink { room_id: room.to_owned(), name: Some(app.panes.link_name.trim().to_owned()), restrictions: None });
            }
        }
        Some(_) => app.panes.link_name_open = false,
        None if closed => app.panes.link_name_open = false,
        None => {}
    }
}

/// Desktop's `CallingAdhocCallInfo` for a call link we haven't joined
/// (`isUnknownContactDiscrete` false): those we know by name, sorted by it,
/// and how many others there are, who show as one row.
pub fn known_and_unknown(app: &App, users: &[String]) -> (Vec<String>, usize) {
    let (mut known, unknown): (Vec<String>, Vec<String>) = users.iter().cloned().partition(|u| *u == app.aci || app.names.contains_key(u));
    known.sort_by_key(|u| if *u == app.aci { String::new() } else { app.name(u).to_lowercase() });
    (known, unknown.len())
}

/// Desktop's `UnknownContacts` row: up to three blank avatars, "N people"
/// (or "+N more" after people we know), and an info button; true when the
/// button was clicked.
pub fn unknown_row(ui: &mut Ui, count: usize, after_known: bool, color: egui::Color32) -> bool {
    let label = if after_known {
        crate::i18n::tf("icu:CallingAdhocCallInfo__UnknownContactLabel--in-addition", &format!("+{count} more"), &[("count", crate::i18n::Arg::from(count))])
    } else {
        crate::i18n::tf("icu:CallingAdhocCallInfo__UnknownContactLabel", &if count == 1 { "1 person".to_owned() } else { format!("{count} people") }, &[("count", crate::i18n::Arg::from(count))])
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), Sense::hover());
    let shown = count.min(3);
    let size = match shown { 1 => 32., 2 => 28., _ => 24. };
    for i in 0..shown {
        let at = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * size * 0.6, rect.center().y - size / 2.), Vec2::splat(size));
        widgets::paint_avatar(ui, at, &widgets::AvatarSpec::person("", i as u8 * 3));
    }
    let text_x = rect.left() + (shown as f32 - 1.) * size * 0.6 + size + 10.;
    ui.painter().text(Pos2::new(text_x, rect.center().y), egui::Align2::LEFT_CENTER, label, theme::regular(14.), color);
    let info = Rect::from_center_size(Pos2::new(rect.right() - 14., rect.center().y), Vec2::splat(24.));
    theme::paint_symbol(ui.painter(), "info", info.center(), 18., color);
    ui.interact(info, ui.scope_id().with("unknown-info"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Desktop's info dialog from that row, while `open` (an egui temp flag).
pub fn unknown_info_dialog(ctx: &egui::Context, open: egui::Id) {
    if !ctx.data(|d| d.get_temp::<bool>(open).unwrap_or(false)) {
        return;
    }
    let body = tr("Before joining a call you can only see the names of phone contacts, people you\u{2019}re in a group with, or people you\u{2019}ve chatted with 1:1. You\u{2019}ll see all names and photos once you\u{2019}ve joined the call.");
    if crate::dialog::alert(ctx, egui::Id::unique("adhoc-unknown-info"), "", body, tr("OK")) {
        ctx.data_mut(|d| d.remove::<bool>(open));
    }
}

/// Peeks a link's call every 10 seconds while its details are open, so
/// they show who's in it.
fn peek_link(app: &mut App, ctx: &egui::Context, room: &str) {
    let id = egui::Id::unique(("link-details-peek", room));
    let now = ctx.input(|i| i.time);
    if ctx.data(|d| d.get_temp::<f64>(id)).is_none_or(|at| now - at > 10.) {
        ctx.data_mut(|d| d.insert_temp(id, now));
        app.send(Command::PeekGroupCall { conversation: ConversationId::call_link(room) });
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(10));
}

fn link_details(app: &mut App, ui: &mut Ui, palette: &Palette, room: &str) {
    let Some(link) = app.panes.links.iter().find(|l| l.room_id == room).cloned() else {
        ui.centered_and_justified(|ui| {
            crate::widgets::spinner(ui, 24.);
        });
        return;
    };
    let admin = link.admin_key.is_some();
    let url = langis_core::call_link_url(&link.root_key).unwrap_or_default();
    link_name_dialog(app, ui, room, &link.name);
    peek_link(app, ui.ctx(), room);
    let info_open = egui::Id::unique("link-details-unknown-info");
    unknown_info_dialog(ui.ctx(), info_open);
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
                    if widgets::pill_button(ui, tr("Join"), true).clicked() {
                        crate::panes::open_lobby(app, ConversationId::call_link(room), true);
                    }
                });
            });
            ui.add_space(20.);
            // Who's in its call now, as Desktop's call link info lists them
            // before joining: people we know, then "+N more".
            let users = app.panes.active_calls.get(&ConversationId::call_link(room)).cloned().unwrap_or_default();
            if !users.is_empty() {
                let (known, unknown) = known_and_unknown(app, &users);
                egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let n = users.len();
                    let title = crate::i18n::tf("icu:calling__in-this-call", &if n == 1 { "In this call \u{b7} 1 person".to_owned() } else { format!("In this call \u{b7} {n} people") }, &[("people", crate::i18n::Arg::from(n))]);
                    ui.label(RichText::new(title).font(theme::semibold(14.)));
                    ui.add_space(6.);
                    for u in &known {
                        ui.horizontal(|ui| {
                            match app.conversation(&ConversationId::direct(u)) {
                                Some(c) => widgets::avatar(ui, &widgets::AvatarSpec::of(c), 32.),
                                None => widgets::avatar(ui, &widgets::AvatarSpec::person(&app.name(u), 0), 32.),
                            };
                            ui.label(RichText::new(if *u == app.aci { tr("You").to_owned() } else { app.name(u) }).font(theme::regular(14.)));
                        });
                        ui.add_space(4.);
                    }
                    if unknown > 0 && unknown_row(ui, unknown, !known.is_empty(), palette.text) {
                        ui.ctx().data_mut(|d| d.insert_temp(info_open, true));
                    }
                });
                ui.add_space(12.);
            }
            egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                // While its call goes on, Desktop's settings and Delete wait.
                let active = app.panes.active_calls.get(&ConversationId::call_link(room)).is_some_and(|u| !u.is_empty());
                if admin {
                    // Desktop's Add call name / Edit call name row.
                    let label = if link.name.is_empty() { tr("Add call name") } else { tr("Edit call name") };
                    if menu_row(ui, Some(Icon::Edit), label, false).clicked() {
                        app.panes.link_name = link.name.clone();
                        app.panes.link_name_open = true;
                    }
                    let mut approve = link.restrictions == CallLinkRestrictions::AdminApproval;
                    let row = ui.horizontal(|ui| {
                        ui.add_space(10.);
                        ui.label(tr("Require admin approval"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(10.);
                            ui.add_enabled_ui(!active, |ui| {
                                if widgets::switch(ui, &mut approve).changed() {
                                    let restrictions = if approve { CallLinkRestrictions::AdminApproval } else { CallLinkRestrictions::None };
                                    app.send(Command::UpdateCallLink { room_id: room.to_owned(), name: None, restrictions: Some(restrictions) });
                                }
                            });
                        });
                    });
                    if active {
                        row.response.on_hover_text(tr("This setting can\u{2019}t be changed while the call is active"));
                    }
                    ui.add_space(6.);
                }
                if menu_row(ui, Some(Icon::Copy), tr("Copy link"), false).clicked() {
                    ui.ctx().copy_text(url.clone());
                    app.toast("Call link copied.");
                }
                // Desktop's `CallLinkDetails__ShareLinkViaSignal`.
                if menu_row(ui, Some(Icon::Forward), tr("Share link via Signal"), false).clicked() {
                    app.panes.share_text = Some((url.clone(), Vec::new(), String::new()));
                }
                let label = if admin { tr("Delete link") } else { "Remove link" };
                let r = ui.add_enabled_ui(!active, |ui| menu_row(ui, Some(Icon::Trash), label, true)).inner;
                if active {
                    r.on_hover_text(tr("This link can\u{2019}t be deleted while the call is active"));
                } else if r.clicked() {
                    app.panes.confirm_delete_link = Some(room.to_owned());
                }
            });
            ui.add_space(12.);
            body(ui, palette, "Anyone with this link can join the call. Share it only with people you trust.");
            // Desktop's `CallHistoryGroupPanelSection` for the link: its calls.
            let calls: Vec<(i64, &str)> = app
                .panes
                .adhoc
                .iter()
                .filter(|c| c.room_id == room)
                .map(|c| (c.at, match c.status { AdhocCallStatus::Joined => "Joined", _ => tr("Call link") }))
                .collect();
            if !calls.is_empty() {
                ui.add_space(16.);
                egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(12)).inner_margin(14).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let mut day = None;
                    for (at, what) in calls {
                        let label = crate::format::day_label(at);
                        if day.as_ref() != Some(&label) {
                            ui.label(RichText::new(&label).font(theme::semibold(13.)));
                            ui.add_space(4.);
                            day = Some(label);
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::format::message_time(at)).font(theme::regular(13.)).color(palette.text_secondary));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(what).font(theme::regular(13.)));
                            });
                        });
                    }
                });
            }
        });
    });
    // Desktop's `DeleteLinkModal`.
    if app.panes.confirm_delete_link.as_deref() == Some(room) {
        let (title, body_text, action) = if admin {
            (tr("Delete call link?"), tr("This link will no longer work for anyone who has it."), tr("Delete"))
        } else {
            ("Remove link?", "This link will be removed from your call history. You can join again if someone shares it with you.", tr("Remove"))
        };
        match crate::dialog::confirm(ui.ctx(), egui::Id::unique("delete-call-link"), title, body_text, action, crate::dialog::Variant::StrongDestructive) {
            Some(crate::dialog::Choice::Action) => {
                app.send(Command::DeleteCallLink { room_id: room.to_owned() });
                app.panes.link_details = None;
                app.panes.confirm_delete_link = None;
            }
            Some(crate::dialog::Choice::Cancel) => app.panes.confirm_delete_link = None,
            None => {}
        }
    }
}

/// The in-call extras: raise hand, reactions, the participants list and
/// people waiting to join a link's call. Drawn over the call screen.
/// The group call's extras: reactions, raised hands, the reaction picker
/// and the participants list.
/// Desktop's `CallingToastManager`: toasts at the call's top center, on
/// gray-80 in 13px white, three seconds each, newest below.
fn call_toasts(app: &mut App, ui: &Ui, area: Rect) {
    app.panes.call_toasts.retain(|(_, at)| at.elapsed() < std::time::Duration::from_secs(3));
    if app.panes.call_toasts.is_empty() {
        return;
    }
    let mut y = area.top() + 20.;
    for (text, _) in app.panes.call_toasts.iter().rev().take(3).rev() {
        let galley = ui.painter().layout_no_wrap(text.clone(), theme::regular(13.), egui::Color32::WHITE);
        let rect = Rect::from_center_size(Pos2::new(area.center().x, y + 18.), galley.size() + Vec2::new(32., 18.));
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("call-toasts")));
        painter.rect_filled(rect, CornerRadius::same(8), theme::rgb(0x2e2e2e));
        painter.galley(rect.center() - galley.size() / 2., galley, egui::Color32::WHITE);
        y += rect.height() + 8.;
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
}

/// While we present: Desktop's `CallingScreenSharingController`, "You're
/// presenting to everyone." and Stop sharing, at the call's top center.
/// Returns how much room it takes.
fn presenting_bar(app: &mut App, ui: &Ui, area: Rect) -> f32 {
    if !app.call.as_ref().is_some_and(|c| c.sharing) {
        return 0.;
    }
    let white = egui::Color32::WHITE;
    let text = ui.painter().layout_no_wrap(tr("You're presenting to everyone.").into(), theme::regular(13.), white);
    let button = ui.painter().layout_no_wrap(tr("Stop sharing").into(), theme::semibold(13.), white);
    let width = 16. + 16. + 8. + text.size().x + 16. + button.size().x + 24. + 12.;
    let rect = Rect::from_center_size(Pos2::new(area.center().x, area.top() + 20. + 22.), Vec2::new(width, 44.));
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("presenting-bar")));
    painter.rect_filled(rect, CornerRadius::same(10), theme::rgb(0x2e2e2e));
    crate::icons::Icon::ShareScreenFillLight.paint(ui, Rect::from_center_size(Pos2::new(rect.left() + 24., rect.center().y), Vec2::splat(16.)), 16., white);
    painter.galley(Pos2::new(rect.left() + 40., rect.center().y - text.size().y / 2.), text, white);
    let stop = Rect::from_min_size(Pos2::new(rect.right() - 12. - button.size().x - 24., rect.center().y - 14.), Vec2::new(button.size().x + 24., 28.));
    let r = ui.interact(stop, ui.scope_id().with("stop-sharing"), Sense::click());
    painter.rect_filled(stop, CornerRadius::same(14), if r.hovered() { theme::rgb(0xd32f2f) } else { theme::rgb(0xf44336) });
    painter.galley(stop.center() - button.size() / 2., button, white);
    if r.clicked() {
        app.send(Command::StopSharing);
    }
    52.
}

pub fn in_call_extras(app: &mut App, ui: &mut Ui, area: Rect) {
    let taken = presenting_bar(app, ui, area);
    call_toasts(app, ui, Rect::from_min_max(area.min + Vec2::new(0., taken), area.max));
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
        let name = if Some(r.demux_id) == call.local_demux_id { tr("You").to_owned() } else { app.name(&r.aci) };
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
    // Desktop's raised hands toast (`CallControls__RaiseHandsToast--*`):
    // who raised a hand, and "Open queue", or "Lower" when ours is up;
    // the queue (`CallingRaisedHandsList`) lists them in order.
    if call.raised_hands.is_empty() {
        app.panes.raised_open = false;
    } else {
        let mine = call.local_demux_id.is_some_and(|d| call.raised_hands.contains(&d));
        let others: Vec<(u32, String)> = call
            .raised_hands
            .iter()
            .filter(|d| Some(**d) != call.local_demux_id)
            .map(|d| (*d, call.participants.iter().find(|p| p.demux_id == *d).map(|p| app.name(&p.aci)).unwrap_or_default()))
            .collect();
        let first = |i: usize| others.get(i).map(|(_, n)| n.split_whitespace().next().unwrap_or(n).to_owned()).unwrap_or_default();
        let text = match (mine, others.len()) {
            (true, 0) => tr("You raised a hand.").to_owned(),
            (true, 1) => crate::i18n::tf("icu:CallControls__RaiseHandsToast--you-and-one", &format!("You and {} raised a hand.", first(0)), &[("otherName", crate::i18n::Arg::from(&first(0)))]),
            (true, n) => crate::i18n::tf("icu:CallControls__RaiseHandsToast--you-and-more", &format!("You, {}, and {} more raised a hand.", first(0), n - 1), &[("otherName", crate::i18n::Arg::from(&first(0))), ("overflowCount", crate::i18n::Arg::from(&n - 1))]),
            (false, 1) => crate::i18n::tf("icu:CallControls__RaiseHandsToast--one", &format!("{} raised a hand.", first(0)), &[("name", crate::i18n::Arg::from(&first(0)))]),
            (false, 2) => crate::i18n::tf("icu:CallControls__RaiseHandsToast--two", &format!("{} and {} raised a hand.", first(0), first(1)), &[("name", crate::i18n::Arg::from(&first(0))), ("otherName", crate::i18n::Arg::from(&first(1)))]),
            (false, n) => crate::i18n::tf("icu:CallControls__RaiseHandsToast--more", &format!("{}, {}, and {} more raised a hand.", first(0), first(1), n - 2), &[("name", crate::i18n::Arg::from(&first(0))), ("otherName", crate::i18n::Arg::from(&first(1))), ("overflowCount", crate::i18n::Arg::from(&n - 2))]),
        };
        let button = if mine { tr("Lower") } else { tr("Open queue") };
        let galley = ui.painter().layout_no_wrap(text, theme::regular(13.), white);
        let label = ui.painter().layout_no_wrap(button.to_owned(), theme::semibold(13.), theme::rgb(0xb0c8f9));
        let rect = Rect::from_min_size(area.min + Vec2::new(16., 16.), Vec2::new(16. + 16. + 8. + galley.size().x + 16. + label.size().x + 16., 40.));
        ui.painter().rect_filled(rect, CornerRadius::same(8), theme::rgb(0x2e2e2e));
        theme::paint_symbol(ui.painter(), "raisehand-fill", Pos2::new(rect.left() + 24., rect.center().y), 16., white);
        ui.painter().galley(Pos2::new(rect.left() + 40., rect.center().y - galley.size().y / 2.), galley, white);
        let at = Rect::from_min_size(Pos2::new(rect.right() - 16. - label.size().x, rect.center().y - label.size().y / 2.), label.size());
        let r = ui.interact(at.expand(6.), ui.scope_id().with("raised-hands-button"), Sense::click());
        ui.painter().galley(at.min, label, theme::rgb(0xb0c8f9));
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if mine {
                app.send(Command::RaiseHand(false));
            } else {
                app.panes.raised_open = !app.panes.raised_open;
            }
        }
        if app.panes.raised_open {
            let mut close = false;
            egui::Area::new(egui::Id::unique("raised-hands")).fixed_pos(Pos2::new(rect.left(), rect.bottom() + 8.)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(theme::rgb(0x2e2e2e)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(300.);
                    ui.horizontal(|ui| {
                        let n = call.raised_hands.len();
                        ui.label(RichText::new(format!("{n} raised {}", if n == 1 { "hand" } else { "hands" })).font(theme::semibold(14.)).color(white));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (x, r) = ui.allocate_exact_size(Vec2::splat(18.), Sense::click());
                            theme::paint_symbol(ui.painter(), "x", x.center(), 16., white);
                            close = r.clicked();
                        });
                    });
                    ui.add_space(6.);
                    for (i, d) in call.raised_hands.iter().enumerate() {
                        let me = Some(*d) == call.local_demux_id;
                        let (name, aci) = if me {
                            (tr("You").to_owned(), app.aci.clone())
                        } else {
                            let aci = call.participants.iter().find(|p| p.demux_id == *d).map(|p| p.aci.clone()).unwrap_or_default();
                            (app.name(&aci), aci)
                        };
                        let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.), Sense::hover());
                        ui.painter().text(Pos2::new(row.left() + 4., row.center().y), egui::Align2::LEFT_CENTER, (i + 1).to_string(), theme::regular(13.), egui::Color32::from_white_alpha(180));
                        let color = aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                        let initials_of = if me { app.me.0.clone() } else { name.clone() };
                        widgets::paint_avatar(ui, Rect::from_min_size(Pos2::new(row.left() + 22., row.center().y - 16.), Vec2::splat(32.)), &widgets::AvatarSpec::person(&initials_of, color));
                        ui.painter().text(Pos2::new(row.left() + 62., row.center().y), egui::Align2::LEFT_CENTER, &name, theme::regular(13.), white);
                        if me {
                            let lower = Rect::from_min_size(Pos2::new(row.right() - 60., row.center().y - 12.), Vec2::new(56., 24.));
                            let r = ui.interact(lower, ui.scope_id().with("lower-own-hand"), Sense::click());
                            ui.painter().text(lower.center(), egui::Align2::CENTER_CENTER, tr("Lower"), theme::semibold(13.), theme::rgb(0xb0c8f9));
                            if r.clicked() {
                                app.send(Command::RaiseHand(false));
                            }
                        } else {
                            theme::paint_symbol(ui.painter(), "raisehand-fill", Pos2::new(row.right() - 16., row.center().y), 16., white);
                        }
                    }
                });
            });
            if close {
                app.panes.raised_open = false;
            }
        }
    }
    if app.panes.reactions_open {
        // `CallControls__ReactionPickerContainer`: 70 above the bottom,
        // min(44%, 32vw) in.
        let at = Pos2::new(area.left() + (area.width() * 0.44).min(area.width() * 0.32), area.bottom() - 16. - 70. - 56.);
        egui::Area::new(egui::Id::unique("call-reactions")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(egui::Color32::from_gray(40)).corner_radius(CornerRadius::same(20)).inner_margin(8).show(ui, |ui| {
                ui.horizontal(|ui| {
                    // The account's preferred reactions, then More for any.
                    for emoji in app.reactions.clone() {
                        let mut job = egui::text::LayoutJob::default();
                        crate::chat::append_with_emoji(&mut job, &emoji, egui::TextFormat { font_id: theme::regular(24.), color: white, ..Default::default() });
                        if ui.add(egui::Button::new(job).frame(false)).clicked() {
                            app.send(Command::CallReact { emoji: emoji.clone() });
                            app.panes.reactions_open = false;
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new("+").font(theme::semibold(22.)).color(white)).frame(false)).on_hover_text(tr("More reactions")).clicked() {
                        app.panes.call_emoji = Some(Default::default());
                        app.panes.reactions_open = false;
                    }
                });
            });
        });
    }
    // The full emoji picker, from More.
    if let Some(mut picker) = app.panes.call_emoji.take() {
        let mut open = true;
        let recent = app.recent_emoji.clone();
        let anchor = Pos2::new(area.center().x - crate::emoji_picker::WIDTH / 2., area.bottom() - 100.);
        if let Some(emoji) = picker.show(ui.ctx(), anchor, &mut open, &recent) {
            app.use_emoji(emoji);
            app.send(Command::CallReact { emoji: emoji.to_owned() });
        } else if open {
            app.panes.call_emoji = Some(picker);
        }
    }
    // Desktop's `CallingParticipantsList`: 320x440 on gray-80, radius 10,
    // "In this call · N people" and its close X, then a row per person
    // (avatar, name, their presenting, camera and microphone states),
    // each opening their ContactModal.
    if app.panes.participants_open {
        let mut close = false;
        let mut copied = false;
        let mut share_link: Option<String> = None;
        let mut open_contact = None;
        egui::Area::new(egui::Id::unique("call-participants")).fixed_pos(Pos2::new(area.right() - 336., area.top() + 20.)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(theme::rgb(0x2e2e2e)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                ui.set_width(310.);
                ui.set_max_height((area.height() - 107.).min(430.));
                ui.horizontal(|ui| {
                    ui.add_space(5.);
                    let n = call.participants.len() + 1;
                    let title = if n <= 1 { tr("No one else is here").to_owned() } else { format!("In this call \u{b7} {n} {}", if n == 1 { "person" } else { "people" }) };
                    ui.label(RichText::new(title).font(theme::semibold(14.)).color(white));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (x, r) = ui.allocate_exact_size(Vec2::splat(18.), Sense::click());
                        theme::paint_symbol(ui.painter(), "x", x.center(), 16., white);
                        if r.on_hover_text("Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.);
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    let my_name = app.me.0.clone();
                    let row = |ui: &mut Ui, name: &str, aci: &str, photo: Option<&str>, presenting: bool, video: bool, audio: bool, me: bool| -> bool {
                        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.), if me { Sense::hover() } else { Sense::click() });
                        if !me && r.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(6), theme::rgb(0x6b6b6b));
                        }
                        let color = aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                        let avatar = Rect::from_min_size(Pos2::new(rect.left() + 10., rect.center().y - 16.), Vec2::splat(32.));
                        let initials_of = if me { my_name.as_str() } else { name };
                        widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo, ..widgets::AvatarSpec::person(initials_of, color) });
                        ui.painter().text(Pos2::new(avatar.right() + 8., rect.center().y), egui::Align2::LEFT_CENTER, name, theme::regular(13.), white);
                        let mut x = rect.right() - 10. - 8.;
                        // Desktop's status icons: muted, camera off, presenting.
                        for (symbol, show) in [("mic-slash-fill", !audio), ("videocamera-slash-fill", !video)] {
                            if show {
                                theme::paint_symbol(ui.painter(), symbol, Pos2::new(x, rect.center().y), 16., white);
                                x -= 32.;
                            }
                        }
                        if presenting {
                            crate::icons::Icon::ShareScreenFillLight.paint(ui, Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(16.)), 16., white);
                        }
                        r.clicked()
                    };
                    let me = (app.me.0.clone(), app.me.1.clone());
                    let my_video = app.call_view.video_on.unwrap_or(call.video);
                    row(ui, tr("You"), &app.aci, me.1.as_deref(), call.sharing, my_video, app.call_view.audio_on, true);
                    for p in &call.participants {
                        let name = app.name(&p.aci);
                        let photo = app.conversation(&ConversationId::direct(&p.aci)).and_then(|c| c.avatar_path.clone());
                        if row(ui, &name, &p.aci, photo.as_deref(), p.presenting, p.video, p.audio, false) {
                            open_contact = Some(p.aci.clone());
                        }
                    }
                    // A call link's: Desktop's Copy and Share buttons.
                    if let Some(room) = call.conversation.as_call_link()
                        && let Some(url) = app.panes.links.iter().find(|l| l.room_id == room).and_then(|l| langis_core::call_link_url(&l.root_key))
                    {
                        ui.add_space(8.);
                        for (symbol, label, share) in [("copy", tr("Copy call link"), false), ("forward", tr("Share call link via Signal"), true)] {
                            let (r, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.), Sense::click());
                            if hit.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), egui::Color32::from_white_alpha(20));
                            }
                            theme::paint_symbol(ui.painter(), symbol, Pos2::new(r.left() + 18., r.center().y), 18., white);
                            ui.painter().text(Pos2::new(r.left() + 40., r.center().y), egui::Align2::LEFT_CENTER, label, theme::regular(14.), white);
                            if hit.clicked() {
                                if share {
                                    share_link = Some(url.clone());
                                } else {
                                    ui.ctx().copy_text(url.clone());
                                    copied = true;
                                }
                            }
                        }
                    }
                });
            });
        });
        if close {
            app.panes.participants_open = false;
        }
        if copied {
            app.panes.call_toasts.push((tr("Link copied").into(), std::time::Instant::now()));
        }
        if let Some(url) = share_link {
            app.panes.share_text = Some((url, Vec::new(), String::new()));
            app.call_view.pip = true;
        }
        if let Some(aci) = open_contact {
            crate::contact_modal::open(app, Some(call.conversation.clone()), aci);
        }
    }
    // Desktop's `CallingPendingParticipants`, for a link we administer: the
    // first request ("Would like to join...") with approve and deny and
    // "+N requests", or, expanded, every request with Approve all and
    // Deny all (each asking first).
    if let Some(link) = call.link.as_ref().filter(|l| l.admin && !l.pending.is_empty()) {
        let pending = link.pending.clone();
        let n = pending.len();
        let expanded = app.panes.pending_expanded && n > 1;
        let mut act: Vec<(String, bool)> = Vec::new();
        let mut open_pending: Option<String> = None;
        let mut toggle = false;
        egui::Area::new(egui::Id::unique("call-pending")).fixed_pos(Pos2::new(area.left() + 20., area.top() + 20.)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(theme::rgb(0x2e2e2e)).corner_radius(CornerRadius::same(16)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(320.);
                let person = |ui: &mut Ui, aci: &str, subtitle: Option<&str>, act: &mut Vec<(String, bool)>, open: &mut Option<String>| {
                    let row = ui.horizontal(|ui| {
                        let name = app.name(aci);
                        let color = aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                        let photo = app.conversation(&ConversationId::direct(aci)).and_then(|c| c.avatar_path.clone());
                        let (at, _) = ui.allocate_exact_size(Vec2::splat(36.), Sense::hover());
                        widgets::paint_avatar(ui, at, &widgets::AvatarSpec { photo: photo.as_deref(), ..widgets::AvatarSpec::person(&name, color) });
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            ui.label(RichText::new(&name).font(theme::semibold(14.)).color(white));
                            if let Some(sub) = subtitle {
                                ui.label(RichText::new(sub).font(theme::regular(13.)).color(egui::Color32::from_white_alpha(180)));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let round = |ui: &mut Ui, symbol: &str, fill: egui::Color32, tip: &str| {
                                let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                                ui.painter().circle_filled(r.center(), 16., fill);
                                theme::paint_symbol(ui.painter(), symbol, r.center(), 18., white);
                                resp.on_hover_text(tip).clicked()
                            };
                            if round(ui, "check", palette.green, tr("Approve join request")) {
                                act.push((aci.to_owned(), true));
                            }
                            if round(ui, "x", palette.destructive, tr("Deny join request")) {
                                act.push((aci.to_owned(), false));
                            }
                        });
                    });
                    // The person opens Desktop's CallLinkPendingParticipantModal.
                    let who = egui::Rect::from_min_max(row.response.rect.min, egui::pos2(row.response.rect.right() - 80., row.response.rect.bottom()));
                    if ui.interact(who, ui.scope_id().with(("pending-person", aci)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *open = Some(aci.to_owned());
                    }
                };
                if expanded {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{n} {} to join the call", if n == 1 { "request" } else { "requests" })).font(theme::semibold(14.)).color(white));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (x, r) = ui.allocate_exact_size(Vec2::splat(18.), Sense::click());
                            theme::paint_symbol(ui.painter(), "chevron-up", x.center(), 16., white);
                            if r.clicked() {
                                toggle = true;
                            }
                        });
                    });
                    ui.add_space(8.);
                    egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                        for aci in &pending {
                            person(ui, aci, None, &mut act, &mut open_pending);
                            ui.add_space(6.);
                        }
                    });
                    ui.add_space(6.);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Deny all")).color(white)).fill(theme::rgb(0x4a4a4a)).corner_radius(CornerRadius::same(15))).clicked() {
                            app.panes.pending_all = Some(false);
                        }
                        if ui.add(egui::Button::new(RichText::new(tr("Approve all")).color(white)).fill(palette.green).corner_radius(CornerRadius::same(15))).clicked() {
                            app.panes.pending_all = Some(true);
                        }
                    });
                } else {
                    person(ui, &pending[0], Some(tr("Would like to join...")), &mut act, &mut open_pending);
                    if n > 1 {
                        ui.add_space(6.);
                        let more = n - 1;
                        let r = ui.add(egui::Label::new(RichText::new(format!("+{more} {}", if more == 1 { "request" } else { "requests" })).font(theme::semibold(13.)).color(white)).sense(Sense::click()));
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            toggle = true;
                        }
                    }
                }
            });
        });
        if toggle {
            app.panes.pending_expanded = !app.panes.pending_expanded;
        }
        if let Some(aci) = open_pending {
            app.panes.pending_person = Some(aci);
        }
        // Desktop's CallLinkPendingParticipantModal: who's asking, then
        // Approve entry or Deny entry.
        if let Some(aci) = app.panes.pending_person.clone() {
            let name = app.name(&aci);
            let p = app.conversation(&ConversationId::direct(&aci)).cloned();
            let mut answer = None;
            let closed = crate::panes::modal(ui, "pending-person", 320., |ui| {
                ui.vertical_centered(|ui| {
                    let spec = match &p {
                        Some(p) => widgets::AvatarSpec::of(p),
                        None => widgets::AvatarSpec::person(&name, 0),
                    };
                    widgets::avatar(ui, &spec, 80.);
                    ui.add_space(10.);
                    ui.label(RichText::new(&name).font(theme::semibold(18.)));
                    if let Some(phone) = p.as_ref().and_then(|p| p.phone.clone()) {
                        ui.label(RichText::new(phone).font(theme::regular(13.)).color(palette.text_secondary));
                    }
                    ui.add_space(14.);
                    if widgets::pill_button(ui, tr("Approve entry"), true).clicked() {
                        answer = Some(true);
                    }
                    ui.add_space(6.);
                    if widgets::pill_button(ui, tr("Deny entry"), false).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if let Some(approve) = answer {
                act.push((aci, approve));
                app.panes.pending_person = None;
            } else if closed {
                app.panes.pending_person = None;
            }
        }
        for (aci, approve) in act {
            app.send(Command::ApproveCallJoin { aci, approve });
        }
        if let Some(approve) = app.panes.pending_all {
            use crate::dialog::{Answer, Variant};
            let (title, body) = if approve {
                (format!("Approve {n} {}?", if n == 1 { "request" } else { "requests" }), format!("{n} {} will be added to the call.", if n == 1 { "person" } else { "people" }))
            } else {
                (format!("Deny {n} {}?", if n == 1 { "request" } else { "requests" }), format!("{n} {} will not be added to the call.", if n == 1 { "person" } else { "people" }))
            };
            let label = if approve { tr("Approve all") } else { tr("Deny all") };
            match crate::dialog::show(ui.ctx(), egui::Id::unique("pending-all"), &title, &body, Some(tr("Cancel")), &[(label, if approve { Variant::StrongPrimary } else { Variant::StrongDestructive })]) {
                Some(Answer::Action(_)) => {
                    app.panes.pending_all = None;
                    for aci in &pending {
                        app.send(Command::ApproveCallJoin { aci: aci.clone(), approve });
                    }
                    if approve {
                        app.toast(format!("{n} {} added to the call", if n == 1 { "person" } else { "people" }));
                    }
                }
                Some(Answer::Cancel) => app.panes.pending_all = None,
                None => {}
            }
        }
    }
    if call.link.as_ref().is_some_and(|l| l.waiting) {
        ui.painter().text(area.center() + Vec2::new(0., 80.), egui::Align2::CENTER_CENTER, tr("Waiting to be let in…"), theme::medium(15.), white);
    }
}
