//! Desktop's system tray (`SystemTrayService`): an icon in the panel
//! (StatusNotifierItem, which Waybar, KDE and GNOME's AppIndicator show)
//! with the unread count, Show or Hide, and Quit; with Minimize to system
//! tray on, closing the window keeps the app running there.

use std::sync::mpsc::{Receiver, Sender, channel};

use crate::i18n::tr;

/// What the tray asked for.
pub enum TrayEvent {
    Show,
    Hide,
    Quit,
}

struct Tray {
    tx: Sender<TrayEvent>,
    ctx: egui::Context,
    unread: u32,
    shown: bool,
}

impl Tray {
    fn ask(&self, e: TrayEvent) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }
}

/// The app icon as the tray wants it: ARGB32, network byte order.
fn icon() -> Vec<ksni::Icon> {
    let Ok(img) = image::load_from_memory(include_bytes!("../assets/app/langis-64.png")) else { return Vec::new() };
    let img = img.to_rgba8();
    let mut data = Vec::with_capacity(img.as_raw().len());
    for p in img.pixels() {
        data.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
    }
    vec![ksni::Icon { width: img.width() as i32, height: img.height() as i32, data }]
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "langis".into()
    }

    fn title(&self) -> String {
        if self.unread > 0 { format!("langis ({})", self.unread) } else { "langis".into() }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        icon()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: self.title(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.ask(if self.shown { TrayEvent::Hide } else { TrayEvent::Show });
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let toggle = if self.shown { tr("Hide") } else { tr("Show") };
        vec![
            StandardItem {
                label: toggle.into(),
                activate: Box::new(|this: &mut Self| this.ask(if this.shown { TrayEvent::Hide } else { TrayEvent::Show })),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem { label: tr("Quit").into(), activate: Box::new(|this: &mut Self| this.ask(TrayEvent::Quit)), ..Default::default() }.into(),
        ]
    }
}

/// The running tray icon.
pub struct TrayIcon {
    handle: ksni::blocking::Handle<Tray>,
    pub events: Receiver<TrayEvent>,
}

impl TrayIcon {
    /// Puts the icon in the panel; `None` without a tray host on D-Bus.
    pub fn start(ctx: &egui::Context) -> Option<Self> {
        use ksni::blocking::TrayMethods as _;
        let (tx, events) = channel();
        let tray = Tray { tx, ctx: ctx.clone(), unread: 0, shown: true };
        let handle = tray.spawn().ok()?;
        Some(Self { handle, events })
    }

    /// Keeps the count and the Show/Hide item current.
    pub fn update(&self, unread: u32, shown: bool) {
        self.handle.update(|t| {
            t.unread = unread;
            t.shown = shown;
        });
    }

    pub fn stop(&self) {
        let _ = self.handle.shutdown();
    }
}
