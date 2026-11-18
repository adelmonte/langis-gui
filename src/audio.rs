//! Voice messages and audio files played in the chat, as Desktop's
//! `MessageAudio`: waveforms worked out as `computeWaveform` does, and a
//! player that keeps going while you switch chats. `ffmpeg` decodes (and
//! changes the speed); PipeWire's `pw-play` plays.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// `MessageAudio`'s `BAR_COUNT`.
pub const BAR_COUNT: usize = 47;
/// `VoiceNotesPlaybackContext`'s `MAX_AUDIO_DURATION`: longer files get no
/// waveform.
const MAX_DURATION: f32 = 15. * 60.;
/// Decoded at this rate for the waveform; the bars are averages over
/// thousands of samples either way.
const WAVEFORM_RATE: u32 = 8000;
const PLAY_RATE: u32 = 48000;

/// `PlaybackRateButton`'s rates, in the order a click goes through them.
const RATES: [f32; 4] = [1., 1.5, 2., 0.5];

pub fn next_rate(rate: f32) -> f32 {
    let i = RATES.iter().position(|r| *r == rate).unwrap_or(0);
    RATES[(i + 1) % RATES.len()]
}

/// "1×", "1.5×", "2×", ".5×".
pub fn rate_label(rate: f32) -> &'static str {
    match rate {
        r if r == 1.5 => "1.5×",
        r if r == 2. => "2×",
        r if r == 0.5 => ".5×",
        _ => "1×",
    }
}

/// `durationToPlaybackText`: "0:07", "12:34", "1:02:03".
pub fn playback_text(seconds: f32) -> String {
    let s = seconds.max(0.) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[derive(Clone)]
pub struct Peaks {
    /// `BAR_COUNT` heights from 0 to 1.
    pub bars: Vec<f32>,
    pub duration: f32,
}

/// `WaveformBuilder`: at most 100 mean squares, halved and merged as
/// samples keep coming.
struct WaveformBuilder {
    waveform: [f64; 100],
    length: u64,
    shift: u32,
}

impl WaveformBuilder {
    fn new() -> Self {
        Self { waveform: [0.; 100], length: 0, shift: 0 }
    }

    fn push(&mut self, sample: f32) {
        let mut index = (self.length >> self.shift) as usize;
        if index >= 100 {
            for i in 0..50 {
                self.waveform[i] = (self.waveform[i * 2] + self.waveform[i * 2 + 1]) / 2.;
            }
            self.waveform[50..].fill(0.);
            self.shift += 1;
            index = 50;
        }
        self.waveform[index] += f64::from(sample).powi(2) / f64::from(1u32 << self.shift);
        self.length += 1;
    }

    fn collect(mut self) -> Vec<u8> {
        let mut length = (self.length >> self.shift) as usize;
        let last = self.length % (1u64 << self.shift);
        if last != 0 {
            self.waveform[length] = self.waveform[length] * f64::from(1u32 << self.shift) / last as f64;
            length += 1;
        }
        self.waveform[..length].iter().map(|m| to_peak(*m)).collect()
    }
}

/// `toPeak`: decibels over a -60 dB noise bed, scaled to 0 to 255.
fn to_peak(mean_square: f64) -> u8 {
    let value = ((10. * mean_square.log10()) + 60.).max(0.) / 60.;
    (value * 255.).round().clamp(0., 255.) as u8
}

/// `waveformToPeaks`: the waveform averaged into `bars` bars.
fn to_bars(waveform: &[u8], bars: usize) -> Vec<f32> {
    let stride = waveform.len() as f64 / bars as f64;
    (0..bars)
        .map(|i| {
            let end = ((i + 1) as f64 * stride).min(waveform.len() as f64);
            let (mut sum, mut count) = (0f64, 0f64);
            let mut j = (i as f64 * stride).round() as usize;
            while (j as f64) < end {
                sum += f64::from(waveform[j]) / 255.;
                count += 1.;
                j += 1;
            }
            (sum / (count + 1e-23)) as f32
        })
        .collect()
}

fn peaks_of(samples: &[f32]) -> Peaks {
    let duration = samples.len() as f32 / WAVEFORM_RATE as f32;
    if duration > MAX_DURATION {
        return Peaks { bars: vec![0.; BAR_COUNT], duration };
    }
    let mut builder = WaveformBuilder::new();
    for s in samples {
        builder.push(*s);
    }
    Peaks { bars: to_bars(&builder.collect(), BAR_COUNT), duration }
}

/// The child dies with us, so a voice message stops when langis does.
fn die_with_parent(command: &mut Command) -> &mut Command {
    // SAFETY: prctl is async-signal-safe; nothing else runs before exec.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        })
    }
}

fn decode(path: &str) -> Option<Vec<f32>> {
    let mut child = die_with_parent(
        Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i", path, "-vn", "-ac", "1", "-ar", &WAVEFORM_RATE.to_string(), "-f", "f32le", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .spawn()
    .ok()?;
    let mut bytes = Vec::new();
    child.stdout.take()?.read_to_end(&mut bytes).ok()?;
    child.wait().ok().filter(|s| s.success())?;
    Some(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// What's playing, from where and how fast.
pub struct Player {
    pub message_id: i64,
    pub conversation: langis_store::ConversationId,
    /// `getVoiceNoteTitle`: who sent it, or "You".
    pub title: String,
    path: String,
    pub duration: f32,
    pub rate: f32,
    /// Where it was when it last started or paused.
    offset: f32,
    started: Option<Instant>,
    children: Vec<Child>,
}

impl Player {
    pub fn playing(&self) -> bool {
        self.started.is_some()
    }

    /// Seconds in.
    pub fn position(&self) -> f32 {
        let run = self.started.map(|s| s.elapsed().as_secs_f32() * self.rate).unwrap_or(0.);
        (self.offset + run).min(self.duration)
    }

    fn stop_children(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.children.clear();
    }

    fn start(&mut self) {
        self.stop_children();
        let decoder = die_with_parent(
            Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-ss", &format!("{:.3}", self.offset), "-i", &self.path, "-vn"])
                .args(["-filter:a", &format!("atempo={}", self.rate), "-ac", "1", "-ar", &PLAY_RATE.to_string(), "-f", "f32le", "-"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        )
        .spawn();
        let Ok(mut decoder) = decoder else {
            tracing::warn!("couldn't run ffmpeg to play audio");
            return;
        };
        let Some(pcm) = decoder.stdout.take() else { return };
        let output = die_with_parent(
            Command::new("pw-play")
                .args(["--raw", "--format", "f32", "--rate", &PLAY_RATE.to_string(), "--channels", "1"])
                .args(["-P", "{ application.name = langis media.role = Music }", "-"])
                .stdin(Stdio::from(pcm))
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .spawn();
        self.children.push(decoder);
        match output {
            Ok(child) => self.children.push(child),
            Err(error) => {
                tracing::warn!(%error, "couldn't run pw-play to play audio");
                self.stop_children();
                return;
            }
        }
        self.started = Some(Instant::now());
    }

    fn pause(&mut self) {
        self.offset = self.position();
        self.started = None;
        self.stop_children();
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop_children();
    }
}

pub struct Audio {
    pub player: Option<Player>,
    /// The speed chosen last, for the next message too.
    rate: f32,
    peaks: Arc<Mutex<HashMap<String, Option<Peaks>>>>,
    computing: Mutex<HashSet<String>>,
}

impl Default for Audio {
    fn default() -> Self {
        Self { player: None, rate: 1., peaks: Default::default(), computing: Default::default() }
    }
}

impl Audio {
    /// The waveform of the file at `path`: worked out in the background the
    /// first time it's asked for (None until then, or if it can't be read).
    pub fn peaks(&self, ctx: &egui::Context, path: &str) -> Option<Peaks> {
        if let Some(found) = self.peaks.lock().ok()?.get(path) {
            return found.clone();
        }
        if self.computing.lock().ok()?.insert(path.to_owned()) {
            let (peaks, path, ctx) = (self.peaks.clone(), path.to_owned(), ctx.clone());
            std::thread::spawn(move || {
                let result = decode(&path).filter(|s| !s.is_empty()).map(|s| peaks_of(&s));
                if let Ok(mut peaks) = peaks.lock() {
                    peaks.insert(path, result);
                }
                ctx.request_repaint();
            });
        }
        None
    }

    pub fn active(&self, message_id: i64) -> Option<&Player> {
        self.player.as_ref().filter(|p| p.message_id == message_id)
    }

    /// Plays or pauses a message's audio; `at`, from 0 to 1, seeks.
    pub fn toggle(&mut self, message_id: i64, conversation: &langis_store::ConversationId, title: String, path: &str, duration: f32, at: Option<f32>) {
        match self.player.as_mut().filter(|p| p.message_id == message_id) {
            Some(player) => {
                if let Some(at) = at {
                    player.pause();
                    player.offset = (at * player.duration).clamp(0., player.duration);
                    player.start();
                } else if player.playing() {
                    player.pause();
                } else {
                    if player.position() >= player.duration - 0.05 {
                        player.offset = 0.;
                    }
                    player.start();
                }
            }
            None => {
                let rate = self.rate;
                let mut player = Player {
                    message_id,
                    conversation: conversation.clone(),
                    title,
                    path: path.to_owned(),
                    duration,
                    rate,
                    offset: at.map(|a| a * duration).unwrap_or(0.),
                    started: None,
                    children: Vec::new(),
                };
                player.start();
                self.player = Some(player);
            }
        }
    }

    /// The next speed, carrying on from where it is.
    pub fn cycle_rate(&mut self) {
        if let Some(player) = self.player.as_mut() {
            let playing = player.playing();
            player.pause();
            player.rate = next_rate(player.rate);
            self.rate = player.rate;
            if playing {
                player.start();
            }
        }
    }

    /// Called each frame: lets go of a finished player (returning what it
    /// played, to go on to the next), and asks for frames while one plays.
    pub fn tick(&mut self, ctx: &egui::Context) -> Option<(langis_store::ConversationId, i64)> {
        let player = self.player.as_mut()?;
        if !player.playing() {
            return None;
        }
        let output_done = player.children.last_mut().is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
        if player.position() >= player.duration || output_done {
            let finished = self.player.take().map(|p| (p.conversation.clone(), p.message_id));
            ctx.request_repaint();
            return finished;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        None
    }

    /// A player as if it were partway through, for the screenshots.
    #[cfg(test)]
    pub fn pretend(&mut self, message_id: i64, conversation: &langis_store::ConversationId, duration: f32, position: f32, rate: f32) {
        self.player = Some(Player {
            message_id,
            conversation: conversation.clone(),
            title: "Maya Chen".into(),
            path: String::new(),
            duration,
            rate,
            offset: position,
            started: Some(Instant::now() + std::time::Duration::from_secs(3600)),
            children: Vec::new(),
        });
    }

    pub fn stop(&mut self) {
        self.player = None;
    }

    pub fn pause(&mut self) {
        if let Some(player) = self.player.as_mut() {
            player.pause();
        }
    }

    pub fn resume(&mut self) {
        if let Some(player) = self.player.as_mut().filter(|p| !p.playing()) {
            player.start();
        }
    }
}

/// Desktop's `MiniPlayer`: under the chat's header while anything is
/// playing or paused: play or pause, who it's from and how far in, the
/// speed and close. True when shown.
pub fn mini_player(app: &mut crate::app::App, ui: &mut egui::Ui, palette: &crate::theme::Palette) -> bool {
    use egui::{Align2, CornerRadius, Pos2, Rect, Sense, Vec2};
    let Some(player) = app.audio.player.as_ref() else { return false };
    let (playing, rate) = (player.playing(), player.rate);
    let text = format!("{}  ·  {}", player.title, playback_text(player.position()));
    let mut toggle = false;
    let mut cycle = false;
    let mut close = false;
    let fill = if palette.dark { crate::theme::rgb(0x3b3b3b) } else { crate::theme::rgb(0xf6f6f6) };
    egui::Panel::top("mini-player").exact_size(36.).show_separator_line(false).frame(egui::Frame::new().fill(fill)).show(ui, |ui| {
        let rect = ui.max_rect();
        let mut x = rect.left() + 16.;
        let y = rect.center().y;
        let button = Rect::from_center_size(Pos2::new(x + 7., y), Vec2::splat(16.));
        let response = ui.interact(button, ui.scope_id().with("mini-play"), Sense::click()).on_hover_text(if playing { "Pause audio" } else { "Play audio" });
        crate::icons::Icon::paint(if playing { crate::icons::Icon::PauseFill } else { crate::icons::Icon::PlayFill }, ui, button, 16., palette.text);
        toggle = response.clicked();
        x += 14. + 18.;
        let close_rect = Rect::from_center_size(Pos2::new(rect.right() - 16. - 8., y), Vec2::splat(16.));
        let response = ui.interact(close_rect, ui.scope_id().with("mini-close"), Sense::click()).on_hover_text("Close");
        if response.hovered() {
            ui.painter().rect_filled(close_rect, CornerRadius::same(4), if palette.dark { crate::theme::rgb(0x2e2e2e) } else { crate::theme::rgb(0xe9e9e9) });
        }
        crate::icons::Icon::X.paint(ui, close_rect, 16., if palette.dark { crate::theme::rgb(0xdedede) } else { crate::theme::rgb(0x3b3b3b) });
        close = response.clicked();
        let mut right = close_rect.left() - 18.;
        if playing {
            let pill = Rect::from_min_size(Pos2::new(right - 38., y - 9.), Vec2::new(38., 18.));
            let (pill_fill, pill_text) = if palette.dark { (egui::Color32::from_white_alpha(20), crate::theme::rgb(0xb9b9b9)) } else { (egui::Color32::from_black_alpha(20), crate::theme::rgb(0x5e5e5e)) };
            ui.painter().rect_filled(pill, CornerRadius::same(4), pill_fill);
            ui.painter().text(pill.center(), Align2::CENTER_CENTER, rate_label(rate), crate::theme::semibold(11.), pill_text);
            cycle = ui.interact(pill, ui.scope_id().with("mini-rate"), Sense::click()).on_hover_text("Playback speed").clicked();
            right = pill.left() - 18.;
        }
        let mut job = egui::text::LayoutJob::simple_singleline(text, crate::theme::regular(14.), palette.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(0.));
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(Pos2::new(x, y - galley.size().y / 2.), galley, palette.text);
    });
    if toggle {
        if playing { app.audio.pause() } else { app.audio.resume() }
    }
    if cycle {
        app.audio.cycle_rate();
    }
    if close {
        app.audio.stop();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_cycle_like_desktop() {
        assert_eq!(next_rate(1.), 1.5);
        assert_eq!(next_rate(1.5), 2.);
        assert_eq!(next_rate(2.), 0.5);
        assert_eq!(next_rate(0.5), 1.);
        assert_eq!(rate_label(0.5), ".5×");
    }

    #[test]
    fn playback_text_formats() {
        assert_eq!(playback_text(7.4), "0:07");
        assert_eq!(playback_text(754.), "12:34");
        assert_eq!(playback_text(3723.), "1:02:03");
    }

    #[test]
    fn decodes_a_file_with_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.ogg");
        let made = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-c:a", "libopus"])
            .arg(&path)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("no ffmpeg with opus; skipped");
            return;
        }
        let peaks = peaks_of(&decode(path.to_str().unwrap()).unwrap());
        assert!((peaks.duration - 2.).abs() < 0.1, "{}", peaks.duration);
        assert!(peaks.bars.iter().all(|b| *b > 0.5), "{:?}", peaks.bars);
    }

    #[test]
    fn waveform_follows_loudness() {
        // A second of silence, then a second of a full-scale tone.
        let rate = WAVEFORM_RATE as usize;
        let mut samples = vec![0.; rate];
        samples.extend((0..rate).map(|i| (i as f32 * 0.1).sin()));
        let peaks = peaks_of(&samples);
        assert_eq!(peaks.bars.len(), BAR_COUNT);
        assert!((peaks.duration - 2.).abs() < 0.01);
        assert!(peaks.bars[2] < 0.05, "{:?}", peaks.bars);
        assert!(peaks.bars[BAR_COUNT - 3] > 0.9, "{:?}", peaks.bars);
    }
}
