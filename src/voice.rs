//! Voice messages: Desktop's recording bar (`CompositionRecording`) in
//! place of the composer. The microphone is read through ffmpeg (from
//! PulseAudio, which PipeWire also serves, else ALSA) and encoded as AAC in
//! an M4A file, as Signal's voice messages are (`VOICE_CONTENT_TYPE`).
//! Without ffmpeg the mic button says so.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Ui, Vec2};
use langis_core::Command;

use crate::app::{App, PendingSend};
use crate::chat::ChatView;
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::i18n::tr;

/// The sample rate recorded and encoded (mono).
const RATE: u32 = 48_000;
/// Desktop's limit on a voice message (`MAX_AUDIO_DURATION`: one hour).
const MAX: Duration = Duration::from_secs(60 * 60);
/// Loudness samples kept for the waveform, one per 50 ms.
const LEVEL_EVERY: usize = (RATE / 20) as usize;

/// A recording in progress: ffmpeg reading the microphone, and ffmpeg
/// encoding what it reads.
/// A finished recording waiting to be sent or thrown away: its file (for
/// playing it back), its length and its levels.
pub struct Draft {
    bytes: Arc<Vec<u8>>,
    path: PathBuf,
    duration: f32,
    levels: Vec<f32>,
    _dir: tempfile::TempDir,
}

pub struct Recording {
    capture: Child,
    encoder: Child,
    pump: Option<std::thread::JoinHandle<()>>,
    file: PathBuf,
    started: Instant,
    /// Recent loudness, 0 to 1, newest last.
    levels: Arc<Mutex<Vec<f32>>>,
    _dir: tempfile::TempDir,
}

/// The microphone input ffmpeg reads: PulseAudio's default source, or
/// `LANGIS_GUI_VOICE_INPUT` ("format:device", for tests: `lavfi:sine`).
fn input() -> (String, String) {
    match std::env::var("LANGIS_GUI_VOICE_INPUT") {
        Ok(spec) => match spec.split_once(':') {
            Some((format, device)) => (format.to_owned(), device.to_owned()),
            None => ("pulse".into(), spec),
        },
        Err(_) => ("pulse".into(), "default".into()),
    }
}

impl Recording {
    pub(crate) fn start() -> Result<Self, String> {
        let dir = tempfile::Builder::new().prefix("langis-gui-voice-").tempdir().map_err(|e| e.to_string())?;
        let file = dir.path().join("voice.m4a");
        let (format, device) = input();
        let spawn_capture = |format: &str, device: &str| {
            let mut process = Process::new("ffmpeg");
            process.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
            // A generated test source runs in real time, like a microphone.
            if format == "lavfi" {
                process.arg("-re");
            }
            process
                .args(["-f", format, "-i", device, "-ac", tr("1"), "-ar", &RATE.to_string(), "-f", "s16le", "-"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        };
        let mut capture = spawn_capture(&format, &device).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { "Voice messages need ffmpeg installed.".to_owned() } else { format!("Couldn't start recording: {e}") }
        })?;
        // No PulseAudio server: try ALSA's default device.
        std::thread::sleep(Duration::from_millis(150));
        if format == "pulse" && capture.try_wait().ok().flatten().is_some() {
            capture = spawn_capture("alsa", "default").map_err(|e| format!("Couldn't start recording: {e}"))?;
        }
        let mut encoder = Process::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "s16le", "-ar", &RATE.to_string(), "-ac", tr("1"), "-i", "-", "-c:a", "aac", "-b:a", "64k", "-movflags", "+faststart"])
            .arg(&file)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Couldn't start recording: {e}"))?;
        let mut from = capture.stdout.take().ok_or("no capture output")?;
        let to: ChildStdin = encoder.stdin.take().ok_or("no encoder input")?;
        let levels = Arc::new(Mutex::new(Vec::new()));
        let pump = {
            let levels = levels.clone();
            std::thread::Builder::new()
                .name("voice-recording".into())
                .spawn(move || pump(&mut from, to, &levels))
                .map_err(|e| e.to_string())?
        };
        Ok(Self { capture, encoder, pump: Some(pump), file, started: Instant::now(), levels, _dir: dir })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stops and returns the encoded file's bytes.
    /// Stops and keeps the recording as a draft.
    fn into_draft(self) -> Result<Draft, String> {
        let duration = self.elapsed().as_secs_f32();
        let levels = self.levels.lock().unwrap().clone();
        let dir = tempfile::Builder::new().prefix("langis-gui-voice-").tempdir().map_err(|e| e.to_string())?;
        let bytes = self.finish()?;
        let path = dir.path().join("draft.m4a");
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(Draft { bytes: Arc::new(bytes), path, duration, levels, _dir: dir })
    }

    fn finish(mut self) -> Result<Vec<u8>, String> {
        let _ = self.capture.kill();
        let _ = self.capture.wait();
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
        let status = self.encoder.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("the recording couldn't be encoded".into());
        }
        std::fs::read(&self.file).map_err(|e| e.to_string())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.capture.kill();
        let _ = self.encoder.kill();
        let _ = self.capture.wait();
        let _ = self.encoder.wait();
    }
}

/// Copies the microphone's samples to the encoder, noting their loudness.
fn pump(from: &mut impl std::io::Read, mut to: ChildStdin, levels: &Mutex<Vec<f32>>) {
    let mut buf = vec![0u8; 8192];
    let (mut peak, mut count) = (0f32, 0usize);
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if to.write_all(&buf[..n]).is_err() {
            break;
        }
        for pair in buf[..n].chunks_exact(2) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]) as f32 / i16::MAX as f32;
            peak = peak.max(sample.abs());
            count += 1;
            if count == LEVEL_EVERY {
                let mut levels = levels.lock().unwrap();
                levels.push(peak.sqrt());
                let len = levels.len();
                if len > 400 {
                    levels.drain(..len - 400);
                }
                peak = 0.;
                count = 0;
            }
        }
    }
    // Closing the encoder's input ends its file.
    drop(to);
}

pub fn start(app: &mut App, chat: &mut ChatView) {
    match Recording::start() {
        Ok(recording) => chat.recording = Some(recording),
        Err(error) => app.toast(error),
    }
}

/// Stops the recording and sends it, replying if a reply was set.
/// Desktop's `CompositionRecordingDraft`: delete, play or pause with the
/// waveform and its time, then send.
pub fn draft_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let Some(draft) = chat.voice_draft.as_ref() else { return };
    const DRAFT_ID: i64 = -1;
    let mut discard = false;
    let mut go = false;
    let player = app.audio.active(DRAFT_ID).map(|p| (p.playing(), p.position()));
    let (playing, position) = player.unwrap_or((false, 0.));
    ui.horizontal(|ui| {
        if crate::widgets::icon_button(ui, Icon::Trash, 20., tr("Discard")).clicked() {
            discard = true;
        }
        let width = ui.available_width() - 48.;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 36.), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(18), palette.bubble_in);
        let play = Rect::from_center_size(Pos2::new(rect.left() + 20., rect.center().y), Vec2::splat(28.));
        let pr = ui.interact(play, ui.scope_id().with("draft-play"), Sense::click());
        ui.painter().circle_filled(play.center(), 14., palette.fill_pressed);
        theme::paint_symbol(ui.painter(), if playing { "pause-fill" } else { "play-fill" }, play.center(), 14., palette.text);
        if pr.on_hover_text(if playing { tr("Pause") } else { tr("Play") }).clicked() {
            let path = draft.path.to_string_lossy().into_owned();
            app.audio.toggle(DRAFT_ID, &c.id, tr("You").into(), &path, draft.duration, None);
        }
        // The levels in bars, played ones in the text color.
        let bars = Rect::from_min_max(Pos2::new(play.right() + 10., rect.top() + 8.), Pos2::new(rect.right() - 60., rect.bottom() - 8.));
        let n = ((bars.width() / 4.) as usize).max(1);
        let peaks: Vec<f32> = (0..n).map(|i| {
            let a = i * draft.levels.len() / n;
            let b = ((i + 1) * draft.levels.len() / n).max(a + 1).min(draft.levels.len());
            draft.levels.get(a..b).map(|s| s.iter().copied().fold(0., f32::max)).unwrap_or(0.)
        }).collect();
        let done_to = if draft.duration > 0. { position / draft.duration } else { 0. };
        for (i, level) in peaks.iter().enumerate() {
            let x = bars.left() + i as f32 * 4.;
            let color = if (i as f32 / n as f32) < done_to { palette.text } else { palette.text_secondary.gamma_multiply(0.6) };
            ui.painter().rect_filled(Rect::from_center_size(Pos2::new(x + 1., bars.center().y), Vec2::new(2., (level * bars.height()).max(2.))), CornerRadius::same(1), color);
        }
        let left = if playing { draft.duration - position } else { draft.duration };
        let secs = left.max(0.).round() as u32;
        ui.painter().text(Pos2::new(rect.right() - 16., rect.center().y), egui::Align2::RIGHT_CENTER, format!("{}:{:02}", secs / 60, secs % 60), theme::medium(13.), palette.text_secondary);
        let (send_rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
        ui.painter().circle_filled(send_rect.center(), 16., palette.badge);
        Icon::SendFill.paint(ui, send_rect, 18., Color32::WHITE);
        if r.on_hover_text("Send").clicked() {
            go = true;
        }
    });
    if playing {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
    // Desktop's time-limit question (`voiceRecordingInterruptedMax`).
    let max_id = egui::Id::unique("voice-max");
    if ui.ctx().data(|d| d.get_temp::<bool>(max_id)).unwrap_or(false) {
        use crate::dialog::{Answer, Variant};
        match crate::dialog::show(ui.ctx(), max_id.with("dialog"), "", tr("Voice message recording stopped because the maximum time limit was reached."), Some(tr("Discard")), &[(tr("Send anyway"), Variant::StrongPrimary)]) {
            Some(Answer::Action(_)) => {
                ui.ctx().data_mut(|d| d.remove::<bool>(max_id));
                go = true;
            }
            Some(Answer::Cancel) => {
                ui.ctx().data_mut(|d| d.remove::<bool>(max_id));
                discard = true;
            }
            None => {}
        }
    }
    if discard || go {
        if app.audio.active(DRAFT_ID).is_some() {
            app.audio.stop();
        }
        let draft = chat.voice_draft.take();
        if go && let Some(draft) = draft {
            let file = langis_core::OutgoingFile { bytes: draft.bytes, file_name: "voice.m4a".into(), voice: true };
            let command = Command::SendFile { conversation: c.id.clone(), file, caption: String::new(), quote: chat.quote.take(), mentions: Vec::new(), styles: Vec::new() };
            app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
            chat.scroll_to_bottom = 10;
        }
    }
}

fn send(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    let Some(recording) = chat.recording.take() else { return };
    if recording.elapsed() < Duration::from_millis(500) {
        app.toast("Hold on a little longer to record a voice message.");
        return;
    }
    match recording.finish() {
        Ok(bytes) => {
            let file = langis_core::OutgoingFile { bytes: Arc::new(bytes), file_name: "voice.m4a".into(), voice: true };
            let command = Command::SendFile { conversation: c.id.clone(), file, caption: String::new(), quote: chat.quote.take(), mentions: Vec::new(), styles: Vec::new() };
            app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
            chat.scroll_to_bottom = 10;
        }
        Err(error) => app.toast(format!("Couldn't send the voice message: {error}")),
    }
}

/// The bar while recording: cancel, the time and waveform, send.
pub fn recording_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let Some(recording) = chat.recording.as_mut() else { return };
    // ffmpeg stopped on its own: no microphone.
    if let Ok(Some(_)) = recording.capture.try_wait()
        && recording.levels.lock().unwrap().is_empty()
    {
        chat.recording = None;
        app.toast("Couldn't record: no microphone was found.");
        return;
    }
    let elapsed = recording.elapsed();
    let levels = recording.levels.lock().unwrap().clone();
    let mut cancel = false;
    let mut done = false;
    // Desktop keeps the recording as a draft when the window loses focus
    // (`saveDraftRecordingIfNeeded`), and stops at the time limit, asking
    // whether to send it.
    if !app.focused || elapsed >= MAX {
        let max = elapsed >= MAX;
        match chat.recording.take().map(Recording::into_draft) {
            Some(Ok(draft)) => {
                chat.voice_draft = Some(draft);
                if max {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("voice-max"), true));
                }
            }
            Some(Err(error)) => app.toast(format!("Couldn't keep the voice message: {error}")),
            None => {}
        }
        return;
    }
    ui.horizontal(|ui| {
        if crate::widgets::icon_button(ui, Icon::Trash, 20., tr("Cancel")).clicked() {
            cancel = true;
        }
        let width = ui.available_width() - 48.;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 36.), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(18), palette.bubble_in);
        // A blinking red dot and the time.
        let on = (elapsed.as_millis() / 600) % 2 == 0;
        let dot = Pos2::new(rect.left() + 18., rect.center().y);
        ui.painter().circle_filled(dot, 5., if on { palette.destructive } else { palette.destructive.gamma_multiply(0.4) });
        let secs = elapsed.as_secs();
        ui.painter().text(Pos2::new(rect.left() + 32., rect.center().y), egui::Align2::LEFT_CENTER, format!("{}:{:02}", secs / 60, secs % 60), theme::medium(14.), palette.text);
        // The waveform, newest on the right.
        let bars = Rect::from_min_max(Pos2::new(rect.left() + 84., rect.top() + 8.), Pos2::new(rect.right() - 16., rect.bottom() - 8.));
        let step = 4.;
        let n = (bars.width() / step) as usize;
        let shown = &levels[levels.len().saturating_sub(n)..];
        let start_x = bars.right() - shown.len() as f32 * step;
        for (i, level) in shown.iter().enumerate() {
            let h = (level * bars.height()).max(2.);
            let x = start_x + i as f32 * step;
            let bar = Rect::from_center_size(Pos2::new(x + 1., bars.center().y), Vec2::new(2., h));
            ui.painter().rect_filled(bar, CornerRadius::same(1), palette.text_secondary);
        }
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
        ui.painter().circle_filled(rect.center(), 16., palette.badge);
        Icon::SendFill.paint(ui, rect, 18., Color32::WHITE);
        if r.on_hover_text("Send").clicked() {
            done = true;
        }
    });
    ui.add_space(2.);
    ui.label(RichText::new("Recording a voice message").font(theme::regular(11.)).color(palette.text_secondary));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        cancel = true;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        done = true;
    }
    if cancel {
        chat.recording = None;
    } else if done {
        send(app, chat, c);
    }
    ui.ctx().request_repaint_after(Duration::from_millis(50));
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use super::*;

    /// Records two seconds of a test tone through the same two ffmpeg
    /// processes and checks the file is AAC in an M4A container.
    #[test]
    #[ignore = "needs ffmpeg"]
    fn records_aac_in_m4a() {
        // SAFETY: the test runs alone (it's ignored unless asked for).
        unsafe { std::env::set_var("LANGIS_GUI_VOICE_INPUT", "lavfi:sine=frequency=440:sample_rate=48000") };
        let recording = Recording::start().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        assert!(!recording.levels.lock().unwrap().is_empty());
        let bytes = recording.finish().unwrap();
        assert!(bytes.len() > 1000, "{} bytes", bytes.len());
        // An MP4/M4A file: an "ftyp" box first.
        assert_eq!(&bytes[4..8], b"ftyp");
        let mut child = Process::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert_eq!(out.trim(), "aac");
    }

    #[test]
    #[ignore = "needs ffmpeg"]
    fn keeps_a_draft_to_play_back() {
        // SAFETY: the test runs alone (it's ignored unless asked for).
        unsafe { std::env::set_var("LANGIS_GUI_VOICE_INPUT", "lavfi:sine=frequency=440:sample_rate=48000") };
        let recording = Recording::start().unwrap();
        std::thread::sleep(Duration::from_secs(1));
        let draft = recording.into_draft().unwrap();
        assert!(draft.duration >= 1.);
        assert!(!draft.levels.is_empty());
        assert_eq!(std::fs::read(&draft.path).unwrap(), *draft.bytes);
    }
}
