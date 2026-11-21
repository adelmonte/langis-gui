//! What Chromium and Electron give Signal Desktop without a line of its
//! own, and egui leaves to the app: the context menu on text (Copy of a
//! selection, Copy link, a text field's Cut, Copy, Paste and Select All, as
//! `app/spell_check.main.ts` builds it), a right-click that keeps the
//! selection, the pointer over anything clickable, the focus ring after Tab,
//! and the window's keys (zoom by Chromium's levels, F11, Ctrl+Q, Ctrl+M).

use std::sync::Arc;

use egui::text_selection::LabelSelectionState;
use egui::{Color32, Context, CursorIcon, Event, Galley, Id, Key, KeyboardShortcut, Modifiers, PointerButton, Pos2, Response, Sense, Stroke, Ui};

use crate::app::App;
use crate::i18n::tr;

/// What the open context menu is for.
#[derive(Clone)]
enum Target {
    /// Selected text outside a text field.
    Selection,
    /// A link: Copy link.
    Link(String),
    /// A photo: Copy image.
    Image(std::path::PathBuf),
    /// A text field: what it can do now.
    Field { id: Id, selected: bool, rich: bool },
}

fn menu_id() -> Id {
    Id::unique("electron-context-menu")
}

fn target_id() -> Id {
    Id::unique("electron-context-menu-target")
}

fn open(ctx: &Context, target: Target, at: Pos2) {
    ctx.data_mut(|d| d.insert_temp(target_id(), target));
    crate::menu::open_at(ctx, menu_id(), at);
}

/// Input to replay at the start of next frame, once a text field has its
/// focus back (`App::raw_input_hook`).
#[derive(Clone, Default)]
struct Queued(Vec<Event>);

fn queue(ctx: &Context, event: Event) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Queued>(Id::unique("queued-input")).0.push(event));
    ctx.request_repaint();
}

pub fn inject(ctx: &Context, raw: &mut egui::RawInput) {
    let queued = ctx.data_mut(|d| d.remove_temp::<Queued>(Id::unique("queued-input")));
    if let Some(Queued(events)) = queued {
        raw.events.extend(events);
    }
}

/// Is any label text selected?
pub fn has_selection(ctx: &Context) -> bool {
    ctx.plugin::<LabelSelectionState>().lock().has_selection()
}

/// Paints `galley` at `pos` as text that can be selected (drag, double- and
/// triple-click, Shift+click) and copied (Ctrl+C), as Chromium's text is.
/// `response` must sense drags. A right-click keeps the selection, as in
/// Chromium.
pub fn paint_selectable(ui: &Ui, response: &Response, pos: Pos2, galley: Arc<Galley>, color: Color32) {
    let mut r = response.clone();
    if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary) || i.pointer.button_pressed(PointerButton::Middle)) {
        r.sense = Sense::hover();
    }
    LabelSelectionState::label_text_selection(ui, &r, pos, galley, color, Stroke::NONE);
}

/// Text that can be selected and copied, with Electron's context menu
/// (Copy, while something is selected). Returns its response.
pub fn selectable(ui: &Ui, id: Id, pos: Pos2, galley: Arc<Galley>, color: Color32) -> Response {
    // `galley.rect`: a centered galley's rows sit around x = 0.
    let rect = galley.rect.translate(pos.to_vec2());
    let response = ui.interact(rect, id, select_sense());
    paint_selectable(ui, &response, pos, galley, color);
    copy_menu(ui, &response);
    response
}

/// What selectable text senses: a drag to select, clicks for words and
/// lines; Tab passes it by, as egui's selectable label.
fn select_sense() -> Sense {
    Sense::click_and_drag() - Sense::FOCUSABLE
}

/// A label whose text can be selected and copied (`selectable`), wrapped
/// at the available width.
pub fn label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    let galley = text.into().into_galley(ui, Some(egui::TextWrapMode::Wrap), ui.available_width(), egui::TextStyle::Body);
    let color = ui.visuals().text_color();
    let (rect, response) = ui.allocate_exact_size(galley.size(), select_sense());
    paint_selectable(ui, &response, rect.min, galley, color);
    copy_menu(ui, &response);
    response
}

/// Electron's context menu on plain text: Copy, when something is selected.
pub fn copy_menu(ui: &Ui, response: &Response) {
    if response.secondary_clicked() && has_selection(ui.ctx()) {
        let at = response.interact_pointer_pos().unwrap_or(response.rect.center());
        open(ui.ctx(), Target::Selection, at);
    }
}

/// Electron's context menu on a link: Copy link.
pub fn link_menu(ui: &Ui, response: &Response, url: &str) {
    if response.secondary_clicked() {
        let at = response.interact_pointer_pos().unwrap_or(response.rect.center());
        open(ui.ctx(), Target::Link(url.to_owned()), at);
    }
}

/// Electron's context menu on an image: Copy image.
pub fn image_menu(ui: &Ui, response: &Response, path: &std::path::Path) {
    if response.secondary_clicked() {
        let at = response.interact_pointer_pos().unwrap_or(response.rect.center());
        open(ui.ctx(), Target::Image(path.to_owned()), at);
    }
}

/// Puts the photo at `path` on the clipboard, as Desktop's Copy image. The
/// clipboard is served until something else takes it.
fn copy_image(path: std::path::PathBuf) {
    std::thread::spawn(move || {
        let image = match image::open(&path) {
            Ok(image) => image.into_rgba8(),
            Err(error) => return tracing::warn!(%error, "couldn't read the image to copy"),
        };
        let data = arboard::ImageData { width: image.width() as usize, height: image.height() as usize, bytes: image.into_raw().into() };
        let copied = arboard::Clipboard::new().and_then(|mut clipboard| {
            use arboard::SetExtLinux as _;
            clipboard.set().wait().image(data)
        });
        if let Err(error) = copied {
            tracing::warn!(%error, "couldn't copy the image");
        }
    });
}

/// A text field's selection before this frame's secondary press, which
/// egui's `TextEdit` collapses to the pointer.
#[derive(Clone)]
struct Before(Id, egui::text_edit::TextEditState);

fn text_field_under_pointer(ctx: &Context) -> Option<(Id, egui::text_edit::TextEditState)> {
    let hovered: Vec<Id> = ctx.interaction_snapshot(|s| s.contains_pointer.iter().copied().collect());
    hovered.into_iter().find_map(|id| egui::text_edit::TextEditState::load(ctx, id).map(|s| (id, s)))
}

/// Before the frame's widgets: the window's keys, and a text field's
/// selection kept for a right-click.
fn copy_row_id() -> Id {
    Id::unique("context-menu-copy-row")
}

pub fn begin(app: &mut App, ctx: &Context) {
    begin_frame(ctx);
    window_keys(app, ctx);
}

/// `begin`, but the window's keys.
fn begin_frame(ctx: &Context) {
    // Copy of a selection acts on the press, before egui's labels drop the
    // selection for a press outside them: the labels gather the text
    // when they see the Copy event, as they're drawn.
    if crate::menu::is_open(ctx, menu_id())
        && matches!(ctx.data(|d| d.get_temp::<Target>(target_id())), Some(Target::Selection))
        && let Some(row) = ctx.data(|d| d.get_temp::<egui::Rect>(copy_row_id()))
        && ctx.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| row.contains(p)))
    {
        ctx.input_mut(|i| i.events.push(Event::Copy));
    }
    crate::menu::take_keys(ctx);
    if ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary))
        && let Some((id, state)) = text_field_under_pointer(ctx)
        && state.cursor.char_range().is_some_and(|r| !r.is_empty())
    {
        ctx.data_mut(|d| d.insert_temp(Id::unique("field-before-right-click"), Before(id, state)));
    }
}

/// After the frame's widgets: the context menus, the pointer and the
/// focus ring.
pub fn end(app: &mut App, ctx: &Context) {
    let composer = app.chat.as_ref().map(|c| crate::composer::input_id(&c.id));
    end_frame(ctx, composer);
}

/// `end`, given the composer's id (a rich editor: Paste and Match Style).
fn end_frame(ctx: &Context, composer: Option<Id>) {
    // A right-click in a text field keeps what was selected, as in Chromium.
    let before = Id::unique("field-before-right-click");
    if let Some(Before(id, state)) = ctx.data(|d| d.get_temp::<Before>(before)) {
        ctx.data_mut(|d| d.remove::<Before>(before));
        state.store(ctx, id);
    }
    let spell_open = crate::menu::is_open(ctx, Id::unique("composer-spell-menu"));
    if ctx.input(|i| i.pointer.button_released(PointerButton::Secondary)) && !crate::menu::is_open(ctx, menu_id()) && !spell_open
        && let Some((id, state)) = text_field_under_pointer(ctx)
    {
        let selected = state.cursor.char_range().is_some_and(|r| !r.is_empty());
        let rich = composer == Some(id);
        let at = ctx.pointer_latest_pos().unwrap_or_default();
        ctx.memory_mut(|m| m.request_focus(id));
        open(ctx, Target::Field { id, selected, rich }, at);
    }
    show_menu(ctx);
    pointer(ctx);
    focus_ring(ctx);
}

fn show_menu(ctx: &Context) {
    let Some(target) = ctx.data(|d| d.get_temp::<Target>(target_id())) else { return };
    let open = crate::menu::show(ctx, menu_id(), |m| match &target {
        Target::Selection => {
            let row = m.next_row();
            m.ui.ctx().data_mut(|d| d.insert_temp(copy_row_id(), row));
            // A press on it copied already (`begin`); Enter copies next frame.
            if m.item(None, tr("Copy")) && !m.ui.input(|i| i.pointer.any_pressed() || i.pointer.any_released()) {
                queue(m.ui.ctx(), Event::Copy);
            }
        }
        Target::Link(url) => {
            if m.item(None, tr("Copy link")) {
                m.ui.ctx().copy_text(url.clone());
            }
        }
        Target::Image(path) => {
            if m.item(None, tr("Copy image")) {
                copy_image(path.clone());
            }
        }
        Target::Field { id, selected, rich } => field_items(m, *id, *selected, *rich),
    });
    if !open {
        ctx.data_mut(|d| d.remove::<Target>(target_id()));
    }
}

/// A text field's part of Electron's menu: Cut and Copy with a selection,
/// Paste, Paste and Match Style in the composer (a rich editor), Select All.
pub fn field_items(m: &mut crate::menu::Menu, id: Id, selected: bool, rich: bool) {
    let ctx = m.ui.ctx().clone();
    let act = |event: Option<Event>, paste: bool| {
        ctx.memory_mut(|mem| mem.request_focus(id));
        if let Some(event) = event {
            queue(&ctx, event);
        }
        if paste {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
    };
    if selected && m.item(None, tr("Cut")) {
        act(Some(Event::Cut), false);
    }
    if selected && m.item(None, tr("Copy")) {
        act(Some(Event::Copy), false);
    }
    if m.item(None, tr("Paste")) {
        act(None, true);
    }
    // egui pastes plain text only, so both are the same paste.
    if rich && m.item(None, tr("Paste and Match Style")) {
        act(None, true);
    }
    if m.item(None, tr("Select All")) {
        act(Some(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }), false);
    }
}

/// Opts a clickable area out of the pointing hand (the bubble, whose click
/// only opens its menu, as Desktop's message isn't a button).
pub fn no_pointer(ctx: &Context, id: Id) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<NoPointer>(Id::unique("no-pointer")).0.push(id));
}

#[derive(Clone, Default)]
struct NoPointer(Vec<Id>);

/// Desktop's `button { cursor: pointer }`: the hand over anything that
/// takes a click and nothing else said otherwise. Areas covering much of
/// the window (a backdrop that closes on a click) keep the arrow.
fn pointer(ctx: &Context) {
    let skip = ctx.data_mut(|d| d.remove_temp::<NoPointer>(Id::unique("no-pointer"))).unwrap_or_default().0;
    if ctx.output(|o| o.cursor_icon) != CursorIcon::Default {
        return;
    }
    let screen = ctx.content_rect().area();
    let hovered: Vec<Id> = ctx.interaction_snapshot(|s| s.hovered.iter().copied().collect());
    for id in hovered {
        if skip.contains(&id) {
            continue;
        }
        let Some(r) = ctx.read_response(id) else { continue };
        if r.enabled() && r.sense.senses_click() && !r.sense.senses_drag() && r.rect.area() < screen * 0.3 && r.contains_pointer() {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
            return;
        }
    }
}

/// Chromium's `:focus-visible`: after Tab moves the focus, a ring around
/// the focused control (text fields show their caret instead).
fn focus_ring(ctx: &Context) {
    let flag = Id::unique("keyboard-focus");
    let tabbed = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::Tab, pressed: true, .. })));
    let pressed = ctx.input(|i| i.pointer.any_pressed());
    let mut keyboard = ctx.data(|d| d.get_temp::<bool>(flag)).unwrap_or(false);
    if tabbed {
        keyboard = true;
    } else if pressed {
        keyboard = false;
    }
    ctx.data_mut(|d| d.insert_temp(flag, keyboard));
    if !keyboard {
        return;
    }
    let Some(id) = ctx.memory(|m| m.focused()) else { return };
    if egui::text_edit::TextEditState::load(ctx, id).is_some() {
        return;
    }
    let Some(r) = ctx.read_response(id) else { return };
    let palette = crate::theme::Palette::of(ctx);
    ctx.layer_painter(r.layer_id).rect_stroke(r.rect.expand(2.), egui::CornerRadius::same(8), Stroke::new(2., palette.accent), egui::StrokeKind::Outside);
}

/// Chromium's zoom levels: each step 1.2 times the last
/// (`ZoomFactorService`), between a quarter and five times.
pub fn zoom_step(app: &App, ctx: &Context, steps: i32) {
    let now = ctx.zoom_factor();
    let factor = if steps == 0 {
        1.
    } else {
        let level = (now.ln() / 1.2f32.ln()).round() + steps as f32;
        1.2f32.powf(level).clamp(0.25, 5.)
    };
    if (factor - now).abs() > 0.001 {
        ctx.set_zoom_factor(factor);
        crate::settings::write(app, crate::settings::SETTING_ZOOM, factor.to_string());
    }
}

/// The accelerators of Desktop's menu (`app/menu.std.ts`) and Electron's
/// roles: Ctrl+= and Ctrl+- zoom, Ctrl+0 resets, Ctrl+wheel zooms, F11
/// toggles full screen, Ctrl+M minimizes and Ctrl+Q quits.
fn window_keys(app: &mut App, ctx: &Context) {
    let cmd = Modifiers::COMMAND;
    let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
    if shortcut(cmd, Key::Plus) || shortcut(cmd, Key::Equals) {
        zoom_step(app, ctx, 1);
    }
    if shortcut(cmd, Key::Minus) {
        zoom_step(app, ctx, -1);
    }
    if shortcut(cmd, Key::Num0) {
        zoom_step(app, ctx, 0);
    }
    // Ctrl+wheel: a step a notch (or 50 points of a touchpad).
    let wheel: f32 = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.command => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y / 50.,
                    egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
                }),
                _ => None,
            })
            .sum()
    });
    let acc_id = Id::unique("ctrl-wheel");
    let acc = ctx.data(|d| d.get_temp::<f32>(acc_id)).unwrap_or(0.) + wheel;
    let steps = acc.trunc() as i32;
    let held = ctx.input(|i| i.modifiers.command);
    let rest = if wheel == 0. && !held { 0. } else { acc - steps as f32 };
    ctx.data_mut(|d| d.insert_temp(acc_id, rest));
    if steps != 0 {
        zoom_step(app, ctx, steps.signum());
    }
    if shortcut(Modifiers::NONE, Key::F11) {
        let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
    }
    if shortcut(cmd, Key::M) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    if shortcut(cmd, Key::Q) {
        app.quit(ctx);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, OutputCommand, PointerButton, Pos2, Vec2};
    use egui_kittest::Harness;

    fn press(h: &Harness, at: Pos2, button: PointerButton, pressed: bool) {
        h.event(Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
    }

    fn copied(h: &Harness) -> Option<String> {
        h.output().platform_output.commands.iter().find_map(|c| match c {
            OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
    }

    /// Drag to select, right-click on the selection, Copy: the text is
    /// copied, as Electron's context menu copies it.
    #[test]
    fn right_click_copies_selected_text() {
        let mut h = Harness::new_ui(|ui| {
            crate::theme::install(ui.ctx());
            super::begin_frame(ui.ctx());
            let r = crate::interaction::label(ui, "hello world");
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("test-label"), r.rect));
            super::end_frame(ui.ctx(), None);
        });
        h.set_size(Vec2::new(400., 300.));
        h.run();
        let rect = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("test-label"))).unwrap();
        let (start, end) = (Pos2::new(rect.left() + 1., rect.center().y), Pos2::new(rect.right() + 20., rect.center().y));
        h.hover_at(start);
        h.run();
        h.drag_at(start);
        h.run();
        h.hover_at(end);
        h.run();
        h.drop_at(end);
        h.run();
        assert!(super::has_selection(&h.ctx));
        let inside = rect.center();
        h.hover_at(inside);
        h.run();
        press(&h, inside, PointerButton::Secondary, true);
        h.run();
        press(&h, inside, PointerButton::Secondary, false);
        h.run();
        assert!(super::has_selection(&h.ctx), "a right-click keeps the selection");
        assert!(crate::menu::is_open(&h.ctx, super::menu_id()));
        // The menu's first row: Copy.
        let row = inside + Vec2::new(40., 6. + 15.);
        h.hover_at(row);
        h.run();
        press(&h, row, PointerButton::Primary, true);
        h.step();
        assert_eq!(copied(&h).as_deref(), Some("hello world"));
    }

    /// A right-click in a text field keeps its selection and opens Electron's
    /// menu for it, Cut and Copy included.
    #[test]
    fn right_click_in_a_field_keeps_the_selection() {
        let mut text = String::from("hello world");
        // A bare field has no accessible name; the check isn't the point here.
        let mut h = Harness::builder().with_accessibility_check(false).build_ui(move |ui| {
            super::begin_frame(ui.ctx());
            let r = ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::unique("test-field")));
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("test-field-rect"), r.rect));
            super::end_frame(ui.ctx(), None);
        });
        h.set_size(Vec2::new(400., 300.));
        h.run();
        let rect = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("test-field-rect"))).unwrap();
        let at = rect.center();
        h.hover_at(at);
        h.run();
        press(&h, at, PointerButton::Primary, true);
        press(&h, at, PointerButton::Primary, false);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
        h.run();
        let selected = |h: &Harness| egui::text_edit::TextEditState::load(&h.ctx, egui::Id::unique("test-field")).and_then(|s| s.cursor.char_range()).is_some_and(|r| !r.is_empty());
        assert!(selected(&h));
        press(&h, at, PointerButton::Secondary, true);
        h.run();
        press(&h, at, PointerButton::Secondary, false);
        h.run();
        assert!(selected(&h), "a right-click keeps the selection");
        assert!(crate::menu::is_open(&h.ctx, super::menu_id()));
        assert!(matches!(h.ctx.data(|d| d.get_temp::<super::Target>(super::target_id())), Some(super::Target::Field { selected: true, .. })));
    }
}
