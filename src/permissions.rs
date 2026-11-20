//! Desktop's media permissions: Settings' "Allow access to the microphone"
//! and "Allow access to the camera" (both off until allowed), and its
//! `PermissionsPopup`, asked before a call or a voice message needs them.

use crate::app::App;
use crate::i18n::tr;
use langis_store::ConversationId;

pub const SETTING_MIC: &str = "ui.media_permissions";
pub const SETTING_CAMERA: &str = "ui.media_camera_permissions";

/// What wants the microphone or camera.
#[derive(Clone, PartialEq, Debug)]
pub enum Need {
    VoiceMessage,
    /// A call: the lobby for it, then, once allowed.
    Call(ConversationId, bool),
    /// Answering a ringing call (with video or not).
    Answer(bool),
    /// The camera in a call.
    Camera,
}

impl Need {
    fn camera(&self) -> bool {
        matches!(self, Need::Call(_, true) | Need::Answer(true) | Need::Camera)
    }

    fn mic(&self) -> bool {
        !matches!(self, Need::Camera)
    }
}

/// True when `need` may go ahead now; otherwise the popup asks.
pub fn allowed(app: &mut App, need: Need) -> bool {
    let (mic, camera) = app.media_perms;
    if (!need.mic() || mic) && (!need.camera() || camera) {
        return true;
    }
    app.panes.permission_ask = Some(need);
    false
}

fn save(app: &App, key: &'static str, on: bool) {
    crate::settings::write(app, key, on.to_string());
}

pub fn set_mic(app: &mut App, on: bool) {
    app.media_perms.0 = on;
    save(app, SETTING_MIC, on);
}

pub fn set_camera(app: &mut App, on: bool) {
    app.media_perms.1 = on;
    save(app, SETTING_CAMERA, on);
}

/// Desktop's `PermissionsPopup`: why, then Cancel or Allow Access, which
/// turns the setting on and carries on.
pub fn popup(app: &mut App, ctx: &egui::Context) {
    let Some(need) = app.panes.permission_ask.clone() else { return };
    let (mic, _) = app.media_perms;
    let text = if need.camera() && (mic || !need.mic()) {
        "For video calling, you must allow langis to access your camera."
    } else if matches!(need, Need::VoiceMessage) {
        "To send voice messages, allow langis to access your microphone."
    } else {
        "For calling, you must allow langis to access your microphone."
    };
    use crate::dialog::{Answer, Variant};
    let Some(answer) = crate::dialog::show(ctx, egui::Id::unique("permissions-popup"), "", text, Some(tr("Cancel")), &[(tr("Allow Access"), Variant::StrongPrimary)]) else { return };
    app.panes.permission_ask = None;
    if answer != Answer::Action(0) {
        return;
    }
    if need.mic() {
        set_mic(app, true);
    }
    if need.camera() {
        set_camera(app, true);
    }
    match need {
        Need::Call(conversation, video) => crate::panes::open_lobby(app, conversation, video),
        Need::Answer(video) => {
            app.call_view.video_on = Some(video);
            app.send(langis_core::Command::AcceptCall { video });
        }
        Need::Camera => {
            app.call_view.video_on = Some(true);
            app.send(langis_core::Command::SetCallVideo(true));
        }
        Need::VoiceMessage => {}
    }
}
