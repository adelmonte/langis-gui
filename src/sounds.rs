//! The app's sounds: Signal Desktop's own (`sounds/*.ogg`, AGPL-3.0,
//! credited in NOTICE), played where Desktop plays them
//! (`ts/util/Sound.std.ts`, `ts/services/notifications.preload.ts`,
//! `ts/util/callingTones.preload.ts`). PipeWire's `pw-play` plays them, or
//! `ffplay` without it.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Desktop's `SoundType`s that this app plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// A message or reaction notification, and a message in the open chat.
    Pop,
    /// An incoming call's notification.
    TriTone,
    /// A message sent, with in-chat sounds on.
    Whoosh,
    /// While a call rings us.
    Ringtone,
    /// A call ending.
    HangUp,
    /// Someone raising a hand in a call.
    HandRaised,
    /// Someone starting to present in a call.
    Presenting,
    /// The last of a run of voice messages finishing.
    VoiceNoteEnd,
}

impl Sound {
    fn file(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Pop => ("pop.ogg", include_bytes!("../assets/sounds/pop.ogg")),
            Self::TriTone => ("notification.ogg", include_bytes!("../assets/sounds/notification.ogg")),
            Self::Whoosh => ("whoosh.ogg", include_bytes!("../assets/sounds/whoosh.ogg")),
            Self::Ringtone => ("ringtone_minimal.ogg", include_bytes!("../assets/sounds/ringtone_minimal.ogg")),
            Self::HangUp => ("navigation-cancel.ogg", include_bytes!("../assets/sounds/navigation-cancel.ogg")),
            Self::HandRaised => ("notification_simple-01.ogg", include_bytes!("../assets/sounds/notification_simple-01.ogg")),
            Self::Presenting => {
                ("navigation_selection-complete-celebration.ogg", include_bytes!("../assets/sounds/navigation_selection-complete-celebration.ogg"))
            }
            Self::VoiceNoteEnd => ("state-change_confirm-up.ogg", include_bytes!("../assets/sounds/state-change_confirm-up.ogg")),
        }
    }

    /// The sound as a file the players can open: written to the cache
    /// directory the first time.
    fn path(self) -> Option<PathBuf> {
        let (name, bytes) = self.file();
        // No shared /tmp fallback: another user could plant files there.
        let dir = dirs::cache_dir()?.join("langis-gui").join("sounds");
        let path = dir.join(name);
        if std::fs::metadata(&path).map(|m| m.len() as usize) .ok() != Some(bytes.len()) {
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::write(&path, bytes).ok()?;
        }
        Some(path)
    }
}

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

fn play(sound: Sound) -> Option<Child> {
    let path = sound.path()?;
    let die_with_parent = |command: &mut Command| unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    };
    let mut pw = Command::new("pw-play");
    pw.args(["-P", "{ application.name = langis media.role = Notification }"]).arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    die_with_parent(&mut pw);
    if let Ok(child) = pw.spawn() {
        return Some(child);
    }
    let mut ffplay = Command::new("ffplay");
    ffplay.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]).arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    die_with_parent(&mut ffplay);
    ffplay.spawn().ok()
}

/// Plays a sound once, in the background.
pub fn once(sound: Sound) {
    std::thread::spawn(move || {
        if let Some(mut child) = play(sound) {
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
            let Some(mut child) = play(Sound::Ringtone) else { return };
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
    fn every_sound_is_a_real_ogg() {
        use super::Sound::*;
        for sound in [Pop, TriTone, Whoosh, Ringtone, HangUp, HandRaised, Presenting, VoiceNoteEnd] {
            let (name, bytes) = sound.file();
            assert!(bytes.starts_with(b"OggS"), "{name}");
        }
    }
}
