//! The window's size, place, and maximized and full screen states, kept
//! between runs as Desktop keeps them (`ephemeralConfig`'s `window`), in
//! `window` beside the account (not for the demo).

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowState {
    pub size: [f32; 2],
    /// Where it was; Wayland doesn't tell, nor let it be set.
    pub pos: Option<[f32; 2]>,
    pub maximized: bool,
    pub fullscreen: bool,
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("window")
}

/// "width height maximized fullscreen [x y]".
pub fn parse(text: &str) -> Option<WindowState> {
    let v: Vec<&str> = text.split_whitespace().collect();
    let num = |i: usize| v.get(i).and_then(|s| s.parse::<f32>().ok()).filter(|n| n.is_finite());
    let flag = |i: usize| v.get(i).map(|s| *s == "1");
    let size = [num(0)?, num(1)?];
    if size[0] < 1. || size[1] < 1. {
        return None;
    }
    let pos = num(4).zip(num(5)).map(|(x, y)| [x, y]);
    Some(WindowState { size, pos, maximized: flag(2)?, fullscreen: flag(3)? })
}

pub fn format(s: &WindowState) -> String {
    let b = |on: bool| if on { "1" } else { "0" };
    let mut out = format!("{} {} {} {}", s.size[0].round(), s.size[1].round(), b(s.maximized), b(s.fullscreen));
    if let Some([x, y]) = s.pos {
        out.push_str(&format!(" {} {}", x.round(), y.round()));
    }
    out
}

pub fn load(data_dir: &Path) -> Option<WindowState> {
    parse(&std::fs::read_to_string(path(data_dir)).ok()?)
}

/// Follows the window each frame; the size and place are the ones it had
/// last while neither maximized nor full screen, as Electron's.
pub fn track(state: &mut Option<WindowState>, ctx: &egui::Context) {
    let (inner, outer, maximized, fullscreen, minimized) = ctx.input(|i| {
        let v = i.viewport();
        (v.inner_rect, v.outer_rect, v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false), v.minimized.unwrap_or(false))
    });
    if minimized {
        return;
    }
    let Some(inner) = inner else { return };
    let s = state.get_or_insert(WindowState { size: [inner.width(), inner.height()], pos: None, maximized, fullscreen });
    s.maximized = maximized;
    s.fullscreen = fullscreen;
    if !maximized && !fullscreen {
        s.size = [inner.width(), inner.height()];
        s.pos = outer.map(|r| [r.left(), r.top()]).filter(|p| p[0] != 0. || p[1] != 0.);
    }
}

pub fn save(data_dir: &Path, state: &WindowState) {
    if let Err(error) = std::fs::write(path(data_dir), format(state)) {
        tracing::warn!(%error, "couldn't save the window's state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = WindowState { size: [1200., 800.], pos: Some([10., 20.]), maximized: true, fullscreen: false };
        assert_eq!(parse(&format(&s)), Some(s));
        let s = WindowState { pos: None, ..s };
        assert_eq!(parse(&format(&s)), Some(s));
        assert_eq!(parse("garbage"), None);
        assert_eq!(parse("0 0 0 0"), None);
    }
}
