//! Videos and GIFs played in the app, as Desktop plays them in `<video>`:
//! inline GIFs muted and looping in their bubbles, videos in the viewer and
//! in stories with their sound. `ffmpeg` decodes frames at the size they're
//! shown, paced to the video's own rate (`-re`), and its sound goes to
//! PipeWire's `pw-play` as voice messages do. Posters (a video's first
//! frame) are made once with `ffmpeg` and cached.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Context, TextureHandle, TextureOptions};

/// The child dies with us, so nothing keeps playing after langis-gui quits.
fn die_with_parent(command: &mut Command) -> &mut Command {
    // SAFETY: prctl is async-signal-safe; nothing else runs before exec.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        })
    }
}

/// What ffprobe says about a file: its size and length.
#[derive(Clone, Copy, Debug, Default)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub duration: f32,
    pub audio: bool,
}

pub fn probe(path: &str) -> Option<Probe> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,width,height:format=duration", "-of", "default=noprint_wrappers=1", "-protocol_whitelist", "file", path])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut p = Probe::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "width" if p.width == 0 => p.width = value.parse().unwrap_or(0),
            "height" if p.height == 0 => p.height = value.parse().unwrap_or(0),
            "duration" => p.duration = value.parse().unwrap_or(0.),
            "codec_type" if value == "audio" => p.audio = true,
            _ => {}
        }
    }
    (p.width > 0 && p.height > 0).then_some(p)
}

/// The size to decode at: fits `max` (in pixels), keeps the shape, and is
/// even, as the encoders' pixel formats want.
fn fit(w: u32, h: u32, max: (u32, u32)) -> (u32, u32) {
    let scale = (max.0 as f32 / w as f32).min(max.1 as f32 / h as f32).min(1.);
    let even = |v: f32| ((v.round() as u32).max(2) / 2) * 2;
    (even(w as f32 * scale), even(h as f32 * scale))
}

struct Frame {
    seq: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// One video playing (or paused) at one size.
pub struct Player {
    path: String,
    max: (u32, u32),
    pub looped: bool,
    pub muted: bool,
    probe: Arc<Mutex<Option<Probe>>>,
    frame: Arc<Mutex<Option<Frame>>>,
    shown: u64,
    texture: Option<TextureHandle>,
    /// Where it was when it last started or paused, and since when it runs.
    offset: f32,
    started: Option<Instant>,
    ended: Arc<AtomicBool>,
    children: Arc<Mutex<Vec<Child>>>,
    run: Arc<AtomicBool>,
    /// After a seek while paused: show one frame there, then stop.
    pause_after_frame: bool,
}

impl Player {
    /// Starts playing `path` from the start, decoded to fit `max` pixels.
    pub fn new(path: &str, max: (u32, u32), looped: bool, muted: bool) -> Self {
        let mut player = Self {
            path: path.to_owned(),
            max: (max.0.max(2), max.1.max(2)),
            looped,
            muted,
            probe: Arc::new(Mutex::new(None)),
            frame: Arc::new(Mutex::new(None)),
            shown: 0,
            texture: None,
            offset: 0.,
            started: None,
            ended: Arc::new(AtomicBool::new(false)),
            children: Arc::new(Mutex::new(Vec::new())),
            run: Arc::new(AtomicBool::new(false)),
            pause_after_frame: false,
        };
        player.start();
        player
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn probe(&self) -> Option<Probe> {
        *self.probe.lock().unwrap()
    }

    pub fn duration(&self) -> f32 {
        self.probe().map(|p| p.duration).unwrap_or(0.)
    }

    pub fn playing(&self) -> bool {
        self.started.is_some() && !self.ended()
    }

    /// It played to its end (never, looping).
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    /// Seconds in.
    pub fn position(&self) -> f32 {
        let run = self.started.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.);
        let d = self.duration();
        let at = self.offset + run;
        if self.looped && d > 0. { at % d } else if d > 0. { at.min(d) } else { at }
    }

    fn stop_children(&mut self) {
        self.run.store(false, Ordering::Relaxed);
        for mut child in self.children.lock().unwrap().drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn start(&mut self) {
        self.stop_children();
        self.ended.store(false, Ordering::Relaxed);
        let run = Arc::new(AtomicBool::new(true));
        self.run = run.clone();
        let (path, max, looped, muted, at) = (self.path.clone(), self.max, self.looped, self.muted, self.offset);
        let (probe_slot, frame_slot, ended, children) = (self.probe.clone(), self.frame.clone(), self.ended.clone(), self.children.clone());
        std::thread::spawn(move || {
            let known = *probe_slot.lock().unwrap();
            let Some(p) = known.or_else(|| probe(&path)) else {
                ended.store(true, Ordering::Relaxed);
                return;
            };
            *probe_slot.lock().unwrap() = Some(p);
            let (w, h) = fit(p.width, p.height, max);
            let mut video = Command::new("ffmpeg");
            video.args(["-nostdin", "-v", "error", "-re"]);
            if looped {
                video.args(["-stream_loop", "-1"]);
            }
            video
                .args(["-protocol_whitelist", "file", "-ss", &format!("{at:.3}"), "-i", &path, "-an"])
                .args(["-vf", &format!("scale={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let Ok(mut decoder) = die_with_parent(&mut video).spawn() else {
                tracing::warn!("couldn't run ffmpeg to play a video");
                ended.store(true, Ordering::Relaxed);
                return;
            };
            let stdout = decoder.stdout.take();
            if !run.load(Ordering::Relaxed) {
                let _ = decoder.kill();
                let _ = decoder.wait();
                return;
            }
            children.lock().unwrap().push(decoder);
            if !muted && p.audio {
                let sound = die_with_parent(
                    Command::new("ffmpeg")
                        .args(["-nostdin", "-v", "error", "-protocol_whitelist", "file", "-ss", &format!("{at:.3}"), "-i", &path, "-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "-"])
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null()),
                )
                .spawn();
                if let Ok(mut sound) = sound
                    && let Some(pcm) = sound.stdout.take()
                {
                    let out = die_with_parent(
                        Command::new("pw-play")
                            .args(["--raw", "--format", "f32", "--rate", "48000", "--channels", "2"])
                            .args(["-P", "{ application.name = langis media.role = Movie }", "-"])
                            .stdin(Stdio::from(pcm))
                            .stdout(Stdio::null())
                            .stderr(Stdio::null()),
                    )
                    .spawn();
                    let mut kids = children.lock().unwrap();
                    kids.push(sound);
                    if let Ok(out) = out {
                        kids.push(out);
                    }
                    // Paused or dropped while these started: they'd play on.
                    if !run.load(Ordering::Relaxed) {
                        for kid in kids.iter_mut() {
                            let _ = kid.kill();
                            let _ = kid.wait();
                        }
                        return;
                    }
                }
            }
            let Some(mut stdout) = stdout else { return };
            let size = (w * h * 4) as usize;
            let mut seq = 0;
            loop {
                let mut pixels = vec![0u8; size];
                if stdout.read_exact(&mut pixels).is_err() || !run.load(Ordering::Relaxed) {
                    break;
                }
                seq += 1;
                *frame_slot.lock().unwrap() = Some(Frame { seq, width: w, height: h, pixels });
            }
            if run.load(Ordering::Relaxed) {
                ended.store(true, Ordering::Relaxed);
            }
        });
        self.started = Some(Instant::now());
    }

    pub fn pause(&mut self) {
        if self.started.is_none() {
            return;
        }
        self.offset = self.position();
        self.started = None;
        self.stop_children();
    }

    pub fn resume(&mut self) {
        if self.started.is_some() && !self.ended() {
            return;
        }
        if self.ended() {
            self.offset = 0.;
        }
        self.start();
    }

    pub fn toggle(&mut self) {
        if self.playing() { self.pause() } else { self.resume() }
    }

    /// Goes to `seconds` in, playing on if it was.
    pub fn seek(&mut self, seconds: f32) {
        let playing = self.playing();
        self.offset = seconds.clamp(0., self.duration().max(0.));
        self.started = None;
        self.stop_children();
        if playing {
            self.start();
        } else {
            // One frame at the new place, then still.
            self.start();
            self.pause_after_frame = true;
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        if self.muted != muted {
            self.muted = muted;
            if self.playing() {
                self.offset = self.position();
                self.start();
            }
        }
    }

    /// The newest frame as a texture, uploaded when it changed; asks for
    /// another paint while playing.
    pub fn texture(&mut self, ctx: &Context) -> Option<&TextureHandle> {
        let fresh = {
            let slot = self.frame.lock().unwrap();
            slot.as_ref().filter(|f| f.seq != self.shown).map(|f| (f.seq, egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.pixels)))
        };
        if let Some((seq, image)) = fresh {
            match &mut self.texture {
                Some(t) => t.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture(format!("video:{}", self.path), image, TextureOptions::LINEAR)),
            }
            self.shown = seq;
            if self.pause_after_frame {
                self.pause_after_frame = false;
                self.started = None;
                self.stop_children();
            }
        }
        if self.playing() {
            ctx.request_repaint_after(Duration::from_millis(15));
        }
        self.texture.as_ref()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop_children();
    }
}

/// Inline players (muted, looping GIFs in bubbles), kept while drawn and
/// let go 2 seconds after.
#[derive(Default)]
struct Inline {
    players: HashMap<(String, u32, u32), (Player, Instant)>,
}

static INLINE: Mutex<Option<Inline>> = Mutex::new(None);

/// Draws `path` playing muted and looping into `rect` (a GIF in a bubble),
/// cropped to fill. False until its first frame is here.
pub fn inline(ui: &egui::Ui, path: &str, rect: egui::Rect, radius: egui::CornerRadius) -> bool {
    let ppp = ui.ctx().pixels_per_point();
    let max = ((rect.width() * ppp).ceil() as u32, (rect.height() * ppp).ceil() as u32);
    // Decode a little larger than the box's longer side, so cropping to
    // fill still has the pixels.
    // A power of two, so resizing the window doesn't start a player for
    // every pixel width.
    let side = max.0.max(max.1).max(64).next_power_of_two();
    let key = (path.to_owned(), side, side);
    let mut guard = INLINE.lock().unwrap();
    let inline = guard.get_or_insert_with(Default::default);
    let (player, used) = inline.players.entry(key).or_insert_with(|| (Player::new(path, (side, side), true, true), Instant::now()));
    *used = Instant::now();
    let Some(texture) = player.texture(ui.ctx()) else { return false };
    let size = texture.size_vec2();
    let (photo, frame) = (size.x / size.y.max(1.), rect.width() / rect.height().max(1.));
    let uv = if photo > frame {
        let keep = frame / photo;
        egui::Rect::from_min_max(egui::pos2((1. - keep) / 2., 0.), egui::pos2((1. + keep) / 2., 1.))
    } else {
        let keep = photo / frame;
        egui::Rect::from_min_max(egui::pos2(0., (1. - keep) / 2.), egui::pos2(1., (1. + keep) / 2.))
    };
    egui::Image::new((texture.id(), rect.size())).uv(uv).corner_radius(radius).paint_at(ui, rect);
    true
}

/// Lets go of inline players not drawn for 2 seconds; call once a frame.
pub fn gc() {
    if let Some(inline) = INLINE.lock().unwrap().as_mut() {
        inline.players.retain(|_, (_, used)| used.elapsed() < Duration::from_secs(2));
    }
}

/// Where posters are kept: a folder for this run only, in the session's
/// runtime directory (memory, private to the user, gone at logout), by the
/// video's path. A poster of a video that disappears or is deleted
/// doesn't outlive the app: [`clear_posters`] empties the folder at exit
/// and at start.
fn posters_dir() -> Option<PathBuf> {
    Some(dirs::runtime_dir().or_else(dirs::cache_dir)?.join("langis-gui").join("posters"))
}

fn poster_path(path: &str) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    Some(posters_dir()?.join(format!("{:016x}.jpg", hasher.finish())))
}

/// Deletes every poster made (and the folder older versions kept in the
/// cache directory).
pub fn clear_posters() {
    if let Some(dir) = posters_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    if let Some(old) = dirs::cache_dir().map(|d| d.join("langis-gui").join("posters")) {
        let _ = std::fs::remove_dir_all(old);
    }
    if let Some(making) = MAKING.lock().unwrap().as_mut() {
        making.clear();
    }
}

static MAKING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// A video's first frame as a still image, made in the background the
/// first time it's asked for (Desktop's video screenshot). `None` until
/// it's there; the caller asks again on a later frame.
pub fn poster(ctx: &Context, path: &str) -> Option<String> {
    let out = poster_path(path)?;
    if out.exists() {
        return Some(out.to_string_lossy().into_owned());
    }
    let mut making = MAKING.lock().unwrap();
    let making = making.get_or_insert_with(Default::default);
    if making.insert(path.to_owned()) {
        let (path, ctx) = (path.to_owned(), ctx.clone());
        std::thread::spawn(move || {
            if let Some(dir) = out.parent() {
                use std::os::unix::fs::DirBuilderExt;
                let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
            }
            let tmp = out.with_extension("part.jpg");
            let made = Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-y", "-protocol_whitelist", "file", "-i", &path, "-frames:v", "1", "-vf", "scale='min(800,iw)':-2", "-q:v", "4", &tmp.to_string_lossy()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if made {
                let _ = std::fs::rename(&tmp, &out);
                ctx.request_repaint();
            }
        });
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn plays_frames_at_the_size_asked() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=1", "-pix_fmt", "yuv420p", clip.to_str().unwrap()])
            .status()
            .unwrap();
        assert!(made.success());
        let path = clip.to_str().unwrap();
        let p = super::probe(path).expect("probed");
        assert_eq!((p.width, p.height), (320, 240));
        assert!((p.duration - 1.).abs() < 0.2, "{}", p.duration);

        let player = super::Player::new(path, (160, 160), false, true);
        let start = std::time::Instant::now();
        while player.frame.lock().unwrap().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let frame = player.frame.lock().unwrap();
        let frame = frame.as_ref().expect("a frame");
        assert_eq!((frame.width, frame.height), (160, 120));
        assert_eq!(frame.pixels.len(), 160 * 120 * 4);
    }

    #[test]
    fn decode_sizes_fit_and_stay_even() {
        assert_eq!(super::fit(1920, 1080, (300, 300)), (300, 168));
        assert_eq!(super::fit(640, 480, (2000, 2000)), (640, 480));
        assert_eq!(super::fit(101, 51, (1000, 1000)), (100, 50));
    }
}
