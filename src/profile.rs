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
    /// Desktop's AvatarEditor, open.
    pub avatar_editor: Option<crate::avatar_editor::AvatarEditor>,
    /// What was saved, for Cancel on the name and About pages.
    pub saved: (String, String, String, String),
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
        avatar_editor: None,
        saved: Default::default(),
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
                e.saved = (e.given.clone(), e.family.clone(), e.about.clone(), e.about_emoji.clone());
                e.photo = p.photo.map(|p| p.to_string_lossy().into_owned());
                e.username = p.username;
                e.link = p.username_link.and_then(|l| l.url());
            }
        })))
    });
}

/// Desktop's name limit (`ProfileEditor`'s `maxLengthCount`), and when its
/// counter shows.
const NAME_MAX: usize = 26;
/// Desktop's About limit (`BioInput`).
const ABOUT_MAX: usize = 140;

/// Desktop's About presets (`DEFAULT_BIOS`).
const ABOUT_PRESETS: [(&str, &str); 7] = [
    ("👋", "Speak Freely"),
    ("🤐", "Encrypted"),
    ("🙏", "Be kind"),
    ("☕", "Coffee lover"),
    ("👍", "Free to chat"),
    ("📵", "Taking a break"),
    ("🚀", "Working on something new"),
];

fn send_profile(app: &mut App, e: &mut ProfileEditor, avatar: AvatarChange) {
    let edit = ProfileEdit {
        given_name: e.given.trim().to_owned(),
        family_name: e.family.trim().to_owned(),
        about: e.about.trim().to_owned(),
        about_emoji: e.about_emoji.trim().to_owned(),
        avatar,
    };
    app.send(Command::SetProfile(edit));
}

/// The editor's state for a settings page, reading our profile when it
/// isn't yet.
fn editor(app: &mut App) -> Option<ProfileEditor> {
    if app.panes.profile.is_none() {
        open(app);
    }
    app.panes.profile.take()
}

/// Desktop's `ProfileEditor` as a settings page: the photo with Edit
/// photo, the name and About rows, and the username with its link.
pub fn page(app: &mut App, ui: &mut Ui) {
    use crate::axo_list::{self, End, Item};
    let Some(mut e) = editor(app) else { return };
    let palette = Palette::of(ui.ctx());
    if !e.loaded {
        crate::widgets::spinner(ui, 24.);
        app.panes.profile = Some(e);
        return;
    }
    ui.add_space(16.);
    ui.vertical_centered(|ui| {
        let name = format!("{} {}", e.given, e.family);
        let spec = widgets::AvatarSpec { name: name.trim(), color: 0, photo: e.photo.as_deref(), group: false, note_to_self: false };
        if widgets::avatar(ui, &spec, 80.).interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            e.avatar_editor = Some(crate::avatar_editor::AvatarEditor::new(e.photo.clone(), name.trim()));
        }
        ui.add_space(12.);
        if widgets::axo_button(ui, "Edit photo", widgets::Axo::SubtleSecondary).clicked() {
            e.avatar_editor = Some(crate::avatar_editor::AvatarEditor::new(e.photo.clone(), name.trim()));
        }
    });
    ui.add_space(16.);
    let mut go = None;
    let full = format!("{} {}", e.given, e.family).trim().to_owned();
    let about = if e.about.is_empty() && e.about_emoji.is_empty() { "About".to_owned() } else { format!("{} {}", e.about_emoji, e.about).trim().to_owned() };
    axo_list::list(ui, None, Some("Your profile and changes to it will be visible to people you message, contacts and groups."), |ui| {
        if axo_list::item(ui, Item::new(&full).symbol("person"), End::Click).clicked() {
            go = Some(crate::settings::Page::ProfileName);
        }
        if axo_list::item(ui, Item::new(&about).symbol("pencil"), End::Click).clicked() {
            go = Some(crate::settings::Page::ProfileAbout);
        }
    });
    axo_list::gap(ui);
    let username = e.username.clone();
    axo_list::list(ui, None, Some("People can now message you using your optional username so you don't have to give out your phone number."), |ui| {
        let label = username.clone().unwrap_or_else(|| "Username".into());
        let row = axo_list::item(ui, Item::new(&label).symbol("at"), End::Click);
        let menu = egui::Id::unique("profile-username-menu");
        if row.clicked() {
            match &username {
                // Desktop's username row opens its menu once there's one.
                Some(_) => crate::menu::open_at(ui.ctx(), menu, row.row.rect.right_bottom() - egui::vec2(200., 0.)),
                None => e.editing_username = Some(UsernameEdit { nickname: String::new(), digits: String::new(), typed_at: Instant::now(), asked: None, reservation: None, problem: None, saving: false }),
            }
        }
        crate::menu::show(ui.ctx(), menu, |m| {
            if let Some(name) = &username {
                if m.item(Some("copy"), "Copy username") {
                    ui.ctx().copy_text(name.clone());
                    app.toast("Username copied");
                }
                if m.item(Some("pencil"), "Edit username") {
                    let (nickname, digits) = name.split_once('.').map(|(n, d)| (n.to_owned(), d.to_owned())).unwrap_or((name.clone(), String::new()));
                    e.editing_username = Some(UsernameEdit { nickname, digits, typed_at: Instant::now(), asked: None, reservation: None, problem: None, saving: false });
                }
                if m.item(Some("trash"), "Delete") {
                    e.confirm_delete_username = true;
                }
            }
        });
        if username.is_some() && axo_list::item(ui, Item::new("QR code or link").symbol("qrcode"), End::Click).clicked() {
            open_link_screen(app);
        }
    });
    let _ = palette;
    if let Some(page) = go {
        app.settings.page = page;
    }
    app.panes.profile = Some(e);
}

/// A field with Desktop's remaining-characters counter (shown as it nears
/// the limit).
fn counted_field(ui: &mut Ui, palette: &Palette, text: &mut String, hint: &str, max: usize, show_at: usize) -> egui::Response {
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).char_limit(max).desired_width(ui.available_width() - 40.).margin(egui::Margin::symmetric(10, 8)));
    let left = max.saturating_sub(text.chars().count());
    if left <= show_at {
        ui.painter().text(egui::pos2(r.rect.right() + 8., r.rect.center().y), egui::Align2::LEFT_CENTER, left.to_string(), theme::regular(12.), palette.text_secondary);
    }
    r
}

/// Desktop's name editor: first and last name with counters, Cancel and
/// Save.
pub fn name_page(app: &mut App, ui: &mut Ui) {
    let Some(mut e) = editor(app) else { return };
    let palette = Palette::of(ui.ctx());
    if e.loaded {
        ui.add_space(16.);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 0)).show(ui, |ui| {
            counted_field(ui, &palette, &mut e.given, "First name (required)", NAME_MAX, 9).request_focus();
            ui.add_space(12.);
            counted_field(ui, &palette, &mut e.family, "Last name (optional)", NAME_MAX, 9);
            let changed = (e.given.as_str(), e.family.as_str()) != (e.saved.0.as_str(), e.saved.1.as_str());
            ui.add_space(8.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = changed && !e.given.trim().is_empty();
                if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "Save", true).clicked()).inner && ok {
                    send_profile(app, &mut e, AvatarChange::Keep);
                }
                ui.add_space(8.);
                if widgets::pill_button(ui, "Cancel", false).clicked() {
                    e.given = e.saved.0.clone();
                    e.family = e.saved.1.clone();
                    app.settings.page = crate::settings::Page::Profile;
                }
            });
        });
    }
    app.panes.profile = Some(e);
}

/// Desktop's About editor: the emoji and text with its counter, the
/// presets as a list, Cancel and Save.
pub fn about_page(app: &mut App, ui: &mut Ui) {
    let Some(mut e) = editor(app) else { return };
    let palette = Palette::of(ui.ctx());
    if e.loaded {
        ui.add_space(16.);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let face = if e.about_emoji.is_empty() { "🙂".to_owned() } else { e.about_emoji.clone() };
                let mut job = egui::text::LayoutJob::default();
                crate::chat::append_with_emoji(&mut job, &face, egui::TextFormat { font_id: theme::emoji(18.), color: palette.text, ..Default::default() });
                if ui.add(egui::Button::new(job).corner_radius(CornerRadius::same(8)).min_size(egui::vec2(36., 36.))).on_hover_text("Choose an emoji").clicked() {
                    e.about_picker = if e.about_picker.is_some() { None } else { Some(Default::default()) };
                }
                counted_field(ui, &palette, &mut e.about, "Write a few words about yourself...", ABOUT_MAX, 30);
            });
            if let Some(picker) = e.about_picker.as_mut() {
                ui.add_space(6.);
                if let Some(emoji) = picker.body(ui, &app.recent_emoji) {
                    e.about_emoji = emoji.to_owned();
                    e.about_picker = None;
                }
            }
            ui.add_space(12.);
            for (emoji, text) in ABOUT_PRESETS {
                let (rect, hit) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.), Sense::click());
                if hit.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                }
                ui.painter().text(egui::pos2(rect.left() + 12., rect.center().y), egui::Align2::LEFT_CENTER, emoji, theme::emoji(18.), palette.text);
                ui.painter().text(egui::pos2(rect.left() + 44., rect.center().y), egui::Align2::LEFT_CENTER, text, theme::regular(14.), palette.text);
                if hit.clicked() {
                    e.about_emoji = emoji.to_owned();
                    e.about = text.to_owned();
                }
            }
            let changed = (e.about.as_str(), e.about_emoji.as_str()) != (e.saved.2.as_str(), e.saved.3.as_str());
            ui.add_space(12.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled_ui(changed, |ui| widgets::pill_button(ui, "Save", true).clicked()).inner && changed {
                    send_profile(app, &mut e, AvatarChange::Keep);
                }
                ui.add_space(8.);
                if widgets::pill_button(ui, "Cancel", false).clicked() {
                    e.about = e.saved.2.clone();
                    e.about_emoji = e.saved.3.clone();
                    app.settings.page = crate::settings::Page::Profile;
                }
            });
        });
    }
    app.panes.profile = Some(e);
}

/// The profile's dialogs: the avatar editor, Desktop's username editor
/// (`EditUsernameModalBody`) and Delete username.
pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut e) = app.panes.profile.take() else { return };
    let palette = Palette::of(ui.ctx());
    if let Some(editor) = e.avatar_editor.as_mut() {
        let mut toast = None;
        if let Some(change) = editor.show(ui, &mut toast) {
            e.avatar_editor = None;
            if !matches!(change, AvatarChange::Keep) {
                if let AvatarChange::New(bytes) = &change {
                    let preview = avatar_preview_path();
                    let _ = std::fs::write(&preview, bytes.as_slice());
                    ui.ctx().forget_image(&format!("file://{}", preview.display()));
                    e.photo = Some(preview.to_string_lossy().into_owned());
                } else {
                    e.photo = None;
                }
                send_profile(app, &mut e, change);
            }
        }
        if let Some(t) = toast {
            app.toast(t);
        }
    }
    if e.editing_username.is_some() {
        let mut done = false;
        let closed = modal(ui, "username-editor", 400., |ui| {
            title(ui, "Username");
            done = username(app, ui, &palette, &mut e);
        });
        if closed || done {
            e.editing_username = None;
        }
    }
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
    app.panes.profile = Some(e);
}

/// Desktop's `EditUsernameModalBody`: the nickname, a dot and its
/// numbers, the check as it's typed, Cancel and Done. True to close.
fn username(app: &mut App, ui: &mut Ui, palette: &Palette, e: &mut ProfileEditor) -> bool {
    let Some(edit) = e.editing_username.as_mut() else { return true };
    let field = egui::Frame::new().fill(palette.fill_secondary).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let a = ui.add(egui::TextEdit::singleline(&mut edit.nickname).hint_text("Username").frame(egui::Frame::NONE).desired_width(ui.available_width() - 70.));
            ui.label(RichText::new(".").font(theme::regular(14.)).color(palette.text_secondary));
            let b = ui.add(egui::TextEdit::singleline(&mut edit.digits).hint_text("00").frame(egui::Frame::NONE).char_limit(9).desired_width(48.));
            if a.changed() || b.changed() {
                edit.typed_at = Instant::now();
                edit.reservation = None;
                edit.problem = None;
            }
            a
        })
        .inner
    });
    if ui.memory(|m| m.focused().is_none()) {
        field.inner.request_focus();
    }
    // Desktop reserves half a second after typing stops.
    let wanted = (edit.nickname.trim().to_lowercase(), Some(edit.digits.trim().to_owned()).filter(|d| !d.is_empty()));
    if !wanted.0.is_empty() && edit.typed_at.elapsed() > Duration::from_millis(500) && edit.asked.as_ref() != Some(&wanted) {
        edit.asked = Some(wanted.clone());
        app.send(Command::ReserveUsername { nickname: wanted.0, discriminator: wanted.1 });
    } else if !wanted.0.is_empty() {
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }
    ui.add_space(6.);
    if let Some(problem) = &edit.problem {
        ui.label(RichText::new(problem).font(theme::regular(12.)).color(palette.destructive));
    } else {
        body(ui, palette, "Usernames are always paired with a set of numbers.");
    }
    let ready = edit.reservation.is_some() && !edit.saving;
    let mut close = false;
    ui.add_space(16.);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let label = if edit.saving { "…" } else { "Done" };
        if ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, label, true).clicked()).inner
            && ready
            && let Some(r) = edit.reservation.clone()
        {
            edit.saving = true;
            app.send(Command::ConfirmUsername(r));
        }
        ui.add_space(4.);
        if widgets::pill_button(ui, "Cancel", false).clicked() {
            close = true;
        }
    });
    close
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
            crate::widgets::spinner(ui, 24.);
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
pub fn square_photo(bytes: &[u8]) -> Option<Vec<u8>> {
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

