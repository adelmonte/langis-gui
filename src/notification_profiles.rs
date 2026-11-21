//! Desktop's notification profiles: Settings > Notifications > Notification
//! profiles (`ts/components/PreferencesNotificationProfiles.dom.tsx`: the
//! list, the create flow of name, allowed and schedule, the edit page and
//! its name and schedule pages), the chat list's menu
//! (`NotificationProfilesMenu.dom.tsx`) and its moon button while one is
//! on (`LeftPane.dom.tsx`), and the "on"/"off" toasts
//! (`notificationProfilesService`). Which profile is on comes from
//! langis's `notification_profile_state`; notifications ask [`silenced`].

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use langis_core::Command;
use langis_store::{ConversationId, NotificationProfile, NotificationProfileEdit, NotificationProfileOverride, NotificationProfileState};

use crate::app::{App, Loaded};
use crate::axo_list::{self, End, Item};
use crate::i18n::{Arg, tf, tr};
use crate::theme::{self, Palette};
use crate::widgets;

/// Ours: Desktop's `hasSeenNotificationProfileOnboarding`.
const SETTING_ONBOARDED: &str = "ui.notification_profiles_onboarded";

const HOUR: u64 = 3_600_000;

/// Desktop's sample names (`NotificationProfilesNamePage`), with their emoji.
const SAMPLES: [(&str, &str); 5] = [("💪", "Work"), ("😴", "Sleep"), ("🚗", "Driving"), ("😊", "Downtime"), ("💡", "Focus")];

/// The days in Desktop's UI order (Sunday first), as indexes into
/// `schedule_days` (Monday first), with their long and short names.
const DAYS: [(usize, &str, &str); 7] = [
    (6, "Sunday", "Sun"),
    (0, "Monday", "Mon"),
    (1, "Tuesday", "Tue"),
    (2, "Wednesday", "Wed"),
    (3, "Thursday", "Thu"),
    (4, "Friday", "Fri"),
    (5, "Saturday", "Sat"),
];

/// Desktop's avatar background colors (`AvatarColorMap`'s `bg`), one picked
/// at random for a new profile, as ARGB.
const COLORS: [u32; 12] = [
    0xffe3e3fe, 0xffdde7fc, 0xffd8e8f0, 0xffcde4cd, 0xffeae0f8, 0xfff5e3fe, 0xfff6d8ec, 0xfff5d7d7, 0xfffef5d0, 0xffeae6d5,
    0xffd2d2dc, 0xffd7d7d9,
];

#[derive(Default)]
pub struct Profiles {
    /// The current profiles, newest first.
    pub all: Vec<NotificationProfile>,
    /// Deleted ones too, for working out which is on.
    every: Vec<NotificationProfile>,
    pub override_: Option<NotificationProfileOverride>,
    /// Which is on, worked out at `computed_at` (unix millis).
    pub state: NotificationProfileState,
    computed_at: u64,
    loaded: bool,
    pub view: View,
    /// The edit page's "Permanently delete this notification profile?".
    delete_asked: bool,
    /// The name page's emoji picker.
    emoji: Option<crate::emoji_picker::Picker>,
    /// Desktop's `PreferencesSelectChatsDialog`: its search and the chats
    /// chosen.
    chooser: Option<(String, Vec<ConversationId>)>,
    /// The name page's name and emoji while editing a profile's, saved on
    /// Done or Back (not on every key, which would write Storage Service
    /// each time).
    name_draft: Option<(String, Option<String>)>,
    /// Desktop's onboarding dialog, shown once.
    onboarding: Option<bool>,
}

/// Which page of the settings flow is showing.
#[derive(Default, Clone, PartialEq)]
pub enum View {
    #[default]
    List,
    Edit(String),
    Name(String),
    Schedule(String),
    Create(Create, Box<NotificationProfile>),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Create {
    Name,
    Allowed,
    Schedule,
    Done,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Reads the profiles and the override again.
pub fn load(app: &App) {
    app.load(|db| async move {
        let every = db.notification_profiles().await.ok()?;
        let override_ = db.notification_profile_override().await.ok().flatten();
        let onboarded = db.setting(SETTING_ONBOARDED).await.ok().flatten().is_some();
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            let p = &mut app.panes.profiles;
            p.all = langis_store::current_profiles(&every).into_iter().cloned().collect();
            p.every = every;
            p.override_ = override_;
            if p.onboarding.is_none() {
                p.onboarding = Some(!onboarded);
            }
            recompute(app);
            app.panes.profiles.loaded = true;
        })))
    });
}

/// Works out which profile is on now; Desktop's toast when that changed.
fn recompute(app: &mut App) {
    let p = &mut app.panes.profiles;
    let t = now();
    let state = langis_store::notification_profile_state(&langis_store::local_calendar, &p.every, p.override_.as_ref(), t);
    let before = p.state.active.clone();
    p.computed_at = t;
    let after = state.active.clone();
    p.state = state;
    if !p.loaded || before.as_ref().map(|b| &b.id) == after.as_ref().map(|a| &a.id) {
        return;
    }
    match (before, after) {
        (_, Some(on)) => app.toast(tf("icu:NotificationProfilesToast--enabled", &format!("“{}” on", on.name), &[("name", Arg::from(&on.name))])),
        (Some(off), None) => app.toast(tf("icu:NotificationProfilesToast--disabled", &format!("“{}” off", off.name), &[("name", Arg::from(&off.name))])),
        (None, None) => {}
    }
}

/// Each frame: looks again when the schedule or an override's end comes
/// (and once a minute in case the clock jumped), and asks for a frame
/// then.
pub fn tick(app: &mut App) {
    if !app.panes.profiles.loaded {
        return;
    }
    let t = now();
    let p = &app.panes.profiles;
    if p.state.next_change.is_some_and(|at| t >= at) || t.saturating_sub(p.computed_at) >= 60_000 {
        recompute(app);
    }
    let p = &app.panes.profiles;
    let wait = p.state.next_change.map_or(60_000, |at| at.saturating_sub(t).min(60_000)).max(500);
    app.ctx.request_repaint_after(std::time::Duration::from_millis(wait));
}

/// The profile on now.
pub fn active(app: &App) -> Option<&NotificationProfile> {
    app.panes.profiles.state.active.as_ref()
}

/// Whether the profile on now keeps a notification from `conversation`
/// quiet (Desktop's `shouldNotify`, negated): `is_call` for a ringing call,
/// `mention_or_reply` for a message that mentions or answers us.
pub fn silenced(app: &App, conversation: &ConversationId, is_call: bool, mention_or_reply: bool) -> bool {
    !langis_store::should_notify(active(app), conversation, is_call, mention_or_reply)
}

/// Desktop's `setProfileOverride(id, enabled, endsAtMs)`.
fn set_override(app: &App, id: &str, enabled: bool, ends_at: Option<u64>) {
    let edit = if enabled {
        NotificationProfileEdit::Enable { id: id.to_owned(), ends_at, at: now() }
    } else {
        NotificationProfileEdit::Disable { at: now() }
    };
    app.send(Command::EditNotificationProfiles(edit));
}

fn save(app: &App, profile: NotificationProfile) {
    app.send(Command::EditNotificationProfiles(NotificationProfileEdit::Save(profile)));
}

/// hhmm as Desktop's `formatTimeForDisplay`: "9:00 AM", or 24-hour
/// outside English.
pub fn time_label(hhmm: u32) -> String {
    let (h, m) = (hhmm / 100, hhmm % 100);
    if crate::i18n::locale() != "en" {
        return format!("{h:02}:{m:02}");
    }
    let (h12, pm) = match h {
        0 => (12, false),
        12 => (12, true),
        h if h > 12 => (h - 12, true),
        h => (h, false),
    };
    format!("{h12}:{m:02} {}", if pm { tr("PM") } else { tr("AM") })
}

fn local_time(millis: u64) -> String {
    use chrono::TimeZone as _;
    chrono::Local.timestamp_millis_opt(millis as i64).single().map_or_else(String::new, |t| crate::format::localized(&t, "%-I:%M %p"))
}

/// Desktop's `ScheduleSummary`: "Weekdays", "Weekends", "Daily", or the
/// days, Sunday first.
pub fn schedule_summary(days: &[bool; 7]) -> String {
    match days {
        [true, true, true, true, true, false, false] => tr("Weekdays").into(),
        [false, false, false, false, false, true, true] => tr("Weekends").into(),
        [true, true, true, true, true, true, true] => tr("Daily").into(),
        _ => DAYS.iter().filter(|(i, _, _)| days[*i]).map(|(_, _, short)| tr(short)).collect::<Vec<_>>().join(tr(", ")),
    }
}

fn argb(color: u32) -> Color32 {
    Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8)
}

/// Desktop's `ProfileAvatar`: the profile's color with its emoji, or the
/// moon; with no profile, the accent tint and the moon. `active` rings it.
pub fn paint_avatar(painter: &egui::Painter, palette: &Palette, rect: Rect, profile: Option<&NotificationProfile>, active: bool) {
    let center = rect.center();
    let radius = rect.width() / 2.;
    let fill = match profile {
        Some(p) if p.color != 0 => argb(p.color),
        _ => palette.accent.gamma_multiply(if palette.dark { 0.35 } else { 0.18 }),
    };
    painter.circle_filled(center, radius, fill);
    if active {
        painter.circle_stroke(center, radius - 1., Stroke::new(2., palette.accent));
    }
    // Desktop's sizes: 48 in 80, 20 in 36, 16 in 28, 12 in 20.
    let glyph = match rect.width() {
        w if w >= 60. => 48.,
        w if w >= 32. => 20.,
        w if w >= 24. => 16.,
        _ => 12.,
    };
    match profile.and_then(|p| p.emoji.as_deref()) {
        Some(emoji) => {
            painter.text(center, Align2::CENTER_CENTER, emoji, theme::emoji(glyph), Color32::WHITE);
        }
        // The moon in the light theme's accent on a profile's color.
        None => {
            let color = if profile.is_some() { theme::rgb(0x2c58c3) } else { palette.accent };
            theme::paint_symbol(painter, "moon-fill", center, glyph, color);
        }
    }
}

/// Desktop's `NotificationProfilesMenu`: "Notification Profile" (On, or
/// On until a time), each profile to turn on or off, with "For one hour"
/// and "Until …" under the one that's on, then Settings.
pub fn menu(app: &mut App, m: &mut crate::menu::Menu) {
    let active_id = active(app).map(|p| p.id.clone());
    let ends_at = match &app.panes.profiles.override_ {
        Some(NotificationProfileOverride::Enabled { ends_at: Some(at), .. }) => Some(*at),
        _ => None,
    };
    let on = match (&active_id, ends_at) {
        (Some(_), Some(at)) => {
            let end = local_time(at);
            Some(tf("icu:NotificationProfileMenu--on-with-end", &format!("On until {end}"), &[("endTime", Arg::from(&end))]))
        }
        (Some(_), None) => Some(tr("On").to_owned()),
        _ => None,
    };
    m.header(tr("Notification Profile"), on.as_deref());
    let profiles = app.panes.profiles.all.clone();
    if !profiles.is_empty() {
        m.separator();
    }
    // Desktop's "until": 8 AM before it, 6 PM in the day, 8 AM tomorrow after.
    let t = now();
    let (midnight, _) = langis_store::local_calendar(t);
    let (tomorrow, _) = langis_store::local_calendar(midnight + 26 * HOUR);
    let (eight, six) = (midnight + 8 * HOUR, midnight + 18 * HOUR);
    let until = if t < eight {
        eight
    } else if t > six {
        tomorrow + 8 * HOUR
    } else {
        six
    };
    let palette = Palette::of(m.ui.ctx());
    for (i, profile) in profiles.iter().enumerate() {
        if i > 0 {
            m.separator();
        }
        let is_active = active_id.as_deref() == Some(profile.id.as_str());
        if m.custom(20., &profile.name, false, |painter, rect| paint_avatar(painter, &palette, rect, Some(profile), is_active)) {
            set_override(app, &profile.id, !is_active, None);
        }
        if is_active {
            if m.item(None, tr("For one hour")) {
                set_override(app, &profile.id, true, Some(t + HOUR));
            }
            let time = local_time(until);
            let label = tf("icu:NotificationProfileMenu--until-time", &format!("Until {time}"), &[("time", Arg::from(&time))]);
            if m.item(None, &label) {
                set_override(app, &profile.id, true, Some(until));
            }
        }
    }
    m.separator();
    if m.item(Some("settings"), tr("Settings")) {
        open_settings(app);
    }
}

fn open_settings(app: &mut App) {
    app.tab = crate::app::Tab::Settings;
    app.settings.page = crate::settings::Page::NotificationProfiles;
    app.panes.profiles.view = View::List;
}

/// Desktop's LeftPane button while a profile is on: the moon, opening the
/// menu.
pub fn indicator(app: &mut App, ui: &mut Ui) {
    if active(app).is_none() {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.), Sense::click());
    paint_avatar(ui.painter(), &palette, rect, None, false);
    let response = response.on_hover_text(tr("Notification profile"));
    crate::menu::dropdown(ui, &response, egui::Id::unique("notification-profile-indicator"), |m| menu(app, m));
}

/// The settings page's title.
pub fn title(app: &App) -> String {
    let p = &app.panes.profiles;
    match &p.view {
        View::List => tr("Notification Profiles").into(),
        View::Edit(id) => p.all.iter().find(|x| x.id == *id).map(|x| x.name.clone()).unwrap_or_default(),
        View::Name(_) => tr("Name").into(),
        View::Schedule(_) => tr("Schedule").into(),
        View::Create(..) => String::new(),
    }
}

/// The page's back button.
pub fn back(app: &mut App) {
    let view = std::mem::take(&mut app.panes.profiles.view);
    app.panes.profiles.view = match view {
        View::List => {
            app.settings.page = crate::settings::Page::Notifications;
            View::List
        }
        View::Edit(_) | View::Create(Create::Name | Create::Done, _) => View::List,
        View::Name(id) => {
            finish_name(app, &id);
            View::Edit(id)
        }
        View::Schedule(id) => View::Edit(id),
        View::Create(Create::Allowed, d) => View::Create(Create::Name, d),
        View::Create(Create::Schedule, d) => View::Create(Create::Allowed, d),
    };
}

/// Saves the name page's draft, when it changed and has a name.
fn finish_name(app: &mut App, id: &str) {
    let Some((name, emoji)) = app.panes.profiles.name_draft.take() else { return };
    let Some(mut p) = app.panes.profiles.all.iter().find(|p| p.id == id).cloned() else { return };
    let name = name.trim().to_owned();
    if name.is_empty() || (name == p.name && emoji == p.emoji) {
        return;
    }
    p.name = name;
    p.emoji = emoji;
    save(app, p);
}

/// The settings page, for the view showing.
pub fn page(app: &mut App, ui: &mut Ui) {
    let view = app.panes.profiles.view.clone();
    // A profile deleted (here or elsewhere) while open: back to the list.
    let find = |app: &App, id: &str| app.panes.profiles.all.iter().find(|p| p.id == id).cloned();
    match view {
        View::List => list_page(app, ui),
        View::Edit(id) => match find(app, &id) {
            Some(p) => edit_page(app, ui, p),
            None => app.panes.profiles.view = View::List,
        },
        View::Name(id) => match find(app, &id) {
            Some(p) => {
                let (mut name, mut emoji) = app.panes.profiles.name_draft.take().unwrap_or((p.name, p.emoji));
                let done = name_page(app, ui, &mut name, &mut emoji, true);
                app.panes.profiles.name_draft = Some((name, emoji));
                if done {
                    finish_name(app, &id);
                    app.panes.profiles.view = View::Edit(id);
                }
            }
            None => app.panes.profiles.view = View::List,
        },
        View::Schedule(id) => match find(app, &id) {
            Some(mut p) => {
                let before = p.clone();
                let done = schedule_page(app, ui, &mut p, true);
                if p != before {
                    save(app, p);
                }
                if done {
                    app.panes.profiles.view = View::Edit(id);
                }
            }
            None => app.panes.profiles.view = View::List,
        },
        View::Create(step, mut draft) => {
            let next = match step {
                Create::Name => {
                    let (mut name, mut emoji) = (draft.name.clone(), draft.emoji.clone());
                    let done = name_page(app, ui, &mut name, &mut emoji, false);
                    draft.name = name;
                    draft.emoji = emoji;
                    done.then_some(Create::Allowed)
                }
                Create::Allowed => allowed_page(app, ui, &mut draft).then_some(Create::Schedule),
                Create::Schedule => {
                    let done = schedule_page(app, ui, &mut draft, false);
                    if done {
                        let mut profile = (*draft).clone();
                        profile.name = profile.name.trim().to_owned();
                        profile.created_at = now();
                        save(app, profile);
                    }
                    done.then_some(Create::Done)
                }
                Create::Done => {
                    if done_page(ui, &draft) {
                        app.panes.profiles.view = View::List;
                        return;
                    }
                    None
                }
            };
            // The page may have moved on (back, or another view) meanwhile.
            if matches!(app.panes.profiles.view, View::Create(s, _) if s == step) {
                app.panes.profiles.view = View::Create(next.unwrap_or(step), draft);
            }
        }
    }
}

/// Desktop's `Title`, centered.
fn page_title(ui: &mut Ui, text: &str) {
    ui.add_space(16.);
    ui.vertical_centered(|ui| ui.label(egui::RichText::new(text).font(theme::semibold(20.))));
}

fn centered_text(ui: &mut Ui, text: &str) {
    let palette = Palette::of(ui.ctx());
    ui.add_space(12.);
    ui.vertical_centered(|ui| {
        ui.set_max_width(335.);
        ui.add(egui::Label::new(egui::RichText::new(text).font(theme::regular(13.)).color(palette.text)).wrap());
    });
    ui.add_space(12.);
}

/// Desktop's `ButtonContainer`: the page's main button, centered. True when
/// pressed.
fn main_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    ui.add_space(24.);
    let mut pressed = false;
    ui.vertical_centered(|ui| {
        pressed = ui.add_enabled_ui(enabled, |ui| widgets::pill_button(ui, label, true).clicked()).inner && enabled;
    });
    ui.add_space(24.);
    pressed
}

/// A row with a 36px leading circle (a profile, the plus, a chat), as
/// Desktop's `FullWidthButton`. True when clicked.
fn row(ui: &mut Ui, label: &str, lead: impl FnOnce(&Ui, Rect)) -> bool {
    let palette = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect.shrink2(Vec2::new(8., 2.)), CornerRadius::same(8), palette.fill);
    }
    let circle = Rect::from_min_size(Pos2::new(rect.left() + 16., rect.center().y - 18.), Vec2::splat(36.));
    lead(ui, circle);
    ui.painter().text(Pos2::new(circle.right() + 14., rect.center().y), Align2::LEFT_CENTER, label, theme::regular(14.), palette.text);
    response.clicked()
}

fn plus_circle(ui: &Ui, rect: Rect) {
    let palette = Palette::of(ui.ctx());
    ui.painter().circle_filled(rect.center(), 18., palette.fill_secondary);
    theme::paint_symbol(ui.painter(), "plus", rect.center(), 20., palette.text);
}

fn list_page(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    axo_list::list(ui, Some(tr("Profiles")), None, |ui| {
        if row(ui, tr("Create profile"), plus_circle) {
            let mut draft = NotificationProfile::new(langis_core::new_notification_profile_id(), String::new(), 0);
            draft.color = COLORS[(now() as usize / 7) % COLORS.len()];
            app.panes.profiles.view = View::Create(Create::Name, Box::new(draft));
        }
        for p in app.panes.profiles.all.clone() {
            if row(ui, &p.name, |ui, r| paint_avatar(ui.painter(), &palette, r, Some(&p), false)) {
                app.panes.profiles.view = View::Edit(p.id.clone());
            }
        }
    });
    // Desktop's onboarding, the first time with no profiles.
    if app.panes.profiles.onboarding == Some(true) {
        if !app.panes.profiles.all.is_empty() {
            dismiss_onboarding(app);
        } else if onboarding_dialog(ui) {
            dismiss_onboarding(app);
        }
    }
}

fn dismiss_onboarding(app: &mut App) {
    app.panes.profiles.onboarding = Some(false);
    crate::settings::write(app, SETTING_ONBOARDED, "true".into());
}

fn onboarding_dialog(ui: &Ui) -> bool {
    let palette = Palette::of(ui.ctx());
    crate::dialog::show_with(
        ui.ctx(),
        egui::Id::unique("notification-profiles-onboarding"),
        tr("Notification Profiles"),
        tr("Create a profile to receive notifications and calls only from the people and groups you choose"),
        None,
        &[(tr("Continue"), crate::dialog::Variant::StrongPrimary)],
        |ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 88.), Sense::hover());
            paint_avatar(ui.painter(), &palette, Rect::from_center_size(r.center(), Vec2::splat(80.)), None, false);
        },
    )
    .is_some()
}

/// Desktop's `NotificationProfilesNamePage`: the name with its emoji
/// button, then the sample names. True on Next (or Done when editing).
fn name_page(app: &mut App, ui: &mut Ui, name: &mut String, emoji: &mut Option<String>, editing: bool) -> bool {
    let palette = Palette::of(ui.ctx());
    if !editing {
        page_title(ui, tr("Name your profile"));
    }
    ui.add_space(24.);
    egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            // The emoji button: the chosen emoji, or the picker's icon.
            let (r, b) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
            match emoji.as_deref() {
                Some(e) => {
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, e, theme::emoji(20.), Color32::WHITE);
                }
                None => theme::paint_symbol(ui.painter(), "face-smiling", r.center(), 20., palette.text_secondary),
            }
            if b.clicked() {
                let p = &mut app.panes.profiles;
                p.emoji = if p.emoji.is_some() { None } else { Some(Default::default()) };
            }
            ui.add_space(6.);
            ui.add(egui::TextEdit::singleline(name).hint_text(tr("Profile name")).char_limit(140).desired_width(ui.available_width()).margin(egui::Margin::symmetric(10, 8)));
            if let Some(mut picker) = app.panes.profiles.emoji.take() {
                let mut still = true;
                if let Some(e) = picker.show(ui.ctx(), Pos2::new(r.left(), r.bottom() + 456.), &mut still, &app.recent_emoji) {
                    *emoji = Some(e.to_owned());
                    still = false;
                }
                if still {
                    app.panes.profiles.emoji = Some(picker);
                }
            }
        });
    });
    // Desktop clears the emoji with the name.
    if name.is_empty() {
        *emoji = None;
    }
    ui.add_space(12.);
    ui.vertical_centered(|ui| {
        ui.set_max_width(320.);
        for (e, sample) in SAMPLES {
            let sample = tr(sample);
            if row(ui, sample, |ui, r| {
                ui.painter().text(r.center(), Align2::CENTER_CENTER, e, theme::emoji(22.), Color32::WHITE);
            }) {
                *name = sample.to_owned();
                *emoji = Some(e.to_owned());
            }
        }
    });
    main_button(ui, if editing { tr("Done") } else { tr("Next") }, !name.trim().is_empty())
}

/// Desktop's `AllowedMembersSection`: the title, "Add people or groups",
/// and each allowed chat (a click opens the chooser).
fn allowed_section(app: &mut App, ui: &mut Ui, title: &str, members: &[ConversationId]) {
    axo_list::list(ui, Some(title), None, |ui| {
        let mut open = row(ui, tr("Add people or groups"), plus_circle);
        for id in members {
            let Some(c) = app.conversation(id).cloned() else { continue };
            open |= row(ui, &c.title, |ui, r| widgets::paint_avatar(ui, r, &widgets::AvatarSpec::of(&c)));
        }
        if open {
            app.panes.profiles.chooser = Some((String::new(), members.to_vec()));
        }
    });
}

/// Desktop's `ExceptionsSection`. True when something changed.
fn exceptions(ui: &mut Ui, profile: &mut NotificationProfile) -> bool {
    let mut changed = false;
    axo_list::gap(ui);
    axo_list::list(ui, Some(tr("Exceptions")), None, |ui| {
        changed |= axo_list::switch(ui, Item::new(tr("Allow all calls")), &mut profile.allow_all_calls);
        changed |= axo_list::switch(ui, Item::new(tr("Notify for all mentions")), &mut profile.allow_all_mentions);
    });
    changed
}

/// Takes the chooser's answer into `members`, once it's closed with Done.
fn take_chosen(app: &mut App, members: &mut Vec<ConversationId>) -> bool {
    let Some(chosen) = app.ctx.data_mut(|d| d.remove_temp::<Vec<ConversationId>>(egui::Id::unique("notification-profile-chosen"))) else { return false };
    *members = chosen;
    true
}

/// Desktop's `NotificationProfilesAllowedPage`. True on Next.
fn allowed_page(app: &mut App, ui: &mut Ui, draft: &mut NotificationProfile) -> bool {
    take_chosen(app, &mut draft.allowed_members);
    page_title(ui, tr("Allowed notifications"));
    centered_text(ui, tr("Add people and groups who you want notifications from when this profile is on"));
    allowed_section(app, ui, tr("Allowed notifications"), &draft.allowed_members.clone());
    exceptions(ui, draft);
    main_button(ui, tr("Next"), true)
}

/// Desktop's `TimePicker`: hour, minute and AM/PM (hour and minute in
/// 24-hour time outside English). Returns the new hhmm when changed.
fn time_picker(ui: &mut Ui, id: egui::Id, hhmm: u32, enabled: bool) -> Option<u32> {
    let (h, m) = (hhmm / 100 % 24, hhmm % 100 % 60);
    let twelve = crate::i18n::locale() == "en";
    let mut out = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            let hours: Vec<String> = if twelve { (1..=12).map(|x| x.to_string()).collect() } else { (0..24).map(|x| format!("{x:02}")).collect() };
            let hour_ix = if twelve { ((h + 11) % 12) as usize } else { h as usize };
            let refs: Vec<&str> = hours.iter().map(String::as_str).collect();
            if let Some(i) = axo_list::select_pill(ui, id.with("h"), 56., &refs, hour_ix) {
                let i = i as u32;
                let nh = if twelve { (i + 1) % 12 + if h >= 12 { 12 } else { 0 } } else { i };
                out = Some(nh * 100 + m);
            }
            // Every five minutes, and the minute set if it isn't one.
            let mut minutes: Vec<u32> = (0..60).step_by(5).collect();
            if !minutes.contains(&m) {
                minutes.push(m);
                minutes.sort();
            }
            let labels: Vec<String> = minutes.iter().map(|x| format!("{x:02}")).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let ix = minutes.iter().position(|x| *x == m).unwrap_or(0);
            if let Some(i) = axo_list::select_pill(ui, id.with("m"), 56., &refs, ix) {
                out = Some(h * 100 + minutes[i]);
            }
            if twelve {
                let periods = [tr("AM"), tr("PM")];
                if let Some(i) = axo_list::select_pill(ui, id.with("p"), 64., &periods, usize::from(h >= 12)) {
                    let nh = match (i, h >= 12) {
                        (1, false) => h + 12,
                        (0, true) => h - 12,
                        _ => h,
                    };
                    out = Some(nh * 100 + m);
                }
            }
        });
    });
    out
}

/// Desktop's `NotificationProfilesSchedulePage`. True on Next (or Done).
fn schedule_page(app: &mut App, ui: &mut Ui, profile: &mut NotificationProfile, editing: bool) -> bool {
    let _ = app;
    let palette = Palette::of(ui.ctx());
    if !editing {
        page_title(ui, tr("Add a schedule"));
        centered_text(ui, tr("Enable and edit your schedule to automate your notification profile."));
    }
    let on = profile.schedule_enabled;
    axo_list::list(ui, None, None, |ui| {
        axo_list::switch(ui, Item::new(tr("Turn on automatically")), &mut profile.schedule_enabled);
    });
    axo_list::gap(ui);
    let id = egui::Id::unique("notification-profile-schedule").with(&profile.id);
    axo_list::list(ui, Some(tr("Schedule")), None, |ui| {
        for (label, which) in [(tr("From"), 0), (tr("Until"), 1)] {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.), Sense::hover());
            let color = if on { palette.text } else { palette.text_secondary };
            ui.painter().text(Pos2::new(rect.left() + 16., rect.center().y), Align2::LEFT_CENTER, label, theme::regular(14.), color);
            let current = if which == 0 { profile.schedule_start.unwrap_or(900) } else { profile.schedule_end.unwrap_or(1700) };
            let width = if crate::i18n::locale() == "en" { 194. } else { 124. };
            let area = Rect::from_min_size(Pos2::new(rect.right() - 16. - width, rect.center().y - 14.), Vec2::new(width, 28.));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            if let Some(t) = time_picker(&mut child, id.with(which), current, on) {
                if which == 0 {
                    profile.schedule_start = Some(t);
                } else {
                    profile.schedule_end = Some(t);
                }
            }
        }
        ui.add_space(6.);
        for (i, long, _) in DAYS {
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 3, bottom: 3 }).show(ui, |ui| {
                ui.add_enabled_ui(on, |ui| {
                    let mut day = profile.schedule_days[i];
                    if widgets::checkbox(ui, &mut day, tr(long)).changed() {
                        profile.schedule_days[i] = day;
                    }
                });
            });
        }
        ui.add_space(6.);
    });
    main_button(ui, if editing { tr("Done") } else { tr("Next") }, true)
}

/// Desktop's `NotificationProfilesDonePage`. True on Done.
fn done_page(ui: &mut Ui, profile: &NotificationProfile) -> bool {
    let palette = Palette::of(ui.ctx());
    ui.add_space(48.);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.), Sense::hover());
    paint_avatar(ui.painter(), &palette, Rect::from_center_size(r.center(), Vec2::splat(80.)), Some(profile), false);
    page_title(ui, tr("Profile Created"));
    centered_text(ui, tr("If you’ve added a schedule, your profile will turn on and off automatically. You can also click the  ···  icon at the top of the chat list to turn it on or off manually."));
    main_button(ui, tr("Done"), true)
}

/// Desktop's `NotificationProfilesEditPage`: the profile with its name
/// (pencil to rename) and On/Off, who's allowed, the schedule, the
/// exceptions, and Delete profile.
fn edit_page(app: &mut App, ui: &mut Ui, mut profile: NotificationProfile) {
    let palette = Palette::of(ui.ctx());
    let mut changed = take_chosen(app, &mut profile.allowed_members);
    let is_active = active(app).is_some_and(|a| a.id == profile.id);
    // The header: avatar, name and pencil, then the On/Off select.
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.), Sense::click());
    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 24., rect.center().y - 18.), Vec2::splat(36.));
    paint_avatar(ui.painter(), &palette, avatar, Some(&profile), false);
    let name = ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y), Align2::LEFT_CENTER, &profile.name, theme::regular(15.), palette.text);
    if response.hovered() {
        theme::paint_symbol(ui.painter(), "pencil", Pos2::new(name.right() + 16., rect.center().y), 18., palette.text_secondary);
    }
    let options = [tr("On"), tr("Off")];
    let select = Rect::from_min_size(Pos2::new(rect.right() - 24. - 80., rect.center().y - 14.), Vec2::new(80., 28.));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(select));
    let picked = axo_list::select_pill(&mut child, egui::Id::unique("notification-profile-on").with(&profile.id), 80., &options, usize::from(!is_active));
    if let Some(i) = picked {
        set_override(app, &profile.id, i == 0, None);
    } else if response.clicked() && !select.contains(response.interact_pointer_pos().unwrap_or_default()) {
        app.panes.profiles.name_draft = None;
        app.panes.profiles.view = View::Name(profile.id.clone());
    }
    allowed_section(app, ui, tr("Allowed notifications"), &profile.allowed_members.clone());
    axo_list::gap(ui);
    let timing = tf(
        "icu:NotificationProfiles--edit--schedule-timing",
        &format!("{} to {}", time_label(profile.schedule_start.unwrap_or(900)), time_label(profile.schedule_end.unwrap_or(1700))),
        &[("startTime", Arg::from(&time_label(profile.schedule_start.unwrap_or(900)))), ("endTime", Arg::from(&time_label(profile.schedule_end.unwrap_or(1700))))],
    );
    let summary = schedule_summary(&profile.schedule_days);
    axo_list::list(ui, Some(tr("Schedule")), None, |ui| {
        let state = if profile.schedule_enabled { tr("On") } else { tr("Off") };
        if axo_list::item(ui, Item::new(&timing).description(&summary), End::Value(state)).clicked() {
            app.panes.profiles.view = View::Schedule(profile.id.clone());
        }
    });
    changed |= exceptions(ui, &mut profile);
    axo_list::gap(ui);
    axo_list::list(ui, None, None, |ui| {
        let mut item = Item::new(tr("Delete profile")).symbol("trash");
        item.destructive = true;
        if axo_list::item(ui, item, End::Click).clicked() {
            app.panes.profiles.delete_asked = true;
        }
    });
    if main_button(ui, tr("Done"), true) {
        app.panes.profiles.view = View::List;
    }
    if changed {
        save(app, profile);
    }
}

/// The flow's dialogs: the chats chooser and the delete confirmation.
pub fn dialogs(app: &mut App, ui: &mut Ui) {
    use langis_store::ConversationKind;
    if let Some((mut query, mut chosen)) = app.panes.profiles.chooser.take() {
        let palette = Palette::of(ui.ctx());
        let mut answer = None;
        let closed = crate::panes::modal(ui, "notification-profile-chats", 420., |ui| {
            crate::panes::title(ui, tr("Allowed notifications"));
            widgets::text_field(ui, &mut query, tr("Search"), false);
            ui.add_space(8.);
            let q = query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                let list: Vec<langis_store::Conversation> = app
                    .conversations
                    .iter()
                    .filter(|c| c.kind != ConversationKind::NoteToSelf)
                    .filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q))
                    .cloned()
                    .collect();
                for c in list {
                    let on = chosen.contains(&c.id);
                    let (rect, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.), Sense::click());
                    if hit.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                    }
                    let avatar = Rect::from_min_size(Pos2::new(rect.left() + 8., rect.center().y - 16.), Vec2::splat(32.));
                    widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(&c));
                    ui.painter().text(Pos2::new(avatar.right() + 12., rect.center().y), Align2::LEFT_CENTER, &c.title, theme::regular(14.), palette.text);
                    crate::panes::round_check(ui, &palette, Pos2::new(rect.right() - 20., rect.center().y), on);
                    if hit.clicked() {
                        if on {
                            chosen.retain(|x| *x != c.id);
                        } else {
                            chosen.push(c.id.clone());
                        }
                    }
                }
            });
            answer = crate::panes::buttons(ui, &[tr("Done"), tr("Cancel")]);
        });
        match answer {
            Some(0) => app.ctx.data_mut(|d| {
                d.insert_temp(egui::Id::unique("notification-profile-chosen"), chosen);
            }),
            Some(_) => {}
            None if closed => {}
            None => app.panes.profiles.chooser = Some((query, chosen)),
        }
    }
    if app.panes.profiles.delete_asked {
        let id = match &app.panes.profiles.view {
            View::Edit(id) => Some(id.clone()),
            _ => None,
        };
        let answer = crate::dialog::show(
            ui.ctx(),
            egui::Id::unique("notification-profile-delete"),
            "",
            tr("Permanently delete this notification profile?"),
            Some(tr("Cancel")),
            &[(tr("Delete"), crate::dialog::Variant::StrongDestructive)],
        );
        match answer {
            Some(crate::dialog::Answer::Action(_)) => {
                app.panes.profiles.delete_asked = false;
                if let Some(id) = id {
                    app.send(Command::EditNotificationProfiles(NotificationProfileEdit::Delete { id, at: now() }));
                }
                app.panes.profiles.view = View::List;
            }
            Some(crate::dialog::Answer::Cancel) => app.panes.profiles.delete_asked = false,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_read_as_desktops() {
        assert_eq!(schedule_summary(&[true, true, true, true, true, false, false]), "Weekdays");
        assert_eq!(schedule_summary(&[false, false, false, false, false, true, true]), "Weekends");
        assert_eq!(schedule_summary(&[true; 7]), "Daily");
        // Sunday first.
        assert_eq!(schedule_summary(&[true, false, true, false, false, false, true]), "Sun, Mon, Wed");
        assert_eq!(time_label(900), "9:00 AM");
        assert_eq!(time_label(0), "12:00 AM");
        assert_eq!(time_label(1230), "12:30 PM");
        assert_eq!(time_label(2345), "11:45 PM");
    }
}
