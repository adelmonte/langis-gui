//! Desktop's Preferences (`ts/components/Preferences.dom.tsx`): the
//! profile card and the pages on the left, each page an `AxoPanel` of
//! `PreferencesList`s on the right. Rows for what langis doesn't do (spell
//! check, sounds, badges, donations, backups, the language) are left out.

use egui::RichText;
use egui::{Align2, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use langis_core::auto_download::{AutoDownload, SETTING_AUTO_DOWNLOAD};
use langis_core::{AccountSetting, ChatChange, Command, DeviceKind, ShareQuality};
use langis_store::ConversationKind;

use crate::app::{App, Loaded};
use crate::axo_list::{self, End, Item};
use crate::dialog::{self, Choice, Variant};
use crate::theme::{self, Palette};
use crate::widgets;

/// Ours: whether to show desktop notifications (on unless "false").
pub const SETTING_NOTIFICATIONS: &str = "ui.notifications";
pub const SETTING_AUTO_CONVERT_EMOJI: &str = "ui.auto_convert_emoji";
pub const SETTING_TEXT_FORMATTING: &str = "ui.text_formatting";
pub const SETTING_SKIN_TONE: &str = "ui.skin_tone";
/// Ours: what notifications show: "all", "name" or "none".
pub const SETTING_NOTIFICATION_CONTENT: &str = "ui.notification_content";
/// Desktop's `navTabsCollapsed`: the nav tabs hidden, "true" or unset.
pub const SETTING_NAV_TABS_COLLAPSED: &str = "ui.nav_tabs_collapsed";
/// Desktop's `zoomFactor`: the window's zoom, "1" when unset.
pub const SETTING_ZOOM: &str = "ui.zoom";

/// Desktop's `DEFAULT_ZOOM_FACTORS`.
const ZOOMS: [(f32, &str); 7] = [(0.75, "75%"), (0.9, "90%"), (1., "100%"), (1.1, "110%"), (1.25, "125%"), (1.5, "150%"), (2., "200%")];

/// Desktop's `SettingsPage`s that langis has.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Page {
    /// Desktop's ProfileEditor, from the profile chip, and its name and
    /// About editors.
    Profile,
    ProfileName,
    ProfileAbout,
    Account,
    #[default]
    General,
    Appearance,
    ChatColor,
    Chats,
    ChatFolders,
    /// Desktop's EditChatFolderPage, for `SettingsView::folder`.
    ChatFolderEdit,
    Calls,
    Notifications,
    WhileMuted,
    Privacy,
    PhoneNumber,
    Blocked,
    DataUsage,
}

impl Page {
    /// The nav button a page lights up, and its symbol and label.
    const NAV: [(Page, &'static str, &'static str); 8] = [
        (Page::Account, "person-circle", "Account"),
        (Page::General, "settings", "General"),
        (Page::Appearance, "appearance", "Appearance"),
        (Page::Chats, "message", "Chats"),
        (Page::Calls, "phone", "Calls"),
        (Page::Notifications, "bell", "Notifications"),
        (Page::Privacy, "lock", "Privacy"),
        (Page::DataUsage, "piechart", "Data usage"),
    ];

    fn top(self) -> Page {
        match self {
            Page::ChatColor => Page::Appearance,
            Page::ChatFolders => Page::Chats,
            Page::ChatFolderEdit => Page::ChatFolders,
            Page::WhileMuted => Page::Notifications,
            Page::PhoneNumber | Page::Blocked => Page::Privacy,
            Page::ProfileName | Page::ProfileAbout => Page::Profile,
            other => other,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Profile => "Profile",
            Page::ProfileName => "Your name",
            Page::ProfileAbout => "About",
            Page::ChatColor => "Chat Color",
            Page::ChatFolders => "Chat folders",
            Page::ChatFolderEdit => "Edit folder",
            Page::WhileMuted => "While muted",
            Page::PhoneNumber => "Phone Number",
            Page::Blocked => "Blocked",
            other => Page::NAV.iter().find(|(p, _, _)| *p == other).map(|(_, _, l)| *l).unwrap_or(""),
        }
    }
}

/// The stored values the pages show.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub account: Vec<(AccountSetting, bool)>,
    pub auto_download: AutoDownload,
    pub incoming_calls: bool,
    pub always_relay: bool,
    pub notifications: bool,
    pub notification_content: String,
    pub phone_sharing: bool,
    pub phone_discoverable: bool,
    pub key_transparency_disabled: bool,
    pub universal_timer: u32,
    pub contacts_synced_at: Option<i64>,
    pub stories_disabled: bool,
    pub username: Option<String>,
    /// langis's image optimization (`max`, `fast` or `off`) and Giphy key.
    pub optimization: String,
    pub giphy_key: String,
}

/// A question the page is asking.
#[derive(Clone, PartialEq, Debug)]
enum Ask {
    DeleteData,
    LinkedDevice,
    StoriesOff,
    NobodyCanFind,
    Unblock(langis_store::ConversationId, String, bool),
    DeleteFolder(String, String),
    ResetNotifications,
}

/// A folder as the editor holds it: its id, not new, and what it holds.
pub fn folder_edit(f: &langis_store::ChatFolder) -> (String, bool, langis_store::ChatFolderParams) {
    let params = langis_store::ChatFolderParams {
        name: f.name.clone(),
        show_only_unread: f.show_only_unread,
        show_muted_chats: f.show_muted_chats,
        include_all_individual_chats: f.include_all_individual_chats,
        include_all_group_chats: f.include_all_group_chats,
        included: f.included.clone(),
        excluded: f.excluded.clone(),
    };
    (f.id.clone(), false, params)
}

#[derive(Default)]
pub struct SettingsView {
    pub page: Page,
    pub snapshot: Option<Snapshot>,
    /// Something changed elsewhere: read the values again.
    pub stale: bool,
    custom_quality: Option<ShareQuality>,
    /// A chat folder being made or changed: its id (new or existing) and
    /// what it holds.
    pub folder: Option<(String, bool, langis_store::ChatFolderParams)>,
    ask: Option<Ask>,
    /// Desktop's Add blocked user: its search, while choosing.
    block_chooser: Option<String>,
    /// Desktop's Help menu windows: Debug Log, About.
    pub help: Option<Help>,
    /// The folder page's Add chats (false) or Exclude chats (true) dialog:
    /// its search and what's chosen, with the chat types.
    folder_chats: Option<FolderChats>,
    /// Back from the folder page, asking about unsaved changes first.
    folder_leaving: bool,
    /// The emoji picker for the folder's name.
    folder_emoji: Option<crate::emoji_picker::Picker>,
}

struct FolderChats {
    exclude: bool,
    query: String,
    chosen: Vec<langis_store::ConversationId>,
    individual: bool,
    groups: bool,
}

pub enum Help {
    /// Desktop's `DebugLogWindow`: the log once assembled, then, after
    /// Submit, its link (or why it failed).
    DebugLog { log: Option<String>, uploading: bool, link: Option<Result<String, String>> },
    About,
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
            phone_sharing: langis_core::setting_on(get(langis_core::SETTING_PHONE_NUMBER_SHARING).await.as_deref()),
            phone_discoverable: get(langis_core::SETTING_PHONE_NUMBER_DISCOVERABLE).await.as_deref() != Some("false"),
            key_transparency_disabled: langis_core::setting_on(get(langis_core::SETTING_KEY_TRANSPARENCY_DISABLED).await.as_deref()),
            universal_timer: get(langis_core::SETTING_UNIVERSAL_EXPIRE_TIMER).await.and_then(|v| v.parse().ok()).unwrap_or(0),
            contacts_synced_at: get(langis_core::SETTING_CONTACTS_SYNCED_AT).await.and_then(|v| v.parse().ok()),
            stories_disabled: langis_core::setting_on(get(langis_core::SETTING_STORIES_DISABLED).await.as_deref()),
            username: None,
            optimization: get(langis_core::SETTING_IMAGE_OPTIMIZATION).await.unwrap_or_else(|| "max".into()),
            giphy_key: get(langis_core::SETTING_GIPHY_API_KEY).await.unwrap_or_default(),
        };
        Some(Loaded::Settings(Box::new(snapshot)))
    });
    let aci = app.aci.clone();
    app.load(move |db| async move {
        let username = langis_core::own_profile(&db, &aci).await.username;
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(s) = app.settings.snapshot.as_mut() {
                s.username = username;
            }
        })))
    });
}

/// Writes one of the app's own settings (`ui.` keys; the core's go
/// through commands).
pub fn write(app: &App, key: &'static str, value: String) {
    app.load(move |db| async move {
        let _ = db.set_setting(key, &value).await;
        None
    });
}

/// The saved zoom, applied once the account is open.
pub fn load_zoom(app: &App) {
    app.load(|db| async move {
        let zoom: f32 = db.setting(SETTING_ZOOM).await.ok().flatten().and_then(|z| z.parse().ok()).unwrap_or(1.);
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if zoom > 0.2 {
                app.ctx.set_zoom_factor(zoom);
            }
        })))
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    if app.settings.stale || app.settings.snapshot.is_none() {
        app.settings.stale = false;
        load(app);
    }
    egui::Panel::left("settings-pages")
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
                crate::sidebar::header_start(app, ui);
                ui.label(egui::RichText::new("Settings").font(theme::semibold(18.)));
            });
            ui.add_space(14.);
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 11, right: 11, top: 2, bottom: 12 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    nav(app, ui, &palette);
                });
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(4.);
        // A folder opened for editing (here or from the chat list) shows
        // its page; done with it, back to the list.
        if app.settings.folder.is_some() && app.settings.page != Page::ChatFolderEdit {
            app.settings.page = Page::ChatFolderEdit;
        } else if app.settings.folder.is_none() && app.settings.page == Page::ChatFolderEdit {
            app.settings.page = Page::ChatFolders;
        }
        let page = app.settings.page;
        let title = match (&app.settings.folder, page) {
            (Some((_, true, _)), Page::ChatFolderEdit) => "Create a folder",
            _ => page.title(),
        };
        if axo_list::panel_header(ui, title, page != page.top()) {
            if page == Page::ChatFolderEdit {
                app.settings.folder_leaving = true;
            } else {
                app.settings.page = page.top();
            }
        }
        let Some(snapshot) = app.settings.snapshot.clone() else {
            ui.centered_and_justified(|ui| crate::widgets::spinner(ui, 24.));
            return;
        };
        egui::ScrollArea::vertical().id_salt(("settings-page", page as u8)).auto_shrink(false).show(ui, |ui| {
            axo_list::container(ui, |ui| match page {
                Page::Profile => crate::profile::page(app, ui),
                Page::ProfileName => crate::profile::name_page(app, ui),
                Page::ProfileAbout => crate::profile::about_page(app, ui),
                Page::Account => account(app, ui),
                Page::General => general(app, ui),
                Page::Appearance => appearance(app, ui),
                Page::ChatColor => crate::chat_colors::picker(app, ui, None),
                Page::Chats => chats(app, ui, &snapshot),
                Page::ChatFolders => chat_folders(app, ui),
                Page::ChatFolderEdit => folder_page(app, ui),
                Page::Calls => calls(app, ui, &snapshot),
                Page::Notifications => notifications(app, ui, &snapshot),
                Page::WhileMuted => while_muted(app, ui, &snapshot),
                Page::Privacy => privacy(app, ui, &snapshot),
                Page::PhoneNumber => phone_number(app, ui, &palette, &snapshot),
                Page::Blocked => blocked(app, ui, &palette),
                Page::DataUsage => data_usage(app, ui, &snapshot),
            });
        });
    });
    ask(app, ui.ctx());
    folder_dialogs(app, ui);
    help_window(app, ui);
}

/// The app's version, for About and the debug log.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Opens the Debug Log window and assembles the log off the UI thread:
/// langis's `debug_log::assemble`, with phone numbers, ids and our own
/// paths redacted, as Desktop's.
fn open_debug_log(app: &mut App) {
    app.settings.help = Some(Help::DebugLog { log: None, uploading: false, link: None });
    let dir = langis_core::debug_log::log_dir(&app.data_dir);
    let mut sensitive = vec![app.data_dir.clone()];
    sensitive.extend(dirs::home_dir());
    let mut user = Vec::new();
    if let Some(phone) = &app.phone {
        user.push(("e164".to_owned(), phone.clone()));
    }
    if !app.aci.is_empty() {
        user.push(("uuid".to_owned(), app.aci.clone()));
    }
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_owned())))
        .unwrap_or_else(|| std::env::consts::OS.to_owned());
    let header = langis_core::debug_log::Header { app_version: VERSION.into(), os_version: os, arch: std::env::consts::ARCH.into(), user };
    app.load(move |_| async move {
        // Three days of info lines: small enough to read here.
        let log = langis_core::debug_log::assemble(&dir, &header, &sensitive);
        Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(Help::DebugLog { log: slot, .. }) = app.settings.help.as_mut() {
                *slot = Some(log);
            }
        })))
    });
}

fn help_window(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let close;
    let mut submit = None;
    let mut save = None;
    match app.settings.help.as_mut() {
        None => return,
        Some(Help::About) => {
            close = crate::panes::modal(ui, "about", 340., |ui| {
                crate::panes::title(ui, "About langis-gui");
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new(egui::include_image!("../assets/app/langis-256.png")).fit_to_exact_size(egui::vec2(96., 96.)));
                    ui.add_space(8.);
                    ui.label(RichText::new("langis-gui").font(theme::semibold(18.)));
                    ui.label(RichText::new(format!("Version {VERSION}")).color(palette.text_secondary));
                    ui.add_space(8.);
                    ui.label(RichText::new("An unofficial desktop client, not affiliated with Signal.").font(theme::regular(13.)).color(palette.text_secondary));
                    ui.label(RichText::new("AGPL-3.0. Icons and sounds from Signal Desktop (AGPL-3.0); Inter (OFL).").font(theme::regular(12.)).color(palette.text_secondary));
                    ui.add_space(8.);
                    if ui.link("github.com/adelmonte/langis-gui").clicked() {
                        crate::chat::open_externally("https://github.com/adelmonte/langis-gui");
                    }
                });
            });
        }
        Some(Help::DebugLog { log, uploading, link }) => {
            close = crate::panes::modal(ui, "debug-log", 560., |ui| {
                crate::panes::title(ui, "Debug Log");
                match link {
                    Some(Ok(url)) => {
                        // Desktop's submitted state: the link and Copy Link.
                        ui.label(RichText::new("Debug log submitted").font(theme::semibold(14.)));
                        ui.add_space(8.);
                        let mut shown = url.clone();
                        ui.add(egui::TextEdit::singleline(&mut shown).desired_width(f32::INFINITY));
                        ui.add_space(12.);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::pill_button(ui, "Copy Link", true).clicked() {
                                ui.ctx().copy_text(url.clone());
                                submit = Some(String::new());
                            }
                        });
                    }
                    _ => {
                        ui.label(RichText::new("When you click Submit, your log will be posted online for 30 days at a unique, unpublished URL. You may Save it locally first.").font(theme::regular(13.)).color(palette.text_secondary));
                        if let Some(Err(why)) = link {
                            ui.add_space(6.);
                            ui.colored_label(palette.destructive, "Something went wrong with the upload! Please email support@signal.org and attach your log as a text file.").on_hover_text(why.as_str());
                        }
                        ui.add_space(10.);
                        match log {
                            None => {
                                ui.vertical_centered(|ui| crate::widgets::spinner(ui, 24.));
                            }
                            Some(text) => {
                                // Desktop shows the start of it, read-only.
                                let mut preview: String = text.chars().take(20_000).collect();
                                egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut preview).font(egui::FontId::monospace(11.)).desired_width(f32::INFINITY).interactive(true));
                                });
                                ui.add_space(12.);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if *uploading {
                                        crate::widgets::spinner(ui, 24.);
                                    } else if widgets::pill_button(ui, "Submit", true).clicked() {
                                        submit = Some(text.clone());
                                    }
                                    if widgets::pill_button(ui, "Save", false).clicked() {
                                        save = Some(text.clone());
                                    }
                                });
                            }
                        }
                    }
                }
            });
        }
    }
    if let Some(text) = save
        && let Some(path) = rfd::FileDialog::new().set_file_name(format!("debuglog-{}.txt", chrono::Local::now().format("%Y-%m-%d-%H%M%S"))).save_file()
    {
        match std::fs::write(&path, text) {
            Ok(()) => app.toast(format!("Saved to {}", path.display())),
            Err(error) => app.toast(format!("Couldn't save: {error}")),
        }
    }
    if let Some(text) = submit {
        if text.is_empty() {
            app.toast("Link Copied to Your Clipboard");
            app.settings.help = None;
            return;
        }
        if let Some(Help::DebugLog { uploading, .. }) = app.settings.help.as_mut() {
            *uploading = true;
        }
        app.load(move |_| async move {
            let result = langis_core::debug_log::upload(&text, VERSION).await.map_err(|e| e.to_string());
            Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| {
                if let Some(Help::DebugLog { uploading, link, .. }) = app.settings.help.as_mut() {
                    *uploading = false;
                    *link = Some(result);
                }
            })))
        });
    }
    if close {
        app.settings.help = None;
    }
}

/// The profile card, then a button per page (`PreferencesButton`).
fn nav(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let width = ui.available_width();
    let username = app.settings.snapshot.as_ref().and_then(|s| s.username.clone());
    let painter = ui.painter().clone();
    let name = painter.layout(app.me.0.clone(), theme::semibold(14.), palette.text, width - 20. - 58. - 46.);
    let mut lines = vec![];
    if let Some(phone) = &app.phone {
        lines.push(painter.layout_no_wrap(langis_core::share_contact::format_phone(phone, Some(phone)), theme::regular(12.), palette.text_secondary));
    }
    if let Some(u) = &username {
        lines.push(painter.layout_no_wrap(u.clone(), theme::regular(12.), palette.text_secondary));
    }
    let text_h = name.size().y + lines.iter().map(|l| 2. + l.size().y).sum::<f32>();
    let h = 14. + text_h.max(48.) + 14.;
    let (rect, chip) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    if chip.hovered() {
        painter.rect_filled(rect, CornerRadius::same(8), if chip.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill });
    }
    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 10., rect.center().y - 24.), Vec2::splat(48.));
    let spec = widgets::AvatarSpec { name: &app.me.0, color: 0, photo: app.me.1.as_deref(), group: false, note_to_self: false };
    widgets::paint_avatar(ui, avatar, &spec);
    let mut y = rect.center().y - text_h / 2.;
    let x = avatar.right() + 10.;
    let name_h = name.size().y;
    painter.galley(Pos2::new(x, y), name, palette.text);
    y += name_h;
    for line in lines {
        y += 2.;
        let lh = line.size().y;
        painter.galley(Pos2::new(x, y), line, palette.text_secondary);
        y += lh;
    }
    let mut qr_clicked = false;
    if username.is_some() {
        let qr = Rect::from_center_size(Pos2::new(rect.right() - 10. - 18., rect.center().y), Vec2::splat(36.));
        let r = ui.interact(qr, egui::Id::unique("settings").with("profile-qr"), Sense::click());
        painter.circle_filled(qr.center(), 18., if r.hovered() { palette.fill_secondary } else { palette.fill });
        theme::paint_symbol(&painter, "qrcode", qr.center(), 20., palette.text);
        qr_clicked = r.on_hover_text("Open username link screen").clicked();
    }
    if qr_clicked {
        crate::profile::open_link_screen(app);
    } else if chip.on_hover_text("Edit profile").clicked() {
        app.settings.page = Page::Profile;
        crate::profile::open(app);
    }
    ui.add_space(4.);
    for (i, (page, symbol, label)) in Page::NAV.iter().enumerate() {
        if i == 1 {
            // Desktop's divider after the account buttons.
            let (r, _) = ui.allocate_exact_size(Vec2::new(width, 17.), Sense::hover());
            painter.hline(r.left() + 14.0..=r.right() - 14., r.top() + 8.5, Stroke::new(1., palette.border));
        }
        ui.add_space(2.);
        let (r, response) = ui.allocate_exact_size(Vec2::new(width, 40.), Sense::click());
        ui.add_space(2.);
        let current = app.settings.page.top() == *page;
        if current || response.hovered() {
            let fill = if current || response.is_pointer_button_down_on() { palette.fill_pressed } else { palette.fill };
            painter.rect_filled(r, CornerRadius::same(14), fill);
        }
        theme::paint_symbol(&painter, symbol, Pos2::new(r.left() + 18. + 9., r.center().y), 18., palette.text);
        painter.text(Pos2::new(r.left() + 18. + 18. + 12., r.center().y), Align2::LEFT_CENTER, *label, theme::regular(14.), palette.text);
        if response.clicked() {
            app.settings.page = *page;
            app.settings.stale = true;
        }
    }
}

fn account_on(s: &Snapshot, setting: AccountSetting) -> bool {
    s.account.iter().find(|(x, _)| *x == setting).map(|(_, on)| *on).unwrap_or(setting.default_on())
}

fn account_switch(app: &mut App, ui: &mut Ui, s: &Snapshot, setting: AccountSetting, item: Item<'_>) {
    let mut on = account_on(s, setting);
    if axo_list::switch(ui, item, &mut on) {
        app.send(Command::SetAccountSetting { setting, on });
        if let Some(snapshot) = app.settings.snapshot.as_mut()
            && let Some(entry) = snapshot.account.iter_mut().find(|(x, _)| *x == setting)
        {
            entry.1 = on;
        }
    }
}

fn account(app: &mut App, ui: &mut Ui) {
    axo_list::list(ui, None, None, |ui| {
        let item = Item::new("This is a linked device").symbol("device-laptop").description("To manage your account settings, open Signal on your primary device.");
        if axo_list::item(ui, item, End::Arrow(None)).clicked() {
            app.settings.ask = Some(Ask::LinkedDevice);
        }
    });
}

fn general(app: &mut App, ui: &mut Ui) {
    let footer = "To change the name of this device, open Signal on your phone and navigate to Settings > Linked devices";
    axo_list::list(ui, None, Some(footer), |ui| {
        if let Some(phone) = &app.phone {
            axo_list::item(ui, Item::new("Phone Number"), End::Value(&langis_core::share_contact::format_phone(phone, Some(phone))));
        }
        axo_list::item(ui, Item::new("Device Name"), End::Value(&crate::app::device_name()));
    });
    // Desktop's Open at login and Start minimized, as an XDG autostart
    // entry (~/.config/autostart/langis.desktop).
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        let entry = autostart_path();
        let current = entry.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        let mut at_login = current.is_some();
        let mut minimized = current.as_deref().is_some_and(|t| t.contains("--minimized"));
        let mut changed = axo_list::switch(ui, Item::new("Open at computer login"), &mut at_login);
        changed |= axo_list::switch(ui, Item::new("Start minimized").description("When opened at login, start in the background").disabled(!at_login), &mut minimized);
        if changed && let Some(entry) = entry {
            let result = if at_login {
                // The Desktop Entry spec's quoting: the path in double quotes,
                // with `"`, `` ` ``, `$` and `\\` escaped, and `%` doubled.
                let path = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "langis-gui".into());
                let quoted: String = path.chars().flat_map(|c| match c {
                    '"' | '`' | '$' | '\\' => vec!['\\', c],
                    '%' => vec!['%', '%'],
                    c => vec![c],
                }).collect();
                let exec = format!("\"{quoted}\"");
                let text = format!(
                    "[Desktop Entry]\nType=Application\nName=langis\nExec={exec}{}\nIcon=langis\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                    if minimized { " --minimized" } else { "" }
                );
                entry.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&entry, text))
            } else {
                std::fs::remove_file(&entry).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
            };
            if let Err(error) = result {
                app.toast(format!("Couldn't change the login item: {error}"));
            }
        }
    });
    // Desktop's Help menu (Debug Log, About), here in Settings.
    axo_list::gap(ui);
    axo_list::list(ui, Some("Help"), None, |ui| {
        if axo_list::item(ui, Item::new("Debug log").symbol("file"), End::Click).clicked() {
            open_debug_log(app);
        }
        if axo_list::item(ui, Item::new("About langis-gui").symbol("info"), End::Click).clicked() {
            app.settings.help = Some(Help::About);
        }
    });
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        let item = Item::new("Delete application data").description("This will delete all data in the application, removing all messages and saved account information.");
        if axo_list::item(ui, item, End::Action("Delete data", true)).clicked() {
            app.settings.ask = Some(Ask::DeleteData);
        }
    });
}

fn autostart_path() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("autostart").join("langis.desktop"))
}

fn appearance(app: &mut App, ui: &mut Ui) {
    let colors = app.chat_colors.clone();
    let zoom = ui.ctx().zoom_factor();
    axo_list::list(ui, None, None, |ui| {
        let themes = ["system", "light", "dark"];
        let chosen = themes.iter().position(|t| *t == app.theme).or(Some(0));
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("theme"), Item::new("Theme").symbol("contrast"), &["System", "Light", "Dark"], chosen) {
            let theme = themes[i].to_owned();
            app.theme = theme.clone();
            app.apply_theme();
            app.send(Command::SetLocalSetting(langis_core::LocalSetting::Theme(langis_core::Theme::from_setting(Some(&theme)))));
        }
        let bubble = crate::chat_colors::of(&colors.default, &colors);
        if axo_list::item(ui, Item::new("Chat color").symbol("palette"), End::Swatch(bubble)).clicked() {
            app.settings.page = Page::ChatColor;
        }
        let mut labels: Vec<String> = ZOOMS.iter().map(|(_, l)| (*l).to_owned()).collect();
        let mut values: Vec<f32> = ZOOMS.iter().map(|(z, _)| *z).collect();
        if !values.iter().any(|z| (z - zoom).abs() < 0.001) {
            let at = values.iter().position(|z| *z > zoom).unwrap_or(values.len());
            values.insert(at, zoom);
            labels.insert(at, format!("{}%", (zoom * 100.).round()));
        }
        let chosen = values.iter().position(|z| (z - zoom).abs() < 0.001);
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("zoom"), Item::new("Zoom level").symbol("zoom-in"), &options, chosen) {
            ui.ctx().set_zoom_factor(values[i]);
            write(app, SETTING_ZOOM, values[i].to_string());
        }
    });
}

fn chats(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    axo_list::list(ui, None, None, |ui| {
        account_switch(app, ui, s, AccountSetting::PreferContactAvatars, Item::new("Use address book photos").description("Display contact photos from your address book if available."));
        account_switch(app, ui, s, AccountSetting::KeepMutedChatsArchived, Item::new("Keep muted chats archived").description("Muted chats that are archived will remain archived when a new message arrives."));
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Text input"), None, |ui| {
        account_switch(app, ui, s, AccountSetting::LinkPreviews, Item::new("Generate link previews").description("Retrieve link previews directly from websites for messages you send."));
        // Desktop's text input rows, kept as `ui.` settings.
        let mut convert = app.input.auto_convert;
        if axo_list::switch(ui, Item::new("Convert typed emoticons to emoji").description("For example, :-) will be converted to 🙂"), &mut convert) {
            app.input.auto_convert = convert;
            write(app, SETTING_AUTO_CONVERT_EMOJI, convert.to_string());
        }
        let mut spell = app.input.spell;
        if axo_list::switch(ui, Item::new("Spell check text entered in message composition box"), &mut spell) {
            app.input.spell = spell;
            write(app, crate::spell::SETTING_SPELL_CHECK, spell.to_string());
        }
        let mut formatting = app.input.formatting;
        if axo_list::switch(ui, Item::new("Text formatting").description("Show the formatting toolbar when text is selected"), &mut formatting) {
            app.input.formatting = formatting;
            write(app, SETTING_TEXT_FORMATTING, formatting.to_string());
        }
        let tones = ["Default", "Light", "Medium light", "Medium", "Medium dark", "Dark"];
        let chosen = app.input.skin_tone.map(|t| langis_emoji::SkinTone::ALL.iter().position(|x| *x == t).unwrap_or(0) + 1).unwrap_or(0);
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("skin-tone"), Item::new("Default emoji skin tone"), &tones, Some(chosen)) {
            app.input.skin_tone = (i > 0).then(|| langis_emoji::SkinTone::ALL[i - 1]);
            crate::emoji_picker::set_skin_tone(app.input.skin_tone);
            write(app, SETTING_SKIN_TONE, i.to_string());
        }
    });
    // Images are shrunk without loss before sending; how hard is the choice
    // here (Desktop's "Sent media quality" is the nearest).
    axo_list::gap(ui);
    axo_list::list(ui, Some("Media"), None, |ui| {
        let levels = ["max", "fast", "off"];
        let chosen = levels.iter().position(|l| *l == s.optimization).or(Some(0));
        let item = Item::new("Sent image optimization").description("Every level keeps the picture as it is and removes its metadata; smaller takes longer");
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("optimization"), item, &["Smallest", "Faster", "Off"], chosen) {
            app.send(Command::SetLocalSetting(langis_core::LocalSetting::ImageOptimization(langis_core::media::Optimization::from_setting(Some(levels[i])))));
            app.settings.stale = true;
        }
    });
    // GIF search needs your own Giphy key.
    axo_list::gap(ui);
    axo_list::list(ui, Some("GIFs"), Some("Get a key at developers.giphy.com. Without one, GIF search is off."), |ui| {
        let id = egui::Id::unique("settings").with("giphy-key");
        let mut key = ui.ctx().data_mut(|d| d.get_temp_mut_or_insert_with(id, || s.giphy_key.clone()).clone());
        ui.horizontal(|ui| {
            ui.add_space(12.);
            ui.set_width(ui.available_width() - 12.);
            let r = widgets::text_field(ui, &mut key, "Giphy API key", true);
            if r.lost_focus() && key.trim() != s.giphy_key {
                app.send(Command::SetLocalSetting(langis_core::LocalSetting::GiphyKey(key.clone())));
                app.settings.stale = true;
                app.toast("Giphy key saved.");
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, key));
    });
    axo_list::gap(ui);
    let count = app.panes.folders.iter().filter(|f| f.folder_type == langis_store::ChatFolderType::Custom).count();
    axo_list::list(ui, Some("Chat folders"), None, |ui| {
        let (label, value) = if count > 0 {
            ("Add or edit folders", Some(if count == 1 { "1 folder".to_owned() } else { format!("{count} folders") }))
        } else {
            ("Add a chat folder", None)
        };
        let item = Item::new(label).description("Organize your chats into folders and quickly switch between them on your chat list.");
        if axo_list::item(ui, item, End::Arrow(value.as_deref())).clicked() {
            app.settings.page = Page::ChatFolders;
        }
    });
    axo_list::gap(ui);
    let synced = s.contacts_synced_at.map(|at| format!("Import all Signal groups and contacts from your mobile device. Last import at {}", crate::format::short_timestamp(at)));
    axo_list::list(ui, None, None, |ui| {
        // Desktop's Export chat history (plain text): each chat a text file,
        // its files beside it.
        let item = Item::new("Export chat history").description("Save every chat as a text file, with its downloaded files, to a folder you choose.");
        if axo_list::item(ui, item, End::Action("Export", false)).clicked()
            && let Some(folder) = rfd::FileDialog::new().set_title("Export chat history to").pick_folder()
        {
            let dir = folder.join(format!("Signal chats {}", chrono::Local::now().format("%Y-%m-%d %H%M")));
            let names = app.names.clone();
            app.toast("Exporting your chats…");
            app.load(move |db| async move {
                let result = crate::export::export_all(&db, &dir, &names).await;
                Some(crate::app::Loaded::Apply(Box::new(move |app: &mut App| match result {
                    Ok((chats, messages)) => app.toast(format!("Exported {chats} chats ({messages} messages) to {}", dir.display())),
                    Err(error) => app.toast(format!("Couldn't export: {error}")),
                })))
            });
        }
        let item = Item::new("Import contacts").description(synced.as_deref().unwrap_or("Import all Signal groups and contacts from your mobile device."));
        if axo_list::item(ui, item, End::Action("Import now", false)).clicked() {
            app.send(Command::SyncContacts);
            app.toast("Asking your phone for its contacts…");
        }
    });
}

/// Desktop's chat folders page: the folders, and the suggested ones.
fn chat_folders(app: &mut App, ui: &mut Ui) {
    use langis_store::{ChatFolderParams, ChatFolderType};
    let palette = Palette::of(ui.ctx());
    let intro = ui.painter().layout(
        "Organize your chats into folders and quickly switch between them on your chat list".to_owned(),
        theme::regular(12.),
        palette.text_secondary,
        ui.available_width() - 32.,
    );
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), intro.size().y + 8.), Sense::hover());
    ui.painter().galley(r.min + Vec2::new(16., 0.), intro, palette.text_secondary);
    axo_list::gap(ui);
    let folders: Vec<_> = app.panes.folders.clone();
    axo_list::list(ui, Some("Folders"), None, |ui| {
        if axo_list::item(ui, Item::new("Create a folder").symbol("plus"), End::Arrow(None)).clicked() {
            app.settings.folder = Some((new_id(), true, ChatFolderParams::defaults()));
        }
        let order: Vec<String> = folders.iter().map(|f| f.id.clone()).collect();
        let mut reorder: Option<Vec<String>> = None;
        for (i, f) in folders.iter().enumerate() {
            if f.folder_type == ChatFolderType::All {
                axo_list::item(ui, Item::new("All chats").symbol("folder"), End::None);
                continue;
            }
            let summary = folder_summary(f);
            let r = axo_list::item(ui, Item::new(&f.name).symbol("folder").description(&summary), End::Arrow(None));
            let edit = || folder_edit(f);
            if r.clicked() {
                app.settings.folder = Some(edit());
            }
            crate::menu::context(ui, &r.row, egui::Id::unique("settings").with(("folder-menu", &f.id)), |m| {
                if m.item(Some("pencil"), "Edit folder") {
                    app.settings.folder = Some(edit());
                }
                // Desktop drags folders into order; here they move a step.
                if i > 0 && m.item(Some("arrow-up"), "Move up") {
                    let mut o = order.clone();
                    o.swap(i, i - 1);
                    reorder = Some(o);
                }
                if i + 1 < order.len() && m.item(Some("arrow-down"), "Move down") {
                    let mut o = order.clone();
                    o.swap(i, i + 1);
                    reorder = Some(o);
                }
                if m.item(Some("trash"), "Delete") {
                    app.settings.ask = Some(Ask::DeleteFolder(f.id.clone(), f.name.clone()));
                }
            });
        }
        if let Some(order) = reorder {
            app.send(Command::EditChatFolders(langis_store::ChatFolderEdit::Reorder(order)));
        }
    });
    axo_list::gap(ui);
    let suggested = [
        ("Unread", "Unread messages from all chats", ChatFolderParams { name: "Unread".into(), show_only_unread: true, include_all_individual_chats: true, include_all_group_chats: true, ..ChatFolderParams::defaults() }),
        ("1:1 chats", "Only messages from direct chats", ChatFolderParams { name: "1:1 chats".into(), include_all_individual_chats: true, ..ChatFolderParams::defaults() }),
        ("Groups", "Only messages from group chats", ChatFolderParams { name: "Groups".into(), include_all_group_chats: true, ..ChatFolderParams::defaults() }),
    ];
    let has = |p: &ChatFolderParams| {
        folders.iter().any(|f| {
            f.folder_type == ChatFolderType::Custom
                && f.show_only_unread == p.show_only_unread
                && f.include_all_individual_chats == p.include_all_individual_chats
                && f.include_all_group_chats == p.include_all_group_chats
                && f.included.is_empty()
        })
    };
    let missing: Vec<_> = suggested.into_iter().filter(|(_, _, p)| !has(p)).collect();
    if !missing.is_empty() {
        axo_list::list(ui, Some("Suggested folders"), None, |ui| {
            for (name, description, params) in missing {
                if axo_list::item(ui, Item::new(name).symbol("folder").description(description), End::Action("Add", false)).clicked() {
                    app.send(Command::EditChatFolders(langis_store::ChatFolderEdit::Create { id: new_id(), params }));
                }
            }
        });
    }
}

fn folder_summary(f: &langis_store::ChatFolder) -> String {
    let mut parts = Vec::new();
    if f.include_all_individual_chats {
        parts.push("All 1:1 chats".to_owned());
    }
    if f.include_all_group_chats {
        parts.push("All groups".to_owned());
    }
    if !f.included.is_empty() {
        parts.push(format!("{} chats", f.included.len()));
    }
    if f.show_only_unread {
        parts.push("unread only".to_owned());
    }
    if parts.is_empty() { "No chats".into() } else { parts.join(", ") }
}

/// Desktop's folder name limit.
const FOLDER_NAME_MAX: usize = 32;

/// Desktop's `EditChatFolderPage`: the name (with an emoji button), the
/// included chats with Add chats, the exceptions with Exclude chats, Only
/// show unread and Include muted, Delete folder, then Cancel and Save.
fn folder_page(app: &mut App, ui: &mut Ui) {
    use langis_store::ConversationKind;
    let Some((id, new, mut params)) = app.settings.folder.take() else { return };
    let palette = Palette::of(ui.ctx());
    ui.add_space(8.);
    // The name, with Desktop's emoji button at its end.
    egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut params.name).hint_text("Folder name (required)").char_limit(FOLDER_NAME_MAX).desired_width(ui.available_width() - 44.).margin(egui::Margin::symmetric(10, 8)));
            let b = widgets::icon_button(ui, crate::icons::Icon::Emoji, 20., "Add an emoji");
            if b.clicked() {
                app.settings.folder_emoji = if app.settings.folder_emoji.is_some() { None } else { Some(Default::default()) };
            }
            if let Some(mut picker) = app.settings.folder_emoji.take() {
                let mut still = true;
                if let Some(e) = picker.show(ui.ctx(), Pos2::new(b.rect.right() - crate::emoji_picker::WIDTH, b.rect.bottom() + 456.), &mut still, &app.recent_emoji) {
                    if params.name.chars().count() + e.chars().count() <= FOLDER_NAME_MAX {
                        params.name.push_str(e);
                    }
                    still = false;
                }
                if still {
                    app.settings.folder_emoji = Some(picker);
                }
            }
        });
    });
    axo_list::gap(ui);
    let chat = |app: &App, id: &langis_store::ConversationId| app.conversation(id).cloned();
    let mut remove_included: Option<langis_store::ConversationId> = None;
    let mut remove_excluded: Option<langis_store::ConversationId> = None;
    let mut open_chats: Option<bool> = None;
    axo_list::list(ui, Some("Included chats"), Some("Choose chats that you want to appear in this folder"), |ui| {
        if axo_list::item(ui, Item::new("Add chats").symbol("plus"), End::Click).clicked() {
            open_chats = Some(false);
        }
        if params.include_all_individual_chats && axo_list::item(ui, Item::new("All 1:1 chats").symbol("person"), End::Action("Remove", false)).clicked() {
            params.include_all_individual_chats = false;
        }
        if params.include_all_group_chats && axo_list::item(ui, Item::new("All groups").symbol("group"), End::Action("Remove", false)).clicked() {
            params.include_all_group_chats = false;
        }
        for cid in params.included.clone() {
            let Some(c) = chat(app, &cid) else { continue };
            if folder_chat_row(ui, &palette, &c) {
                remove_included = Some(cid);
            }
        }
    });
    if params.include_all_individual_chats || params.include_all_group_chats || !params.excluded.is_empty() {
        axo_list::gap(ui);
        axo_list::list(ui, Some("Exceptions"), Some("Choose chats that you do not want to appear in this folder"), |ui| {
            if axo_list::item(ui, Item::new("Exclude chats").symbol("minus"), End::Click).clicked() {
                open_chats = Some(true);
            }
            for cid in params.excluded.clone() {
                let Some(c) = chat(app, &cid) else { continue };
                if folder_chat_row(ui, &palette, &c) {
                    remove_excluded = Some(cid);
                }
            }
        });
    }
    if let Some(r) = remove_included {
        params.included.retain(|x| *x != r);
    }
    if let Some(r) = remove_excluded {
        params.excluded.retain(|x| *x != r);
    }
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        axo_list::switch(ui, Item::new("Only show unread chats").description("When enabled, only chats with unread messages will be shown in this folder."), &mut params.show_only_unread);
        axo_list::switch(ui, Item::new("Include muted chats"), &mut params.show_muted_chats);
    });
    let mut delete = false;
    if !new {
        axo_list::gap(ui);
        axo_list::list(ui, None, None, |ui| {
            let mut item = Item::new("Delete folder");
            item.destructive = true;
            if axo_list::item(ui, item, End::Click).clicked() {
                delete = true;
            }
        });
    }
    // Cancel and Save, Save once something changed.
    let original_id = egui::Id::unique(("folder-editor-original", &id));
    let original: langis_store::ChatFolderParams = ui.ctx().data_mut(|d| d.get_temp_mut_or_insert_with(original_id, || params.clone()).clone());
    let changed = params != original || new;
    let mut answer = None;
    ui.add_space(16.);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(24.);
            let ready = changed && !params.name.trim().is_empty();
            if ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, if new { "Create" } else { "Save" }, true).clicked()).inner && ready {
                answer = Some(0);
            }
            ui.add_space(8.);
            if widgets::pill_button(ui, "Cancel", false).clicked() {
                answer = Some(1);
            }
        });
    });
    ui.add_space(24.);
    if let Some(exclude) = open_chats {
        let chosen = if exclude { params.excluded.clone() } else { params.included.clone() };
        app.settings.folder_chats = Some(FolderChats { exclude, query: String::new(), chosen, individual: params.include_all_individual_chats, groups: params.include_all_group_chats });
    }
    let _ = ConversationKind::Group;
    if app.settings.folder_leaving {
        app.settings.folder_leaving = false;
        answer = Some(1);
    }
    let asking_id = egui::Id::unique(("folder-editor-asking", &id));
    if answer == Some(1) && params != original && !new {
        ui.ctx().data_mut(|d| d.insert_temp(asking_id, true));
        answer = None;
    }
    let finish = |ui: &Ui| ui.ctx().data_mut(|d| {
        d.remove::<langis_store::ChatFolderParams>(original_id);
        d.remove::<bool>(asking_id);
    });
    if delete {
        finish(ui);
        app.settings.ask = Some(Ask::DeleteFolder(id, params.name));
        return;
    }
    match answer {
        Some(0) => {
            finish(ui);
            params.name = params.name.trim().to_owned();
            let edit = if new { langis_store::ChatFolderEdit::Create { id, params } } else { langis_store::ChatFolderEdit::Update { id, params } };
            app.send(Command::EditChatFolders(edit));
        }
        Some(_) => finish(ui),
        None => app.settings.folder = Some((id, new, params)),
    }
}

/// A chat in the folder page's lists, with its Remove.
fn folder_chat_row(ui: &mut Ui, palette: &Palette, c: &langis_store::Conversation) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.), Sense::hover());
    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 16., rect.center().y - 16.), Vec2::splat(32.));
    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
    ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y), Align2::LEFT_CENTER, &c.title, theme::regular(14.), palette.text);
    let button = Rect::from_min_max(Pos2::new(rect.right() - 16. - 76., rect.center().y - 15.), Pos2::new(rect.right() - 16., rect.center().y + 15.));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(button));
    widgets::axo_button(&mut child, "Remove", widgets::Axo::SubtleSecondary).clicked()
}

/// The folder page's dialogs: Desktop's Add chats / Exclude chats (the chat
/// types as chips, then every chat with a check), and Save changes?.
fn folder_dialogs(app: &mut App, ui: &mut Ui) {
    use langis_store::ConversationKind;
    if let Some(mut fc) = app.settings.folder_chats.take() {
        let palette = Palette::of(ui.ctx());
        let mut answer = None;
        let closed = crate::panes::modal(ui, "folder-chats", 420., |ui| {
            crate::panes::title(ui, if fc.exclude { "Exclude chats" } else { "Add chats" });
            widgets::text_field(ui, &mut fc.query, "Search", false);
            ui.add_space(8.);
            if !fc.exclude {
                ui.label(egui::RichText::new("Chat types").font(theme::semibold(13.)).color(palette.text_secondary));
                ui.add_space(4.);
                ui.horizontal(|ui| {
                    for (label, on) in [("1:1 chats", &mut fc.individual), ("Groups", &mut fc.groups)] {
                        let fill = if *on { palette.badge } else { palette.fill_secondary };
                        let text = if *on { egui::Color32::WHITE } else { palette.text };
                        let shown = label.to_owned();
                        if ui.add(egui::Button::new(egui::RichText::new(shown).color(text).font(theme::medium(13.))).fill(fill).corner_radius(CornerRadius::same(14))).clicked() {
                            *on = !*on;
                        }
                    }
                });
                ui.add_space(8.);
            }
            ui.label(egui::RichText::new("Chats").font(theme::semibold(13.)).color(palette.text_secondary));
            let q = fc.query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                let list: Vec<langis_store::Conversation> = app.conversations.iter().filter(|c| c.kind != ConversationKind::NoteToSelf || !fc.exclude).filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q)).cloned().collect();
                for c in list {
                    let on = fc.chosen.contains(&c.id);
                    let (rect, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.), Sense::click());
                    if hit.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                    }
                    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 8., rect.center().y - 16.), Vec2::splat(32.));
                    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(&c));
                    ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y), Align2::LEFT_CENTER, &c.title, theme::regular(14.), palette.text);
                    crate::panes::round_check(ui, &palette, Pos2::new(rect.right() - 20., rect.center().y), on);
                    if hit.clicked() {
                        if on {
                            fc.chosen.retain(|x| *x != c.id);
                        } else {
                            fc.chosen.push(c.id.clone());
                        }
                    }
                }
            });
            answer = crate::panes::buttons(ui, &["Done", "Cancel"]);
        });
        match answer {
            Some(0) => {
                if let Some((_, _, params)) = app.settings.folder.as_mut() {
                    if fc.exclude {
                        params.excluded = fc.chosen.clone();
                        params.included.retain(|x| !fc.chosen.contains(x));
                    } else {
                        params.included = fc.chosen.clone();
                        params.excluded.retain(|x| !fc.chosen.contains(x));
                        params.include_all_individual_chats = fc.individual;
                        params.include_all_group_chats = fc.groups;
                    }
                }
            }
            Some(_) => {}
            None if closed => {}
            None => app.settings.folder_chats = Some(fc),
        }
    }
    // Leaving with changes: Desktop's "Save changes?".
    let Some((id, new, params)) = app.settings.folder.clone() else { return };
    let asking_id = egui::Id::unique(("folder-editor-asking", &id));
    if !ui.ctx().data(|d| d.get_temp::<bool>(asking_id)).unwrap_or(false) {
        return;
    }
    use crate::dialog::{Answer, Variant};
    let original_id = egui::Id::unique(("folder-editor-original", &id));
    let done = |ui: &Ui| ui.ctx().data_mut(|d| {
        d.remove::<langis_store::ChatFolderParams>(original_id);
        d.remove::<bool>(asking_id);
    });
    match crate::dialog::show(ui.ctx(), egui::Id::unique("folder-unsaved"), "Save changes?", "You made changes to this chat folder.", Some("Discard"), &[("Save", Variant::StrongPrimary)]) {
        Some(Answer::Action(_)) => {
            done(ui);
            app.settings.folder = None;
            if !params.name.trim().is_empty() {
                let edit = if new { langis_store::ChatFolderEdit::Create { id, params } } else { langis_store::ChatFolderEdit::Update { id, params } };
                app.send(Command::EditChatFolders(edit));
            }
        }
        Some(Answer::Cancel) => {
            done(ui);
            app.settings.folder = None;
        }
        None => {}
    }
}

/// A new folder's id (a random UUID, as Desktop makes them).
pub fn new_id() -> String {
    use std::io::Read as _;
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// `CallingDeviceSelectionItems`: Video, Microphone and Speakers, each a
/// select of what's there.
pub fn device_items(app: &mut App, ui: &mut Ui, salt: &str) {
    let devices = app.call_devices.clone();
    let lists = [
        (DeviceKind::Camera, "Video", "videocamera", &devices.devices.cameras, &devices.camera),
        (DeviceKind::Microphone, "Microphone", "mic", &devices.devices.microphones, &devices.microphone),
        (DeviceKind::Speaker, "Speakers", "speaker", &devices.devices.speakers, &devices.speaker),
    ];
    for (kind, label, symbol, list, chosen) in lists {
        let names: Vec<&str> = if list.is_empty() { vec!["No devices available"] } else { list.iter().map(|d| d.name.as_str()).collect() };
        let index = chosen.as_ref().and_then(|id| list.iter().position(|d| &d.id == id)).or(Some(0));
        let item = Item::new(label).symbol(symbol).disabled(list.is_empty());
        if let Some(i) = axo_list::select(ui, egui::Id::unique(salt).with(("device", label)), item, &names, index)
            && let Some(d) = list.get(i)
        {
            app.send(Command::SetCallDevice { kind, id: d.id.clone() });
            app.send(Command::LoadCallDevices);
        }
    }
}

fn calls(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    axo_list::list(ui, None, None, |ui| {
        let mut incoming = s.incoming_calls;
        if axo_list::switch(ui, Item::new("Enable incoming calls"), &mut incoming) {
            app.send(Command::SetLocalSetting(langis_core::LocalSetting::IncomingCalls(incoming)));
            // Shown at once; a reload could race the command.
            if let Some(snapshot) = app.settings.snapshot.as_mut() {
                snapshot.incoming_calls = incoming;
            }
        }
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Devices"), None, |ui| device_items(app, ui, "settings"));
    axo_list::gap(ui);
    axo_list::list(ui, Some("Advanced"), None, |ui| {
        let mut relay = s.always_relay;
        let item = Item::new("Always relay calls").description("Relay all calls through the Signal server to avoid revealing your IP address to your contact. Enabling will reduce call quality.");
        if axo_list::switch(ui, item, &mut relay) {
            app.send(Command::SetLocalSetting(langis_core::LocalSetting::AlwaysRelayCalls(relay)));
            if let Some(snapshot) = app.settings.snapshot.as_mut() {
                snapshot.always_relay = relay;
            }
        }
    });
    axo_list::gap(ui);
    screen_sharing(app, ui);
}

/// Ours: the quality screen sharing sends at.
fn screen_sharing(app: &mut App, ui: &mut Ui) {
    let current = app.call_devices.share_quality;
    let preset = ShareQuality::PRESETS.iter().position(|(_, q)| *q == current);
    let custom = app.settings.custom_quality.is_some() || preset.is_none();
    let mut names: Vec<&str> = ShareQuality::PRESETS.iter().map(|(n, _)| *n).collect();
    names.push("Custom");
    let chosen = if custom { Some(names.len() - 1) } else { preset };
    let mut pick = None;
    axo_list::list(ui, Some("Screen sharing"), None, |ui| {
        let item = Item::new("Quality").description("Higher looks sharper and smoother, and uses more bandwidth and processor.");
        pick = axo_list::select(ui, egui::Id::unique("settings").with("share-quality"), item, &names, chosen);
        if let Some(mut q) = app.settings.custom_quality.or(custom.then_some(current)) {
            let before = q;
            let mut native = q.max_height == 0;
            control_row(ui, "Resolution", |ui| {
                if !native {
                    if q.max_height == 0 {
                        q.max_height = 1080;
                    }
                    ui.add(egui::Slider::new(&mut q.max_height, 144..=2160).suffix("p").step_by(2.));
                }
                crate::widgets::checkbox(ui, &mut native, "Full");
            });
            if native {
                q.max_height = 0;
            }
            control_row(ui, "Frame rate", |ui| {
                ui.add(egui::Slider::new(&mut q.fps, 1..=60).suffix(" fps"));
            });
            app.settings.custom_quality = Some(q);
            let released = !ui.input(|i| i.pointer.any_down());
            if (q != before || q != current) && released {
                set_quality(app, q);
            }
        }
    });
    match pick {
        Some(i) if i < ShareQuality::PRESETS.len() => {
            app.settings.custom_quality = None;
            set_quality(app, ShareQuality::PRESETS[i].1);
        }
        Some(_) => app.settings.custom_quality = Some(current),
        None => {}
    }
}

/// An item whose end is an egui control.
fn control_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let palette = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_min_height(42.);
        ui.add_space(12.);
        ui.label(egui::RichText::new(label).font(theme::regular(13.)).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8., 0.);
            ui.add_space(12.);
            add(ui);
        });
    });
}

fn set_quality(app: &mut App, q: ShareQuality) {
    app.send(Command::SetShareQuality(q));
    app.call_devices.share_quality = q;
}

fn notifications(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    axo_list::list(ui, None, None, |ui| {
        let mut on = s.notifications;
        if axo_list::switch(ui, Item::new("Enable notifications"), &mut on) {
            write(app, SETTING_NOTIFICATIONS, on.to_string());
            app.settings.stale = true;
            app.notify.0 = on;
        }
        let kinds = ["all", "name", "none"];
        let chosen = kinds.iter().position(|k| *k == s.notification_content).or(Some(0));
        let item = Item::new("Show").disabled(!s.notifications);
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("notification-content"), item, &["Name, content, and actions", "Name only", "No name or content"], chosen) {
            let content = kinds[i].to_owned();
            app.notify.1 = content.clone();
            write(app, SETTING_NOTIFICATION_CONTENT, content);
            app.settings.stale = true;
        }
        // What shows while muted, in Desktop's summary.
        let shown: Vec<&str> = [(AccountSetting::NotifyCallsIfMuted, "Calls"), (AccountSetting::NotifyMentionsIfMuted, "Mentions"), (AccountSetting::NotifyRepliesIfMuted, "Replies")]
            .into_iter()
            .filter(|(k, _)| account_on(s, *k))
            .map(|(_, l)| l)
            .collect();
        let summary = if shown.is_empty() { "None".to_owned() } else { shown.join(", ") };
        let item = Item::new("While muted").description("Choose notifications to show for muted chats.").disabled(!s.notifications);
        if axo_list::item(ui, item, End::Arrow(Some(&summary))).clicked() && s.notifications {
            app.settings.page = Page::WhileMuted;
        }
    });
    // Desktop's sound and attention rows, then its call ones.
    let enabled = s.notifications;
    for (label, rows) in [
        (
            "Sounds",
            vec![
                (0usize, "Notification sound", "Play a sound with each notification"),
                (1, "In-chat sounds", "Play a sound for new messages in the chat you're reading"),
                (4, "Draw attention to this window when a notification arrives", ""),
            ],
        ),
        ("Calls", vec![(3, "Incoming call notifications", "Show a notification when a call comes in"), (2, "Call ringtone", "Play a ringtone for incoming calls")]),
    ] {
    axo_list::gap(ui);
    axo_list::list(ui, Some(label), None, |ui| {
        for (i, title, description) in rows {
            let prefs = &mut app.sound;
            let slot = match i {
                0 => &mut prefs.notification,
                1 => &mut prefs.in_chat,
                2 => &mut prefs.ringtone,
                3 => &mut prefs.call_notify,
                _ => &mut prefs.attention,
            };
            let mut on = *slot;
            // Message sounds follow notifications being on; call ones don't.
            let off = !enabled && matches!(i, 0 | 4);
            let item = if description.is_empty() { Item::new(title) } else { Item::new(title).description(description) }.disabled(off);
            if axo_list::switch(ui, item, &mut on) && !off {
                *slot = on;
                write(app, crate::sounds::KEYS[i], on.to_string());
                if on && i == 0 {
                    crate::sounds::once(crate::sounds::Sound::Pop);
                }
            }
        }
    });
    }
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        account_switch(app, ui, s, AccountSetting::ReactionNotifications, Item::new("Reaction notifications").description("Notify when someone reacts to your message"));
        account_switch(app, ui, s, AccountSetting::ShowUnreadReminders, Item::new("Unread reminders").description("Occasionally notify when there are unread messages in muted chats."));
    });
    // Desktop's Reset notification settings: this device's back to default.
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        if axo_list::item(ui, Item::new("Reset notification settings"), End::Click).clicked() {
            app.settings.ask = Some(Ask::ResetNotifications);
        }
    });
}

/// Desktop's Reset notification settings: this device's back to default.
fn reset_notifications(app: &mut App) {
    {
        {
            app.sound = crate::sounds::Prefs::default();
            let d = app.sound;
            for (key, on) in crate::sounds::KEYS.iter().zip([d.notification, d.in_chat, d.ringtone, d.call_notify, d.attention]) {
                write(app, key, on.to_string());
            }
            write(app, SETTING_NOTIFICATIONS, "true".into());
            write(app, SETTING_NOTIFICATION_CONTENT, "all".into());
            app.notify = (true, "all".into());
            app.settings.stale = true;
            app.toast("Notification settings reset.");
        }
    }
}

fn while_muted(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    axo_list::list(ui, None, None, |ui| {
        account_switch(app, ui, s, AccountSetting::NotifyCallsIfMuted, Item::new("Calls").symbol("phone").description("Ring or notify when a call is started in muted chats."));
        account_switch(app, ui, s, AccountSetting::NotifyMentionsIfMuted, Item::new("Mentions").symbol("at").description("Notify when you are mentioned in muted chats."));
        account_switch(app, ui, s, AccountSetting::NotifyRepliesIfMuted, Item::new("Replies").symbol("reply").description("Notify when someone replies to your message in muted chats."));
    });
}

fn privacy(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    axo_list::list(ui, None, None, |ui| {
        let item = Item::new("Phone Number").description("Choose who can see your phone number and who can contact you on Signal with it.");
        if axo_list::item(ui, item, End::Arrow(None)).clicked() {
            app.settings.page = Page::PhoneNumber;
        }
    });
    axo_list::gap(ui);
    let users = app.conversations.iter().filter(|c| c.blocked && c.kind != ConversationKind::Group).count();
    let groups = app.conversations.iter().filter(|c| c.blocked && c.kind == ConversationKind::Group).count();
    let count = match (users, groups) {
        (0, 0) => "No users or groups".to_owned(),
        (u, 0) => if u == 1 { "1 user".into() } else { format!("{u} users") },
        (0, g) => if g == 1 { "1 group".into() } else { format!("{g} groups") },
        (u, g) => if u + g == 1 { "1 user or group".into() } else { format!("{} users or groups", u + g) },
    };
    axo_list::list(ui, None, None, |ui| {
        if axo_list::item(ui, Item::new("Blocked"), End::Arrow(Some(&count))).clicked() {
            app.settings.page = Page::Blocked;
        }
    });
    axo_list::gap(ui);
    let help = "See and share when message are being read and typed. If disabled, you won\u{2019}t see read receipts or typing indicators from others.";
    axo_list::list(ui, Some("Messaging"), Some(help), |ui| {
        account_switch(app, ui, s, AccountSetting::ReadReceipts, Item::new("Read receipts"));
        account_switch(app, ui, s, AccountSetting::TypingIndicators, Item::new("Typing indicators"));
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Disappearing messages"), None, |ui| {
        let mut values: Vec<u32> = crate::details::TIMERS.iter().map(|(v, _)| *v).filter(|v| *v != u32::MAX).collect();
        values.sort_by_key(|v| if *v == 0 { 0 } else { u32::MAX - v });
        if !values.contains(&s.universal_timer) {
            values.push(s.universal_timer);
        }
        let mut labels: Vec<String> = values.iter().map(|v| if *v == 0 { "Off".to_owned() } else { crate::format::timer_long(*v) }).collect();
        labels.push("Custom time…".to_owned());
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        let chosen = values.iter().position(|v| *v == s.universal_timer);
        let item = Item::new("Default timer for new chats").description("Set a default disappearing message timer for all new chats started by you.");
        if let Some(i) = axo_list::select(ui, egui::Id::unique("settings").with("universal-timer"), item, &options, chosen) {
            if i == values.len() {
                // Desktop's Custom time, for the default too.
                let (value, unit) = crate::panes::custom_timer_start(if s.universal_timer > 0 { s.universal_timer } else { 3_600 });
                app.panes.custom_timer = Some((crate::panes::TimerTarget::Universal, value, unit));
            } else {
                app.send(Command::SetUniversalExpireTimer(values[i]));
                app.settings.stale = true;
            }
        }
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Stories"), None, |ui| {
        let item = Item::new("Share & View Stories").description("If you opt out of stories you will no longer be able to share or view stories.");
        if s.stories_disabled {
            if axo_list::item(ui, item, End::Action("Turn on stories", false)).clicked() {
                app.send(Command::SetStoriesDisabled(false));
                app.settings.stale = true;
            }
        } else if axo_list::item(ui, item, End::Action("Turn off stories", true)).clicked() {
            app.settings.ask = Some(Ask::StoriesOff);
        }
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Sealed Sender"), None, |ui| {
        account_switch(app, ui, s, AccountSetting::SealedSenderIndicators, Item::new("Show status icon").description("Show an icon in message details when they were delivered using sealed sender."));
    });
    axo_list::gap(ui);
    axo_list::list(ui, Some("Advanced"), None, |ui| {
        let mut auto = !s.key_transparency_disabled;
        let item = Item::new("Automatic key verification").description("When enabled, Signal will attempt to automatically verify the encryption of 1:1 chats.");
        if axo_list::switch(ui, item, &mut auto) {
            app.send(Command::SetKeyTransparencyDisabled(!auto));
            app.settings.stale = true;
        }
    });
}

/// Desktop's phone number page (`SettingsRow` and `SettingsRadio`).
fn phone_number(app: &mut App, ui: &mut Ui, palette: &Palette, s: &Snapshot) {
    let sharing = if s.phone_sharing {
        "Your phone number will be visible to people and groups you message."
    } else if s.phone_discoverable {
        "Your phone number will not be visible to anyone unless they have it saved in their phone\u{2019}s contacts."
    } else {
        "Your phone number will not be visible to anyone."
    };
    if let Some(everyone) = radio_row(ui, palette, "Who can see my number", s.phone_sharing, sharing, !s.phone_discoverable) {
        app.send(Command::SetPhoneNumberSharing(everyone));
        app.settings.stale = true;
    }
    ui.add_space(24.);
    let finding = if s.phone_discoverable {
        "Anyone who has your phone number will see you're on Signal and can start chats with you."
    } else {
        "Nobody will be able to see you're on Signal unless you message them or have an existing chat with them."
    };
    match radio_row(ui, palette, "Who can find me by number", s.phone_discoverable, finding, s.phone_sharing) {
        Some(true) => {
            app.send(Command::SetPhoneNumberDiscoverable(true));
            app.settings.stale = true;
        }
        Some(false) => app.settings.ask = Some(Ask::NobodyCanFind),
        None => {}
    }
}

/// A titled pair of Everyone and Nobody radios, then the description.
/// `everyone_only` keeps Nobody from being picked (Desktop's `readOnly`).
fn radio_row(ui: &mut Ui, palette: &Palette, title: &str, everyone: bool, description: &str, everyone_only: bool) -> Option<bool> {
    let width = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, 28.), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 24., r.top() + 10.), Align2::LEFT_CENTER, title, theme::semibold(14.), palette.text);
    let mut picked = None;
    for (label, value, readonly) in [("Everyone", true, false), ("Nobody", false, everyone_only)] {
        let (row, response) = ui.allocate_exact_size(Vec2::new(width, 40.), if readonly { Sense::hover() } else { Sense::click() });
        let center = Pos2::new(row.left() + 24. + 10., row.center().y);
        let on = everyone == value;
        let color = if readonly { palette.text_secondary } else { palette.text };
        if on {
            ui.painter().circle_filled(center, 10., palette.badge);
            theme::paint_symbol(ui.painter(), "check", center, 14., egui::Color32::WHITE);
        } else {
            ui.painter().circle_stroke(center, 9., Stroke::new(1.5, palette.text_secondary));
        }
        ui.painter().text(Pos2::new(center.x + 10. + 16., center.y), Align2::LEFT_CENTER, label, theme::regular(14.), color);
        if response.clicked() && !on {
            picked = Some(value);
        }
    }
    let g = ui.painter().layout(description.to_owned(), theme::regular(12.), palette.text_secondary, width - 48.);
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, g.size().y + 8.), Sense::hover());
    ui.painter().galley(Pos2::new(r.left() + 24., r.top() + 4.), g, palette.text_secondary);
    picked
}

/// Desktop's blocked page: the people, then the groups.
fn blocked(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let people: Vec<_> = app.conversations.iter().filter(|c| c.blocked && c.kind != ConversationKind::Group).cloned().collect();
    let groups: Vec<_> = app.conversations.iter().filter(|c| c.blocked && c.kind == ConversationKind::Group).cloned().collect();
    let intro = ui.painter().layout("Blocked users will not be able to call you or send you messages".to_owned(), theme::regular(12.), palette.text_secondary, ui.available_width() - 32.);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), intro.size().y + 8.), Sense::hover());
    ui.painter().galley(r.min + Vec2::new(16., 0.), intro, palette.text_secondary);
    // `PreferencesBlockChooserDialog`: pick someone (or a group) to block.
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        if axo_list::item(ui, Item::new("Add blocked user").symbol("plus"), End::Click).clicked() {
            app.settings.block_chooser = Some(String::new());
        }
    });
    if let Some(mut query) = app.settings.block_chooser.take() {
        let mut picked = None;
        let mut cancel = false;
        let closed = crate::panes::modal(ui, "block-chooser", 380., |ui| {
            crate::panes::title(ui, "Block someone");
            widgets::text_field(ui, &mut query, "Search", false);
            ui.add_space(6.);
            let q = query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                for c in app.conversations.iter().filter(|c| !c.blocked && c.kind != ConversationKind::NoteToSelf && (q.is_empty() || c.title.to_lowercase().contains(&q))) {
                    let r = crate::panes::contact_row(ui, palette, &c.title, c.phone.as_deref(), c.color, c.avatar_path.as_deref());
                    if r.clicked() {
                        picked = Some(c.id.clone());
                    }
                }
            });
            cancel = crate::panes::buttons(ui, &["Cancel"]).is_some();
        });
        match picked {
            // Blocking asks first, as everywhere.
            Some(id) => app.panes.chat_confirm = Some((id, crate::panes::ChatConfirm::Block)),
            None if !cancel && !closed => app.settings.block_chooser = Some(query),
            None => {}
        }
    }
    for (title, list) in [("Blocked Users", people), ("Blocked Groups", groups)] {
        if list.is_empty() {
            continue;
        }
        axo_list::gap(ui);
        axo_list::list(ui, Some(title), None, |ui| {
            for c in &list {
                let width = ui.available_width();
                let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 52.), Sense::click());
                if response.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(14), palette.fill);
                }
                let avatar = Rect::from_min_size(Pos2::new(rect.left() + 12., rect.center().y - 18.), Vec2::splat(36.));
                widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
                // The name, and when it was blocked under it.
                match c.blocked_at {
                    Some(at) => {
                        let date = chrono::DateTime::from_timestamp_millis(at).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default();
                        ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y - 8.), Align2::LEFT_CENTER, &c.title, theme::regular(13.), palette.text);
                        ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y + 9.), Align2::LEFT_CENTER, format!("Blocked on {date}"), theme::regular(12.), palette.text_secondary);
                    }
                    None => {
                        ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y), Align2::LEFT_CENTER, &c.title, theme::regular(13.), palette.text);
                    }
                }
                if response.clicked() {
                    app.settings.ask = Some(Ask::Unblock(c.id.clone(), c.title.clone(), c.kind == ConversationKind::Group));
                }
            }
        });
    }
}

fn data_usage(app: &mut App, ui: &mut Ui, s: &Snapshot) {
    let mut auto = s.auto_download.clone();
    let mut changed = false;
    axo_list::list(ui, Some("Media auto-download"), Some("Voice messages and stickers are always automatically downloaded."), |ui| {
        for (label, on) in [("Photos", &mut auto.photos), ("Videos", &mut auto.videos), ("Audio", &mut auto.audio), ("Documents", &mut auto.documents)] {
            changed |= axo_list::switch(ui, Item::new(label), on);
        }
    });
    if changed {
        app.send(Command::SetAutoDownload(auto.clone()));
        if let Some(snapshot) = app.settings.snapshot.as_mut() {
            snapshot.auto_download = auto;
        }
    }
}

/// The open question's dialog, and what its answer does.
fn ask(app: &mut App, ctx: &egui::Context) {
    let Some(question) = app.settings.ask.clone() else { return };
    let id = egui::Id::unique("settings-ask");
    let done = match &question {
        Ask::LinkedDevice => dialog::alert(
            ctx,
            id,
            "Linked Devices let you access your Signal account and messages securely on any of your additional devices.",
            "You can link a desktop, tablet or additional phone. Your primary device manages what\u{2019}s linked, you can link up to 5 devices. Account settings and Signal Secure Backups can only be managed on your primary device.",
            "OK",
        ),
        Ask::DeleteData => match dialog::confirm(ctx, id, "Delete all data?", "Delete all data and messages from this version of Signal Desktop? Your Signal account and data on your phone or other devices will not be deleted.", "Delete data", Variant::StrongDestructive) {
            Some(Choice::Action) => {
                app.send(Command::Unlink);
                true
            }
            Some(Choice::Cancel) => true,
            None => false,
        },
        Ask::ResetNotifications => match dialog::confirm(ctx, id, "Reset notification settings?", "Notification settings on this device will go back to their defaults.", "Reset", Variant::StrongDestructive) {
            Some(Choice::Action) => {
                reset_notifications(app);
                true
            }
            Some(Choice::Cancel) => true,
            None => false,
        },
        Ask::StoriesOff => match dialog::confirm(ctx, id, "Turn off stories", "You will no longer be able to share or view stories. Story updates you have recently shared will also be deleted.", "Turn off", Variant::StrongDestructive) {
            Some(Choice::Action) => {
                app.send(Command::SetStoriesDisabled(true));
                app.settings.stale = true;
                true
            }
            Some(Choice::Cancel) => true,
            None => false,
        },
        Ask::NobodyCanFind => match dialog::confirm(ctx, id, "Are you sure?", "If you change \u{201c}Who can find me by number\u{201d} to \u{201c}Nobody\u{201d} it will make it harder for people to find you on Signal.", "OK", Variant::StrongPrimary) {
            Some(Choice::Action) => {
                app.send(Command::SetPhoneNumberDiscoverable(false));
                app.settings.stale = true;
                true
            }
            Some(Choice::Cancel) => true,
            None => false,
        },
        Ask::Unblock(conversation, name, group) => {
            let body = if *group { "Group members will be able to add you to this group again." } else { "You will be able to message and call each other." };
            match dialog::confirm(ctx, id, &format!("Unblock {name}?"), body, "Unblock", Variant::StrongPrimary) {
                Some(Choice::Action) => {
                    app.send(Command::SetChat { conversation: conversation.clone(), change: ChatChange::Blocked(false) });
                    true
                }
                Some(Choice::Cancel) => true,
                None => false,
            }
        }
        Ask::DeleteFolder(folder, name) => match dialog::confirm(ctx, id, "Delete folder?", &format!("Do you want to delete the folder \u{201c}{name}\u{201d}?"), "Delete", Variant::StrongDestructive) {
            Some(Choice::Action) => {
                let at = chrono::Utc::now().timestamp_millis() as u64;
                app.send(Command::EditChatFolders(langis_store::ChatFolderEdit::Delete { id: folder.clone(), at }));
                true
            }
            Some(Choice::Cancel) => true,
            None => false,
        },
    };
    if done {
        app.settings.ask = None;
    }
}
