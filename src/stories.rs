//! Desktop's Stories tab: everyone's stories on the left, the viewer on the
//! right, the text and photo story composer, and story privacy (My Story,
//! custom stories, view receipts, turning stories off).

use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::{Command, OutgoingStory, StoryList, StoryTarget};
use langis_store::{ConversationId, Story, StoryBackground, StoryContent, TextStory};

use crate::app::{App, Loaded};
use crate::icons::Icon;
use crate::panes::{body, buttons, menu_row, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

/// Desktop's `getStoryDuration`: 5 seconds for a photo; a text story 1
/// more for each 15 characters past the first 15, and 2 more with a link
/// preview; a video its own length, up to 30 seconds.
const DEFAULT_TIME: Duration = Duration::from_secs(5);
const MAX_VIDEO_TIME: f32 = 30.;

fn story_time(s: &Story, player: Option<&crate::video::Player>) -> Duration {
    match &s.content {
        StoryContent::Text(t) => {
            let chars = t.text.chars().count() as u64;
            let extra = chars.div_ceil(15).saturating_sub(1);
            DEFAULT_TIME + Duration::from_secs(extra) + if t.preview.is_some() { Duration::from_secs(2) } else { Duration::ZERO }
        }
        StoryContent::Media { attachment, .. } if !attachment.content_type.starts_with("image/") => {
            match player.map(|p| p.duration()).filter(|d| *d > 0.) {
                Some(d) => Duration::from_secs_f32(d.min(MAX_VIDEO_TIME)),
                None => DEFAULT_TIME,
            }
        }
        _ => DEFAULT_TIME,
    }
}

/// Desktop's text story backgrounds (`BackgroundStyle`), ARGB.
const BACKGROUNDS: [u32; 8] = [0xff688bd4, 0xff8687c1, 0xffb47f8c, 0xff899188, 0xff539383, 0xff2c5f45, 0xff4437d8, 0xffd35a2f];

#[derive(Default)]
pub struct StoriesView {
    pub list: Vec<Story>,
    pub lists: Vec<StoryList>,
    pub disabled: bool,
    pub receipts: bool,
    pub loaded: bool,
    /// The open story: whose (see [`key`]), which, and since when.
    pub viewer: Option<(String, usize, Instant)>,
    pub reply: String,
    pub composer: Option<Composer>,
    pub privacy: bool,
    /// A story list being made or changed.
    pub editing: Option<StoryList>,
    /// The video story playing, its sound on.
    pub player: Option<crate::video::Player>,
    /// Paused since, by the pause button (Desktop pauses while you reply).
    pub paused: Option<Instant>,
    /// The full emoji picker, for a reaction.
    pub picker: Option<crate::emoji_picker::Picker>,
    /// Our story whose views and replies are listed.
    pub views_of: Option<i64>,
    /// The views modal's tab: Views, or Replies.
    pub views_tab: usize,
    /// Desktop's `StoryDetailsModal` (Info) for this story.
    pub info_of: Option<i64>,
    /// "Hide story?" waiting for this story's answer.
    pub confirm_hide: Option<i64>,
    /// The Story privacy page open, if not its first.
    pub privacy_page: Option<PrivacyPage>,
    /// The story whose whole caption is open (Read more).
    pub caption_open: Option<i64>,
    /// The stories list's search.
    pub search: String,
    /// "Delete this story?" waiting for this story's answer.
    pub confirm_delete: Option<i64>,
    /// The list's "Hide story?" (or Unhide) for a chat and its name.
    pub confirm_hide_chat: Option<(ConversationId, String)>,
    /// Chats whose stories are hidden (`stories_hidden`).
    pub hidden: Vec<ConversationId>,
    /// The Hidden stories section, opened.
    pub show_hidden: bool,
    /// Desktop's My Stories page, in place of the viewer.
    pub my_stories: bool,
    /// Groups' story send modes (New group story, Remove group story).
    pub send_modes: Vec<(ConversationId, langis_store::StorySendMode)>,
}

pub struct Composer {
    pub text: String,
    /// Index into the solid colors, then the gradients.
    pub background: usize,
    pub media: Option<(std::sync::Arc<Vec<u8>>, String, String)>,
    pub targets: Vec<StoryTarget>,
    /// Desktop's `TextStoryCreator` buttons: the font, and the text's
    /// style (white, black, or on a pill).
    pub font: langis_store::StoryFont,
    pub text_style: usize,
    /// The first link's preview, and the link it's for.
    pub preview: Option<langis_core::LinkPreviewDraft>,
    pub preview_for: Option<String>,
    /// Picking groups for New group story, with the search.
    pub new_group: Option<(Vec<ConversationId>, String)>,
    /// The link added with the link button; else the text's first.
    pub link: Option<String>,
    /// The link button's field, while it's open.
    pub link_field: Option<String>,
    /// `SendStoryModal`, after Next.
    pub sending: bool,
    /// "Discard story?" waiting for an answer.
    pub confirm_discard: bool,
    /// The media editor, on a photo story.
    pub editor: Option<crate::media_editor::Editor>,
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            text: String::new(),
            background: 0,
            media: None,
            targets: vec![StoryTarget::MyStory],
            font: langis_store::StoryFont::Regular,
            text_style: 0,
            preview: None,
            preview_for: None,
            new_group: None,
            link: None,
            link_field: None,
            sending: false,
            confirm_discard: false,
            editor: None,
        }
    }
}

/// Gradient backgrounds after the solid ones, top to bottom (ARGB).
const GRADIENTS: [(u32, u32); 6] = [
    (0xff19a9e0, 0xffe28a4c),
    (0xff4d2fbf, 0xffde4f8e),
    (0xff12976d, 0xff9dd149),
    (0xffbd4a2b, 0xfff2c14e),
    (0xff1d2b64, 0xfff8cdda),
    (0xff232526, 0xff5e6a71),
];

/// The fonts the font button cycles through.
const FONTS: [langis_store::StoryFont; 5] =
    [langis_store::StoryFont::Regular, langis_store::StoryFont::Bold, langis_store::StoryFont::Serif, langis_store::StoryFont::Script, langis_store::StoryFont::Condensed];

impl Composer {
    fn story_background(&self) -> StoryBackground {
        if self.background < BACKGROUNDS.len() {
            StoryBackground::Color(BACKGROUNDS[self.background])
        } else {
            let (a, b) = GRADIENTS[(self.background - BACKGROUNDS.len()).min(GRADIENTS.len() - 1)];
            StoryBackground::Gradient { colors: vec![a, b], positions: vec![0., 1.], angle: 180 }
        }
    }

    /// The text story as it'll go: its font, colors and background.
    fn text_story(&self, text: String) -> TextStory {
        let (foreground, text_background) = match self.text_style {
            1 => (Some(0xff000000), None),
            2 => (Some(0xff000000), Some(0xffffffff)),
            3 => (Some(0xffffffff), Some(0xff000000)),
            _ => (Some(0xffffffff), None),
        };
        TextStory { text, font: self.font, foreground, text_background, background: self.story_background(), ..Default::default() }
    }
}

pub fn load(app: &App) {
    app.load(|db| async move {
        let now = chrono::Utc::now().timestamp_millis();
        let list = db.stories(now).await.ok()?;
        let lists: Vec<StoryList> =
            db.setting(langis_core::SETTING_STORY_LISTS).await.ok().flatten().and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        let disabled = langis_core::setting_on(db.setting(langis_core::SETTING_STORIES_DISABLED).await.ok().flatten().as_deref());
        let receipts = db.setting(langis_core::SETTING_STORY_VIEW_RECEIPTS).await.ok().flatten().as_deref() != Some("false");
        let hidden: Vec<ConversationId> = db.setting(langis_store::HIDDEN_STORIES).await.ok().flatten().and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok()).unwrap_or_default().into_iter().map(ConversationId).collect();
        let send_modes = db.story_send_modes().await.unwrap_or_default();
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            let s = &mut app.panes.stories;
            s.list = list;
            s.lists = lists;
            s.disabled = disabled;
            s.receipts = receipts;
            s.hidden = hidden;
            s.send_modes = send_modes;
            s.loaded = true;
        })))
    });
}

/// Whose stories a story is among: a group's, ours, or a person's.
fn key(s: &Story) -> String {
    match (&s.group, &s.author) {
        (Some(group), _) => format!("g:{group}"),
        (None, None) => "me".into(),
        (None, Some(author)) => format!("p:{author}"),
    }
}

/// Stories by whose, newest group first; ours come separately.
fn groups(list: &[Story]) -> Vec<(String, Vec<Story>)> {
    let mut out: Vec<(String, Vec<Story>)> = Vec::new();
    for s in list {
        let k = key(s);
        match out.iter_mut().find(|(key, _)| *key == k) {
            Some((_, stories)) => stories.push(s.clone()),
            None => out.push((k, vec![s.clone()])),
        }
    }
    // Oldest first within each, as they play.
    for (_, stories) in &mut out {
        stories.sort_by_key(|s| s.sent_at);
    }
    out
}

fn argb(c: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

fn who(app: &App, k: &str, stories: &[Story]) -> String {
    if k == "me" {
        return tr("My Story").into();
    }
    if let Some(group) = stories.first().and_then(|s| s.group.as_ref()) {
        return app.conversation(group).map(|c| c.title.clone()).unwrap_or_else(|| "Group".into());
    }
    stories.first().and_then(|s| s.author_name.clone()).or_else(|| stories.first().and_then(|s| s.author.as_ref()).map(|a| app.name(a))).unwrap_or_default()
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = Palette::of(ui.ctx());
    if !app.panes.stories.loaded {
        app.panes.stories.loaded = true;
        load(app);
    }
    let all = groups(&app.panes.stories.list);
    egui::Panel::left("stories-list")
        .resizable(true)
        .default_size(300.)
        .min_size(240.)
        .max_size(420.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.sidebar))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(full.right() - 0.5, full.y_range(), Stroke::new(1., palette.border));
            ui.add_space(12.);
            ui.horizontal(|ui| {
                crate::sidebar::header_start(app, ui);
                ui.label(RichText::new(tr("Stories")).font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.);
                    let more = widgets::icon_button(ui, Icon::More, 20., tr("More actions"));
                    crate::menu::dropdown(ui, &more, egui::Id::unique("stories-more"), |m| {
                        if m.item(Some("lock"), tr("Story privacy")) {
                            app.panes.stories.privacy = true;
                        }
                    });
                    if !app.panes.stories.disabled {
                        // Desktop's StoriesAddStoryButton: a photo or video, or text.
                        let add = widgets::icon_button(ui, Icon::Plus, 20., tr("Add a story"));
                        let mut pick = None;
                        crate::menu::dropdown(ui, &add, egui::Id::unique("stories-add"), |m| {
                            if m.item(Some("photo"), tr("Photo or video")) {
                                pick = Some(true);
                            }
                            if m.item(Some("text-square"), tr("Text story")) {
                                pick = Some(false);
                            }
                        });
                        match pick {
                            Some(true) => new_media_story(app),
                            Some(false) => new_composer(app),
                            None => {}
                        }
                    }
                });
            });
            ui.add_space(8.);
            if app.panes.stories.disabled {
                ui.add_space(24.);
                ui.vertical_centered(|ui| {
                    body(ui, &palette, "Stories are turned off.");
                    if ui.link(tr("Turn on stories")).clicked() {
                        app.send(Command::SetStoriesDisabled(false));
                    }
                });
                return;
            }
            // Desktop's StoriesPane search.
            ui.horizontal(|ui| {
                ui.add_space(16.);
                ui.set_width(ui.available_width() - 16.);
                widgets::text_field(ui, &mut app.panes.stories.search, tr("Search"), false);
            });
            ui.add_space(6.);
            let q = app.panes.stories.search.trim().to_lowercase();
            let all: Vec<(String, Vec<Story>)> = all.iter().filter(|(k, s)| q.is_empty() || k == "me" || who(app, k, s).to_lowercase().contains(&q)).cloned().collect();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                // Ours first: "My Stories", or the prompt to add one.
                let mine = all.iter().find(|(k, _)| k == "me");
                let subtitle = match mine {
                    Some((_, s)) => crate::format::short_timestamp(s.last().map(|s| s.sent_at).unwrap_or_default()),
                    None => "Tap to add".into(),
                };
                if story_row(app, ui, &palette, tr("My Stories"), &subtitle, None, mine.and_then(|(_, s)| s.last()), false).clicked() {
                    match mine {
                        // Desktop's `MyStories` page lists them.
                        Some(_) => {
                            app.panes.stories.my_stories = true;
                            app.panes.stories.viewer = None;
                            app.panes.stories.player = None;
                        }
                        None => new_composer(app),
                    }
                }
                let hidden = app.panes.stories.hidden.clone();
                let is_hidden = |stories: &[Story]| stories.first().and_then(|s| s.group.clone().or_else(|| s.author.as_deref().map(ConversationId::direct))).is_some_and(|c| hidden.contains(&c));
                let (unread, read): (Vec<_>, Vec<_>) = all.iter().filter(|(k, s)| k != "me" && !is_hidden(s)).partition(|(_, s)| s.iter().any(|s| !s.read));
                for (heading, section) in [("", unread), (tr("Viewed"), read)] {
                    if section.is_empty() {
                        continue;
                    }
                    if !heading.is_empty() {
                        ui.add_space(8.);
                        ui.horizontal(|ui| {
                            ui.add_space(24.);
                            ui.label(RichText::new(heading).font(theme::semibold(13.)).color(palette.text_secondary));
                        });
                    }
                    for (k, stories) in section {
                        other_row(app, ui, &palette, k, stories, false);
                    }
                }
                // Desktop's Hidden stories, folded under the rest.
                let hidden_rows: Vec<_> = all.iter().filter(|(k, s)| k != "me" && is_hidden(s)).collect();
                if !hidden_rows.is_empty() {
                    ui.add_space(8.);
                    let label = format!("{} Hidden stories", if app.panes.stories.show_hidden { "▾" } else { "▸" });
                    ui.horizontal(|ui| {
                        ui.add_space(24.);
                        if ui.add(egui::Label::new(RichText::new(label).font(theme::semibold(13.)).color(palette.text_secondary)).sense(Sense::click())).clicked() {
                            app.panes.stories.show_hidden = !app.panes.stories.show_hidden;
                        }
                    });
                    if app.panes.stories.show_hidden {
                        for (k, stories) in hidden_rows {
                            other_row(app, ui, &palette, k, stories, true);
                        }
                    }
                }
                if all.iter().all(|(k, _)| k == "me") {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(tr("No stories")).font(theme::semibold(14.)));
                        body(ui, &palette, tr("New updates will appear here."));
                    });
                }
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect();
        match app.panes.stories.viewer.clone() {
            // Desktop's StoryViewer covers the whole window.
            Some((k, index, since)) => {
                let screen = ui.ctx().content_rect();
                egui::Area::new(egui::Id::unique("story-viewer")).fixed_pos(screen.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    ui.set_min_size(screen.size());
                    ui.set_max_size(screen.size());
                    viewer(app, ui, &palette, &all, &k, index, since);
                });
            }
            None if app.panes.stories.my_stories => my_stories(app, ui, &palette, &all),
            None => {
                ui.painter().rect_filled(rect, 0, palette.background);
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(tr("Click to view a story")).font(theme::regular(14.)).color(palette.text_secondary));
                });
            }
        }
    });
}

fn new_composer(app: &mut App) {
    app.panes.stories.composer = Some(Composer::default());
}

/// Desktop's "Photo or video" story: a file picked, into the creator.
fn new_media_story(app: &mut App) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Media"), &["png", "jpg", "jpeg", "webp", "gif", "mp4", "mov"]).pick_file() else { return };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
    if size > crate::composer::MAX_FILE_BYTES {
        app.toast("That file is too large.");
        return;
    }
    let Ok(bytes) = std::fs::read(&path) else { return };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let content_type = match ext.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        _ => "image/jpeg",
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let editor = if content_type.starts_with("image/") && content_type != "image/gif" { crate::media_editor::Editor::open(0, &bytes) } else { None };
    app.panes.stories.composer = Some(Composer { media: Some((std::sync::Arc::new(bytes), content_type.into(), name)), editor, ..Composer::default() });
}

/// Someone's (or a group's) row: opens their stories; its menu hides or
/// shows them and goes to the chat (Desktop's `StoryListItem`).
fn other_row(app: &mut App, ui: &mut Ui, palette: &Palette, k: &str, stories: &[Story], hidden: bool) {
    let name = who(app, k, stories);
    let last = stories.last().map(|s| s.sent_at).unwrap_or_default();
    let chat = stories.first().and_then(|s| s.group.clone().or_else(|| s.author.as_deref().map(ConversationId::direct)));
    let unread = stories.iter().any(|s| !s.read);
    let response = story_row(app, ui, palette, &name, &crate::format::short_timestamp(last), chat.as_ref(), stories.last(), unread && !hidden);
    if response.clicked() {
        let first = stories.iter().position(|s| !s.read).unwrap_or(0);
        app.panes.stories.viewer = Some((k.to_owned(), first, Instant::now()));
        app.panes.stories.my_stories = false;
        app.panes.stories.player = None;
        app.panes.stories.paused = None;
        app.panes.stories.reply.clear();
    }
    let Some(chat) = chat else { return };
    let menu = egui::Id::unique(("story-row-menu", k));
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        crate::menu::open_at(ui.ctx(), menu, at);
    }
    crate::menu::show(ui.ctx(), menu, |m| {
        // Desktop's StoryListItem: hiding asks first; unhiding doesn't.
        if m.item(Some(if hidden { "eye" } else { "eye-slash" }), if hidden { tr("Unhide story") } else { tr("Hide story") }) {
            if hidden {
                app.send(Command::SetChat { conversation: chat.clone(), change: langis_core::ChatChange::HideStory(false) });
            } else {
                app.panes.stories.confirm_hide_chat = Some((chat.clone(), name.clone()));
            }
        }
        if m.item(Some("message"), tr("Go to chat")) {
            app.open(chat.clone());
        }
    });
}

/// One row of the list: an avatar ringed when unread, a name and a time,
/// and the newest story small on the right (`StoryListItem__preview`).
#[allow(clippy::too_many_arguments)]
fn story_row(app: &App, ui: &mut Ui, palette: &Palette, name: &str, subtitle: &str, chat: Option<&ConversationId>, latest: Option<&Story>, unread: bool) -> egui::Response {
    let width = ui.available_width();
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, 64.), Sense::click());
    let rect = Rect::from_min_max(outer.min + Vec2::new(10., 2.), outer.max - Vec2::new(10., 2.));
    let open = app.panes.stories.viewer.as_ref().is_some_and(|(k, _, _)| chat.is_some_and(|c| k.ends_with(&c.to_string())));
    if open || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), if open { palette.fill_pressed } else { palette.fill });
    }
    let avatar = Rect::from_min_size(rect.min + Vec2::new(14., 8.), Vec2::splat(40.));
    if unread {
        ui.painter().circle_stroke(avatar.center(), 22.5, Stroke::new(2., palette.accent));
    }
    match chat.and_then(|c| app.conversation(c)) {
        Some(c) => widgets::paint_avatar(ui, avatar.shrink(2.), &widgets::AvatarSpec::of(c)),
        None => {
            let spec = widgets::AvatarSpec { name: &app.me.0, color: 0, photo: app.me.1.as_deref(), group: false, note_to_self: false };
            widgets::paint_avatar(ui, avatar.shrink(2.), &spec);
        }
    }
    let x = avatar.right() + 12.;
    ui.painter().text(Pos2::new(x, rect.top() + 12.), egui::Align2::LEFT_TOP, name, theme::semibold(14.), palette.text);
    ui.painter().text(Pos2::new(x, rect.top() + 32.), egui::Align2::LEFT_TOP, subtitle, theme::regular(12.), palette.text_secondary);
    if let Some(s) = latest {
        let thumb = Rect::from_min_size(Pos2::new(rect.right() - 12. - 32., rect.center().y - 28.), Vec2::new(32., 56.));
        story_thumb(ui, s, thumb);
    }
    response
}

/// A story small: a text story drawn small, a photo or video's picture.
fn story_thumb(ui: &Ui, s: &Story, rect: Rect) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    match &s.content {
        StoryContent::Text(t) => {
            let small = TextStory { text: String::new(), preview: None, ..t.clone() };
            let _ = paint_text_story(ui, &small, rect, None);
        }
        StoryContent::Media { attachment, .. } => {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::BLACK);
            let image = attachment.content_type.starts_with("image/");
            let poster = if image { None } else { attachment.local_path.as_deref().and_then(|p| crate::video::poster(ui.ctx(), p)) };
            if let Some(path) = attachment.local_path.as_deref().filter(|_| image).or(attachment.thumbnail_path.as_deref()).or(poster.as_deref()) {
                crate::chat::cover_image(ui, path, rect, CornerRadius::same(6));
            }
        }
    }
}

/// Desktop's `MyStories`: each of ours with its picture, views and time;
/// a click views it, and its menu saves or deletes it.
fn my_stories(app: &mut App, ui: &mut Ui, palette: &Palette, all: &[(String, Vec<Story>)]) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, palette.background);
    let mine: Vec<Story> = all.iter().find(|(k, _)| k == "me").map(|(_, s)| s.clone()).unwrap_or_default();
    if mine.is_empty() {
        app.panes.stories.my_stories = false;
        return;
    }
    let mut info = None;
    let mut delete = None;
    let mut view = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(full.shrink2(Vec2::new(32., 24.))), |ui| {
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, Icon::ChevronLeft, 20., tr("Back")).clicked() {
                app.panes.stories.my_stories = false;
            }
            ui.label(RichText::new(tr("My Stories")).font(theme::semibold(18.)));
        });
        ui.add_space(12.);
        // Desktop's MyStories: a section for each list or group they went
        // to, newest first in each (one sent to two lists is in both).
        let mut sections: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, s) in mine.iter().enumerate().rev() {
            let mut names: Vec<String> = match &s.group {
                Some(g) => vec![app.conversation(g).map(|c| c.title.clone()).unwrap_or_else(|| "Group".into())],
                None => {
                    let mut ids: Vec<&str> = s.recipients.iter().flat_map(|r| r.list_ids.iter().map(String::as_str)).collect();
                    ids.sort();
                    ids.dedup();
                    ids.iter()
                        .map(|id| match app.panes.stories.lists.iter().find(|l| l.id == *id) {
                            Some(l) if l.is_my_story() => tr("My Story").to_owned(),
                            Some(l) => l.name.clone(),
                            None if *id == langis_core::MY_STORY_ID => tr("My Story").to_owned(),
                            None => tr("Custom story").to_owned(),
                        })
                        .collect()
                }
            };
            if names.is_empty() {
                names.push(tr("My Story").into());
            }
            names.dedup();
            for name in names {
                match sections.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, list)) => list.push(i),
                    None => sections.push((name, vec![i])),
                }
            }
        }
        sections.sort_by_key(|(n, _)| n != tr("My Story"));
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (section, indices) in &sections {
                ui.add_space(8.);
                ui.label(RichText::new(section).font(theme::semibold(14.)).color(palette.text));
                ui.add_space(4.);
            for &i in indices {
                let s = &mine[i];
                let (row, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.), Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(row, CornerRadius::same(12), palette.fill);
                }
                story_thumb(ui, s, Rect::from_min_size(row.min + Vec2::new(10., 6.), Vec2::new(40., 72.)));
                let views = if app.panes.stories.receipts { crate::i18n::tf("icu:MyStories__views", &format!("{} views", s.views.len()), &[("views", crate::i18n::Arg::from(&s.views.len()))]) } else { tr("Views off").to_owned() };
                ui.painter().text(Pos2::new(row.left() + 64., row.top() + 22.), egui::Align2::LEFT_TOP, views, theme::semibold(14.), palette.text);
                ui.painter().text(Pos2::new(row.left() + 64., row.top() + 44.), egui::Align2::LEFT_TOP, crate::format::short_timestamp(s.sent_at), theme::regular(12.), palette.text_secondary);
                if r.clicked() {
                    view = Some(i);
                }
                let menu = egui::Id::unique(("my-story-menu", s.id, section.as_str()));
                if r.secondary_clicked()
                    && let Some(at) = r.interact_pointer_pos()
                {
                    crate::menu::open_at(ui.ctx(), menu, at);
                }
                // Desktop's row buttons: Download (a photo or video), then
                // More options.
                let more = Rect::from_center_size(Pos2::new(row.right() - 24., row.center().y), Vec2::splat(32.));
                let more_hit = ui.interact(more, ui.scope_id().with(("my-story-more", s.id, section.as_str())), Sense::click());
                ui.painter().circle_filled(more.center(), 16., if more_hit.hovered() { palette.fill_pressed } else { palette.fill });
                theme::paint_symbol(ui.painter(), "more", more.center(), 18., palette.text);
                if more_hit.on_hover_text(tr("More options")).clicked() {
                    crate::menu::open_at(ui.ctx(), menu, more.left_bottom());
                }
                if let StoryContent::Media { attachment, .. } = &s.content
                    && let Some(path) = &attachment.local_path
                {
                    let dl = more.translate(Vec2::new(-40., 0.));
                    let dl_hit = ui.interact(dl, ui.scope_id().with(("my-story-dl", s.id, section.as_str())), Sense::click());
                    ui.painter().circle_filled(dl.center(), 16., if dl_hit.hovered() { palette.fill_pressed } else { palette.fill });
                    theme::paint_symbol(ui.painter(), "download", dl.center(), 18., palette.text);
                    if dl_hit.on_hover_text(tr("Download")).clicked() {
                        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "story".into());
                        if let Some(to) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                            let _ = std::fs::copy(path, to);
                        }
                    }
                }
                crate::menu::show(ui.ctx(), menu, |m| {
                    if let StoryContent::Media { attachment, .. } = &s.content
                        && let Some(path) = &attachment.local_path
                        && m.item(Some("download"), tr("Download"))
                    {
                        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "story".into());
                        if let Some(to) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                            let _ = std::fs::copy(path, to);
                        }
                    }
                    if m.item(Some("info"), tr("Info")) {
                        info = Some(s.id);
                    }
                    if m.item(Some("trash"), tr("Delete")) {
                        delete = Some(s.id);
                    }
                });
            }
            }
        });
    });
    if let Some(id) = info {
        app.panes.stories.info_of = Some(id);
    }
    // Its Info dialog, here as in the viewer.
    if let Some(id) = app.panes.stories.info_of
        && let Some(story) = mine.iter().find(|s| s.id == id).cloned()
    {
        info_modal(app, ui, &story);
    }
    if let Some(i) = view {
        app.panes.stories.my_stories = false;
        app.panes.stories.viewer = Some(("me".into(), i, Instant::now()));
    }
    if let Some(id) = delete {
        app.panes.stories.confirm_delete = Some(id);
    }
}

/// Paints a story's content into `rect`.
fn paint_story(ui: &Ui, s: &Story, rect: Rect) -> Option<Rect> {
    paint_story_caption(ui, s, rect, true).0
}

/// A story, its caption at most three lines unless `expanded`; returns the
/// link card, and the caption's rect when it was cut short (Desktop's Read
/// more).
fn paint_story_caption(ui: &Ui, s: &Story, rect: Rect, expanded: bool) -> (Option<Rect>, Option<Rect>) {
    match &s.content {
        StoryContent::Text(t) => (paint_text_story(ui, t, rect, None).card, None),
        StoryContent::Media { attachment, caption, .. } => {
            ui.painter().rect_filled(rect, CornerRadius::same(12), Color32::BLACK);
            match &attachment.local_path {
                Some(path) if attachment.content_type.starts_with("image/") => {
                    let size = Vec2::new(attachment.width.unwrap_or(9) as f32, attachment.height.unwrap_or(16) as f32);
                    let scale = (rect.width() / size.x).min(rect.height() / size.y);
                    let shown = Rect::from_center_size(rect.center(), size * scale);
                    egui::Image::new(crate::thumbs::uri(ui.ctx(), path, shown.size())).corner_radius(CornerRadius::same(12)).paint_at(ui, shown);
                }
                Some(path) => {
                    // A video: Desktop plays it; here it opens in the player.
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Video story", theme::medium(16.), Color32::WHITE);
                    let _ = path;
                }
                None => {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Downloading…", theme::regular(14.), Color32::from_white_alpha(200));
                }
            }
            let mut cut = None;
            if !caption.is_empty() {
                let mut job = egui::text::LayoutJob::simple(caption.clone(), theme::regular(14.), Color32::WHITE, rect.width() - 32.);
                if !expanded {
                    job.wrap.max_rows = 3;
                    job.wrap.overflow_character = Some('\u{2026}');
                }
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                let elided = galley.elided;
                let extra = if elided { 20. } else { 0. };
                let at = Pos2::new(rect.left() + 16., rect.bottom() - galley.size().y - extra - 16.);
                let back = Rect::from_min_size(at - Vec2::splat(6.), galley.size() + Vec2::new(12., 12. + extra));
                ui.painter().rect_filled(back, CornerRadius::same(8), Color32::from_black_alpha(120));
                ui.painter().galley(at, galley.clone(), Color32::WHITE);
                if elided {
                    ui.painter().text(Pos2::new(at.x, at.y + galley.size().y + 2.), egui::Align2::LEFT_TOP, tr("Read more"), theme::semibold(13.), Color32::WHITE);
                    cut = Some(back);
                }
            }
            (None, cut)
        }
    }
}

/// A text story's link card: what it's titled, where it goes, and its
/// picture (a sent story's file, or a draft's bytes).
pub struct Card {
    pub title: String,
    pub url: String,
    pub image: Option<egui::ImageSource<'static>>,
}

impl Card {
    fn of_story(p: &langis_store::LinkPreview, ctx: &egui::Context) -> Self {
        let image = p.image.as_ref().and_then(|i| i.local_path.as_deref()).map(|path| crate::thumbs::uri(ctx, path, Vec2::splat(96.)));
        Self { title: p.title.clone().unwrap_or_default(), url: p.url.clone(), image: image.map(|u| egui::ImageSource::Uri(u.into())) }
    }

    fn of_draft(p: &langis_core::LinkPreviewDraft) -> Self {
        let image = p.image.as_ref().map(|i| egui::ImageSource::Bytes { uri: format!("bytes://story-link/{}", p.url).into(), bytes: i.bytes.to_vec().into() });
        Self { title: p.title.clone(), url: p.url.clone(), image }
    }
}

fn domain(url: &str) -> String {
    url.split("://").nth(1).and_then(|r| r.split(['/', '?', '#']).next()).unwrap_or(url).trim_start_matches("www.").to_owned()
}

/// A gradient over `rect` at a CSS angle (0 runs bottom to top, 90 left to
/// right), from the first color to the last.
fn paint_gradient(ui: &Ui, rect: Rect, colors: &[u32], angle: u32) {
    let (from, to) = (argb(*colors.first().unwrap_or(&0xff000000)), argb(*colors.last().unwrap_or(&0xff000000)));
    let a = (angle as f32).to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let along = corners.map(|p| (p - rect.center()).dot(dir));
    let (lo, hi) = along.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let mut mesh = egui::Mesh::default();
    for (p, v) in corners.iter().zip(along) {
        let t = if hi > lo { (v - lo) / (hi - lo) } else { 0. };
        mesh.colored_vertex(*p, from.lerp_to_gamma(to, t));
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// A text story's font at `size` (Desktop's five; the app's own faces stand
/// in for its serif, script and condensed ones).
fn story_font(font: langis_store::StoryFont, size: f32) -> egui::FontId {
    match font {
        langis_store::StoryFont::Bold | langis_store::StoryFont::Condensed => theme::semibold(size),
        langis_store::StoryFont::Script | langis_store::StoryFont::Serif => theme::italic(size),
        _ => theme::medium(size),
    }
}

/// The text's size: Desktop shrinks it as it grows.
fn text_size(text: &str, scale: f32) -> f32 {
    let n = text.chars().count();
    scale * if n < 50 { 28. } else if n < 200 { 22. } else { 16. }
}

/// Where a text story's text and link card were painted.
#[derive(Default)]
struct Painted {
    text: Option<Rect>,
    card: Option<Rect>,
}

/// Paints a text story into `rect`, its link card under the text (or alone
/// in the middle).
fn paint_text_story(ui: &Ui, t: &TextStory, rect: Rect, draft: Option<&langis_core::LinkPreviewDraft>) -> Painted {
    match &t.background {
        StoryBackground::Color(c) => {
            ui.painter().rect_filled(rect, CornerRadius::same(12), argb(*c));
        }
        StoryBackground::Gradient { colors, angle, .. } => paint_gradient(ui, rect, colors, *angle),
    }
    let scale = rect.width() / 360.;
    let card = draft.map(Card::of_draft).or_else(|| t.preview.as_ref().map(|p| Card::of_story(p, ui.ctx())));
    let card_size = Vec2::new(rect.width() - 48. * scale, 88. * scale);
    let color = t.foreground.map(argb).unwrap_or(Color32::WHITE);
    let galley = (!t.text.is_empty()).then(|| {
        let mut job = egui::text::LayoutJob::default();
        crate::chat::append_with_emoji(&mut job, &t.text, egui::TextFormat { font_id: story_font(t.font, text_size(&t.text, scale)), color, ..Default::default() });
        job.wrap.max_width = rect.width() - 48. * scale;
        job.halign = egui::Align::Center;
        ui.fonts_mut(|f| f.layout_job(job))
    });
    let gap = 16. * scale;
    let total = galley.as_ref().map_or(0., |g| g.size().y) + card.as_ref().map_or(0., |_| card_size.y + if galley.is_some() { gap } else { 0. });
    let mut y = rect.center().y - total / 2.;
    let mut painted = Painted::default();
    if let Some(galley) = galley {
        let size = galley.size();
        if let Some(bg) = t.text_background {
            let r = Rect::from_center_size(Pos2::new(rect.center().x, y + size.y / 2.), size + Vec2::new(24., 16.) * scale);
            ui.painter().rect_filled(r, CornerRadius::same((10. * scale) as u8), argb(bg));
        }
        painted.text = Some(galley.rect.translate(Vec2::new(rect.center().x, y)));
        ui.painter().galley(Pos2::new(rect.center().x, y), galley, color);
        y += size.y + gap;
    }
    let Some(card) = card else { return painted };
    let r = Rect::from_min_size(Pos2::new(rect.center().x - card_size.x / 2., y), card_size);
    // Desktop's TextAttachment preview: white, the picture on the left.
    ui.painter().rect_filled(r, CornerRadius::same((12. * scale) as u8), Color32::from_white_alpha(235));
    let mut text_left = r.left() + 12. * scale;
    if let Some(image) = card.image {
        let pic = Rect::from_min_size(r.min + Vec2::splat(8. * scale), Vec2::splat(r.height() - 16. * scale));
        egui::Image::new(image).corner_radius(CornerRadius::same((8. * scale) as u8)).fit_to_exact_size(pic.size()).paint_at(ui, pic);
        text_left = pic.right() + 10. * scale;
    }
    let width = r.right() - 12. * scale - text_left;
    if width > 8. {
        let mut job = egui::text::LayoutJob::simple(card.title, theme::semibold(14. * scale), Color32::BLACK, width);
        job.wrap.max_rows = 2;
        let title = ui.fonts_mut(|f| f.layout_job(job));
        let domain = ui.painter().layout_no_wrap(domain(&card.url), theme::regular(12. * scale), Color32::from_gray(90));
        let h = title.size().y + 4. * scale + domain.size().y;
        let top = r.center().y - h / 2.;
        let below = top + title.size().y + 4. * scale;
        ui.painter().galley(Pos2::new(text_left, top), title, Color32::BLACK);
        ui.painter().galley(Pos2::new(text_left, below), domain, Color32::from_gray(90));
    }
    painted.card = Some(r);
    painted
}

/// Desktop's `StoryViewer`, in the right pane.
fn viewer(app: &mut App, ui: &mut Ui, palette: &Palette, all: &[(String, Vec<Story>)], k: &str, index: usize, since: Instant) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, Color32::from_gray(18));
    let Some(pos) = all.iter().position(|(key, _)| key == k) else {
        app.panes.stories.viewer = None;
        return;
    };
    let stories = &all[pos].1;
    let Some(story) = stories.get(index).cloned() else {
        app.panes.stories.viewer = None;
        return;
    };
    let mine = story.author.is_none();
    // Shown: viewed, and its media fetched.
    if !story.read && !mine {
        app.send(Command::ViewStory { id: story.id });
        if let Some(s) = app.panes.stories.list.iter_mut().find(|s| s.id == story.id) {
            s.read = true;
        }
    }
    if let StoryContent::Media { attachment, .. } = &story.content
        && attachment.local_path.is_none()
    {
        let key = egui::Id::unique(("story-fetch", story.id));
        if !ui.ctx().data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
            ui.ctx().data_mut(|d| d.insert_temp(key, true));
            app.send(Command::FetchStory { id: story.id });
        }
    }
    // A video story plays with its sound; its player follows the story.
    let video_path = match &story.content {
        StoryContent::Media { attachment, .. } if !attachment.content_type.starts_with("image/") => attachment.local_path.clone(),
        _ => None,
    };
    match &video_path {
        Some(path) if app.panes.stories.player.as_ref().is_none_or(|p| p.path() != path) => {
            let ppp = ui.ctx().pixels_per_point();
            app.panes.stories.player = Some(crate::video::Player::new(path, ((full.width() * ppp) as u32, (full.height() * ppp) as u32), false, false));
        }
        None => app.panes.stories.player = None,
        _ => {}
    }
    // The story, 9:16, centered.
    let height = (full.height() - 140.).max(200.);
    let width = (height * 9. / 16.).min(full.width() - 120.);
    let rect = Rect::from_center_size(Pos2::new(full.center().x, full.top() + 24. + height / 2.), Vec2::new(width, height));
    // A text story's link opens from its card (Desktop's TextAttachment).
    let (card, cut) = paint_story_caption(ui, &story, rect, app.panes.stories.caption_open == Some(story.id));
    // Desktop's Read more opens the whole caption.
    if let Some(cut) = cut
        && ui.interact(cut, ui.scope_id().with("story-read-more"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    {
        app.panes.stories.caption_open = Some(story.id);
        app.panes.stories.paused.get_or_insert_with(Instant::now);
    }
    if let Some(card) = card
        && let StoryContent::Text(TextStory { preview: Some(p), .. }) = &story.content
    {
        let hit = ui.interact(card, ui.scope_id().with("story-link"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(domain(&p.url));
        if hit.clicked() && !langis_core::link_preview::is_link_sneaky(&p.url) {
            app.panes.stories.paused.get_or_insert_with(Instant::now);
            crate::chat::open_link(app, &p.url);
        }
    }
    if let Some(player) = app.panes.stories.player.as_mut()
        && let Some(texture) = player.texture(ui.ctx())
    {
        let size = texture.size_vec2();
        let scale = (rect.width() / size.x).min(rect.height() / size.y);
        let shown = Rect::from_center_size(rect.center(), size * scale);
        egui::Image::new((texture.id(), shown.size())).corner_radius(CornerRadius::same(12)).paint_at(ui, shown);
    }
    // Paused by the button, while replying, or while picking a reaction;
    // the clock (and a video) stand still meanwhile.
    let replying = ui.memory(|m| m.focused() == Some(egui::Id::unique("story-reply-field"))) || !app.panes.stories.reply.is_empty();
    let held = replying || app.panes.stories.picker.is_some() || app.panes.stories.views_of.is_some() || app.panes.stories.info_of.is_some() || app.panes.stories.confirm_hide.is_some() || app.panes.stories.confirm_delete.is_some();
    let paused = app.panes.stories.paused.is_some() || held;
    let elapsed = match app.panes.stories.paused {
        Some(at) => at.duration_since(since),
        None => since.elapsed(),
    };
    if let Some(player) = app.panes.stories.player.as_mut() {
        if paused && player.playing() {
            player.pause();
        } else if !paused && !player.playing() && !player.ended() {
            player.resume();
        }
    }
    let total = story_time(&story, app.panes.stories.player.as_ref());
    let elapsed = match app.panes.stories.player.as_ref() {
        Some(p) => Duration::from_secs_f32(p.position()),
        None => elapsed,
    };
    // Progress bars over the top.
    let seg = (rect.width() - 16. - 4. * (stories.len() as f32 - 1.)) / stories.len() as f32;
    for i in 0..stories.len() {
        let x = rect.left() + 8. + i as f32 * (seg + 4.);
        let bar = Rect::from_min_size(Pos2::new(x, rect.top() + 8.), Vec2::new(seg, 3.));
        ui.painter().rect_filled(bar, CornerRadius::same(2), Color32::from_white_alpha(80));
        let done = if i < index { 1. } else if i == index { (elapsed.as_secs_f32() / total.as_secs_f32()).min(1.) } else { 0. };
        ui.painter().rect_filled(Rect::from_min_size(bar.min, Vec2::new(seg * done, 3.)), CornerRadius::same(2), Color32::WHITE);
    }
    // Desktop's StoryViewer header: the avatar, who (and, in a group, by
    // whom) and when, then the "…" menu, pause and (for a video) mute.
    let name = who(app, k, stories);
    let author = if story.group.is_some() { story.author_name.clone().map(|a| format!("{a} \u{b7} ")).unwrap_or_default() } else { String::new() };
    let avatar = Rect::from_min_size(rect.left_top() + Vec2::new(12., 20.), Vec2::splat(28.));
    match story.group.as_ref().and_then(|g| app.conversation(g)).or_else(|| story.author.as_ref().and_then(|a| app.conversation(&ConversationId::direct(a)))) {
        Some(c) => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c)),
        None if mine => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo: app.me.1.as_deref(), ..widgets::AvatarSpec::person(&app.me.0, 0) }),
        None => widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::person(&name, 0)),
    }
    // Desktop's "{name} to {group}" for a group's story.
    let header = if story.group.is_some() {
        let from = if mine { tr("You").to_owned() } else { story.author_name.clone().unwrap_or_else(|| story.author.as_deref().map(|a| app.name(a)).unwrap_or_default()) };
        crate::i18n::tf("icu:Stories__from-to-group", &format!("{from} to {name}"), &[("name", crate::i18n::Arg::from(&from)), ("group", crate::i18n::Arg::from(&name))])
    } else {
        let _ = &author;
        name.clone()
    };
    ui.painter().text(Pos2::new(avatar.right() + 8., avatar.center().y), egui::Align2::LEFT_CENTER, header, theme::semibold(13.), Color32::WHITE);
    let pause = Rect::from_center_size(Pos2::new(rect.right() - 22., rect.top() + 34.), Vec2::splat(28.));
    let time = ui.painter().layout_no_wrap(crate::format::short_timestamp(story.sent_at), theme::regular(13.), Color32::from_white_alpha(200));
    let more = Rect::from_center_size(Pos2::new(pause.left() - 18. - if app.panes.stories.player.is_some() { 36. } else { 0. }, pause.center().y), Vec2::splat(28.));
    ui.painter().galley(Pos2::new(more.left() - 8. - time.size().x, more.center().y - time.size().y / 2.), time, Color32::from_white_alpha(200));
    let more_hit = ui.interact(more, ui.scope_id().with("story-more"), Sense::click());
    theme::paint_symbol(ui.painter(), "more", more.center(), 20., Color32::WHITE);
    let mut menu_choice = None;
    crate::menu::dropdown(ui, &more_hit, egui::Id::unique("story-viewer-menu"), |m| {
        // Desktop's StoryViewer context menu.
        if m.item(Some("info"), tr("Info")) {
            menu_choice = Some("info");
        }
        if mine {
            if m.item(Some("trash"), tr("Delete")) {
                menu_choice = Some("delete");
            }
        } else {
            if m.item(Some("x-circle"), tr("Hide story")) {
                menu_choice = Some("hide");
            }
            if m.item(Some("open"), tr("Go to chat")) {
                menu_choice = Some("chat");
            }
        }
    });
    if menu_choice.is_some() {
        app.panes.stories.paused.get_or_insert_with(Instant::now);
    }
    match menu_choice {
        Some("info") => app.panes.stories.info_of = Some(story.id),
        Some("delete") => app.panes.stories.confirm_delete = Some(story.id),
        Some("hide") => app.panes.stories.confirm_hide = Some(story.id),
        Some("chat") => {
            let chat = story.group.clone().or_else(|| story.author.as_ref().map(|a| ConversationId::direct(a)));
            app.panes.stories.viewer = None;
            app.panes.stories.player = None;
            if let Some(chat) = chat {
                app.open(chat);
            }
            return;
        }
        _ => {}
    }
    let pause_hit = ui.interact(pause, ui.scope_id().with("story-pause"), Sense::click());
    theme::paint_symbol(ui.painter(), if app.panes.stories.paused.is_some() { "play-fill" } else { "pause-fill" }, pause.center(), 18., Color32::WHITE);
    if pause_hit.on_hover_text(if app.panes.stories.paused.is_some() { tr("Play") } else { tr("Pause") }).clicked() {
        match app.panes.stories.paused.take() {
            // Resuming moves the start on by the time paused.
            Some(at) => app.panes.stories.viewer = Some((k.to_owned(), index, since + at.elapsed())),
            None => app.panes.stories.paused = Some(Instant::now()),
        }
    }
    if let Some(player) = app.panes.stories.player.as_mut() {
        let mute = Rect::from_center_size(Pos2::new(pause.left() - 18., pause.center().y), Vec2::splat(28.));
        let r = ui.interact(mute, ui.scope_id().with("story-mute"), Sense::click());
        theme::paint_symbol(ui.painter(), if player.muted { "speaker-slash" } else { "speaker" }, mute.center(), 18., Color32::WHITE);
        if r.on_hover_text(if player.muted { tr("Unmute") } else { tr("Mute") }).clicked() {
            let muted = !player.muted;
            player.set_muted(muted);
        }
    }
    // Close, and the arrows either side.
    let close = Rect::from_min_size(Pos2::new(full.right() - 52., full.top() + 12.), Vec2::splat(40.));
    let close_hit = ui.interact(close, ui.scope_id().with("story-close"), Sense::click());
    Icon::X.paint(ui, close, 22., Color32::WHITE);
    let prev = Rect::from_center_size(Pos2::new(rect.left() - 36., rect.center().y), Vec2::splat(40.));
    let next = Rect::from_center_size(Pos2::new(rect.right() + 36., rect.center().y), Vec2::splat(40.));
    let prev_hit = ui.interact(prev, ui.scope_id().with("story-prev"), Sense::click());
    let next_hit = ui.interact(next, ui.scope_id().with("story-next"), Sense::click());
    for (r, hit, symbol) in [(prev, &prev_hit, "chevron-[start]"), (next, &next_hit, "chevron-[end]")] {
        ui.painter().circle_filled(r.center(), 20., Color32::from_white_alpha(if hit.hovered() { 50 } else { 25 }));
        theme::paint_symbol(ui.painter(), symbol, r.center(), 22., Color32::WHITE);
    }
    let keys = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::Escape)));
    let mut go: Option<isize> = None;
    if prev_hit.clicked() || (keys.0 && !replying) {
        go = Some(-1);
    }
    let finished = elapsed >= total || app.panes.stories.player.as_ref().is_some_and(|p| p.ended());
    if next_hit.clicked() || (keys.1 && !replying) || (finished && !paused) {
        go = Some(1);
    }
    if close_hit.clicked() || (keys.2 && app.panes.stories.picker.is_none() && app.panes.stories.views_of.is_none() && app.panes.stories.info_of.is_none() && app.panes.stories.confirm_hide.is_none() && app.panes.stories.confirm_delete.is_none()) {
        app.panes.stories.viewer = None;
        app.panes.stories.player = None;
        app.panes.stories.paused = None;
        return;
    }
    // Below: ours show views and delete; theirs take a reaction or reply.
    let below = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() + 12.), Pos2::new(rect.right(), full.bottom() - 12.));
    let mut open_picker = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(below), |ui| {
        ui.horizontal(|ui| {
            if mine {
                let views = story.views.len();
                let receipts = app.panes.stories.receipts;
                let text = if receipts { format!("{views} views · {} replies", story.replies.len()) } else { format!("Views off · {} replies", story.replies.len()) };
                // Desktop's `StoryViewsNRepliesModal`, from the count.
                if ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    app.panes.stories.views_tab = if receipts { 0 } else { 1 };
                    app.panes.stories.views_of = Some(story.id);
                }

            } else if story.allows_replies {
                // The chat's preferred reactions, then the full picker.
                for emoji in app.reactions.clone() {
                    let mut job = egui::text::LayoutJob::default();
                    crate::chat::append_with_emoji(&mut job, &emoji, egui::TextFormat { font_id: theme::regular(20.), color: Color32::WHITE, ..Default::default() });
                    if ui.add(egui::Button::new(job).frame(false)).on_hover_text("React").clicked() {
                        app.send(Command::ReplyToStory { id: story.id, body: String::new(), emoji: Some(emoji.clone()) });
                        app.toast("Reaction sent.");
                    }
                }
                if ui.add(egui::Button::new(RichText::new("+").font(theme::semibold(18.)).color(Color32::WHITE)).frame(false)).on_hover_text("More reactions").clicked() {
                    open_picker = true;
                }
                // `StoryViewer__reply-placeholder` / `__reply-group`.
                let first = story.author_name.as_deref().or(Some(name.as_str())).and_then(|n| n.split_whitespace().next()).unwrap_or_default().to_owned();
                let hint = if story.group.is_some() { tr("Reply to Group").to_owned() } else { crate::i18n::tf("icu:StoryViewer__reply-placeholder", &format!("Reply to {first}"), &[("firstName", crate::i18n::Arg::from(&first))]) };
                let field = ui.add(egui::TextEdit::singleline(&mut app.panes.stories.reply).id(egui::Id::unique("story-reply-field")).hint_text(hint).desired_width(ui.available_width() - 70.));
                let send = ui.button(tr("Send")).clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send && !app.panes.stories.reply.trim().is_empty() {
                    let body = std::mem::take(&mut app.panes.stories.reply);
                    app.send(Command::ReplyToStory { id: story.id, body: body.trim().to_owned(), emoji: None });
                    app.toast("Reply sent.");
                }
            }
        });
        // A group story's replies open Desktop's modal from their count.
        if !mine && story.group.is_some() {
            let n = story.replies.len();
            let text = match n {
                0 => tr("Reply to Group").to_owned(),
                1 => "1 reply".to_owned(),
                n => crate::i18n::tf("icu:MyStories__replies", &format!("{n} replies"), &[("replyCount", crate::i18n::Arg::from(&n))]),
            };
            ui.add_space(6.);
            if ui.add(egui::Button::new(RichText::new(text).font(theme::semibold(13.)).color(Color32::WHITE)).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                app.panes.stories.views_tab = 1;
                app.panes.stories.views_of = Some(story.id);
            }
        }
    });
    if open_picker {
        app.panes.stories.picker = Some(Default::default());
    }
    // The full emoji picker, for any reaction.
    if let Some(mut picker) = app.panes.stories.picker.take() {
        let mut open = true;
        let recent = app.recent_emoji.clone();
        if let Some(emoji) = picker.show(ui.ctx(), Pos2::new(below.left(), below.top()), &mut open, &recent) {
            app.use_emoji(emoji);
            app.send(Command::ReplyToStory { id: story.id, body: String::new(), emoji: Some(emoji.to_owned()) });
            app.toast("Reaction sent.");
        } else if open {
            app.panes.stories.picker = Some(picker);
        }
    }
    views_modal(app, ui, &story);
    info_modal(app, ui, &story);
    if app.panes.stories.confirm_hide == Some(story.id) {
        let body = format!("Hide story? New story updates from {name} won\u{2019}t appear at the top of the stories list anymore.");
        match crate::dialog::confirm(ui.ctx(), egui::Id::unique("hide-story"), "", &body, tr("Hide"), crate::dialog::Variant::StrongPrimary) {
            Some(crate::dialog::Choice::Action) => {
                app.panes.stories.confirm_hide = None;
                let chat = story.group.clone().or_else(|| story.author.as_ref().map(|a| ConversationId::direct(a)));
                if let Some(chat) = chat {
                    app.send(Command::SetChat { conversation: chat, change: langis_core::ChatChange::HideStory(true) });
                }
                app.panes.stories.viewer = None;
                app.panes.stories.player = None;
                return;
            }
            Some(crate::dialog::Choice::Cancel) => app.panes.stories.confirm_hide = None,
            None => {}
        }
    }
    if let Some(step) = go {
        let next = index as isize + step;
        app.panes.stories.reply.clear();
        app.panes.stories.paused = None;
        app.panes.stories.player = None;
        if next < 0 {
            if pos > 0 {
                let len = all[pos - 1].1.len();
                app.panes.stories.viewer = Some((all[pos - 1].0.clone(), len - 1, Instant::now()));
            } else {
                app.panes.stories.viewer = Some((k.to_owned(), 0, Instant::now()));
            }
        } else if (next as usize) < stories.len() {
            app.panes.stories.viewer = Some((k.to_owned(), next as usize, Instant::now()));
        } else if pos + 1 < all.len() && all[pos + 1].0 != "me" {
            app.panes.stories.viewer = Some((all[pos + 1].0.clone(), 0, Instant::now()));
        } else {
            app.panes.stories.viewer = None;
        }
    }
    ui.ctx().request_repaint_after(Duration::from_millis(50));
    let _ = palette;
}

/// Desktop's `StoryDetailsModal`: when it was sent (and received), who
/// sent it, its file's size, and for ours, who it went to.
fn info_modal(app: &mut App, ui: &mut Ui, story: &Story) {
    if app.panes.stories.info_of != Some(story.id) {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let closed = crate::panes::modal(ui, "story-info", 380., |ui| {
        crate::panes::title(ui, "Details");
        let row = |ui: &mut Ui, label: &str, value: String| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).font(theme::semibold(13.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(value).font(theme::regular(13.)).color(palette.text_secondary));
                });
            });
        };
        let full = |at: i64| chrono::DateTime::from_timestamp_millis(at).map(|t| crate::format::localized(&t.with_timezone(&chrono::Local), "%a, %b %-d, %Y %-I:%M %p")).unwrap_or_default();
        row(ui, tr("Sent"), full(story.sent_at));
        if story.author.is_some() {
            row(ui, tr("Received"), full(story.received_at));
            row(ui, tr("From"), story.author_name.clone().unwrap_or_else(|| story.author.as_deref().map(|a| app.name(a)).unwrap_or_default()));
        }
        if let StoryContent::Media { attachment, .. } = &story.content {
            row(ui, "File size", crate::format::file_size(attachment.size));
        }
        if story.author.is_none() && !story.recipients.is_empty() {
            ui.add_space(10.);
            ui.label(RichText::new(format!("Sent to {}", story.recipients.len())).font(theme::semibold(13.)));
            egui::ScrollArea::vertical().max_height(240.).show(ui, |ui| {
                for r in &story.recipients {
                    let viewed = story.views.iter().any(|v| v.viewer == r.service_id);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(app.name(&r.service_id)).font(theme::regular(13.)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if viewed {
                                ui.label(RichText::new(tr("Viewed")).font(theme::regular(12.)).color(palette.text_secondary));
                            }
                        });
                    });
                }
            });
        }
    });
    if closed {
        app.panes.stories.info_of = None;
    }
}

/// A group story reply's menu: Delete for me, and (ours) for everyone.
fn reply_menu(app: &mut App, label: &egui::Response, story: i64, r: &langis_store::StoryReply) {
    let menu = egui::Id::unique(("story-reply-menu", story, r.sent_at));
    if label.secondary_clicked()
        && let Some(at) = label.interact_pointer_pos()
    {
        crate::menu::open_at(&label.ctx, menu, at);
    }
    let mut todo = None;
    crate::menu::show(&label.ctx, menu, |m| {
        if m.item(Some("trash"), tr("Delete for me")) {
            todo = Some(false);
        }
        if r.author.is_none() && m.item(Some("trash"), tr("Delete for everyone")) {
            todo = Some(true);
        }
    });
    if let Some(for_everyone) = todo {
        app.send(Command::DeleteStoryReply { story, author: r.author.clone(), sent_at: r.sent_at, for_everyone });
    }
}

/// Desktop's `StoryViewsNRepliesModal`: for ours, Views and Replies tabs
/// (Views only with view receipts on); for a group story, the replies with
/// a field to add one.
fn views_modal(app: &mut App, ui: &mut Ui, story: &Story) {
    if app.panes.stories.views_of != Some(story.id) {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let mine = story.author.is_none();
    let receipts = app.panes.stories.receipts;
    let views = mine && receipts;
    let replies = story.group.is_some() || !story.replies.is_empty();
    let tab = if !views { 1 } else if !replies { 0 } else { app.panes.stories.views_tab };
    let closed = modal(ui, "story-views", 420., |ui| {
        if views && replies {
            // The tabs, as Desktop's `Tabs` (underlined, centered).
            ui.horizontal(|ui| {
                let half = (ui.available_width() - 28.) / 2.;
                for (i, label) in [tr("Views"), tr("Replies")].iter().enumerate() {
                    let (r, hit) = ui.allocate_exact_size(Vec2::new(half, 32.), Sense::click());
                    let on = tab == i;
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *label, theme::semibold(14.), if on { palette.text } else { palette.text_secondary });
                    if on {
                        ui.painter().hline(r.x_range().shrink(half / 4.), r.bottom() - 1., Stroke::new(2., palette.text));
                    }
                    if hit.clicked() {
                        app.panes.stories.views_tab = i;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::X, 18., tr("Close")).clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("langis-modal-close"), true));
                    }
                });
            });
            ui.add_space(12.);
        } else {
            title(ui, if views { tr("Views") } else { tr("Replies") });
        }
        egui::ScrollArea::vertical().max_height(420.).min_scrolled_height(200.).auto_shrink([false, true]).show(ui, |ui| {
            if tab == 0 {
                if story.views.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| body(ui, &palette, tr("No views yet")));
                }
                for v in &story.views {
                    ui.horizontal(|ui| {
                        ui.set_min_height(44.);
                        let name = app.name(&v.viewer);
                        match app.conversation(&ConversationId::direct(&v.viewer)) {
                            Some(c) => widgets::avatar(ui, &widgets::AvatarSpec::of(c), 36.),
                            None => widgets::avatar(ui, &widgets::AvatarSpec::person(&name, v.viewer.bytes().fold(0u8, |a, b| a.wrapping_add(b))), 36.),
                        };
                        ui.add_space(4.);
                        ui.label(RichText::new(name).font(theme::regular(14.)));
                        if let Some(at) = v.at {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(crate::format::short_timestamp(at)).font(theme::regular(12.)).color(palette.text_secondary));
                            });
                        }
                    });
                }
            } else {
                if story.replies.is_empty() {
                    ui.add_space(24.);
                    ui.vertical_centered(|ui| body(ui, &palette, tr("No replies yet")));
                }
                for r in &story.replies {
                    reply_row(app, ui, &palette, story.id, r);
                }
            }
        });
        // Desktop's reply composer, for a group story's replies.
        if tab == 1 && story.group.is_some() && (story.allows_replies || mine) {
            ui.add_space(10.);
            ui.horizontal(|ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut app.panes.stories.reply).hint_text("Reply to Group").desired_width(ui.available_width() - 64.));
                let send = widgets::pill_button(ui, tr("Send"), true).clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send && !app.panes.stories.reply.trim().is_empty() {
                    let body = std::mem::take(&mut app.panes.stories.reply);
                    app.send(Command::ReplyToStory { id: story.id, body: body.trim().to_owned(), emoji: None });
                }
            });
        }
    });
    if closed {
        app.panes.stories.views_of = None;
    }
}

/// A reply in `StoryViewsNRepliesModal`: the avatar, the name in its color
/// and the time, then the text (or "Reacted to the story" with the emoji).
fn reply_row(app: &mut App, ui: &mut Ui, palette: &Palette, story: i64, r: &langis_store::StoryReply) {
    let row = ui.horizontal_top(|ui| {
        let (name, color) = match &r.author {
            None => (tr("You").to_owned(), palette.text),
            Some(a) => (r.author_name.clone().unwrap_or_else(|| app.name(a)), theme::name_color(a, palette.dark)),
        };
        match r.author.as_ref().and_then(|a| app.conversation(&ConversationId::direct(a))) {
            Some(c) => widgets::avatar(ui, &widgets::AvatarSpec::of(c), 28.),
            None if r.author.is_none() => widgets::avatar(ui, &widgets::AvatarSpec { photo: app.me.1.as_deref(), ..widgets::AvatarSpec::person(&app.me.0, 0) }, 28.),
            None => widgets::avatar(ui, &widgets::AvatarSpec::person(&name, 0), 28.),
        };
        ui.add_space(4.);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).font(theme::semibold(13.)).color(color));
                ui.label(RichText::new(crate::format::short_timestamp(r.sent_at)).font(theme::regular(12.)).color(palette.text_secondary));
            });
            if r.deleted {
                ui.label(RichText::new("This message was deleted.").font(theme::italic(14.)).color(palette.text_secondary));
            } else if let Some(emoji) = &r.emoji {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Reacted to the story")).font(theme::regular(14.)).color(palette.text_secondary));
                    let mut job = egui::text::LayoutJob::default();
                    crate::chat::append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::regular(20.), color: palette.text, ..Default::default() });
                    ui.label(job);
                });
            } else {
                ui.add(egui::Label::new(RichText::new(&r.body).font(theme::regular(14.))).wrap());
            }
        });
    });
    ui.add_space(8.);
    if !r.deleted {
        let hit = ui.interact(row.response.rect, ui.scope_id().with(("reply", r.sent_at)), Sense::click());
        reply_menu(app, &hit, story, r);
    }
}

/// Desktop's "Delete this story?" and the list's "Hide story?".
fn story_confirms(app: &mut App, ui: &mut Ui) {
    use crate::dialog::{Choice, Variant, confirm};
    if let Some(id) = app.panes.stories.confirm_delete {
        let ended = app.panes.stories.list.iter().find(|s| s.id == id).and_then(|s| s.group.as_ref()).and_then(|g| app.conversation(g)).is_some_and(|g| g.ended);
        let body = if ended { tr("Delete this story? It will only be deleted for you because the group has ended.") } else { tr("Delete this story? It will also be deleted for everyone who received it.") };
        match confirm(ui.ctx(), egui::Id::unique("delete-story"), "", body, tr("Delete"), Variant::StrongDestructive) {
            Some(Choice::Action) => {
                app.panes.stories.confirm_delete = None;
                app.send(Command::DeleteStory { id });
                if app.panes.stories.viewer.is_some() {
                    app.panes.stories.viewer = None;
                    app.panes.stories.player = None;
                }
            }
            Some(Choice::Cancel) => app.panes.stories.confirm_delete = None,
            None => {}
        }
    }
    if let Some((chat, name)) = app.panes.stories.confirm_hide_chat.clone() {
        let body = crate::i18n::tf("icu:StoryListItem__hide-modal--body", &format!("Hide story? New story updates from {name} won\u{2019}t appear at the top of the stories list anymore."), &[("name", crate::i18n::Arg::from(&name))]);
        match confirm(ui.ctx(), egui::Id::unique("hide-story-list"), "", &body, tr("Hide"), Variant::StrongPrimary) {
            Some(Choice::Action) => {
                app.panes.stories.confirm_hide_chat = None;
                app.send(Command::SetChat { conversation: chat, change: langis_core::ChatChange::HideStory(true) });
            }
            Some(Choice::Cancel) => app.panes.stories.confirm_hide_chat = None,
            None => {}
        }
    }
}

/// The composer and privacy dialogs.
pub fn overlays(app: &mut App, ui: &mut Ui) {
    story_confirms(app, ui);
    composer(app, ui);
    privacy(app, ui);
    list_editor(app, ui);
}

fn target_label(app: &App, t: &StoryTarget) -> String {
    match t {
        StoryTarget::MyStory => tr("My Story").into(),
        StoryTarget::List(id) => app.panes.stories.lists.iter().find(|l| &l.id == id).map(|l| l.name.clone()).unwrap_or_else(|| tr("Custom story").into()),
        StoryTarget::Group(id) => app.conversation(id).map(|c| c.title.clone()).unwrap_or_default(),
    }
}

/// Desktop's `TextStoryCreator` limit.
const TEXT_MAX: usize = 700;

/// Desktop's `StoryCreator`, over the whole window: the story large in the
/// middle, typed on directly, with the font, text style, background and
/// link buttons under it and Next, which opens `SendStoryModal`.
fn composer(app: &mut App, ui: &mut Ui) {
    let Some(mut c) = app.panes.stories.composer.take() else { return };
    // A text story's link (added, or the text's first) gets its preview.
    if c.media.is_none() {
        let link = c.link.clone().or_else(|| langis_core::link_preview::first_previewable(&c.text));
        if link != c.preview_for {
            c.preview = None;
            c.preview_for = link.clone();
            if let Some(url) = link {
                app.send(Command::FetchLinkPreview { url });
            }
        }
    }
    let screen = ui.ctx().content_rect();
    let mut close = false;
    let empty = c.media.is_none() && c.text.trim().is_empty() && c.link.is_none();
    egui::Area::new(egui::Id::unique("story-creator")).fixed_pos(screen.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        ui.set_min_size(screen.size());
        ui.set_max_size(screen.size());
        let full = screen;
        // The whole window is the creator's: clicks don't reach what's under.
        ui.interact(full, ui.scope_id().with("story-creator-bg"), Sense::click());
        ui.painter().rect_filled(full, 0, Color32::from_gray(18));
        let x = Rect::from_min_size(full.min + Vec2::splat(12.), Vec2::splat(40.));
        let x_hit = ui.interact(x, ui.scope_id().with("story-creator-x"), Sense::click());
        if x_hit.hovered() {
            ui.painter().circle_filled(x.center(), 20., Color32::from_white_alpha(30));
        }
        theme::paint_symbol(ui.painter(), "x", x.center(), 22., Color32::WHITE);
        let busy = c.sending || c.new_group.is_some() || c.link_field.is_some() || c.confirm_discard || c.editor.is_some();
        if x_hit.on_hover_text("Close").clicked() || (!busy && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            close = true;
        }
        let height = (full.height() - 120.).max(240.);
        let width = (height * 9. / 16.).min(full.width() - 120.);
        let rect = Rect::from_center_size(Pos2::new(full.center().x, full.top() + 24. + height / 2.), Vec2::new(width, height));
        match c.media.clone() {
            Some((bytes, content_type, name)) => {
                ui.painter().rect_filled(rect, CornerRadius::same(12), Color32::BLACK);
                if content_type.starts_with("image/") {
                    egui::Image::new(egui::ImageSource::Bytes { uri: format!("bytes://story-media/{name}").into(), bytes: bytes.to_vec().into() })
                        .corner_radius(CornerRadius::same(12))
                        .fit_to_exact_size(rect.size())
                        .paint_at(ui, rect);
                } else {
                    theme::paint_symbol(ui.painter(), "videocamera", rect.center() - Vec2::new(0., 16.), 32., Color32::WHITE);
                    ui.painter().text(rect.center() + Vec2::new(0., 18.), egui::Align2::CENTER_CENTER, name, theme::regular(14.), Color32::WHITE);
                }
                // The caption, along the bottom.
                let field = Rect::from_min_max(Pos2::new(rect.left() + 12., rect.bottom() - 52.), Pos2::new(rect.right() - 12., rect.bottom() - 12.));
                ui.painter().rect_filled(field, CornerRadius::same(20), Color32::from_black_alpha(140));
                ui.put(
                    field.shrink2(Vec2::new(14., 8.)),
                    egui::TextEdit::singleline(&mut c.text).frame(egui::Frame::NONE).hint_text(RichText::new("Add a caption").color(Color32::from_white_alpha(160))).text_color(Color32::WHITE).font(theme::regular(14.)).char_limit(TEXT_MAX),
                );
            }
            None => {
                // The story as it'll look, its text laid out (unseen) for
                // the pill and the card; the field types over it.
                let shown = if c.text.is_empty() { " ".to_owned() } else { c.text.clone() };
                let mut story = c.text_story(shown);
                let color = story.foreground.map(argb).unwrap_or(Color32::WHITE);
                story.foreground = Some(0);
                let painted = paint_text_story(ui, &story, rect, c.preview.as_ref());
                if let Some(text_rect) = painted.text {
                    let scale = rect.width() / 360.;
                    let font = story_font(c.font, text_size(&c.text, scale));
                    let wrap = rect.width() - 48. * scale;
                    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, _: f32| {
                        let mut job = egui::text::LayoutJob::default();
                        crate::chat::append_with_emoji(&mut job, text.as_str(), egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
                        job.wrap.max_width = wrap;
                        job.halign = egui::Align::Center;
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    let field = Rect::from_center_size(Pos2::new(rect.center().x, text_rect.center().y), Vec2::new(wrap, text_rect.height().max(font.size * 1.3)));
                    let id = egui::Id::unique("story-creator-text");
                    ui.put(
                        field,
                        egui::TextEdit::multiline(&mut c.text)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .margin(0)
                            .desired_rows(1)
                            .align(egui::Align2::CENTER_TOP)
                            .hint_text(RichText::new(tr("Add text")).font(font.clone()).color(color.gamma_multiply(0.6)))
                            .char_limit(TEXT_MAX)
                            .layouter(&mut layouter),
                    );
                    if !busy && !ui.memory(|m| m.has_focus(id)) && ui.memory(|m| m.focused().is_none()) {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                }
                // Desktop's counter, as the limit nears.
                let left = TEXT_MAX.saturating_sub(c.text.chars().count());
                if left <= 100 {
                    ui.painter().text(rect.right_bottom() + Vec2::new(-12., -12.), egui::Align2::RIGHT_BOTTOM, left.to_string(), theme::regular(13.), if left <= 20 { theme::rgb(0xff4a3a) } else { Color32::from_white_alpha(200) });
                }
                // The card's X removes the link.
                if let Some(card) = painted.card {
                    let x = Rect::from_center_size(card.right_top() + Vec2::new(-4., 4.), Vec2::splat(22.));
                    let hit = ui.interact(x, ui.scope_id().with("story-creator-unlink"), Sense::click());
                    ui.painter().circle_filled(x.center(), 11., Color32::from_gray(if hit.hovered() { 70 } else { 40 }));
                    theme::paint_symbol(ui.painter(), "x", x.center(), 14., Color32::WHITE);
                    if hit.on_hover_text("Remove link").clicked() {
                        c.link = None;
                        c.preview = None;
                        // A link in the text stays without its card.
                        c.preview_for = langis_core::link_preview::first_previewable(&c.text);
                    }
                }
            }
        }
        // The buttons under the story: Desktop's TextStoryCreator toolbar,
        // then Next.
        let bar = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() + 16.), Pos2::new(rect.right(), rect.bottom() + 56.));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            let round = |ui: &mut Ui, tip: &str, paint: &dyn Fn(&egui::Painter, Pos2)| {
                let (r, hit) = ui.allocate_exact_size(Vec2::splat(36.), Sense::click());
                ui.painter().circle_filled(r.center(), 18., Color32::from_white_alpha(if hit.hovered() { 45 } else { 25 }));
                paint(ui.painter(), r.center());
                hit.on_hover_text(tip)
            };
            if c.media.is_none() {
                // Font: "Aa" in the font itself; a click moves to the next.
                let font = c.font;
                if round(ui, "Font", &|p, at| {
                    p.text(at, egui::Align2::CENTER_CENTER, "Aa", story_font(font, 15.), Color32::WHITE);
                })
                .clicked()
                {
                    let i = FONTS.iter().position(|f| *f == c.font).unwrap_or(0);
                    c.font = FONTS[(i + 1) % FONTS.len()];
                }
                // Text style: white, black, on white, on black.
                let style = c.text_style % 4;
                if round(ui, "Text style", &|p, at| {
                    let (fg, bg) = [(Color32::WHITE, None), (Color32::BLACK, None), (Color32::BLACK, Some(Color32::WHITE)), (Color32::WHITE, Some(Color32::BLACK))][style];
                    if let Some(bg) = bg {
                        p.rect_filled(Rect::from_center_size(at, Vec2::new(22., 18.)), CornerRadius::same(4), bg);
                    } else if fg == Color32::BLACK {
                        p.circle_filled(at, 12., Color32::from_white_alpha(200));
                    }
                    p.text(at, egui::Align2::CENTER_CENTER, "A", theme::semibold(14.), fg);
                })
                .clicked()
                {
                    c.text_style = (c.text_style + 1) % 4;
                }
                // Background: its swatch; a click moves to the next.
                let background = c.story_background();
                if round(ui, "Background", &|p, at| {
                    let r = Rect::from_center_size(at, Vec2::splat(20.));
                    match &background {
                        StoryBackground::Color(col) => {
                            p.circle_filled(at, 10., argb(*col));
                        }
                        StoryBackground::Gradient { colors, .. } => {
                            p.circle_filled(at, 10., argb(colors[0]));
                            p.with_clip_rect(Rect::from_min_max(Pos2::new(r.left(), at.y), r.max)).circle_filled(at, 10., argb(*colors.last().unwrap_or(&colors[0])));
                        }
                    }
                    p.circle_stroke(at, 10., Stroke::new(2., Color32::WHITE));
                })
                .clicked()
                {
                    c.background = (c.background + 1) % (BACKGROUNDS.len() + GRADIENTS.len());
                }
                // Link: Desktop's popover to type or paste one.
                let link = round(ui, tr("Add link"), &|p, at| theme::paint_symbol(p, "link", at, 18., Color32::WHITE));
                if link.clicked() {
                    c.link_field = Some(c.link.clone().unwrap_or_default());
                }
            } else {
                // A photo opens in Desktop's MediaEditor (draw, text,
                // stickers, crop).
                if c.media.as_ref().is_some_and(|(_, t, _)| t.starts_with("image/") && t != "image/gif") && widgets::pill_button(ui, tr("Edit"), false).clicked() {
                    c.editor = c.media.as_ref().and_then(|(bytes, _, _)| crate::media_editor::Editor::open(0, bytes));
                }
                if widgets::pill_button(ui, "Use text instead", false).clicked() {
                    c.media = None;
                    c.text.clear();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = !empty;
                let next = ui.add_enabled(ready, egui::Button::new(RichText::new(tr("Next")).font(theme::medium(14.)).color(Color32::WHITE)).fill(theme::rgb(0x4655ff)).corner_radius(CornerRadius::same(18)).min_size(Vec2::new(80., 36.)));
                if next.clicked() {
                    c.sending = true;
                }
            });
        });
        // The link popover, above the bar.
        if let Some(mut field) = c.link_field.take() {
            let at = Pos2::new(bar.left(), bar.top() - 8.);
            let mut keep = true;
            egui::Area::new(egui::Id::unique("story-creator-link")).fixed_pos(at).pivot(egui::Align2::LEFT_BOTTOM).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(Color32::from_gray(46)).corner_radius(CornerRadius::same(8)).inner_margin(12).show(ui, |ui| {
                    ui.set_width(bar.width().max(280.) - 24.);
                    let r = ui.add(egui::TextEdit::singleline(&mut field).hint_text("Type or paste a URL").text_color(Color32::WHITE).desired_width(f32::INFINITY));
                    r.request_focus();
                    ui.add_space(8.);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let done = ui.add(egui::Button::new(RichText::new(tr("Done")).color(Color32::WHITE)).fill(theme::rgb(0x4655ff)).corner_radius(CornerRadius::same(14))).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if done {
                            let url = field.trim();
                            let url = if url.is_empty() || url.contains("://") { url.to_owned() } else { format!("https://{url}") };
                            c.link = (!url.is_empty() && !langis_core::link_preview::is_link_sneaky(&url)).then_some(url);
                            if c.link.is_none() && !field.trim().is_empty() {
                                app.toast("That link can't be previewed.");
                            }
                            keep = false;
                        }
                        if ui.button(tr("Cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            keep = false;
                        }
                    });
                });
            });
            if keep {
                c.link_field = Some(field);
            }
        }
    });
    if let Some(editor) = c.editor.as_mut() {
        if !app.fun.asked {
            app.fun.asked = true;
            app.send(Command::LoadStickers);
        }
        if let Some(result) = editor.show(ui.ctx(), &app.fun.packs) {
            c.editor = None;
            if let (Some(png), Some(media)) = (result, c.media.as_mut()) {
                let stem = std::path::Path::new(&media.2).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "story".into());
                *media = (std::sync::Arc::new(png), "image/png".into(), format!("{stem}.png"));
            }
        }
        app.panes.stories.composer = Some(c);
        return;
    }
    if close {
        if empty {
            return;
        }
        c.confirm_discard = true;
    }
    if c.confirm_discard {
        match crate::dialog::confirm(ui.ctx(), egui::Id::unique("discard-story"), "", "Are you sure you want to discard this story?", tr("Discard"), crate::dialog::Variant::StrongDestructive) {
            Some(crate::dialog::Choice::Action) => return,
            Some(crate::dialog::Choice::Cancel) => c.confirm_discard = false,
            None => {}
        }
    }
    if c.sending && send_modal(app, ui, &mut c) {
        return;
    }
    app.panes.stories.composer = Some(c);
}

/// Desktop's `SendStoryModal`: where the story goes (My Story, custom
/// stories, group stories, with New group story), then Send story. True
/// once it's sent.
fn send_modal(app: &mut App, ui: &mut Ui, c: &mut Composer) -> bool {
    let mut answer = None;
    let closed = modal(ui, "story-send", 380., |ui| {
        title(ui, tr("Send story"));
        // Desktop's header: Send to, and New (custom or group story).
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Send to")).font(theme::semibold(13.)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let new = widgets::axo_button(ui, tr("New"), widgets::Axo::SubtleSecondary);
                crate::menu::dropdown(ui, &new, egui::Id::unique("send-story-new"), |m| {
                    if m.item(Some("stories"), tr("New custom story")) {
                        app.panes.stories.editing = Some(StoryList { id: new_uuid(), name: String::new(), members: Vec::new(), block_list: false, allows_replies: true });
                    }
                    if m.item(Some("group"), tr("New group story")) {
                        c.new_group = Some((Vec::new(), String::new()));
                    }
                });
            });
        });
        ui.add_space(6.);
        // Groups offered: those added to stories (`Always`), and any with
        // stories up now (`IfActive`), not removed (`Never`).
        let modes = app.panes.stories.send_modes.clone();
        let with_stories: Vec<ConversationId> = app.panes.stories.list.iter().filter_map(|s| s.group.clone()).collect();
        let offered = |g: &langis_store::Conversation| match modes.iter().find(|(id, _)| *id == g.id).map(|(_, m)| *m) {
            Some(langis_store::StorySendMode::Always) => true,
            Some(langis_store::StorySendMode::Never) => false,
            _ => with_stories.contains(&g.id),
        };
        let mut options = vec![StoryTarget::MyStory];
        options.extend(app.panes.stories.lists.iter().filter(|l| !l.is_my_story()).map(|l| StoryTarget::List(l.id.clone())));
        let groups: Vec<ConversationId> = app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && !g.left && offered(g)).map(|g| g.id.clone()).collect();
        options.extend(groups.iter().cloned().map(StoryTarget::Group));
        let mut remove: Option<ConversationId> = None;
        let palette = Palette::of(ui.ctx());
        egui::ScrollArea::vertical().max_height(360.).show(ui, |ui| {
            for t in options {
                // Desktop's rows: avatar, name, what it reaches, the check,
                // and its menu.
                let (rect, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.), Sense::click());
                let avatar = Rect::from_min_size(Pos2::new(rect.left() + 4., rect.center().y - 18.), Vec2::splat(36.));
                let (subtitle, closed) = match &t {
                    StoryTarget::MyStory => {
                        let my = app.panes.stories.lists.iter().find(|l| l.is_my_story()).cloned().unwrap_or_else(StoryList::my_story);
                        let spec = widgets::AvatarSpec { photo: app.me.1.as_deref(), ..widgets::AvatarSpec::person(&app.me.0, 0) };
                        widgets::paint_avatar(ui, avatar, &spec);
                        let sub = if my.block_list && my.members.is_empty() {
                            tr("All Signal connections").to_owned()
                        } else if my.block_list {
                            crate::i18n::tf("icu:SendStoryModal__excluded", &format!("{} excluded", my.members.len()), &[("count", crate::i18n::Arg::from(my.members.len()))])
                        } else {
                            format!("{} \u{b7} {}", tr("Only share with"), my.members.len())
                        };
                        (sub, false)
                    }
                    StoryTarget::List(_) => {
                        ui.painter().circle_filled(avatar.center(), 18., palette.fill_secondary);
                        theme::paint_symbol(ui.painter(), "stories", avatar.center(), 18., palette.text);
                        (tr("Custom story").to_owned(), false)
                    }
                    StoryTarget::Group(g) => {
                        let conv = app.conversation(g).cloned();
                        if let Some(conv) = &conv {
                            widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(conv));
                        }
                        let members = conv.as_ref().map(|c| c.members.len() + 1).unwrap_or(0);
                        let closed = conv.as_ref().is_some_and(|c| c.group_state.announcements_only && !c.admins.contains(&app.aci));
                        (format!("{} \u{b7} {}", tr("Group story"), crate::i18n::tf("icu:ConversationDetailsHeader--members", &format!("{members} members"), &[("number", crate::i18n::Arg::from(members))])), closed)
                    }
                };
                let ink = if closed { palette.text_secondary } else { palette.text };
                ui.painter().text(Pos2::new(avatar.right() + 10., rect.center().y - 9.), egui::Align2::LEFT_CENTER, target_label(app, &t), theme::medium(14.), ink);
                ui.painter().text(Pos2::new(avatar.right() + 10., rect.center().y + 10.), egui::Align2::LEFT_CENTER, &subtitle, theme::regular(12.), palette.text_secondary);
                let on = c.targets.contains(&t);
                crate::panes::round_check(ui, &palette, Pos2::new(rect.right() - 16., rect.center().y), on);
                let hit = if closed { hit.on_hover_text(tr("Only admins can send stories to this group.")) } else { hit };
                if hit.clicked() && !closed {
                    if on {
                        c.targets.retain(|x| *x != t);
                    } else {
                        c.targets.push(t.clone());
                    }
                }
                // Its "…" menu.
                let more = Rect::from_center_size(Pos2::new(rect.right() - 48., rect.center().y), Vec2::splat(26.));
                let more_hit = ui.interact(more, ui.scope_id().with(("send-story-more", format!("{t:?}"))), Sense::click());
                theme::paint_symbol(ui.painter(), "more", more.center(), 16., palette.text_secondary);
                let menu = egui::Id::unique(("send-story-menu", format!("{t:?}")));
                if more_hit.clicked() {
                    crate::menu::open_at(ui.ctx(), menu, more.left_bottom());
                }
                crate::menu::show(ui.ctx(), menu, |m| match &t {
                    StoryTarget::MyStory => {
                        if m.item(Some("settings"), tr("My Story privacy")) {
                            app.panes.stories.privacy = true;
                        }
                    }
                    StoryTarget::List(id) => {
                        if let Some(l) = app.panes.stories.lists.iter().find(|l| &l.id == id).cloned() {
                            if m.item(Some("pencil"), tr("Edit viewers")) {
                                app.panes.stories.editing = Some(l.clone());
                            }
                            if m.item(Some("trash"), tr("Delete story")) {
                                let body = crate::i18n::tf("icu:StoriesSettings__delete-list--confirm", &format!("Are you sure you want to delete \u{201c}{}\u{201d}? Updates shared to this story will also be deleted.", l.name), &[("name", crate::i18n::Arg::from(&l.name))]);
                                crate::panes::ask(app, "", body, tr("Delete"), true, vec![Command::DeleteStoryList { id: l.id.clone() }]);
                            }
                        }
                    }
                    StoryTarget::Group(g) => {
                        if m.item(Some("minus-circle"), tr("Remove group story")) {
                            remove = Some(g.clone());
                        }
                    }
                });
            }
        });
        // Desktop asks before a group story goes from the list.
        if let Some(g) = remove {
            c.targets.retain(|t| *t != StoryTarget::Group(g.clone()));
            app.panes.stories.send_modes.retain(|(id, _)| *id != g);
            app.panes.stories.send_modes.push((g.clone(), langis_store::StorySendMode::Never));
            crate::panes::ask(app, "", tr("Remove story? This will remove the story from your list, but you will still be able to view stories from this group."), tr("Remove"), true, vec![Command::SetStorySendMode { groups: vec![g], mode: langis_store::StorySendMode::Never }]);
        }
        answer = buttons(ui, &[tr("Send story"), tr("Cancel")]);
    });
    // New group story: pick groups, added for stories from now on.
    if let Some((mut picked, mut query)) = c.new_group.take() {
        let mut done = None;
        let closed_picker = modal(ui, "new-group-story", 380., |ui| {
            title(ui, tr("New group story"));
            widgets::text_field(ui, &mut query, "Search groups", false);
            ui.add_space(6.);
            let q = query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                for g in app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && !g.left && (q.is_empty() || g.title.to_lowercase().contains(&q))) {
                    let mut on = picked.contains(&g.id);
                    if crate::widgets::checkbox(ui, &mut on, &g.title).changed() {
                        if on {
                            picked.push(g.id.clone());
                        } else {
                            picked.retain(|x| *x != g.id);
                        }
                    }
                }
            });
            done = buttons(ui, &[tr("Add"), tr("Cancel")]);
        });
        match done {
            Some(0) if !picked.is_empty() => {
                app.send(Command::SetStorySendMode { groups: picked.clone(), mode: langis_store::StorySendMode::Always });
                for g in picked {
                    app.panes.stories.send_modes.retain(|(id, _)| *id != g);
                    app.panes.stories.send_modes.push((g.clone(), langis_store::StorySendMode::Always));
                    c.targets.push(StoryTarget::Group(g));
                }
            }
            Some(_) => {}
            None if closed_picker => {}
            None => c.new_group = Some((picked, query)),
        }
    }
    match answer {
        Some(0) if c.targets.is_empty() => {
            app.toast("Choose where to send your story.");
            false
        }
        Some(0) => {
            let story = match c.media.take() {
                Some((bytes, content_type, _)) => OutgoingStory::Media { bytes, content_type, caption: c.text.trim().to_owned(), styles: Vec::new() },
                None => {
                    let text = c.text_story(c.text.trim().to_owned());
                    OutgoingStory::Text { story: Box::new(text), preview: c.preview.take() }
                }
            };
            app.send(Command::PostStory { story, targets: std::mem::take(&mut c.targets) });
            true
        }
        Some(_) => {
            c.sending = false;
            false
        }
        None if closed => {
            c.sending = false;
            false
        }
        None => false,
    }
}

/// Desktop's "Story privacy": My Story's audience, custom stories, view
/// receipts, and turning stories off.
/// A page of Desktop's StoriesSettingsModal.
#[derive(Clone, PartialEq)]
pub enum PrivacyPage {
    My,
    List(String),
    Group(ConversationId),
}

/// Desktop's StoriesSettingsModal: its description and every story (My
/// Story, custom stories, group stories), each opening its page; view
/// receipts; turning stories off.
fn privacy(app: &mut App, ui: &mut Ui) {
    if !app.panes.stories.privacy {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let lists = app.panes.stories.lists.clone();
    let my = lists.iter().find(|l| l.is_my_story()).cloned().unwrap_or_else(StoryList::my_story);
    let page = app.panes.stories.privacy_page.clone();
    let mut done = false;
    let mut go: Option<Option<PrivacyPage>> = None;
    let closed = modal(ui, "story-privacy", 440., |ui| {
        match &page {
            None => {
                title(ui, tr("Story privacy"));
                body(ui, &palette, tr("Stories automatically disappear after 24 hours. Choose who can view your story or create new stories with specific viewers or groups."));
                ui.add_space(12.);
                ui.label(RichText::new(tr("My Stories")).font(theme::semibold(14.)));
                ui.add_space(4.);
                if menu_row(ui, Some(Icon::PlusCircle), tr("New Story"), false).clicked() {
                    app.panes.stories.editing = Some(StoryList { id: new_uuid(), name: String::new(), members: Vec::new(), block_list: false, allows_replies: true });
                }
                let row = |ui: &mut Ui, title_text: &str, sub: &str, avatar: &dyn Fn(&Ui, Rect)| -> bool {
                    let (rect, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.), Sense::click());
                    if hit.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                    }
                    let a = Rect::from_min_size(Pos2::new(rect.left() + 6., rect.center().y - 18.), Vec2::splat(36.));
                    avatar(ui, a);
                    ui.painter().text(Pos2::new(a.right() + 10., rect.center().y - 9.), egui::Align2::LEFT_CENTER, title_text, theme::medium(14.), palette.text);
                    ui.painter().text(Pos2::new(a.right() + 10., rect.center().y + 10.), egui::Align2::LEFT_CENTER, sub, theme::regular(12.), palette.text_secondary);
                    hit.clicked()
                };
                egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                    let me = (app.me.0.clone(), app.me.1.clone());
                    let my_sub = if my.block_list && my.members.is_empty() { tr("All Signal connections").to_owned() } else { crate::i18n::tf("icu:StoriesSettings__viewers", &format!("{} viewers", my.members.len()), &[("count", crate::i18n::Arg::from(my.members.len()))]) };
                    if row(ui, tr("My Story"), &my_sub, &|ui, a| widgets::paint_avatar(ui, a, &widgets::AvatarSpec { photo: me.1.as_deref(), ..widgets::AvatarSpec::person(&me.0, 0) })) {
                        go = Some(Some(PrivacyPage::My));
                    }
                    for l in lists.iter().filter(|l| !l.is_my_story()) {
                        let sub = crate::i18n::tf("icu:StoriesSettings__viewers", &format!("{} viewers", l.members.len()), &[("count", crate::i18n::Arg::from(l.members.len()))]);
                        if row(ui, &l.name, &sub, &|ui, a| {
                            ui.painter().circle_filled(a.center(), 18., palette.fill_secondary);
                            theme::paint_symbol(ui.painter(), "stories", a.center(), 18., palette.text);
                        }) {
                            go = Some(Some(PrivacyPage::List(l.id.clone())));
                        }
                    }
                    let modes = app.panes.stories.send_modes.clone();
                    for (g, _) in modes.iter().filter(|(_, m)| *m == langis_store::StorySendMode::Always) {
                        if let Some(conv) = app.conversation(g).cloned()
                            && row(ui, &conv.title, tr("Group story"), &|ui, a| widgets::paint_avatar(ui, a, &widgets::AvatarSpec::of(&conv)))
                        {
                            go = Some(Some(PrivacyPage::Group(g.clone())));
                        }
                    }
                });
                ui.add_space(12.);
                let mut receipts = app.panes.stories.receipts;
                ui.horizontal(|ui| {
                    ui.label(tr("View Receipts"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::switch(ui, &mut receipts).changed() {
                            app.panes.stories.receipts = receipts;
                            app.send(Command::SetStoryViewReceipts(receipts));
                        }
                    });
                });
                body(ui, &palette, tr("See and share when stories are viewed. If disabled, you won't see when others view your story."));
                ui.add_space(8.);
                if menu_row(ui, None, tr("Turn off stories"), true).clicked() {
                    app.send(Command::SetStoriesDisabled(true));
                    app.panes.stories.disabled = true;
                    done = true;
                }
                done |= buttons(ui, &[tr("Done")]).is_some();
            }
            Some(PrivacyPage::My) => {
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, Icon::ChevronLeft, 18., tr("Back")).clicked() {
                        go = Some(None);
                    }
                    ui.label(RichText::new(tr("My Story")).font(theme::semibold(14.)));
                });
                ui.add_space(8.);
                let mode = if my.block_list && my.members.is_empty() {
                    0
                } else if my.block_list {
                    1
                } else {
                    2
                };
                let mut pick = mode;
                ui.radio_value(&mut pick, 0, tr("All Signal connections"));
                ui.radio_value(&mut pick, 1, format!("{} ({})", tr("All except..."), if my.block_list { my.members.len() } else { 0 }));
                ui.radio_value(&mut pick, 2, format!("{} ({})", tr("Only share with..."), if my.block_list { 0 } else { my.members.len() }));
                if pick != mode {
                    let mut list = my.clone();
                    list.block_list = pick != 2;
                    if pick == 0 || mode == 0 {
                        list.members.clear();
                    }
                    if pick == 0 {
                        app.send(Command::SaveStoryList(list));
                    } else {
                        app.panes.stories.editing = Some(list);
                    }
                }
                body(ui, &palette, tr("Choose who can view your story. Changes won't affect stories you've already sent."));
                ui.add_space(12.);
                replies_switch(app, ui, &palette, &my);
            }
            Some(PrivacyPage::List(id)) => {
                let Some(l) = lists.iter().find(|l| &l.id == id).cloned() else {
                    go = Some(None);
                    return;
                };
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, Icon::ChevronLeft, 18., tr("Back")).clicked() {
                        go = Some(None);
                    }
                    ui.label(RichText::new(&l.name).font(theme::semibold(14.)));
                });
                ui.add_space(8.);
                ui.label(RichText::new(tr("Who can view this story")).font(theme::semibold(13.)).color(palette.text_secondary));
                if menu_row(ui, Some(Icon::PlusCircle), tr("Add viewer"), false).clicked() {
                    app.panes.stories.editing = Some(l.clone());
                }
                egui::ScrollArea::vertical().max_height(240.).show(ui, |ui| {
                    for m in &l.members {
                        let name = app.name(m);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&name).font(theme::regular(14.)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::axo_button(ui, tr("Remove"), widgets::Axo::SubtleSecondary).clicked() {
                                    let mut without = l.clone();
                                    without.members.retain(|x| x != m);
                                    let title_text = crate::i18n::tf("icu:StoriesSettings__remove--title", &format!("Remove {name}"), &[("title", crate::i18n::Arg::from(&name))]);
                                    crate::panes::ask(app, title_text, tr("This person will no longer see your story."), tr("Remove"), true, vec![Command::SaveStoryList(without)]);
                                }
                            });
                        });
                    }
                });
                ui.add_space(12.);
                replies_switch(app, ui, &palette, &l);
                ui.add_space(8.);
                if menu_row(ui, None, tr("Delete custom story"), true).clicked() {
                    let body_text = crate::i18n::tf("icu:StoriesSettings__delete-list--confirm", &format!("Are you sure you want to delete \u{201c}{}\u{201d}? Updates shared to this story will also be deleted.", l.name), &[("name", crate::i18n::Arg::from(&l.name))]);
                    crate::panes::ask(app, "", body_text, tr("Delete"), true, vec![Command::DeleteStoryList { id: l.id.clone() }]);
                    go = Some(None);
                }
            }
            Some(PrivacyPage::Group(g)) => {
                let name = app.conversation(g).map(|c| c.title.clone()).unwrap_or_default();
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, Icon::ChevronLeft, 18., tr("Back")).clicked() {
                        go = Some(None);
                    }
                    ui.label(RichText::new(&name).font(theme::semibold(14.)));
                });
                ui.add_space(8.);
                body(ui, &palette, tr("Group story"));
                ui.add_space(8.);
                if menu_row(ui, None, tr("Remove group story"), true).clicked() {
                    let body_text = crate::i18n::tf("icu:StoriesSettings__remove_group--confirm", &format!("Are you sure you want to remove \u{201c}{name}\u{201d}?"), &[("groupTitle", crate::i18n::Arg::from(&name))]);
                    crate::panes::ask(app, "", body_text, tr("Remove"), true, vec![Command::SetStorySendMode { groups: vec![g.clone()], mode: langis_store::StorySendMode::Never }]);
                    app.panes.stories.send_modes.retain(|(id, _)| id != g);
                    go = Some(None);
                }
            }
        }
    });
    if let Some(p) = go {
        app.panes.stories.privacy_page = p;
    }
    if done || (closed && page.is_none()) {
        app.panes.stories.privacy = false;
        app.panes.stories.privacy_page = None;
    } else if closed {
        app.panes.stories.privacy_page = None;
    }
}

/// Desktop's Replies & reactions switch for a story list.
fn replies_switch(app: &mut App, ui: &mut Ui, palette: &Palette, l: &StoryList) {
    ui.label(RichText::new(tr("Replies & reactions")).font(theme::semibold(13.)).color(palette.text_secondary));
    let mut replies = l.allows_replies;
    ui.horizontal(|ui| {
        ui.label(tr("Allow replies & reactions"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::switch(ui, &mut replies).changed() {
                app.send(Command::SaveStoryList(StoryList { allows_replies: replies, ..l.clone() }));
            }
        });
    });
    body(ui, palette, tr("Let people who can view your story react and reply."));
}

/// Picking who a story list includes (or, for My Story's "All except",
/// leaves out), and a custom story's name.
fn list_editor(app: &mut App, ui: &mut Ui) {
    let Some(mut list) = app.panes.stories.editing.take() else { return };
    let palette = Palette::of(ui.ctx());
    let people = crate::panes::people(app);
    let mut answer = None;
    let closed = modal(ui, "story-list", 400., |ui| {
        title(ui, if list.is_my_story() { if list.block_list { tr("Hide story from") } else { "Share story with" } } else { tr("Custom story") });
        if !list.is_my_story() {
            widgets::text_field(ui, &mut list.name, tr("Story name (required)"), false);
            ui.add_space(6.);
        }
        // Desktop's Choose Viewers search.
        let search_id = egui::Id::unique("story-viewers-search");
        let mut q: String = ui.ctx().data(|d| d.get_temp(search_id)).unwrap_or_default();
        widgets::text_field(ui, &mut q, tr("Search"), false);
        ui.ctx().data_mut(|d| d.insert_temp(search_id, q.clone()));
        let q = q.trim().to_lowercase();
        body(ui, &palette, &format!("{} chosen", list.members.len()));
        egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
            for (sid, name) in people.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)) {
                let mut on = list.members.contains(sid);
                if crate::widgets::checkbox(ui, &mut on, name).changed() {
                    if on {
                        list.members.push(sid.clone());
                    } else {
                        list.members.retain(|m| m != sid);
                    }
                }
            }
        });
        answer = buttons(ui, &[tr("Save"), tr("Cancel")]);
    });
    match answer {
        Some(0) if list.is_my_story() || !list.name.trim().is_empty() => {
            list.name = list.name.trim().to_owned();
            app.send(Command::SaveStoryList(list));
        }
        Some(0) => {
            app.toast("Give the story a name.");
            app.panes.stories.editing = Some(list);
        }
        Some(_) => {}
        None if closed => {}
        None => app.panes.stories.editing = Some(list),
    }
}

/// A random UUID (v4) for a new story list.
fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    getrandom(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

fn getrandom(out: &mut [u8]) {
    use std::io::Read as _;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(out);
    }
}
