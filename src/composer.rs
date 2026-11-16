//! Desktop's composition area (`CompositionArea`): the rounded input with
//! its formatting (bold, italic, strikethrough, monospace, spoiler, from
//! the selection's toolbar or Desktop's shortcuts), @mentions in groups,
//! a link preview for the first link, the reply and edit bars, emoji and
//! attachments, and voice messages.

use std::collections::HashSet;
use std::ops::Range;
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use langis_core::{Command, LinkPreviewDraft};
use langis_store::{ConversationKind, MentionSpan, Message, StyleSpan, TextStyle};

use crate::app::{App, PendingSend};
use crate::chat::ChatView;
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

/// How long after the last keystroke "typing" stops (Desktop's
/// `TYPING_IDLE_TIMEOUT`, a little under its 15s pause timer).
const TYPING_IDLE: Duration = Duration::from_secs(3);

/// How long the text must rest before a link's preview is fetched
/// (Desktop's `LINK_PREVIEW_DEBOUNCE`-like pause).
const PREVIEW_DELAY: Duration = Duration::from_millis(500);

/// The composer's formatting and mentions, over its text's bytes, kept in
/// step with the text as it's edited.
#[derive(Default, Clone, Debug)]
pub struct Spans {
    pub styles: Vec<StyleSpan>,
    pub mentions: Vec<MentionSpan>,
    /// The text the spans were last placed over.
    prev: String,
}

impl Spans {
    /// Spans for a text set from outside (a draft, a message to edit).
    pub fn new(text: &str, styles: Vec<StyleSpan>, mentions: Vec<MentionSpan>) -> Self {
        Self { styles, mentions, prev: text.to_owned() }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Moves the spans for an edit from `prev` to `text`: what's after
    /// the edit shifts, a style around it grows or shrinks, and a mention
    /// the edit touched is gone (Desktop deletes a mention whole).
    pub fn follow(&mut self, text: &str) {
        if self.prev == text {
            return;
        }
        let old = std::mem::replace(&mut self.prev, text.to_owned());
        let prefix: usize = old.chars().zip(text.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
        let max_suffix = (old.len() - prefix).min(text.len() - prefix);
        let suffix: usize = old[prefix..]
            .chars()
            .rev()
            .zip(text[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .scan(0, |sum, n| {
                *sum += n;
                Some((*sum, n))
            })
            .take_while(|(sum, _)| *sum <= max_suffix)
            .map(|(_, n)| n)
            .sum();
        let old_end = old.len() - suffix;
        let new_end = text.len() - suffix;
        let delta = new_end as isize - old_end as isize;
        let moved = |at: usize, end: bool| -> usize {
            if at < prefix || (end && at == prefix) {
                at
            } else if at >= old_end {
                (at as isize + delta) as usize
            } else if end {
                prefix
            } else {
                new_end
            }
        };
        self.styles = self
            .styles
            .iter()
            .filter_map(|s| {
                let (a, b) = (moved(s.start, false), moved(s.start + s.len, true));
                // Typing inside a style keeps it going.
                let b = if s.start < prefix && s.start + s.len > prefix && s.start + s.len >= old_end { (s.start + s.len) as isize + delta } else { b as isize } as usize;
                (b > a).then_some(StyleSpan { start: a, len: b - a, style: s.style })
            })
            .collect();
        self.mentions.retain(|m| m.start + m.len <= prefix || m.start >= old_end);
        for m in &mut self.mentions {
            if m.start >= old_end {
                m.start = (m.start as isize + delta) as usize;
            }
        }
        let len = text.len();
        self.styles.retain(|s| s.start + s.len <= len && text.is_char_boundary(s.start) && text.is_char_boundary(s.start + s.len));
        self.mentions.retain(|m| m.start + m.len <= len && text.is_char_boundary(m.start) && text.is_char_boundary(m.start + m.len));
    }

    /// Whether `style` covers all of `range`.
    pub fn has(&self, style: TextStyle, range: &Range<usize>) -> bool {
        let mut at = range.start;
        let mut spans: Vec<&StyleSpan> = self.styles.iter().filter(|s| s.style == style).collect();
        spans.sort_by_key(|s| s.start);
        for s in spans {
            if s.start <= at && s.start + s.len > at {
                at = s.start + s.len;
            }
        }
        at >= range.end
    }

    /// Desktop's formatting toggle: on for the range unless it's all on.
    pub fn toggle(&mut self, style: TextStyle, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        if self.has(style, &range) {
            let mut out = Vec::new();
            for s in std::mem::take(&mut self.styles) {
                let end = s.start + s.len;
                if s.style != style || end <= range.start || s.start >= range.end {
                    out.push(s);
                    continue;
                }
                if s.start < range.start {
                    out.push(StyleSpan { start: s.start, len: range.start - s.start, style });
                }
                if end > range.end {
                    out.push(StyleSpan { start: range.end, len: end - range.end, style });
                }
            }
            self.styles = out;
        } else {
            let (mut start, mut end) = (range.start, range.end);
            self.styles.retain(|s| {
                let overlaps = s.style == style && s.start <= end && s.start + s.len >= start;
                if overlaps {
                    start = start.min(s.start);
                    end = end.max(s.start + s.len);
                }
                !overlaps
            });
            self.styles.push(StyleSpan { start, len: end - start, style });
        }
    }

    /// Puts a mention of `aci` (shown "@name") in place of `replace`;
    /// returns the char index just after it (and the space that follows).
    pub fn insert_mention(&mut self, text: &mut String, replace: Range<usize>, name: &str, aci: &str) -> usize {
        let shown = format!("@{name}");
        text.replace_range(replace.clone(), &format!("{shown} "));
        self.follow(text);
        self.mentions.push(MentionSpan { start: replace.start, len: shown.len(), aci: aci.to_owned() });
        self.mentions.sort_by_key(|m| m.start);
        text[..replace.start + shown.len() + 1].chars().count()
    }

    /// The text as Signal sends it: each mention a placeholder, who they
    /// are in order, and the styles over that body (trimmed of the
    /// whitespace around it, as Desktop sends).
    pub fn to_send(&self, text: &str) -> (String, Vec<String>, Vec<StyleSpan>) {
        let lead = text.len() - text.trim_start().len();
        let end = text.trim_end().len().max(lead);
        let mut body = String::new();
        let mut acis = Vec::new();
        // Where each byte of `text` lands in `body`.
        let mut map = vec![0usize; text.len() + 1];
        let mut mentions: Vec<&MentionSpan> = self.mentions.iter().filter(|m| m.start >= lead && m.start + m.len <= end).collect();
        mentions.sort_by_key(|m| m.start);
        let mut at = lead;
        for m in mentions {
            for (i, c) in text[at..m.start].char_indices() {
                map[at + i] = body.len();
                body.push(c);
            }
            for slot in &mut map[m.start..m.start + m.len] {
                *slot = body.len();
            }
            body.push(langis_core::mapping::MENTION_PLACEHOLDER);
            acis.push(m.aci.clone());
            at = m.start + m.len;
        }
        for (i, c) in text[at..end].char_indices() {
            map[at + i] = body.len();
            body.push(c);
        }
        for slot in &mut map[end..] {
            *slot = body.len();
        }
        let styles = self
            .styles
            .iter()
            .filter_map(|s| {
                let a = map[s.start.clamp(lead, end)];
                let b = map[(s.start + s.len).clamp(lead, end)];
                (b > a).then_some(StyleSpan { start: a, len: b - a, style: s.style })
            })
            .collect();
        (body, acis, styles)
    }
}

/// The link preview for the composer's first link.
#[derive(Default)]
pub struct Preview {
    /// The link it's for, and when that link was last seen changing.
    url: Option<String>,
    since: Option<Instant>,
    requested: Option<String>,
    pub draft: Option<LinkPreviewDraft>,
    /// Links whose preview was closed: not offered again in this draft.
    dismissed: HashSet<String>,
}

impl Preview {
    pub fn arrived(&mut self, url: &str, draft: Option<LinkPreviewDraft>) {
        if self.requested.as_deref() == Some(url) {
            self.draft = draft;
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Follows the text's first link; true when it's time to fetch it.
    fn follow(&mut self, text: &str, enabled: bool) -> Option<String> {
        let link = enabled
            .then(|| crate::chat::find_links(text).into_iter().map(|(_, url)| url).find(|u| langis_core::link_preview::should_preview(u)))
            .flatten()
            .filter(|u| !self.dismissed.contains(u));
        if link != self.url {
            self.url = link;
            self.since = Some(Instant::now());
            if self.draft.as_ref().is_some_and(|d| Some(&d.url) != self.url.as_ref()) {
                self.draft = None;
            }
        }
        let url = self.url.clone()?;
        let rested = self.since.is_some_and(|t| t.elapsed() >= PREVIEW_DELAY);
        if rested && self.requested.as_ref() != Some(&url) {
            self.requested = Some(url.clone());
            return Some(url);
        }
        None
    }
}

/// Lays out the composer's text with its styles and mentions.
fn layout(ui: &Ui, text: &str, spans: &Spans, palette: &Palette, wrap: f32) -> std::sync::Arc<egui::Galley> {
    let mut cuts = vec![0, text.len()];
    for s in &spans.styles {
        cuts.extend([s.start, s.start + s.len]);
    }
    for m in &spans.mentions {
        cuts.extend([m.start, m.start + m.len]);
    }
    cuts.retain(|&c| c <= text.len() && text.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();
    let mut job = egui::text::LayoutJob::default();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let active = |style: TextStyle| spans.styles.iter().any(|s| s.style == style && s.start <= a && b <= s.start + s.len);
        let mention = spans.mentions.iter().any(|m| m.start <= a && b <= m.start + m.len);
        let mut format = egui::TextFormat { font_id: theme::regular(14.), color: palette.text, line_height: Some(20.), ..Default::default() };
        if active(TextStyle::Bold) || mention {
            format.font_id = theme::semibold(14.);
        }
        if active(TextStyle::Italic) {
            format.font_id = theme::italic(14.);
        }
        if active(TextStyle::Monospace) {
            format.font_id = egui::FontId::monospace(13.);
        }
        if active(TextStyle::Strikethrough) {
            format.strikethrough = Stroke::new(1., palette.text);
        }
        if mention {
            format.background = palette.fill_pressed;
        }
        if active(TextStyle::Spoiler) {
            format.background = palette.text_secondary.gamma_multiply(0.35);
        }
        crate::chat::append_with_emoji(&mut job, &text[a..b], format);
    }
    job.wrap.max_width = wrap;
    ui.fonts_mut(|f| f.layout_job(job))
}

/// The formatting buttons, in Desktop's order, with their shortcuts
/// (`FormattingMenu`: Ctrl+B, Ctrl+I, Ctrl+Shift+X, Ctrl+E, Ctrl+Shift+B).
const FORMATS: [(TextStyle, Icon, &str, egui::Key, bool); 5] = [
    (TextStyle::Bold, Icon::TextformatBoldBold, "Bold (Ctrl+B)", egui::Key::B, false),
    (TextStyle::Italic, Icon::TextformatItalicBold, "Italic (Ctrl+I)", egui::Key::I, false),
    (TextStyle::Strikethrough, Icon::TextformatStrikethroughBold, "Strikethrough (Ctrl+Shift+X)", egui::Key::X, true),
    (TextStyle::Monospace, Icon::TextformatMonospaceBold, "Monospace (Ctrl+E)", egui::Key::E, false),
    (TextStyle::Spoiler, Icon::TextformatSpoilerBold, "Spoiler (Ctrl+Shift+B)", egui::Key::B, true),
];

/// Who can be mentioned: the group's members (not us), by name.
fn members(app: &App, c: &langis_store::Conversation) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = c.members.iter().filter(|m| **m != app.aci).map(|m| (m.clone(), app.name(m))).collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    out
}

/// The "@query" being typed just before `cursor` (a byte offset), if any
/// (Desktop's `MentionCompletion` match: `@` at a word's start).
fn mention_query(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    let before = text.get(..cursor)?;
    let at = before.rfind('@')?;
    let query = &before[at + 1..];
    if query.chars().any(|c| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '+')) || query.chars().count() > 30 {
        return None;
    }
    if before[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    Some((at..cursor, query.to_lowercase()))
}

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(b, _)| b).unwrap_or(text.len())
}

pub fn show(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let blocked = c.blocked || c.left || c.ended;
    egui::Panel::bottom("composer")
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin { left: 16, right: 16, top: 10, bottom: 12 }))
        .show(ui, |ui| {
            if blocked {
                ui.vertical_centered(|ui| {
                    let text = if c.blocked { "You blocked this chat." } else { "You can't send messages to this group." };
                    ui.label(RichText::new(text).color(palette.text_secondary));
                });
                return;
            }
            if chat.recording.is_some() {
                crate::voice::recording_bar(app, ui, palette, chat, c);
                return;
            }
            edit_bar(ui, palette, chat);
            reply_bar(app, ui, palette, chat);
            preview_card(ui, palette, chat);
            input_row(app, ui, palette, chat, c);
        });
}

fn edit_bar(ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    if chat.editing.is_none() {
        return;
    }
    let mut close = false;
    egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(18.), Sense::hover());
            Icon::Edit.paint(ui, icon, 16., palette.text);
            ui.label(RichText::new("Edit message").font(theme::semibold(13.)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::icon_button(ui, Icon::XCompact, 16., "Cancel editing").clicked();
            });
        });
    });
    if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) && chat.popup.is_none() {
        chat.editing = None;
        chat.composer.clear();
        chat.spans.clear();
        chat.preview.reset();
    }
    ui.add_space(6.);
}

fn reply_bar(app: &App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    let Some(q) = chat.quote.and_then(|id| chat.messages.iter().find(|m| m.id == id)) else { return };
    let who = match &q.author {
        None => "You".to_owned(),
        Some(a) => app.name(a),
    };
    let text = if q.body.is_empty() { q.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_default() } else { crate::format::one_line(&q.body) };
    let mut close = false;
    egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("Replying to {who}")).font(theme::semibold(13.)));
                ui.add(egui::Label::new(RichText::new(text).font(theme::regular(13.)).color(palette.text_secondary)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::icon_button(ui, Icon::XCompact, 16., "Cancel reply").clicked();
            });
        });
    });
    if close {
        chat.quote = None;
    }
    ui.add_space(6.);
}

/// Desktop's `StagedLinkPreview` above the input.
fn preview_card(ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    let Some(p) = chat.preview.draft.clone() else { return };
    let mut close = false;
    egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            if let Some(image) = &p.image {
                let uri = format!("bytes://link-preview/{}", p.url);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.), Sense::hover());
                egui::Image::new(egui::ImageSource::Bytes { uri: uri.into(), bytes: image.bytes.to_vec().into() })
                    .corner_radius(CornerRadius::same(8))
                    .fit_to_exact_size(rect.size())
                    .paint_at(ui, rect);
            }
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 40.);
                ui.add(egui::Label::new(RichText::new(&p.title).font(theme::semibold(13.))).truncate());
                if let Some(d) = &p.description {
                    ui.add(egui::Label::new(RichText::new(crate::format::one_line(d)).font(theme::regular(12.)).color(palette.text_secondary)).truncate());
                }
                let domain = p.url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(&p.url).to_owned();
                ui.label(RichText::new(domain).font(theme::regular(12.)).color(palette.text_secondary));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::icon_button(ui, Icon::XCompact, 16., "Remove link preview").clicked();
            });
        });
    });
    if close {
        chat.preview.dismissed.insert(p.url.clone());
        chat.preview.draft = None;
    }
    ui.add_space(6.);
}

fn input_row(app: &mut App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let id = input_id(&c.id);
    let group = c.kind == ConversationKind::Group;
    let mut send = false;

    // Where the cursor and selection are (from last frame), in bytes.
    let state = egui::TextEdit::load_state(ui.ctx(), id);
    let range = state.as_ref().and_then(|s| s.cursor.char_range());
    let (cursor, selection) = match range {
        Some(r) => {
            let (p, q) = (r.primary.index.0, r.secondary.index.0);
            (byte_of(&chat.composer, p), byte_of(&chat.composer, p.min(q))..byte_of(&chat.composer, p.max(q)))
        }
        None => (chat.composer.len(), 0..0),
    };
    let focused = ui.memory(|m| m.has_focus(id));

    // @mentions: the members matching what's typed after "@".
    let mut mention = None;
    if group && focused && selection.is_empty() {
        if let Some((replace, query)) = mention_query(&chat.composer, cursor) {
            let found: Vec<(String, String)> = members(app, c).into_iter().filter(|(_, name)| query.is_empty() || name.to_lowercase().contains(&query)).take(8).collect();
            if !found.is_empty() {
                mention = Some((replace, found));
            }
        }
    }
    if mention.is_none() {
        chat.mention_index = 0;
    }
    let mut picked_mention = None;
    if let Some((_, found)) = &mention {
        // Arrow keys and Enter go to the list while it's open.
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
            )
        });
        let n = found.len();
        if down {
            chat.mention_index = (chat.mention_index + 1) % n;
        }
        if up {
            chat.mention_index = (chat.mention_index + n - 1) % n;
        }
        chat.mention_index = chat.mention_index.min(n - 1);
        if enter {
            picked_mention = Some(chat.mention_index);
        }
    }

    // Formatting shortcuts on the selection.
    let mut toggled = None;
    if focused && !selection.is_empty() {
        for (style, _, _, key, shift) in FORMATS {
            let mods = if shift { egui::Modifiers::COMMAND | egui::Modifiers::SHIFT } else { egui::Modifiers::COMMAND };
            if ui.input_mut(|i| i.consume_key(mods, key)) {
                toggled = Some(style);
            }
        }
    }

    // Arrow up in an empty composer edits our last message (Desktop's).
    if focused && chat.composer.is_empty() && chat.editing.is_none() && ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        let now = chrono::Utc::now().timestamp_millis();
        let note_to_self = c.kind == ConversationKind::NoteToSelf;
        if let Some(m) = chat.messages.iter().rev().find(|m| m.direction == langis_store::Direction::Outgoing && m.notice.is_none()).cloned()
            && langis_core::edits::can_edit(&m, note_to_self, now)
        {
            chat.start_editing(app, m);
        }
    }

    let mut input_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        let fun = widgets::icon_button(ui, Icon::Emoji, 20., "Emoji, stickers and GIFs");
        if fun.clicked() {
            chat.emoji_open = !chat.emoji_open;
        }
        let has_text = !chat.composer.trim().is_empty();
        let right_w = 2. * 32. + 8.;
        let width = (ui.available_width() - right_w).max(100.);
        let spans = chat.spans.clone();
        let pal = *palette;
        let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| layout(ui, text.as_str(), &spans, &pal, wrap);
        let response = egui::Frame::new()
            .fill(palette.bubble_in)
            .corner_radius(CornerRadius::same(18))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(width - 24.);
                ui.add(
                    egui::TextEdit::multiline(&mut chat.composer)
                        .hint_text(if chat.editing.is_some() { "Edit message" } else { "Message" })
                        .frame(egui::Frame::NONE)
                        .desired_rows(1)
                        .desired_width(width - 24.)
                        .font(theme::regular(14.))
                        .layouter(&mut layouter)
                        .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter))
                        .id(id),
                )
            })
            .inner;
        input_rect = response.rect;
        chat.spans.follow(&chat.composer);
        if response.changed() {
            chat.last_typed = Some(Instant::now());
            if !chat.typing_sent && !chat.composer.is_empty() && chat.editing.is_none() {
                chat.typing_sent = true;
                app.send(Command::Typing { conversation: c.id.clone(), started: true });
            }
        }
        if response.has_focus() && picked_mention.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
            send = true;
        }
        if !response.has_focus() && app.safety.is_none() && app.lightbox.is_none() && chat.dialog.is_none() && ui.memory(|m| m.focused().is_none()) {
            response.request_focus();
        }
        if widgets::icon_button(ui, Icon::Plus, 20., "Add attachment").clicked() {
            chat.attach_menu = !chat.attach_menu;
        }
        let attach_button = ui.min_rect();
        if has_text || chat.editing.is_some() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
            ui.painter().circle_filled(rect.center(), 16., palette.badge);
            let icon = if chat.editing.is_some() { Icon::Check } else { Icon::SendFill };
            icon.paint(ui, rect, 18., Color32::WHITE);
            if r.on_hover_text(if chat.editing.is_some() { "Save" } else { "Send" }).clicked() {
                send = true;
            }
        } else if widgets::icon_button(ui, Icon::Mic, 20., "Voice message").clicked() {
            crate::voice::start(app, chat);
        }
        if chat.emoji_open {
            let anchor = fun.rect.left_top();
            if let Some(picked) = crate::fun::show(app, ui.ctx(), anchor, chat, c) {
                insert_at_cursor(ui.ctx(), id, chat, picked);
            }
        }
        if chat.attach_menu {
            attach_menu(app, ui, palette, chat, c, Pos2::new(attach_button.right() - 220., input_rect.top() - 8.));
        }
    });

    if let Some((replace, found)) = mention {
        let index = picked_mention.unwrap_or(usize::MAX);
        let mut chosen = (index != usize::MAX).then(|| found[index.min(found.len() - 1)].clone());
        let at = Pos2::new(input_rect.left(), input_rect.top() - 12. - found.len() as f32 * 40. - 12.);
        let closed = crate::actions::popover(ui.ctx(), "mentions", at, 260., Some(input_rect), |ui| {
            for (i, (aci, name)) in found.iter().enumerate() {
                let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.), Sense::click());
                if i == chat.mention_index || r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                }
                let color = aci.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                let photo = app.conversation(&langis_store::ConversationId::direct(aci)).and_then(|c| c.avatar_path.clone());
                let spec = widgets::AvatarSpec { name, color, photo: photo.as_deref(), group: false, note_to_self: false };
                widgets::paint_avatar(ui, Rect::from_min_size(rect.min + Vec2::new(8., 6.), Vec2::splat(28.)), &spec);
                ui.painter().text(Pos2::new(rect.left() + 46., rect.center().y), Align2::LEFT_CENTER, name, theme::regular(14.), palette.text);
                if r.clicked() {
                    chosen = Some((aci.clone(), name.clone()));
                }
            }
        });
        if let Some((aci, name)) = chosen {
            let after = chat.spans.insert_mention(&mut chat.composer, replace, &name, &aci);
            set_cursor(ui.ctx(), id, after);
        } else if closed {
            // Escape: leave the text as typed.
        }
    }

    // The selection's formatting toolbar.
    if focused && !selection.is_empty() && mention_query(&chat.composer, cursor).is_none() {
        let at = Pos2::new(input_rect.left() + 8., input_rect.top() - 52.);
        let spans = chat.spans.clone();
        crate::actions::popover(ui.ctx(), "formatting", at, 5. * 36. + 8., Some(input_rect), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                for (style, icon, tip, _, _) in FORMATS {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
                    let on = spans.has(style, &selection);
                    if on || r.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), if on { palette.fill_pressed } else { palette.fill });
                    }
                    icon.paint(ui, rect, 18., palette.text);
                    if r.on_hover_text(tip).clicked() {
                        toggled = Some(style);
                    }
                }
            });
        });
    }
    if let Some(style) = toggled {
        chat.spans.toggle(style, selection.clone());
    }

    // The link preview for the first link.
    let previews_on = app.link_previews;
    if chat.editing.is_none() || previews_on {
        if let Some(url) = chat.preview.follow(&chat.composer, previews_on) {
            app.send(Command::FetchLinkPreview { url });
        }
        if chat.preview.url.is_some() && chat.preview.draft.is_none() {
            ui.ctx().request_repaint_after(PREVIEW_DELAY);
        }
    }

    if send {
        send_text(app, chat, c);
    }
    if chat.typing_sent && chat.last_typed.is_some_and(|t| t.elapsed() > TYPING_IDLE) {
        chat.typing_sent = false;
        app.send(Command::Typing { conversation: c.id.clone(), started: false });
    }
    if chat.typing_sent {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

/// The composer's text field for a chat.
pub fn input_id(conversation: &langis_store::ConversationId) -> egui::Id {
    egui::Id::unique(("composer", &conversation.0))
}

/// Selects chars `from..to` in the composer and focuses it.
pub fn select(ctx: &egui::Context, id: egui::Id, from: usize, to: usize) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(egui::text::CharIndex(from)), CCursor::new(egui::text::CharIndex(to)))));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

pub fn set_cursor(ctx: &egui::Context, id: egui::Id, char_index: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(egui::text::CharIndex(char_index)))));
        state.store(ctx, id);
    }
    ctx.memory_mut(|m| m.request_focus(id));
}

/// Puts `text` at the cursor (an emoji from the picker).
fn insert_at_cursor(ctx: &egui::Context, id: egui::Id, chat: &mut ChatView, text: &str) {
    let at = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
    let char_index = at.unwrap_or_else(|| chat.composer.chars().count());
    let byte = byte_of(&chat.composer, char_index);
    chat.composer.insert_str(byte, text);
    chat.spans.follow(&chat.composer);
    set_cursor(ctx, id, char_index + text.chars().count());
}

/// Desktop's attachment menu: photos and files, a poll, view-once media.
fn attach_menu(app: &mut App, ui: &Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation, at: Pos2) {
    let _ = palette;
    let mut choice = None;
    let closed = crate::actions::popover(ui.ctx(), "attach-menu", at - Vec2::new(0., 3. * 32. + 12.), 210., None, |ui| {
        if crate::actions::menu_row(ui, Some(Icon::PhotoSquare), "Photos, videos & files", false) {
            choice = Some(0);
        }
        if c.kind != ConversationKind::NoteToSelf && crate::actions::menu_row(ui, Some(Icon::ViewOnce), "View-once photo or video", false) {
            choice = Some(2);
        }
        if crate::actions::menu_row_with(ui, |ui, rect, color| crate::glyphs::poll(ui, rect, color), true, "Poll", false) {
            choice = Some(1);
        }
    });
    if closed || choice.is_some() {
        chat.attach_menu = false;
    }
    match choice {
        Some(0) => attach(app, chat, c),
        Some(1) => chat.dialog = Some(crate::actions::Dialog::Poll(crate::polls::NewPoll::default())),
        Some(2) => attach_view_once(app, c),
        _ => {}
    }
}

pub fn send_text(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    if chat.composer.trim().is_empty() {
        return;
    }
    let (body, mentions, styles) = chat.spans.to_send(&chat.composer);
    let preview = chat.preview.draft.take().filter(|p| chat.composer.contains(&p.url) || chat.composer.contains(p.url.trim_start_matches("https://")));
    let command = match chat.editing.take() {
        Some(message_id) => Command::EditMessage { message_id, body, mentions, styles, preview },
        None => Command::SendText { conversation: c.id.clone(), body, quote: chat.quote.take(), preview, mentions, styles },
    };
    chat.composer.clear();
    chat.spans.clear();
    chat.preview.reset();
    chat.scroll_to_bottom = 10;
    if chat.typing_sent {
        chat.typing_sent = false;
        app.send(Command::Typing { conversation: c.id.clone(), started: false });
    }
    // The draft went out with the message.
    if !chat.draft_at_open.is_empty() {
        chat.draft_at_open.clear();
        let id = c.id.clone();
        app.load(move |db| async move {
            let _ = db.set_draft(&id, None).await;
            db.conversations().await.ok().map(crate::app::Loaded::Conversations)
        });
    }
    if matches!(command, Command::EditMessage { .. }) {
        app.send(command);
    } else {
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
}

/// Picks files to send: images go optimized as photos, the rest as files,
/// with the composer's text as the caption.
fn attach(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    let Some(paths) = rfd::FileDialog::new().set_title("Add attachment").pick_files() else { return };
    let (caption, mentions, styles) = chat.spans.to_send(&chat.composer);
    chat.composer.clear();
    chat.spans.clear();
    chat.preview.reset();
    let quote = chat.quote.take();
    let mut images = Vec::new();
    let mut first = true;
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            app.toast(format!("Couldn't read {}", path.display()));
            continue;
        };
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "heic") {
            images.push(langis_core::OutgoingImage { bytes: std::sync::Arc::new(bytes), file_name });
        } else {
            // The caption goes with the first file only.
            let (caption, mentions, styles) = if first && images.is_empty() { (caption.clone(), mentions.clone(), styles.clone()) } else { Default::default() };
            first = false;
            let command = Command::SendFile {
                conversation: c.id.clone(),
                file: langis_core::OutgoingFile { bytes: std::sync::Arc::new(bytes), file_name: file_name.unwrap_or_else(|| "file".into()), voice: false },
                caption,
                quote,
                mentions,
                styles,
            };
            app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
        }
    }
    if !images.is_empty() {
        let (caption, mentions, styles) = if first { (caption, mentions, styles) } else { Default::default() };
        let command = Command::SendImages { conversation: c.id.clone(), images, caption, quote, mentions, styles };
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
    chat.scroll_to_bottom = 10;
}

/// Desktop's view-once toggle: one photo or video, seen once.
fn attach_view_once(app: &mut App, c: &langis_store::Conversation) {
    let Some(path) = rfd::FileDialog::new().set_title("View-once photo or video").add_filter("Photos and videos", &["jpg", "jpeg", "png", "webp", "mp4", "mov", "m4v"]).pick_file() else { return };
    let Ok(bytes) = std::fs::read(&path) else {
        app.toast(format!("Couldn't read {}", path.display()));
        return;
    };
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let media = if matches!(ext.as_str(), "mp4" | "mov" | "m4v") {
        langis_core::OutgoingViewOnce::Video(langis_core::OutgoingFile { bytes: std::sync::Arc::new(bytes), file_name: file_name.unwrap_or_else(|| "video.mp4".into()), voice: false })
    } else {
        langis_core::OutgoingViewOnce::Image(langis_core::OutgoingImage { bytes: std::sync::Arc::new(bytes), file_name })
    };
    let command = Command::SendViewOnce { conversation: c.id.clone(), media };
    app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
}

/// The text, styles and mentions of a message, for editing it.
pub fn editable(m: &Message) -> (String, Spans) {
    (m.body.clone(), Spans::new(&m.body, m.styles.clone(), m.mentions.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold(start: usize, len: usize) -> StyleSpan {
        StyleSpan { start, len, style: TextStyle::Bold }
    }

    #[test]
    fn styles_follow_edits() {
        let mut spans = Spans::new("hello world", vec![bold(6, 5)], Vec::new());
        // Typing before the style moves it.
        spans.follow("oh hello world");
        assert_eq!(spans.styles, vec![bold(9, 5)]);
        // Typing inside it grows it.
        spans.follow("oh hello wonderful world");
        assert_eq!(spans.styles, vec![bold(9, 15)]);
        // Typing right after it doesn't.
        spans.follow("oh hello wonderful world!");
        assert_eq!(spans.styles, vec![bold(9, 15)]);
        // Deleting all of it removes it.
        spans.follow("oh hello !");
        assert!(spans.styles.is_empty());
    }

    #[test]
    fn toggling_adds_merges_and_splits() {
        let mut spans = Spans::default();
        spans.toggle(TextStyle::Bold, 0..4);
        spans.toggle(TextStyle::Bold, 2..8);
        assert_eq!(spans.styles, vec![bold(0, 8)]);
        assert!(spans.has(TextStyle::Bold, &(1..7)));
        spans.toggle(TextStyle::Bold, 3..5);
        spans.styles.sort_by_key(|s| s.start);
        assert_eq!(spans.styles, vec![bold(0, 3), bold(5, 3)]);
    }

    #[test]
    fn mentions_become_placeholders_with_styles_moved() {
        let mut text = "hi @pri".to_owned();
        let mut spans = Spans::new(&text, Vec::new(), Vec::new());
        let after = spans.insert_mention(&mut text, 3..7, "Priya Patel", "aci-priya");
        assert_eq!(text, "hi @Priya Patel ");
        assert_eq!(after, 16);
        text.push_str("look");
        spans.follow(&text);
        spans.toggle(TextStyle::Italic, 16..20);
        let (body, acis, styles) = spans.to_send(&text);
        assert_eq!(body, "hi \u{FFFC} look");
        assert_eq!(acis, vec!["aci-priya".to_owned()]);
        assert_eq!(styles, vec![StyleSpan { start: 7, len: 4, style: TextStyle::Italic }]);
        // Editing inside the mention drops it, leaving its text.
        text.replace_range(5..6, "");
        spans.follow(&text);
        assert!(spans.mentions.is_empty());
    }

    #[test]
    fn mention_queries() {
        assert_eq!(mention_query("hey @ma", 7), Some((4..7, "ma".into())));
        assert_eq!(mention_query("@", 1), Some((0..1, String::new())));
        assert_eq!(mention_query("mail@ma", 7), None);
        assert_eq!(mention_query("hey @ma rest", 12), None);
    }
}
