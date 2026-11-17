//! `LANGIS_DEBUG_REPAINT=1`: once a second, prints to stderr how many frames
//! were drawn, why egui repainted, which kinds of backend events arrived
//! (names only), and the process's memory. Nothing else is logged.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub struct Diag {
    on: bool,
    since: Instant,
    frames: u32,
    loads: u32,
    causes: BTreeMap<String, u32>,
    events: BTreeMap<String, u32>,
}

impl Default for Diag {
    fn default() -> Self {
        Diag {
            on: std::env::var_os("LANGIS_DEBUG_REPAINT").is_some_and(|v| !v.is_empty() && v != "0"),
            since: Instant::now(),
            frames: 0,
            loads: 0,
            causes: BTreeMap::new(),
            events: BTreeMap::new(),
        }
    }
}

impl Diag {
    pub fn event(&mut self, event: &langis_core::Event) {
        if self.on {
            let text = format!("{event:?}");
            let name: String = text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            *self.events.entry(name).or_default() += 1;
        }
    }

    pub fn loaded(&mut self) {
        if self.on {
            self.loads += 1;
        }
    }

    pub fn frame(&mut self, ctx: &egui::Context) {
        if !self.on {
            return;
        }
        self.frames += 1;
        for cause in ctx.repaint_causes() {
            let text = cause.to_string();
            let short = text.find(".crates.io-").and_then(|i| text[i..].find('/').map(|j| &text[i + j + 1..])).unwrap_or(&text);
            *self.causes.entry(short.trim().to_owned()).or_default() += 1;
        }
        if self.since.elapsed() < Duration::from_secs(1) {
            return;
        }
        let rss = std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).map(|pages| pages * 4096 / (1 << 20));
        let textures = ctx.tex_manager().read().allocated().count();
        eprintln!(
            "[repaint] {} frames, {} loads, rss {} MiB, {} textures | causes {:?} | events {:?}",
            self.frames,
            self.loads,
            rss.map_or("?".into(), |m| m.to_string()),
            textures,
            self.causes,
            self.events
        );
        self.since = Instant::now();
        self.frames = 0;
        self.loads = 0;
        self.causes.clear();
        self.events.clear();
    }
}
