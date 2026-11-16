//! Desktop's profile editor (from the nav avatar): photo, name, about,
//! username and username link.

use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use langis_core::Command;
use langis_core::profiles::{AvatarChange, ProfileEdit};
use langis_core::usernames::{Reservation, UsernameProblem};

use crate::app::{App, Loaded};
use crate::panes::{body, buttons, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;

pub struct ProfileEditor {
    pub loaded: bool,
    pub given: String,
    pub family: String,
    pub about: String,
    pub about_emoji: String,
    pub photo: Option<String>,
    pub avatar: AvatarChange,
    pub username: Option<String>,
    pub link: Option<String>,
    /// Editing the username: the nickname, the digits, and the reservation.
    pub editing_username: Option<UsernameEdit>,
}

pub struct UsernameEdit {
    pub nickname: String,
    pub digits: String,
    pub typed_at: Instant,
    pub asked: Option<(String, Option<String>)>,
    pub reservation: Option<Reservation>,
    pub problem: Option<String>,
    pub saving: bool,
}

impl ProfileEditor {
    pub fn on_reserved(&mut self, nickname: String, discriminator: Option<String>, result: Result<Reservation, UsernameProblem>) {
        let Some(edit) = self.editing_username.as_mut() else { return };
        if edit.asked.as_ref() != Some(&(nickname.clone(), discriminator)) {
            return;
        }
        match result {
            Ok(reservation) => {
                // Desktop fills in the digits the server picked.
                if let Some((_, digits)) = reservation.username.split_once('.')
                    && edit.digits.is_empty()
                {
                    edit.digits = digits.to_owned();
                    edit.asked = Some((nickname, Some(digits.to_owned())));
                }
                edit.reservation = Some(reservation);
                edit.problem = None;
            }
            Err(problem) => {
                edit.reservation = None;
                edit.problem = Some(problem.message(&nickname));
            }
        }
    }

    pub fn on_saved(&mut self, result: Result<Option<String>, UsernameProblem>) {
        match result {
            Ok(username) => {
                self.username = username;
                self.editing_username = None;
            }
            Err(problem) => {
                if let Some(edit) = self.editing_username.as_mut() {
                    edit.saving = false;
                    edit.problem = Some(problem.message(&edit.nickname));
                }
            }
        }
    }
}

/// Opens the editor, reading our profile first.
pub fn open(app: &mut App) {
    app.panes.profile = Some(ProfileEditor {
        loaded: false,
        given: String::new(),
        family: String::new(),
        about: String::new(),
        about_emoji: String::new(),
        photo: None,
        avatar: AvatarChange::Keep,
        username: None,
        link: None,
        editing_username: None,
    });
    let aci = app.aci.clone();
    app.load(move |db| async move {
        let p = langis_core::own_profile(&db, &aci).await;
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if let Some(e) = app.panes.profile.as_mut() {
                e.loaded = true;
                e.given = p.given_name;
                e.family = p.family_name;
                e.about = p.about;
                e.about_emoji = p.about_emoji;
                e.photo = p.photo.map(|p| p.to_string_lossy().into_owned());
                e.username = p.username;
                e.link = p.username_link.and_then(|l| l.url());
            }
        })))
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut e) = app.panes.profile.take() else { return };
    let palette = Palette::of(ui.ctx());
    let mut answer = None;
    let closed = modal(ui, "profile", 420., |ui| {
        title(ui, "Profile");
        if !e.loaded {
            ui.spinner();
            return;
        }
        ui.vertical_centered(|ui| {
            let name = format!("{} {}", e.given, e.family);
            let photo = match &e.avatar {
                AvatarChange::Remove => None,
                _ => e.photo.clone(),
            };
            let spec = widgets::AvatarSpec { name: name.trim(), color: 0, photo: photo.as_deref(), group: false, note_to_self: false };
            widgets::avatar(ui, &spec, 80.);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 160.).max(0.) / 2.);
                if ui.link("Edit photo").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg"]).pick_file()
                    && let Ok(bytes) = std::fs::read(&path)
                {
                    e.photo = Some(path.to_string_lossy().into_owned());
                    e.avatar = AvatarChange::New(std::sync::Arc::new(bytes));
                }
                if photo.is_some() && ui.link("Remove").clicked() {
                    e.avatar = AvatarChange::Remove;
                }
            });
        });
        ui.add_space(10.);
        widgets::text_field(ui, &mut e.given, "First name (required)", false);
        ui.add_space(6.);
        widgets::text_field(ui, &mut e.family, "Last name (optional)", false);
        ui.add_space(6.);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut e.about_emoji).hint_text("🙂").desired_width(32.));
            widgets::text_field(ui, &mut e.about, "About", false);
        });
        ui.add_space(12.);
        ui.label(RichText::new("Username").font(theme::semibold(14.)));
        username(app, ui, &palette, &mut e);
        answer = buttons(ui, &["Save", "Cancel"]);
    });
    match answer {
        Some(0) if e.given.trim().is_empty() => {
            app.toast("Your profile needs a first name.");
            app.panes.profile = Some(e);
        }
        Some(0) => {
            let edit = ProfileEdit {
                given_name: e.given.trim().to_owned(),
                family_name: e.family.trim().to_owned(),
                about: e.about.trim().to_owned(),
                about_emoji: e.about_emoji.trim().to_owned(),
                avatar: std::mem::replace(&mut e.avatar, AvatarChange::Keep),
            };
            app.send(Command::SetProfile(edit));
            // Kept open until the save is answered (Event::ProfileSaved).
            app.panes.profile = Some(e);
        }
        Some(_) => {}
        None if closed => {}
        None => app.panes.profile = Some(e),
    }
}

fn username(app: &mut App, ui: &mut Ui, palette: &Palette, e: &mut ProfileEditor) {
    match e.editing_username.as_mut() {
        None => {
            match &e.username {
                Some(name) => {
                    ui.label(RichText::new(name).font(theme::regular(14.)));
                    if let Some(link) = &e.link {
                        ui.horizontal(|ui| {
                            ui.set_max_width(372.);
                            ui.add(egui::Label::new(RichText::new(link).font(theme::regular(12.)).color(palette.text_secondary)).truncate());
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(link.clone());
                                app.toast("Username link copied.");
                            }
                        });
                        if ui.link("Reset link").clicked() {
                            app.send(Command::ResetUsernameLink);
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.link("Edit username").clicked() {
                            let (nickname, digits) = name.split_once('.').map(|(n, d)| (n.to_owned(), d.to_owned())).unwrap_or((name.clone(), String::new()));
                            e.editing_username = Some(UsernameEdit { nickname, digits, typed_at: Instant::now(), asked: None, reservation: None, problem: None, saving: false });
                        }
                        if ui.link("Delete username").clicked() {
                            app.send(Command::DeleteUsername);
                        }
                    });
                }
                None => {
                    body(ui, palette, "People can now message you using your optional username so you don't have to give out your phone number.");
                    if ui.link("Set up username").clicked() {
                        e.editing_username = Some(UsernameEdit { nickname: String::new(), digits: String::new(), typed_at: Instant::now(), asked: None, reservation: None, problem: None, saving: false });
                    }
                }
            }
        }
        Some(edit) => {
            ui.horizontal(|ui| {
                let a = ui.add(egui::TextEdit::singleline(&mut edit.nickname).hint_text("Username").desired_width(220.));
                ui.label(".");
                let b = ui.add(egui::TextEdit::singleline(&mut edit.digits).hint_text("00").desired_width(48.));
                if a.changed() || b.changed() {
                    edit.typed_at = Instant::now();
                    edit.reservation = None;
                    edit.problem = None;
                }
            });
            // Desktop reserves half a second after typing stops.
            let wanted = (edit.nickname.trim().to_lowercase(), Some(edit.digits.trim().to_owned()).filter(|d| !d.is_empty()));
            if !wanted.0.is_empty() && edit.typed_at.elapsed() > Duration::from_millis(500) && edit.asked.as_ref() != Some(&wanted) {
                edit.asked = Some(wanted.clone());
                app.send(Command::ReserveUsername { nickname: wanted.0, discriminator: wanted.1 });
            } else if !wanted.0.is_empty() {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            if let Some(problem) = &edit.problem {
                ui.label(RichText::new(problem).font(theme::regular(12.)).color(palette.destructive));
            } else if let Some(r) = &edit.reservation {
                ui.label(RichText::new(format!("{} is available", r.username)).font(theme::regular(12.)).color(palette.green));
            }
            let mut cancel = false;
            ui.horizontal(|ui| {
                let ready = edit.reservation.is_some() && !edit.saving;
                if ui.add_enabled(ready, egui::Button::new("Save username")).clicked()
                    && let Some(r) = edit.reservation.clone()
                {
                    edit.saving = true;
                    app.send(Command::ConfirmUsername(r));
                }
                cancel = ui.button("Cancel").clicked();
            });
            if cancel {
                e.editing_username = None;
            }
        }
    }
}
