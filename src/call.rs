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
}

impl Default for CallView {
    fn default() -> Self {
        Self { textures: HashMap::new(), audio_on: true, video_on: None }
    }
}

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
