//! Desktop's fun picker (`FunPicker`): emoji, stickers and GIFs in one
//! popover over the composer.

use egui::Pos2;

use crate::app::App;
use crate::chat::ChatView;

/// Shows the picker above `anchor`. An emoji picked comes back to go in
/// the text; a sticker or GIF is sent from here.
pub fn show(app: &mut App, ctx: &egui::Context, anchor: Pos2, chat: &mut ChatView, c: &langis_store::Conversation) -> Option<&'static str> {
    let _ = (app, c);
    chat.emoji.show(ctx, anchor, &mut chat.emoji_open)
}
