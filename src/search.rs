//! Message search: the chat list's "Messages" results under its chats
//! (Desktop's `LeftPaneSearchHelper`), and search within the open chat
//! from its header (`ConversationHeader`'s search, with up and down
//! through the matches).

use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_store::{ConversationId, SearchHit};

use crate::app::{App, Loaded};
use crate::chat::ChatView;
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

/// How long typing rests before searching.
const DELAY: Duration = Duration::from_millis(300);
/// Most results listed.
const LIMIT: u32 = 100;

/// The chat list's message search.
#[derive(Default)]
pub struct Global {
    /// The query searched (trimmed), and when the text last changed.
    searched: Option<String>,
    typed: Option<(String, Instant)>,
    pub hits: Vec<SearchHit>,
}

/// Search in the open chat.
#[derive(Default)]
pub struct Find {
    pub query: String,
    typed: Option<Instant>,
    searched: Option<String>,
    /// Newest first.
    pub hits: Vec<SearchHit>,
    /// The match shown.
    pub index: usize,
    focus: bool,
    /// Desktop lists the chat's results (`searchInConversation`): shown
    /// under the bar until one is picked.
    list: bool,
    /// In a group, only what this member sent (Desktop's "from" filter).
    from: Option<String>,
}

impl Find {
    pub fn open() -> Self {
        Self { focus: true, list: true, ..Default::default() }
    }

    /// The hits shown: all, or only one member's.
    fn shown(&self) -> Vec<usize> {
        (0..self.hits.len()).filter(|&i| self.from.as_ref().is_none_or(|f| self.hits[i].author.as_deref() == Some(f.as_str()))).collect()
    }
}

/// Results for a query, to whichever search asked.
pub enum Found {
    Global { query: String, hits: Vec<SearchHit> },
    Chat { conversation: ConversationId, query: String, hits: Vec<SearchHit> },
}

impl App {
    pub fn on_found(&mut self, found: Found) {
        match found {
            Found::Global { query, hits } => {
                if self.message_search.searched.as_deref() == Some(query.as_str()) {
                    self.message_search.hits = hits;
                }
            }
            Found::Chat { conversation, query, hits } => {
                let Some(chat) = self.chat.as_mut().filter(|c| c.id == conversation) else { return };
                let Some(find) = chat.find.as_mut().filter(|f| f.searched.as_deref() == Some(query.as_str())) else { return };
                find.hits = hits;
                find.index = 0;
                if let Some(hit) = find.hits.first() {
                    let id = hit.message_id;
                    self.open_at(conversation, id);
                }
            }
        }
    }
}

/// The search as typed: searches once it rests.
pub fn follow_global(app: &mut App, ctx: &egui::Context) {
    let query = app.search.trim().to_owned();
    if query.is_empty() {
        app.message_search = Global::default();
        return;
    }
    match &app.message_search.typed {
        Some((typed, _)) if *typed == query => {}
        _ => app.message_search.typed = Some((query.clone(), Instant::now())),
    }
    let rested = app.message_search.typed.as_ref().is_some_and(|(_, at)| at.elapsed() >= DELAY);
    if !rested {
        ctx.request_repaint_after(DELAY);
        return;
    }
    if app.message_search.searched.as_deref() != Some(query.as_str()) {
        app.message_search.searched = Some(query.clone());
        app.load(move |db| async move {
            let hits = db.search_messages(&query, LIMIT).await.unwrap_or_default();
            Some(Loaded::Found(Found::Global { query, hits }))
        });
    }
}

/// A snippet with its matches (between `\u{2}` and `\u{3}`) in bold.
fn snippet(text: &str, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut bold = false;
    for part in text.split_inclusive(['\u{2}', '\u{3}']) {
        let (body, mark) = match part.chars().last() {
            Some(c @ ('\u{2}' | '\u{3}')) => (&part[..part.len() - c.len_utf8()], Some(c)),
            _ => (part, None),
        };
        let font = if bold { theme::semibold(13.) } else { theme::regular(13.) };
        let format = egui::TextFormat { font_id: font, color, ..Default::default() };
        crate::chat::append_with_emoji(&mut job, &body.replace(['\n', '\r'], " "), format);
        if let Some(m) = mark {
            bold = m == '\u{2}';
        }
    }
    job.wrap.max_width = width;
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    job
}

/// The "Messages" results under the chat list's matches. Returns a hit
/// clicked.
pub fn message_results(app: &App, ui: &mut Ui, palette: &Palette) -> Option<(ConversationId, i64)> {
    let hits = &app.message_search.hits;
    if hits.is_empty() {
        return None;
    }
    let mut open = None;
    ui.add_space(10.);
    ui.horizontal(|ui| {
        ui.add_space(24.);
        ui.label(RichText::new("Messages").font(theme::semibold(14.)));
    });
    ui.add_space(4.);
    for hit in hits {
        let Some(c) = app.conversation(&hit.conversation_id) else { continue };
        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.), Sense::click());
        let row = rect.shrink2(Vec2::new(10., 2.));
        if r.hovered() {
            ui.painter().rect_filled(row, CornerRadius::same(10), palette.fill);
        }
        widgets::paint_avatar(ui, Rect::from_min_size(Pos2::new(row.left() + 12., row.center().y - 24.), Vec2::splat(48.)), &widgets::AvatarSpec::of(c));
        let left = row.left() + 72.;
        let time = ui.painter().layout_no_wrap(crate::format::short_timestamp(hit.sent_at), theme::regular(12.), palette.text_secondary);
        let title_w = row.right() - 12. - time.size().x - 8. - left;
        let who = match (&hit.author, c.kind == langis_store::ConversationKind::Group) {
            (None, _) => format!("You in {}", c.title),
            (Some(a), true) => format!("{} in {}", app.name(a), c.title),
            (Some(_), false) => c.title.clone(),
        };
        let mut job = egui::text::LayoutJob::simple_singleline(who, theme::semibold(14.), palette.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(title_w);
        let title = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(Pos2::new(left, row.top() + 12.), title, palette.text);
        ui.painter().galley(Pos2::new(row.right() - 12. - time.size().x, row.top() + 14.), time, palette.text_secondary);
        let g = ui.fonts_mut(|f| f.layout_job(snippet(&hit.snippet, palette.text_secondary, row.right() - 12. - left)));
        ui.painter().galley(Pos2::new(left, row.top() + 32.), g, palette.text_secondary);
        if r.clicked() {
            open = Some((hit.conversation_id.clone(), hit.message_id));
        }
    }
    open
}

/// The bar under the chat's header while searching in it.
pub fn find_bar(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    let Some(find) = chat.find.as_mut() else { return };
    let mut close = false;
    let mut go: Option<usize> = None;
    let members: Vec<String> = app.conversation(&chat.id).filter(|c| c.kind == langis_store::ConversationKind::Group).map(|c| c.members.clone()).unwrap_or_default();
    let mut bar_rect = Rect::NOTHING;
    egui::Panel::top("find-in-chat")
        .exact_size(48.)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            bar_rect = rect;
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1., palette.border));
            ui.horizontal_centered(|ui| {
                ui.add_space(16.);
                let w = (ui.available_width() - 280.).max(160.);
                let field = ui.allocate_ui(Vec2::new(w, 32.), |ui| widgets::text_field(ui, &mut find.query, "Search in chat", false)).inner;
                if std::mem::take(&mut find.focus) {
                    field.request_focus();
                }
                if field.changed() {
                    find.typed = Some(Instant::now());
                    find.list = true;
                }
                if field.clicked() {
                    find.list = true;
                }
                if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let shown = find.shown();
                    if !shown.is_empty() {
                        let at = shown.iter().position(|&i| i == find.index).map(|p| (p + 1) % shown.len()).unwrap_or(0);
                        go = Some(shown[at]);
                    }
                }
                let shown = find.shown();
                let count = if find.query.trim().is_empty() || find.searched.is_none() {
                    String::new()
                } else if shown.is_empty() {
                    "No results".to_owned()
                } else {
                    let at = shown.iter().position(|&i| i == find.index).map(|p| p + 1).unwrap_or(1);
                    format!("{at} of {}", shown.len())
                };
                // Desktop's "from" filter, in a group.
                if !members.is_empty() {
                    let label = match &find.from {
                        Some(aci) => format!("From {}", app.name(aci)),
                        None => "From anyone".to_owned(),
                    };
                    let menu = ui.id().with("search-from");
                    let r = ui.add(egui::Button::new(RichText::new(label).font(theme::regular(13.))).corner_radius(CornerRadius::same(12)));
                    if r.clicked() {
                        crate::menu::open_at(ui.ctx(), menu, Pos2::new(r.rect.left(), r.rect.bottom() + 4.));
                    }
                    crate::menu::show(ui.ctx(), menu, |m| {
                        if m.item(None, "Anyone") {
                            find.from = None;
                            find.list = true;
                        }
                        for aci in &members {
                            if m.item(None, &app.name(aci)) {
                                find.from = Some(aci.clone());
                                find.list = true;
                            }
                        }
                    });
                }
                ui.label(RichText::new(count).font(theme::regular(13.)).color(palette.text_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.);
                    if widgets::icon_button(ui, Icon::X, 18., "Close search").clicked() {
                        close = true;
                    }
                    // Down is newer; the hits are newest first.
                    let at = shown.iter().position(|&i| i == find.index);
                    if widgets::icon_button(ui, Icon::ChevronShallowDown, 18., "Next").clicked() && !shown.is_empty() {
                        go = Some(shown[at.map(|p| p.saturating_sub(1)).unwrap_or(0)]);
                    }
                    if widgets::icon_button(ui, Icon::ChevronShallowUp, 18., "Previous").clicked() && !shown.is_empty() {
                        go = Some(shown[at.map(|p| (p + 1).min(shown.len() - 1)).unwrap_or(0)]);
                    }
                });
            });
        });
    // The chat's results, newest first, under the bar.
    let shown = find.shown();
    if find.list && !shown.is_empty() && !find.query.trim().is_empty() {
        let width = (bar_rect.width() - 32.).min(560.);
        let area = egui::Area::new(egui::Id::unique("find-results"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(bar_rect.left() + 16., bar_rect.bottom() + 4.))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(palette.popover)
                    .corner_radius(CornerRadius::same(12))
                    .stroke(Stroke::new(1., palette.border))
                    .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(50) })
                    .inner_margin(6)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        egui::ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                            for &i in &shown {
                                let hit = &find.hits[i];
                                let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 58.), Sense::click());
                                if r.hovered() || i == find.index {
                                    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                                }
                                let who = match &hit.author {
                                    None => "You".to_owned(),
                                    Some(a) => app.name(a),
                                };
                                ui.painter().text(Pos2::new(rect.left() + 10., rect.top() + 8.), egui::Align2::LEFT_TOP, who, theme::semibold(13.), palette.text);
                                ui.painter().text(Pos2::new(rect.right() - 10., rect.top() + 8.), egui::Align2::RIGHT_TOP, crate::format::short_timestamp(hit.sent_at), theme::regular(12.), palette.text_secondary);
                                let g = ui.fonts_mut(|f| f.layout_job(snippet(&hit.snippet, palette.text_secondary, width - 20.)));
                                ui.painter().galley(Pos2::new(rect.left() + 10., rect.top() + 26.), g, palette.text_secondary);
                                if r.clicked() {
                                    go = Some(i);
                                }
                            }
                        });
                    });
            });
        let outside = ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !bar_rect.contains(p));
        if outside {
            find.list = false;
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if find.list && !shown.is_empty() {
            find.list = false;
        } else {
            close = true;
        }
    }
    if close {
        chat.find = None;
        return;
    }
    let query = find.query.trim().to_owned();
    let rested = find.typed.is_none_or(|t| t.elapsed() >= DELAY);
    if !rested {
        ui.ctx().request_repaint_after(DELAY);
    } else if !query.is_empty() && find.searched.as_deref() != Some(query.as_str()) {
        find.searched = Some(query.clone());
        let conversation = chat.id.clone();
        app.load(move |db| async move {
            let hits = db.search_conversation(&conversation, &query, 500).await.unwrap_or_default();
            Some(Loaded::Found(Found::Chat { conversation, query, hits }))
        });
    }
    if let Some(index) = go
        && let Some(hit) = find.hits.get(index)
    {
        find.index = index;
        find.list = false;
        let id = hit.message_id;
        if chat.messages.iter().any(|m| m.id == id) {
            chat.scroll_target = Some(id);
        } else {
            chat.jump_later(id);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn snippets_bold_their_matches() {
        let job = super::snippet("Meet at the \u{2}trailhead\u{3} at 8", egui::Color32::WHITE, 200.);
        assert_eq!(job.text, "Meet at the trailhead at 8");
        let bold: Vec<&str> = job.sections.iter().filter(|s| s.format.font_id == crate::theme::semibold(13.)).map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0]).collect();
        assert_eq!(bold, ["trailhead"]);
    }
}
