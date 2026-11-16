//! Desktop's Preferences: sections on the left, their settings on the
//! right.

use egui::{CornerRadius, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::auto_download::{AutoDownload, SETTING_AUTO_DOWNLOAD};
use langis_core::{AccountSetting, Command, DeviceKind, ShareQuality};

use crate::app::{App, Loaded};
use crate::theme::{self, Palette};
use crate::widgets;

/// Ours: whether to show desktop notifications (on unless "false").
pub const SETTING_NOTIFICATIONS: &str = "ui.notifications";
/// Ours: what notifications show: "all", "name" or "none".
pub const SETTING_NOTIFICATION_CONTENT: &str = "ui.notification_content";

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Appearance,
    Chats,
    Calls,
    Notifications,
    Privacy,
    DataUsage,
}

impl Section {
    const ALL: [Section; 6] = [Self::Appearance, Self::Chats, Self::Calls, Self::Notifications, Self::Privacy, Self::DataUsage];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Chats => "Chats",
            Self::Calls => "Calls",
            Self::Notifications => "Notifications",
            Self::Privacy => "Privacy",
            Self::DataUsage => "Data usage",
        }
    }
}

/// The stored values the page shows.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub account: Vec<(AccountSetting, bool)>,
    pub auto_download: AutoDownload,
    pub incoming_calls: bool,
    pub always_relay: bool,
    pub notifications: bool,
    pub notification_content: String,
}

#[derive(Default)]
pub struct SettingsView {
    pub section: Section,
    pub snapshot: Option<Snapshot>,
    /// Something changed elsewhere: read the values again.
    pub stale: bool,
    custom_quality: Option<ShareQuality>,
}

fn load(app: &App) {
    app.load(|db| async move {
        let mut account = Vec::new();
        for setting in AccountSetting::ALL {
            let value = db.setting(setting.key()).await.ok().flatten();
            account.push((setting, setting.is_on(value.as_deref())));
        }
        let get = |key: &'static str| {
            let db = db.clone();
            async move { db.setting(key).await.ok().flatten() }
        };
        let auto_download = AutoDownload::from_setting(get(SETTING_AUTO_DOWNLOAD).await.as_deref());
        let snapshot = Snapshot {
            account,
            auto_download,
            incoming_calls: get(langis_core::SETTING_INCOMING_CALLS).await.as_deref() != Some("false"),
            always_relay: langis_core::setting_on(get(langis_core::SETTING_ALWAYS_RELAY_CALLS).await.as_deref()),
            notifications: get(SETTING_NOTIFICATIONS).await.as_deref() != Some("false"),
            notification_content: get(SETTING_NOTIFICATION_CONTENT).await.unwrap_or_else(|| "all".into()),
        };
        Some(Loaded::Settings(Box::new(snapshot)))
    });
}

/// Writes one of the app's own settings (or a local-only one the core reads
/// as it goes; see CORE-WISHLIST.md).
fn write(app: &App, key: &'static str, value: String) {
    app.load(move |db| async move {
        let _ = db.set_setting(key, &value).await;
        None
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    if app.settings.stale || app.settings.snapshot.is_none() {
        app.settings.stale = false;
        load(app);
    }
    egui::Panel::left("settings-sections")
        .exact_size(300.)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.sidebar))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1., palette.border));
            ui.add_space(14.);
            ui.horizontal(|ui| {
                ui.add_space(24.);
                ui.label(RichText::new("Settings").font(theme::semibold(18.)));
            });
            ui.add_space(14.);
            for section in Section::ALL {
                let (outer, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.), Sense::click());
                let rect = outer.shrink2(Vec2::new(10., 2.));
                let selected = app.settings.section == section;
                if selected || response.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(12), if selected { palette.fill_pressed } else { palette.fill });
                }
                ui.painter().text(rect.left_center() + Vec2::new(14., 0.), egui::Align2::LEFT_CENTER, section.label(), theme::regular(14.), palette.text);
                if response.clicked() {
                    app.settings.section = section;
                }
            }
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 32, right: 32, top: 20, bottom: 32 }).show(ui, |ui| {
                ui.set_max_width(640.);
                ui.label(RichText::new(app.settings.section.label()).font(theme::semibold(20.)));
                ui.add_space(16.);
                let Some(snapshot) = app.settings.snapshot.clone() else {
                    ui.spinner();
                    return;
                };
                match app.settings.section {
                    Section::Appearance => appearance(app, ui, &palette),
                    Section::Chats => chats(app, ui, &palette, &snapshot),
                    Section::Calls => calls(app, ui, &palette, &snapshot),
                    Section::Notifications => notifications(app, ui, &palette, &snapshot),
                    Section::Privacy => privacy(app, ui, &palette, &snapshot),
                    Section::DataUsage => data_usage(app, ui, &palette, &snapshot),
                }
            });
        });
    });
}

/// A setting's row: its name (and a line on what it does), then the
/// control on the right.
fn row(ui: &mut Ui, palette: &Palette, title: &str, detail: Option<&str>, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(44.);
        let text_width = (ui.available_width() - 300.).max(160.);
        ui.vertical(|ui| {
            ui.set_width(text_width);
            ui.label(RichText::new(title).font(theme::regular(14.)));
            if let Some(detail) = detail {
                ui.label(RichText::new(detail).font(theme::regular(12.)).color(palette.text_secondary));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(4.);
}

fn heading(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.add_space(12.);
    ui.label(RichText::new(text).font(theme::semibold(14.)).color(palette.text_secondary));
    ui.add_space(4.);
}

fn account_switch(app: &App, ui: &mut Ui, palette: &Palette, snapshot: &Snapshot, setting: AccountSetting, title: &str, detail: Option<&str>) {
    let mut on = snapshot.account.iter().find(|(s, _)| *s == setting).map(|(_, on)| *on).unwrap_or(setting.default_on());
    row(ui, palette, title, detail, |ui| {
        if widgets::switch(ui, &mut on).changed() {
            app.send(Command::SetAccountSetting { setting, on });
        }
    });
}

fn appearance(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let mut theme = app.theme.clone();
    row(ui, palette, "Theme", None, |ui| {
        egui::ComboBox::from_id_salt("theme")
            .selected_text(match theme.as_str() {
                "light" => "Light",
                "dark" => "Dark",
                _ => "System",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut theme, "system".to_owned(), "System");
                ui.selectable_value(&mut theme, "light".to_owned(), "Light");
                ui.selectable_value(&mut theme, "dark".to_owned(), "Dark");
            });
    });
    if theme != app.theme {
        app.theme = theme.clone();
        app.apply_theme();
        write(app, langis_core::SETTING_THEME, theme);
    }
}

fn chats(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    account_switch(app, ui, palette, s, AccountSetting::LinkPreviews, "Generate link previews", Some("Retrieve link previews directly from websites for messages you send."));
    account_switch(app, ui, palette, s, AccountSetting::PreferContactAvatars, "Use address book photos", Some("Display photos from your address book if available."));
    account_switch(app, ui, palette, s, AccountSetting::KeepMutedChatsArchived, "Keep muted chats archived", None);
}

fn calls(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    let mut incoming = s.incoming_calls;
    row(ui, palette, "Enable incoming calls", None, |ui| {
        if widgets::switch(ui, &mut incoming).changed() {
            write(app, langis_core::SETTING_INCOMING_CALLS, incoming.to_string());
            app.settings.stale = true;
        }
    });
    let mut relay = s.always_relay;
    row(ui, palette, "Always relay calls", Some("Relay all calls through the Signal server to avoid revealing your IP address to your contact."), |ui| {
        if widgets::switch(ui, &mut relay).changed() {
            write(app, langis_core::SETTING_ALWAYS_RELAY_CALLS, relay.to_string());
            app.settings.stale = true;
        }
    });

    heading(ui, palette, "Devices");
    let devices = app.call_devices.clone();
    let lists = [
        (DeviceKind::Camera, "Camera", &devices.devices.cameras, &devices.camera),
        (DeviceKind::Microphone, "Microphone", &devices.devices.microphones, &devices.microphone),
        (DeviceKind::Speaker, "Speakers", &devices.devices.speakers, &devices.speaker),
    ];
    for (kind, title, list, chosen) in lists {
        let mut pick: Option<String> = None;
        row(ui, palette, title, None, |ui| {
            let current = chosen.as_ref().and_then(|id| list.iter().find(|d| &d.id == id)).map(|d| d.name.clone()).unwrap_or_else(|| "Default".into());
            egui::ComboBox::from_id_salt(title).width(220.).selected_text(current).show_ui(ui, |ui| {
                for d in list.iter() {
                    if ui.selectable_label(chosen.as_ref() == Some(&d.id), &d.name).clicked() {
                        pick = Some(d.id.clone());
                    }
                }
                if list.is_empty() {
                    ui.label("None found");
                }
            });
        });
        if let Some(id) = pick {
            app.send(Command::SetCallDevice { kind, id });
            app.send(Command::LoadCallDevices);
        }
    }

    heading(ui, palette, "Screen sharing");
    let current = devices.share_quality;
    let preset = ShareQuality::PRESETS.iter().find(|(_, q)| *q == current).map(|(n, _)| *n);
    let custom = app.settings.custom_quality.is_some() || preset.is_none();
    let mut choice: Option<ShareQuality> = None;
    row(ui, palette, "Quality", Some("Higher looks sharper and smoother, and uses more bandwidth and processor."), |ui| {
        let label = if custom { "Custom".to_owned() } else { preset.unwrap_or("Custom").to_owned() };
        egui::ComboBox::from_id_salt("share-quality").width(260.).selected_text(label).show_ui(ui, |ui| {
            for (name, q) in ShareQuality::PRESETS {
                if ui.selectable_label(!custom && q == current, name).clicked() {
                    choice = Some(q);
                }
            }
            if ui.selectable_label(custom, "Custom").clicked() {
                app.settings.custom_quality = Some(current);
            }
        });
    });
    if let Some(q) = choice {
        app.settings.custom_quality = None;
        set_quality(app, q);
    }
    if let Some(mut q) = app.settings.custom_quality.or(custom.then_some(current)) {
        let before = q;
        let mut native = q.max_height == 0;
        row(ui, palette, "Resolution", Some("The tallest the shared picture is sent; full keeps the screen's own size."), |ui| {
            ui.checkbox(&mut native, "Full");
            if !native {
                if q.max_height == 0 {
                    q.max_height = 1080;
                }
                ui.add(egui::Slider::new(&mut q.max_height, 144..=2160).suffix("p").step_by(2.));
            }
        });
        if native {
            q.max_height = 0;
        }
        row(ui, palette, "Frame rate", None, |ui| {
            ui.add(egui::Slider::new(&mut q.fps, 1..=60).suffix(" fps"));
        });
        app.settings.custom_quality = Some(q);
        let released = !ui.input(|i| i.pointer.any_down());
        if q != before || (q != current && released) {
            if released {
                set_quality(app, q);
            }
        }
    }
}

fn set_quality(app: &mut App, q: ShareQuality) {
    app.send(Command::SetShareQuality(q));
    app.call_devices.share_quality = q;
}

fn notifications(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    let mut on = s.notifications;
    row(ui, palette, "Notifications", Some("Show a desktop notification when a message arrives."), |ui| {
        if widgets::switch(ui, &mut on).changed() {
            write(app, SETTING_NOTIFICATIONS, on.to_string());
            app.settings.stale = true;
            app.notify.0 = on;
        }
    });
    let mut content = s.notification_content.clone();
    row(ui, palette, "Notification content", None, |ui| {
        let label = match content.as_str() {
            "name" => "Name only",
            "none" => "No name or content",
            _ => "Name, content, and actions",
        };
        egui::ComboBox::from_id_salt("notification-content").width(220.).selected_text(label).show_ui(ui, |ui| {
            ui.selectable_value(&mut content, "all".to_owned(), "Name, content, and actions");
            ui.selectable_value(&mut content, "name".to_owned(), "Name only");
            ui.selectable_value(&mut content, "none".to_owned(), "No name or content");
        });
    });
    if content != s.notification_content {
        app.notify.1 = content.clone();
        write(app, SETTING_NOTIFICATION_CONTENT, content);
        app.settings.stale = true;
    }
    account_switch(app, ui, palette, s, AccountSetting::ReactionNotifications, "Notify for reactions", None);
    account_switch(app, ui, palette, s, AccountSetting::NotifyMentionsIfMuted, "Notify for mentions in muted chats", None);
    account_switch(app, ui, palette, s, AccountSetting::NotifyCallsIfMuted, "Notify for calls in muted chats", None);
}

fn privacy(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    account_switch(app, ui, palette, s, AccountSetting::ReadReceipts, "Read receipts", Some("If read receipts are disabled, you won't be able to see read receipts from others."));
    account_switch(app, ui, palette, s, AccountSetting::TypingIndicators, "Typing indicators", Some("If typing indicators are disabled, you won't be able to see typing indicators from others."));
    account_switch(app, ui, palette, s, AccountSetting::SealedSenderIndicators, "Show sealed sender indicators", None);
    heading(ui, palette, "Database");
    ui.label(
        RichText::new(format!("Your messages are kept encrypted in {}.", app.data_dir.display()))
            .font(theme::regular(13.))
            .color(palette.text_secondary),
    );
}

fn data_usage(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    let mut auto = s.auto_download.clone();
    heading(ui, palette, "Media auto-download");
    let mut changed = false;
    for (title, on) in [("Photos", &mut auto.photos), ("Videos", &mut auto.videos), ("Audio", &mut auto.audio), ("Documents", &mut auto.documents)] {
        row(ui, palette, title, None, |ui| {
            changed |= widgets::switch(ui, on).changed();
        });
    }
    if changed {
        app.send(Command::SetAutoDownload(auto.clone()));
        if let Some(snapshot) = app.settings.snapshot.as_mut() {
            snapshot.auto_download = auto;
        }
    }
}
