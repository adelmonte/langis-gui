//! Calls: Desktop's incoming call bar, and the call screen with its video,
//! participants and controls.

use std::collections::HashMap;

use egui::{Align2, Color32, ColorImage, CornerRadius, Pos2, Rect, RichText, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use langis_core::{CallStatus, CallUpdate, Command, Frame, FrameSlot};

use crate::app::App;
use crate::icons::Icon;
use crate::theme::{self, Palette};
use crate::widgets;

/// What the call screen keeps between frames: video textures, and our
/// microphone and camera as we last set them.
pub struct CallView {
    textures: HashMap<String, (u64, TextureHandle)>,
    pub audio_on: bool,
    pub video_on: Option<bool>,
    /// Desktop's `CallingPip`: the call minimized to a small window over
    /// the app.
    pub pip: bool,
    /// `CallingDeviceSelection`, opened from the header.
    pub settings_open: bool,
    pub layout: Layout,
    pip_snap: PipSnap,
    /// Where the pointer took hold of the window, while it's dragged.
    pip_grab: Option<Vec2>,
    /// The grid's page, and how far down the overflow column is scrolled
    /// (Desktop's `GroupCallOverflowArea` and paginated grid).
    grid_page: usize,
    overflow_from: usize,
}

impl Default for CallView {
    fn default() -> Self {
        Self { textures: HashMap::new(), audio_on: true, video_on: None, pip: false, settings_open: false, layout: Layout::Grid, pip_snap: PipSnap::Right(PIP_TOP_MARGIN), pip_grab: None, grid_page: 0, overflow_from: 0 }
    }
}

/// `CallingPip`'s `PositionMode`: against an edge, at an offset along it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PipSnap {
    Left(f32),
    Right(f32),
    Top(f32),
    Bottom(f32),
}

const PIP_TOP_MARGIN: f32 = 78.;
const PIP_PADDING: f32 = 15.;

impl CallView {
    /// The slot's latest frame as a texture, uploaded only when it's new.
    pub fn texture(&mut self, ctx: &egui::Context, key: &str, slot: &FrameSlot) -> Option<(TextureHandle, Vec2)> {
        let frame: Frame = slot.latest()?;
        let size = Vec2::new(frame.width as f32, frame.height as f32);
        if let Some((seq, texture)) = self.textures.get(key)
            && *seq == frame.seq
        {
            return Some((texture.clone(), size));
        }
        let (w, h) = (frame.width as usize, frame.height as usize);
        if frame.bgra.len() < w * h * 4 {
            return None;
        }
        let pixels = frame.bgra[..w * h * 4].chunks_exact(4).map(|p| Color32::from_rgb(p[2], p[1], p[0])).collect();
        let image = ColorImage::new([w, h], pixels);
        match self.textures.get_mut(key) {
            Some((seq, texture)) => {
                texture.set(image, TextureOptions::LINEAR);
                *seq = frame.seq;
                Some((texture.clone(), size))
            }
            None => {
                let texture = ctx.load_texture(format!("call-{key}"), image, TextureOptions::LINEAR);
                self.textures.insert(key.to_owned(), (frame.seq, texture.clone()));
                Some((texture, size))
            }
        }
    }
}

fn status_text(call: &CallUpdate) -> String {
    match call.status {
        CallStatus::Ringing => if call.video { "Incoming video call" } else { "Incoming voice call" }.into(),
        CallStatus::Calling => "Connecting…".into(),
        CallStatus::RingingThem => "Ringing…".into(),
        CallStatus::Connecting => "Reconnecting…".into(),
        CallStatus::Connected { since } => crate::format::duration((chrono::Utc::now().timestamp_millis() - since) / 1000),
    }
}

/// Desktop's `IncomingCallBar`, along the top of the window.
pub fn incoming_bar(app: &mut App, ui: &mut Ui) {
    let Some(call) = app.call.clone().filter(|c| c.incoming && c.status == CallStatus::Ringing) else { return };
    let palette = Palette::of(ui.ctx());
    let conversation = app.conversation(&call.conversation).cloned();
    let title = conversation.as_ref().map(|c| c.title.clone()).unwrap_or_default();
    let subtitle = match (&call.ringer, call.group) {
        (Some(ringer), true) => format!("{} is calling you", app.name(ringer)),
        _ => if call.video { "Signal video call" } else { "Signal voice call" }.into(),
    };
    egui::Area::new(egui::Id::unique("incoming-call"))
        .anchor(Align2::CENTER_TOP, Vec2::new(0., 12.))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(palette.calling)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::symmetric(16, 12))
                .shadow(egui::Shadow { offset: [0, 4], blur: 20, spread: 0, color: Color32::from_black_alpha(80) })
                .show(ui, |ui| {
                    ui.set_min_width(420.);
                    ui.horizontal(|ui| {
                        if let Some(c) = &conversation {
                            widgets::avatar(ui, &widgets::AvatarSpec::of(c), 40.);
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&title).font(theme::semibold(15.)).color(Color32::WHITE));
                            ui.label(RichText::new(subtitle).font(theme::regular(13.)).color(Color32::from_white_alpha(200)));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if call.video && round_button(ui, Icon::VideoFillLight, palette.green, "Accept with video").clicked() {
                                app.call_view.video_on = Some(true);
                                app.send(Command::AcceptCall { video: true });
                            }
                            if round_button(ui, Icon::PhoneFill, palette.green, "Accept").clicked() {
                                app.call_view.video_on = Some(false);
                                app.send(Command::AcceptCall { video: false });
                            }
                            if round_button(ui, Icon::PhoneDownFillLight, palette.destructive, "Decline").clicked() {
                                app.send(Command::DeclineCall);
                            }
                        });
                    });
                });
        });
}

/// A 40px round button with a white icon.
fn round_button(ui: &mut Ui, icon: Icon, fill: Color32, tip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.), Sense::click());
    let fill = if response.hovered() { fill.gamma_multiply(0.85) } else { fill };
    ui.painter().circle_filled(rect.center(), 20., fill);
    icon.paint(ui, rect, 20., Color32::WHITE);
    response.on_hover_text(tip)
}

/// The call, filling the window as Desktop's `CallScreen` does.
pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(call) = app.call.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let video = app.backend.as_ref().map(|b| b.call_video().clone()).unwrap_or_default();
    let video_on = *app.call_view.video_on.get_or_insert(call.video);
    let conversation = app.conversation(&call.conversation).cloned();
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, palette.calling);
        let stage = Rect::from_min_max(full.min + Vec2::new(16., 64.), full.max - Vec2::new(16., 112.));

        if call.group {
            group_grid(app, ui, &ctx, &call, &video, stage);
        } else {
            let remote = (call.remote_video || call.remote_sharing).then(|| app.call_view.texture(&ctx, "remote", &video.remote)).flatten();
            match remote {
                Some((texture, size)) => {
                    let fit = fit_in(size, stage);
                    egui::Image::new(&texture).corner_radius(CornerRadius::same(12)).paint_at(ui, fit);
                }
                None => {
                    if let Some(c) = &conversation {
                        let rect = Rect::from_center_size(stage.center(), Vec2::splat(112.));
                        widgets::paint_avatar(ui, rect, &widgets::AvatarSpec::of(c));
                    }
                }
            }
        }
        // Our camera, or what we present, in the corner.
        if (video_on || call.sharing)
            && let Some((texture, size)) = app.call_view.texture(&ctx, "local", &video.local)
        {
            let w = 200.;
            let h = w * size.y / size.x.max(1.);
            let rect = Rect::from_min_size(Pos2::new(stage.right() - w - 8., stage.bottom() - h - 8.), Vec2::new(w, h));
            egui::Image::new(&texture).corner_radius(CornerRadius::same(10)).paint_at(ui, rect);
        }

        controls(app, ui, &call, conversation.as_ref(), full, video_on);
        header(app, ui, full);
        crate::calls_tab::in_call_extras(app, ui, full);
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
}

/// A group call's participants in a grid, video where it's on.
/// Desktop's `CallViewMode`: who gets the stage in a group call.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Layout {
    /// `Paginated`: everyone in tiles.
    #[default]
    Grid,
    /// Large tiles for who fits; the rest in a column at the side.
    Sidebar,
    /// The first in line large; everyone else in the column.
    Speaker,
}

impl Layout {
    fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::Sidebar => "Sidebar",
            Self::Speaker => "Speaker",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Sidebar => "grid-sidebar",
            Self::Speaker => "sidebar",
        }
    }
}

/// `GroupCallOverflowArea`'s tiles: 107 wide.
const OVERFLOW_W: f32 = 107.;
/// `LARGE_TILES_MIN_HEIGHT`, for the sidebar layout's grid.
const LARGE_TILE_H: f32 = 200.;

/// One participant's tile: their video, or their avatar, and their name.
fn paint_tile(app: &mut App, ui: &mut Ui, ctx: &egui::Context, p: &langis_core::CallParticipant, video: &langis_core::CallVideo, rect: Rect, small: bool) {
    ui.painter().rect_filled(rect, CornerRadius::same(if small { 8 } else { 12 }), Color32::from_gray(50));
    let name = app.name(&p.aci);
    let frame = (p.video || p.presenting).then(|| app.call_view.texture(ctx, &format!("group-{}", p.demux_id), &video.group.slot(p.demux_id))).flatten();
    match frame {
        Some((texture, size)) if small => paint_cover(ui, &texture, size, rect, CornerRadius::same(8)),
        Some((texture, size)) => {
            egui::Image::new(&texture).corner_radius(CornerRadius::same(12)).paint_at(ui, fit_in(size, rect));
        }
        None => {
            let color = p.aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
            let side = (rect.size().min_elem() * 0.4).min(96.);
            widgets::paint_avatar(ui, Rect::from_center_size(rect.center(), Vec2::splat(side)), &widgets::AvatarSpec::person(&name, color));
        }
    }
    if !small {
        let label = if p.audio { name } else { format!("{name} (muted)") };
        ui.painter().text(rect.left_bottom() + Vec2::new(10., -10.), Align2::LEFT_BOTTOM, label, theme::medium(13.), Color32::WHITE);
    } else if !p.audio {
        theme::paint_symbol(ui.painter(), "mic-slash", rect.left_bottom() + Vec2::new(12., -12.), 14., Color32::WHITE);
    }
}

/// Tiles in a grid of about equal rows and columns.
fn grid_rects(n: usize, area: Rect) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let columns = (n as f32).sqrt().ceil() as usize;
    let rows = n.div_ceil(columns);
    let gap = 8.;
    let tile = Vec2::new((area.width() - gap * (columns - 1) as f32) / columns as f32, (area.height() - gap * (rows - 1) as f32) / rows as f32);
    (0..n).map(|i| Rect::from_min_size(area.min + Vec2::new((i % columns) as f32 * (tile.x + gap), (i / columns) as f32 * (tile.y + gap)), tile)).collect()
}

/// The smallest a grid tile gets before the grid pages.
const MIN_GRID_TILE_H: f32 = 140.;

fn group_grid(app: &mut App, ui: &mut Ui, ctx: &egui::Context, call: &CallUpdate, video: &langis_core::CallVideo, stage: Rect) {
    if call.participants.is_empty() {
        ui.painter().text(stage.center(), Align2::CENTER_CENTER, "No one else is here", theme::regular(15.), Color32::from_white_alpha(200));
        return;
    }
    // `prioritySortedParticipants`: presenters, then the latest speakers.
    let mut people = call.participants.clone();
    people.sort_by(|a, b| b.presenting.cmp(&a.presenting).then(b.speaker_time.unwrap_or(i64::MIN).cmp(&a.speaker_time.unwrap_or(i64::MIN))));
    let layout = if people.len() < 2 { Layout::Grid } else { app.call_view.layout };
    let gap = 8.;
    let main_area = Rect::from_min_max(stage.min, Pos2::new(stage.right() - OVERFLOW_W - gap, stage.bottom()));
    // The grid pages once tiles would be under 140 high.
    let per_page = {
        let rows = ((stage.height() + gap) / (MIN_GRID_TILE_H + gap)).floor().max(1.) as usize;
        let columns = ((stage.width() + gap) / (MIN_GRID_TILE_H * 4. / 3. + gap)).floor().max(1.) as usize;
        (rows * columns).max(1)
    };
    let pages = people.len().div_ceil(per_page);
    if layout == Layout::Grid && pages > 1 {
        app.call_view.grid_page = app.call_view.grid_page.min(pages - 1);
        let from = app.call_view.grid_page * per_page;
        people = people[from..(from + per_page).min(people.len())].to_vec();
    }
    let (large, overflow): (usize, bool) = match layout {
        Layout::Grid => (people.len(), false),
        Layout::Speaker => (1, true),
        Layout::Sidebar => {
            // As many large tiles as fit at 200 high (4:3), the rest beside.
            let rows = ((main_area.height() + gap) / (LARGE_TILE_H + gap)).floor().max(1.) as usize;
            let columns = ((main_area.width() + gap) / (LARGE_TILE_H * 4. / 3. + gap)).floor().max(1.) as usize;
            let fit = (rows * columns).min(people.len());
            (fit, fit < people.len())
        }
    };
    let area = if overflow { main_area } else { stage };
    let mut visible = Vec::new();
    for (p, rect) in people.iter().zip(grid_rects(large, area)) {
        paint_tile(app, ui, ctx, p, video, rect, false);
        visible.push(p.demux_id);
    }
    if overflow {
        // `GroupCallOverflowArea`: the rest, 107 wide, top to bottom,
        // scrolled with its arrows when they don't all fit.
        let tile_h = OVERFLOW_W * 3. / 4.;
        let rest = &people[large..];
        let fits = (((stage.height() - 2. * 28.) + gap) / (tile_h + gap)).floor().max(1.) as usize;
        let scrolls = rest.len() > ((stage.height() + gap) / (tile_h + gap)).floor() as usize;
        let shown = if scrolls { fits } else { rest.len() };
        let from = if scrolls { app.call_view.overflow_from.min(rest.len().saturating_sub(shown)) } else { 0 };
        app.call_view.overflow_from = from;
        let mut y = stage.top() + if scrolls { 28. } else { 0. };
        for p in rest.iter().skip(from).take(shown) {
            let rect = Rect::from_min_size(Pos2::new(stage.right() - OVERFLOW_W, y), Vec2::new(OVERFLOW_W, tile_h));
            paint_tile(app, ui, ctx, p, video, rect, true);
            visible.push(p.demux_id);
            y += tile_h + gap;
        }
        if scrolls {
            let column = Rect::from_min_max(Pos2::new(stage.right() - OVERFLOW_W, stage.top()), stage.max);
            let up = Rect::from_center_size(Pos2::new(column.center().x, column.top() + 12.), Vec2::new(OVERFLOW_W, 24.));
            let down = Rect::from_center_size(Pos2::new(column.center().x, stage.top() + 28. + shown as f32 * (tile_h + gap) + 12.), Vec2::new(OVERFLOW_W, 24.));
            for (r, symbol, step, enabled) in [(up, "chevron-up", -1isize, from > 0), (down, "chevron-down", 1, from + shown < rest.len())] {
                let hit = ui.interact(r, ui.scope_id().with(("overflow", symbol)), Sense::click());
                ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_white_alpha(if hit.hovered() && enabled { 40 } else { 20 }));
                theme::paint_symbol(ui.painter(), symbol, r.center(), 16., Color32::from_white_alpha(if enabled { 230 } else { 80 }));
                if enabled && hit.clicked() {
                    app.call_view.overflow_from = (from as isize + step * shown as isize).max(0) as usize;
                }
            }
        }
    }
    // The grid's pages: arrows either side, and which page.
    if layout == Layout::Grid && pages > 1 {
        let page = app.call_view.grid_page;
        for (x, symbol, step, enabled) in [(stage.left() + 24., "chevron-[start]", -1isize, page > 0), (stage.right() - 24., "chevron-[end]", 1, page + 1 < pages)] {
            let r = Rect::from_center_size(Pos2::new(x, stage.center().y), Vec2::splat(40.));
            let hit = ui.interact(r, ui.scope_id().with(("grid-page", symbol)), Sense::click());
            if !enabled {
                continue;
            }
            ui.painter().circle_filled(r.center(), 20., Color32::from_black_alpha(if hit.hovered() { 200 } else { 140 }));
            theme::paint_symbol(ui.painter(), symbol, r.center(), 20., Color32::WHITE);
            if hit.clicked() {
                app.call_view.grid_page = (page as isize + step) as usize;
            }
        }
        let label = format!("{} of {}", page + 1, pages);
        ui.painter().text(Pos2::new(stage.center().x, stage.bottom() - 10.), Align2::CENTER_BOTTOM, label, theme::regular(12.), Color32::from_white_alpha(200));
    }
    // Ask for video only of who's shown, as Desktop does.
    let main = match layout {
        Layout::Speaker => people.first().map(|p| p.demux_id),
        _ => people.iter().find(|p| p.presenting).map(|p| p.demux_id),
    };
    let want = (visible.clone(), main);
    let key = egui::Id::unique("group-video-shown");
    if ctx.data(|d| d.get_temp::<(Vec<u32>, Option<u32>)>(key)) != Some(want.clone()) {
        ctx.data_mut(|d| d.insert_temp(key, want));
        app.send(Command::ShowGroupVideo { visible, main });
    }
}

/// `size` scaled to fit in `outer`, centered.
fn fit_in(size: Vec2, outer: Rect) -> Rect {
    let scale = (outer.width() / size.x.max(1.)).min(outer.height() / size.y.max(1.));
    Rect::from_center_size(outer.center(), size * scale)
}

/// Fills `rect` with `texture` as `object-fit: cover` does: scaled to fill,
/// cropped evenly.
fn paint_cover(ui: &Ui, texture: &TextureHandle, size: Vec2, rect: Rect, radius: CornerRadius) {
    let (photo, frame) = (size.x / size.y.max(1.), rect.width() / rect.height().max(1.));
    let uv = if photo > frame {
        let keep = frame / photo;
        Rect::from_min_max(Pos2::new((1. - keep) / 2., 0.), Pos2::new((1. + keep) / 2., 1.))
    } else {
        let keep = photo / frame;
        Rect::from_min_max(Pos2::new(0., (1. - keep) / 2.), Pos2::new(1., (1. + keep) / 2.))
    };
    egui::Image::new(texture).uv(uv).corner_radius(radius).paint_at(ui, rect);
}

/// One of `CallingPip`'s 36px round buttons; true when clicked.
fn pip_button(ui: &Ui, id: egui::Id, center: Pos2, icon: Icon, fill: Color32, color: Color32, tip: &str) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(36.));
    let response = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    let fill = if response.hovered() { fill.gamma_multiply(0.85) } else { fill };
    ui.painter().circle_filled(center, 18., fill);
    icon.paint(ui, Rect::from_center_size(center, Vec2::splat(20.)), 20., color);
    response.clicked()
}

/// Desktop's `CallingPip`: the minimized call, over the app. Dragged by
/// the pointer, it snaps to the nearest edge when let go; hovered, it shows
/// Fullscreen and the video, mute and hang up buttons.
pub fn pip(app: &mut App, ctx: &egui::Context) {
    let Some(call) = app.call.clone() else { return };
    if !app.call_view.pip {
        return;
    }
    let palette = Palette::of(ctx);
    let video = app.backend.as_ref().map(|b| b.call_video().clone()).unwrap_or_default();
    let video_on = *app.call_view.video_on.get_or_insert(call.video);
    let audio_on = app.call_view.audio_on;
    let screen = ctx.content_rect();
    let large = screen.width() >= 1200.;
    let width = if large { 224. } else { 160. };
    let mut height = if large { 400. } else { 286. };
    let remote = if call.group {
        call.participants.iter().find(|p| p.video).map(|p| video.group.slot(p.demux_id)).and_then(|slot| app.call_view.texture(ctx, "pip-remote", &slot))
    } else {
        (call.remote_video || call.remote_sharing).then(|| app.call_view.texture(ctx, "remote", &video.remote)).flatten()
    };
    // A portrait picture sets the height, within 1.2 to 2 times the width.
    if let Some((_, size)) = &remote {
        let ratio = size.y / size.x.max(1.);
        if (1.2..=2.).contains(&ratio) {
            height = width * ratio;
        }
    }
    let (min_x, max_x) = (screen.left() + PIP_PADDING, (screen.right() - PIP_PADDING - width).max(screen.left() + PIP_PADDING));
    let (min_y, max_y) = (screen.top() + PIP_TOP_MARGIN, (screen.bottom() - PIP_PADDING - height).max(screen.top() + PIP_TOP_MARGIN));
    let snapped = match app.call_view.pip_snap {
        PipSnap::Left(y) => Pos2::new(min_x, y),
        PipSnap::Right(y) => Pos2::new(max_x, y),
        PipSnap::Top(x) => Pos2::new(x, min_y),
        PipSnap::Bottom(x) => Pos2::new(x, max_y),
    };
    let snapped = Pos2::new(snapped.x.clamp(min_x, max_x), snapped.y.clamp(min_y, max_y));
    let id = egui::Id::unique("calling-pip");
    let pointer = ctx.pointer_latest_pos();
    let target = match (app.call_view.pip_grab, pointer) {
        (Some(grab), Some(pointer)) => pointer - grab,
        _ => snapped,
    };
    // `transform ease-out 300ms`, none while dragged.
    let time = if app.call_view.pip_grab.is_some() { 0. } else { 0.3 };
    let pos = Pos2::new(
        ctx.animate_value_with_time(id.with("x"), target.x, time),
        ctx.animate_value_with_time(id.with("y"), target.y, time),
    );
    let mut toggle_video = false;
    let mut toggle_audio = false;
    let mut hang_up = false;
    let mut full = false;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
        let radius = CornerRadius::same(18);
        let shadow = egui::epaint::Shadow { offset: [0, 8], blur: 20, spread: 0, color: Color32::from_black_alpha(77) };
        ui.painter().add(shadow.as_shape(rect, radius));
        ui.painter().rect_filled(rect, radius, theme::rgb(0x121212));
        match &remote {
            Some((texture, size)) => paint_cover(ui, texture, *size, rect, radius),
            None => {
                if let Some(c) = app.conversation(&call.conversation).cloned() {
                    widgets::paint_avatar(ui, Rect::from_center_size(rect.center(), Vec2::splat(if large { 80. } else { 64. })), &widgets::AvatarSpec::of(&c));
                }
            }
        }
        // Our camera, top left, when there's someone to see besides us.
        let lonely = call.group && call.participants.is_empty();
        if video_on && !lonely
            && let Some((texture, size)) = app.call_view.texture(ctx, "local", &video.local)
        {
            let local = Rect::from_min_size(rect.min + Vec2::splat(8.), if large { Vec2::new(120., 90.) } else { Vec2::new(80., 60.) });
            ui.painter().rect_filled(local, CornerRadius::same(12), theme::rgb(0x2e2e2e));
            paint_cover(ui, &texture, size, local, CornerRadius::same(12));
        }
        // The controls fade in while hovered (`ease-out-local-preview`, 0.5s).
        let hovered = pointer.is_some_and(|p| rect.contains(p)) || app.call_view.pip_grab.is_some();
        let shown = ctx.animate_bool_with_time(id.with("controls"), hovered, 0.5);
        if shown > 0. {
            ui.scope(|ui| {
                ui.multiply_opacity(shown);
                let (gray15, gray60, gray80) = (theme::rgb(0xe9e9e9), theme::rgb(0x5e5e5e), theme::rgb(0x2e2e2e));
                full = pip_button(ui, id.with("full"), Pos2::new(rect.right() - 16. - 18., rect.top() + 16. + 18.), Icon::PipMaximizeLight, gray80, gray15, "Fullscreen call");
                // The actions slide up from below into a bar 4 in from the edges.
                let bottom = rect.bottom() + 60. - (60. + 4.) * shown;
                let bar = Rect::from_min_max(Pos2::new(rect.left() + 4., bottom - 56.), Pos2::new(rect.right() - 4., bottom));
                ui.painter().with_clip_rect(rect).rect_filled(bar, CornerRadius::same(18), theme::rgb(0x343434));
                let y = bar.center().y;
                let (cam_icon, cam_fill, cam_color) = if video_on { (Icon::VideoFillLight, gray60, gray15) } else { (Icon::VideoSlashFillLight, gray15, theme::rgb(0x1b1b1b)) };
                let (mic_icon, mic_fill, mic_color) = if audio_on { (Icon::MicFillLight, gray60, gray15) } else { (Icon::MicSlashFillLight, gray15, theme::rgb(0x1b1b1b)) };
                toggle_video = pip_button(ui, id.with("video"), Pos2::new(bar.center().x - 31. - 18., y), cam_icon, cam_fill, cam_color, if video_on { "Turn off camera" } else { "Turn on camera" });
                toggle_audio = pip_button(ui, id.with("audio"), Pos2::new(bar.center().x, y), mic_icon, mic_fill, mic_color, if audio_on { "Mute" } else { "Unmute" });
                hang_up = pip_button(ui, id.with("hangup"), Pos2::new(bar.center().x + 31. + 18., y), Icon::PhoneDownFillLight, palette.destructive, Color32::WHITE, "Leave call");
            });
        }
        if response.drag_started()
            && let Some(p) = response.interact_pointer_pos()
        {
            app.call_view.pip_grab = Some(p - rect.min);
        }
        response.on_hover_cursor(if app.call_view.pip_grab.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        // Let go: the nearest edge (`handleMouseUp`'s snap candidates).
        if app.call_view.pip_grab.is_some() && !ctx.input(|i| i.pointer.any_down()) {
            app.call_view.pip_grab = None;
            let at = rect.min;
            let candidates = [
                (at.x - screen.left(), PipSnap::Left(at.y)),
                (screen.right() - (at.x + width), PipSnap::Right(at.y)),
                (at.y - (screen.top() + PIP_TOP_MARGIN), PipSnap::Top(at.x)),
                (screen.bottom() - (at.y + height), PipSnap::Bottom(at.x)),
            ];
            if let Some((_, snap)) = candidates.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)) {
                app.call_view.pip_snap = snap;
            }
        }
    });
    if toggle_video {
        app.call_view.video_on = Some(!video_on);
        app.send(Command::SetCallVideo(!video_on));
    }
    if toggle_audio {
        app.call_view.audio_on = !audio_on;
        app.send(Command::SetCallAudio(!audio_on));
    }
    if hang_up {
        app.send(Command::HangUp);
    }
    if full {
        app.call_view.pip = false;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
}

/// Desktop's `CallingDeviceSelection`: an `sm` AxoDialog, "Call Settings",
/// with the camera, microphone and speakers to choose from.
pub fn settings_dialog(app: &mut App, ctx: &egui::Context) {
    // From a call, or from the lobby before one.
    if !app.call_view.settings_open || (app.call.is_none() && app.panes.lobby.is_none()) {
        app.call_view.settings_open = false;
        return;
    }
    let palette = Palette::of(ctx);
    let fill = if palette.dark { Color32::from_rgb(0x32, 0x32, 0x32) } else { Color32::from_rgb(0xfa, 0xfa, 0xfa) };
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("call-settings"))
        .frame(egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(34)).inner_margin(egui::Margin { left: 0, right: 0, top: 12, bottom: 16 }))
        .show(ctx, |ui| {
            let width = 320f32.min(ctx.content_rect().width() - 32.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (head, _) = ui.allocate_exact_size(Vec2::new(width, 32.), Sense::hover());
            ui.painter().text(head.center(), Align2::CENTER_CENTER, "Call Settings", theme::semibold(14.), palette.text);
            let x = Rect::from_center_size(Pos2::new(head.right() - 16. - 16., head.center().y), Vec2::splat(32.));
            let response = ui.interact(x, ui.scope_id().with("close"), Sense::click());
            if response.hovered() {
                ui.painter().circle_filled(x.center(), 16., palette.fill);
            }
            theme::paint_symbol(ui.painter(), "x", x.center(), 20., palette.text);
            close |= response.on_hover_text("Close").clicked();
            ui.add_space(8.);
            // `forceStackedLayout`: each label over its select.
            let devices = app.call_devices.clone();
            let lists = [
                (langis_core::DeviceKind::Camera, "Video", "videocamera", &devices.devices.cameras, &devices.camera),
                (langis_core::DeviceKind::Microphone, "Microphone", "mic", &devices.devices.microphones, &devices.microphone),
                (langis_core::DeviceKind::Speaker, "Speakers", "speaker", &devices.devices.speakers, &devices.speaker),
            ];
            egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 0)).show(ui, |ui| {
                for (kind, label, symbol, list, chosen) in lists {
                    ui.add_space(8.);
                    let (row, _) = ui.allocate_exact_size(Vec2::new(width - 48., 28.), Sense::hover());
                    theme::paint_symbol(ui.painter(), symbol, Pos2::new(row.left() + 10., row.center().y), 20., palette.text);
                    ui.painter().text(Pos2::new(row.left() + 30., row.center().y), Align2::LEFT_CENTER, label, theme::regular(14.), palette.text);
                    let names: Vec<&str> = if list.is_empty() { vec!["No devices available"] } else { list.iter().map(|d| d.name.as_str()).collect() };
                    let index = chosen.as_ref().and_then(|id| list.iter().position(|d| &d.id == id)).unwrap_or(0);
                    ui.add_space(4.);
                    if let Some(i) = crate::axo_list::select_pill(ui, egui::Id::unique("call-settings").with(label), width - 48., &names, index)
                        && let Some(d) = list.get(i)
                    {
                        app.send(Command::SetCallDevice { kind, id: d.id.clone() });
                        app.send(Command::LoadCallDevices);
                    }
                    ui.add_space(4.);
                }
            });
        });
    if close || modal.should_close() {
        app.call_view.settings_open = false;
    }
}

/// Desktop's `CallingHeader`, top right: Call Settings, then Minimize call
/// (into `CallingPip`). Returns where the next button to the left ends.
fn header(app: &mut App, ui: &mut Ui, area: Rect) -> f32 {
    let mut x = area.right() - 16.;
    let mut button = |ui: &mut Ui, symbol: &str, on: bool, tip: &str| {
        let rect = Rect::from_min_size(Pos2::new(x - 40., area.top() + 12.), Vec2::splat(40.));
        x -= 48.;
        let response = ui.interact(rect, ui.scope_id().with(("call-header", tip)), Sense::click()).on_hover_text(tip);
        let fill = if on { Color32::from_white_alpha(70) } else if response.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(20) };
        ui.painter().circle_filled(rect.center(), 20., fill);
        theme::paint_symbol(ui.painter(), symbol, rect.center(), 20., Color32::WHITE);
        response.clicked()
    };
    if button(ui, "pip", false, "Minimize call") {
        app.call_view.pip = true;
        app.panes.participants_open = false;
        app.panes.reactions_open = false;
    }
    if button(ui, "settings", app.call_view.settings_open, "Call Settings") {
        app.call_view.settings_open = true;
        app.send(Command::LoadCallDevices);
    }
    // Change layout, when there are three or more in the call.
    if app.call.as_ref().is_some_and(|c| c.group && c.participants.len() >= 2) {
        let rect = Rect::from_min_size(Pos2::new(x - 40., area.top() + 12.), Vec2::splat(40.));
        x -= 48.;
        let response = ui.interact(rect, ui.scope_id().with("call-layout"), Sense::click()).on_hover_text("Change layout");
        let fill = if response.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(20) };
        ui.painter().circle_filled(rect.center(), 20., fill);
        theme::paint_symbol(ui.painter(), app.call_view.layout.symbol(), rect.center(), 20., Color32::WHITE);
        let current = app.call_view.layout;
        let mut chosen = None;
        crate::menu::dropdown(ui, &response, egui::Id::unique("call-layout-menu"), |m| {
            for layout in [Layout::Grid, Layout::Sidebar, Layout::Speaker] {
                if m.check(current == layout, layout.label()) {
                    chosen = Some(layout);
                }
            }
        });
        if let Some(layout) = chosen {
            app.call_view.layout = layout;
        }
    }
    x
}

/// Desktop's `CallingButton` look: regular (gray-60, gray-15 icon),
/// highlighted (gray-15, gray-90 icon) or disabled (regular at 40%).
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Regular,
    Highlighted,
    Disabled,
}

/// Desktop's `CallControls`: an 80px bar along the bottom, at most 640 wide,
/// with who and the status at the start, the buttons in the middle and
/// End (Leave, in a group) at the end.
fn controls(app: &mut App, ui: &mut Ui, call: &CallUpdate, conversation: Option<&langis_store::Conversation>, full: Rect, video_on: bool) {
    // `CallControls__OuterSpacer`s: the local preview's width and 16 more.
    let spacer = 106.67 + 16.;
    let room = full.width() - 32.;
    let width = (room - 2. * spacer).min(640.).max(480f32.min(room));
    let bar = Rect::from_min_size(Pos2::new(full.center().x - width / 2., full.bottom() - 16. - 80.), Vec2::new(width, 80.));
    let shadow = egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(102) };
    ui.painter().add(shadow.as_shape(bar, CornerRadius::same(18)));
    ui.painter().rect_filled(bar, CornerRadius::same(18), theme::rgb(0x343434));
    let inner = bar.shrink2(Vec2::new(24., 22.));

    // The buttons, 36 across and 20 apart, centered.
    let can_present = app.share_available != Some(false);
    let raised = call.local_demux_id.is_some_and(|d| call.raised_hands.contains(&d));
    let audio_on = app.call_view.audio_on;
    let mut buttons: Vec<(&str, Icon, Look, &str)> = vec![
        ("video", if video_on { Icon::VideoFillLight } else { Icon::VideoSlashFillLight }, if video_on { Look::Regular } else { Look::Highlighted }, if video_on { "Turn off camera" } else { "Turn on camera" }),
        ("audio", if audio_on { Icon::MicFillLight } else { Icon::MicSlashFillLight }, if audio_on { Look::Regular } else { Look::Highlighted }, if audio_on { "Mute mic" } else { "Unmute mic" }),
    ];
    if call.group {
        buttons.push(("hand", Icon::RaiseHandFillLight, if raised { Look::Highlighted } else { Look::Regular }, if raised { "Lower hand" } else { "Raise hand" }));
    }
    buttons.push((
        "present",
        Icon::ShareScreenFillLight,
        if !can_present { Look::Disabled } else if call.sharing { Look::Highlighted } else { Look::Regular },
        if call.sharing { "Stop presenting" } else { "Present" },
    ));
    if call.group {
        buttons.push(("react", Icon::HeartPlusFill, if app.panes.reactions_open { Look::Highlighted } else { Look::Regular }, "React"));
    }
    let row_w = buttons.len() as f32 * 56.;
    let mut x = bar.center().x - row_w / 2. + 10.;
    let mut clicked = None;
    for (key, icon, look, tip) in buttons {
        let rect = Rect::from_min_size(Pos2::new(x, bar.center().y - 18.), Vec2::splat(36.));
        x += 56.;
        let response = ui.interact(rect, ui.scope_id().with(("call-control", key)), if look == Look::Disabled { Sense::hover() } else { Sense::click() }).on_hover_text(tip);
        let (fill, color) = match look {
            Look::Highlighted => (theme::rgb(0xdedede), theme::rgb(0x1b1b1b)),
            Look::Regular => (theme::rgb(0x5e5e5e), theme::rgb(0xdedede)),
            Look::Disabled => (theme::rgb(0x5e5e5e).gamma_multiply(0.4), theme::rgb(0xdedede).gamma_multiply(0.4)),
        };
        let fill = if response.hovered() && look != Look::Disabled { fill.gamma_multiply(0.85) } else { fill };
        ui.painter().circle_filled(rect.center(), 18., fill);
        icon.paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(20.)), 20., color);
        if response.clicked() {
            clicked = Some(key);
        }
    }
    let buttons_left = bar.center().x - row_w / 2.;

    // `CallControls__InfoDisplay`: the title over the status.
    let info_w = (buttons_left - inner.left() - 8.).max(0.);
    let title = conversation.map(|c| c.title.clone()).unwrap_or_default();
    let mut job = egui::text::LayoutJob::simple_singleline(title, theme::semibold(14.), theme::rgb(0xdedede));
    job.wrap = egui::text::TextWrapping::truncate_at_width(info_w);
    let title = ui.fonts_mut(|f| f.layout_job(job));
    let status_y = bar.center().y + 1.;
    ui.painter().galley(Pos2::new(inner.left(), status_y - 2. - title.size().y), title, Color32::WHITE);
    let gray20 = theme::rgb(0xc6c6c6);
    if call.group && matches!(call.status, CallStatus::Connected { .. }) {
        // `CallParticipantCount`: "N people" and a chevron, opening the list.
        let count = call.participants.len() + 1;
        let text = if count == 1 { "1 person".to_owned() } else { format!("{count} people") };
        let galley = ui.painter().layout_no_wrap(text, theme::regular(13.), gray20);
        let rect = Rect::from_min_size(Pos2::new(inner.left(), status_y), Vec2::new(galley.size().x + 15., 18.));
        let response = ui.interact(rect, ui.scope_id().with("participant-count"), Sense::click()).on_hover_text("Participants");
        ui.painter().galley(rect.min, galley, gray20);
        theme::paint_symbol(ui.painter(), "chevron-[end]", Pos2::new(rect.right() - 7., rect.center().y), 14., gray20);
        if response.clicked() {
            app.panes.participants_open = !app.panes.participants_open;
        }
    } else {
        let mut job = egui::text::LayoutJob::simple_singleline(status_text(call), theme::regular(13.), gray20);
        job.wrap = egui::text::TextWrapping::truncate_at_width(info_w);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(Pos2::new(inner.left(), status_y), galley, gray20);
    }

    // `CallControls__JoinLeaveButton--hangup`: a red pill at the end.
    let label = if call.group { "Leave" } else { "End" };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::semibold(12.), Color32::WHITE);
    let pill = Rect::from_min_size(Pos2::new(inner.right() - galley.size().x - 32., bar.center().y - 15.5), Vec2::new(galley.size().x + 32., 31.));
    let response = ui.interact(pill, ui.scope_id().with("hang-up"), Sense::click());
    let red = theme::rgb(0xf44336);
    ui.painter().rect_filled(pill, CornerRadius::same(40), if response.hovered() { red.gamma_multiply(0.85) } else { red });
    ui.painter().galley(pill.center() - galley.size() / 2., galley, Color32::WHITE);
    if response.clicked() {
        app.send(Command::HangUp);
    }

    match clicked {
        Some("video") => {
            app.call_view.video_on = Some(!video_on);
            app.send(Command::SetCallVideo(!video_on));
        }
        Some("audio") => {
            app.call_view.audio_on = !audio_on;
            app.send(Command::SetCallAudio(!audio_on));
        }
        Some("hand") => app.send(Command::RaiseHand(!raised)),
        Some("present") => {
            if call.sharing {
                app.send(Command::StopSharing);
            } else {
                app.share_requested = true;
                app.send(Command::LoadShareSources);
            }
        }
        Some("react") => app.panes.reactions_open = !app.panes.reactions_open,
        _ => {}
    }
}
