//! Signal links from other apps, and one window per account, as Desktop's
//! `requestSingleInstanceLock` and `handleSignalRoute` do.
//!
//! The installed launcher entry claims `sgnl://` and `signalcaptcha://`
//! links, so the browser runs `langis-gui <link>`; the https signal.me,
//! signal.group, signal.link and signal.art links work as an argument too.
//! A running copy listens on a Unix socket (in `$XDG_RUNTIME_DIR`, named
//! for its data directory); a second launch hands it the link and the
//! launcher's Wayland activation token over that socket, and exits. The
//! running copy shows its window (also from the tray), asks the compositor
//! to bring it forward, and opens the link once the account is ready.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use langis_core::routes::{self, Route};

use crate::app::App;

/// What a second launch hands over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// The Signal link it was started with, if any.
    pub url: Option<String>,
    /// `XDG_ACTIVATION_TOKEN` from whoever launched it, so the compositor
    /// lets the running window take focus.
    pub token: Option<String>,
}

impl Request {
    /// The first line of every request; a connection without it (a copy
    /// just checking whether we're here) is no request.
    const HEADER: &str = "langis-gui handoff 1";

    fn encode(&self) -> String {
        let mut out = format!("{}\n", Self::HEADER);
        for (key, value) in [("url", &self.url), ("token", &self.token)] {
            if let Some(value) = value.as_deref().filter(|v| !v.contains(['\n', '\r'])) {
                out.push_str(&format!("{key} {value}\n"));
            }
        }
        out
    }

    fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(Self::HEADER) {
            return None;
        }
        let mut request = Self::default();
        for line in lines {
            match line.split_once(' ') {
                Some(("url", url)) => request.url = Some(url.to_owned()),
                Some(("token", token)) => request.token = Some(token.to_owned()),
                _ => {}
            }
        }
        Some(request)
    }
}

/// The socket for the account in `data_dir`: in `runtime_dir`
/// (`$XDG_RUNTIME_DIR`), named for the directory, or in the data directory
/// itself without one.
pub fn socket_path(runtime_dir: Option<&Path>, data_dir: &Path) -> PathBuf {
    let dir = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_owned());
    match runtime_dir {
        Some(runtime) => runtime.join(format!("langis-gui-{:016x}.sock", fnv1a(dir.as_os_str().as_encoded_bytes()))),
        None => dir.join("langis-gui.sock"),
    }
}

/// The socket for this account, from the environment.
pub fn socket_for(data_dir: &Path) -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_absolute());
    socket_path(runtime.as_deref(), data_dir)
}

/// A stable hash (std's isn't stable across builds, and an old copy may be
/// the one running).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, b| (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// Hands `request` to the copy listening at `path`. An error means no one
/// took it (none running, or a stale socket), so this launch carries on.
pub fn send(path: &Path, request: &Request) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(request.encode().as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = String::new();
    stream.take(16).read_to_string(&mut answer)?;
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::other("the running copy didn't answer"))
    }
}

/// The listening side: requests come out of `requests`.
pub struct Inbox {
    pub requests: Receiver<Request>,
    path: PathBuf,
}

impl Drop for Inbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Listens at `path`; `wake` runs after each request is queued (a repaint).
/// A socket left by a copy that's gone is replaced; one another copy still
/// answers on is an error.
pub fn listen(path: &Path, wake: impl Fn() + Send + 'static) -> std::io::Result<Inbox> {
    if let Some(dir) = path.parent() {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let listener = match UnixListener::bind(path) {
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(e);
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path)?
        }
        other => other?,
    };
    let (tx, requests) = channel();
    std::thread::Builder::new().name("langis-handoff".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            if !same_user(&stream) {
                continue;
            }
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let mut text = String::new();
            if (&mut stream).take(8192).read_to_string(&mut text).is_err() {
                continue;
            }
            let Some(request) = Request::decode(&text) else { continue };
            let _ = stream.write_all(b"ok\n");
            if tx.send(request).is_err() {
                break;
            }
            wake();
        }
    })?;
    Ok(Inbox { requests, path: path.to_owned() })
}

/// Only our own user may hand us links.
fn same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for the call and sized for SO_PEERCRED.
    let ok = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut cred).cast(), &raw mut len) } == 0;
    // SAFETY: getuid can't fail.
    ok && cred.uid == unsafe { libc::getuid() }
}

/// Opens a Signal link as Desktop's `handleSignalRoute` does, reusing what
/// a click on the same link in a message does.
pub fn open(app: &mut App, url: &str) {
    let Some(route) = routes::parse(url) else {
        tracing::warn!("not a Signal link, ignored");
        return;
    };
    match route {
        // The handlers take the web form of each link.
        Route::GroupInvite(code) => crate::chat::open_link(app, &Route::group_link(&code)),
        Route::LinkCall(key) => crate::chat::open_link(app, &Route::call_link(&key)),
        Route::ContactByEncryptedUsername(encrypted) => crate::chat::open_link(app, &Route::username_link(&encrypted)),
        Route::ContactByPhoneNumber(number) => crate::chat::open_link(app, &format!("https://signal.me/#p/{number}")),
        Route::AddStickers { .. } => crate::chat::open_link(app, url),
        Route::ShowWindow => {}
        // Desktop's `challengeHandler.handleCaptcha`: the challenge waiting
        // on it gets its answer; with none waiting there's nothing to do.
        Route::Captcha(captcha) => {
            if app.panes.captcha.is_some() {
                app.panes.captcha = Some(format!("signalcaptcha://{captcha}"));
                app.send(langis_core::Command::SubmitCaptcha(captcha));
            } else {
                tracing::info!("a captcha came with no challenge waiting");
            }
        }
        Route::Unsupported(_) => app.toast(crate::i18n::tr(routes::UNKNOWN_LINK)),
    }
}

/// Desktop's `showConversationViaSignalDotMe`: looks up who a signal.me
/// link names and opens the chat with them; [`found`] takes the answer.
pub fn find_and_open(app: &mut App, url: &str) {
    app.link_lookup = Some(url.to_owned());
    app.send(langis_core::Command::FindRecipient { query: url.to_owned() });
}

/// The answer to [`find_and_open`]'s lookup, if it's this one.
pub fn found(app: &mut App, query: &str, result: &Result<langis_store::DirectoryEntry, String>) -> bool {
    if app.link_lookup.as_deref().is_none_or(|q| q.trim() != query.trim()) {
        return false;
    }
    app.link_lookup = None;
    match result {
        Ok(entry) => {
            app.open(entry.id.clone());
            app.tab = crate::app::Tab::Chats;
        }
        Err(why) => app.toast(why.clone()),
    }
    true
}

/// Brings the window forward on Wayland, where winit can't
/// (`focus_window` does nothing there): `xdg_activation_v1` with the
/// launcher's token, or one of our own when there's none.
pub struct Activator {
    conn: wayland_client::Connection,
    queue: wayland_client::EventQueue<Tokens>,
    activation: XdgActivationV1,
    surface: wayland_client::protocol::wl_surface::WlSurface,
    tokens: Tokens,
}

use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::{self, XdgActivationTokenV1};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

/// The token the compositor gave us, when we asked for one.
#[derive(Default)]
pub struct Tokens(Option<String>);

impl wayland_client::Dispatch<wayland_client::protocol::wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for Tokens {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<XdgActivationV1, ()> for Tokens {
    fn event(_: &mut Self, _: &XdgActivationV1, _: <XdgActivationV1 as wayland_client::Proxy>::Event, _: &(), _: &wayland_client::Connection, _: &wayland_client::QueueHandle<Self>) {}
}

impl wayland_client::Dispatch<XdgActivationTokenV1, ()> for Tokens {
    fn event(state: &mut Self, _: &XdgActivationTokenV1, event: xdg_activation_token_v1::Event, _: &(), _: &wayland_client::Connection, _: &wayland_client::QueueHandle<Self>) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.0 = Some(token);
        }
    }
}

impl Activator {
    /// `None` off Wayland, or without `xdg_activation_v1`.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
        use wayland_client::Proxy;
        let RawDisplayHandle::Wayland(display) = cc.display_handle().ok()?.as_raw() else { return None };
        let RawWindowHandle::Wayland(window) = cc.window_handle().ok()?.as_raw() else { return None };
        // SAFETY: the display is eframe's live connection, which outlives
        // the app (as in `wayland_dnd`).
        let backend = unsafe { wayland_client::backend::Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let conn = wayland_client::Connection::from_backend(backend);
        let (globals, queue) = wayland_client::globals::registry_queue_init::<Tokens>(&conn).ok()?;
        let activation: XdgActivationV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        // SAFETY: the window's surface, alive as long as the window.
        let id = unsafe { wayland_client::backend::ObjectId::from_ptr(wayland_client::protocol::wl_surface::WlSurface::interface(), window.surface.as_ptr().cast()) }.ok()?;
        let surface = wayland_client::protocol::wl_surface::WlSurface::from_id(&conn, id).ok()?;
        Some(Self { conn, queue, activation, surface, tokens: Tokens::default() })
    }

    /// Asks the compositor to focus the window. Without the launcher's
    /// token, ours may be turned down (it carries no input serial).
    pub fn activate(&mut self, token: Option<&str>) {
        let token = match token {
            Some(token) => token.to_owned(),
            None => {
                let ask = self.activation.get_activation_token(&self.queue.handle(), ());
                ask.set_surface(&self.surface);
                ask.commit();
                self.tokens.0 = None;
                let _ = self.queue.roundtrip(&mut self.tokens);
                ask.destroy();
                let Some(token) = self.tokens.0.take() else { return };
                token
            }
        };
        self.activation.activate(token, &self.surface);
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_is_per_data_dir() {
        let runtime = Path::new("/run/user/1000");
        let a = socket_path(Some(runtime), Path::new("/home/a/.local/share/langis-gui"));
        let b = socket_path(Some(runtime), Path::new("/home/a/other"));
        assert_ne!(a, b);
        assert_eq!(a, socket_path(Some(runtime), Path::new("/home/a/.local/share/langis-gui")));
        assert!(a.starts_with(runtime));
        assert_eq!(socket_path(None, Path::new("/data")), Path::new("/data/langis-gui.sock"));
    }

    #[test]
    fn requests_round_trip() {
        let request = Request { url: Some("sgnl://signal.group/#abc".into()), token: Some("t0k".into()) };
        assert_eq!(Request::decode(&request.encode()), Some(request));
        assert_eq!(Request::decode(&Request::default().encode()), Some(Request::default()));
        assert_eq!(Request::decode(""), None);
        // A newline can't smuggle in another field.
        let sneaky = Request { url: Some("x\ntoken evil".into()), token: None };
        assert_eq!(Request::decode(&sneaky.encode()), Some(Request::default()));
    }

    #[test]
    fn hands_over_to_the_running_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = socket_path(Some(dir.path()), Path::new("/some/data"));
        // No one running: the launch carries on.
        assert!(send(&path, &Request::default()).is_err());
        let inbox = listen(&path, || {}).unwrap();
        // A second listener for the same account is refused.
        assert!(listen(&path, || {}).is_err());
        let request = Request { url: Some("https://signal.me/#p/+14155550123".into()), token: None };
        send(&path, &request).unwrap();
        assert_eq!(inbox.requests.recv_timeout(Duration::from_secs(5)).unwrap(), request);
        send(&path, &Request::default()).unwrap();
        assert_eq!(inbox.requests.recv_timeout(Duration::from_secs(5)).unwrap(), Request::default());
        drop(inbox);
        assert!(!path.exists());
    }

    #[test]
    fn replaces_a_stale_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stale.sock");
        // Bound and closed: the file stays, with no one behind it.
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(send(&path, &Request::default()).is_err());
        let inbox = listen(&path, || {}).unwrap();
        send(&path, &Request { url: None, token: Some("x".into()) }).unwrap();
        assert_eq!(inbox.requests.recv_timeout(Duration::from_secs(5)).unwrap().token.as_deref(), Some("x"));
    }
}
