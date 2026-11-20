//! The window: nav tabs, the chat list and the open chat (or settings),
//! and what floats over them (calls, dialogs, toasts).

use egui::{Align2, CornerRadius, RichText, Ui, Vec2};

use crate::app::{App, Phase, Tab};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

pub fn show(app: &mut App, ui: &mut Ui) {
    // "Now" and "5m" under messages and the chat list's times age while
    // the window sits idle: redraw every 30 seconds to keep them right.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(30));
    // GIF players not drawn for a while are let go.
    crate::video::gc();
    crate::anim::gc();
    let palette = Palette::of(ui.ctx());
    if !matches!(app.phase, Phase::Ready) {
        crate::startup::show(app, ui);
        return;
    }
    crate::shortcuts::handle(app, ui.ctx());
    let full_call = !app.call_view.pip && app.call.as_ref().is_some_and(|c| !(c.incoming && c.status == langis_core::CallStatus::Ringing));
    if full_call {
        crate::call::show(app, ui);
    } else {
        crate::sidebar::nav_tabs(app, ui);
        match app.tab {
            Tab::Chats => {
                crate::sidebar::chat_list(app, ui);
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    ui.painter().rect_filled(ui.max_rect(), 0, palette.background);
                    let details = app.panes.details.as_ref().is_some_and(|d| app.selected.as_ref() == Some(&d.conversation));
                    if details {
                        crate::details::show(app, ui);
                    } else if app.chat.is_some() {
                        crate::chat::show(app, ui);
                    } else {
                        crate::audio::mini_player(app, ui, &palette);
                        empty_chat(ui, &palette);
                    }
                });
            }
            Tab::Calls => crate::calls_tab::show(app, ui),
            Tab::Stories => crate::stories::show(app, ui),
            Tab::Settings => crate::settings::show(app, ui),
        }
        crate::call::incoming_bar(app, ui);
    }
    crate::call::pip(app, ui.ctx());
    crate::call::settings_dialog(app, ui.ctx());
    crate::contact_modal::show(app, ui.ctx());
    crate::shortcuts::guide(app, ui.ctx());
    crate::panes::overlays(app, ui);
    safety_dialog(app, ui);
    share_picker(app, ui);
    crate::panes::delete_chat_dialog(app, ui.ctx());
    crate::panes::chat_confirm_dialog(app, ui.ctx());
    drop_overlay(app, ui);
    crate::panes::custom_time_dialogs(app, ui.ctx());
    crate::chat_colors::editor(app, ui.ctx());
    app.close_dialog(ui.ctx());
    crate::fun::dialogs(app, ui);
    crate::lightbox::show(app, ui);
    toasts(app, ui);
}

/// Desktop's pane when no chat is open.
fn empty_chat(ui: &mut Ui, palette: &Palette) {
    ui.centered_and_justified(|ui| {
        ui.label(RichText::new("Click a chat to start messaging").font(theme::regular(14.)).color(palette.text_secondary));
    });
}

/// Desktop's `SafetyNumberChangeDialog`.
fn safety_dialog(app: &mut App, ui: &mut Ui) {
    let Some(prompt) = &app.safety else { return };
    // While someone's safety number is open from here, the prompt waits
    // under it, its send kept.
    if app.panes.details.as_ref().is_some_and(|d| d.page == crate::details::Page::Safety) {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let people = prompt.people.clone();
    let mut answer = None;
    let mut view: Option<String> = None;
    let modal = egui::Modal::new(egui::Id::unique("safety-number-change"))
        .frame(egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24))
        .show(ui.ctx(), |ui| {
            ui.set_width(360.);
            ui.label(RichText::new(tr("Safety Number Changes")).font(theme::semibold(18.)));
            ui.add_space(8.);
            ui.label(
                RichText::new(
                    "The following people may have reinstalled or changed devices. Verify your safety number with them to ensure privacy.",
                )
                .color(palette.text_secondary),
            );
            ui.add_space(12.);
            for (sid, name) in &people {
                ui.horizontal(|ui| {
                    let color = sid.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                    widgets::avatar(ui, &widgets::AvatarSpec::person(name, color), 36.);
                    ui.label(RichText::new(name).font(theme::medium(14.)));
                    // Desktop's per-person View: their safety number, to verify.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::pill_button(ui, tr("View"), false).clicked() {
                            view = Some(sid.clone());
                        }
                    });
                });
            }
            ui.add_space(16.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::pill_button(ui, tr("Send anyway"), true).clicked() {
                    answer = Some(true);
                }
                if widgets::pill_button(ui, tr("Cancel"), false).clicked() {
                    answer = Some(false);
                }
            });
        });
    if modal.should_close() && answer.is_none() && view.is_none() {
        answer = Some(false);
    }
    if let Some(sid) = view {
        crate::details::open_safety(app, langis_store::ConversationId::direct(&sid));
        return;
    }
    match answer {
        Some(true) => app.send_anyway(),
        Some(false) => app.safety = None,
        None => {}
    }
}

/// When the system offers several things to present, as Desktop's
/// `CallingSelectPresentingSourcesModal`. (Through the desktop portal
/// there's one, and the system shows its own picker.)
fn share_picker(app: &mut App, ui: &mut Ui) {
    let Some(sources) = app.share_sources.clone() else { return };
    let palette = Palette::of(ui.ctx());
    let mut chosen = None;
    let modal = egui::Modal::new(egui::Id::unique("share-sources"))
        .frame(egui::Frame::new().fill(palette.popover).corner_radius(CornerRadius::same(16)).inner_margin(24))
        .show(ui.ctx(), |ui| {
            ui.set_width(360.);
            ui.label(RichText::new(tr("Share your screen")).font(theme::semibold(18.)));
            ui.add_space(12.);
            for source in &sources {
                if ui.selectable_label(false, RichText::new(&source.name).font(theme::regular(14.))).clicked() {
                    chosen = Some(source.id.clone());
                }
            }
            ui.add_space(12.);
            if widgets::pill_button(ui, tr("Cancel"), false).clicked() {
                chosen = Some(String::new());
            }
        });
    if modal.should_close() {
        chosen.get_or_insert_default();
    }
    if let Some(id) = chosen {
        app.share_sources = None;
        if !id.is_empty() {
            app.send(langis_core::Command::StartSharing { source: id });
        }
    }
}

/// Desktop's toasts (`Toast`, `ToastManager`): gray-75 with gray-05
/// body-2 text, at most 280 wide, 16px from the bottom left of the chat
/// list, with a shadow and an optional bold button.
fn toasts(app: &mut App, ui: &mut Ui) {
    let Some(toast) = app.toasts.last() else { return };
    let text = toast.text.clone();
    let action = toast.action.clone();
    let left = if app.nav_collapsed { 0. } else { 80. };
    let mut clicked = false;
    egui::Area::new(egui::Id::unique("toast"))
        .anchor(Align2::LEFT_BOTTOM, Vec2::new(left + 16., -16.))
        .order(egui::Order::Tooltip)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x3b, 0x3b, 0x3b))
                .corner_radius(CornerRadius::same(8))
                .shadow(egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: egui::Color32::from_black_alpha(77) })
                .show(ui, |ui| {
                    ui.set_max_width(280.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.;
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 13)).show(ui, |ui| {
                            ui.set_max_width(if action.is_some() { 180. } else { 248. });
                            ui.label(RichText::new(&text).color(egui::Color32::from_rgb(0xe9, 0xe9, 0xe9)).font(theme::regular(13.)));
                        });
                        if let Some((label, _)) = &action {
                            let b = egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 13)).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(label).color(egui::Color32::from_rgb(0xb0, 0xc8, 0xf9)).font(theme::semibold(13.))).sense(egui::Sense::click()))
                            });
                            clicked = b.inner.clicked();
                        }
                    });
                });
        });
    if clicked && let Some((_, command)) = action {
        app.send(command);
        app.toasts.pop();
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}

/// Desktop's drop zone: while files are dragged over the window and a chat
/// is open, the window says they'll be attached.
fn drop_overlay(app: &App, ui: &Ui) {
    let wayland = app.drops.as_ref().is_some_and(|d| d.hovering());
    let x11 = ui.input(|i| !i.raw.hovered_files.is_empty());
    if !(wayland || x11) || app.chat.is_none() {
        return;
    }
    let palette = Palette::of(ui.ctx());
    let screen = ui.ctx().content_rect();
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop-overlay")));
    painter.rect_filled(screen, 0, palette.background.gamma_multiply(0.85));
    let zone = screen.shrink(24.);
    painter.rect_stroke(zone, CornerRadius::same(16), egui::Stroke::new(2., palette.accent), egui::StrokeKind::Inside);
    painter.text(zone.center(), egui::Align2::CENTER_CENTER, "Drop files here to attach them", theme::semibold(18.), palette.text);
}
