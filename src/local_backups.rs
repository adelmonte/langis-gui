//! Desktop's Settings > Backups (`PreferencesBackups.dom.tsx`), its
//! "Desktop backups" (`PreferencesLocalBackups.dom.tsx`: choose a folder,
//! the recovery key and its confirmation, Back up now, the folder, View
//! key, Turn off) and the export dialogs (`LocalBackupExportWorkflow`,
//! `LocalBackupsConfirmKeyDialog`). The backup itself is langis's
//! (`langis_core::local_backup`).
//!
//! Left out: Desktop asks the system to authenticate (polkit) before
//! showing the key or setting up, and clears a copied key from the
//! clipboard after 45 seconds. Signal Secure Backups (the remote tier) is
//! only described: it's managed on the phone.

use std::path::PathBuf;

use egui::{RichText, Ui, Vec2};
use langis_core::Command;
use langis_core::local_backup::{self, LocalBackupError, LocalBackupProgress};

use crate::app::{App, Loaded};
use crate::axo_list::{self, End, Item};
use crate::dialog::{self, Answer, Variant};
use crate::i18n::{Arg, tf, tr};
use crate::settings::Page;
use crate::theme::{self, Palette};
use crate::widgets;

/// What the pages know of Desktop backups.
#[derive(Default)]
pub struct State {
    /// The key and the settings were asked for.
    asked: bool,
    /// Read from the settings: `None` until loaded.
    loaded: Option<Stored>,
    /// The recovery key: outer `None` until langis answered, inner `None`
    /// when this device has none.
    key: Option<Option<String>>,
    /// The key pages' step and what's typed in the confirm step.
    step: KeyStep,
    typed: String,
    /// "Do not share recovery key", before copying.
    do_not_share: bool,
    /// "Your recovery key has changed": what to do after viewing it.
    key_changed: bool,
    /// "Turn off backups" and its "Delete my backup" box.
    turning_off: Option<bool>,
    /// The export dialog.
    workflow: Option<LocalBackupProgress>,
    /// Waiting on the system's password prompt, and where a yes leads.
    auth: Option<(std::sync::mpsc::Receiver<bool>, Page)>,
}

#[derive(Clone, Default)]
struct Stored {
    folder: Option<String>,
    last: Option<(i64, PathBuf)>,
    viewed_hash: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum KeyStep {
    #[default]
    View,
    Confirm,
    Caution,
}

fn load(app: &App) {
    app.load(|db| async move {
        let get = async |key: &str| db.setting(key).await.ok().flatten().filter(|v| !v.is_empty());
        let stored = Stored {
            folder: get(local_backup::SETTING_FOLDER).await,
            last: get(local_backup::SETTING_LAST).await.as_deref().and_then(local_backup::parse_last),
            viewed_hash: get(local_backup::SETTING_KEY_VIEWED).await,
        };
        Some(Loaded::Apply(Box::new(move |app: &mut App| app.settings.backups.loaded = Some(stored))))
    });
}

/// Takes the Desktop backup events; `true` when it was one.
pub fn on_event(app: &mut App, event: &langis_core::Event) -> bool {
    use langis_core::Event;
    match event {
        Event::RecoveryKey(key) => app.settings.backups.key = Some(key.clone()),
        Event::LocalBackupsChanged => load(app),
        Event::LocalBackup(LocalBackupProgress::Failed(LocalBackupError::Cancelled)) => app.settings.backups.workflow = None,
        Event::LocalBackup(progress) => {
            // A dialog closed (cancelled) stays closed until the next start.
            if app.settings.backups.workflow.is_some() || matches!(progress, LocalBackupProgress::Messages) {
                app.settings.backups.workflow = Some(progress.clone());
            }
        }
        _ => return false,
    }
    true
}

impl State {
    /// Set up: a folder and a confirmed key (Desktop's `isLocalBackupsSetup`).
    fn set_up(&self) -> bool {
        self.loaded.as_ref().is_some_and(|s| s.folder.is_some() && s.viewed_hash.is_some())
    }

    /// The confirmed key is this one (Desktop compares `backupKeyViewedHash`).
    fn key_current(&self) -> bool {
        match (&self.key, self.loaded.as_ref().and_then(|s| s.viewed_hash.as_deref())) {
            (Some(Some(key)), Some(hash)) => local_backup::key_hash(key) == hash,
            _ => false,
        }
    }
}

/// Asks for the recovery key and the settings, once a visit to Settings.
fn ask(app: &mut App) {
    if !app.settings.backups.asked {
        app.settings.backups.asked = true;
        app.send(Command::LoadRecoveryKey);
        load(app);
    }
    // The system's answer, once it's given.
    if let Some((answer, page)) = app.settings.backups.auth.take() {
        match answer.try_recv() {
            Ok(true) => app.settings.page = page,
            Ok(false) => app.toast(tr("Authentication failed")),
            Err(std::sync::mpsc::TryRecvError::Empty) => app.settings.backups.auth = Some((answer, page)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }
}

/// Desktop's `promptOSAuth` on Linux: polkit's `pkcheck` asks for the
/// user's password, when our action is registered (`data/org.langis.policy`
/// installed under /usr/share/polkit-1/actions, which needs root). Without
/// it, as in Desktop, there's nothing to ask and the step goes ahead.
fn authorize(app: &mut App, reason: &'static str, then: Page) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = app.ctx.clone();
    std::thread::spawn(move || {
        let action = format!("org.langis.{reason}");
        let run = |cmd: &str, args: &[&str]| std::process::Command::new(cmd).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        let registered = run("pkaction", &["--action-id", &action]).is_ok_and(|s| s.success());
        let allowed = !registered
            || run("pkcheck", &["-u", "--process", &std::process::id().to_string(), "--action-id", &action]).is_ok_and(|s| s.success());
        let _ = tx.send(allowed);
        ctx.request_repaint();
    });
    app.settings.backups.auth = Some((rx, then));
}

/// Desktop's `copyTextTemporarily`: the recovery key leaves the clipboard
/// after 45 seconds, unless something else was copied since.
fn copy_for_a_while(ctx: &egui::Context, text: String) {
    ctx.copy_text(text.clone());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(45));
        if crate::wlclip::text().is_some_and(|now| now == text) {
            crate::wlclip::clear();
        }
    });
}

/// Desktop's Backups page.
/// Desktop's "Other ways to back up" under Settings > Backups (whose
/// Signal Secure Backups part is `crate::backups::page`).
pub fn other_ways(app: &mut App, ui: &mut Ui) {
    ask(app);
    axo_list::gap(ui);
    let state = &app.settings.backups;
    let no_key = state.key == Some(None);
    let set_up = state.set_up();
    axo_list::list(ui, Some(tr("Other ways to back up")), None, |ui| {
        let description = if no_key {
            "Your phone didn't give this device your recovery key, so it can't make a backup your phone could restore. Linking again gives it."
        } else {
            tr("Create an end-to-end encrypted backup that you can restore on your phone.")
        };
        let item = Item::new(tr("Desktop backups")).symbol("folder").description(description).disabled(no_key);
        let action = if set_up { tr("Manage") } else { tr("Set up") };
        if axo_list::item(ui, item, End::Action(action, false)).clicked() && !no_key {
            if set_up {
                app.settings.page = Page::LocalBackups;
            } else {
                authorize(app, "enable-backups", Page::LocalBackupsSetupFolder);
            }
        }
    });
}

fn description(ui: &mut Ui, palette: &Palette, text: &str) {
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 4, bottom: 12 }).show(ui, |ui| {
        ui.label(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary));
    });
}

/// Desktop's "Last backup" time: `dateStyle: 'medium', timeStyle: 'short'`.
fn last_backup_text(last: Option<&(i64, PathBuf)>) -> String {
    use chrono::TimeZone as _;
    match last.and_then(|(at, _)| chrono::Local.timestamp_millis_opt(*at).single()) {
        Some(time) => crate::format::localized(&time, "%b %-d, %Y, %-I:%M %p"),
        None => tr("Never").into(),
    }
}

/// Desktop's local backups page, once set up.
pub fn manage_page(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    ask(app);
    let Some(stored) = app.settings.backups.loaded.clone() else { return };
    // Desktop sends an unfinished setup back to its step.
    if app.settings.backups.turning_off.is_none() {
        if stored.folder.is_none() {
            app.settings.page = Page::LocalBackupsSetupFolder;
            return;
        }
        if stored.viewed_hash.is_none() {
            app.settings.page = Page::LocalBackupsSetupKey;
            return;
        }
    }
    description(ui, &palette, tr("Your backup is end-to-end encrypted and stored on your computer."));
    let key_current = app.settings.backups.key_current();
    axo_list::list(ui, None, None, |ui| {
        let last = last_backup_text(stored.last.as_ref());
        let item = Item::new(tr("Last backup")).description(&last);
        if axo_list::item(ui, item, End::Action(tr("Back up now"), false)).clicked() {
            if key_current {
                start(app);
            } else {
                app.settings.backups.key_changed = true;
            }
        }
        let folder = stored.folder.clone().unwrap_or_default();
        let item = Item::new(tr("Backup folder")).description(&folder);
        if axo_list::item(ui, item, End::Action(tr("Show in folder"), false)).clicked() {
            crate::chat::open_externally(&folder);
        }
        let item = Item::new(tr("Recovery key")).description(tr("Your recovery key is a 64-character code used to restore your backup"));
        if axo_list::item(ui, item, End::Action(tr("View key"), false)).clicked() {
            if key_current {
                app.settings.backups.step = KeyStep::View;
                authorize(app, "view-aep", Page::LocalBackupsKeyReference);
            } else {
                app.settings.backups.key_changed = true;
            }
        }
        if axo_list::item(ui, Item::new(tr("Turn off backups")), End::Action(tr("Turn off"), true)).clicked() {
            app.settings.backups.turning_off = Some(true);
        }
    });
    axo_list::gap(ui);
    description(
        ui,
        &palette,
        tr("To restore a backup, install a new copy of Signal on your phone. Open the app and tap \u{201c}Restore or transfer.\u{201d}"),
    );
}

fn start(app: &mut App) {
    app.settings.backups.workflow = Some(LocalBackupProgress::Messages);
    app.send(Command::StartLocalBackup);
}

/// Desktop's `LocalBackupsSetupFolderPicker`.
pub fn folder_page(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    ask(app);
    if app.settings.backups.loaded.as_ref().is_some_and(|s| s.folder.is_some()) {
        app.settings.backups.step = KeyStep::View;
        app.settings.page = Page::LocalBackupsSetupKey;
        return;
    }
    setup_screen(ui, &palette, "folder", tr("Choose a folder"), tr("Choose a folder in your computer\u{2019}s storage where your end-to-end encrypted backup will be stored."), |ui| {
        if widgets::pill_button(ui, tr("Choose folder"), true).clicked()
            && let Some(parent) = rfd::FileDialog::new().set_title(tr("Choose a folder")).pick_folder()
        {
            app.send(Command::SetLocalBackupFolder { parent });
        }
    });
}

/// A setup step: Desktop's `Preferences--LocalBackupsSetupScreen`, an icon,
/// a header and its text, centered.
fn setup_screen(ui: &mut Ui, palette: &Palette, symbol: &str, header: &str, body: &str, below: impl FnOnce(&mut Ui)) {
    ui.add_space(24.);
    ui.vertical_centered(|ui| {
        ui.set_max_width(400.);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 32., palette.fill_secondary);
        theme::paint_symbol(ui.painter(), symbol, rect.center(), 32., palette.text);
        ui.add_space(16.);
        ui.label(RichText::new(header).font(theme::semibold(20.)));
        ui.add_space(8.);
        ui.label(RichText::new(body).font(theme::regular(13.)).color(palette.text_secondary));
        ui.add_space(20.);
        below(ui);
    });
}

/// Desktop's `LocalBackupsBackupKeyViewer`: view the key, type it back,
/// "Keep your key safe"; or, from the backups page, view it again.
pub fn key_page(app: &mut App, ui: &mut Ui, referencing: bool) {
    let palette = Palette::of(ui.ctx());
    ask(app);
    if app.settings.backups.key.is_none() {
        return;
    }
    let Some(Some(key)) = app.settings.backups.key.clone() else {
        ui.label(RichText::new("This device has no recovery key.").color(palette.text_secondary));
        return;
    };
    let shown = local_backup::format_key(&key, true);
    let step = app.settings.backups.step;
    let viewing = step == KeyStep::View || referencing;
    let (title, body) = if viewing {
        (
            tr("Your recovery key"),
            tr("This key is required to recover your account and data. Store it somewhere safe and never share it with anyone. Signal will never message you for your recovery key."),
        )
    } else {
        (tr("Confirm your recovery key"), tr("Enter the recovery key that you just recorded"))
    };
    let mut done = false;
    setup_screen(ui, &palette, "lock", title, body, |ui| {
        egui::Frame::new().fill(palette.fill).corner_radius(8).inner_margin(12).show(ui, |ui| {
            ui.set_width(360.);
            if viewing {
                ui.label(RichText::new(&shown).font(egui::FontId::monospace(16.)).color(palette.text));
            } else {
                let typed = &mut app.settings.backups.typed;
                let response = ui.add(
                    egui::TextEdit::multiline(typed)
                        .font(egui::FontId::monospace(16.))
                        .hint_text(tr("Enter key"))
                        .char_limit(79)
                        .frame(egui::Frame::NONE)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                if response.changed() {
                    // Desktop regroups what's typed as it goes.
                    *typed = local_backup::format_key(typed, false);
                }
            }
        });
        if viewing {
            ui.add_space(12.);
            if widgets::axo_button(ui, tr("Copy to clipboard"), widgets::Axo::SubtleSecondary).clicked() {
                app.settings.backups.do_not_share = true;
            }
        }
        ui.add_space(24.);
        ui.horizontal(|ui| {
            if !viewing && widgets::pill_button(ui, tr("See key again"), false).clicked() {
                app.settings.backups.step = KeyStep::View;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if referencing {
                    if widgets::pill_button(ui, tr("Done"), true).clicked() {
                        done = true;
                    }
                } else if viewing {
                    if widgets::pill_button(ui, tr("Next"), true).clicked() {
                        app.settings.backups.typed.clear();
                        app.settings.backups.step = KeyStep::Confirm;
                    }
                } else {
                    let matches = local_backup::key_matches(&app.settings.backups.typed, &key);
                    let next = ui.add_enabled_ui(matches, |ui| widgets::pill_button(ui, tr("Next"), true)).inner;
                    if matches && next.clicked() {
                        app.settings.backups.step = KeyStep::Caution;
                    }
                }
            });
        });
    });
    if done {
        confirmed(app);
    }
    if app.settings.backups.do_not_share {
        let ctx = ui.ctx().clone();
        let text = tr("Do not share your recovery key with anyone. This will let them take over your account. Signal will never message you for your recovery key. Never respond to a chat pretending to be Signal.");
        if dialog::alert(&ctx, egui::Id::unique("recovery-key-do-not-share"), tr("Do not share recovery key"), text, tr("Got it")) {
            app.settings.backups.do_not_share = false;
            copy_for_a_while(&ctx, shown.clone());
            app.toast(tr("Recovery key copied"));
        }
    }
    if app.settings.backups.step == KeyStep::Caution && !referencing {
        let ctx = ui.ctx().clone();
        // Desktop's `LocalBackupsConfirmKeyDialog`: Continue is the only way on.
        if dialog::alert(
            &ctx,
            egui::Id::unique("recovery-key-keep-safe"),
            tr("Keep your key safe"),
            tr("Signal will not be able to help you restore your backup if you lose your key. Store it somewhere safe and secure, and do not share it with others."),
            tr("Continue"),
        ) {
            confirmed(app);
        }
    }
}

/// Desktop's `onBackupKeyViewed`: the key's hash is kept, and the backups
/// page shows.
fn confirmed(app: &mut App) {
    app.send(Command::RecoveryKeyConfirmed);
    let state = &mut app.settings.backups;
    state.step = KeyStep::View;
    state.typed.clear();
    if let (Some(stored), Some(Some(key))) = (state.loaded.as_mut(), &state.key) {
        stored.viewed_hash = Some(local_backup::key_hash(key));
    }
    app.settings.page = Page::LocalBackups;
}

/// The dialogs over the backups pages: "Your recovery key has changed",
/// "Turn off backups", and the export's progress, completion or failure.
pub fn dialogs(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if app.settings.backups.key_changed {
        match dialog::show(
            &ctx,
            egui::Id::unique("recovery-key-changed"),
            tr("Your recovery key has changed"),
            tr("Your recovery key has been updated. Any new backups you make can only be restored using your new recovery key."),
            Some(tr("Cancel")),
            &[(tr("View new key"), Variant::StrongPrimary)],
        ) {
            Some(Answer::Action(_)) => {
                app.settings.backups.key_changed = false;
                app.settings.backups.step = KeyStep::View;
                app.settings.page = Page::LocalBackupsKeyReference;
            }
            Some(Answer::Cancel) => app.settings.backups.key_changed = false,
            None => {}
        }
    }
    if let Some(mut delete) = app.settings.backups.turning_off {
        let answer = dialog::show_with(
            &ctx,
            egui::Id::unique("turn-off-local-backups"),
            tr("Turn off backups"),
            tr("You will no longer be able to create backups on this computer without re-enabling."),
            Some(tr("Cancel")),
            &[(tr("Turn off"), Variant::StrongDestructive)],
            |ui| {
                ui.add_space(10.);
                widgets::checkbox(ui, &mut delete, tr("Delete my backup from this computer"));
            },
        );
        app.settings.backups.turning_off = Some(delete);
        match answer {
            Some(Answer::Action(_)) => {
                app.send(Command::DisableLocalBackups { delete });
                app.settings.backups.turning_off = None;
                app.settings.page = Page::Backups;
            }
            Some(Answer::Cancel) => app.settings.backups.turning_off = None,
            None => {}
        }
    }
    workflow(app, ui);
}

/// Desktop's `LocalBackupExportWorkflow`.
fn workflow(app: &mut App, ui: &mut Ui) {
    let Some(progress) = app.settings.backups.workflow.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    match progress {
        LocalBackupProgress::Messages | LocalBackupProgress::Attachments { .. } => {
            let fraction = match progress {
                LocalBackupProgress::Attachments { done, total } if total > 0 => Some(done as f32 / total as f32),
                LocalBackupProgress::Attachments { .. } => Some(0.),
                _ => None,
            };
            let answer = dialog::show_with(&ctx, egui::Id::unique("local-backup-progress"), tr("Exporting backup"), "", Some(tr("Cancel")), &[], |ui| {
                ui.add_space(12.);
                match fraction {
                    Some(fraction) => {
                        ui.add(egui::ProgressBar::new(fraction).desired_height(6.));
                        ui.add_space(8.);
                        if let LocalBackupProgress::Attachments { done, total } = progress {
                            let percent = format!("{:.0}%", fraction * 100.);
                            let text = tf(
                                "icu:PlaintextExport--ProgressDialog--Progress",
                                &format!("Exporting {} of {} ({percent})...", crate::format::file_size(done), crate::format::file_size(total)),
                                &[
                                    ("currentBytes", Arg::from(&crate::format::file_size(done))),
                                    ("totalBytes", Arg::from(&crate::format::file_size(total))),
                                    ("percentage", Arg::from(&percent)),
                                ],
                            );
                            ui.label(RichText::new(text).font(theme::semibold(12.)));
                        }
                    }
                    None => {
                        let t = ui.input(|i| i.time) as f32;
                        ui.add(egui::ProgressBar::new((t * 0.5).fract()).desired_height(6.));
                        ui.ctx().request_repaint();
                    }
                }
                ui.add_space(8.);
                ui.label(RichText::new(tr("This may take a few minutes")).font(theme::regular(12.)).color(palette.text_secondary));
            });
            if answer.is_some() {
                app.send(Command::CancelLocalBackup);
                app.settings.backups.workflow = None;
            }
        }
        LocalBackupProgress::Complete { .. } => {
            let folder = app.settings.backups.loaded.as_ref().and_then(|s| s.folder.clone());
            let answer = dialog::show_with(
                &ctx,
                egui::Id::unique("local-backup-complete"),
                tr("Backup export complete"),
                tr("To restore this backup:"),
                None,
                &[(tr("Show in folder"), Variant::StrongPrimary), (tr("OK"), Variant::StrongPrimary)],
                |ui| {
                    ui.add_space(12.);
                    for (symbol, text) in [
                        ("arrow-up-down", tr("Transfer your backup folder to your phone's storage")),
                        ("device-phone", tr("Install a new copy of Signal on your phone")),
                        ("folder", tr("Tap \u{201c}Restore or Transfer\u{201d} and choose \u{201c}On Device Backup\u{201d}")),
                    ] {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.), egui::Sense::hover());
                            theme::paint_symbol(ui.painter(), symbol, rect.center(), 18., palette.text);
                            ui.add_space(10.);
                            ui.add(egui::Label::new(RichText::new(text).font(theme::regular(13.))).wrap());
                        });
                        ui.add_space(10.);
                    }
                },
            );
            match answer {
                Some(Answer::Action(0)) => {
                    if let Some(folder) = folder {
                        crate::chat::open_externally(&folder);
                    }
                    app.settings.backups.workflow = None;
                }
                Some(_) => app.settings.backups.workflow = None,
                None => {}
            }
        }
        LocalBackupProgress::Failed(error) => {
            let (title, detail) = match error {
                LocalBackupError::NotEnoughStorage { bytes_needed } => (
                    tr("Not enough storage space"),
                    tf(
                        "icu:PlaintextExport--Error--NotEnoughStorage--Detail",
                        &format!("Your chat history can\u{2019}t be exported because your computer doesn\u{2019}t have enough free storage space. Free up {} of space and then try again.", crate::format::file_size(bytes_needed)),
                        &[("bytes", Arg::from(&crate::format::file_size(bytes_needed)))],
                    ),
                ),
                LocalBackupError::RanOutOfStorage { bytes_needed } => (
                    tr("Couldn\u{2019}t export chat history"),
                    tf(
                        "icu:PlaintextExport--Error--RanOutOfStorage--Detail",
                        &format!("Your chat history couldn\u{2019}t be exported because your computer doesn\u{2019}t have enough free storage space. Free up {} of space and then try again.", crate::format::file_size(bytes_needed)),
                        &[("bytes", Arg::from(&crate::format::file_size(bytes_needed)))],
                    ),
                ),
                LocalBackupError::Permissions => (
                    tr("Can\u{2019}t export chat history"),
                    tr("Your chat history can\u{2019}t be exported because Signal doesn\u{2019}t have permission to write files to disk. Try changing your disk's permissions or updating your system settings and then export again.").to_owned(),
                ),
                _ => (tr("Couldn\u{2019}t export chat history"), tr("An error occurred and your chat history could not be exported.").to_owned()),
            };
            if dialog::alert(&ctx, egui::Id::unique("local-backup-failed"), title, &detail, tr("OK")) {
                app.settings.backups.workflow = None;
            }
        }
    }
}
