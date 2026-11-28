//! The clipboard, straight from Wayland through wl-clipboard-rs. eframe's
//! own clipboard support pulls in an X11 backend, so it's left off and
//! [`Plugin`] stands in: egui's copy commands become Wayland selections,
//! and paste events are filled from the Wayland clipboard.

use std::io::Read;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use eframe::egui;
use wl_clipboard_rs::{copy, paste};

/// Kept in egui-winit's in-app fallback clipboard so that Ctrl+V always
/// produces a paste event for [`Plugin`] to fill.
const SEED: &str = "\u{0}wlclip";

fn read(clipboard: paste::ClipboardType, mime: paste::MimeType<'_>) -> Option<Vec<u8>> {
    let (mut pipe, _) = paste::get_contents(clipboard, paste::Seat::Unspecified, mime).ok()?;
    // A stuck clipboard owner mustn't hang the UI.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = tx.send(pipe.read_to_end(&mut buf).map(|_| buf));
    });
    rx.recv_timeout(Duration::from_secs(2)).ok()?.ok()
}

fn offer(clipboard: copy::ClipboardType, mime: copy::MimeType, data: Vec<u8>) -> bool {
    let mut opts = copy::Options::new();
    opts.clipboard(clipboard);
    opts.copy(copy::Source::Bytes(data.into()), mime).is_ok()
}

/// The clipboard's text, if it offers any.
pub fn text() -> Option<String> {
    read(paste::ClipboardType::Regular, paste::MimeType::Text).and_then(|b| String::from_utf8(b).ok())
}

fn primary_text() -> Option<String> {
    read(paste::ClipboardType::Primary, paste::MimeType::Text).and_then(|b| String::from_utf8(b).ok())
}

/// The clipboard's content as exactly `mime`.
#[allow(dead_code)]
pub fn get(mime: &str) -> Option<Vec<u8>> {
    read(paste::ClipboardType::Regular, paste::MimeType::Specific(mime))
}

/// The clipboard's image, decoded, if it offers one.
pub fn image() -> Option<image::RgbaImage> {
    let types = paste::get_mime_types(paste::ClipboardType::Regular, paste::Seat::Unspecified).ok()?;
    let mime = ["image/png", "image/jpeg", "image/webp", "image/gif"].into_iter().find(|m| types.contains(*m))?;
    let bytes = read(paste::ClipboardType::Regular, paste::MimeType::Specific(mime))?;
    image::load_from_memory(&bytes).ok().map(|i| i.into_rgba8())
}

/// Puts text on the clipboard; it's served until something else is copied.
pub fn set_text(text: &str) -> bool {
    offer(copy::ClipboardType::Regular, copy::MimeType::Text, text.as_bytes().to_vec())
}

/// Puts an image on the clipboard, as PNG.
pub fn set_image(image: &image::RgbaImage) -> bool {
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).is_ok()
        && offer(copy::ClipboardType::Regular, copy::MimeType::Specific("image/png".into()), png.into_inner())
}

/// Empties the clipboard.
#[allow(dead_code)]
pub fn clear() {
    let _ = copy::clear(copy::ClipboardType::Regular, copy::Seat::All);
}

/// What Ctrl+V pastes. A browser's "Copy image" also offers the image's
/// HTML as text; then the image wins.
fn paste_event() -> Option<egui::Event> {
    let text = text().map(|t| t.replace("\r\n", "\n")).filter(|t| !t.is_empty());
    let markup = text.as_deref().is_some_and(|t| t.trim_start().starts_with('<'));
    if !markup {
        if let Some(text) = text {
            return Some(egui::Event::Paste(text));
        }
    }
    if let Some(image) = image() {
        let size = [image.width() as usize, image.height() as usize];
        return Some(egui::Event::PasteImage(Arc::new(egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()))));
    }
    text.map(egui::Event::Paste)
}

/// Connects egui's copy and paste to the Wayland clipboard.
#[derive(Default)]
pub struct Plugin {
    seeded: bool,
}

impl egui::Plugin for Plugin {
    fn debug_name(&self) -> &'static str {
        "wlclip"
    }

    fn input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        let events = std::mem::take(&mut input.events);
        for event in events {
            match event {
                egui::Event::Paste(_) => input.events.extend(paste_event()),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Middle, pressed: true, .. } => {
                    input.events.push(event);
                    if let Some(text) = primary_text().filter(|t| !t.is_empty()) {
                        input.events.push(egui::Event::MiddleClickPaste { pos, text });
                    }
                }
                event => input.events.push(event),
            }
        }
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        for command in &output.platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) if text != SEED => {
                    set_text(text);
                }
                egui::OutputCommand::TextSelectionSettled(text) if !text.is_empty() => {
                    offer(copy::ClipboardType::Primary, copy::MimeType::Text, text.as_bytes().to_vec());
                }
                egui::OutputCommand::CopyImage(image) => {
                    let rgba = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    if let Some(image) = image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, rgba) {
                        set_image(&image);
                    }
                }
                _ => {}
            }
        }
        if !self.seeded {
            self.seeded = true;
            output.platform_output.commands.push(egui::OutputCommand::CopyText(SEED.into()));
        }
    }
}
