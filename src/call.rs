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
    pip_snap: PipSnap,
    /// Where the pointer took hold of the window, while it's dragged.
    pip_grab: Option<Vec2>,
}

impl Default for CallView {
    fn default() -> Self {
        Self { textures: HashMap::new(), audio_on: true, video_on: None, pip: false, settings_open: false, pip_snap: PipSnap::Right(PIP_TOP_MARGIN), pip_grab: None }
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
    fn texture(&mut self, ctx: &egui::Context, key: &str, slot: &FrameSlot) -> Option<(TextureHandle, Vec2)> {
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

        // Who and how long, at the top.
        let title = conversation.as_ref().map(|c| c.title.clone()).unwrap_or_default();
        ui.painter().text(Pos2::new(full.center().x, full.top() + 20.), Align2::CENTER_TOP, &title, theme::semibold(16.), Color32::WHITE);
        ui.painter().text(Pos2::new(full.center().x, full.top() + 42.), Align2::CENTER_TOP, status_text(&call), theme::regular(13.), Color32::from_white_alpha(190));

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

        // Desktop's call controls, in a bar at the bottom.
        let can_present = app.share_available != Some(false);
        let buttons = if can_present { 4. } else { 3. };
        let bar = Rect::from_center_size(Pos2::new(full.center().x, full.bottom() - 56.), Vec2::new(buttons * 56. + (buttons - 1.) * 16. + 32., 72.));
        ui.painter().rect_filled(bar, CornerRadius::same(36), Color32::from_white_alpha(14));
        let mut x = bar.left() + 16.;
        let mut button = |icon: Icon, on: bool, fill: Option<Color32>, tip: &str| {
            let rect = Rect::from_min_size(Pos2::new(x, bar.top() + 8.), Vec2::splat(56.));
            x += 56. + 16.;
            let response = ui.interact(rect, ui.scope_id().with(("call-control", tip)), Sense::click()).on_hover_text(tip);
            let (bg, fg) = match fill {
                Some(fill) => (fill, Color32::WHITE),
                None if on => (Color32::from_white_alpha(40), Color32::WHITE),
                None => (Color32::WHITE, Color32::BLACK),
            };
            let bg = if response.hovered() { bg.gamma_multiply(0.85) } else { bg };
            ui.painter().circle_filled(rect.center(), 28., bg);
            icon.paint(ui, rect, 24., fg);
            response.clicked()
        };
        let present = can_present
            && button(Icon::ShareScreenFillLight, !call.sharing, call.sharing.then_some(palette.accent), if call.sharing { "Stop presenting" } else { "Present" });
        let camera = button(if video_on { Icon::VideoFillLight } else { Icon::VideoSlashFillLight }, video_on, None, if video_on { "Turn off camera" } else { "Turn on camera" });
        let audio_on = app.call_view.audio_on;
        let mic = button(if audio_on { Icon::MicFillLight } else { Icon::MicSlashFillLight }, audio_on, None, if audio_on { "Mute" } else { "Unmute" });
        let hang_up = button(Icon::PhoneDownFillLight, true, Some(palette.destructive), "Leave call");
        if present {
            if call.sharing {
                app.send(Command::StopSharing);
            } else {
                app.share_requested = true;
                app.send(Command::LoadShareSources);
            }
        }
        if camera {
            app.call_view.video_on = Some(!video_on);
            app.send(Command::SetCallVideo(!video_on));
        }
        if mic {
            app.call_view.audio_on = !audio_on;
            app.send(Command::SetCallAudio(!audio_on));
        }
        if hang_up {
            app.send(Command::HangUp);
        }
        let x = header(app, ui, full);
        crate::calls_tab::in_call_extras(app, ui, full, x);
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
}

/// A group call's participants in a grid, video where it's on.
fn group_grid(app: &mut App, ui: &mut Ui, ctx: &egui::Context, call: &CallUpdate, video: &langis_core::CallVideo, stage: Rect) {
    let people = &call.participants;
    if people.is_empty() {
        ui.painter().text(stage.center(), Align2::CENTER_CENTER, "No one else is here", theme::regular(15.), Color32::from_white_alpha(200));
        return;
    }
    let n = people.len();
    let columns = (n as f32).sqrt().ceil() as usize;
    let rows = n.div_ceil(columns);
    let gap = 8.;
    let tile = Vec2::new((stage.width() - gap * (columns - 1) as f32) / columns as f32, (stage.height() - gap * (rows - 1) as f32) / rows as f32);
    let mut visible = Vec::new();
    for (i, p) in people.iter().enumerate() {
        let (col, row) = (i % columns, i / columns);
        let rect = Rect::from_min_size(stage.min + Vec2::new(col as f32 * (tile.x + gap), row as f32 * (tile.y + gap)), tile);
        ui.painter().rect_filled(rect, CornerRadius::same(12), Color32::from_gray(50));
        let name = app.name(&p.aci);
        let frame = (p.video || p.presenting).then(|| app.call_view.texture(ctx, &format!("group-{}", p.demux_id), &video.group.slot(p.demux_id))).flatten();
        match frame {
            Some((texture, size)) => {
                egui::Image::new(&texture).corner_radius(CornerRadius::same(12)).paint_at(ui, fit_in(size, rect));
            }
            None => {
                let color = p.aci.bytes().fold(0u8, |h, b| h.wrapping_mul(31).wrapping_add(b));
                let side = (tile.min_elem() * 0.4).min(96.);
                widgets::paint_avatar(ui, Rect::from_center_size(rect.center(), Vec2::splat(side)), &widgets::AvatarSpec::person(&name, color));
            }
        }
        let label = if p.audio { name } else { format!("{name} (muted)") };
        ui.painter().text(rect.left_bottom() + Vec2::new(10., -10.), Align2::LEFT_BOTTOM, label, theme::medium(13.), Color32::WHITE);
        visible.push(p.demux_id);
    }
    // Ask for video only of who's shown, as Desktop does.
    let main = people.iter().find(|p| p.presenting).map(|p| p.demux_id);
    let want = (visible.clone(), main);
    let key = egui::Id::unique("group-video-shown");
    if ctx.data(|d| d.get_temp::<(Vec<u32>, Option<u32>)>(key)) != Some(want.clone()) {
        ctx.data_mut(|d| d.insert_temp(key, want));
        app.send(Command::ShowGroupVideo { visible, main });
    }
    let _ = RichText::new("");
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
    if !app.call_view.settings_open || app.call.is_none() {
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
    x
}
