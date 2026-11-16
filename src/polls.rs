//! Polls: Desktop's "New poll" dialog (`PollCreateModal`) and a poll in
//! the timeline.

use egui::{RichText, Ui};
use langis_core::Command;

use crate::actions::Do;
use crate::theme::{self, Palette};
use crate::widgets;

/// Desktop's limits (`POLL_QUESTION_MAX_LENGTH`, `POLL_OPTION_MAX_LENGTH`,
/// `POLL_OPTIONS_MAX_COUNT`).
const QUESTION_MAX: usize = 200;
const OPTION_MAX: usize = 150;
const OPTIONS_MAX: usize = 10;

/// A poll being written.
pub struct NewPoll {
    pub question: String,
    pub options: Vec<String>,
    pub allow_multiple: bool,
    pub error: Option<&'static str>,
}

impl Default for NewPoll {
    fn default() -> Self {
        Self { question: String::new(), options: vec![String::new(), String::new()], allow_multiple: false, error: None }
    }
}

impl NewPoll {
    /// Desktop's checks before sending, in its words.
    fn check(&self) -> Result<(), &'static str> {
        let question = self.question.trim();
        if question.is_empty() {
            return Err("Poll question is required");
        }
        if question.chars().count() > QUESTION_MAX {
            return Err("Question is too long");
        }
        if self.options.iter().any(|o| o.trim().chars().count() > OPTION_MAX) {
            return Err("Option is too long");
        }
        if self.options.iter().filter(|o| !o.trim().is_empty()).count() < 2 {
            return Err("Poll requires at least 2 options");
        }
        Ok(())
    }
}

/// The "New poll" dialog's body; Send gives the command.
pub fn dialog(ui: &mut Ui, palette: &Palette, poll: &mut NewPoll, conversation: &langis_store::ConversationId, out: &mut Vec<Do>) {
    ui.set_width(400.);
    ui.label(RichText::new("New poll").font(theme::semibold(18.)));
    ui.add_space(12.);
    ui.label(RichText::new("Question").font(theme::medium(13.)).color(palette.text_secondary));
    widgets::text_field(ui, &mut poll.question, "Ask a question", false);
    ui.add_space(12.);
    ui.label(RichText::new("Options").font(theme::medium(13.)).color(palette.text_secondary));
    let mut remove = None;
    for i in 0..poll.options.len() {
        ui.horizontal(|ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width() - if poll.options.len() > 2 { 36. } else { 0. };
            ui.allocate_ui(egui::vec2(w, 34.), |ui| {
                widgets::text_field(ui, &mut poll.options[i], &format!("Option {}", i + 1), false);
            });
            if poll.options.len() > 2 && widgets::icon_button(ui, crate::icons::Icon::XCompact, 14., "Remove option").clicked() {
                remove = Some(i);
            }
        });
        ui.add_space(4.);
    }
    if let Some(i) = remove {
        poll.options.remove(i);
    }
    // Desktop adds a field once the last one is used.
    if poll.options.len() < OPTIONS_MAX && poll.options.last().is_some_and(|o| !o.trim().is_empty()) {
        poll.options.push(String::new());
    }
    ui.add_space(8.);
    ui.horizontal(|ui| {
        widgets::switch(ui, &mut poll.allow_multiple);
        ui.label(RichText::new("Allow multiple votes").font(theme::regular(14.)));
    });
    if let Some(error) = poll.error {
        ui.add_space(8.);
        ui.label(RichText::new(error).color(palette.destructive));
    }
    ui.add_space(16.);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if widgets::pill_button(ui, "Send", true).clicked() {
            match poll.check() {
                Ok(()) => {
                    let options = poll.options.iter().map(|o| o.trim().to_owned()).filter(|o| !o.is_empty()).collect();
                    out.push(Do::Command(Command::SendPoll {
                        conversation: conversation.clone(),
                        question: poll.question.trim().to_owned(),
                        options,
                        allow_multiple: poll.allow_multiple,
                    }));
                    out.push(Do::Close);
                }
                Err(error) => poll.error = Some(error),
            }
        }
        if widgets::pill_button(ui, "Cancel", false).clicked() {
            out.push(Do::Close);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poll_needs_a_question_and_two_options() {
        let mut poll = NewPoll::default();
        assert_eq!(poll.check(), Err("Poll question is required"));
        poll.question = "Lunch?".into();
        poll.options[0] = "Tacos".into();
        assert_eq!(poll.check(), Err("Poll requires at least 2 options"));
        poll.options[1] = "Ramen".into();
        assert_eq!(poll.check(), Ok(()));
    }
}
