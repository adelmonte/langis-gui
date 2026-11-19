//! Animated stickers and images (APNG, animated WebP, GIF files), played
//! as Desktop's `<img>` plays them. The frames are decoded once, in the
//! background, at the size they're shown, then kept as textures while
//! drawn and let go 10 seconds after. A still picture plays as itself.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{TextureHandle, TextureOptions};
use image::AnimationDecoder;

/// Frames decoded, before they're textures.
struct Decoded {
    frames: Vec<(egui::ColorImage, Duration)>,
}

enum State {
    Decoding(Arc<Mutex<Option<Option<Decoded>>>>),
    /// Not animated (or unreadable): drawn as a still image by the caller.
    Still,
    Ready { frames: Vec<(TextureHandle, Duration)>, total: Duration },
}

struct Entry {
    state: State,
    used: Instant,
}

static CACHE: Mutex<Option<HashMap<(String, u32), Entry>>> = Mutex::new(None);

/// Whether a file may be animated, by its name or type.
pub fn maybe_animated(path: &str, content_type: Option<&str>) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(content_type, Some("image/gif" | "image/webp" | "image/apng" | "image/png"))
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".png")
        || lower.ends_with(".apng")
}

fn decode(path: &str, side: u32) -> Option<Decoded> {
    let bytes = std::fs::read(path).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let cursor = std::io::Cursor::new(&bytes);
    let frames: Vec<image::Frame> = match format {
        image::ImageFormat::Gif => image::codecs::gif::GifDecoder::new(cursor).ok()?.into_frames().collect_frames().ok()?,
        image::ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(cursor).ok()?;
            if !decoder.has_animation() {
                return None;
            }
            decoder.into_frames().collect_frames().ok()?
        }
        image::ImageFormat::Png => {
            let decoder = image::codecs::png::PngDecoder::new(cursor).ok()?;
            if !decoder.is_apng().ok()? {
                return None;
            }
            decoder.apng().ok()?.into_frames().collect_frames().ok()?
        }
        _ => return None,
    };
    if frames.len() < 2 {
        return None;
    }
    let frames = frames
        .into_iter()
        .take(300)
        .map(|f| {
            let (num, den) = f.delay().numer_denom_ms();
            // Browsers draw 0 and 10ms delays as 100ms.
            let ms = if den == 0 { 100 } else { num / den };
            let delay = Duration::from_millis(if ms <= 10 { 100 } else { ms as u64 });
            let buffer = f.into_buffer();
            let (w, h) = buffer.dimensions();
            let scale = (side as f32 / w.max(h) as f32).min(1.);
            let buffer = if scale < 1. {
                image::imageops::resize(&buffer, ((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1), image::imageops::FilterType::Triangle)
            } else {
                buffer
            };
            let image = egui::ColorImage::from_rgba_unmultiplied([buffer.width() as usize, buffer.height() as usize], buffer.as_raw());
            (image, delay)
        })
        .collect();
    Some(Decoded { frames })
}

/// Draws the animation at `path` in `rect` (fitted, its shape kept), and
/// true; false while it's decoding or when it isn't animated, for the
/// caller to draw it still.
pub fn paint(ui: &egui::Ui, path: &str, rect: egui::Rect) -> bool {
    let ctx = ui.ctx().clone();
    let side = ((rect.width().max(rect.height()) * ctx.pixels_per_point()).ceil() as u32).clamp(32, 512).next_power_of_two();
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(Default::default);
    let entry = cache.entry((path.to_owned(), side)).or_insert_with(|| {
        let slot = Arc::new(Mutex::new(None));
        let (out, path, ctx) = (slot.clone(), path.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            *out.lock().unwrap() = Some(decode(&path, side));
            ctx.request_repaint();
        });
        Entry { state: State::Decoding(slot), used: Instant::now() }
    });
    entry.used = Instant::now();
    if let State::Decoding(slot) = &entry.state {
        let done = slot.lock().unwrap().take();
        match done {
            None => return false,
            Some(None) => entry.state = State::Still,
            Some(Some(decoded)) => {
                let total = decoded.frames.iter().map(|(_, d)| *d).sum();
                let frames = decoded
                    .frames
                    .into_iter()
                    .enumerate()
                    .map(|(i, (image, delay))| (ctx.load_texture(format!("anim:{path}:{side}:{i}"), image, TextureOptions::LINEAR), delay))
                    .collect();
                entry.state = State::Ready { frames, total };
            }
        }
    }
    let State::Ready { frames, total } = &entry.state else { return false };
    // Which frame now, by the clock, looping.
    let now = ctx.input(|i| i.time);
    let total_ms = total.as_millis().max(1) as f64;
    let mut at = Duration::from_millis(((now * 1000.) % total_ms) as u64);
    let mut shown = &frames[0];
    for frame in frames {
        if at < frame.1 {
            shown = frame;
            break;
        }
        at -= frame.1;
    }
    ctx.request_repaint_after(shown.1.saturating_sub(at).max(Duration::from_millis(15)));
    let size = shown.0.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let fitted = egui::Rect::from_center_size(rect.center(), size * scale);
    egui::Image::new((shown.0.id(), fitted.size())).paint_at(ui, fitted);
    true
}

/// Lets go of animations not drawn for 10 seconds; call once a frame.
pub fn gc() {
    if let Some(cache) = CACHE.lock().unwrap().as_mut() {
        cache.retain(|_, e| e.used.elapsed() < Duration::from_secs(10));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_an_animated_gif_and_leaves_a_still_png() {
        use image::codecs::gif::{GifEncoder, Repeat};
        let dir = tempfile::tempdir().unwrap();
        let gif = dir.path().join("a.gif");
        {
            let file = std::fs::File::create(&gif).unwrap();
            let mut encoder = GifEncoder::new(file);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for shade in [0u8, 120, 240] {
                let buffer = image::RgbaImage::from_pixel(64, 32, image::Rgba([shade, 0, 0, 255]));
                encoder.encode_frame(image::Frame::from_parts(buffer, 0, 0, image::Delay::from_numer_denom_ms(50, 1))).unwrap();
            }
        }
        let decoded = super::decode(gif.to_str().unwrap(), 32).expect("animated");
        assert_eq!(decoded.frames.len(), 3);
        assert_eq!(decoded.frames[0].0.size, [32, 16]);
        assert_eq!(decoded.frames[0].1, std::time::Duration::from_millis(50));

        let png = dir.path().join("b.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255])).save(&png).unwrap();
        assert!(super::decode(png.to_str().unwrap(), 32).is_none());
    }
}
