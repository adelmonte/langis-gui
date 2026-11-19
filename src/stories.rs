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
        return "My Story".into();
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
                ui.label(RichText::new("Stories").font(theme::semibold(18.)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.);
                    let more = widgets::icon_button(ui, Icon::More, 20., "More actions");
                    crate::menu::dropdown(ui, &more, egui::Id::unique("stories-more"), |m| {
                        if m.item(Some("lock"), "Story privacy") {
                            app.panes.stories.privacy = true;
                        }
                    });
                    if !app.panes.stories.disabled && widgets::icon_button(ui, Icon::Plus, 20., "Add a story").clicked() {
                        new_composer(app);
                    }
                });
            });
            ui.add_space(8.);
            if app.panes.stories.disabled {
                ui.add_space(24.);
                ui.vertical_centered(|ui| {
                    body(ui, &palette, "Stories are turned off.");
                    if ui.link("Turn on stories").clicked() {
                        app.send(Command::SetStoriesDisabled(false));
                    }
                });
                return;
            }
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                // Ours first: "My Stories", or the prompt to add one.
                let mine = all.iter().find(|(k, _)| k == "me");
                let subtitle = match mine {
                    Some((_, s)) => crate::format::short_timestamp(s.last().map(|s| s.sent_at).unwrap_or_default()),
                    None => "Tap to add".into(),
                };
                if story_row(app, ui, &palette, "My Stories", &subtitle, None, mine.and_then(|(_, s)| s.last()), false).clicked() {
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
                for (heading, section) in [("", unread), ("Viewed", read)] {
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
                    ui.vertical_centered(|ui| body(ui, &palette, "No recent stories to show right now"));
                }
            });
        });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect();
        match app.panes.stories.viewer.clone() {
            Some((k, index, since)) => viewer(app, ui, &palette, &all, &k, index, since),
            None if app.panes.stories.my_stories => my_stories(app, ui, &palette, &all),
            None => {
                ui.painter().rect_filled(rect, 0, palette.background);
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("No story selected").font(theme::regular(14.)).color(palette.text_secondary));
                });
            }
        }
    });
}

fn new_composer(app: &mut App) {
    app.panes.stories.composer = Some(Composer {
        text: String::new(),
        background: 0,
        media: None,
        targets: vec![StoryTarget::MyStory],
        font: langis_store::StoryFont::Regular,
        text_style: 0,
        preview: None,
        preview_for: None,
        new_group: None,
    });
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
        if m.item(Some(if hidden { "eye" } else { "eye-slash" }), if hidden { "Show story" } else { "Hide story" }) {
            app.send(Command::SetChat { conversation: chat.clone(), change: langis_core::ChatChange::HideStory(!hidden) });
        }
        if m.item(Some("message"), "Go to chat") {
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
            let small = TextStory { text: String::new(), ..t.clone() };
            paint_text_story(ui, &small, rect);
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
    let mut delete = None;
    let mut view = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(full.shrink2(Vec2::new(32., 24.))), |ui| {
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, Icon::ChevronLeft, 20., "Back").clicked() {
                app.panes.stories.my_stories = false;
            }
            ui.label(RichText::new("My Stories").font(theme::semibold(18.)));
        });
        ui.add_space(12.);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, s) in mine.iter().enumerate().rev() {
                let (row, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.), Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(row, CornerRadius::same(12), palette.fill);
                }
                story_thumb(ui, s, Rect::from_min_size(row.min + Vec2::new(10., 6.), Vec2::new(40., 72.)));
                let views = if app.panes.stories.receipts { format!("{} views", s.views.len()) } else { "Views off".to_owned() };
                ui.painter().text(Pos2::new(row.left() + 64., row.top() + 22.), egui::Align2::LEFT_TOP, views, theme::semibold(14.), palette.text);
                ui.painter().text(Pos2::new(row.left() + 64., row.top() + 44.), egui::Align2::LEFT_TOP, crate::format::short_timestamp(s.sent_at), theme::regular(12.), palette.text_secondary);
                if r.clicked() {
                    view = Some(i);
                }
                let menu = egui::Id::unique(("my-story-menu", s.id));
                if r.secondary_clicked()
                    && let Some(at) = r.interact_pointer_pos()
                {
                    crate::menu::open_at(ui.ctx(), menu, at);
                }
                crate::menu::show(ui.ctx(), menu, |m| {
                    if let StoryContent::Media { attachment, .. } = &s.content
                        && let Some(path) = &attachment.local_path
                        && m.item(Some("download"), "Download")
                    {
                        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "story".into());
                        if let Some(to) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                            let _ = std::fs::copy(path, to);
                        }
                    }
                    if m.item(Some("trash"), "Delete") {
                        delete = Some(s.id);
                    }
                });
            }
        });
    });
    if let Some(i) = view {
        app.panes.stories.my_stories = false;
        app.panes.stories.viewer = Some(("me".into(), i, Instant::now()));
    }
    if let Some(id) = delete {
        app.send(Command::DeleteStory { id });
    }
}

/// Paints a story's content into `rect`.
fn paint_story(ui: &Ui, s: &Story, rect: Rect) {
    match &s.content {
        StoryContent::Text(t) => paint_text_story(ui, t, rect),
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
            if !caption.is_empty() {
                let galley = ui.painter().layout(caption.clone(), theme::regular(14.), Color32::WHITE, rect.width() - 32.);
                let at = Pos2::new(rect.left() + 16., rect.bottom() - galley.size().y - 16.);
                ui.painter().rect_filled(Rect::from_min_size(at - Vec2::splat(6.), galley.size() + Vec2::splat(12.)), CornerRadius::same(8), Color32::from_black_alpha(120));
                ui.painter().galley(at, galley, Color32::WHITE);
            }
        }
    }
}

fn paint_text_story(ui: &Ui, t: &TextStory, rect: Rect) {
    match &t.background {
        StoryBackground::Color(c) => {
            ui.painter().rect_filled(rect, CornerRadius::same(12), argb(*c));
        }
        StoryBackground::Gradient { colors, .. } => {
            // A top to bottom blend of the first and last colors.
            let (top, bottom) = (argb(*colors.first().unwrap_or(&0xff000000)), argb(*colors.last().unwrap_or(&0xff000000)));
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(rect.left_top(), top);
            mesh.colored_vertex(rect.right_top(), top);
            mesh.colored_vertex(rect.right_bottom(), bottom);
            mesh.colored_vertex(rect.left_bottom(), bottom);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(egui::Shape::mesh(mesh));
        }
    }
    let color = t.foreground.map(argb).unwrap_or(Color32::WHITE);
    let size = if t.text.chars().count() < 50 { 28. } else { 20. };
    let font = match t.font {
        langis_store::StoryFont::Bold | langis_store::StoryFont::Condensed => theme::semibold(size),
        langis_store::StoryFont::Script | langis_store::StoryFont::Serif => theme::italic(size),
        _ => theme::medium(size),
    };
    let mut job = egui::text::LayoutJob::default();
    crate::chat::append_with_emoji(&mut job, &t.text, egui::TextFormat { font_id: font, color, ..Default::default() });
    job.wrap.max_width = rect.width() - 48.;
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let pos = Pos2::new(rect.center().x, rect.center().y - galley.size().y / 2.);
    if let Some(bg) = t.text_background {
        let r = Rect::from_center_size(rect.center(), galley.size() + Vec2::new(24., 16.));
        ui.painter().rect_filled(r, CornerRadius::same(10), argb(bg));
    }
    ui.painter().galley(pos, galley, color);
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
    paint_story(ui, &story, rect);
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
    let held = replying || app.panes.stories.picker.is_some() || app.panes.stories.views_of.is_some();
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
    // Who and when, then pause and (for a video) mute.
    let name = who(app, k, stories);
    let author = if story.group.is_some() { story.author_name.clone().map(|a| format!("{a} · ")).unwrap_or_default() } else { String::new() };
    let header = format!("{author}{name}  {}", crate::format::short_timestamp(story.sent_at));
    ui.painter().text(rect.left_top() + Vec2::new(12., 20.), egui::Align2::LEFT_TOP, header, theme::medium(13.), Color32::WHITE);
    let pause = Rect::from_center_size(Pos2::new(rect.right() - 22., rect.top() + 30.), Vec2::splat(28.));
    let pause_hit = ui.interact(pause, ui.scope_id().with("story-pause"), Sense::click());
    theme::paint_symbol(ui.painter(), if app.panes.stories.paused.is_some() { "play-fill" } else { "pause-fill" }, pause.center(), 18., Color32::WHITE);
    if pause_hit.on_hover_text(if app.panes.stories.paused.is_some() { "Play" } else { "Pause" }).clicked() {
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
        if r.on_hover_text(if player.muted { "Unmute" } else { "Mute" }).clicked() {
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
    if close_hit.clicked() || (keys.2 && app.panes.stories.picker.is_none() && app.panes.stories.views_of.is_none()) {
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
                    app.panes.stories.views_of = Some(story.id);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE))).clicked() {
                        app.send(Command::DeleteStory { id: story.id });
                        app.panes.stories.viewer = None;
                        app.panes.stories.player = None;
                    }
                });
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
                let hint = if story.group.is_some() { "Reply to group" } else { "Reply" };
                let field = ui.add(egui::TextEdit::singleline(&mut app.panes.stories.reply).id(egui::Id::unique("story-reply-field")).hint_text(hint).desired_width(ui.available_width() - 70.));
                let send = ui.button("Send").clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send && !app.panes.stories.reply.trim().is_empty() {
                    let body = std::mem::take(&mut app.panes.stories.reply);
                    app.send(Command::ReplyToStory { id: story.id, body: body.trim().to_owned(), emoji: None });
                    app.toast("Reply sent.");
                }
            }
        });
        // A group story's replies, under it; ours or theirs can go.
        if story.group.is_some() && !story.replies.is_empty() {
            egui::ScrollArea::vertical().max_height(120.).show(ui, |ui| {
                for r in &story.replies {
                    let who = if r.author.is_none() { "You".to_owned() } else { r.author_name.clone().unwrap_or_default() };
                    let text = if r.deleted { "This message was deleted.".into() } else { r.emoji.clone().unwrap_or_else(|| r.body.clone()) };
                    let label = ui.add(egui::Label::new(RichText::new(format!("{who}: {text}")).color(Color32::from_white_alpha(220))).sense(Sense::click()));
                    if !r.deleted {
                        reply_menu(app, &label, story.id, r);
                    }
                }
            });
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
        if m.item(Some("trash"), "Delete for me") {
            todo = Some(false);
        }
        if r.author.is_none() && m.item(Some("trash"), "Delete for everyone") {
            todo = Some(true);
        }
    });
    if let Some(for_everyone) = todo {
        app.send(Command::DeleteStoryReply { story, author: r.author.clone(), sent_at: r.sent_at, for_everyone });
    }
}

/// Desktop's `StoryViewsNRepliesModal`: who viewed our story (when view
/// receipts are on) and the replies to it.
fn views_modal(app: &mut App, ui: &mut Ui, story: &Story) {
    if app.panes.stories.views_of != Some(story.id) {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let mut done = false;
    let receipts = app.panes.stories.receipts;
    let closed = modal(ui, "story-views", 400., |ui| {
        title(ui, if receipts { "Views and replies" } else { "Replies" });
        egui::ScrollArea::vertical().max_height(420.).show(ui, |ui| {
            if receipts {
                ui.label(RichText::new(format!("Views ({})", story.views.len())).font(theme::semibold(14.)));
                if story.views.is_empty() {
                    body(ui, &palette, "No views yet");
                }
                for v in &story.views {
                    ui.horizontal(|ui| {
                        let name = app.name(&v.viewer);
                        let color = v.viewer.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                        widgets::avatar(ui, &widgets::AvatarSpec::person(&name, color), 28.);
                        ui.label(RichText::new(name).font(theme::regular(14.)));
                        if let Some(at) = v.at {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(crate::format::short_timestamp(at)).font(theme::regular(12.)).color(palette.text_secondary));
                            });
                        }
                    });
                }
                ui.add_space(12.);
            } else {
                body(ui, &palette, "View receipts are off. Turn them on in Story privacy to see who viewed your stories.");
                ui.add_space(8.);
            }
            ui.label(RichText::new(format!("Replies ({})", story.replies.len())).font(theme::semibold(14.)));
            for r in &story.replies {
                let who = if r.author.is_none() { "You".to_owned() } else { r.author_name.clone().unwrap_or_default() };
                let text = if r.deleted { "This message was deleted.".into() } else { r.emoji.clone().unwrap_or_else(|| r.body.clone()) };
                let label = ui.add(egui::Label::new(RichText::new(format!("{who}: {text}")).font(theme::regular(14.))).sense(Sense::click()));
                if !r.deleted {
                    reply_menu(app, &label, story.id, r);
                }
            }
        });
        if buttons(ui, &["Done"]).is_some() {
            done = true;
        }
    });
    if done || closed {
        app.panes.stories.views_of = None;
    }
}

/// The composer and privacy dialogs.
pub fn overlays(app: &mut App, ui: &mut Ui) {
    composer(app, ui);
    privacy(app, ui);
    list_editor(app, ui);
}

fn target_label(app: &App, t: &StoryTarget) -> String {
    match t {
        StoryTarget::MyStory => "My Story".into(),
        StoryTarget::List(id) => app.panes.stories.lists.iter().find(|l| &l.id == id).map(|l| l.name.clone()).unwrap_or_else(|| "Custom story".into()),
        StoryTarget::Group(id) => app.conversation(id).map(|c| c.title.clone()).unwrap_or_default(),
    }
}

/// Desktop's `TextStoryCreator` and media story sending, with where to send.
fn composer(app: &mut App, ui: &mut Ui) {
    let Some(mut c) = app.panes.stories.composer.take() else { return };
    let palette = Palette::of(ui.ctx());
    let mut answer = None;
    // A text story's first link gets its preview, as Desktop's does.
    if c.media.is_none() {
        let link = langis_core::link_preview::first_previewable(&c.text);
        if link != c.preview_for {
            c.preview = None;
            c.preview_for = link.clone();
            if let Some(url) = link {
                app.send(Command::FetchLinkPreview { url });
            }
        }
    }
    let closed = modal(ui, "story-composer", 600., |ui| {
        title(ui, "New story");
        ui.horizontal(|ui| {
            // The preview, 9:16.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(200., 356.), Sense::hover());
            match &c.media {
                Some((bytes, content_type, name)) => {
                    ui.painter().rect_filled(rect, CornerRadius::same(12), Color32::BLACK);
                    if content_type.starts_with("image/") {
                        egui::Image::new(egui::ImageSource::Bytes { uri: format!("bytes://story-media/{name}").into(), bytes: bytes.to_vec().into() })
                            .corner_radius(CornerRadius::same(12))
                            .fit_to_exact_size(rect.size())
                            .paint_at(ui, rect);
                    } else {
                        theme::paint_symbol(ui.painter(), "videocamera", rect.center() - Vec2::new(0., 14.), 28., Color32::WHITE);
                        ui.painter().text(rect.center() + Vec2::new(0., 16.), egui::Align2::CENTER_CENTER, name, theme::regular(12.), Color32::WHITE);
                    }
                    if !c.text.is_empty() {
                        let g = ui.painter().layout(c.text.clone(), theme::regular(12.), Color32::WHITE, rect.width() - 20.);
                        let at = Pos2::new(rect.left() + 10., rect.bottom() - g.size().y - 10.);
                        ui.painter().rect_filled(Rect::from_min_size(at - Vec2::splat(4.), g.size() + Vec2::splat(8.)), CornerRadius::same(6), Color32::from_black_alpha(120));
                        ui.painter().galley(at, g, Color32::WHITE);
                    }
                }
                None => {
                    let story = c.text_story(if c.text.is_empty() { "Add text".into() } else { c.text.clone() });
                    paint_text_story(ui, &story, rect);
                    if let Some(p) = &c.preview {
                        // The link's card at the bottom (Desktop's TextAttachment).
                        let card = Rect::from_min_size(Pos2::new(rect.left() + 10., rect.bottom() - 62.), Vec2::new(rect.width() - 20., 52.));
                        ui.painter().rect_filled(card, CornerRadius::same(8), Color32::from_white_alpha(230));
                        let mut job = egui::text::LayoutJob::simple_singleline(p.title.clone(), theme::semibold(11.), Color32::BLACK);
                        job.wrap = egui::text::TextWrapping::truncate_at_width(card.width() - 16.);
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        ui.painter().galley(card.min + Vec2::new(8., 8.), g, Color32::BLACK);
                        let domain = p.url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(&p.url).to_owned();
                        ui.painter().text(card.min + Vec2::new(8., 28.), egui::Align2::LEFT_TOP, domain, theme::regular(10.), Color32::from_gray(90));
                    }
                }
            }
            ui.add_space(12.);
            ui.vertical(|ui| {
                if c.media.is_none() {
                    ui.add(egui::TextEdit::multiline(&mut c.text).hint_text("Add text").desired_rows(4).desired_width(340.));
                    ui.add_space(6.);
                    // The font and text style buttons.
                    ui.horizontal(|ui| {
                        let font_name = match c.font {
                            langis_store::StoryFont::Bold => "Bold",
                            langis_store::StoryFont::Serif => "Serif",
                            langis_store::StoryFont::Script => "Script",
                            langis_store::StoryFont::Condensed => "Condensed",
                            _ => "Regular",
                        };
                        if widgets::pill_button(ui, &format!("Font: {font_name}"), false).clicked() {
                            let i = FONTS.iter().position(|f| *f == c.font).unwrap_or(0);
                            c.font = FONTS[(i + 1) % FONTS.len()];
                        }
                        let style = ["White text", "Black text", "On white", "On black"][c.text_style % 4];
                        if widgets::pill_button(ui, style, false).clicked() {
                            c.text_style = (c.text_style + 1) % 4;
                        }
                    });
                    ui.add_space(6.);
                    // Solid colors, then gradients.
                    ui.horizontal_wrapped(|ui| {
                        for i in 0..BACKGROUNDS.len() + GRADIENTS.len() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.), Sense::click());
                            if i < BACKGROUNDS.len() {
                                ui.painter().circle_filled(r.center(), 12., argb(BACKGROUNDS[i]));
                            } else {
                                let (a, b) = GRADIENTS[i - BACKGROUNDS.len()];
                                ui.painter().circle_filled(r.center(), 12., argb(a));
                                let half = Rect::from_min_max(Pos2::new(r.left(), r.center().y), r.max);
                                ui.painter().with_clip_rect(half).circle_filled(r.center(), 12., argb(b));
                            }
                            if i == c.background {
                                ui.painter().circle_stroke(r.center(), 13., Stroke::new(2., palette.text));
                            }
                            if resp.clicked() {
                                c.background = i;
                            }
                        }
                    });
                    if c.preview.is_some() && ui.link("Remove link preview").clicked() {
                        c.preview = None;
                    }
                }
                ui.add_space(6.);
                if ui.link(if c.media.is_some() { "Use text instead" } else { "Choose a photo or video" }).clicked() {
                    if c.media.is_some() {
                        c.media = None;
                    } else if let Some(path) = rfd::FileDialog::new().add_filter("Media", &["png", "jpg", "jpeg", "webp", "gif", "mp4", "mov"]).pick_file()
                        && let Ok(bytes) = std::fs::read(&path)
                    {
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
                        c.media = Some((std::sync::Arc::new(bytes), content_type.into(), name));
                    }
                }
                if c.media.is_some() {
                    ui.add(egui::TextEdit::singleline(&mut c.text).hint_text("Add a caption").desired_width(340.));
                }
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Send to").font(theme::semibold(13.)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Desktop's New group story.
                        if ui.link("New group story").clicked() {
                            c.new_group = Some((Vec::new(), String::new()));
                        }
                    });
                });
                // Groups offered: those added to stories (`Always`), and any
                // with stories up now (`IfActive`), not removed (`Never`).
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
                egui::ScrollArea::vertical().max_height(150.).show(ui, |ui| {
                    for t in options {
                        ui.horizontal(|ui| {
                            let mut on = c.targets.contains(&t);
                            if ui.checkbox(&mut on, target_label(app, &t)).changed() {
                                if on {
                                    c.targets.push(t.clone());
                                } else {
                                    c.targets.retain(|x| *x != t);
                                }
                            }
                            // Desktop's Remove group story.
                            if let StoryTarget::Group(g) = &t
                                && ui.small_button("Remove").on_hover_text("Remove this group story").clicked()
                            {
                                remove = Some(g.clone());
                            }
                        });
                    }
                });
                if let Some(g) = remove {
                    app.send(Command::SetStorySendMode { groups: vec![g.clone()], mode: langis_store::StorySendMode::Never });
                    c.targets.retain(|t| *t != StoryTarget::Group(g.clone()));
                    app.panes.stories.send_modes.retain(|(id, _)| *id != g);
                    app.panes.stories.send_modes.push((g, langis_store::StorySendMode::Never));
                }
            });
        });
        answer = buttons(ui, &["Send story", "Cancel"]);
    });
    // New group story: pick groups, added for stories from now on.
    if let Some((mut picked, mut query)) = c.new_group.take() {
        let mut done = None;
        let closed_picker = modal(ui, "new-group-story", 380., |ui| {
            title(ui, "New group story");
            widgets::text_field(ui, &mut query, "Search groups", false);
            ui.add_space(6.);
            let q = query.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                for g in app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && !g.left && (q.is_empty() || g.title.to_lowercase().contains(&q))) {
                    let mut on = picked.contains(&g.id);
                    if ui.checkbox(&mut on, &g.title).changed() {
                        if on {
                            picked.push(g.id.clone());
                        } else {
                            picked.retain(|x| *x != g.id);
                        }
                    }
                }
            });
            done = buttons(ui, &["Add", "Cancel"]);
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
        Some(0) => {
            if c.targets.is_empty() || (c.media.is_none() && c.text.trim().is_empty()) {
                app.toast("Add some text or a photo, and choose where to send it.");
                app.panes.stories.composer = Some(c);
                return;
            }
            let story = match c.media.take() {
                Some((bytes, content_type, _)) => OutgoingStory::Media { bytes, content_type, caption: c.text.trim().to_owned(), styles: Vec::new() },
                None => {
                    let text = c.text_story(c.text.trim().to_owned());
                    OutgoingStory::Text { story: Box::new(text), preview: c.preview.take() }
                }
            };
            app.send(Command::PostStory { story, targets: c.targets });
        }
        Some(_) => {}
        None if closed => {}
        None => app.panes.stories.composer = Some(c),
    }
}

/// Desktop's "Story privacy": My Story's audience, custom stories, view
/// receipts, and turning stories off.
fn privacy(app: &mut App, ui: &mut Ui) {
    if !app.panes.stories.privacy {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let lists = app.panes.stories.lists.clone();
    let my = lists.iter().find(|l| l.is_my_story()).cloned().unwrap_or_else(StoryList::my_story);
    let mut done = false;
    let closed = modal(ui, "story-privacy", 440., |ui| {
        title(ui, "Story privacy");
        ui.label(RichText::new("My Story").font(theme::semibold(14.)));
        let mode = if my.block_list && my.members.is_empty() {
            0
        } else if my.block_list {
            1
        } else {
            2
        };
        let mut pick = mode;
        ui.radio_value(&mut pick, 0, "All Signal connections");
        ui.radio_value(&mut pick, 1, format!("All except… ({})", if my.block_list { my.members.len() } else { 0 }));
        ui.radio_value(&mut pick, 2, format!("Only share with… ({})", if my.block_list { 0 } else { my.members.len() }));
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
        } else if mode != 0 && ui.link("Choose people").clicked() {
            app.panes.stories.editing = Some(my.clone());
        }
        let mut replies = my.allows_replies;
        if ui.checkbox(&mut replies, "Allow replies and reactions").changed() {
            app.send(Command::SaveStoryList(StoryList { allows_replies: replies, ..my.clone() }));
        }
        ui.add_space(12.);
        ui.label(RichText::new("Custom stories").font(theme::semibold(14.)));
        for l in lists.iter().filter(|l| !l.is_my_story()) {
            ui.horizontal(|ui| {
                ui.label(format!("{} ({})", l.name, l.members.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Delete").clicked() {
                        app.send(Command::DeleteStoryList { id: l.id.clone() });
                    }
                    if ui.small_button("Edit").clicked() {
                        app.panes.stories.editing = Some(l.clone());
                    }
                });
            });
        }
        if menu_row(ui, Some(Icon::PlusCircle), "New custom story", false).clicked() {
            app.panes.stories.editing = Some(StoryList { id: new_uuid(), name: String::new(), members: Vec::new(), block_list: false, allows_replies: true });
        }
        ui.add_space(12.);
        let mut receipts = app.panes.stories.receipts;
        ui.horizontal(|ui| {
            ui.label("View receipts");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::switch(ui, &mut receipts).changed() {
                    app.panes.stories.receipts = receipts;
                    app.send(Command::SetStoryViewReceipts(receipts));
                }
            });
        });
        body(ui, &palette, "See and share when stories are viewed. If disabled, you won't see when others view your story.");
        ui.add_space(8.);
        if menu_row(ui, None, "Turn off stories", true).clicked() {
            app.send(Command::SetStoriesDisabled(true));
            app.panes.stories.disabled = true;
            done = true;
        }
        done |= buttons(ui, &["Done"]).is_some();
    });
    if done || closed {
        app.panes.stories.privacy = false;
    }
}

/// Picking who a story list includes (or, for My Story's "All except",
/// leaves out), and a custom story's name.
fn list_editor(app: &mut App, ui: &mut Ui) {
    let Some(mut list) = app.panes.stories.editing.take() else { return };
    let palette = Palette::of(ui.ctx());
    let people = crate::panes::people(app);
    let mut answer = None;
    let closed = modal(ui, "story-list", 400., |ui| {
        title(ui, if list.is_my_story() { if list.block_list { "Hide story from" } else { "Share story with" } } else { "Custom story" });
        if !list.is_my_story() {
            widgets::text_field(ui, &mut list.name, "Story name (required)", false);
            ui.add_space(6.);
        }
        body(ui, &palette, &format!("{} chosen", list.members.len()));
        egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
            for (sid, name) in &people {
                let mut on = list.members.contains(sid);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        list.members.push(sid.clone());
                    } else {
                        list.members.retain(|m| m != sid);
                    }
                }
            }
        });
        answer = buttons(ui, &["Save", "Cancel"]);
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
