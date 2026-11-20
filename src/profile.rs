//! Desktop's profile editor (from the nav avatar): photo, name, about,
//! username and username link.

use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, RichText, Sense, Ui};
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
    /// "Delete username" waiting on its confirmation.
    pub confirm_delete_username: bool,
    /// The About emoji's picker, while open.
    pub about_picker: Option<crate::emoji_picker::Picker>,
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
        confirm_delete_username: false,
        about_picker: None,
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
                    // Desktop's AvatarEditor crops to a square; this takes
                    // the middle square, at most 512 pixels.
                    match square_photo(&bytes) {
                        Some(cropped) => {
                            let preview = avatar_preview_path();
                            let _ = std::fs::write(&preview, &cropped);
                            ui.ctx().forget_image(&format!("file://{}", preview.display()));
                            e.photo = Some(preview.to_string_lossy().into_owned());
                            e.avatar = AvatarChange::New(std::sync::Arc::new(cropped));
                        }
                        None => app.toast("That picture couldn't be read."),
                    }
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
            // The About emoji from the picker, as Desktop's BioInput does.
            let face = if e.about_emoji.is_empty() { "🙂".to_owned() } else { e.about_emoji.clone() };
            let mut job = egui::text::LayoutJob::default();
            crate::chat::append_with_emoji(&mut job, &face, egui::TextFormat { font_id: theme::emoji(18.), color: palette.text, ..Default::default() });
            if ui.add(egui::Button::new(job).corner_radius(CornerRadius::same(8))).on_hover_text("Choose an emoji").clicked() {
                e.about_picker = if e.about_picker.is_some() { None } else { Some(Default::default()) };
            }
            widgets::text_field(ui, &mut e.about, "About", false);
        });
        if let Some(picker) = e.about_picker.as_mut() {
            ui.add_space(6.);
            if let Some(emoji) = picker.body(ui, &app.recent_emoji) {
                e.about_emoji = emoji.to_owned();
                e.about_picker = None;
            }
        }
        // Desktop's About presets.
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
            for (emoji, text) in [("👋", "Speak Freely"), ("🤐", "Encrypted"), ("👍", "Free to chat"), ("☕", "Coffee lover"), ("📴", "Taking a break")] {
                let mut job = egui::text::LayoutJob::default();
                crate::chat::append_with_emoji(&mut job, &format!("{emoji} {text}"), egui::TextFormat { font_id: theme::regular(12.), color: palette.text, ..Default::default() });
                if ui.add(egui::Button::new(job).corner_radius(CornerRadius::same(12))).clicked() {
                    e.about_emoji = emoji.to_owned();
                    e.about = text.to_owned();
                }
            }
            if (!e.about.is_empty() || !e.about_emoji.is_empty()) && ui.add(egui::Button::new(RichText::new("Clear").font(theme::regular(12.))).corner_radius(CornerRadius::same(12))).clicked() {
                e.about.clear();
                e.about_emoji.clear();
            }
        });
        ui.add_space(12.);
        ui.label(RichText::new("Username").font(theme::semibold(14.)));
        username(app, ui, &palette, &mut e);
        answer = buttons(ui, &["Save", "Cancel"]);
    });
    if e.confirm_delete_username {
        let name = e.username.clone().unwrap_or_default();
        let description = format!("This will remove your username and disable your QR code and link. \u{201c}{name}\u{201d} will be available for others to claim. Are you sure?");
        match crate::dialog::confirm(ui.ctx(), egui::Id::unique("delete-username"), "", &description, "Delete", crate::dialog::Variant::StrongDestructive) {
            Some(crate::dialog::Choice::Action) => {
                e.confirm_delete_username = false;
                app.send(Command::DeleteUsername);
            }
            Some(crate::dialog::Choice::Cancel) => e.confirm_delete_username = false,
            None => {}
        }
    }
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
                        if ui.link("QR code").clicked() {
                            open_link_screen(app);
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.link("Edit username").clicked() {
                            let (nickname, digits) = name.split_once('.').map(|(n, d)| (n.to_owned(), d.to_owned())).unwrap_or((name.clone(), String::new()));
                            e.editing_username = Some(UsernameEdit { nickname, digits, typed_at: Instant::now(), asked: None, reservation: None, problem: None, saving: false });
                        }
                        if ui.link("Delete username").clicked() {
                            e.confirm_delete_username = true;
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

/// Desktop's username link colors (`UsernameLinkEditor`'s `COLOR_MAP`), by
/// `UsernameLink.Color` 1 to 8: the QR code's color and the card's.
const LINK_COLORS: [(u32, u32); 8] = [
    (0x2449c0, 0x506ecd),
    (0x000000, 0xffffff),
    (0x464852, 0x6a6c75),
    (0x73694f, 0xaa9c7c),
    (0x55733f, 0x7c9b69),
    (0xd96b2d, 0xee691a),
    (0xbb617b, 0xf77099),
    (0x7651c5, 0xa183d4),
];

pub fn open_link_screen(app: &mut App) {
    app.panes.username_link = Some(None);
    let aci = app.aci.clone();
    app.load(move |db| async move {
        let p = langis_core::own_profile(&db, &aci).await;
        let found = p.username.zip(p.username_link);
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            if app.panes.username_link.is_some() {
                app.panes.username_link = Some(found);
            }
        })))
    });
}

/// Desktop's username link screen: the QR code on a card in our color,
/// the username under it, the color choices, Copy link and Reset.
pub fn link_screen(app: &mut App, ui: &mut Ui) {
    let Some(state) = app.panes.username_link.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let mut close = false;
    let mut color_pick = None;
    let mut reset = false;
    let closed = modal(ui, "username-link", 380., |ui| {
        title(ui, "Username link");
        let Some((name, link)) = &state else {
            ui.spinner();
            body(ui, &palette, "Set a username in your profile to get a link and QR code.");
            close = buttons(ui, &["Done"]).is_some();
            return;
        };
        let color = if (1..=8).contains(&link.color) { link.color } else { 1 };
        let (fg, bg) = LINK_COLORS[(color - 1) as usize];
        let fg = theme::rgb(fg);
        let bg = theme::rgb(bg);
        ui.vertical_centered(|ui| {
            let (card, _) = ui.allocate_exact_size(egui::vec2(260., 300.), Sense::hover());
            ui.painter().rect_filled(card, CornerRadius::same(24), bg);
            let qr_box = egui::Rect::from_center_size(egui::pos2(card.center().x, card.top() + 120.), egui::vec2(200., 200.));
            ui.painter().rect_filled(qr_box.expand(8.), CornerRadius::same(16), Color32::WHITE);
            if let Some(url) = link.url()
                && let Some(qr) = crate::startup::qr_texture_colored(ui.ctx(), &format!("username-link:{color}:{url}"), url.as_bytes(), fg)
            {
                egui::Image::new((qr.id(), qr_box.size())).paint_at(ui, qr_box);
            }
            let text = if bg == Color32::WHITE { Color32::BLACK } else { Color32::WHITE };
            ui.painter().text(egui::pos2(card.center().x, card.bottom() - 34.), egui::Align2::CENTER_CENTER, name, theme::semibold(16.), text);
            ui.add_space(12.);
            // The eight colors.
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 8. * 30.) / 2.);
                for (i, (_, swatch)) in LINK_COLORS.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(30., 30.), Sense::click());
                    ui.painter().circle_filled(r.center(), 11., theme::rgb(*swatch));
                    ui.painter().circle_stroke(r.center(), 11., egui::Stroke::new(1., palette.border));
                    if i as i32 + 1 == color {
                        ui.painter().circle_stroke(r.center(), 14., egui::Stroke::new(2., palette.text));
                    }
                    if resp.clicked() {
                        color_pick = Some(i as i32 + 1);
                    }
                }
            });
            ui.add_space(12.);
            ui.horizontal(|ui| {
                if let Some(url) = link.url()
                    && widgets::pill_button(ui, "Copy link", false).clicked()
                {
                    ui.ctx().copy_text(url);
                    app.toast("Username link copied.");
                }
                if widgets::pill_button(ui, "Reset", false).clicked() {
                    reset = true;
                }
            });
        });
        close = buttons(ui, &["Done"]).is_some();
    });
    if let Some(color) = color_pick {
        app.send(Command::SetUsernameLinkColor(color));
        if let Some(Some((_, link))) = app.panes.username_link.as_mut() {
            link.color = color;
        }
    }
    if reset {
        app.send(Command::ResetUsernameLink);
        app.toast("Resetting your username link. The old QR code and link won't work any more.");
        open_link_screen(app);
    }
    if close || closed {
        app.panes.username_link = None;
    }
}

/// The middle square of a picture, at most 512 pixels, as a JPEG.
fn square_photo(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let side = image.width().min(image.height());
    let x = (image.width() - side) / 2;
    let y = (image.height() - side) / 2;
    let square = image.crop_imm(x, y, side, side).resize_exact(side.min(512), side.min(512), image::imageops::FilterType::Lanczos3).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    square.write_to(&mut out, image::ImageFormat::Jpeg).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod photo_tests {
    #[test]
    fn photos_are_cropped_to_the_middle_square() {
        let wide = image::RgbImage::from_fn(800, 400, |x, _| if x < 200 || x >= 600 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) });
        let mut png = std::io::Cursor::new(Vec::new());
        wide.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let out = super::square_photo(&png.into_inner()).unwrap();
        let back = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (400, 400));
        // Only the blue middle is left.
        let edge = back.get_pixel(2, 200);
        assert!(edge[2] > 200 && edge[0] < 60, "{edge:?}");
    }
}

/// Where a picked avatar waits to be shown before it's saved: a private
/// (0700) folder of the app's own, not the shared /tmp.
fn avatar_preview_path() -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let dir = dirs::runtime_dir().or_else(dirs::cache_dir).unwrap_or_else(std::env::temp_dir).join("langis-gui");
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    dir.join("avatar-preview.jpg")
}

