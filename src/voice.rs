//! Voice messages: Desktop's recording bar in place of the composer.

use egui::Ui;

use crate::app::App;
use crate::chat::ChatView;
use crate::theme::Palette;

/// A recording in progress.
pub struct Recording;

pub fn start(app: &mut App, chat: &mut ChatView) {
    let _ = chat;
    app.toast("Voice messages aren't available yet.");
}

pub fn recording_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let _ = (app, ui, palette, c);
    chat.recording = None;
}
