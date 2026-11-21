//! Message search: the chat list's "Messages" results under its chats
//! (Desktop's `LeftPaneSearchHelper`), or one chat's, with that chat as a
//! pill in the search box (`searchInConversation`, from its header).

use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use langis_store::{ConversationId, SearchHit};

use crate::app::{App, Loaded};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

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
    /// People matching who have no chat in the list (Desktop's Contacts).
    pub contacts: Vec<langis_store::DirectoryEntry>,
    /// Searched, not answered yet.
    pub loading: bool,
    /// Desktop's search in one chat (`searchInConversation`): the pill in
    /// the chat list's search box, and only that chat's messages.
    pub within: Option<ConversationId>,
}

/// Results for a query, to whichever search asked.
pub enum Found {
    Global { query: String, hits: Vec<SearchHit>, contacts: Vec<langis_store::DirectoryEntry> },
}

impl App {
    pub fn on_found(&mut self, found: Found) {
        match found {
            Found::Global { query, hits, contacts } => {
                if self.message_search.searched.as_deref() == Some(query.as_str()) {
                    self.message_search.hits = hits;
                    self.message_search.contacts = contacts;
                    self.message_search.loading = false;
                }
            }
        }
    }
}

/// The search as typed: searches once it rests.
pub fn follow_global(app: &mut App, ctx: &egui::Context) {
    let query = app.search.trim().to_owned();
    let within = app.message_search.within.clone();
    if query.is_empty() {
        app.message_search = Global { within, ..Global::default() };
        return;
    }
    // The searched key carries the chat, so changing it searches again.
    let key = match &within {
        Some(c) => format!("{c}\u{0}{query}"),
        None => query.clone(),
    };
    match &app.message_search.typed {
        Some((typed, _)) if *typed == query => {}
        _ => app.message_search.typed = Some((query.clone(), Instant::now())),
    }
    let rested = app.message_search.typed.as_ref().is_some_and(|(_, at)| at.elapsed() >= DELAY);
    if !rested {
        ctx.request_repaint_after(DELAY);
        return;
    }
    if app.message_search.searched.as_deref() != Some(key.as_str()) {
        app.message_search.searched = Some(key.clone());
        app.message_search.loading = true;
        if let Some(conversation) = within {
            app.load(move |db| async move {
                let hits = db.search_conversation(&conversation, &query, LIMIT).await.unwrap_or_default();
                Some(Loaded::Found(Found::Global { query: key, hits, contacts: Vec::new() }))
            });
            return;
        }
        // People without a chat yet: those with one show under Chats.
        // Nor anyone we removed (Desktop's search, `removalStage`).
        let listed: std::collections::HashSet<ConversationId> = app.conversations.iter().filter(|c| c.last_message_at > 0 || c.removal_stage.is_some()).map(|c| c.id.clone()).collect();
        app.load(move |db| async move {
            let hits = db.search_messages(&query, LIMIT).await.unwrap_or_default();
            let q = query.to_lowercase();
            let digits: String = query.chars().filter(char::is_ascii_digit).collect();
            let contacts = db
                .people()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|p| !listed.contains(&p.id))
                .filter(|p| p.title.to_lowercase().contains(&q) || (digits.len() >= 3 && p.phone.as_deref().is_some_and(|ph| ph.contains(&digits))))
                .take(20)
                .collect();
            Some(Loaded::Found(Found::Global { query, hits, contacts }))
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
/// Desktop's search results after Chats: Contacts (people with no chat
/// yet), then Messages, each under its list header; spinner rows while
/// searching. Returns a message to open, or a person to start a chat with
/// (message id 0).
pub fn message_results(app: &App, ui: &mut Ui, palette: &Palette) -> Option<(ConversationId, i64)> {
    let mut open = None;
    let heading = |ui: &mut Ui, text: &str| {
        let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), Sense::hover());
        ui.painter().text(Pos2::new(row.left() + 11. + 16., row.bottom() - 8.), egui::Align2::LEFT_BOTTOM, text, theme::semibold(14.), palette.text);
    };
    let search = &app.message_search;
    if !search.contacts.is_empty() {
        heading(ui, tr("Contacts"));
        for p in &search.contacts {
            let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.), Sense::click());
            let row = rect.shrink2(Vec2::new(11., 2.));
            if r.hovered() {
                ui.painter().rect_filled(row, CornerRadius::same(10), palette.fill);
            }
            let sid = p.id.as_direct().unwrap_or_default();
            let color = sid.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
            widgets::paint_avatar(ui, Rect::from_min_size(Pos2::new(row.left() + 14., row.center().y - 18.), Vec2::splat(36.)), &widgets::AvatarSpec::person(&p.title, color));
            ui.painter().text(Pos2::new(row.left() + 62., row.center().y - 1.), egui::Align2::LEFT_BOTTOM, &p.title, theme::semibold(14.), palette.text);
            if let Some(phone) = &p.phone {
                ui.painter().text(Pos2::new(row.left() + 62., row.center().y + 1.), egui::Align2::LEFT_TOP, phone, theme::regular(13.), palette.text_secondary);
            }
            if r.clicked() {
                open = Some((p.id.clone(), 0));
            }
        }
    }
    let hits = &search.hits;
    if search.loading && hits.is_empty() {
        // Desktop's loading rows.
        heading(ui, tr("Messages"));
        for _ in 0..3 {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.), Sense::hover());
            let row = rect.shrink2(Vec2::new(11., 2.));
            ui.painter().circle_filled(Pos2::new(row.left() + 38., row.center().y), 24., palette.fill);
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(row.left() + 74., row.top() + 16.), Vec2::new(120., 12.)), CornerRadius::same(6), palette.fill);
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(row.left() + 74., row.top() + 38.), Vec2::new(row.width() - 100., 12.)), CornerRadius::same(6), palette.fill);
        }
        return open;
    }
    if hits.is_empty() {
        return open;
    }
    heading(ui, tr("Messages"));
    for hit in hits {
        let Some(c) = app.conversation(&hit.conversation_id) else { continue };
        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.), Sense::click());
        let row = rect.shrink2(Vec2::new(11., 2.));
        if r.hovered() {
            ui.painter().rect_filled(row, CornerRadius::same(10), palette.fill);
        }
        // Desktop's `MessageSearchResult`: the sender's avatar, and
        // "X to Y" (`searchResultHeader--*`).
        let group = c.kind == langis_store::ConversationKind::Group;
        let avatar = Rect::from_min_size(Pos2::new(row.left() + 14., row.center().y - 24.), Vec2::splat(48.));
        let who = match (&hit.author, c.kind) {
            (None, langis_store::ConversationKind::NoteToSelf) => {
                widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec::of(c));
                tr("Note to Self").to_owned()
            }
            (None, _) => {
                widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo: app.me.1.as_deref(), ..widgets::AvatarSpec::person(&app.me.0, 0) });
                crate::i18n::tf("icu:searchResultHeader--you-to-group", &format!("You to {}", c.title), &[("receiverGroup", crate::i18n::Arg::from(&c.title))])
            }
            (Some(a), _) => {
                let name = app.name(a);
                let photo = app.conversation(&ConversationId::direct(a)).and_then(|p| p.avatar_path.clone());
                let color = a.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                widgets::paint_avatar(ui, avatar, &widgets::AvatarSpec { photo: photo.as_deref(), ..widgets::AvatarSpec::person(&name, color) });
                if group { format!("{name} to {}", c.title) } else { crate::i18n::tf("icu:searchResultHeader--sender-to-you", &format!("{name} to You"), &[("sender", crate::i18n::Arg::from(&name))]) }
            }
        };
        let left = row.left() + 74.;
        let time = ui.painter().layout_no_wrap(crate::format::short_timestamp(hit.sent_at), theme::regular(12.), palette.text_secondary);
        let title_w = row.right() - 12. - time.size().x - 8. - left;
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
