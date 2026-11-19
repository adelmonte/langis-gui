//! langis-gui: a desktop Signal client for Linux, drawn with egui on top of
//! the langis library, laid out like Signal Desktop.

mod actions;
mod app;
mod thumbs;
mod shortcuts;
mod contact_modal;
mod audio;
mod axo_list;
mod media_gallery;
mod call;
mod calls_tab;
mod chat;
mod chat_colors;
mod composer;
mod details;
mod diag;
mod dialog;
mod emoji_picker;
mod format;
mod fun;
mod glyphs;
mod icons;
mod lightbox;
mod menu;
mod notices;
mod panes;
mod polls;
mod profile;
#[cfg(test)]
mod screenshots;
#[cfg(test)]
mod screenshots_b;
mod scroll;
mod search;
mod settings;
mod sidebar;
mod startup;
mod stories;
mod symbols;
mod theme;
mod view;
mod video;
mod voice;
mod widgets;

use std::path::PathBuf;

/// How many people's stories are unread, for the Stories tab's badge.
pub fn stories_unread(app: &app::App) -> u32 {
    let mut whose: Vec<String> = app
        .panes
        .stories
        .list
        .iter()
        .filter(|s| !s.read && s.author.is_some())
        .map(|s| s.group.as_ref().map(|g| g.to_string()).or_else(|| s.author.clone()).unwrap_or_default())
        .collect();
    whose.sort();
    whose.dedup();
    whose.len() as u32
}

#[derive(Default, Clone)]
pub struct Args {
    /// Made-up chats in a scratch directory; never connects to Signal.
    pub demo: bool,
    pub data_dir: Option<PathBuf>,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Self::default();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--demo" => args.demo = true,
                "--data-dir" => args.data_dir = Some(it.next().ok_or("--data-dir needs a directory")?.into()),
                "-h" | "--help" => {
                    println!(
                        "langis-gui [--demo] [--data-dir DIR]\n\n  --demo          made-up chats in a scratch directory, no network\n  --data-dir DIR  where the account lives (default ~/.local/share/langis-gui)"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(args)
    }
}

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .init();
    let args = match Args::parse() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("langis")
            .with_app_id("langis")
            .with_inner_size([1100., 760.])
            .with_min_inner_size([640., 480.])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native("langis", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)))))
}

/// The window's icon (the launcher's comes from the installed theme icon).
fn window_icon() -> std::sync::Arc<egui::IconData> {
    let png = include_bytes!("../assets/app/langis-128.png");
    let icon = image::load_from_memory_with_format(png, image::ImageFormat::Png).map(|image| image.into_rgba8());
    std::sync::Arc::new(match icon {
        Ok(rgba) => egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() },
        Err(_) => egui::IconData::default(),
    })
}
