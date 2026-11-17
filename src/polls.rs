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

/// A poll's part of its bubble: the question, how to vote, and each
/// option with its votes (Desktop's `PollMessageContents`).
pub struct PollView {
    question: std::sync::Arc<egui::Galley>,
    options: Vec<std::sync::Arc<egui::Galley>>,
    pub height: f32,
}

const OPTION_H: f32 = 46.;
const FOOTER_H: f32 = 34.;

pub fn layout(ui: &Ui, poll: &langis_store::Poll, width: f32, color: egui::Color32) -> PollView {
    let question = ui.fonts_mut(|f| f.layout_job(egui::text::LayoutJob::simple(poll.question.clone(), theme::semibold(15.), color, width)));
    let options: Vec<_> = poll
        .options
        .iter()
        .map(|o| {
            let mut job = egui::text::LayoutJob::simple_singleline(o.clone(), theme::regular(14.), color);
            job.wrap = egui::text::TextWrapping::truncate_at_width(width - 70.);
            ui.fonts_mut(|f| f.layout_job(job))
        })
        .collect();
    let height = question.size().y + 22. + options.len() as f32 * OPTION_H + FOOTER_H;
    PollView { question, options, height }
}

/// Votes per option, and how many people voted.
fn tally(poll: &langis_store::Poll) -> (Vec<u32>, u32) {
    let mut counts = vec![0u32; poll.options.len()];
    let mut voters = 0;
    for vote in &poll.votes {
        if vote.options.is_empty() {
            continue;
        }
        voters += 1;
        for &o in &vote.options {
            if let Some(c) = counts.get_mut(o as usize) {
                *c += 1;
            }
        }
    }
    (counts, voters)
}

/// Draws the poll at `rect`; returns a vote to send, or `Some(None)` for
/// "View votes".
pub fn paint(
    ui: &Ui,
    view: PollView,
    poll: &langis_store::Poll,
    message_id: i64,
    rect: egui::Rect,
    outgoing: bool,
    palette: &Palette,
) -> Option<Option<Vec<u32>>> {
    use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
    let text = if outgoing { Color32::WHITE } else { palette.text };
    let secondary = if outgoing { palette.bubble_out_meta() } else { palette.text_secondary };
    let mut out = None;
    let mut y = rect.top();
    let qh = view.question.size().y;
    ui.painter().galley(Pos2::new(rect.left(), y), view.question, text);
    y += qh + 2.;
    let sub = if poll.ended {
        "Poll · Final results"
    } else if poll.allow_multiple {
        "Poll · Select one or more"
    } else {
        "Poll · Select one"
    };
    ui.painter().text(Pos2::new(rect.left(), y), Align2::LEFT_TOP, sub, theme::regular(12.), secondary);
    y += 20.;
    let mine: Vec<u32> = poll.votes.iter().find(|v| v.author.is_none()).map(|v| v.options.clone()).unwrap_or_default();
    let (counts, voters) = tally(poll);
    let can_vote = !poll.ended;
    let max = counts.iter().copied().max().unwrap_or(0);
    for (i, galley) in view.options.into_iter().enumerate() {
        let row = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(rect.width(), OPTION_H));
        let i = i as u32;
        let chosen = mine.contains(&i);
        let mut x = row.left();
        if can_vote {
            let c = Pos2::new(x + 10., row.top() + 13.);
            if chosen {
                ui.painter().circle_filled(c, 9., if outgoing { Color32::WHITE } else { palette.badge });
                crate::icons::Icon::Check.paint(ui, Rect::from_center_size(c, Vec2::splat(14.)), 12., if outgoing { palette.bubble_out } else { Color32::WHITE });
            } else {
                ui.painter().circle_stroke(c, 8.5, Stroke::new(1.5, secondary));
            }
            x += 28.;
        }
        ui.painter().galley(Pos2::new(x, row.top() + 4.), galley, text);
        let count = counts.get(i as usize).copied().unwrap_or(0);
        ui.painter().text(Pos2::new(row.right(), row.top() + 4.), Align2::RIGHT_TOP, count.to_string(), theme::regular(13.), secondary);
        // The bar: share of voters, the leader stronger once it's ended.
        let bar = Rect::from_min_size(Pos2::new(x, row.top() + 28.), Vec2::new(row.right() - x, 6.));
        ui.painter().rect_filled(bar, CornerRadius::same(3), if outgoing { Color32::from_white_alpha(60) } else { palette.fill_secondary });
        if voters > 0 && count > 0 {
            let fill = Rect::from_min_size(bar.min, Vec2::new(bar.width() * count as f32 / voters as f32, bar.height()));
            let strong = if outgoing { Color32::WHITE } else { palette.badge };
            let color = if poll.ended && count < max { strong.gamma_multiply(0.5) } else { strong };
            ui.painter().rect_filled(fill, CornerRadius::same(3), color);
        }
        if can_vote && ui.interact(row, ui.scope_id().with(("poll-option", message_id, i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            let vote = if poll.allow_multiple {
                let mut v = mine.clone();
                if chosen {
                    v.retain(|o| *o != i);
                } else {
                    v.push(i);
                    v.sort_unstable();
                }
                v
            } else if chosen {
                Vec::new()
            } else {
                vec![i]
            };
            out = Some(Some(vote));
        }
        y += OPTION_H;
    }
    let label = if voters == 0 { "No votes".to_owned() } else { format!("View votes ({voters})") };
    let footer = Rect::from_min_size(Pos2::new(rect.left(), y + 2.), Vec2::new(rect.width(), FOOTER_H - 6.));
    let r = ui.interact(footer, ui.scope_id().with(("poll-votes", message_id)), Sense::click());
    if r.hovered() && voters > 0 {
        ui.painter().rect_filled(footer, CornerRadius::same(8), if outgoing { Color32::from_white_alpha(30) } else { palette.fill });
    }
    ui.painter().text(footer.center(), Align2::CENTER_CENTER, label, theme::medium(13.), if outgoing { Color32::WHITE } else { palette.accent_text });
    if r.clicked() && voters > 0 {
        out = Some(None);
    }
    out
}

/// Desktop's "Poll votes": who chose each option.
pub fn votes_dialog(app: &crate::app::App, ui: &mut Ui, palette: &Palette, poll: &langis_store::Poll) {
    ui.set_width(380.);
    ui.label(RichText::new("Poll votes").font(theme::semibold(18.)));
    ui.add_space(4.);
    ui.label(RichText::new(&poll.question).color(palette.text_secondary));
    egui::ScrollArea::vertical().max_height(420.).auto_shrink([false, true]).show(ui, |ui| {
        for (i, option) in poll.options.iter().enumerate() {
            let who: Vec<String> = poll
                .votes
                .iter()
                .filter(|v| v.options.contains(&(i as u32)))
                .map(|v| match &v.author {
                    None => "You".to_owned(),
                    Some(a) => app.name(a),
                })
                .collect();
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.label(RichText::new(option).font(theme::semibold(14.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if who.len() == 1 { "1 vote".to_owned() } else { format!("{} votes", who.len()) }).font(theme::regular(12.)).color(palette.text_secondary));
                });
            });
            for name in who {
                ui.horizontal(|ui| {
                    let color = name.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                    widgets::avatar(ui, &widgets::AvatarSpec::person(&name, color), 24.);
                    ui.label(RichText::new(name).font(theme::regular(14.)));
                });
            }
        }
    });
}
