//! The app's sounds, made here rather than taken from Desktop: a short
//! chime for a new message, a softer tick in the open chat, and a ringtone
//! for incoming calls. Each is synthesized as 48 kHz samples and played by
//! PipeWire's `pw-play`, as voice messages are.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

const RATE: u32 = 48_000;

/// Desktop's notification sound settings, kept as `ui.` settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// A sound with each notification (Desktop's `audioNotification`).
    pub notification: bool,
    /// A sound for messages in the open chat (`audioMessage`).
    pub in_chat: bool,
    /// The ringtone for incoming calls (`callRingtoneNotification`).
    pub ringtone: bool,
    /// A notification for incoming calls (`callSystemNotification`).
    pub call_notify: bool,
    /// Flash the window in the taskbar (`notificationDrawAttention`).
    pub attention: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { notification: true, in_chat: false, ringtone: true, call_notify: true, attention: false }
    }
}

pub const KEYS: [&str; 5] = ["ui.sound_notification", "ui.sound_in_chat", "ui.sound_ringtone", "ui.call_notifications", "ui.draw_attention"];

impl Prefs {
    pub fn from_values(values: [Option<String>; 5]) -> Self {
        let d = Self::default();
        let on = |v: &Option<String>, default: bool| v.as_deref().map(|v| v == "true").unwrap_or(default);
        Self {
            notification: on(&values[0], d.notification),
            in_chat: on(&values[1], d.in_chat),
            ringtone: on(&values[2], d.ringtone),
            call_notify: on(&values[3], d.call_notify),
            attention: on(&values[4], d.attention),
        }
    }
}

/// A note: its pitch in hertz, start and length in seconds, loudness.
struct Note(f32, f32, f32, f32);

/// Sine notes with a soft attack and an exponential fade, mixed.
fn render(notes: &[Note], length: f32) -> Vec<f32> {
    let n = (length * RATE as f32) as usize;
    let mut out = vec![0f32; n];
    for &Note(freq, start, dur, gain) in notes {
        let from = (start * RATE as f32) as usize;
        let len = (dur * RATE as f32) as usize;
        for i in 0..len.min(n.saturating_sub(from)) {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.008).min(1.);
            let fade = (-t * 6. / dur).exp();
            // A little of the octave above makes it bell-like.
            let wave = (std::f32::consts::TAU * freq * t).sin() + 0.25 * (std::f32::consts::TAU * freq * 2. * t).sin();
            out[from + i] += wave * attack * fade * gain;
        }
    }
    out
}

/// A new message: two rising notes.
pub fn message_chime() -> Vec<f32> {
    render(&[Note(880., 0., 0.18, 0.22), Note(1318.5, 0.09, 0.3, 0.18)], 0.45)
}

/// A message in the chat you're looking at: one quiet tick.
pub fn in_chat_tick() -> Vec<f32> {
    render(&[Note(1046.5, 0., 0.12, 0.12)], 0.15)
}

/// One ring of an incoming call: a phrase of four notes, then quiet;
/// played again and again while it rings.
pub fn ring() -> Vec<f32> {
    render(
        &[Note(659.25, 0., 0.25, 0.22), Note(783.99, 0.2, 0.25, 0.22), Note(987.77, 0.4, 0.35, 0.22), Note(783.99, 0.7, 0.5, 0.18)],
        2.,
    )
}

fn play(samples: Vec<f32>) -> Option<Child> {
    let mut child = unsafe {
        Command::new("pw-play")
            .args(["--raw", "--format", "f32", "--rate", &RATE.to_string(), "--channels", "1"])
            .args(["-P", "{ application.name = langis media.role = Notification }", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            })
            .spawn()
            .ok()?
    };
    let mut stdin = child.stdin.take()?;
    std::thread::spawn(move || {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let _ = stdin.write_all(&bytes);
    });
    Some(child)
}

/// Plays a sound once, in the background.
pub fn once(samples: Vec<f32>) {
    std::thread::spawn(move || {
        if let Some(mut child) = play(samples) {
            let _ = child.wait();
        }
    });
}

/// The ringtone while a call rings: started once, stopped with `stop_ring`.
static RINGING: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = Mutex::new(None);

pub fn start_ring() {
    let mut ringing = RINGING.lock().unwrap();
    if ringing.is_some() {
        return;
    }
    let on = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    *ringing = Some(on.clone());
    std::thread::spawn(move || {
        while on.load(std::sync::atomic::Ordering::Relaxed) {
            let Some(mut child) = play(ring()) else { return };
            // Wait for the ring, stopping it early when answered.
            loop {
                if !on.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    });
}

pub fn stop_ring() {
    if let Some(on) = RINGING.lock().unwrap().take() {
        on.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sounds_are_short_and_never_clip() {
        for (sound, max_len) in [(super::message_chime(), 0.5), (super::in_chat_tick(), 0.2), (super::ring(), 2.)] {
            assert!(!sound.is_empty());
            assert!(sound.len() as f32 / super::RATE as f32 <= max_len + 0.01);
            assert!(sound.iter().all(|s| s.abs() < 1.), "a sample clips");
        }
    }
}
