//! Desktop's Settings > Backups for a linked device
//! (`ts/components/PreferencesBackups.dom.tsx`,
//! `PreferencesBackupDetails.dom.tsx`), the left pane's media download
//! banner (`BackupMediaDownloadProgress.dom.tsx`) and its settings variant
//! (`BackupMediaDownloadProgressSettings.dom.tsx`), with the cancel
//! confirmation (`BackupMediaDownloadCancelConfirmationDialog.dom.tsx`).
//! Desktop's "Desktop backups" (local backups) row is left out: langis
//! doesn't make local backups.

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::Command;
use langis_core::backups::{BackupMediaDownload, BackupMediaDownloadAction as Action, BackupStatus, BackupTier, Cost, Subscription};

use crate::app::App;
use crate::dialog::{self, Choice, Variant};
use crate::i18n::{Arg, tf, tr};
use crate::theme::{self, Palette};
use crate::widgets::{self, Axo};

/// Desktop's `SIGNAL_BACKUPS_LEARN_MORE_URL`.
const LEARN_MORE: &str = "https://support.signal.org/hc/articles/360007059752-Backup-and-Restore-Messages";

#[derive(Default)]
pub struct BackupsView {
    /// From `Event::BackupStatus`.
    pub status: Option<BackupStatus>,
    /// From `Event::BackupMediaDownload`.
    pub download: Option<BackupMediaDownload>,
    /// "Cancel media syncing?" is open.
    confirm_cancel: bool,
    /// The settings page last drawn and on which frame, to refresh on
    /// each visit as Desktop does.
    shown: Option<(bool, u64)>,
}

// ------------------------------------------------------------------ helpers

/// Desktop's `formatFileSize`: `filesize` in JEDEC units (1 KB = 1024),
/// whole numbers under 1 MB (10^6 bytes) and one decimal from there.
pub fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    if bytes < 1_000_000 {
        format!("{} {}", value.round() as u64, UNITS[unit])
    } else {
        let text = format!("{value:.1}");
        format!("{} {}", text.strip_suffix(".0").unwrap_or(&text), UNITS[unit])
    }
}

/// `Intl.NumberFormat` with `currencyDisplay: 'narrowSymbol'` in English,
/// for the currencies Signal bills most; others by code.
pub fn format_cost(cost: &Cost) -> String {
    let code = cost.currency.to_uppercase();
    let whole = ["BIF", "CLP", "DJF", "GNF", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "VND", "VUV", "XAF", "XOF", "XPF"].contains(&code.as_str());
    let amount = if whole { format!("{:.0}", cost.amount) } else { format!("{:.2}", cost.amount) };
    let symbol = match code.as_str() {
        "USD" | "CAD" | "AUD" | "NZD" | "MXN" | "SGD" | "HKD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" | "CNY" => "¥",
        "INR" => "₹",
        "KRW" => "₩",
        "BRL" => "R$",
        "CHF" => "CHF ",
        _ => return format!("{code} {amount}"),
    };
    format!("{symbol}{amount}")
}

/// `formatTimestamp` with `dateStyle: 'medium'`.
fn date(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|t| crate::format::localized(&t.with_timezone(&chrono::Local), "%b %-d, %Y"))
        .unwrap_or_default()
}

/// And with `timeStyle: 'short'`.
fn date_time(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|t| crate::format::localized(&t.with_timezone(&chrono::Local), "%b %-d, %Y, %-I:%M %p"))
        .unwrap_or_default()
}

fn free_plan(days: u32) -> String {
    let english = format!("Text + {days} {} media backup", if days == 1 { "day" } else { "days" });
    tf("icu:Preferences--backup-messages-plan__description", &english, &[("mediaDayCount", Arg::from(days))])
}

/// `renderSubscriptionDetails`: lines, the first red for a canceled one.
fn subscription_lines(subscription: &Subscription) -> Vec<(String, bool)> {
    match subscription {
        Subscription::Active { cost, renews_at } => {
            let mut lines = Vec::new();
            if let Some(cost) = cost {
                let cost = format_cost(cost);
                lines.push((tf("icu:Preferences--backup-subscription-monthly-cost", &format!("{cost} / month"), &[("cost", Arg::from(&cost))]), false));
            }
            if let Some(at) = renews_at {
                let date = date(*at);
                lines.push((tf("icu:Preferences--backup-plan__renewal-date", &format!("Renews {date}"), &[("date", Arg::from(&date))]), false));
            }
            lines
        }
        Subscription::PendingCancellation { expires_at, .. } => {
            let mut lines = vec![(tr("Subscription canceled").to_owned(), true)];
            if let Some(at) = expires_at {
                let date = date(*at);
                lines.push((tf("icu:Preferences--backup-plan__expiry-date", &format!("Expires {date}"), &[("date", Arg::from(&date))]), false));
            }
            lines
        }
        Subscription::NotFound | Subscription::Expired => {
            vec![(tr("Your subscription was not found. Check your phone to view your backup details.").to_owned(), false)]
        }
    }
}

/// `getSubscriptionNote`.
fn subscription_note(subscription: Option<&Subscription>) -> &'static str {
    match subscription {
        Some(Subscription::Active { .. } | Subscription::PendingCancellation { .. }) => {
            tr("You can manage or cancel your Signal Secure Backups subscription on your phone.")
        }
        _ => tr("You can manage or renew your Signal Secure Backups subscription on your phone."),
    }
}

fn secondary(ui: &mut Ui, text: &str, size: f32) {
    let palette = Palette::of(ui.ctx());
    ui.label(RichText::new(text).font(theme::regular(size)).color(palette.text_secondary));
}

/// The subscription lines, or Desktop's spinner over stale data.
fn subscription_details(ui: &mut Ui, status: &BackupStatus) {
    let palette = Palette::of(ui.ctx());
    if status.show_spinner(chrono::Utc::now().timestamp_millis()) {
        widgets::spinner(ui, 24.);
        return;
    }
    let subscription = status.subscription.clone().unwrap_or(Subscription::NotFound);
    for (line, canceled) in subscription_lines(&subscription) {
        if canceled {
            ui.label(RichText::new(line).font(theme::semibold(14.)).color(palette.destructive));
        } else {
            ui.label(RichText::new(line).font(theme::regular(13.)).color(palette.text));
        }
    }
}

// --------------------------------------------------------------------- page

/// Asks for fresh status when a backups page is first drawn after not
/// being drawn (Desktop's `useEffect` on the page).
fn refresh_on_visit(app: &mut App, ui: &Ui, details: bool) {
    let frame = ui.ctx().cumulative_frame_nr();
    let fresh = app.backups.shown.is_none_or(|(d, at)| d != details || at + 1 < frame);
    app.backups.shown = Some((details, frame));
    if fresh {
        app.send(Command::RefreshBackupStatus { details });
    }
}

/// Settings > Backups. Returns true when Manage was clicked (go to the
/// details page).
pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    refresh_on_visit(app, ui, false);
    let palette = Palette::of(ui.ctx());
    let mut manage = false;
    ui.add_space(8.);
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 12 }).show(ui, |ui| {
        ui.label(
            RichText::new(tr("Back up your message history so you never lose data when you get a new phone or reinstall Signal."))
                .font(theme::regular(14.))
                .color(palette.text_secondary),
        );
    });
    let status = app.backups.status.clone();
    let tier = status.as_ref().and_then(|s| s.tier);
    crate::axo_list::list(ui, None, None, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 12)).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0., 2.);
                let (icon, _) = ui.allocate_exact_size(Vec2::new(30., 20.), Sense::hover());
                theme::paint_symbol(ui.painter(), "backup", Pos2::new(icon.left() + 9., icon.center().y), 18., palette.text_secondary);
                let button_w = if tier.is_some() { 110. } else { 0. };
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - button_w).max(120.));
                    ui.label(RichText::new(tr("Signal Secure Backups")).font(theme::regular(14.)).color(palette.text));
                    match (tier, &status) {
                        (None, _) | (_, None) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                secondary(ui, tr("Automatic backups with Signal\u{2019}s secure, end-to-end encrypted storage service. Get started on your phone. "), 13.);
                                if ui.add(egui::Label::new(RichText::new(tr("Learn more")).font(theme::regular(13.)).color(palette.accent_text)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    crate::chat::open_externally(LEARN_MORE);
                                }
                                secondary(ui, ".", 13.);
                            });
                        }
                        (Some(BackupTier::Paid), Some(status)) => {
                            secondary(ui, tr("Text + all media backup"), 13.);
                            ui.add_space(6.);
                            subscription_details(ui, status);
                        }
                        (Some(BackupTier::Free), Some(status)) => {
                            secondary(ui, &free_plan(status.free_media_days), 13.);
                            ui.add_space(6.);
                            ui.label(RichText::new(tr("Your backup plan is free")).font(theme::regular(13.)).color(palette.text));
                        }
                    }
                });
                if tier.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if widgets::axo_button(ui, tr("Manage"), Axo::SubtleSecondary).clicked() {
                            manage = true;
                        }
                    });
                }
            });
        });
    });
    manage
}

/// Desktop's `BackupsDetailsPage`: the plan's summary, and the last backup
/// and the media download under "Backup details".
pub fn details_page(app: &mut App, ui: &mut Ui) {
    refresh_on_visit(app, ui, true);
    let palette = Palette::of(ui.ctx());
    let Some(status) = app.backups.status.clone() else {
        widgets::spinner(ui, 24.);
        return;
    };
    // `Preferences--backups-summary__container`: gray-02 / gray-80.
    let fill = if palette.dark { theme::rgb(0x2e2e2e) } else { theme::rgb(0xf6f6f6) };
    ui.add_space(8.);
    egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(16, 20)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::new(0., 2.);
        let (plan, active, note) = match status.tier {
            Some(BackupTier::Paid) => {
                let active = matches!(status.subscription, Some(Subscription::Active { .. }));
                (tr("Text + all media backup").to_owned(), active, subscription_note(status.subscription.as_ref()))
            }
            _ => (free_plan(status.free_media_days), true, tr("You can manage or upgrade Signal Secure Backups on your phone.")),
        };
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 56.).max(120.));
                secondary(ui, &plan, 13.);
                ui.add_space(8.);
                if status.tier == Some(BackupTier::Paid) {
                    subscription_details(ui, &status);
                } else {
                    ui.label(RichText::new(tr("Your backup plan is free")).font(theme::regular(13.)).color(palette.text));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                plan_icon(ui, active);
            });
        });
        ui.add_space(12.);
        secondary(ui, note, 13.);
    });

    let download = app.backups.download.filter(|d| d.total_bytes > 0 && d.completed_bytes < d.total_bytes);
    if status.last_backup.is_none() && download.is_none() {
        return;
    }
    ui.add_space(30.);
    ui.label(RichText::new(tr("Backup details")).font(theme::semibold(14.)).color(palette.text));
    ui.add_space(10.);
    if let Some(backup) = status.last_backup.filter(|b| b.created_at > 0) {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 10)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0., 2.);
            ui.label(RichText::new(tr("Last backup")).font(theme::regular(14.)).color(palette.text));
            secondary(ui, &format!("{} \u{2022} {}", tr("Your phone"), date_time(backup.created_at)), 13.);
        });
    }
    if let Some(download) = download {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 10)).show(ui, |ui| settings_progress(app, ui, download));
    }
}

/// `Preferences--backups-summary__icon`: Desktop's 48px backups logo,
/// subscribed or not. Drawn: the backup symbol in a disc, accent when
/// active.
fn plan_icon(ui: &mut Ui, active: bool) {
    let palette = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.), Sense::hover());
    let (disc, symbol) = if active { (palette.badge, Color32::WHITE) } else { (palette.fill_secondary, palette.text_secondary) };
    ui.painter().circle_filled(rect.center(), 24., disc);
    theme::paint_symbol(ui.painter(), "backup", rect.center(), 26., symbol);
    if active {
        let at = rect.right_bottom() - Vec2::splat(9.);
        ui.painter().circle_filled(at, 9., palette.background);
        theme::paint_symbol(ui.painter(), "check-circle-fill", at, 18., palette.green);
    }
}

/// `BackupMediaDownloadProgressSettings`.
fn settings_progress(app: &mut App, ui: &mut Ui, download: BackupMediaDownload) {
    let palette = Palette::of(ui.ctx());
    // `roundFractionForProgressBar`: never shows 0 or 1 while it's neither.
    let fraction = (download.completed_bytes as f64 / download.total_bytes as f64).clamp(0.01, 0.99) as f32;
    let (title, description) = if download.paused {
        let resume = tr("Resume");
        (
            tr("Syncing paused").to_owned(),
            tf("icu:BackupMediaDownloadProgressSettings__paused--description", &format!("Click \u{201c}{resume}\u{201d} to continue syncing"), &[("resumeButtonText", Arg::from(resume))]),
        )
    } else {
        let (current, total) = (file_size(download.completed_bytes), file_size(download.total_bytes));
        let percent = format!("{}%", (fraction * 100.).round() as u32);
        (
            tr("Syncing media").to_owned(),
            tf(
                "icu:BackupMediaDownloadProgressSettings__progressbar-hint",
                &format!("{current} of {total} ({percent})..."),
                &[("currentSize", Arg::from(&current)), ("totalSize", Arg::from(&total)), ("fractionComplete", Arg::from(&percent))],
            ),
        )
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0., 5.);
        let buttons = 200.;
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - buttons - 20.).max(120.));
            ui.label(RichText::new(title).font(theme::regular(14.)).color(palette.text));
            progress_bar(ui, fraction);
            ui.label(RichText::new(description).font(theme::regular(12.)).color(palette.text_secondary));
        });
        ui.add_space(20.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.;
            if widgets::axo_button(ui, tr("Cancel"), Axo::SubtleDestructive).clicked() {
                app.backups.confirm_cancel = true;
            }
            let (label, action) = if download.paused { (tr("Resume"), Action::Resume) } else { (tr("Pause"), Action::Pause) };
            if widgets::axo_button(ui, label, Axo::SubtleSecondary).clicked() {
                app.send(Command::BackupMediaDownload(action));
            }
        });
    });
}

/// Desktop's `ProgressBar`: a 4px track with the accent over it.
fn progress_bar(ui: &mut Ui, fraction: f32) {
    let palette = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), palette.fill_secondary);
    let done = Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height()));
    ui.painter().rect_filled(done, CornerRadius::same(2), palette.badge);
}

/// Desktop's determinate `SpinnerV2` (`variant="brand"`): a faint ring and
/// the part done in the accent, from the top.
fn progress_ring(ui: &Ui, center: Pos2, fraction: f32, color: Color32) {
    let (size, stroke) = (24., 3.);
    let radius = size / 2. - stroke / 2.;
    ui.painter().circle_stroke(center, radius, Stroke::new(stroke, color.gamma_multiply(0.2)));
    let steps = (48. * fraction).ceil().max(1.) as usize;
    let start = -std::f32::consts::FRAC_PI_2;
    let points: Vec<Pos2> =
        (0..=steps).map(|i| center + radius * Vec2::angled(start + i as f32 / steps as f32 * std::f32::consts::TAU * fraction)).collect();
    ui.painter().add(egui::Shape::line(points, Stroke::new(stroke, color)));
}

// ------------------------------------------------------------------- banner

/// `BackupMediaDownloadProgress` in the left pane: Desktop's info dialog,
/// a card under the network dialogs, above the chats.
pub fn banner(app: &mut App, ui: &mut Ui) {
    let Some(download) = app.backups.download.filter(|d| d.show_banner()) else { return };
    let palette = Palette::of(ui.ctx());
    let online = app.demo || !matches!(app.connection, langis_core::ConnectionState::Offline);
    let complete = download.complete();
    let fraction = (download.completed_bytes as f64 / download.total_bytes as f64).clamp(0., 1.) as f32;
    let (current, total) = (file_size(download.completed_bytes), file_size(download.total_bytes));
    // What it says, and whether Resume shows in the text.
    let (title, description, resume_link) = if complete {
        (tr("Restore complete").to_owned(), Some(current.clone()), false)
    } else if download.paused {
        (tr("Syncing paused").to_owned(), None, true)
    } else if download.idle {
        let title = tf("icu:BackupMediaDownloadProgress__title-idle", &format!("{current} of {total} restored"), &[("currentSize", Arg::from(&current)), ("totalSize", Arg::from(&total))]);
        (title, Some(tr("Media will continue to download in the background").to_owned()), false)
    } else if !online {
        (tr("Restoring media paused").to_owned(), Some(tr("Waiting for Internet connection...").to_owned()), false)
    } else {
        let hint = tf("icu:BackupMediaDownloadProgress__progressbar-hint", &format!("{current} of {total}"), &[("currentSize", Arg::from(&current)), ("totalSize", Arg::from(&total))]);
        (tr("Syncing media").to_owned(), Some(hint), false)
    };
    let closable = complete || download.idle;
    let (fill, border) = if palette.dark { (theme::rgb(0x2e2e2e), theme::rgb(0x5e5e5e)) } else { (Color32::WHITE, theme::rgb(0xe9e9e9)) };
    let mut action = None;
    let mut open_menu = None;
    ui.horizontal(|ui| {
        ui.add_space(10.);
        let frame = egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1., border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin { left: 16, right: 14, top: 12, bottom: 12 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width() - 10.);
                ui.set_min_height(40.);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    let (icon, _) = ui.allocate_exact_size(Vec2::splat(24.), Sense::hover());
                    if complete {
                        theme::paint_symbol(ui.painter(), "check-circle", icon.center(), 24., palette.badge);
                    } else if download.idle {
                        theme::paint_symbol(ui.painter(), "backup", icon.center(), 24., palette.badge);
                    } else {
                        progress_ring(ui, icon.center(), fraction, palette.badge);
                    }
                    ui.add_space(18.);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 20.).max(60.));
                        ui.spacing_mut().item_spacing.y = 2.;
                        ui.label(RichText::new(&title).font(theme::semibold(13.)).color(palette.text));
                        if let Some(description) = &description {
                            let color = if palette.dark { theme::rgb(0xbebebe) } else { theme::rgb(0x5e5e5e).gamma_multiply(0.8) };
                            ui.label(RichText::new(description).font(theme::regular(12.)).color(color));
                        }
                        if resume_link
                            && ui
                                .add(egui::Label::new(RichText::new(tr("Resume")).font(theme::semibold(12.)).color(palette.accent_text)).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                        {
                            action = Some(Action::Resume);
                        }
                    });
                });
            });
        let card = frame.response.rect;
        if closable {
            // `BackupMediaDownloadProgress__button-close`: 20px, 12px in.
            let close = Rect::from_center_size(Pos2::new(card.right() - 12. - 10., card.top() + 12. + 10.), Vec2::splat(20.));
            let r = ui.interact(close, egui::Id::unique("backup-banner-close"), Sense::click()).on_hover_text(tr("Close"));
            theme::paint_symbol(ui.painter(), "x", close.center(), 20., if r.hovered() { palette.text } else { palette.text_secondary });
            if r.clicked() {
                action = Some(Action::Dismiss);
            }
        } else {
            // `BackupMediaDownloadProgress__more`: a small icon button, 4px
            // from the top and 10px from the end, opening Pause or Resume
            // and Cancel syncing.
            let more = Rect::from_min_size(Pos2::new(card.right() - 10. - 28., card.top() + 4.), Vec2::splat(28.));
            let r = ui.interact(more, egui::Id::unique("backup-banner-more"), Sense::click()).on_hover_text(tr("More options"));
            if r.hovered() {
                ui.painter().circle_filled(more.center(), 14., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "more", more.center(), 20., palette.text_secondary);
            open_menu = Some(r);
        }
    });
    if let Some(trigger) = open_menu {
        let mut chosen = None;
        crate::menu::dropdown(ui, &trigger, egui::Id::unique("backup-banner-menu"), |menu| {
            if download.paused {
                if menu.item(None, tr("Resume")) {
                    chosen = Some(Action::Resume);
                }
            } else if menu.item(None, tr("Pause")) {
                chosen = Some(Action::Pause);
            }
            if menu.item(None, tr("Cancel syncing")) {
                chosen = Some(Action::Cancel);
            }
        });
        action = action.or(chosen);
    }
    match action {
        Some(Action::Cancel) => app.backups.confirm_cancel = true,
        Some(action) => app.send(Command::BackupMediaDownload(action)),
        None => {}
    }
    ui.add_space(6.);
}

/// `BackupMediaDownloadCancelConfirmationDialog`, from either place.
pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    if !app.backups.confirm_cancel {
        return;
    }
    let answer = dialog::confirm(
        ctx,
        egui::Id::unique("backup-media-cancel"),
        tr("Cancel media syncing?"),
        tr("Your media has not completed syncing. If you choose to cancel, you can sync again by re-linking this device. Learn more."),
        tr("Cancel syncing"),
        Variant::StrongDestructive,
    );
    match answer {
        Some(Choice::Action) => {
            app.send(Command::BackupMediaDownload(Action::Cancel));
            app.backups.confirm_cancel = false;
        }
        Some(Choice::Cancel) => app.backups.confirm_cancel = false,
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_costs_read_as_desktops() {
        assert_eq!(file_size(512), "512 B");
        assert_eq!(file_size(500 * 1024), "500 KB");
        assert_eq!(file_size(1_027_604_480), "980 MB");
        assert_eq!(file_size(2_285_895_680), "2.1 GB");
        assert_eq!(format_cost(&Cost { amount: 1.99, currency: "USD".into() }), "$1.99");
        assert_eq!(format_cost(&Cost { amount: 300., currency: "JPY".into() }), "¥300");
        assert_eq!(format_cost(&Cost { amount: 2.49, currency: "EUR".into() }), "€2.49");
        assert_eq!(format_cost(&Cost { amount: 25., currency: "SEK".into() }), "SEK 25.00");
    }

    #[test]
    fn subscriptions_say_what_desktop_says() {
        let lines = subscription_lines(&Subscription::Active { cost: Some(Cost { amount: 1.99, currency: "USD".into() }), renews_at: None });
        assert_eq!(lines, vec![("$1.99 / month".to_owned(), false)]);
        let lines = subscription_lines(&Subscription::PendingCancellation { cost: None, expires_at: None });
        assert_eq!(lines, vec![("Subscription canceled".to_owned(), true)]);
        assert_eq!(subscription_lines(&Subscription::Expired).len(), 1);
        assert_eq!(free_plan(1), "Text + 1 day media backup");
        assert_eq!(free_plan(45), "Text + 45 days media backup");
    }
}
