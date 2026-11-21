//! Desktop's `CallQualitySurveyDialog`: "How was your call?", then the
//! issues if there were some, then "Help us improve". langis decides when it
//! shows (`Event::CallQualitySurvey`) and sends the answers
//! (`Command::SubmitCallQualitySurvey`), with the call's statistics, as
//! Desktop's `submitCallQualitySurvey` does. Desktop can also attach a
//! debug log; this doesn't offer that yet.

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
    /// Desktop's `CallQualitySurvey.Issue` value, the same on every client.
    fn wire(self) -> &'static str {
        match self {
            Issue::Audio => "audio",
            Issue::AudioStuttering => "audio_stuttering",
            Issue::AudioLocalEcho => "audio_local_echo",
            Issue::AudioRemoteEcho => "audio_remote_echo",
            Issue::AudioDrop => "audio_drop",
            Issue::Video => "video",
            Issue::VideoNoCamera => "video_no_camera",
            Issue::VideoLowQuality => "video_low_quality",
            Issue::VideoLowResolution => "video_low_resolution",
            Issue::CallDropped => "call_dropped",
            Issue::Other => "other",
        }
    }

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
    /// The call asked about, sent with the answers.
    call: Option<langis_core::call_survey::CallSurvey>,
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

    /// The answers as Desktop sends them: no issues when it went well, and
    /// the description only with "Something else".
    fn answers(&self) -> (Vec<String>, Option<String>) {
        if self.satisfied {
            return (Vec::new(), None);
        }
        let issues = self.issues.iter().map(|i| i.wire().to_owned()).collect();
        let other = self.issues.contains(&Issue::Other).then(|| self.other.trim().to_owned()).filter(|o| !o.is_empty());
        (issues, other)
    }
}

fn id() -> Id {
    Id::unique("call-quality-survey")
}

/// Opens the survey (from `Event::CallQualitySurvey`).
pub fn open(ctx: &egui::Context, call: langis_core::call_survey::CallSurvey) {
    ctx.data_mut(|d| d.insert_temp(id(), Survey { call: Some(call), ..Survey::default() }));
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
                tr("Information shared with us contains low level app information and does not include the contents of your calls."),
                Some(tr("Cancel")),
                &[(tr("Submit"), Variant::StrongPrimary)],
            );
            match answer {
                Some(Answer::Cancel) => close = true,
                Some(Answer::Action(_)) => {
                    if let Some(call) = survey.call.clone() {
                        let (issues, description) = survey.answers();
                        app.send(langis_core::Command::SubmitCallQualitySurvey {
                            survey: call,
                            satisfied: survey.satisfied,
                            issues,
                            description,
                            debug_log_url: None,
                        });
                    }
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
        // Sent as Desktop's values; the description only with Other.
        s.toggle(Issue::Video, true);
        assert_eq!(s.answers(), (vec!["other".to_owned(), "video".to_owned()], Some("robot voice".to_owned())));
        s.toggle(Issue::Other, false);
        assert_eq!(s.answers().1, None);
        s.satisfied = true;
        assert_eq!(s.answers(), (Vec::new(), None));
    }
}
