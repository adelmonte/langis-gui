//! Files dropped on the window under Wayland. winit 0.30 hands file drops
//! to egui only on X11, so here the app listens itself: its own event
//! queue on the window's Wayland connection (as eframe's clipboard does),
//! a `wl_data_device` for the seat, and a drop's `text/uri-list` read into
//! paths. Each drop comes out of [`Drops::take`]; while something is
//! dragged over the window, [`Drops::hovering`] is true.

use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

const URI_LIST: &str = "text/uri-list";

/// What the listener found, for the app to pick up each frame.
#[derive(Clone, Default)]
pub struct Drops {
    dropped: Arc<Mutex<Vec<PathBuf>>>,
    hovering: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl Drops {
    /// The files dropped since last asked.
    pub fn take(&self) -> Vec<PathBuf> {
        self.dropped.lock().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default()
    }

    /// Ends the listener and waits for it, before the window's Wayland
    /// connection closes (its thread uses that connection).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = thread.join();
        }
    }

    /// Files are being dragged over the window now.
    pub fn hovering(&self) -> bool {
        self.hovering.load(Ordering::Relaxed)
    }
}

struct State {
    conn: Connection,
    drops: Drops,
    ctx: egui::Context,
    /// The offer being dragged, and whether it has a file list.
    offer: Option<(WlDataOffer, bool)>,
    /// Mime types announced for offers before they're entered.
    pending: Vec<(WlDataOffer, bool)>,
}

/// Starts listening, when the window is on Wayland; `None` on X11 (where
/// egui gets drops itself) or if the compositor has no data devices.
pub fn start(cc: &eframe::CreationContext<'_>) -> Option<Drops> {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    let RawDisplayHandle::Wayland(handle) = cc.display_handle().ok()?.as_raw() else { return None };
    // SAFETY: the display outlives the app: winit keeps it until exit,
    // and this connection's thread dies with the process.
    let backend = unsafe { wayland_client::backend::Backend::from_foreign_display(handle.display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let manager: WlDataDeviceManager = globals.bind(&qh, 1..=3, ()).ok()?;
    let seat: WlSeat = globals.bind(&qh, 1..=1, ()).ok()?;
    let _device = manager.get_data_device(&seat, &qh, ());
    let drops = Drops::default();
    let mut state = State { conn: conn.clone(), drops: drops.clone(), ctx: cc.egui_ctx.clone(), offer: None, pending: Vec::new() };
    let stop = drops.stop.clone();
    let thread = std::thread::Builder::new()
        .name("wayland-dnd".into())
        .spawn(move || {
            let _keep = (manager, seat, _device);
            // Read with a timeout rather than block for good, so `stop`
            // can end it while the connection is still open.
            while !stop.load(Ordering::Relaxed) {
                if queue.dispatch_pending(&mut state).is_err() || conn.flush().is_err() {
                    break;
                }
                let Some(guard) = queue.prepare_read() else { continue };
                let mut fd = libc::pollfd { fd: std::os::fd::AsRawFd::as_raw_fd(&guard.connection_fd()), events: libc::POLLIN, revents: 0 };
                // SAFETY: one pollfd we own, for the connection's live fd.
                let ready = unsafe { libc::poll(&mut fd, 1, 200) };
                if ready > 0 {
                    if guard.read().is_err() {
                        break;
                    }
                } else {
                    drop(guard);
                }
            }
            tracing::debug!("wayland drag and drop listener ended");
        })
        .ok()?;
    if let Ok(mut slot) = drops.thread.lock() {
        *slot = Some(thread);
    }
    tracing::info!("listening for files dropped on the window (Wayland)");
    Some(drops)
}

/// The paths in a `text/uri-list`: `file://` lines, percent-decoded.
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        // `file://host/path`: the host (usually empty or localhost) goes.
        .map(|rest| if rest.starts_with('/') { rest } else { rest.find('/').map_or(rest, |i| &rest[i..]) })
        .map(|path| PathBuf::from(percent_decode(path)))
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads the dropped offer's file list through a pipe, on its own thread
/// (the compositor writes it after we flush), then hands the paths over.
fn read_drop(state: &mut State, offer: WlDataOffer) {
    let mut fds = [0; 2];
    // SAFETY: pipe2 fills both ends on success.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        offer.destroy();
        return;
    }
    // SAFETY: fresh descriptors we own.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    offer.receive(URI_LIST.into(), write.as_fd());
    drop(write);
    let _ = state.conn.flush();
    let (drops, ctx) = (state.drops.clone(), state.ctx.clone());
    std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        // A file list is small; a source that sends more is cut off.
        let _ = std::fs::File::from(read).take(1 << 20).read_to_string(&mut text);
        let paths = parse_uri_list(&text);
        if !paths.is_empty()
            && let Ok(mut dropped) = drops.dropped.lock()
        {
            dropped.extend(paths);
        }
        ctx.request_repaint();
    });
    if offer.version() >= 3 {
        offer.finish();
    }
    offer.destroy();
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(_: &mut Self, _: &WlDataDeviceManager, _: <WlDataDeviceManager as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(state: &mut Self, offer: &WlDataOffer, event: wl_data_offer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && mime_type == URI_LIST
        {
            if let Some((o, files)) = state.offer.as_mut().filter(|(o, _)| o == offer) {
                let _ = o;
                *files = true;
            } else if let Some((_, files)) = state.pending.iter_mut().find(|(o, _)| o == offer) {
                *files = true;
            }
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(state: &mut Self, _: &WlDataDevice, event: wl_data_device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.pending.retain(|(o, _)| o.is_alive());
                state.pending.push((id, false));
            }
            wl_data_device::Event::Enter { serial, id, .. } => {
                let Some(offer) = id else { return };
                let files = state.pending.iter().find(|(o, _)| *o == offer).is_some_and(|(_, f)| *f);
                state.pending.retain(|(o, _)| *o != offer);
                if files {
                    offer.accept(serial, Some(URI_LIST.into()));
                    if offer.version() >= 3 {
                        offer.set_actions(DndAction::Copy, DndAction::Copy);
                    }
                    state.drops.hovering.store(true, Ordering::Relaxed);
                    state.ctx.request_repaint();
                } else {
                    offer.accept(serial, None);
                }
                state.offer = Some((offer, files));
            }
            wl_data_device::Event::Leave => {
                if let Some((offer, _)) = state.offer.take() {
                    offer.destroy();
                }
                state.drops.hovering.store(false, Ordering::Relaxed);
                state.ctx.request_repaint();
            }
            wl_data_device::Event::Drop => {
                state.drops.hovering.store(false, Ordering::Relaxed);
                if let Some((offer, files)) = state.offer.take() {
                    if files {
                        read_drop(state, offer);
                    } else {
                        offer.destroy();
                    }
                }
            }
            // A selection (the clipboard) isn't ours to read here.
            wl_data_device::Event::Selection { id: Some(offer) } => {
                state.pending.retain(|(o, _)| *o != offer);
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri_lists_become_paths() {
        let list = "file:///home/me/Pictures/trail%20map.png\r\n# a comment\r\nfile://localhost/tmp/notes.txt\r\nhttps://example.org/x\r\n";
        let paths = super::parse_uri_list(list);
        assert_eq!(paths, [std::path::PathBuf::from("/home/me/Pictures/trail map.png"), std::path::PathBuf::from("/tmp/notes.txt")]);
    }
}
