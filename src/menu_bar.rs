//! Desktop's menu bar on Linux (`app/menu.std.ts`): File, Edit, View,
//! Window and Help, hidden by General's Hide menu bar (then Alt shows it).

use egui::{Event, Key, Modifiers, Ui};

use crate::app::{App, Tab};
use crate::i18n::tr;

/// The setting: the menu bar hidden.
pub const SETTING_HIDE_MENU_BAR: &str = "ui.hide_menu_bar";

fn key(ui: &Ui, key: Key, modifiers: Modifiers) {
    ui.ctx().input_mut(|i| {
        i.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    });
}

fn zoom(app: &App, ui: &Ui, factor: f32) {
    let z = factor.clamp(0.5, 3.);
    ui.ctx().set_zoom_factor(z);
    crate::settings::write(app, crate::settings::SETTING_ZOOM, z.to_string());
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let alt = ui.input(|i| i.modifiers.alt && i.keys_down.is_empty());
    if app.hide_menu_bar && !alt && !app.menu_open {
        return;
    }
    let palette = crate::theme::Palette::of(ui.ctx());
    let mut any_open = false;
    egui::Panel::top("menu-bar").exact_size(26.).frame(egui::Frame::new().fill(palette.sidebar).inner_margin(egui::Margin::symmetric(6, 2))).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            let r = ui.menu_button(tr("File"), |ui| {
                if ui.button(tr("Create/upload sticker pack")).clicked() {
                    app.fun.creator = Some(crate::sticker_creator::Creator::new());
                    ui.close();
                }
                if ui.button(tr("Preferences…")).clicked() {
                    app.tab = Tab::Settings;
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Quit Signal").replace("Signal", "langis")).clicked() {
                    app.quit(ui.ctx());
                    ui.close();
                }
            });
            any_open |= r.inner.is_some();
            let r = ui.menu_button(tr("Edit"), |ui| {
                let cmd = Modifiers::COMMAND;
                for (label, k, m) in [(tr("Undo"), Key::Z, cmd), (tr("Redo"), Key::Z, cmd | Modifiers::SHIFT)] {
                    if ui.button(label).clicked() {
                        key(ui, k, m);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("Cut")).clicked() {
                    ui.ctx().input_mut(|i| i.events.push(Event::Cut));
                    ui.close();
                }
                if ui.button(tr("Copy")).clicked() {
                    ui.ctx().input_mut(|i| i.events.push(Event::Copy));
                    ui.close();
                }
                for label in [tr("Paste"), tr("Paste and Match Style")] {
                    if ui.button(label).clicked() {
                        key(ui, Key::V, Modifiers::COMMAND);
                        ui.close();
                    }
                }
                if ui.button(tr("Delete")).clicked() {
                    key(ui, Key::Delete, Modifiers::NONE);
                    ui.close();
                }
                if ui.button(tr("Select All")).clicked() {
                    key(ui, Key::A, Modifiers::COMMAND);
                    ui.close();
                }
            });
            any_open |= r.inner.is_some();
            let r = ui.menu_button(tr("View"), |ui| {
                let now = ui.ctx().zoom_factor();
                if ui.button(tr("Actual Size")).clicked() {
                    zoom(app, ui, 1.);
                    ui.close();
                }
                if ui.button(tr("Zoom In")).clicked() {
                    zoom(app, ui, now + 0.1);
                    ui.close();
                }
                if ui.button(tr("Zoom Out")).clicked() {
                    zoom(app, ui, now - 0.1);
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Toggle Full Screen")).clicked() {
                    let full = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Debug Log")).clicked() {
                    crate::settings::open_debug_log(app);
                    ui.close();
                }
            });
            any_open |= r.inner.is_some();
            let r = ui.menu_button(tr("Window"), |ui| {
                if ui.button(tr("Minimize")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    ui.close();
                }
            });
            any_open |= r.inner.is_some();
            let r = ui.menu_button(tr("Help"), |ui| {
                if ui.button(tr("Show Keyboard Shortcuts")).clicked() {
                    app.panes.shortcuts = true;
                    ui.close();
                }
                ui.separator();
                for (label, url) in [
                    (tr("Contact Us"), "https://support.signal.org/hc/requests/new"),
                    (tr("Go to Forums"), "https://community.signalusers.org/"),
                    (tr("Go to Support Page"), "https://support.signal.org/hc/sections/360001602812"),
                ] {
                    if ui.button(label).clicked() {
                        crate::chat::open_externally(url);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("About")).clicked() {
                    app.settings.help = Some(crate::settings::Help::About);
                    app.tab = Tab::Settings;
                    ui.close();
                }
            });
            any_open |= r.inner.is_some();
        });
    });
    app.menu_open = any_open;
}
