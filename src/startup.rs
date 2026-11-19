//! Before the chats: choosing how to keep the database key, unlocking it,
//! and linking to the phone (Desktop's install screen).

use egui::{Align, Color32, ColorImage, CornerRadius, Layout, RichText, TextureHandle, TextureOptions, Ui, Vec2};
use langis_core::keys::KeySource;

use crate::app::{App, Phase};
use crate::theme::{self, Palette};
use crate::widgets;

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
            if widgets::pill_button(ui, "Continue", true).clicked() {
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
        Phase::Starting => card(ui, 360., |ui| {
            ui.add_space(120.);
            ui.spinner();
        }),
        Phase::Failed(why) => card(ui, 520., |ui| {
            ui.label(RichText::new("langis-gui couldn't start").font(theme::semibold(20.)));
            ui.add_space(12.);
            ui.label(RichText::new(why.as_str()).color(palette.text_secondary));
        }),
        Phase::Linking { url, error, qr } => {
            let mut retry = false;
            linking(ui, &palette, url.is_some(), qr.as_ref(), error.as_deref(), &mut retry);
            if retry {
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

/// Desktop's "Scan QR code" step: the code on the left, steps on the right.
fn linking(ui: &mut Ui, palette: &Palette, has_url: bool, qr: Option<&TextureHandle>, error: Option<&str>, retry: &mut bool) {
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
        let size = Vec2::new(760., 360.);
        let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal(|ui| {
                // The code, on a white card in either theme.
                let (card, _) = ui.allocate_exact_size(Vec2::splat(296.), egui::Sense::hover());
                ui.painter().rect_filled(card, CornerRadius::same(24), Color32::WHITE);
                match (qr, has_url) {
                    (Some(qr), _) => {
                        egui::Image::new(qr).paint_at(ui, card.shrink(28.));
                    }
                    _ => {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
                            ui.centered_and_justified(|ui| {
                                if error.is_some() {
                                    ui.label(RichText::new("Couldn't get a code").color(Color32::BLACK));
                                } else {
                                    ui.add(egui::Spinner::new().color(Color32::GRAY));
                                }
                            });
                        });
                    }
                }
                ui.add_space(56.);
                ui.vertical(|ui| {
                    ui.add_space(24.);
                    ui.label(RichText::new("Scan QR code using the phone with your Signal account").font(theme::semibold(24.)));
                    ui.add_space(24.);
                    let steps = [
                        "Open Signal on your phone",
                        "Tap Settings, then Linked devices",
                        "Tap Link a new device and scan this code",
                    ];
                    for (n, step) in steps.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(Vec2::splat(24.), egui::Sense::hover());
                            ui.painter().circle_filled(dot.center(), 12., palette.fill_secondary);
                            ui.painter().text(dot.center(), egui::Align2::CENTER_CENTER, (n + 1).to_string(), theme::semibold(13.), palette.text);
                            ui.label(RichText::new(*step).font(theme::regular(16.)));
                        });
                        ui.add_space(10.);
                    }
                    if let Some(error) = error {
                        ui.add_space(8.);
                        ui.colored_label(palette.destructive, error);
                        if widgets::pill_button(ui, "Retry", true).clicked() {
                            *retry = true;
                        }
                    }
                });
            });
        });
    });
}
