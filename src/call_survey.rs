//! Desktop's `CallQualitySurveyDialog`: "How was your call?", then the
//! issues if there were some, then "Help us improve". langis decides when it
//! shows (`Event::CallQualitySurvey`). Desktop submits the answers to
//! Signal's gRPC call quality service, which langis can't reach, so nothing
//! leaves this device and the last page says so.

use egui::Id;

use crate::dialog::{self, Answer, Variant};
use crate::i18n::tr;
use crate::widgets;

/// Desktop's `CallQualitySurvey.Issue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Issue {
    Audio,
    AudioStuttering,
    AudioLocalEcho,
    AudioRemoteEcho,
    AudioDrop,
    Video,
    VideoNoCamera,
    VideoLowQuality,
    VideoLowResolution,
    CallDropped,
    Other,
}

impl Issue {
    fn label(self) -> &'static str {
        match self {
            Issue::Audio => tr("Audio issue"),
            Issue::AudioStuttering => tr("Audio stuttering"),
            Issue::AudioLocalEcho => tr("I heard echo"),
            Issue::AudioRemoteEcho => tr("Others heard echo"),
            Issue::AudioDrop => tr("Audio cut out"),
            Issue::Video => tr("Video issue"),
            Issue::VideoNoCamera => tr("Camera didn't work"),
            Issue::VideoLowQuality => tr("Poor video quality"),
            Issue::VideoLowResolution => tr("Low resolution"),
            Issue::CallDropped => tr("Call dropped"),
            Issue::Other => tr("Something else"),
        }
    }
}

/// Desktop's issue groups: a parent, and what shows under it once checked.
const GROUPS: [(Issue, &[Issue]); 4] = [
    (Issue::Audio, &[Issue::AudioStuttering, Issue::AudioLocalEcho, Issue::AudioRemoteEcho, Issue::AudioDrop]),
    (Issue::Video, &[Issue::VideoNoCamera, Issue::VideoLowQuality, Issue::VideoLowResolution]),
    (Issue::CallDropped, &[]),
    (Issue::Other, &[]),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Step {
    #[default]
    HowWasIt,
    Issues,
    Confirm,
}

#[derive(Clone, Debug, Default)]
pub struct Survey {
    step: Step,
    satisfied: bool,
    issues: Vec<Issue>,
    other: String,
}

impl Survey {
    fn toggle(&mut self, issue: Issue, on: bool) {
        self.issues.retain(|i| *i != issue);
        if on {
            self.issues.push(issue);
        } else if let Some((_, children)) = GROUPS.iter().find(|(p, _)| *p == issue) {
            // Unchecking a parent drops what was under it.
            self.issues.retain(|i| !children.contains(i));
        }
    }

    /// Desktop's Continue: something chosen, and "Something else" described.
    fn can_continue(&self) -> bool {
        !self.issues.is_empty() && (!self.issues.contains(&Issue::Other) || !self.other.trim().is_empty())
    }
}

fn id() -> Id {
    Id::unique("call-quality-survey")
}

/// Opens the survey (from `Event::CallQualitySurvey`).
pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(id(), Survey::default()));
}

pub fn show(app: &mut crate::app::App, ctx: &egui::Context) {
    let Some(mut survey) = ctx.data(|d| d.get_temp::<Survey>(id())) else { return };
    let mut close = false;
    match survey.step {
        Step::HowWasIt => {
            let actions = [(tr("Had issues"), Variant::StrongPrimary), (tr("Great"), Variant::StrongPrimary)];
            match dialog::show(
                ctx,
                id().with("how"),
                tr("How was your call?"),
                tr("This helps us improve calls in Signal. No personally identifiable information will be stored."),
                None,
                &actions,
            ) {
                Some(Answer::Cancel) => close = true,
                Some(Answer::Action(0)) => survey.step = Step::Issues,
                Some(Answer::Action(_)) => {
                    survey.satisfied = true;
                    survey.step = Step::Confirm;
                }
                None => {}
            }
        }
        Step::Issues => {
            let ready = survey.can_continue();
            let answer = dialog::show_with(
                ctx,
                id().with("issues"),
                tr("What issues did you have?"),
                tr("Select all that apply"),
                Some(tr("Cancel")),
                &[(tr("Continue"), Variant::StrongPrimary)],
                |ui| {
                    ui.add_space(8.);
                    for (parent, children) in GROUPS {
                        let mut on = survey.issues.contains(&parent);
                        if widgets::checkbox(ui, &mut on, parent.label()).changed() {
                            survey.toggle(parent, on);
                        }
                        if on {
                            for child in children {
                                ui.horizontal(|ui| {
                                    ui.add_space(24.);
                                    let mut on = survey.issues.contains(child);
                                    if widgets::checkbox(ui, &mut on, child.label()).changed() {
                                        survey.toggle(*child, on);
                                    }
                                });
                            }
                        }
                    }
                    if survey.issues.contains(&Issue::Other) {
                        ui.add_space(6.);
                        ui.add(egui::TextEdit::multiline(&mut survey.other).hint_text(tr("Describe your issue")).desired_rows(3).desired_width(f32::INFINITY));
                    }
                },
            );
            match answer {
                Some(Answer::Cancel) => close = true,
                Some(Answer::Action(_)) if ready => survey.step = Step::Confirm,
                Some(Answer::Action(_)) => app.toast(tr("Make a selection to continue")),
                None => {}
            }
        }
        Step::Confirm => {
            let answer = dialog::show(
                ctx,
                id().with("confirm"),
                tr("Help us improve"),
                "Signal Desktop sends this feedback with diagnostic information about your call. langis can't reach Signal's survey service, so your answers stay on this device and aren't sent.",
                Some(tr("Cancel")),
                &[(tr("Submit"), Variant::StrongPrimary)],
            );
            match answer {
                Some(Answer::Cancel) => close = true,
                Some(Answer::Action(_)) => {
                    tracing::info!(satisfied = survey.satisfied, issues = ?survey.issues, "call quality survey answered (kept local)");
                    close = true;
                }
                None => {}
            }
        }
    }
    ctx.data_mut(|d| {
        if close {
            d.remove::<Survey>(id());
        } else {
            d.insert_temp(id(), survey);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_needs_a_choice_and_a_description_for_something_else() {
        let mut s = Survey::default();
        assert!(!s.can_continue());
        s.toggle(Issue::Audio, true);
        s.toggle(Issue::AudioDrop, true);
        assert!(s.can_continue());
        // Unchecking the parent drops its children.
        s.toggle(Issue::Audio, false);
        assert!(s.issues.is_empty());
        s.toggle(Issue::Other, true);
        assert!(!s.can_continue());
        s.other = "robot voice".into();
        assert!(s.can_continue());
    }
}
