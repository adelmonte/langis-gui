//! Desktop's menus: `AxoDropdownMenu` and `AxoContextMenu`, both drawn by
//! `AxoBaseMenu` (`ts/axo/_internal/AxoBaseMenu.dom.tsx`) on Radix.
//!
//! A panel is 200 to 300px wide with 6px padding, `material-tertiary`,
//! `curved-xl` corners and `shadow-elevation-3`. Rows are 6px padded, 13px
//! text on an 18px line, with a leading column for 16px symbols or 14px
//! checks shared by every row of the panel. Submenus open on hover beside
//! their trigger, their first row level with it (`alignOffset={-6}`), and stay
//! open while the pointer heads towards them. While a menu is open, the rest
//! of the window ignores the pointer: a press outside only closes the menu
//! (Radix's `modal`).

use egui::{Color32, CornerRadius, Id, Order, Pos2, Rect, Sense, Shadow, Stroke, Ui, Vec2};

use crate::theme::{self, Palette};

const PAD: f32 = 6.;
const ROW: f32 = 30.;
const SYMBOL: f32 = 16.;
const CHECK: f32 = 14.;
const GAP: f32 = 6.;
const MIN_W: f32 = 200.;
const MAX_W: f32 = 300.;
/// `collisionPadding`: how close to the window's edge a panel may go.
const EDGE: f32 = 6.;

#[derive(Clone, Copy, Debug)]
enum Anchor {
    /// A dropdown: below its trigger (`sideOffset={4}`), centered on it.
    Below(Rect),
    /// A context menu: at the pointer.
    Point(Pos2),
}

#[derive(Clone, Debug, Default)]
struct State {
    anchor: Option<Anchor>,
    /// The open submenus, outermost first: each one's id and its trigger.
    subs: Vec<(Id, Rect)>,
    /// When the pointer left a submenu's trigger towards it.
    heading: Option<f64>,
}

/// What a panel measured last frame, to size and place it this frame.
#[derive(Clone, Copy, Debug, Default)]
struct Measure {
    width: f32,
    size: Vec2,
    leading: f32,
}

fn state(ctx: &egui::Context, id: Id) -> State {
    ctx.data(|d| d.get_temp::<State>(id)).unwrap_or_default()
}

fn set_state(ctx: &egui::Context, id: Id, state: State) {
    ctx.data_mut(|d| d.insert_temp(id, state));
}

pub fn is_open(ctx: &egui::Context, id: Id) -> bool {
    state(ctx, id).anchor.is_some()
}

pub fn close(ctx: &egui::Context, id: Id) {
    set_state(ctx, id, State::default());
}

/// `AxoDropdownMenu`: opens when `trigger` is clicked. Returns whether it
/// is open.
pub fn dropdown(ui: &Ui, trigger: &egui::Response, id: Id, add: impl FnOnce(&mut Menu)) -> bool {
    if trigger.clicked() {
        set_state(ui.ctx(), id, State { anchor: Some(Anchor::Below(trigger.rect)), ..Default::default() });
    }
    show(ui.ctx(), id, add)
}

/// Opens a dropdown below `rect` without a click on a trigger.
pub fn open_below(ctx: &egui::Context, id: Id, rect: Rect) {
    set_state(ctx, id, State { anchor: Some(Anchor::Below(rect)), ..Default::default() });
}

/// `AxoContextMenu`: opens at the pointer on a secondary click on `target`.
pub fn context(ui: &Ui, target: &egui::Response, id: Id, add: impl FnOnce(&mut Menu)) -> bool {
    if target.secondary_clicked() {
        let at = target.interact_pointer_pos().or(ui.ctx().pointer_latest_pos()).unwrap_or(target.rect.center());
        set_state(ui.ctx(), id, State { anchor: Some(Anchor::Point(at)), ..Default::default() });
    }
    show(ui.ctx(), id, add)
}

/// Opens a context menu at `at`, as a secondary click would.
pub fn open_at(ctx: &egui::Context, id: Id, at: Pos2) {
    set_state(ctx, id, State { anchor: Some(Anchor::Point(at)), ..Default::default() });
}

/// Draws menu `id` if it is open.
pub fn show(ctx: &egui::Context, id: Id, add: impl FnOnce(&mut Menu)) -> bool {
    let mut state = state(ctx, id);
    let Some(anchor) = state.anchor else { return false };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close(ctx, id);
        return false;
    }

    // Radix's modal layer: the window under the menu takes no pointer input.
    let screen = ctx.content_rect();
    let blocker = egui::Area::new(id.with("blocker")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        ui.allocate_exact_size(screen.size(), Sense::click_and_drag()).1
    });
    if blocker.inner.is_pointer_button_down_on() || blocker.inner.clicked() || blocker.inner.secondary_clicked() {
        close(ctx, id);
        return false;
    }

    let measure = measure(ctx, id);
    let size = measure.size.max(Vec2::new(MIN_W, ROW + 2. * PAD));
    let pos = match anchor {
        Anchor::Below(trigger) => {
            let mut x = trigger.center().x - size.x / 2.;
            let mut y = trigger.bottom() + 4.;
            if y + size.y > screen.bottom() - EDGE && trigger.top() - 4. - size.y >= screen.top() + EDGE {
                y = trigger.top() - 4. - size.y;
            }
            x = x.clamp(screen.left() + EDGE, (screen.right() - EDGE - size.x).max(screen.left() + EDGE));
            y = y.min(screen.bottom() - EDGE - size.y).max(screen.top() + EDGE);
            Pos2::new(x, y)
        }
        Anchor::Point(at) => {
            let mut x = at.x;
            let mut y = at.y;
            if x + size.x > screen.right() - EDGE {
                x = (at.x - size.x).max(screen.left() + EDGE);
            }
            if y + size.y > screen.bottom() - EDGE {
                y = (at.y - size.y).max(screen.top() + EDGE);
            }
            Pos2::new(x, y)
        }
    };
    let mut closed = false;
    panel(ctx, id, id, 0, pos, &mut state, &mut closed, add);
    if closed {
        close(ctx, id);
    } else {
        set_state(ctx, id, state);
    }
    !closed
}

fn measure(ctx: &egui::Context, panel: Id) -> Measure {
    ctx.data(|d| d.get_temp::<Measure>(panel.with("measure"))).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn panel(ctx: &egui::Context, root: Id, id: Id, depth: usize, pos: Pos2, state: &mut State, closed: &mut bool, add: impl FnOnce(&mut Menu)) -> Rect {
    let palette = Palette::of(ctx);
    let last = measure(ctx, id);
    let width = last.width.clamp(MIN_W, MAX_W);
    let fill = if palette.dark { Color32::from_rgba_unmultiplied(0x2a, 0x2a, 0x2a, 245) } else { Color32::from_rgba_unmultiplied(0xef, 0xef, 0xef, 245) };
    let outline = if palette.dark { Color32::TRANSPARENT } else { Color32::from_black_alpha(31) };
    let shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(if palette.dark { 61 } else { 31 }) };
    let response = egui::Area::new(id.with("panel")).order(Order::Tooltip).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(0.5, outline))
            .corner_radius(CornerRadius::same(14))
            .shadow(shadow)
            .inner_margin(egui::Margin::same(PAD as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.set_width(width - 2. * PAD);
                let mut menu = Menu { ui, root, id, depth, palette, leading: last.leading, needed: 0., found_leading: 0., state, closed };
                add(&mut menu);
                (menu.needed, menu.found_leading)
            })
            .inner
    });
    let (needed, leading) = response.inner;
    let rect = response.response.rect;
    ctx.data_mut(|d| d.insert_temp(id.with("measure"), Measure { width: needed + 2. * PAD, size: rect.size(), leading }));
    if last.leading != leading || (last.width - (needed + 2. * PAD)).abs() > 0.5 {
        ctx.request_repaint();
    }
    rect
}

/// The rows of one open panel.
pub struct Menu<'a> {
    pub ui: &'a mut Ui,
    root: Id,
    id: Id,
    depth: usize,
    palette: Palette,
    /// The leading column's width, from last frame.
    leading: f32,
    needed: f32,
    found_leading: f32,
    state: &'a mut State,
    closed: &'a mut bool,
}

enum Lead<'s> {
    None,
    Symbol(&'s str),
    Check(bool),
}

impl Menu<'_> {
    /// `Item`: true when chosen, which closes the menu.
    pub fn item(&mut self, symbol: Option<&str>, label: &str) -> bool {
        self.row(symbol.map_or(Lead::None, Lead::Symbol), None, label, None, true)
    }

    /// An `Item` that can't be chosen.
    pub fn item_disabled(&mut self, symbol: Option<&str>, label: &str) -> bool {
        self.row(symbol.map_or(Lead::None, Lead::Symbol), None, label, None, false)
    }

    /// `Item` with a secondary line under the label.
    pub fn item_described(&mut self, symbol: Option<&str>, label: &str, description: &str) -> bool {
        self.row(symbol.map_or(Lead::None, Lead::Symbol), None, label, Some(description), true)
    }

    /// `CheckboxItem` and `RadioItem`: a check before the label when chosen.
    pub fn check(&mut self, checked: bool, label: &str) -> bool {
        self.row(Lead::Check(checked), None, label, None, true)
    }

    /// A `CheckboxItem` with a symbol before its label.
    pub fn check_symbol(&mut self, checked: bool, symbol: &str, label: &str) -> bool {
        self.row(Lead::Check(checked), Some(symbol), label, None, true)
    }

    /// `Label`: small secondary text, not choosable.
    pub fn label(&mut self, text: &str) {
        let font = theme::regular(12.);
        let w = self.ui.available_width();
        let (rect, _) = self.ui.allocate_exact_size(Vec2::new(w, 28.), Sense::hover());
        let galley = self.ui.painter().layout(text.to_owned(), font, self.palette.text_secondary, w - 2. * PAD);
        self.needed = self.needed.max(galley.size().x + 2. * PAD);
        self.ui.painter().galley(Pos2::new(rect.left() + PAD, rect.center().y - galley.size().y / 2.), galley, self.palette.text_secondary);
    }

    /// `Separator`: 4px above and below, inset 2px.
    pub fn separator(&mut self) {
        let w = self.ui.available_width();
        let (rect, _) = self.ui.allocate_exact_size(Vec2::new(w, 8.5), Sense::hover());
        let border = if self.palette.dark { Color32::from_white_alpha(15) } else { Color32::from_black_alpha(15) };
        self.ui.painter().hline(rect.left() + 2.0..=rect.right() - 2., rect.top() + 4.25, Stroke::new(0.5, border));
    }

    /// `Sub`: a row that opens `add` beside it.
    pub fn sub(&mut self, symbol: Option<&str>, label: &str, add: impl FnOnce(&mut Menu)) {
        let sub_id = self.id.with(("sub", label));
        let open = self.state.subs.get(self.depth).is_some_and(|(id, _)| *id == sub_id);
        let rect = self.row_rect();
        let response = self.ui.interact(rect, sub_id.with("trigger"), Sense::click());
        let hovered = response.hovered();
        if hovered || response.clicked() {
            self.state.subs.truncate(self.depth);
            self.state.subs.push((sub_id, rect));
            self.state.heading = None;
        }
        let open = open || hovered || response.clicked();
        let fill = if hovered {
            self.palette.fill_pressed
        } else if open {
            self.palette.fill
        } else {
            Color32::TRANSPARENT
        };
        let natural = self.paint_row(rect, fill, symbol.map_or(Lead::None, Lead::Symbol), None, label, None, true);
        theme::paint_symbol(self.ui.painter(), "chevron-[end]", Pos2::new(rect.right() - PAD - 7., rect.center().y), CHECK, self.palette.text);
        self.needed = self.needed.max(natural + CHECK + 4.);
        if !open {
            return;
        }
        // Beside the trigger, its first row level with it; on the left when
        // there's no room on the right.
        let ctx = self.ui.ctx().clone();
        let size = measure(&ctx, sub_id).size.max(Vec2::new(MIN_W, ROW + 2. * PAD));
        let parent = self.ui.min_rect().expand(PAD);
        let screen = ctx.content_rect();
        let mut x = parent.right();
        if x + size.x > screen.right() - EDGE {
            x = (parent.left() - size.x).max(screen.left() + EDGE);
        }
        let y = (rect.top() - PAD).min(screen.bottom() - EDGE - size.y).max(screen.top() + EDGE);
        let sub_rect = panel(&ctx, self.root, sub_id, self.depth + 1, Pos2::new(x, y), self.state, self.closed, add);
        self.state.subs.truncate(self.depth + 1);
        if let Some(entry) = self.state.subs.get_mut(self.depth) {
            entry.1 = rect;
        }
        // Keep the panel's rect for the pointer-heading test.
        ctx.data_mut(|d| d.insert_temp(sub_id.with("rect"), sub_rect));
    }

    fn row_rect(&mut self) -> Rect {
        let w = self.ui.available_width();
        self.ui.allocate_exact_size(Vec2::new(w, ROW), Sense::hover()).0
    }

    fn row(&mut self, lead: Lead, symbol: Option<&str>, label: &str, description: Option<&str>, enabled: bool) -> bool {
        let w = self.ui.available_width();
        let height = if description.is_some() { ROW + 16. } else { ROW };
        let (rect, _) = self.ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
        let id = self.id.with(("row", label));
        let response = self.ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
        let hovered = enabled && response.hovered();
        if response.hovered() && self.state.subs.len() > self.depth && !self.heading_to_sub() {
            self.state.subs.truncate(self.depth);
        }
        let fill = if hovered { self.palette.fill_pressed } else { Color32::TRANSPARENT };
        let natural = self.paint_row(rect, fill, lead, symbol, label, description, enabled);
        self.needed = self.needed.max(natural);
        if enabled && response.clicked() {
            *self.closed = true;
            return true;
        }
        false
    }

    /// Whether the pointer is moving from the open submenu's trigger
    /// towards its panel, so passing over the rows between doesn't close
    /// it (Radix's pointer grace area).
    fn heading_to_sub(&mut self) -> bool {
        let Some((sub_id, trigger)) = self.state.subs.get(self.depth).copied() else { return false };
        let ctx = self.ui.ctx().clone();
        let Some(target) = ctx.data(|d| d.get_temp::<Rect>(sub_id.with("rect"))) else { return false };
        let (pos, velocity, now) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.velocity(), i.time));
        let Some(pos) = pos else { return false };
        let towards = if target.left() >= trigger.right() - 1. { velocity.x > 20. } else { velocity.x < -20. };
        let within = pos.y >= target.top() - ROW && pos.y <= target.bottom() + ROW;
        let started = *self.state.heading.get_or_insert(now);
        if towards && within && now - started < 0.5 {
            ctx.request_repaint();
            return true;
        }
        self.state.heading = None;
        false
    }

    #[allow(clippy::too_many_arguments)]
    /// Draws a row; returns the width it needs.
    fn paint_row(&mut self, rect: Rect, fill: Color32, lead: Lead, symbol: Option<&str>, label: &str, description: Option<&str>, enabled: bool) -> f32 {
        let painter = self.ui.painter();
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, CornerRadius::same(7), fill);
        }
        let color = if enabled { self.palette.text } else { self.palette.text_disabled() };
        let lead_w = match lead {
            Lead::None => 0.,
            Lead::Symbol(_) => SYMBOL,
            Lead::Check(_) => CHECK,
        };
        self.found_leading = self.found_leading.max(lead_w);
        let column = self.leading.max(lead_w);
        let lead_x = rect.left() + PAD;
        let content_y = if description.is_some() { rect.top() + PAD + 9. } else { rect.center().y };
        match lead {
            Lead::None => {}
            Lead::Symbol(name) => theme::paint_symbol(painter, name, Pos2::new(lead_x + SYMBOL / 2., content_y), SYMBOL, color),
            Lead::Check(true) => theme::paint_symbol(painter, "check", Pos2::new(lead_x + CHECK / 2., content_y), CHECK, color),
            Lead::Check(false) => {}
        }
        let mut x = lead_x + if column > 0. { column + GAP } else { 0. };
        if let Some(name) = symbol {
            theme::paint_symbol(painter, name, Pos2::new(x + SYMBOL / 2., content_y), SYMBOL, color);
            x += SYMBOL + 8.;
        }
        let room = (rect.right() - PAD - x).max(0.);
        let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), theme::regular(13.), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = painter.layout_job(job);
        let natural = painter.layout_no_wrap(label.to_owned(), theme::regular(13.), color).size().x;
        painter.galley(Pos2::new(x, content_y - galley.size().y / 2.), galley, color);
        if let Some(text) = description {
            let g = painter.layout(text.to_owned(), theme::regular(12.), self.palette.text_secondary, room);
            painter.galley(Pos2::new(x, content_y + 10.), g, self.palette.text_secondary);
        }
        x - rect.left() + natural + PAD
    }
}

/// A row's text color when disabled (`label-disabled`).
impl Palette {
    pub fn text_disabled(&self) -> Color32 {
        if self.dark { Color32::from_white_alpha(64) } else { Color32::from_black_alpha(64) }
    }
}
