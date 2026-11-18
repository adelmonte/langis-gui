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

/// How long a photo or text story stays up before the next.
const STORY_TIME: Duration = Duration::from_secs(6);

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
}

pub struct Composer {
    pub text: String,
    pub background: usize,
    pub media: Option<(std::sync::Arc<Vec<u8>>, String, String)>,
    pub targets: Vec<StoryTarget>,
}

pub fn load(app: &App) {
    app.load(|db| async move {
        let now = chrono::Utc::now().timestamp_millis();
        let list = db.stories(now).await.ok()?;
        let lists: Vec<StoryList> =
            db.setting(langis_core::SETTING_STORY_LISTS).await.ok().flatten().and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        let disabled = langis_core::setting_on(db.setting(langis_core::SETTING_STORIES_DISABLED).await.ok().flatten().as_deref());
        let receipts = db.setting(langis_core::SETTING_STORY_VIEW_RECEIPTS).await.ok().flatten().as_deref() != Some("false");
        Some(Loaded::Apply(Box::new(move |app: &mut App| {
            let s = &mut app.panes.stories;
            s.list = list;
            s.lists = lists;
            s.disabled = disabled;
            s.receipts = receipts;
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
                if story_row(app, ui, &palette, "My Stories", &subtitle, None, mine.is_some(), false) {
                    match mine {
                        Some(_) => app.panes.stories.viewer = Some(("me".into(), 0, Instant::now())),
                        None => new_composer(app),
                    }
                }
                let (unread, read): (Vec<_>, Vec<_>) = all.iter().filter(|(k, _)| k != "me").partition(|(_, s)| s.iter().any(|s| !s.read));
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
                        let name = who(app, k, stories);
                        let last = stories.last().map(|s| s.sent_at).unwrap_or_default();
                        let chat = stories.first().and_then(|s| s.group.clone().or_else(|| s.author.as_deref().map(ConversationId::direct)));
                        let unread = stories.iter().any(|s| !s.read);
                        if story_row(app, ui, &palette, &name, &crate::format::short_timestamp(last), chat.as_ref(), true, unread) {
                            let first = stories.iter().position(|s| !s.read).unwrap_or(0);
                            app.panes.stories.viewer = Some((k.clone(), first, Instant::now()));
                            app.panes.stories.reply.clear();
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
    app.panes.stories.composer = Some(Composer { text: String::new(), background: 0, media: None, targets: vec![StoryTarget::MyStory] });
}

/// One row of the list: an avatar ringed when unread, a name and a time.
#[allow(clippy::too_many_arguments)]
fn story_row(app: &App, ui: &mut Ui, palette: &Palette, name: &str, subtitle: &str, chat: Option<&ConversationId>, _has: bool, unread: bool) -> bool {
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
    response.clicked()
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
    // The story, 9:16, centered.
    let height = (full.height() - 140.).max(200.);
    let width = (height * 9. / 16.).min(full.width() - 120.);
    let rect = Rect::from_center_size(Pos2::new(full.center().x, full.top() + 24. + height / 2.), Vec2::new(width, height));
    paint_story(ui, &story, rect);
    // Progress bars over the top.
    let elapsed = since.elapsed();
    let typing = !app.panes.stories.reply.is_empty();
    let seg = (rect.width() - 16. - 4. * (stories.len() as f32 - 1.)) / stories.len() as f32;
    for i in 0..stories.len() {
        let x = rect.left() + 8. + i as f32 * (seg + 4.);
        let bar = Rect::from_min_size(Pos2::new(x, rect.top() + 8.), Vec2::new(seg, 3.));
        ui.painter().rect_filled(bar, CornerRadius::same(2), Color32::from_white_alpha(80));
        let done = if i < index { 1. } else if i == index { (elapsed.as_secs_f32() / STORY_TIME.as_secs_f32()).min(1.) } else { 0. };
        ui.painter().rect_filled(Rect::from_min_size(bar.min, Vec2::new(seg * done, 3.)), CornerRadius::same(2), Color32::WHITE);
    }
    // Who and when.
    let name = who(app, k, stories);
    let author = if story.group.is_some() { story.author_name.clone().map(|a| format!("{a} · ")).unwrap_or_default() } else { String::new() };
    let header = format!("{author}{name}  {}", crate::format::short_timestamp(story.sent_at));
    ui.painter().text(rect.left_top() + Vec2::new(12., 20.), egui::Align2::LEFT_TOP, header, theme::medium(13.), Color32::WHITE);
    // Close, and the arrows either side.
    let close = Rect::from_min_size(Pos2::new(full.right() - 52., full.top() + 12.), Vec2::splat(40.));
    let close_hit = ui.interact(close, ui.scope_id().with("story-close"), Sense::click());
    Icon::X.paint(ui, close, 22., Color32::WHITE);
    let prev = Rect::from_center_size(Pos2::new(rect.left() - 36., rect.center().y), Vec2::splat(40.));
    let next = Rect::from_center_size(Pos2::new(rect.right() + 36., rect.center().y), Vec2::splat(40.));
    let prev_hit = ui.interact(prev, ui.scope_id().with("story-prev"), Sense::click());
    let next_hit = ui.interact(next, ui.scope_id().with("story-next"), Sense::click());
    for (r, hit, label) in [(prev, &prev_hit, "‹"), (next, &next_hit, "›")] {
        ui.painter().circle_filled(r.center(), 20., Color32::from_white_alpha(if hit.hovered() { 50 } else { 25 }));
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, theme::semibold(24.), Color32::WHITE);
    }
    let keys = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::Escape)));
    let mut go: Option<isize> = None;
    if prev_hit.clicked() || (keys.0 && !typing) {
        go = Some(-1);
    }
    if next_hit.clicked() || (keys.1 && !typing) || (elapsed >= STORY_TIME && !typing) {
        go = Some(1);
    }
    if close_hit.clicked() || keys.2 {
        app.panes.stories.viewer = None;
        return;
    }
    // Below: ours show views and delete; theirs take a reply.
    let below = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() + 12.), Pos2::new(rect.right(), full.bottom() - 12.));
    ui.scope_builder(egui::UiBuilder::new().max_rect(below), |ui| {
        ui.horizontal(|ui| {
            if mine {
                let views = story.views.len();
                let receipts = app.panes.stories.receipts;
                let text = if receipts { format!("{views} views · {} replies", story.replies.len()) } else { "Views off".to_owned() };
                ui.label(RichText::new(text).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE))).clicked() {
                        app.send(Command::DeleteStory { id: story.id });
                        app.panes.stories.viewer = None;
                    }
                });
            } else if story.allows_replies {
                for emoji in ["❤️", "👍", "😂"] {
                    let mut job = egui::text::LayoutJob::default();
                    crate::chat::append_with_emoji(&mut job, emoji, egui::TextFormat { font_id: theme::regular(20.), color: Color32::WHITE, ..Default::default() });
                    if ui.add(egui::Button::new(job).frame(false)).on_hover_text("React").clicked() {
                        app.send(Command::ReplyToStory { id: story.id, body: String::new(), emoji: Some(emoji.into()) });
                        app.toast("Reaction sent.");
                    }
                }
                let hint = if story.group.is_some() { "Reply to group" } else { "Reply" };
                let field = ui.add(egui::TextEdit::singleline(&mut app.panes.stories.reply).hint_text(hint).desired_width(ui.available_width() - 70.));
                let send = ui.button("Send").clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send && !app.panes.stories.reply.trim().is_empty() {
                    let body = std::mem::take(&mut app.panes.stories.reply);
                    app.send(Command::ReplyToStory { id: story.id, body: body.trim().to_owned(), emoji: None });
                    app.toast("Reply sent.");
                }
            }
        });
        // A group story's replies, under it.
        if story.group.is_some() && !story.replies.is_empty() {
            egui::ScrollArea::vertical().max_height(120.).show(ui, |ui| {
                for r in &story.replies {
                    let who = if r.author.is_none() { "You".to_owned() } else { r.author_name.clone().unwrap_or_default() };
                    let text = if r.deleted { "This message was deleted.".into() } else { r.emoji.clone().unwrap_or_else(|| r.body.clone()) };
                    ui.label(RichText::new(format!("{who}: {text}")).color(Color32::from_white_alpha(220)));
                }
            });
        }
    });
    if let Some(step) = go {
        let next = index as isize + step;
        app.panes.stories.reply.clear();
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
    let closed = modal(ui, "story-composer", 560., |ui| {
        title(ui, "New story");
        ui.horizontal(|ui| {
            // The preview, 9:16.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(200., 356.), Sense::hover());
            match &c.media {
                Some((_, _, name)) => {
                    ui.painter().rect_filled(rect, CornerRadius::same(12), Color32::BLACK);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, name, theme::regular(13.), Color32::WHITE);
                }
                None => {
                    let story = TextStory { text: if c.text.is_empty() { "Add text".into() } else { c.text.clone() }, background: StoryBackground::Color(BACKGROUNDS[c.background]), ..Default::default() };
                    paint_text_story(ui, &story, rect);
                }
            }
            ui.add_space(12.);
            ui.vertical(|ui| {
                if c.media.is_none() {
                    ui.add(egui::TextEdit::multiline(&mut c.text).hint_text("Add text").desired_rows(4).desired_width(300.));
                    ui.add_space(6.);
                    ui.horizontal_wrapped(|ui| {
                        for (i, bg) in BACKGROUNDS.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.), Sense::click());
                            ui.painter().circle_filled(r.center(), 12., argb(*bg));
                            if i == c.background {
                                ui.painter().circle_stroke(r.center(), 13., Stroke::new(2., palette.text));
                            }
                            if resp.clicked() {
                                c.background = i;
                            }
                        }
                    });
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
                    ui.add(egui::TextEdit::singleline(&mut c.text).hint_text("Add a caption").desired_width(300.));
                }
                ui.add_space(10.);
                ui.label(RichText::new("Send to").font(theme::semibold(13.)));
                let mut options = vec![StoryTarget::MyStory];
                options.extend(app.panes.stories.lists.iter().filter(|l| !l.is_my_story()).map(|l| StoryTarget::List(l.id.clone())));
                options.extend(app.conversations.iter().filter(|g| g.kind == langis_store::ConversationKind::Group && !g.left).map(|g| StoryTarget::Group(g.id.clone())));
                egui::ScrollArea::vertical().max_height(140.).show(ui, |ui| {
                    for t in options {
                        let mut on = c.targets.contains(&t);
                        if ui.checkbox(&mut on, target_label(app, &t)).changed() {
                            if on {
                                c.targets.push(t);
                            } else {
                                c.targets.retain(|x| *x != t);
                            }
                        }
                    }
                });
            });
        });
        answer = buttons(ui, &["Send story", "Cancel"]);
    });
    match answer {
        Some(0) => {
            if c.targets.is_empty() || (c.media.is_none() && c.text.trim().is_empty()) {
                app.toast("Add some text or a photo, and choose where to send it.");
                app.panes.stories.composer = Some(c);
                return;
            }
            let story = match c.media {
                Some((bytes, content_type, _)) => OutgoingStory::Media { bytes, content_type, caption: c.text.trim().to_owned(), styles: Vec::new() },
                None => {
                    let text = TextStory { text: c.text.trim().to_owned(), background: StoryBackground::Color(BACKGROUNDS[c.background]), ..Default::default() };
                    OutgoingStory::Text { story: Box::new(text), preview: None }
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
