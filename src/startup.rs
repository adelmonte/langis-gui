//! Before the chats: choosing how to keep the database key, unlocking it,
//! and linking to the phone (Desktop's install screen).

use egui::{Align, Color32, ColorImage, CornerRadius, Layout, RichText, TextureHandle, TextureOptions, Ui, Vec2};
use langis_core::keys::KeySource;
use langis_core::LinkError;

use crate::app::{App, Phase};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

/// The provisioning URL as a QR code texture, black on white.
pub fn qr_texture(ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
    qr_texture_of(ctx, "link-qr", url.as_bytes())
}

/// A QR code of any bytes (the safety number's scannable fingerprint too).
pub fn qr_texture_of(ctx: &egui::Context, name: &str, data: &[u8]) -> Option<TextureHandle> {
    qr_texture_colored(ctx, name, data, Color32::BLACK)
}

/// A QR code whose dark modules are `dark` (the username link's colors).
pub fn qr_texture_colored(ctx: &egui::Context, name: &str, data: &[u8], dark: Color32) -> Option<TextureHandle> {
    let code = qrcode::QrCode::new(data).ok()?;
    let width = code.width();
    let quiet = 2;
    let side = width + quiet * 2;
    let colors = code.to_colors();
    let mut pixels = vec![Color32::WHITE; side * side];
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == qrcode::Color::Dark {
                pixels[(y + quiet) * side + x + quiet] = dark;
            }
        }
    }
    let image = ColorImage::new([side, side], pixels);
    Some(ctx.load_texture(name, image, TextureOptions::NEAREST))
}

/// A centered card for the screens before the account is ready.
fn card(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let palette = Palette::of(ui.ctx());
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            let top = ((ui.available_height() - 420.) / 2.).max(24.);
            ui.add_space(top);
            ui.set_max_width(width);
            add(ui);
        });
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    let mut start = None;
    match &mut app.phase {
        Phase::Setup { passphrase, first, second, error } => card(ui, 420., |ui| {
            ui.label(RichText::new("Welcome to langis").font(theme::semibold(24.)));
            ui.add_space(8.);
            ui.label(
                RichText::new("Your messages are kept in an encrypted database on this computer. Choose where its key is kept.")
                    .color(palette.text_secondary),
            );
            ui.add_space(20.);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.radio_value(passphrase, false, "In a file only you can read (like Signal Desktop without a keyring)");
                ui.radio_value(passphrase, true, "Protected by a passphrase I type each time");
                if *passphrase {
                    ui.add_space(8.);
                    widgets::text_field(ui, first, "Passphrase", true);
                    widgets::text_field(ui, second, "Passphrase again", true);
                    ui.label(
                        RichText::new("If you forget it, this computer's copy of your history is lost. Your phone keeps its own.")
                            .font(theme::regular(12.))
                            .color(palette.text_secondary),
                    );
                }
                if let Some(error) = error {
                    ui.colored_label(palette.destructive, error.as_str());
                }
            });
            ui.add_space(20.);
            if widgets::pill_button(ui, tr("Continue"), true).clicked() {
                if !*passphrase {
                    start = Some(KeySource::File);
                } else if first.is_empty() {
                    *error = Some("Type a passphrase.".into());
                } else if first != second {
                    *error = Some("The passphrases don't match.".into());
                } else {
                    start = Some(KeySource::Passphrase(std::mem::take(first)));
                    second.clear();
                }
            }
        }),
        Phase::Unlock { passphrase, error } => card(ui, 360., |ui| {
            ui.label(RichText::new("Unlock langis").font(theme::semibold(24.)));
            ui.add_space(16.);
            let field = widgets::text_field(ui, passphrase, "Passphrase", true);
            field.request_focus();
            if let Some(error) = error {
                ui.colored_label(palette.destructive, error.as_str());
            }
            ui.add_space(16.);
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (widgets::pill_button(ui, "Unlock", true).clicked() || enter) && !passphrase.is_empty() {
                start = Some(KeySource::Passphrase(std::mem::take(passphrase)));
            }
        }),
        Phase::Starting => loading(ui, &palette),
        Phase::Syncing => syncing(ui, &palette),
        Phase::Failed(why) => card(ui, 520., |ui| {
            ui.label(RichText::new("langis-gui couldn't start").font(theme::semibold(20.)));
            ui.add_space(12.);
            ui.label(RichText::new(why.as_str()).color(palette.text_secondary));
        }),
        Phase::Linking { url, error, qr } => {
            let mut retry = false;
            let mut quit = false;
            match *error {
                Some(kind @ (LinkError::ConnectionFailed | LinkError::TooManyDevices | LinkError::TooOld)) => error_step(ui, kind, &mut retry, &mut quit),
                _ => linking(ui, &palette, url.is_some(), qr.as_ref(), *error, &mut retry),
            }
            if quit {
                let ctx = ui.ctx().clone();
                app.quit(&ctx);
            } else if retry {
                *error = None;
                *url = None;
                app.send(langis_core::Command::Link);
            }
        }
        Phase::Ready => {}
    }
    if let Some(source) = start {
        app.start(source);
    }
}

/// Desktop's install screen background: gray-02, or gray-95 in dark.
fn install_page(ui: &Ui) -> Color32 {
    if Palette::of(ui.ctx()).dark { theme::rgb(0x121212) } else { theme::rgb(0xf6f6f6) }
}

/// Desktop's `AppViewType.Blank` while starting: three pulsing dots, then
/// "Loading...".
fn loading(ui: &mut Ui, palette: &Palette) {
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let page = install_page(ui);
        ui.painter().rect_filled(ui.max_rect(), 0, page);
        let center = ui.max_rect().center();
        let t = ui.input(|i| i.time) as f32;
        for i in 0..3 {
            let phase = ((t * 3. - i as f32 * 0.5).sin() + 1.) / 2.;
            ui.painter().circle_filled(egui::Pos2::new(center.x - 24. + i as f32 * 24., center.y), 7., palette.text_secondary.gamma_multiply(0.3 + 0.7 * phase));
        }
        ui.painter().text(egui::Pos2::new(center.x, center.y + 32.), egui::Align2::CENTER_TOP, tr("Loading..."), theme::regular(14.), palette.text_secondary);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    });
}

/// Desktop's `InstallScreenLinkInProgressStep`: a 50px spinner, then
/// "Syncing contacts and groups" and its note.
fn syncing(ui: &mut Ui, palette: &Palette) {
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let page = install_page(ui);
        ui.painter().rect_filled(ui.max_rect(), 0, page);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() / 2. - 80.).max(24.));
            crate::widgets::spinner(ui, 50.);
            ui.add_space(24.);
            ui.label(RichText::new(tr("Syncing contacts and groups")).font(theme::semibold(20.)));
            ui.add_space(8.);
            ui.label(RichText::new(tr("Note: Your chat history will not be synced to this device")).font(theme::regular(14.)).color(palette.text_secondary));
        });
    });
}

const SUPPORT_PAGE: &str = "https://support.signal.org/hc/articles/360007320551-Linked-Devices";

/// Desktop's `InstallScreenErrorStep`: "Something went wrong!", what, and
/// its button, with Quit beside it when trying again won't help.
fn error_step(ui: &mut Ui, kind: LinkError, retry: &mut bool, quit: &mut bool) {
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let page = install_page(ui);
        ui.painter().rect_filled(ui.max_rect(), 0, page);
        let palette = Palette::of(ui.ctx());
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() / 2. - 80.).max(24.));
            ui.set_max_width(480.);
            ui.label(RichText::new(tr("Something went wrong!")).font(theme::semibold(24.)));
            ui.add_space(12.);
            let (body, update) = match kind {
                LinkError::TooManyDevices => (tr("Sorry, you have too many devices linked already. Try removing some."), false),
                LinkError::TooOld => (tr("Update Signal on this device to link your phone."), true),
                _ => (tr("Failed to connect to server."), false),
            };
            ui.add(egui::Label::new(RichText::new(body).font(theme::regular(16.)).color(palette.text_secondary)).wrap());
            ui.add_space(16.);
            // Desktop's Update goes to signal.org/download; a newer langis
            // comes from its own releases.
            let width = if update { 180. } else { 120. };
            ui.allocate_ui_with_layout(Vec2::new(width, 32.), Layout::left_to_right(Align::Center), |ui| {
                if update {
                    if widgets::pill_button(ui, tr("Update"), true).clicked() {
                        crate::chat::open_externally(RELEASES_PAGE);
                    }
                    ui.add_space(8.);
                    if widgets::pill_button(ui, tr("Quit"), false).clicked() {
                        *quit = true;
                    }
                } else if widgets::pill_button(ui, tr("Try again"), true).clicked() {
                    *retry = true;
                }
            });
        });
    });
}

const RELEASES_PAGE: &str = "https://github.com/adelmonte/langis-gui/releases";

/// Desktop's `InstallScreenQrCodeNotScannedStep`: one white card (radius 8,
/// padding 22, 44 in dark) holding the 256px code and, beside it, the
/// title, the numbered steps and "Need help?", all in black.
fn linking(ui: &mut Ui, palette: &Palette, has_url: bool, qr: Option<&TextureHandle>, error: Option<LinkError>, retry: &mut bool) {
    let _ = palette;
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let page = install_page(ui);
        ui.painter().rect_filled(ui.max_rect(), 0, page);
        let dark = Palette::of(ui.ctx()).dark;
        let pad = if dark { 44. } else { 22. };
        let black = Color32::BLACK;
        let blue = theme::rgb(0x2c6bed);
        let text_w = 360f32.min(ui.max_rect().width() - 2. * pad - 256. - 46. - 48.).max(200.);
        let size = Vec2::new(2. * pad + 8. + 256. + 38. + text_w, 2. * pad + 272.);
        let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
        ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::WHITE);
        let code = egui::Rect::from_min_size(rect.min + Vec2::new(pad + 8., pad + 8.), Vec2::splat(256.));
        match (qr, has_url, error) {
            (_, _, Some(kind)) => {
                // `__qr-code--load-failed`: a gray-05 outline and, by kind,
                // what happened and a way to retry
                // (`InstallScreenQrCodeNotScannedStep`'s `InstallScreenQrCode`).
                if kind != LinkError::Expired {
                    ui.painter().rect_stroke(code, CornerRadius::same(10), egui::Stroke::new(1.5, theme::rgb(0xe9e9e9)), egui::StrokeKind::Inside);
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(code.shrink(24.)), |ui| {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        let gray = theme::rgb(0x5e5e5e);
                        let text = |ui: &mut Ui, t: &str| {
                            ui.add(egui::Label::new(RichText::new(t).font(theme::regular(13.)).color(gray)).wrap());
                        };
                        let button = match kind {
                            // Max rotations: only a button, in the middle.
                            LinkError::Expired => {
                                ui.add_space(ui.available_height() / 2. - 10.);
                                tr("Refresh code")
                            }
                            LinkError::Timeout => {
                                ui.add_space(60.);
                                text(ui, tr("The QR code couldn't load. Check your connection and try again."));
                                tr("Retry")
                            }
                            _ => {
                                ui.add_space(40.);
                                theme::paint_symbol(ui.painter(), "error-circle", egui::Pos2::new(ui.max_rect().center().x, ui.cursor().top() + 11.), 22., theme::rgb(0xe51d0e));
                                ui.add_space(30.);
                                text(ui, tr("An unexpected error occurred."));
                                text(ui, tr("Please try again."));
                                tr("Retry")
                            }
                        };
                        ui.add_space(16.);
                        let r = ui.add(egui::Label::new(RichText::new(format!("\u{21bb} {button}")).font(theme::semibold(13.)).color(blue)).sense(egui::Sense::click()));
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            *retry = true;
                        }
                    });
                });
            }
            (Some(qr), _, None) => {
                egui::Image::new(qr).paint_at(ui, code);
            }
            _ => {
                crate::widgets::paint_spinner(ui, code.center(), 32., Color32::GRAY);
            }
        }
        let text = egui::Rect::from_min_max(egui::Pos2::new(code.right() + 38., rect.top() + pad), egui::Pos2::new(rect.right() - pad, rect.bottom() - pad));
        ui.scope_builder(egui::UiBuilder::new().max_rect(text), |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            ui.add_space((text.height() - 196.).max(0.) / 2.);
            let mut title = egui::text::LayoutJob::simple(tr("Scan this code in the Signal app on your phone").into(), theme::semibold(20.), black, text.width());
            title.sections[0].format.line_height = Some(26.);
            ui.label(title);
            ui.add_space(8.);
            let step = |ui: &mut Ui, parts: &[(&str, bool)]| {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = text.width();
                for (t, bold) in parts {
                    job.append(t, 0., egui::TextFormat { font_id: if *bold { theme::semibold(14.) } else { theme::regular(14.) }, color: black, line_height: Some(26.), ..Default::default() });
                }
                ui.label(job);
            };
            step(ui, &[("1. Open Signal on your phone", false)]);
            step(ui, &[("2. Tap into ", false), (tr("Settings"), true), (", then tap ", false), (tr("Linked Devices"), true)]);
            step(ui, &[("3. Tap ", false), (tr("Link a New Device"), true)]);
            ui.add_space(8.);
            let help = ui.add(egui::Label::new(RichText::new(tr("Need help?")).font(theme::regular(14.)).color(blue)).sense(egui::Sense::click()));
            if help.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                crate::chat::open_externally(SUPPORT_PAGE);
            }
        });
    });
}
