//! Desktop's system tray (`SystemTrayService`): an icon in the panel
//! (StatusNotifierItem, which Waybar, KDE and GNOME's AppIndicator show)
//! with the unread count, Open window and Quit; with Minimize to system
//! tray on, closing the window keeps the app running there.

use std::sync::mpsc::{Receiver, Sender, channel};

use crate::i18n::tr;

/// What the tray asked for.
pub enum TrayEvent {
    Open,
    Quit,
}

struct Tray {
    tx: Sender<TrayEvent>,
    ctx: egui::Context,
    unread: u32,
}

impl Tray {
    fn ask(&self, e: TrayEvent) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }
}

/// The app icon as the tray wants it: ARGB32, network byte order, in
/// several sizes so panels can pick one for their scale.
fn icons() -> Vec<ksni::Icon> {
    const SIZES: [(&[u8], u32); 6] = [
        (include_bytes!("../assets/app/langis-16.png"), 16),
        (include_bytes!("../assets/app/langis-32.png"), 32),
        (include_bytes!("../assets/app/langis-48.png"), 48),
        (include_bytes!("../assets/app/langis-64.png"), 64),
        (include_bytes!("../assets/app/langis-128.png"), 128),
        (include_bytes!("../assets/app/langis-256.png"), 256),
    ];
    SIZES
        .iter()
        .filter_map(|(png, size)| {
            let img = image::load_from_memory(png).ok()?.to_rgba8();
            let mut data = Vec::with_capacity(img.as_raw().len());
            for p in img.pixels() {
                // The protocol's ARGB32, in network byte order.
                data.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
            }
            Some(ksni::Icon { width: *size as i32, height: *size as i32, data })
        })
        .collect()
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "langis".into()
    }

    fn title(&self) -> String {
        if self.unread > 0 { format!("langis ({})", self.unread) } else { "langis".into() }
    }

    fn icon_name(&self) -> String {
        // The installed theme icon (Settings' hicolor); the pixmaps cover a
        // run from the build directory.
        "langis".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        icons()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: self.title(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.ask(TrayEvent::Open);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem { label: tr("Open window").into(), activate: Box::new(|this: &mut Self| this.ask(TrayEvent::Open)), ..Default::default() }.into(),
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
        let tray = Tray { tx, ctx: ctx.clone(), unread: 0 };
        let handle = tray.spawn().ok()?;
        Some(Self { handle, events })
    }

    /// Keeps the count current.
    pub fn update(&self, unread: u32) {
        self.handle.update(|t| t.unread = unread);
    }

    pub fn stop(&self) {
        let _ = self.handle.shutdown();
    }
}
