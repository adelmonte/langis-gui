//! Photos decoded at the size they're shown, and let go when they aren't.
//!
//! egui_extras decodes a `file://` image at full size and keeps it (and the
//! file's bytes) for good: a 12 megapixel photo holds 48 MB, plus its
//! texture, from the first time it's drawn. Every chat opened added its
//! photos. `thumb://{max}/{path}` decodes on a worker, shrinks the picture
//! to fit `max` pixels a side, and `evict` forgets what hasn't been drawn
//! for a while.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::{ColorImage, Context, Rect, Vec2};

const SCHEME: &str = "thumb://";
const WORKERS: usize = 4;

enum State {
    Pending,
    Ready(Arc<ColorImage>),
    Failed(String),
}

struct Entry {
    state: State,
    used: Instant,
}

type Job = (String, u32, String);

pub struct Thumbs {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    jobs: Mutex<Sender<Job>>,
}

/// The URI for `path` drawn at `size` points: its longer side in pixels,
/// rounded up to a power of two so nearby sizes share one decode.
pub fn uri(ctx: &Context, path: &str, size: Vec2) -> String {
    let px = (size.max_elem() * ctx.pixels_per_point()).ceil().max(64.) as u32;
    format!("{SCHEME}{}/{path}", px.next_power_of_two().min(2048))
}

/// The URI for `path` at full size, for the lightbox: shrunk only to
/// 4096 pixels a side, under every GPU's texture limit (a picture wider
/// than the limit made wgpu panic).
pub fn full_uri(path: &str) -> String {
    format!("{SCHEME}4096/{path}")
}

/// Whether `rect` is on screen in `ui`; images that aren't aren't drawn,
/// so they aren't decoded.
pub fn visible(ui: &egui::Ui, rect: Rect) -> bool {
    ui.is_rect_visible(rect)
}

fn decode(path: &str, max: u32) -> Result<ColorImage, String> {
    // A sender's picture: its header is checked before decoding, and the
    // decode may allocate at most 256 MB.
    let mut reader = image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 << 20);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let image = if image.width() > max || image.height() > max { image.thumbnail(max, max) } else { image };
    let rgba = image.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// Gives freed decode buffers back to the system; glibc keeps them
/// otherwise, and the process stays as big as its largest moment.
fn trim() {
    // SAFETY: malloc_trim only walks the allocator's own free lists.
    unsafe {
        libc::malloc_trim(0);
    }
}

impl Thumbs {
    pub fn install(ctx: &Context) -> Arc<Self> {
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let cache: Arc<Mutex<HashMap<String, Entry>>> = Arc::default();
        for _ in 0..WORKERS {
            let (rx, cache, ctx) = (rx.clone(), cache.clone(), ctx.clone());
            std::thread::spawn(move || worker(&rx, &cache, &ctx));
        }
        let thumbs = Arc::new(Self { cache, jobs: Mutex::new(tx) });
        ctx.add_image_loader(thumbs.clone());
        thumbs
    }

    /// Forgets what hasn't been drawn in `idle` (its picture and its
    /// texture).
    pub fn evict(&self, ctx: &Context, idle: Duration) {
        let stale: Vec<String> = match self.cache.lock() {
            Ok(cache) => cache.iter().filter(|(_, e)| !matches!(e.state, State::Pending) && e.used.elapsed() > idle).map(|(uri, _)| uri.clone()).collect(),
            Err(_) => return,
        };
        if stale.is_empty() {
            return;
        }
        for uri in &stale {
            ctx.forget_image(uri);
        }
        trim();
    }
}

fn worker(rx: &Mutex<Receiver<Job>>, cache: &Mutex<HashMap<String, Entry>>, ctx: &Context) {
    loop {
        let job = match rx.lock() {
            Ok(rx) => rx.recv(),
            Err(_) => return,
        };
        let Ok((uri, max, path)) = job else { return };
        let state = match decode(&path, max) {
            Ok(image) => State::Ready(Arc::new(image)),
            Err(error) => State::Failed(error),
        };
        if let Ok(mut cache) = cache.lock()
            && let Some(entry) = cache.get_mut(&uri)
        {
            entry.state = state;
        }
        trim();
        ctx.request_repaint();
    }
}

impl ImageLoader for Thumbs {
    fn id(&self) -> &str {
        "langis::thumbs"
    }

    fn load(&self, _ctx: &Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        let Some(rest) = uri.strip_prefix(SCHEME) else { return Err(LoadError::NotSupported) };
        let Some((max, path)) = rest.split_once('/') else { return Err(LoadError::NotSupported) };
        let max: u32 = max.parse().map_err(|_| LoadError::NotSupported)?;
        let mut cache = self.cache.lock().map_err(|_| LoadError::Loading("poisoned".into()))?;
        if let Some(entry) = cache.get_mut(uri) {
            entry.used = Instant::now();
            return match &entry.state {
                State::Pending => Ok(ImagePoll::Pending { size: None }),
                State::Ready(image) => Ok(ImagePoll::Ready { image: image.clone() }),
                State::Failed(error) => Err(LoadError::Loading(error.clone())),
            };
        }
        cache.insert(uri.to_owned(), Entry { state: State::Pending, used: Instant::now() });
        if let Ok(jobs) = self.jobs.lock() {
            let _ = jobs.send((uri.to_owned(), max, path.to_owned()));
        }
        Ok(ImagePoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(uri);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn byte_size(&self) -> usize {
        self.cache.lock().map(|c| c.values().map(|e| if let State::Ready(i) = &e.state { i.pixels.len() * 4 } else { 0 }).sum()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_through_egui_and_evicts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        image::RgbaImage::new(1600, 1200).save(&path).unwrap();
        let ctx = Context::default();
        let thumbs = Thumbs::install(&ctx);
        let uri = uri(&ctx, path.to_str().unwrap(), Vec2::new(300., 225.));
        assert!(uri.starts_with("thumb://512/"), "{uri}");
        let deadline = Instant::now() + Duration::from_secs(10);
        let image = loop {
            match ctx.try_load_image(&uri, SizeHint::default()) {
                Ok(ImagePoll::Ready { image }) => break image,
                Ok(ImagePoll::Pending { .. }) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                other => panic!("{:?}", other.err()),
            }
        };
        assert_eq!(image.size, [512, 384]);
        assert!(thumbs.byte_size() > 0);
        thumbs.evict(&ctx, Duration::ZERO);
        assert_eq!(thumbs.byte_size(), 0);
    }

    #[test]
    fn shrinks_to_the_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        image::RgbaImage::new(3000, 2000).save(&path).unwrap();
        let image = decode(path.to_str().unwrap(), 512).unwrap();
        assert_eq!(image.size, [512, 341]);
        let small = decode(path.to_str().unwrap(), 4096).unwrap();
        assert_eq!(small.size, [3000, 2000]);
    }
}
