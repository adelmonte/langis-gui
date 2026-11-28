//! Desktop's sticker pack creator (File > Create/upload sticker pack, its
//! sticker-creator app): add the pictures, an emoji for each, a title,
//! author and cover, then upload with `Command::UploadStickerPack` and
//! share the pack's link. Pictures are made 512px WebP here, as the
//! creator does before uploading.

use std::sync::Arc;

use egui::{CornerRadius, Pos2, Rect, RichText, Sense, Ui, Vec2};
use langis_core::Command;
use langis_core::sticker_upload::{Cover, MAX_STICKER_BYTES, MAX_STICKERS, NewSticker, NewStickerPack, StickerImage, StickerUpload};

use crate::app::App;
use crate::i18n::tr;
use crate::theme::{self, Palette};
use crate::widgets;

/// One picture, made ready to upload.
struct Sticker {
    webp: Arc<Vec<u8>>,
    emoji: String,
    /// For its texture's uri.
    key: u64,
}

#[derive(PartialEq, Clone, Copy)]
enum Stage {
    Drop,
    Emoji,
    Meta,
    Upload,
    Share,
}

pub struct Creator {
    stage: Stage,
    stickers: Vec<Sticker>,
    title: String,
    author: String,
    cover: usize,
    confirm: bool,
    progress: (usize, usize),
    url: Option<String>,
    next_key: u64,
}

impl Creator {
    pub fn new() -> Self {
        Self { stage: Stage::Drop, stickers: Vec::new(), title: String::new(), author: String::new(), cover: 0, confirm: false, progress: (0, 0), url: None, next_key: 0 }
    }

    /// `Event::StickerUpload`, while uploading.
    pub fn on_upload(&mut self, upload: &StickerUpload) -> Option<String> {
        match upload {
            StickerUpload::Progress { done, total } => self.progress = (*done, *total),
            StickerUpload::Done { url, .. } => {
                self.url = Some(url.clone());
                self.stage = Stage::Share;
            }
            StickerUpload::Failed(why) => {
                self.stage = Stage::Meta;
                return Some(format!("Error uploading images: {why}"));
            }
        }
        None
    }
}

/// A picture as the creator takes it: fitted into 512 by 512 on a clear
/// background, as lossless WebP. Err says why not (Desktop's toasts).
fn process(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let format = image::guess_format(bytes).map_err(|_| "Can\u{2019}t add this image because the format is not supported")?;
    if matches!(format, image::ImageFormat::Gif) {
        return Err("Animated art is not currently supported");
    }
    let img = image::load_from_memory(bytes).map_err(|_| "Error processing image")?.to_rgba8();
    let (w, h) = img.dimensions();
    let scale = 512. / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let fitted = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbaImage::new(512, 512);
    image::imageops::overlay(&mut canvas, &fitted, ((512 - nw) / 2) as i64, ((512 - nh) / 2) as i64);
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out).encode(canvas.as_raw(), 512, 512, image::ExtendedColorType::Rgba8).map_err(|_| "Error processing image")?;
    if out.len() > MAX_STICKER_BYTES {
        return Err("Dropped image is too large");
    }
    Ok(out)
}

fn thumb(ui: &Ui, s: &Sticker, rect: Rect) {
    egui::Image::from_bytes(format!("bytes://sticker-creator/{}.webp", s.key), s.webp.to_vec()).paint_at(ui, rect);
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut c) = app.fun.creator.take() else { return };
    let palette = Palette::of(ui.ctx());
    let mut close = false;
    let mut toast: Option<String> = None;
    let screen = ui.ctx().content_rect();
    egui::Area::new(egui::Id::unique("sticker-creator")).fixed_pos(screen.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        ui.set_min_size(screen.size());
        ui.painter().rect_filled(screen, 0, palette.background);
        let body = Rect::from_center_size(screen.center(), Vec2::new((screen.width() - 48.).min(720.), screen.height() - 48.));
        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sticker Pack Creator").font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if c.stage != Stage::Upload && widgets::icon_button(ui, crate::icons::Icon::X, 20., tr("Close")).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(16.);
            match c.stage {
                Stage::Drop => {
                    ui.label(RichText::new("Add your stickers").font(theme::semibold(16.)));
                    ui.label(
                        RichText::new("Stickers must be in PNG or WebP format with a transparent background and 512x512 pixels. Recommended margin is 16px.")
                            .font(theme::regular(13.))
                            .color(palette.text_secondary),
                    );
                    ui.add_space(4.);
                    ui.label(RichText::new(format!("{}/{MAX_STICKERS}", c.stickers.len())).font(theme::regular(12.)).color(palette.text_secondary));
                    ui.add_space(8.);
                    let mut remove = None;
                    let mut add = false;
                    egui::ScrollArea::vertical().max_height(body.height() - 160.).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(12.);
                            for (i, s) in c.stickers.iter().enumerate() {
                                let (r, hit) = ui.allocate_exact_size(Vec2::splat(96.), Sense::hover());
                                ui.painter().rect_filled(r, CornerRadius::same(10), palette.fill);
                                thumb(ui, s, r.shrink(6.));
                                if hit.hovered() {
                                    let x = Rect::from_center_size(Pos2::new(r.right() - 10., r.top() + 10.), Vec2::splat(20.));
                                    ui.painter().circle_filled(x.center(), 10., palette.text);
                                    theme::paint_symbol(ui.painter(), "x", x.center(), 12., palette.background);
                                    if ui.interact(x, ui.scope_id().with(("remove-sticker", i)), Sense::click()).on_hover_text("Remove image").clicked() {
                                        remove = Some(i);
                                    }
                                }
                            }
                            if c.stickers.len() < MAX_STICKERS {
                                let (r, hit) = ui.allocate_exact_size(Vec2::splat(96.), Sense::click());
                                ui.painter().rect_stroke(r, CornerRadius::same(10), egui::Stroke::new(1.5, palette.border), egui::StrokeKind::Inside);
                                theme::paint_symbol(ui.painter(), "plus", r.center(), 24., palette.text_secondary);
                                if hit.on_hover_text("Click to add or drop images here").clicked() {
                                    add = true;
                                }
                            }
                        });
                    });
                    if let Some(i) = remove {
                        c.stickers.remove(i);
                        c.cover = c.cover.min(c.stickers.len().saturating_sub(1));
                    }
                    // Picked, or dropped on the window.
                    let mut files: Vec<std::path::PathBuf> = app.drops.as_ref().map(|d| d.take()).unwrap_or_default().into_iter().filter(|p| p.exists()).collect();
                    if add && let Some(picked) = rfd::FileDialog::new().add_filter("Images", &["png", "webp", "jpg", "jpeg"]).pick_files() {
                        files.extend(picked);
                    }
                    let mut added = 0;
                    for path in files {
                        if c.stickers.len() >= MAX_STICKERS {
                            break;
                        }
                        match std::fs::read(&path).map_err(|_| "Error processing image").and_then(|b| process(&b)) {
                            Ok(webp) => {
                                c.next_key += 1;
                                c.stickers.push(Sticker { webp: Arc::new(webp), emoji: String::new(), key: c.next_key });
                                added += 1;
                            }
                            Err(why) => toast = Some(why.to_owned()),
                        }
                    }
                    if added > 0 && toast.is_none() {
                        toast = Some(if added == 1 { "1 image added".to_owned() } else { format!("{added} images added") });
                    }
                    if footer(ui, false, !c.stickers.is_empty()).is_some() {
                        c.stage = Stage::Emoji;
                    }
                }
                Stage::Emoji => {
                    ui.label(RichText::new("Add an emoji to each sticker").font(theme::semibold(16.)));
                    ui.label(RichText::new("This allows us to suggest stickers to you as you're messaging.").font(theme::regular(13.)).color(palette.text_secondary));
                    ui.add_space(8.);
                    egui::ScrollArea::vertical().max_height(body.height() - 160.).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(12.);
                            for s in &mut c.stickers {
                                ui.vertical(|ui| {
                                    let (r, _) = ui.allocate_exact_size(Vec2::splat(96.), Sense::hover());
                                    ui.painter().rect_filled(r, CornerRadius::same(10), palette.fill);
                                    thumb(ui, s, r.shrink(6.));
                                    ui.add(egui::TextEdit::singleline(&mut s.emoji).hint_text(tr("Emoji")).font(theme::emoji(18.)).desired_width(96.).align(egui::Align2::CENTER_CENTER));
                                });
                            }
                        });
                    });
                    let ready = c.stickers.iter().all(|s| langis_emoji::jumbo_count(s.emoji.trim()) == Some(1));
                    match footer(ui, true, ready) {
                        Some(true) => c.stage = Stage::Meta,
                        Some(false) => c.stage = Stage::Drop,
                        None => {}
                    }
                }
                Stage::Meta => {
                    ui.label(RichText::new("Just a few more details...").font(theme::semibold(16.)));
                    ui.add_space(10.);
                    ui.label(RichText::new(tr("Title")).font(theme::semibold(13.)));
                    widgets::text_field(ui, &mut c.title, "Name your sticker pack", false);
                    ui.add_space(8.);
                    ui.label(RichText::new("Author").font(theme::semibold(13.)));
                    widgets::text_field(ui, &mut c.author, "Enter a name to submit your stickers under", false);
                    ui.add_space(10.);
                    ui.label(RichText::new("Cover image").font(theme::semibold(13.)));
                    ui.label(RichText::new("This is the image that will show up when you share your sticker pack").font(theme::regular(12.)).color(palette.text_secondary));
                    ui.add_space(6.);
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, s) in c.stickers.iter().enumerate() {
                                let (r, hit) = ui.allocate_exact_size(Vec2::splat(72.), Sense::click());
                                ui.painter().rect_filled(r, CornerRadius::same(8), palette.fill);
                                thumb(ui, s, r.shrink(4.));
                                if i == c.cover {
                                    ui.painter().rect_stroke(r, CornerRadius::same(8), egui::Stroke::new(2., palette.accent), egui::StrokeKind::Inside);
                                }
                                if hit.clicked() {
                                    c.cover = i;
                                }
                            }
                        });
                    });
                    let ready = !c.title.trim().is_empty() && !c.author.trim().is_empty();
                    match footer(ui, true, ready) {
                        Some(true) => c.confirm = true,
                        Some(false) => c.stage = Stage::Emoji,
                        None => {}
                    }
                }
                Stage::Upload => {
                    ui.add_space(40.);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Creating your sticker pack").font(theme::semibold(16.)));
                        ui.add_space(12.);
                        widgets::spinner(ui, 32.);
                        ui.add_space(12.);
                        let (done, total) = c.progress;
                        ui.label(RichText::new(format!("{done} of {} uploaded", total.max(c.stickers.len()))).font(theme::regular(13.)).color(palette.text_secondary));
                    });
                }
                Stage::Share => {
                    ui.add_space(40.);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Congratulations! You created a sticker pack.").font(theme::semibold(16.)));
                        ui.add_space(6.);
                        ui.label(RichText::new("Access your new stickers through the sticker icon, or share with your friends using the link below.").font(theme::regular(13.)).color(palette.text_secondary));
                        ui.add_space(16.);
                        let url = c.url.clone().unwrap_or_default();
                        ui.label(RichText::new("Sticker Pack URL").font(theme::semibold(13.)));
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&url).font(theme::regular(12.))).truncate());
                            if widgets::pill_button(ui, tr("Copy"), false).clicked() {
                                ui.ctx().copy_text(url.clone());
                                toast = Some("Link copied".into());
                            }
                        });
                        ui.add_space(16.);
                        if widgets::pill_button(ui, "Create another sticker pack", true).clicked() {
                            c = Creator::new();
                        }
                    });
                }
            }
        });
    });
    // Desktop's last question before uploading.
    if c.confirm {
        use crate::dialog::{Answer, Variant};
        if let Some(answer) = crate::dialog::show(
            ui.ctx(),
            egui::Id::unique("sticker-creator-confirm"),
            "Are you sure you want to upload your sticker pack?",
            "You will no longer be able to make edits or delete after creating a sticker pack.",
            Some(tr("Cancel")),
            &[("Upload", Variant::StrongPrimary)],
        ) {
            c.confirm = false;
            if answer == Answer::Action(0) {
                let stickers = c.stickers.iter().map(|s| NewSticker { image: StickerImage { bytes: s.webp.to_vec(), content_type: "image/webp" }, emoji: s.emoji.trim().to_owned() }).collect();
                app.send(Command::UploadStickerPack(NewStickerPack { title: c.title.trim().to_owned(), author: c.author.trim().to_owned(), stickers, cover: Cover::Sticker(c.cover) }));
                c.progress = (0, c.stickers.len());
                c.stage = Stage::Upload;
            }
        }
    }
    if let Some(t) = toast {
        app.toast(t);
    }
    if !close {
        app.fun.creator = Some(c);
    }
}

/// The stage's buttons: Back (when `back`) and Next. Some(true) on Next,
/// Some(false) on Back.
fn footer(ui: &mut Ui, back: bool, ready: bool) -> Option<bool> {
    let mut out = None;
    ui.add_space(12.);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, "Next", true).clicked()).inner {
            out = Some(true);
        }
        if back && widgets::pill_button(ui, tr("Back"), false).clicked() {
            out = Some(false);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn pictures_become_512_webp() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_fn(300, 150, |x, _| image::Rgba([x as u8, 0, 0, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let webp = super::process(&png.into_inner()).unwrap();
        let img = image::load_from_memory(&webp).unwrap();
        assert_eq!((img.width(), img.height()), (512, 512));
        assert_eq!(img.to_rgba8().get_pixel(5, 5).0[3], 0, "letterboxed clear");
    }
}
