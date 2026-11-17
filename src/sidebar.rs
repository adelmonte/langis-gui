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
    if app.nav_collapsed {
        return;
    }
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
            nav_toggle(app, ui, &palette);
            let chats = tab_item(ui, &palette, if app.tab == Tab::Chats { Icon::ChatFill } else { Icon::Chat }, app.tab == Tab::Chats, unread);
            if chats.on_hover_text("Chats").clicked() {
                app.tab = Tab::Chats;
            }
            let calls = tab_item(ui, &palette, if app.tab == Tab::Calls { Icon::PhoneFill } else { Icon::Phone }, app.tab == Tab::Calls, 0);
            if calls.on_hover_text("Calls").clicked() {
                app.tab = Tab::Calls;
                app.panes.calls_loaded = false;
            }
            let unseen = crate::stories_unread(app);
            let stories = tab_item(ui, &palette, if app.tab == Tab::Stories { Icon::StoriesFill } else { Icon::Stories }, app.tab == Tab::Stories, unseen);
            if stories.on_hover_text("Stories").clicked() {
                app.tab = Tab::Stories;
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
                    let spec = widgets::AvatarSpec { name: &app.me.0, color: 0, photo: app.me.1.as_deref(), group: false, note_to_self: false };
                    if widgets::avatar(ui, &spec, 28.).on_hover_text("Profile").clicked() {
                        crate::profile::open(app);
                    }
                });
            });
        });
}

/// Desktop's `NavTabsToggle`: the menu button atop the nav tabs, or, with
/// them collapsed, before a list's title with the other tabs' unread count.
fn nav_toggle(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let label = if app.nav_collapsed { "Show tabs" } else { "Hide tabs" };
    let (outer, response) = if app.nav_collapsed {
        ui.allocate_exact_size(Vec2::new(36., 36.), Sense::click())
    } else {
        ui.allocate_exact_size(Vec2::new(80., 36.), Sense::click())
    };
    let rect = Rect::from_center_size(outer.center(), Vec2::splat(28.));
    if response.hovered() {
        let fill = if response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    }
    Icon::Menu.paint(ui, rect, 20., palette.text);
    if app.nav_collapsed {
        let chats: u32 = if app.tab == Tab::Chats { 0 } else { app.conversations.iter().filter(|c| !c.muted).map(|c| c.unread_count + c.marked_unread as u32).sum() };
        let stories = if app.tab == Tab::Stories { 0 } else { crate::stories_unread(app) };
        badge(ui, palette, rect, chats + stories);
    }
    if response.on_hover_text(label).clicked() {
        app.nav_collapsed = !app.nav_collapsed;
        crate::settings::write(app, crate::settings::SETTING_NAV_TABS_COLLAPSED, app.nav_collapsed.to_string());
    }
}

/// Where a list's title starts: Desktop's header padding, or the nav toggle
/// when the tabs are collapsed.
pub fn header_start(app: &mut App, ui: &mut Ui) {
    if app.nav_collapsed {
        ui.add_space(10.);
        nav_toggle(app, ui, &Palette::of(ui.ctx()));
        ui.add_space(2.);
    } else {
        ui.add_space(24.);
    }
}

/// A red count over an icon's top right.
fn badge(ui: &Ui, palette: &Palette, rect: Rect, count: u32) {
    if count == 0 {
        return;
    }
    let text = if count > 99 { "99+".to_owned() } else { count.to_string() };
    let galley = ui.painter().layout_no_wrap(text, theme::regular(10.), egui::Color32::WHITE);
    let w = (galley.size().x + 4.).max(16.);
    let at = Rect::from_min_size(Pos2::new(rect.center().x + 10. - 6. + 4., rect.center().y - 10. - 4.), Vec2::new(w, 16.));
    ui.painter().rect_filled(at, CornerRadius::same(8), palette.destructive);
    ui.painter().galley(at.center() - galley.size() / 2., galley, egui::Color32::WHITE);
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
    self::badge(ui, palette, rect, badge);
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
                if app.panes.archived {
                    ui.add_space(12.);
                    if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back").clicked() {
                        app.panes.archived = false;
                    }
                    ui.label(RichText::new("Archived chats").font(theme::semibold(18.)));
                    return;
                }
                header_start(app, ui);
                ui.label(RichText::new("Chats").font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.);
                    let more = widgets::icon_button(ui, Icon::More, 20., "More");
                    egui::Popup::menu(&more).show(|ui| crate::panes::list_menu(app, ui));
                    if widgets::icon_button(ui, Icon::Edit, 20., "New chat").clicked() {
                        app.panes.compose = Some(crate::panes::Compose::Chat { query: String::new(), searching: false, found: None });
                    }
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
            crate::panes::list_notices(app, ui, &palette);
            if !app.panes.archived {
                crate::panes::folder_tabs(app, ui, &palette);
            }

            let query = app.search.trim().to_lowercase();
            let rows: Vec<Conversation> = app
                .conversations
                .iter()
                .filter(|c| crate::panes::visible(app, c) && (query.is_empty() || c.title.to_lowercase().contains(&query)))
                .cloned()
                .collect();
            let mut open = None;
            let mut open_at = None;
            crate::search::follow_global(app, ui.ctx());
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.add_space(4.);
                for c in &rows {
                    let typing = app.typing.get(&c.id).is_some_and(|t| !t.is_empty());
                    let selected = app.selected.as_ref() == Some(&c.id);
                    let row = chat_row(ui, &palette, c, selected, typing);
                    if row.clicked() {
                        open = Some(c.id.clone());
                    }
                    crate::menu::context(ui, &row, egui::Id::unique("chat-list-menu").with(&c.id.0), |m| crate::panes::chat_menu(app, m, c));
                }
                // Desktop's "Archived chats (n)" at the end of the list.
                let archived = app.conversations.iter().filter(|c| c.archived).count();
                if !app.panes.archived && archived > 0 && query.is_empty() {
                    ui.add_space(4.);
                    ui.horizontal(|ui| {
                        ui.add_space(14.);
                        if crate::panes::menu_row(ui, Some(Icon::Archive), &format!("Archived chats ({archived})"), false).clicked() {
                            app.panes.archived = true;
                        }
                    });
                }
                if !query.is_empty() {
                    open_at = crate::search::message_results(app, ui, &palette);
                }
                if rows.is_empty() && app.message_search.hits.is_empty() && !query.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("No results for \u{201c}{}\u{201d}", app.search.trim())).color(palette.text_secondary));
                    });
                }
            });
            if let Some(id) = open {
                app.panes.details = None;
                app.open(id);
            }
            if let Some((conversation, message_id)) = open_at {
                app.open_at(conversation, message_id);
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
        preview_right = at.right() - 1. - 14. * icon.aspect() - 6.;
    }
    let mut job = egui::text::LayoutJob::default();
    let body = |color| egui::TextFormat { font_id: theme::regular(13.), color, line_height: Some(18.), ..Default::default() };
    if typing {
        job.append("typing…", 0., egui::TextFormat { font_id: theme::italic(13.), ..body(palette.text_secondary) });
    } else if let Some(draft) = c.draft.as_deref().filter(|d| !d.is_empty()) {
        job.append("Draft: ", 0., egui::TextFormat { font_id: theme::italic(13.), ..body(palette.text_secondary) });
        crate::chat::append_with_emoji(&mut job, &crate::format::one_line(draft), body(palette.text_secondary));
    } else {
        let preview = c.last_message_preview.as_deref().map(crate::format::one_line).unwrap_or_default();
        crate::chat::append_with_emoji(&mut job, &preview, body(palette.text_secondary));
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
