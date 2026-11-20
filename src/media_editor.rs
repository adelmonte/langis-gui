//! Desktop's `MediaEditor` for a photo waiting in the composer: crop
//! (freeform, square or vertical), rotate and flip; draw with a pen or a
//! highlighter in four widths; add text in three styles; colors from its
//! slider; undo and redo. Done bakes the edits into the picture (strokes
//! and text drawn with `ab_glyph` in Inter) and replaces the staged file.

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use image::RgbaImage;

use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Crop,
    Draw,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CropPreset {
    Freeform,
    Square,
    Vertical,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Regular,
    Highlight,
    Outline,
}

/// Desktop's `DrawWidth`, in pixels of a 1000-pixel-wide picture.
const WIDTHS: [(f32, &str); 4] = [(2., "Thin"), (4., "Regular"), (12., "Medium"), (24., "Heavy")];

#[derive(Clone)]
struct Line {
    /// Points in the turned picture, 0 to 1 across and down.
    points: Vec<Pos2>,
    color: Color32,
    /// Width as a share of the picture's width.
    width: f32,
    highlighter: bool,
}

#[derive(Clone)]
struct Label {
    text: String,
    /// Its center in the turned picture, 0 to 1.
    at: Pos2,
    color: Color32,
    style: TextStyle,
    /// Its size as a share of the picture's width.
    size: f32,
}

/// A sticker placed on the picture (Desktop's `MediaEditor` sticker).
#[derive(Clone)]
struct Placed {
    path: std::path::PathBuf,
    /// Its center in the turned picture, 0 to 1.
    at: Pos2,
    /// Its width as a share of the picture's width.
    size: f32,
}

/// What undo goes back to.
#[derive(Clone)]
struct Edits {
    stickers: Vec<Placed>,
    turns: u8,
    flip: bool,
    /// The crop, 0 to 1 in the turned picture.
    crop: Rect,
    lines: Vec<Line>,
    labels: Vec<Label>,
}

pub struct Editor {
    /// Which staged file this edits.
    pub index: usize,
    original: RgbaImage,
    /// The original turned and flipped, and its preview texture.
    turned: RgbaImage,
    texture: Option<TextureHandle>,
    edits: Edits,
    undo: Vec<Edits>,
    redo: Vec<Edits>,
    mode: Mode,
    preset: CropPreset,
    /// Desktop's color slider, 0 to 100.
    color: f32,
    width: usize,
    highlighter: bool,
    text_style: TextStyle,
    /// The label being typed, by index.
    typing: Option<usize>,
    /// What the pointer holds: a crop handle, or a label being moved.
    grab: Option<Grab>,
    /// Desktop's sticker picker over the bar, open.
    sticker_picker: bool,
    /// The sticker last touched, with its remove button.
    sticker_selected: Option<usize>,
    /// Desktop's caption field ("Message") beside Save, when the caller
    /// gives one (the composer's draft); read it back after Done.
    pub caption: Option<String>,
    caption_focused: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Grab {
    Corner(u8),
    Crop(Vec2),
    Label(usize, Vec2),
    Sticker(usize, Vec2),
}

/// Desktop's `getHSL` for its color slider: black, the hues, then white.
pub fn slider_color(percentage: f32) -> Color32 {
    let (h, s, l) = if percentage <= 7. {
        (0., 0.5, 0.5 * percentage / 7.)
    } else if percentage >= 95. {
        (0., 0., (0.5 + (percentage - 95.) / 5.).min(1.))
    } else {
        (338. * (percentage - 7.) / 88., 1., 0.5)
    };
    let c = (1. - (2. * l - 1f32).abs()) * s;
    let x = c * (1. - ((h / 60.) % 2. - 1f32).abs());
    let m = l - c / 2.;
    let (r, g, b) = match (h / 60.) as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let to = |v: f32| ((v + m) * 255.).round().clamp(0., 255.) as u8;
    Color32::from_rgb(to(r), to(g), to(b))
}

fn turn(image: &RgbaImage, turns: u8, flip: bool) -> RgbaImage {
    let mut out = match turns % 4 {
        1 => image::imageops::rotate90(image),
        2 => image::imageops::rotate180(image),
        3 => image::imageops::rotate270(image),
        _ => image.clone(),
    };
    if flip {
        out = image::imageops::flip_horizontal(&out);
    }
    out
}

impl Editor {
    /// Opens the editor on a staged photo; `None` if it can't be read.
    pub fn open(index: usize, bytes: &[u8]) -> Option<Self> {
        let original = image::load_from_memory(bytes).ok()?.to_rgba8();
        let turned = original.clone();
        let edits = Edits { stickers: Vec::new(), turns: 0, flip: false, crop: Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)), lines: Vec::new(), labels: Vec::new() };
        Some(Self {
            index,
            original,
            turned,
            texture: None,
            edits,
            undo: Vec::new(),
            redo: Vec::new(),
            mode: Mode::Draw,
            preset: CropPreset::Freeform,
            color: 50.,
            width: 1,
            highlighter: false,
            text_style: TextStyle::Regular,
            typing: None,
            grab: None,
            sticker_picker: false,
            sticker_selected: None,
            caption: None,
            caption_focused: false,
        })
    }

    fn undo_step(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let turned_changed = (prev.turns, prev.flip) != (self.edits.turns, self.edits.flip);
            self.redo.push(std::mem::replace(&mut self.edits, prev));
            self.typing = None;
            self.sticker_selected = None;
            if turned_changed {
                self.retexture();
            }
        }
    }

    fn redo_step(&mut self) {
        if let Some(next) = self.redo.pop() {
            let turned_changed = (next.turns, next.flip) != (self.edits.turns, self.edits.flip);
            self.undo.push(std::mem::replace(&mut self.edits, next));
            if turned_changed {
                self.retexture();
            }
        }
    }

    /// Desktop's MediaEditor keys: Ctrl+Z, Ctrl+Shift+Z, Ctrl+D (draw),
    /// Ctrl+T (text), Ctrl+C (crop), and Delete for what's selected.
    fn keys(&mut self, ctx: &egui::Context) {
        let typing = (self.typing.is_some() && ctx.memory(|m| m.focused().is_some())) || self.caption_focused;
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
        if cmd && pressed(egui::Key::Z) {
            if shift {
                self.redo_step();
            } else {
                self.undo_step();
            }
        }
        if typing {
            return;
        }
        if cmd && pressed(egui::Key::D) {
            self.mode = Mode::Draw;
        }
        if cmd && pressed(egui::Key::T) {
            self.mode = Mode::Text;
        }
        if cmd && pressed(egui::Key::C) {
            self.mode = Mode::Crop;
            self.typing = None;
        }
        if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
            if let Some(i) = self.sticker_selected.take().filter(|i| *i < self.edits.stickers.len()) {
                self.remember();
                self.edits.stickers.remove(i);
            } else if let Some(i) = self.typing.take().filter(|i| *i < self.edits.labels.len()) {
                self.remember();
                self.edits.labels.remove(i);
            }
        }
    }

    fn remember(&mut self) {
        self.undo.push(self.edits.clone());
        self.redo.clear();
    }

    fn retexture(&mut self) {
        self.turned = turn(&self.original, self.edits.turns, self.edits.flip);
        self.texture = None;
    }

    /// Turns everything a quarter clockwise, drawings and crop with it.
    fn rotate(&mut self) {
        self.remember();
        let r = |p: Pos2| Pos2::new(1. - p.y, p.x);
        for line in &mut self.edits.lines {
            line.points.iter_mut().for_each(|p| *p = r(*p));
        }
        for label in &mut self.edits.labels {
            label.at = r(label.at);
        }
        for st in &mut self.edits.stickers {
            st.at = r(st.at);
        }
        let c = self.edits.crop;
        self.edits.crop = Rect::from_two_pos(r(c.min), r(c.max));
        // A flip turned a quarter is a flip the other way round.
        self.edits.turns = if self.edits.flip { (self.edits.turns + 3) % 4 } else { (self.edits.turns + 1) % 4 };
        self.retexture();
    }

    fn flip(&mut self) {
        self.remember();
        let f = |p: Pos2| Pos2::new(1. - p.x, p.y);
        for line in &mut self.edits.lines {
            line.points.iter_mut().for_each(|p| *p = f(*p));
        }
        for label in &mut self.edits.labels {
            label.at = f(label.at);
        }
        for st in &mut self.edits.stickers {
            st.at = f(st.at);
        }
        let c = self.edits.crop;
        self.edits.crop = Rect::from_two_pos(f(c.min), f(c.max));
        self.edits.flip = !self.edits.flip;
        self.retexture();
    }

    /// The crop fitted to a preset's shape, centered on the current one.
    fn apply_preset(&mut self, preset: CropPreset) {
        self.preset = preset;
        let aspect = self.turned.width() as f32 / self.turned.height().max(1) as f32;
        let want = match preset {
            CropPreset::Freeform => return,
            CropPreset::Square => 1.,
            CropPreset::Vertical => 9. / 16.,
        };
        self.remember();
        // In 0-1 units a w:h of `want` in pixels is (want / aspect) : 1.
        let ratio = want / aspect;
        let (w, h) = if ratio <= 1. { (ratio, 1.) } else { (1., 1. / ratio) };
        let center = self.edits.crop.center();
        let mut rect = Rect::from_center_size(center, Vec2::new(w, h));
        rect = rect.translate(Vec2::new((-rect.min.x).max(0.) - (rect.max.x - 1.).max(0.), (-rect.min.y).max(0.) - (rect.max.y - 1.).max(0.)));
        self.edits.crop = rect;
    }

    /// Draws the editor in a full-window modal; returns the edited picture
    /// as a PNG when Done is pressed, `Some(None)` when cancelled.
    /// Shows the editor; `stickers` are the installed packs' (for its
    /// sticker button). Some(Some(png)) on Done, Some(None) on Cancel.
    pub fn show(&mut self, ctx: &egui::Context, stickers: &[std::sync::Arc<langis_core::StickerPackView>]) -> Option<Option<Vec<u8>>> {
        let palette = Palette::of(ctx);
        let screen = ctx.content_rect();
        let mut result = None;
        if self.texture.is_none() {
            // A preview at most 2048 across.
            let (w, h) = self.turned.dimensions();
            let scale = (2048. / w.max(h) as f32).min(1.);
            let preview = if scale < 1. {
                image::imageops::resize(&self.turned, (w as f32 * scale) as u32, (h as f32 * scale) as u32, image::imageops::FilterType::Triangle)
            } else {
                self.turned.clone()
            };
            let image = egui::ColorImage::from_rgba_unmultiplied([preview.width() as usize, preview.height() as usize], preview.as_raw());
            self.texture = Some(ctx.load_texture("media-editor", image, TextureOptions::LINEAR));
        }
        self.keys(ctx);
        egui::Area::new(egui::Id::unique("media-editor")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0, Color32::from_gray(18));
            let toolbar = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 72.), rect.max);
            let area = Rect::from_min_max(rect.min + Vec2::new(40., 40.), Pos2::new(rect.right() - 40., toolbar.top() - 16.));
            self.canvas(ui, area);
            if let Some(r) = self.toolbar(ui, toolbar, &palette) {
                result = Some(r);
            }
            if self.sticker_picker {
                self.pick_sticker(ui, toolbar, stickers);
            }
        });
        if self.sticker_picker && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.sticker_picker = false;
        } else if result.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.typing.is_none() {
            result = Some(None);
        }
        result
    }

    /// The picture with its edits; in crop mode, the whole picture with the
    /// crop's frame and handles.
    fn canvas(&mut self, ui: &mut Ui, area: Rect) {
        let Some(texture) = self.texture.clone() else { return };
        let (tw, th) = (self.turned.width() as f32, self.turned.height() as f32);
        let crop = if self.mode == Mode::Crop { Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)) } else { self.edits.crop };
        let shown = Vec2::new(tw * crop.width(), th * crop.height());
        let scale = (area.width() / shown.x).min(area.height() / shown.y);
        let view = Rect::from_center_size(area.center(), shown * scale);
        egui::Image::new((texture.id(), view.size())).uv(crop).paint_at(ui, view);
        // Picture (0-1) to screen and back.
        let to_screen = |p: Pos2| Pos2::new(view.left() + (p.x - crop.min.x) / crop.width() * view.width(), view.top() + (p.y - crop.min.y) / crop.height() * view.height());
        let to_picture = |s: Pos2| Pos2::new(crop.min.x + (s.x - view.left()) / view.width() * crop.width(), crop.min.y + (s.y - view.top()) / view.height() * crop.height());
        let painter = ui.painter().with_clip_rect(view);
        let picture_w = view.width() / crop.width();
        for line in &self.edits.lines {
            let color = if line.highlighter { line.color.gamma_multiply(0.5) } else { line.color };
            let points: Vec<Pos2> = line.points.iter().map(|p| to_screen(*p)).collect();
            painter.add(egui::Shape::line(points, Stroke::new(line.width * picture_w, color)));
        }
        for (i, label) in self.edits.labels.iter().enumerate() {
            let size = label.size * picture_w;
            let galley = painter.layout_no_wrap(if label.text.is_empty() { " ".into() } else { label.text.clone() }, theme::semibold(size.max(4.)), label.color);
            let box_rect = Rect::from_center_size(to_screen(label.at), galley.size() + Vec2::splat(size * 0.3));
            match label.style {
                TextStyle::Highlight => {
                    painter.rect_filled(box_rect, CornerRadius::same((size * 0.2) as u8), label.color);
                    let ink = if label.color.r() as u32 + label.color.g() as u32 + label.color.b() as u32 > 600 { Color32::BLACK } else { Color32::WHITE };
                    painter.galley_with_override_text_color(box_rect.center() - galley.size() / 2., galley, ink);
                }
                TextStyle::Outline => {
                    let outline = if label.color == Color32::WHITE { Color32::BLACK } else { Color32::WHITE };
                    let pos = box_rect.center() - galley.size() / 2.;
                    for d in [Vec2::new(-1., 0.), Vec2::new(1., 0.), Vec2::new(0., -1.), Vec2::new(0., 1.)] {
                        painter.galley_with_override_text_color(pos + d * (size * 0.06).max(1.), galley.clone(), outline);
                    }
                    painter.galley(pos, galley, label.color);
                }
                TextStyle::Regular => painter.galley(box_rect.center() - galley.size() / 2., galley, label.color),
            }
            if self.typing == Some(i) {
                painter.rect_stroke(box_rect.expand(3.), CornerRadius::same(4), Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Outside);
            }
        }
        // Stickers, over the drawing and text.
        let sticker_rects: Vec<Rect> = self
            .edits
            .stickers
            .iter()
            .map(|st| {
                let w = st.size * picture_w;
                Rect::from_center_size(to_screen(st.at), Vec2::splat(w))
            })
            .collect();
        for (st, r) in self.edits.stickers.iter().zip(&sticker_rects) {
            let file = st.path.to_string_lossy();
            egui::Image::new(crate::thumbs::uri(ui.ctx(), &file, r.size())).maintain_aspect_ratio(true).fit_to_exact_size(r.size()).paint_at(&ui.new_child(egui::UiBuilder::new().max_rect(view)), *r);
        }
        let response = ui.interact(view, ui.scope_id().with("media-editor-canvas"), Sense::click_and_drag());
        let pointer = response.interact_pointer_pos();
        // The selected sticker: its frame, and an X to take it off; the
        // scroll wheel over it sizes it.
        if self.mode != Mode::Crop
            && let Some(i) = self.sticker_selected.filter(|i| *i < sticker_rects.len())
        {
            let r = sticker_rects[i];
            ui.painter().rect_stroke(r.expand(2.), CornerRadius::same(4), Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Outside);
            let x = Rect::from_center_size(r.right_top(), Vec2::splat(22.));
            let hit = ui.interact(x, ui.scope_id().with(("sticker-x", i)), Sense::click());
            ui.painter().circle_filled(x.center(), 11., Color32::from_gray(if hit.hovered() { 80 } else { 50 }));
            theme::paint_symbol(ui.painter(), "x", x.center(), 14., Color32::WHITE);
            if hit.on_hover_text("Remove").clicked() {
                self.remember();
                self.edits.stickers.remove(i);
                self.sticker_selected = None;
                return;
            }
            let scroll = ui.input(|inp| inp.smooth_scroll_delta.y);
            if scroll != 0. && ui.rect_contains_pointer(r) {
                let st = &mut self.edits.stickers[i];
                st.size = (st.size * (1. + scroll / 400.)).clamp(0.05, 0.9);
            }
        }
        if self.mode != Mode::Crop {
            if response.drag_started()
                && let Some(p) = pointer
                && let Some(i) = sticker_rects.iter().rposition(|r| r.contains(p))
            {
                self.remember();
                self.grab = Some(Grab::Sticker(i, sticker_rects[i].center() - p));
                self.sticker_selected = Some(i);
            }
            if let (Some(Grab::Sticker(i, offset)), Some(p)) = (self.grab, pointer) {
                if response.dragged() {
                    self.edits.stickers[i].at = to_picture(p + offset);
                }
                if response.drag_stopped() {
                    self.grab = None;
                }
                return;
            }
            if response.clicked()
                && let Some(p) = pointer
            {
                if let Some(i) = sticker_rects.iter().rposition(|r| r.contains(p)) {
                    self.sticker_selected = Some(i);
                    return;
                }
                self.sticker_selected = None;
            }
        }
        match self.mode {
            Mode::Draw => {
                if response.drag_started()
                    && let Some(p) = pointer
                {
                    self.remember();
                    let width = WIDTHS[self.width].0 / 1000.;
                    self.edits.lines.push(Line { points: vec![to_picture(p)], color: slider_color(self.color), width, highlighter: self.highlighter });
                }
                if response.dragged()
                    && let (Some(p), Some(line)) = (pointer, self.edits.lines.last_mut())
                {
                    line.points.push(to_picture(p));
                }
            }
            Mode::Text => {
                // A label under the pointer moves; a click elsewhere starts one.
                let hit = |editor: &Self, s: Pos2| {
                    editor.edits.labels.iter().rposition(|l| {
                        let size = l.size * picture_w;
                        (to_screen(l.at) - s).length() < (size * l.text.chars().count().max(2) as f32 * 0.35).max(size)
                    })
                };
                if response.drag_started()
                    && let Some(p) = pointer
                    && let Some(i) = hit(self, p)
                {
                    self.remember();
                    self.grab = Some(Grab::Label(i, to_screen(self.edits.labels[i].at) - p));
                    self.typing = Some(i);
                }
                if let (Some(Grab::Label(i, offset)), Some(p)) = (self.grab, pointer)
                    && response.dragged()
                {
                    self.edits.labels[i].at = to_picture(p + offset);
                }
                if response.clicked()
                    && let Some(p) = pointer
                {
                    match hit(self, p) {
                        Some(i) => self.typing = Some(i),
                        None => {
                            self.remember();
                            self.edits.labels.push(Label { text: String::new(), at: to_picture(p), color: slider_color(self.color), style: self.text_style, size: 0.06 });
                            self.typing = Some(self.edits.labels.len() - 1);
                        }
                    }
                }
            }
            Mode::Crop => {
                let frame = Rect::from_min_max(to_screen(self.edits.crop.min), to_screen(self.edits.crop.max));
                // Shade outside the crop, its frame, and corner handles.
                for outside in [
                    Rect::from_min_max(view.min, Pos2::new(view.right(), frame.top())),
                    Rect::from_min_max(Pos2::new(view.left(), frame.bottom()), view.max),
                    Rect::from_min_max(Pos2::new(view.left(), frame.top()), Pos2::new(frame.left(), frame.bottom())),
                    Rect::from_min_max(Pos2::new(frame.right(), frame.top()), Pos2::new(view.right(), frame.bottom())),
                ] {
                    ui.painter().rect_filled(outside, 0, Color32::from_black_alpha(150));
                }
                ui.painter().rect_stroke(frame, 0, Stroke::new(2., Color32::WHITE), egui::StrokeKind::Inside);
                let corners = [frame.left_top(), frame.right_top(), frame.left_bottom(), frame.right_bottom()];
                for c in corners {
                    ui.painter().circle_filled(c, 7., Color32::WHITE);
                }
                if response.drag_started()
                    && let Some(p) = pointer
                {
                    self.remember();
                    self.grab = match corners.iter().position(|c| (*c - p).length() < 18.) {
                        Some(k) => Some(Grab::Corner(k as u8)),
                        None if frame.contains(p) => Some(Grab::Crop(frame.min - p)),
                        None => None,
                    };
                }
                if response.dragged()
                    && let Some(p) = pointer
                {
                    let q = to_picture(p);
                    let mut c = self.edits.crop;
                    match self.grab {
                        Some(Grab::Corner(k)) => {
                            let fixed = match k {
                                0 => c.right_bottom(),
                                1 => c.left_bottom(),
                                2 => c.right_top(),
                                _ => c.left_top(),
                            };
                            let mut moving = Pos2::new(q.x.clamp(0., 1.), q.y.clamp(0., 1.));
                            // Presets keep their shape: the height follows the width.
                            if self.preset != CropPreset::Freeform {
                                let aspect = self.turned.width() as f32 / self.turned.height().max(1) as f32;
                                let want = if self.preset == CropPreset::Square { 1. } else { 9. / 16. };
                                let w = (moving.x - fixed.x).abs();
                                let h = w * aspect / want;
                                moving.y = (fixed.y + h * if moving.y >= fixed.y { 1. } else { -1. }).clamp(0., 1.);
                            }
                            let r = Rect::from_two_pos(fixed, moving);
                            if r.width() > 0.05 && r.height() > 0.05 {
                                c = r;
                            }
                        }
                        Some(Grab::Crop(offset)) => {
                            let min = to_picture(p + offset);
                            let size = c.size();
                            let min = Pos2::new(min.x.clamp(0., 1. - size.x), min.y.clamp(0., 1. - size.y));
                            c = Rect::from_min_size(min, size);
                        }
                        _ => {}
                    }
                    self.edits.crop = c;
                }
            }
        }
        if response.drag_stopped() {
            self.grab = None;
        }
    }

    /// Desktop's bottom bar: the tools, each tool's options, Undo, Redo,
    /// Cancel and Done.
    fn toolbar(&mut self, ui: &mut Ui, bar: Rect, palette: &Palette) -> Option<Option<Vec<u8>>> {
        let mut result = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(24., 14.))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                let pill = |ui: &mut Ui, label: &str, on: bool| {
                    let fill = if on { palette.accent } else { Color32::from_gray(60) };
                    ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE).font(theme::medium(13.))).fill(fill).corner_radius(CornerRadius::same(16))).clicked()
                };
                for (mode, label) in [(Mode::Draw, tr("Draw")), (Mode::Text, tr("Text")), (Mode::Crop, "Crop")] {
                    if pill(ui, label, self.mode == mode) {
                        self.mode = mode;
                        self.typing = None;
                    }
                }
                if pill(ui, tr("Sticker"), self.sticker_picker) {
                    self.sticker_picker = !self.sticker_picker;
                }
                ui.add_space(12.);
                match self.mode {
                    Mode::Draw => {
                        if pill(ui, if self.highlighter { tr("Highlighter") } else { tr("Pen") }, false) {
                            self.highlighter = !self.highlighter;
                        }
                        if pill(ui, crate::i18n::tr(WIDTHS[self.width].1), false) {
                            self.width = (self.width + 1) % WIDTHS.len();
                        }
                        self.color_slider(ui);
                    }
                    Mode::Text => {
                        let name = match self.text_style {
                            TextStyle::Regular => tr("Regular"),
                            TextStyle::Highlight => tr("Highlight"),
                            TextStyle::Outline => tr("Outline"),
                        };
                        if pill(ui, name, false) {
                            self.text_style = match self.text_style {
                                TextStyle::Regular => TextStyle::Highlight,
                                TextStyle::Highlight => TextStyle::Outline,
                                TextStyle::Outline => TextStyle::Regular,
                            };
                            if let Some(i) = self.typing {
                                self.edits.labels[i].style = self.text_style;
                            }
                        }
                        let before = self.color;
                        self.color_slider(ui);
                        if let Some(i) = self.typing {
                            if self.color != before {
                                self.edits.labels[i].color = slider_color(self.color);
                            }
                            let label = &mut self.edits.labels[i];
                            let field = ui.add(egui::TextEdit::singleline(&mut label.text).hint_text("Add text").desired_width(220.));
                            field.request_focus();
                            if pill(ui, tr("Remove"), false) {
                                self.remember();
                                self.edits.labels.remove(i);
                                self.typing = None;
                            }
                        } else {
                            ui.label(RichText::new("Click the picture to add text").color(Color32::from_gray(200)));
                        }
                    }
                    Mode::Crop => {
                        for (preset, label) in [(CropPreset::Freeform, tr("Freeform")), (CropPreset::Square, tr("Square")), (CropPreset::Vertical, tr("9:16"))] {
                            if pill(ui, label, self.preset == preset) {
                                self.apply_preset(preset);
                            }
                        }
                        if pill(ui, tr("Rotate"), false) {
                            self.rotate();
                        }
                        if pill(ui, tr("Flip"), false) {
                            self.flip();
                        }
                        if pill(ui, tr("Reset"), false) {
                            self.remember();
                            self.edits.crop = Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let done = if self.caption.is_some() { tr("Save") } else { tr("Done") };
                    if widgets::pill_button(ui, done, true).clicked() {
                        result = Some(Some(self.bake()));
                    }
                    // `MediaEditor__tools--input`: the message to send with
                    // it; Enter saves.
                    if self.typing.is_none()
                        && let Some(caption) = self.caption.as_mut()
                    {
                        let field = ui.add(egui::TextEdit::singleline(caption).hint_text(RichText::new(tr("Message")).color(Color32::from_gray(160))).text_color(Color32::WHITE).background_color(Color32::from_gray(45)).desired_width(200.));
                        self.caption_focused = field.has_focus();
                        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            result = Some(Some(self.bake()));
                        }
                    }
                    // White on the editor's dark bar, whatever the theme.
                    if ui.add(egui::Button::new(RichText::new(tr("Cancel")).color(Color32::WHITE).font(theme::medium(14.))).fill(Color32::from_gray(60)).corner_radius(CornerRadius::same(16))).clicked() {
                        result = Some(None);
                    }
                    let can_redo = !self.redo.is_empty();
                    if ui.add_enabled(can_redo, egui::Button::new(RichText::new(tr("Redo")).color(Color32::WHITE))).clicked() {
                        self.redo_step();
                    }
                    let can_undo = !self.undo.is_empty();
                    if ui.add_enabled(can_undo, egui::Button::new(RichText::new(tr("Undo")).color(Color32::WHITE))).clicked() {
                        self.undo_step();
                    }
                });
            });
        });
        result
    }

    /// Desktop's hue slider: black at the start, white at the end.
    fn color_slider(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(180., 24.), Sense::click_and_drag());
        let track = rect.shrink2(Vec2::new(4., 8.));
        let steps = 60;
        for k in 0..steps {
            let a = k as f32 / steps as f32;
            let r = Rect::from_min_max(Pos2::new(track.left() + track.width() * a, track.top()), Pos2::new(track.left() + track.width() * (a + 1. / steps as f32) + 0.5, track.bottom()));
            ui.painter().rect_filled(r, 0, slider_color(a * 100.));
        }
        if let Some(p) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            self.color = ((p.x - track.left()) / track.width() * 100.).clamp(0., 100.);
        }
        let knob = Pos2::new(track.left() + track.width() * self.color / 100., track.center().y);
        ui.painter().circle_filled(knob, 9., slider_color(self.color));
        ui.painter().circle_stroke(knob, 9., Stroke::new(2., Color32::WHITE));
    }

    /// Desktop's `FunStickerPicker` over the bar: the installed packs'
    /// stickers; one picked goes in the middle of the picture.
    fn pick_sticker(&mut self, ui: &mut Ui, bar: Rect, packs: &[std::sync::Arc<langis_core::StickerPackView>]) {
        let size = Vec2::new(352., 360.);
        let at = Pos2::new(bar.center().x - size.x / 2., bar.top() - size.y - 8.);
        let mut picked = None;
        let area = egui::Area::new(egui::Id::unique("media-editor-stickers")).order(egui::Order::Tooltip).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(Color32::from_gray(38)).corner_radius(CornerRadius::same(8)).inner_margin(12).show(ui, |ui| {
                ui.set_width(size.x - 24.);
                ui.set_height(size.y - 24.);
                if packs.is_empty() {
                    ui.add_space(40.);
                    ui.vertical_centered(|ui| ui.label(RichText::new("No stickers installed yet").color(Color32::from_gray(200))));
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let cell = (ui.available_width() / 4.).floor();
                    for pack in packs {
                        ui.label(RichText::new(&pack.title).font(theme::semibold(13.)).color(Color32::from_gray(200)));
                        for row in pack.stickers.chunks(4) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                for st in row {
                                    let (r, hit) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                                    if hit.hovered() {
                                        ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_gray(60));
                                    }
                                    let file = st.path.to_string_lossy();
                                    egui::Image::new(crate::thumbs::uri(ui.ctx(), &file, r.size())).maintain_aspect_ratio(true).fit_to_exact_size(r.size() - Vec2::splat(12.)).paint_at(ui, r.shrink(6.));
                                    if hit.clicked() {
                                        picked = Some(st.path.clone());
                                    }
                                }
                            });
                        }
                        ui.add_space(6.);
                    }
                });
            });
        });
        if let Some(path) = picked {
            self.remember();
            let center = self.edits.crop.center();
            self.edits.stickers.push(Placed { path, at: center, size: 0.25 * self.edits.crop.width() });
            self.sticker_selected = Some(self.edits.stickers.len() - 1);
            self.sticker_picker = false;
            if self.mode == Mode::Crop {
                self.mode = Mode::Draw;
            }
        } else if ui.input(|i| i.pointer.any_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !bar.contains(p)) {
            self.sticker_picker = false;
        }
    }

    /// The turned picture cropped, its lines and text drawn in, as a PNG.
    fn bake(&self) -> Vec<u8> {
        let (tw, th) = (self.turned.width() as f32, self.turned.height() as f32);
        let c = self.edits.crop;
        let (x, y) = ((c.min.x * tw).round() as u32, (c.min.y * th).round() as u32);
        let (w, h) = (((c.width() * tw).round() as u32).max(1).min(self.turned.width() - x), ((c.height() * th).round() as u32).max(1).min(self.turned.height() - y));
        let mut out = image::imageops::crop_imm(&self.turned, x, y, w, h).to_image();
        // From 0-1 in the turned picture to pixels in the cropped one.
        let px = |p: Pos2| (p.x * tw - x as f32, p.y * th - y as f32);
        for line in &self.edits.lines {
            let radius = (line.width * tw / 2.).max(0.5);
            let alpha = if line.highlighter { 0.5 } else { 1. };
            for pair in line.points.windows(2).chain(std::iter::once(&line.points[..1.min(line.points.len())]).filter(|_| line.points.len() == 1)) {
                let (a, b) = (px(pair[0]), px(*pair.last().unwrap_or(&pair[0])));
                let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let steps = (length / (radius * 0.5).max(0.5)).ceil().max(1.) as usize;
                for s in 0..=steps {
                    let t = s as f32 / steps as f32;
                    dot(&mut out, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, radius, line.color, alpha);
                }
            }
        }
        for label in self.edits.labels.iter().filter(|l| !l.text.trim().is_empty()) {
            let (cx, cy) = px(label.at);
            draw_label(&mut out, label, cx, cy, label.size * tw);
        }
        for st in &self.edits.stickers {
            let Ok(img) = image::open(&st.path) else { continue };
            let side = (st.size * tw).round().max(1.) as u32;
            let img = img.resize(side, side, image::imageops::FilterType::Triangle).to_rgba8();
            let (cx, cy) = px(st.at);
            let (x, y) = ((cx - img.width() as f32 / 2.).round() as i64, (cy - img.height() as f32 / 2.).round() as i64);
            image::imageops::overlay(&mut out, &img, x, y);
        }
        let mut png = std::io::Cursor::new(Vec::new());
        let _ = out.write_to(&mut png, image::ImageFormat::Png);
        png.into_inner()
    }
}

/// A round dot of `color`, blended over what's there.
fn dot(image: &mut RgbaImage, cx: f32, cy: f32, r: f32, color: Color32, alpha: f32) {
    let (w, h) = (image.width() as i64, image.height() as i64);
    let (x0, x1) = (((cx - r).floor() as i64).max(0), ((cx + r).ceil() as i64).min(w - 1));
    let (y0, y1) = (((cy - r).floor() as i64).max(0), ((cy + r).ceil() as i64).min(h - 1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cover = (r + 0.5 - d).clamp(0., 1.) * alpha;
            if cover > 0. {
                blend(image.get_pixel_mut(x as u32, y as u32), color, cover);
            }
        }
    }
}

fn blend(pixel: &mut image::Rgba<u8>, color: Color32, a: f32) {
    let mix = |under: u8, over: u8| (under as f32 * (1. - a) + over as f32 * a).round() as u8;
    pixel[0] = mix(pixel[0], color.r());
    pixel[1] = mix(pixel[1], color.g());
    pixel[2] = mix(pixel[2], color.b());
    pixel[3] = pixel[3].max((a * 255.) as u8);
}

/// Text drawn into the picture in Inter SemiBold, centered on (cx, cy), in
/// the label's style.
fn draw_label(image: &mut RgbaImage, label: &Label, cx: f32, cy: f32, size: f32) {
    use ab_glyph::{Font, FontRef, ScaleFont};
    let Ok(font) = FontRef::try_from_slice(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")) else { return };
    let scaled = font.as_scaled(size.max(4.));
    // Lay it out on one line.
    let mut glyphs = Vec::new();
    let mut x = 0f32;
    let mut last = None;
    for c in label.text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = last {
            x += scaled.kern(prev, id);
        }
        glyphs.push(id.with_scale_and_position(scaled.scale(), ab_glyph::point(x, scaled.ascent())));
        x += scaled.h_advance(id);
        last = Some(id);
    }
    let (width, height) = (x, scaled.ascent() - scaled.descent());
    let (left, top) = (cx - width / 2., cy - height / 2.);
    let pad = size * 0.15;
    let (fill, ink) = match label.style {
        TextStyle::Highlight => {
            let ink = if label.color.r() as u32 + label.color.g() as u32 + label.color.b() as u32 > 600 { Color32::BLACK } else { Color32::WHITE };
            // The colored box behind the text.
            let (x0, y0) = ((left - pad).max(0.) as u32, (top - pad).max(0.) as u32);
            let (x1, y1) = (((left + width + pad) as u32).min(image.width()), ((top + height + pad) as u32).min(image.height()));
            for y in y0..y1 {
                for x in x0..x1 {
                    blend(image.get_pixel_mut(x, y), label.color, 1.);
                }
            }
            (None, ink)
        }
        TextStyle::Outline => (Some(if label.color == Color32::WHITE { Color32::BLACK } else { Color32::WHITE }), label.color),
        TextStyle::Regular => (None, label.color),
    };
    let mut paint = |offset: (f32, f32), color: Color32| {
        for g in &glyphs {
            let Some(outlined) = font.outline_glyph(g.clone()) else { continue };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cover| {
                let px = left + offset.0 + bounds.min.x + gx as f32;
                let py = top + offset.1 + bounds.min.y + gy as f32;
                if px >= 0. && py >= 0. && (px as u32) < image.width() && (py as u32) < image.height() && cover > 0. {
                    blend(image.get_pixel_mut(px as u32, py as u32), color, cover.min(1.));
                }
            });
        }
    };
    if let Some(outline) = fill {
        let d = (size * 0.06).max(1.);
        for o in [(-d, 0.), (d, 0.), (0., -d), (0., d), (-d, -d), (d, d), (-d, d), (d, -d)] {
            paint(o, outline);
        }
    }
    paint((0., 0.), ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_runs_black_through_hues_to_white() {
        assert_eq!(slider_color(0.), Color32::from_rgb(0, 0, 0));
        assert_eq!(slider_color(100.), Color32::from_rgb(255, 255, 255));
        let red = slider_color(7.01);
        assert!(red.r() > 250 && red.g() < 10 && red.b() < 10, "{red:?}");
    }

    #[test]
    fn baking_crops_turns_and_draws() {
        let mut png = std::io::Cursor::new(Vec::new());
        RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 255, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut editor = Editor::open(0, &png.into_inner()).unwrap();
        editor.rotate();
        assert_eq!(editor.turned.dimensions(), (100, 200));
        editor.edits.crop = Rect::from_min_max(Pos2::new(0., 0.), Pos2::new(1., 0.5));
        editor.edits.lines.push(Line { points: vec![Pos2::new(0.1, 0.25), Pos2::new(0.9, 0.25)], color: Color32::RED, width: 0.1, highlighter: false });
        editor.edits.labels.push(Label { text: "Hi".into(), at: Pos2::new(0.5, 0.1), color: Color32::WHITE, style: TextStyle::Outline, size: 0.2 });
        let out = image::load_from_memory(&editor.bake()).unwrap().to_rgba8();
        assert_eq!(out.dimensions(), (100, 100));
        // The red line across the middle of the crop.
        let mid = out.get_pixel(50, 50);
        assert!(mid[0] > 200 && mid[2] < 60, "{mid:?}");
        // Untouched blue below it.
        assert_eq!(out.get_pixel(50, 95)[2], 255);
    }
}
