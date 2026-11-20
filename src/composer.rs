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
const TYPING_REFRESH: Duration = Duration::from_secs(10);

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
    /// Links that had no preview to fetch.
    failed: HashSet<String>,
}

impl Preview {
    pub fn arrived(&mut self, url: &str, draft: Option<LinkPreviewDraft>) {
        if self.requested.as_deref() == Some(url) {
            if draft.is_none() {
                self.failed.insert(url.to_owned());
            }
            self.draft = draft;
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Desktop's `module-staged-link-preview--is-loading`: a link is staged
    /// and its preview hasn't come yet.
    fn loading(&self) -> Option<&str> {
        self.url.as_deref().filter(|_| self.draft.is_none())
    }

    /// Follows the text's first link; true when it's time to fetch it.
    fn follow(&mut self, text: &str, enabled: bool) -> Option<String> {
        let link = enabled
            // The same link finder the backend checks a sent preview against,
            // so a preview shown here isn't dropped on send.
            .then(|| langis_core::link_preview::links_in(text).into_iter().find(|u| langis_core::link_preview::should_preview(u) && !self.dismissed.contains(u) && !self.failed.contains(u)))
            .flatten();
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
fn layout(ui: &Ui, text: &str, spans: &Spans, palette: &Palette, wrap: f32, spell: bool) -> std::sync::Arc<egui::Galley> {
    let wrong = if spell { crate::spell::misspelled(text) } else { Vec::new() };
    let mut cuts = vec![0, text.len()];
    for r in &wrong {
        cuts.extend([r.start, r.end]);
    }
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
    if job.sections.is_empty() {
        // An empty job takes egui's default font height; keep the row 20 tall.
        job.append("", 0., egui::TextFormat { font_id: theme::regular(14.), color: palette.text, line_height: Some(20.), ..Default::default() });
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
            // Desktop's `AnnouncementsOnlyGroupBanner`: only admins may send,
            // and "admins" opens the group's details, where they're listed.
            if c.group_state.announcements_only && !c.admins.iter().any(|a| *a == app.aci) {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        let line = ui.painter().layout_no_wrap("Only admins can send messages".into(), theme::regular(14.), palette.text_secondary).size().x;
                        ui.add_space(((ui.available_width() - line) / 2.).max(0.));
                        ui.spacing_mut().item_spacing.x = 0.;
                        ui.label(RichText::new("Only ").font(theme::regular(14.)).color(palette.text_secondary));
                        let admins = ui.add(egui::Label::new(RichText::new("admins").font(theme::semibold(14.)).color(palette.text_secondary).underline()).sense(Sense::click()));
                        ui.label(RichText::new(" can send messages").font(theme::regular(14.)).color(palette.text_secondary));
                        if admins.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            crate::details::open(app, c.id.clone());
                        }
                    });
                });
                return;
            }
            if chat.recording.is_some() {
                crate::voice::recording_bar(app, ui, palette, chat, c);
                return;
            }
            edit_bar(ui, palette, chat);
            reply_bar(app, ui, palette, chat);
            take_dropped_and_pasted(app, ui, chat);
            media_editor(ui, chat);
            if chat.staged.is_empty() {
                preview_card(ui, palette, chat);
            } else {
                staged_list(ui, palette, chat, c);
            }
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

/// Desktop's `Quote` in the composer (`quote--compose`): the chat's color
/// with its bar, the author, up to 2 lines and the thumbnail, and a close
/// button.
fn reply_bar(app: &App, ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    let Some(q) = chat.quote.and_then(|id| chat.messages.iter().find(|m| m.id == id)).cloned() else { return };
    let who = match &q.author {
        None => "You".to_owned(),
        Some(a) => app.name(a),
    };
    let text = if q.body.is_empty() { q.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_default() } else { crate::format::one_line(&q.body) };
    let thumb = q.attachments.iter().find(|a| !a.sticker && (a.content_type.starts_with("image/") || a.content_type.starts_with("video/")))
        .and_then(|a| if a.content_type.starts_with("image/") { a.local_path.clone() } else { a.thumbnail_path.clone() });
    let color = crate::chat_colors::for_chat(app, &chat.id);
    let text_color = if palette.dark { theme::rgb(0xe9e9e9) } else { theme::rgb(0x1b1b1b) };
    let width = ui.available_width();
    let side = if thumb.is_some() { 54. + 4. } else { 0. };
    let mut job = egui::text::LayoutJob::simple(text, theme::regular(13.), text_color, (width - 24. - 32. - side).max(40.));
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let height = (6. + 19. + galley.size().y + 7.).max(if thumb.is_some() { 54. } else { 0. });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = CornerRadius::same(8);
    crate::chat_colors::paint(ui.painter(), rect, radius, crate::chat_colors::quote_fill(color, palette.dark), ui.ctx().content_rect());
    let bar_color = match color {
        crate::chat_colors::Bubble::Solid(c) | crate::chat_colors::Bubble::Gradient(_, c, _) => c,
    };
    ui.painter().rect_filled(Rect::from_min_size(rect.min, Vec2::new(4., rect.height())), CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, bar_color);
    ui.painter().text(Pos2::new(rect.left() + 12., rect.top() + 6.), egui::Align2::LEFT_TOP, who, theme::semibold(13.), text_color);
    ui.painter().galley(Pos2::new(rect.left() + 12., rect.top() + 25.), galley, text_color);
    let close_at = Pos2::new(rect.right() - 16. - side, rect.top() + 16.);
    if let Some(path) = &thumb {
        let at = Rect::from_min_size(Pos2::new(rect.right() - 54., rect.top()), Vec2::splat(54.).min(Vec2::new(54., rect.height())));
        crate::chat::cover_image(ui, path, at, CornerRadius { ne: 8, se: 8, nw: 0, sw: 0 });
    }
    let close = ui.put(Rect::from_center_size(close_at, Vec2::splat(24.)), |ui: &mut Ui| widgets::icon_button(ui, Icon::XCompact, 16., "Cancel reply")).clicked();
    if close {
        chat.quote = None;
    }
    ui.add_space(6.);
}

/// Desktop's `StagedLinkPreview` above the input.
fn preview_card(ui: &mut Ui, palette: &Palette, chat: &mut ChatView) {
    if let Some(url) = chat.preview.loading().map(str::to_owned) {
        let mut close = false;
        egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.), Sense::hover());
            let color = if palette.dark { theme::rgb(0xbebebe) } else { theme::rgb(0x5e5e5e) };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Loading Preview...", theme::regular(14.), color);
            let x = Rect::from_center_size(Pos2::new(rect.right() - 12., rect.center().y), Vec2::splat(24.));
            close = ui.put(x, |ui: &mut Ui| widgets::icon_button(ui, Icon::XCompact, 16., "Close")).clicked();
        });
        if close {
            chat.preview.dismissed.insert(url);
            chat.preview.url = None;
        }
        ui.add_space(6.);
        return;
    }
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
    // Desktop's `:shortcode` completion, when no mention list is open.
    let mut emoji_list: Option<(std::ops::Range<usize>, Vec<&'static langis_emoji::Emoji>)> = None;
    if mention.is_none() && focused && selection.is_empty()
        && let Some(found) = langis_emoji::completion_at(&chat.composer, cursor)
    {
        let options = langis_emoji::SearchOptions { limit: 8, ..Default::default() };
        let hits: Vec<_> = langis_emoji::search(&found.query, &options).into_iter().map(|m| m.emoji).collect();
        if !hits.is_empty() {
            emoji_list = Some((found.range, hits));
        }
    }
    if mention.is_none() && emoji_list.is_none() {
        chat.mention_index = 0;
    }
    let mut picked_emoji = None;
    if let Some((_, hits)) = &emoji_list {
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
            )
        });
        let n = hits.len();
        if down {
            chat.mention_index = (chat.mention_index + 1) % n;
        }
        if up {
            chat.mention_index = (chat.mention_index + n - 1) % n;
        }
        chat.mention_index = chat.mention_index.min(n - 1);
        if enter {
            picked_emoji = Some(chat.mention_index);
        }
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

    // Desktop's CompositionArea row: emoji, the input, then the mic while
    // the input is empty (or the edit buttons), then the + menu. Buttons
    // sit 4px apart and 12px from the edges, the input 6px further in, all
    // aligned to the input's bottom.
    let mut input_rect = Rect::NOTHING;
    let height_id = id.with("row-height");
    let row_h: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(32.);
    // Buttons sit on the input's bottom edge, a top-aligned row padded by
    // the input's height last frame (a bottom-aligned row would stretch to
    // the panel).
    let at_bottom = |ui: &mut Ui, size: f32, add: &mut dyn FnMut(&mut Ui)| {
        ui.vertical(|ui| {
            ui.add_space((row_h - size - (32. - size) / 2.).max(0.));
            add(ui);
        });
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let editing = chat.editing.is_some();
        let dirty = !chat.composer.is_empty() || !chat.staged.is_empty();
        let right_w = if editing { 4. + 28. + 16. + 28. + 4. + 4. + 32. } else if dirty { 4. + 32. } else { 4. + 32. + 8. + 32. } ;
        let width = (ui.available_width() - (4. + 32. + 4.) - 12. - right_w).max(100.);
        ui.add_space(4.);
        let mut fun = None;
        at_bottom(ui, 32., &mut |ui| fun = Some(widgets::axo_icon_button(ui, "face-smiling", "Emoji, stickers and GIFs")));
        let fun = fun.unwrap();
        if fun.clicked() {
            chat.emoji_open = !chat.emoji_open;
        }
        ui.add_space(4. + 6.);
        let large = chat.composer_large;
        let spans = chat.spans.clone();
        let pal = *palette;
        let spell = app.input.spell;
        let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| layout(ui, text.as_str(), &spans, &pal, wrap, spell);
        let framed = egui::Frame::new()
            .fill(palette.bubble_in)
            .corner_radius(CornerRadius::same(18))
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(width - 24.);
                let max_h = if chat.composer_large { (ui.ctx().content_rect().height() * 0.45).max(120.) } else { 60. };
                egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(if chat.composer_large { max_h } else { 0. }).stick_to_bottom(true).show(ui, |ui| {
                    // The theme's 28px interact height would make one line
                    // taller than the input, and scroll it.
                    ui.spacing_mut().interact_size.y = 20.;
                    let output = egui::TextEdit::multiline(&mut chat.composer)
                            .hint_text(if editing { "Edit message" } else { "Message" })
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::ZERO)
                            .desired_rows(1)
                            .min_size(Vec2::new(0., 20.))
                            .desired_width(width - 24.)
                            .font(theme::regular(14.))
                            .layouter(&mut layouter)
                            .return_key(if large { egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Enter) } else { egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter) })
                            .id(id)
                            .show(ui);
                    (output.response.clone(), output.galley.clone(), output.galley_pos)
                })
                .inner
            });
        let (response, galley, galley_pos) = framed.inner;
        // Misspelled words get the browsers' red zigzag under them, drawn
        // here: the input's scroll area would clip an underline at its edge.
        if spell {
            let clip = framed.response.rect;
            let painter = ui.painter().with_clip_rect(clip);
            for range in crate::spell::misspelled(&chat.composer) {
                let (a, b) = (chat.composer[..range.start].chars().count(), chat.composer[..range.end].chars().count());
                let start = galley.pos_from_cursor(egui::text::CCursor::new(a));
                let end = galley.pos_from_cursor(egui::text::CCursor::new(b));
                if (start.top() - end.top()).abs() > 1. {
                    continue;
                }
                let y = galley_pos.y + start.bottom() - 2.;
                let (x0, x1) = (galley_pos.x + start.left(), galley_pos.x + end.left());
                let mut points = Vec::new();
                let mut x = x0;
                let mut up = true;
                while x < x1 {
                    points.push(Pos2::new(x, if up { y } else { y + 2. }));
                    x += 2.;
                    up = !up;
                }
                points.push(Pos2::new(x1, if up { y } else { y + 2. }));
                painter.add(egui::Shape::line(points, Stroke::new(1., palette.destructive)));
            }
        }
        // A right-click on a misspelled word: Chromium's suggestions, and
        // Add to dictionary.
        let spell_menu = egui::Id::unique("composer-spell-menu");
        if spell
            && response.secondary_clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let at = galley.cursor_from_pos(pointer - galley_pos).index.0;
            let byte = byte_of(&chat.composer, at);
            if let Some(range) = crate::spell::misspelled(&chat.composer).into_iter().find(|r| r.start <= byte && byte <= r.end) {
                let word = chat.composer[range.clone()].to_owned();
                ui.ctx().data_mut(|d| d.insert_temp(spell_menu, (range.start, range.end, word)));
                crate::menu::open_at(ui.ctx(), spell_menu, pointer);
            }
        }
        if let Some((start, end, word)) = ui.ctx().data(|d| d.get_temp::<(usize, usize, String)>(spell_menu)) {
            let mut replace: Option<String> = None;
            let mut add = false;
            let open = crate::menu::show(ui.ctx(), spell_menu, |m| {
                let suggestions = crate::spell::suggestions(&word);
                if suggestions.is_empty() {
                    m.label("No suggestions");
                }
                for s in suggestions {
                    if m.item(None, &s) {
                        replace = Some(s);
                    }
                }
                m.separator();
                if m.item(Some("plus"), "Add to dictionary") {
                    add = true;
                }
            });
            if let Some(with) = replace
                && chat.composer.get(start..end) == Some(word.as_str())
            {
                chat.composer.replace_range(start..end, &with);
                chat.spans.follow(&chat.composer);
            }
            if add {
                let saved = serde_json::to_string(&crate::spell::add_word(&word)).unwrap_or_default();
                app.load(move |db| async move {
                    let _ = db.set_setting(crate::spell::SETTING_SPELL_WORDS, &saved).await;
                    None
                });
            }
            if !open {
                ui.ctx().data_mut(|d| d.remove::<(usize, usize, String)>(spell_menu));
            }
        }
        input_rect = framed.response.rect;
        // Desktop's expand toggle: centered over the input once it's more
        // than a line, and while large.
        if chat.composer_large || input_rect.height() > 44. {
            let toggle = Rect::from_center_size(Pos2::new(input_rect.center().x, input_rect.top() - 14.), Vec2::new(48., 24.));
            let r = ui.interact(toggle, ui.scope_id().with("toggle-large"), Sense::click());
            ui.painter().rect_filled(toggle, CornerRadius::same(12), if r.hovered() { palette.fill_pressed } else { palette.fill });
            theme::paint_symbol(ui.painter(), if chat.composer_large { "chevron-down" } else { "chevron-up" }, toggle.center(), 16., palette.text_secondary);
            if r.on_hover_text(if chat.composer_large { "Make the composer smaller" } else { "Expand the composer" }).clicked() {
                chat.composer_large = !chat.composer_large;
            }
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::K)) {
            chat.composer_large = !chat.composer_large;
        }
        if (input_rect.height() - row_h).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(height_id, input_rect.height()));
            ui.ctx().request_repaint();
        }
        chat.spans.follow(&chat.composer);
        if response.changed() {
            chat.last_typed = Some(Instant::now());
            // Desktop's typingRefreshTimer: "typing" again every 10 seconds,
            // since other clients drop the indicator after 15 without one.
            let refresh = chat.typing_refreshed.is_none_or(|t| t.elapsed() >= TYPING_REFRESH);
            if (!chat.typing_sent || refresh) && !chat.composer.is_empty() && chat.editing.is_none() {
                chat.typing_sent = true;
                chat.typing_refreshed = Some(Instant::now());
                app.send(Command::Typing { conversation: c.id.clone(), started: true });
            }
        }
        // Typed emoticons like :-) turn into emoji, when that's on.
        if response.changed()
            && app.input.auto_convert
            && let Some(c) = egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range()).map(|r| byte_of(&chat.composer, r.primary.index.0))
            && let Some((range, emoji)) = langis_emoji::emoticon_replacement_at(&chat.composer, c)
        {
            let start_char = chat.composer[..range.start].chars().count();
            chat.composer.replace_range(range, emoji.glyph);
            chat.spans.follow(&chat.composer);
            set_cursor(ui.ctx(), id, start_char + emoji.glyph.chars().count());
        }
        // A complete `:shortcode:` turns into its emoji as it's typed.
        if response.changed()
            && let Some(c) = egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range()).map(|r| byte_of(&chat.composer, r.primary.index.0))
            && let Some((range, emoji)) = langis_emoji::shortcode_replacement_at(&chat.composer, c)
        {
            let start_char = chat.composer[..range.start].chars().count();
            chat.composer.replace_range(range, emoji.glyph);
            chat.spans.follow(&chat.composer);
            set_cursor(ui.ctx(), id, start_char + emoji.glyph.chars().count());
        }
        // In the large composer Enter is a new line and Ctrl+Enter sends
        // (Desktop's "Send (in expanded composer)").
        let send_key = if chat.composer_large { ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) } else { ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) };
        if response.has_focus() && picked_mention.is_none() && picked_emoji.is_none() && send_key {
            send = true;
        }
        if !response.has_focus() && app.safety.is_none() && app.lightbox.is_none() && chat.dialog.is_none() && ui.memory(|m| m.focused().is_none()) {
            response.request_focus();
        }
        ui.add_space(6. + 4.);
        if editing {
            // Desktop's discard and accept buttons while editing.
            let mut r = None;
            at_bottom(ui, 28., &mut |ui| r = Some(ui.allocate_exact_size(Vec2::splat(28.), Sense::click())));
            let (discard, r) = r.unwrap();
            ui.painter().circle_filled(discard.center(), 14., theme::rgb(0x848484));
            theme::paint_symbol(ui.painter(), "x", discard.center(), 20., Color32::WHITE);
            if r.on_hover_text("Discard edit").clicked() {
                chat.editing = None;
                chat.composer.clear();
                chat.spans.clear();
                chat.preview.reset();
            }
            ui.add_space(16.);
            let mut r = None;
            at_bottom(ui, 28., &mut |ui| r = Some(ui.allocate_exact_size(Vec2::splat(28.), Sense::click())));
            let (accept, r) = r.unwrap();
            ui.painter().circle_filled(accept.center(), 14., theme::rgb(0x3b45fd));
            theme::paint_symbol(ui.painter(), "check", accept.center(), 20., Color32::WHITE);
            if r.on_hover_text("Save").clicked() {
                send = true;
            }
            ui.add_space(4. + 4.);
        } else if !dirty {
            let mut mic = false;
            at_bottom(ui, 32., &mut |ui| mic = widgets::axo_icon_button(ui, "mic", "Voice message").clicked());
            if mic {
                crate::voice::start(app, chat);
            }
            ui.add_space(4. + 4.);
        }
        let mut plus = None;
        at_bottom(ui, 32., &mut |ui| plus = Some(widgets::axo_icon_button(ui, "plus", "Add attachment or poll")));
        let plus = plus.unwrap();
        ui.add_space(4.);
        attach_menu(app, ui, &plus, chat, c);
        if chat.emoji_open {
            let anchor = fun.rect.left_top();
            if let Some(picked) = crate::fun::show(app, ui.ctx(), anchor, chat, c) {
                insert_at_cursor(ui.ctx(), id, chat, picked);
            }
        }
    });

    if let Some((replace, hits)) = emoji_list {
        let mut chosen = picked_emoji.map(|i| hits[i.min(hits.len() - 1)]);
        let at = Pos2::new(input_rect.left(), input_rect.top() - 8. - hits.len() as f32 * 36. - 24.);
        crate::actions::popover(ui.ctx(), "emoji-completion", at, 260., Some(input_rect), |ui| {
            for (i, e) in hits.iter().enumerate() {
                let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.), Sense::click());
                if i == chat.mention_index || r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                }
                ui.painter().text(Pos2::new(rect.left() + 22., rect.center().y), Align2::CENTER_CENTER, e.glyph, theme::emoji(20.), palette.text);
                let code = e.shortcode().map(|s| format!(":{s}:")).unwrap_or_default();
                ui.painter().text(Pos2::new(rect.left() + 44., rect.center().y), Align2::LEFT_CENTER, code, theme::regular(14.), palette.text);
                if r.clicked() {
                    chosen = Some(*e);
                }
            }
        });
        if let Some(e) = chosen {
            let start_char = chat.composer[..replace.start].chars().count();
            chat.composer.replace_range(replace, e.glyph);
            chat.spans.follow(&chat.composer);
            set_cursor(ui.ctx(), id, start_char + e.glyph.chars().count());
            app.use_emoji(e.glyph);
        }
    }
    if let Some((replace, found)) = mention {
        let index = picked_mention.unwrap_or(usize::MAX);
        let mut chosen = (index != usize::MAX).then(|| found[index.min(found.len() - 1)].clone());
        let at = Pos2::new(input_rect.left(), input_rect.top() - 8. - found.len() as f32 * 40. - 24.);
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
    if app.input.formatting && focused && !selection.is_empty() && mention_query(&chat.composer, cursor).is_none() {
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
        chat.typing_refreshed = None;
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
#[cfg_attr(not(test), allow(dead_code))]
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

/// Desktop's + menu: photos and videos, a file, a poll. View-once media is
/// ours here until drafts can carry Desktop's view-once toggle.
fn attach_menu(app: &mut App, ui: &Ui, plus: &egui::Response, chat: &mut ChatView, _c: &langis_store::Conversation) {
    let mut choice = None;
    crate::menu::dropdown(ui, plus, egui::Id::unique("composer-add-menu"), |m| {
        if m.item(Some("photo"), "Photos & videos") {
            choice = Some(0);
        }
        if m.item(Some("file"), "File") {
            choice = Some(3);
        }
        if m.item(Some("poll"), "Poll") {
            choice = Some(1);
        }
    });
    match choice {
        Some(0) => attach(app, chat, true),
        Some(3) => attach(app, chat, false),
        Some(1) => chat.dialog = Some(crate::actions::Dialog::Poll(crate::polls::NewPoll::default())),
        _ => {}
    }
}

pub fn send_text(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    if !chat.staged.is_empty() && chat.editing.is_none() {
        send_staged(app, chat, c);
        return;
    }
    if chat.composer.trim().is_empty() {
        return;
    }
    let (body, mentions, styles) = chat.spans.to_send(&chat.composer);
    let preview = chat.preview.draft.take().filter(|p| langis_core::link_preview::is_valid(&body, &p.url));
    let command = match chat.editing.take() {
        Some(message_id) => Command::EditMessage { message_id, body, mentions, styles, preview },
        None => Command::SendText { conversation: c.id.clone(), body, quote: chat.quote.take(), preview, mentions, styles },
    };
    chat.composer.clear();
    chat.spans.clear();
    chat.preview.reset();
    chat.scroll_to_bottom = 10;
    chat.composer_large = false;
    if chat.typing_sent {
        chat.typing_sent = false;
        chat.typing_refreshed = None;
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
        // Desktop's Whoosh as a message goes, with in-chat sounds on.
        if app.sound.in_chat {
            crate::sounds::once(crate::sounds::Sound::Whoosh);
        }
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
}

/// A file waiting in the composer (Desktop's `AttachmentDraftType`).
pub struct Staged {
    pub bytes: std::sync::Arc<Vec<u8>>,
    pub name: String,
    pub kind: StagedKind,
    /// Its thumbnail's image uri, for photos.
    id: u64,
    /// The bytes as egui shares them, copied once rather than each frame.
    shared: std::sync::OnceLock<std::sync::Arc<[u8]>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StagedKind {
    /// Sent optimized, as a photo.
    Image,
    Video,
    /// GIFs and everything else go as they are.
    File,
}

impl Staged {
    pub fn new(bytes: Vec<u8>, name: String) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let ext = std::path::Path::new(&name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let kind = match ext.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "heic" | "avif" => StagedKind::Image,
            "mp4" | "mov" | "m4v" | "webm" | "mkv" => StagedKind::Video,
            _ => StagedKind::File,
        };
        Self { bytes: std::sync::Arc::new(bytes), name, kind, id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), shared: Default::default() }
    }

    fn image_bytes(&self) -> egui::load::Bytes {
        egui::load::Bytes::Shared(self.shared.get_or_init(|| self.bytes.as_slice().into()).clone())
    }

    fn uri(&self) -> String {
        format!("bytes://staged/{}/{}", self.id, self.name)
    }
}

/// Desktop's limit on attachments in one message.
const MAX_STAGED: usize = 32;

/// Picks files to add to the message.
/// Ctrl+U: Desktop's Attach file.
pub fn attach_file(app: &mut App, chat: &mut ChatView) {
    attach(app, chat, false);
}

/// Ctrl+P: Desktop's Remove draft link preview.
pub fn remove_preview(chat: &mut ChatView) {
    if let Some(url) = chat.preview.url.clone() {
        chat.preview.dismissed.insert(url);
    }
    chat.preview.url = None;
    chat.preview.draft = None;
}

fn attach(app: &mut App, chat: &mut ChatView, media: bool) {
    let mut dialog = rfd::FileDialog::new().set_title("Add attachment");
    if media {
        dialog = dialog.add_filter("Photos & videos", &["jpg", "jpeg", "png", "gif", "webp", "heic", "avif", "mp4", "mov", "webm", "mkv", "m4v"]);
    }
    let Some(paths) = dialog.pick_files() else { return };
    for path in paths {
        stage_path(app, chat, &path);
    }
}

/// Signal's attachment limit (Desktop's `getMaximumOutgoingAttachmentSizeInKb`, 100 MB).
const MAX_FILE_BYTES: u64 = 100 * 1000 * 1000;

fn stage_path(app: &mut App, chat: &mut ChatView, path: &std::path::Path) {
    if chat.staged.len() >= MAX_STAGED {
        app.toast(format!("You can add up to {MAX_STAGED} attachments to a message."));
        return;
    }
    // Only regular files, within what Signal sends: a dropped `/dev/zero`
    // or a FIFO would otherwise read forever.
    match std::fs::metadata(path) {
        Ok(meta) if !meta.is_file() => {
            app.toast(format!("{} isn't a file.", path.display()));
            return;
        }
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            app.toast("Can’t send this file because it’s too large. You can send files up to 100MB.");
            return;
        }
        Err(error) => {
            app.toast(format!("Couldn't read {}: {error}", path.display()));
            return;
        }
        Ok(_) => {}
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
            chat.staged.push(Staged::new(bytes, name));
        }
        Err(error) => app.toast(format!("Couldn't read {}: {error}", path.display())),
    }
}

/// The media editor while it's open; Done puts the edited picture in the
/// staged one's place, as a PNG.
fn media_editor(ui: &Ui, chat: &mut ChatView) {
    let Some(editor) = chat.media_editor.as_mut() else { return };
    let Some(result) = editor.show(ui.ctx()) else { return };
    let index = editor.index;
    chat.media_editor = None;
    if let (Some(png), Some(staged)) = (result, chat.staged.get_mut(index)) {
        let stem = std::path::Path::new(&staged.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
        *staged = Staged::new(png, format!("{stem}.png"));
    }
}

/// Files dropped on the window, and an image pasted with Ctrl+V, go to
/// the list (Desktop's drop zone and paste handler).
fn take_dropped_and_pasted(app: &mut App, ui: &Ui, chat: &mut ChatView) {
    let dropped = ui.input(|i| i.raw.dropped_files.clone());
    // Wayland's drops, from the app's own listener.
    for path in app.drops.as_ref().map(|d| d.take()).unwrap_or_default() {
        stage_path(app, chat, &path);
    }
    for file in dropped {
        let path = file.path();
        if path.exists() {
            stage_path(app, chat, path);
        } else if let Ok(bytes) = file.bytes() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
            chat.staged.push(Staged::new(bytes, name));
        }
    }
    // egui turns a paste into text; when the clipboard holds an image and
    // no text, the paste comes as an empty one (or none at all).
    let pasted = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(t) if t.is_empty())) || (i.modifiers.command && i.key_pressed(egui::Key::V)));
    if pasted
        && let Ok(mut clipboard) = arboard::Clipboard::new()
        && clipboard.get_text().map(|t| t.is_empty()).unwrap_or(true)
        && let Ok(image) = clipboard.get_image()
    {
        let rgba = image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes.into_owned());
        let mut png = std::io::Cursor::new(Vec::new());
        if let Some(rgba) = rgba
            && rgba.write_to(&mut png, image::ImageFormat::Png).is_ok()
        {
            let name = format!("signal-{}.png", chrono::Local::now().format("%Y-%m-%d-%H%M%S"));
            chat.staged.push(Staged::new(png.into_inner(), name));
        }
    }
}

/// Desktop's `AttachmentList` above the input: each file's tile with a
/// remove button, a tile to add more, and the view-once toggle when it's
/// one photo or video.
fn staged_list(ui: &mut Ui, palette: &Palette, chat: &mut ChatView, c: &langis_store::Conversation) {
    let tile = 64.;
    let mut remove = None;
    let mut edit = None;
    let mut add = false;
    egui::Frame::new().fill(palette.bubble_in).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            egui::ScrollArea::horizontal().id_salt("staged-attachments").max_width(ui.available_width() - 48.).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.;
                    for (i, s) in chat.staged.iter().enumerate() {
                        let (rect, tile_response) = ui.allocate_exact_size(Vec2::splat(tile), Sense::click());
                        // A photo opens Desktop's media editor.
                        if s.kind == StagedKind::Image && tile_response.on_hover_text("Edit").clicked() {
                            edit = Some(i);
                        }
                        ui.painter().rect_filled(rect, CornerRadius::same(8), palette.fill);
                        match s.kind {
                            StagedKind::Image => {
                                egui::Image::new(egui::ImageSource::Bytes { uri: s.uri().into(), bytes: s.image_bytes() })
                                    .corner_radius(CornerRadius::same(8))
                                    .fit_to_exact_size(rect.size())
                                    .paint_at(ui, rect);
                            }
                            kind => {
                                let symbol = if kind == StagedKind::Video { "videocamera" } else { "file" };
                                theme::paint_symbol(ui.painter(), symbol, Pos2::new(rect.center().x, rect.top() + 24.), 22., palette.text_secondary);
                                let mut job = egui::text::LayoutJob::simple_singleline(s.name.clone(), theme::regular(10.), palette.text_secondary);
                                job.wrap = egui::text::TextWrapping::truncate_at_width(tile - 8.);
                                let g = ui.fonts_mut(|f| f.layout_job(job));
                                ui.painter().galley(Pos2::new(rect.center().x - g.size().x / 2., rect.bottom() - 18.), g, palette.text_secondary);
                            }
                        }
                        let x = Rect::from_center_size(rect.right_top() + Vec2::new(-10., 10.), Vec2::splat(18.));
                        let xr = ui.interact(x, ui.scope_id().with(("unstage", s.id)), Sense::click());
                        ui.painter().circle_filled(x.center(), 9., Color32::from_black_alpha(if xr.hovered() { 200 } else { 150 }));
                        theme::paint_symbol(ui.painter(), "x", x.center(), 12., Color32::WHITE);
                        if xr.on_hover_text("Remove attachment").clicked() {
                            remove = Some(i);
                        }
                    }
                    // Desktop's add-more tile.
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(tile), Sense::click());
                    ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(1., palette.border), egui::StrokeKind::Inside);
                    theme::paint_symbol(ui.painter(), "plus", rect.center(), 22., palette.text_secondary);
                    if r.on_hover_text("Add attachment").clicked() {
                        add = true;
                    }
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // View once: for one photo or video, not to ourselves.
                let one_media = chat.staged.len() == 1 && chat.staged[0].kind != StagedKind::File;
                if one_media && c.kind != ConversationKind::NoteToSelf {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                    if chat.view_once {
                        ui.painter().circle_filled(rect.center(), 16., palette.accent);
                    } else if r.hovered() {
                        ui.painter().circle_filled(rect.center(), 16., palette.fill_pressed);
                    }
                    theme::paint_symbol(ui.painter(), "viewonce", rect.center(), 20., if chat.view_once { Color32::WHITE } else { palette.text });
                    if r.on_hover_text(if chat.view_once { "View once: on" } else { "View once: off" }).clicked() {
                        chat.view_once = !chat.view_once;
                    }
                } else {
                    chat.view_once = false;
                }
            });
        });
    });
    if let Some(i) = remove {
        chat.staged.remove(i);
    } else if let Some(i) = edit {
        match crate::media_editor::Editor::open(i, &chat.staged[i].bytes) {
            Some(editor) => chat.media_editor = Some(editor),
            None => tracing::warn!("couldn't open a staged photo to edit"),
        }
    }
    if add {
        let Some(paths) = rfd::FileDialog::new().set_title("Add attachment").pick_files() else { return };
        for path in paths {
            if chat.staged.len() < MAX_STAGED
                && let Ok(bytes) = std::fs::read(&path)
            {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
                chat.staged.push(Staged::new(bytes, name));
            }
        }
    }
    ui.add_space(6.);
}

/// Sends what's staged with the composer's text as its caption: photos
/// together, other files each in their own message (the caption with the
/// first), or one view-once photo or video.
fn send_staged(app: &mut App, chat: &mut ChatView, c: &langis_store::Conversation) {
    let staged = std::mem::take(&mut chat.staged);
    let view_once = std::mem::take(&mut chat.view_once);
    let (caption, mentions, styles) = chat.spans.to_send(&chat.composer);
    chat.composer.clear();
    chat.spans.clear();
    chat.preview.reset();
    chat.composer_large = false;
    chat.scroll_to_bottom = 10;
    let quote = chat.quote.take();
    if view_once && staged.len() == 1 {
        let s = staged.into_iter().next().expect("one staged file");
        let media = match s.kind {
            StagedKind::Video => langis_core::OutgoingViewOnce::Video(langis_core::OutgoingFile { bytes: s.bytes, file_name: s.name, voice: false }),
            _ => langis_core::OutgoingViewOnce::Image(langis_core::OutgoingImage { bytes: s.bytes, file_name: Some(s.name) }),
        };
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(Command::SendViewOnce { conversation: c.id.clone(), media })));
        return;
    }
    let (images, files): (Vec<Staged>, Vec<Staged>) = staged.into_iter().partition(|s| s.kind == StagedKind::Image);
    let mut caption_left = Some((caption, mentions, styles));
    if !images.is_empty() {
        let (caption, mentions, styles) = caption_left.take().unwrap_or_default();
        let images = images.into_iter().map(|s| langis_core::OutgoingImage { bytes: s.bytes, file_name: Some(s.name) }).collect();
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(Command::SendImages { conversation: c.id.clone(), images, caption, quote, mentions, styles })));
    }
    for s in files {
        let (caption, mentions, styles) = caption_left.take().unwrap_or_default();
        let command = Command::SendFile {
            conversation: c.id.clone(),
            file: langis_core::OutgoingFile { bytes: s.bytes, file_name: s.name, voice: false },
            caption,
            quote,
            mentions,
            styles,
        };
        app.send_checked(c.id.clone(), PendingSend::Command(Box::new(command)));
    }
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
