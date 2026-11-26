//! Hiding to the notification area on KWin, KDE Plasma's compositor, in a
//! Wayland session. Wayland has no request to unmap a toplevel or to undo a
//! minimize, so the app cannot take its window off the taskbar and put it
//! back itself. KWin's scripting can: `skipTaskbar` and `minimized` on the
//! window, and `activeWindow` to bring it forward again.

use std::sync::atomic::{AtomicU32, Ordering};

/// Whether this is a Plasma session. The call itself then finds out whether
/// KWin answers and has scripting on.
fn on_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| desktops.split(':').any(|desktop| desktop.eq_ignore_ascii_case("KDE")))
}

/// Takes the app's windows off the taskbar and minimizes them, or brings
/// the main one back, unminimized and focused. `false` when KWin could not
/// be asked, for the caller's own fallback.
pub fn set_hidden(hidden: bool) -> bool {
    if !on_plasma() {
        return false;
    }
    ask_kwin(&script(hidden))
}

/// The script for one hide or show. The pid finds the app's windows; the
/// main one is the window whose caption starts with the app's name.
fn script(hidden: bool) -> String {
    let pid = std::process::id();
    if hidden {
        format!(
            "for (const w of workspace.windowList()) {{
    if (w.pid === {pid}) {{
        w.skipTaskbar = true;
        w.minimized = true;
    }}
}}
"
        )
    } else {
        format!(
            "let main = null;
for (const w of workspace.windowList()) {{
    if (w.pid === {pid}) {{
        w.skipTaskbar = false;
        w.minimized = false;
        if (w.caption.indexOf(\"langis\") === 0) {{
            main = w;
        }}
    }}
}}
if (main !== null) {{
    workspace.activeWindow = main;
}}
"
        )
    }
}

/// Loads the script through KWin's scripting D-Bus, runs it once, and
/// unloads it again. The effects stay after the unload.
fn ask_kwin(script: &str) -> bool {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let Ok(conn) = zbus::blocking::Connection::session() else { return false };
    let Ok(scripting) = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting") else { return false };
    let Ok(file) = tempfile::Builder::new().prefix("langis-kwin-").suffix(".js").tempfile() else { return false };
    if std::io::Write::write_all(&mut file.as_file(), script.as_bytes()).is_err() {
        return false;
    }
    let Some(path) = file.path().to_str() else { return false };
    let name = format!("langis-tray-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let Ok(id) = scripting.call::<_, _, i32>("loadScript", &(path, name.as_str())) else { return false };
    let object = format!("/Scripting/Script{id}");
    let Ok(script) = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", object.as_str(), "org.kde.kwin.Script") else { return false };
    let ran = script.call::<_, _, ()>("run", &()).is_ok();
    let _: Result<bool, _> = scripting.call("unloadScript", &(name.as_str(),));
    ran
}
