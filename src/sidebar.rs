//! Desktop's nav tabs (the 80px column) and chat list (`NavSidebar` with
//! `AxoChatItem` rows).

use egui::{CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use langis_core::ConnectionState;
use langis_store::{Conversation, DeliveryStatus};

use crate::app::{App, Tab};
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

pub fn nav_tabs(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let unread: u32 = app.conversations.iter().filter(|c| !c.muted).map(|c| c.unread_count + c.marked_unread as u32).sum();
    egui::Panel::left("nav-tabs")
        .exact_size(80.)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.sidebar))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1., palette.border));
            ui.add_space(12.);
            let chats = tab_item(ui, &palette, if app.tab == Tab::Chats { Icon::ChatFill } else { Icon::Chat }, app.tab == Tab::Chats, unread);
            if chats.on_hover_text("Chats").clicked() {
                app.tab = Tab::Chats;
            }
            // Settings and our avatar sit at the bottom.
            let bottom = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - 100.), full.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
                let selected = app.tab == Tab::Settings;
                let settings = tab_item(ui, &palette, if selected { Icon::SettingsFill } else { Icon::Settings }, selected, 0);
                if settings.on_hover_text("Settings").clicked() {
                    app.tab = Tab::Settings;
                    app.settings.stale = true;
                }
                ui.add_space(6.);
                ui.vertical_centered(|ui| {
                    let me = app.conversations.iter().find(|c| c.kind == langis_store::ConversationKind::NoteToSelf);
                    let (name, photo) = me.map(|c| (c.title.as_str(), c.avatar_path.as_deref())).unwrap_or(("", None));
                    let spec = widgets::AvatarSpec { name, color: 0, photo, group: false, note_to_self: false };
                    widgets::avatar(ui, &spec, 28.).on_hover_text("Profile");
                });
            });
        });
}

/// One nav tab: a 20px icon in a rounded 60x40 button.
fn tab_item(ui: &mut Ui, palette: &Palette, icon: Icon, selected: bool, badge: u32) -> egui::Response {
    let (outer, response) = ui.allocate_exact_size(Vec2::new(80., 44.), Sense::click());
    let rect = Rect::from_center_size(outer.center(), Vec2::new(60., 40.));
    if selected || response.hovered() {
        let fill = if selected || response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    }
    icon.paint(ui, rect, 20., palette.text);
    if badge > 0 {
        let text = if badge > 99 { "99+".to_owned() } else { badge.to_string() };
        let galley = ui.painter().layout_no_wrap(text, theme::regular(10.), egui::Color32::WHITE);
        let w = (galley.size().x + 4.).max(16.);
        let at = Rect::from_min_size(Pos2::new(rect.center().x + 10. - 6. + 4., rect.center().y - 10. - 4.), Vec2::new(w, 16.));
        ui.painter().rect_filled(at, CornerRadius::same(8), palette.destructive);
        ui.painter().galley(at.center() - galley.size() / 2., galley, egui::Color32::WHITE);
    }
    response
}

pub fn chat_list(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    egui::Panel::left("chat-list")
        .resizable(true)
        .default_size(300.)
        .min_size(240.)
        .max_size(420.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.sidebar))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1., palette.border));
            // Header: "Chats", and the compose button.
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add_space(24.);
                ui.label(RichText::new("Chats").font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.);
                    widgets::icon_button(ui, Icon::More, 20., "More");
                    widgets::icon_button(ui, Icon::Edit, 20., "New chat");
                });
            });
            ui.add_space(6.);
            ui.horizontal(|ui| {
                ui.add_space(16.);
                search_field(ui, &palette, &mut app.search);
                ui.add_space(16.);
            });
            ui.add_space(6.);
            status_banner(app, ui, &palette);

            let query = app.search.trim().to_lowercase();
            let rows: Vec<Conversation> = app
                .conversations
                .iter()
                .filter(|c| !c.archived && (query.is_empty() || c.title.to_lowercase().contains(&query)))
                .cloned()
                .collect();
            let mut open = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.add_space(4.);
                for c in &rows {
                    let typing = app.typing.get(&c.id).is_some_and(|t| !t.is_empty());
                    let selected = app.selected.as_ref() == Some(&c.id);
                    if chat_row(ui, &palette, c, selected, typing).clicked() {
                        open = Some(c.id.clone());
                    }
                }
                if rows.is_empty() && !query.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("No results for \u{201c}{}\u{201d}", app.search.trim())).color(palette.text_secondary));
                    });
                }
            });
            if let Some(id) = open {
                app.open(id);
            }
        });
}

/// Desktop's search input: a magnifier and a hint on a rounded fill.
fn search_field(ui: &mut Ui, palette: &Palette, text: &mut String) {
    let width = ui.available_width() - 16.;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
    Icon::SearchCompactLight.paint(ui, Rect::from_min_size(rect.min, Vec2::new(32., 32.)), 16., palette.text_secondary);
    let field = Rect::from_min_max(rect.min + Vec2::new(30., 7.), rect.max - Vec2::new(8., 5.));
    ui.scope_builder(egui::UiBuilder::new().max_rect(field), |ui| {
        ui.add(egui::TextEdit::singleline(text).hint_text("Search").frame(egui::Frame::NONE).desired_width(field.width()));
    });
}

/// Connection and history transfer, as Desktop's left pane dialogs.
fn status_banner(app: &App, ui: &mut Ui, palette: &Palette) {
    use langis_core::HistoryProgress as H;
    let text = match (&app.connection, &app.history) {
        (_, Some(H::Waiting)) => Some("Waiting for your phone to send your message history…".to_owned()),
        (_, Some(H::Downloading { done, total })) => Some(match total {
            Some(total) if *total > 0 => format!("Downloading message history: {}%", done * 100 / total),
            _ => format!("Downloading message history: {}", crate::format::file_size(*done)),
        }),
        (_, Some(H::Importing { done, total })) if *total > 0 => Some(format!("Importing message history: {}%", done * 100 / total)),
        (_, Some(H::Failed(why))) => Some(format!("Message history transfer failed: {why}")),
        (ConnectionState::Offline, _) if !app.demo => Some("Offline. Check your connection.".to_owned()),
        (ConnectionState::Connecting, _) => Some("Connecting…".to_owned()),
        (ConnectionState::Syncing, _) => Some("Loading messages…".to_owned()),
        _ => None,
    };
    let Some(text) = text else { return };
    ui.horizontal(|ui| {
        ui.add_space(16.);
        egui::Frame::new()
            .fill(palette.fill)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width() - 16.);
                ui.label(RichText::new(text).font(theme::regular(13.)));
            });
    });
    ui.add_space(6.);
}

/// One chat: 68px, avatar 48, title and time over two lines of preview.
fn chat_row(ui: &mut Ui, palette: &Palette, c: &Conversation, selected: bool, typing: bool) -> egui::Response {
    let width = ui.available_width();
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, 72.), Sense::click());
    let rect = Rect::from_min_max(outer.min + Vec2::new(10., 2.), outer.max - Vec2::new(10., 2.));
    if selected || response.hovered() {
        let fill = if selected || response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
        ui.painter().rect_filled(rect, CornerRadius::same(16), fill);
    }
    let inner = rect.shrink2(Vec2::new(14., 10.));
    let avatar = Rect::from_min_size(inner.min, Vec2::splat(48.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));

    let left = avatar.right() + 12.;
    let right = inner.right();
    let painter = ui.painter_at(rect);
    // Top line: the time on the right, the title in what's left.
    let time = painter.layout_no_wrap(crate::format::short_timestamp(c.last_message_at), theme::regular(11.), palette.text_secondary);
    let time_pos = Pos2::new(right - time.size().x, inner.top() + 2.);
    let title_max = (time_pos.x - 6. - left).max(20.);
    let mut title_job = egui::text::LayoutJob::simple_singleline(c.title.clone(), theme::semibold(14.), palette.text);
    title_job.wrap = egui::text::TextWrapping::truncate_at_width(title_max - if c.muted { 18. } else { 0. });
    let title = ui.fonts_mut(|f| f.layout_job(title_job));
    let title_w = title.size().x;
    painter.galley(Pos2::new(left, inner.top()), title, palette.text);
    if c.muted {
        Icon::BellSlashFill.paint(ui, Rect::from_min_size(Pos2::new(left + title_w + 4., inner.top() + 2.), Vec2::splat(14.)), 12., palette.text_secondary);
    }
    painter.galley(time_pos, time, palette.text_secondary);

    // Bottom: the preview (two lines), then the status or the count.
    let mut preview_right = right;
    let unread = c.unread_count + c.marked_unread as u32;
    if unread > 0 {
        let badge = widgets::count_badge(ui, Pos2::new(right, inner.top() + 30.), c.unread_count.max(1), c.muted);
        preview_right = badge.left() - 10.;
    } else if let Some(status) = c.last_outgoing_status.filter(|_| c.draft.as_deref().is_none_or(str::is_empty) && !typing) {
        let icon = status_icon(status);
        let color = if status == DeliveryStatus::Failed { palette.destructive } else { palette.text_secondary };
        let at = Rect::from_min_size(Pos2::new(right - 16., inner.top() + 22.), Vec2::splat(16.));
        icon.paint(ui, at, 14., color);
        preview_right = at.left() - 6.;
    }
    let mut job = egui::text::LayoutJob::default();
    let body = |color| egui::TextFormat { font_id: theme::regular(13.), color, line_height: Some(18.), ..Default::default() };
    if typing {
        job.append("typing…", 0., egui::TextFormat { font_id: theme::italic(13.), ..body(palette.text_secondary) });
    } else if let Some(draft) = c.draft.as_deref().filter(|d| !d.is_empty()) {
        job.append("Draft: ", 0., egui::TextFormat { font_id: theme::italic(13.), ..body(palette.text_secondary) });
        job.append(&crate::format::one_line(draft), 0., body(palette.text_secondary));
    } else {
        let preview = c.last_message_preview.as_deref().map(crate::format::one_line).unwrap_or_default();
        job.append(&preview, 0., body(palette.text_secondary));
    }
    job.wrap = egui::text::TextWrapping { max_width: (preview_right - left).max(20.), max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
    let preview = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(Pos2::new(left, inner.top() + 20.), preview, palette.text_secondary);
    if response.has_focus() {
        painter.rect_stroke(rect, CornerRadius::same(16), Stroke::new(2., palette.accent), StrokeKind::Inside);
    }
    response
}

pub fn status_icon(status: DeliveryStatus) -> Icon {
    match status {
        DeliveryStatus::Sending | DeliveryStatus::Paused => Icon::MessagestatusSending,
        DeliveryStatus::Sent | DeliveryStatus::PartialSent => Icon::MessagestatusSent,
        DeliveryStatus::Delivered => Icon::MessagestatusDelivered,
        DeliveryStatus::Read | DeliveryStatus::Viewed => Icon::MessagestatusRead,
        DeliveryStatus::Failed => Icon::ErrorCircleCompact,
    }
}
