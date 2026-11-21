//! langis-gui: a desktop Signal client for Linux, drawn with egui on top of
//! the langis library, laid out like Signal Desktop.

mod actions;
mod permissions;
mod sticker_creator;
mod anim;
mod app;
mod notification_profiles;
mod thumbs;
mod shortcuts;
mod contact_modal;
mod audio;
mod blurhash;
mod axo_list;
mod avatar_editor;
mod collisions;
mod countries;
mod media_editor;
mod media_gallery;
mod call;
mod call_survey;
mod calls_tab;
mod chat;
mod chat_colors;
mod composer;
mod details;
mod diag;
mod dialog;
mod emoji_picker;
mod export;
mod format;
mod fun;
mod glyphs;
mod i18n;
mod interaction;
mod i18n_locales;
mod icons;
mod lightbox;
mod menu;
mod menu_bar;
mod notices;
mod panes;
mod polls;
mod profile;
#[cfg(test)]
mod screenshots;
#[cfg(test)]
mod screenshots_b;
#[cfg(test)]
mod screenshot_readme;
mod scroll;
mod search;
mod settings;
mod sidebar;
mod sounds;
mod spell;
mod startup;
mod stories;
mod unread_reminders;
mod symbols;
mod theme;
mod tray;
mod view;
mod video;
mod voice;
mod wayland_dnd;
mod widgets;
mod window_state;

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
    /// Start in the background (Desktop's "Start minimized to tray", used
    /// when opened at login).
    pub minimized: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Self::default();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--demo" => args.demo = true,
                "--data-dir" => args.data_dir = Some(it.next().ok_or("--data-dir needs a directory")?.into()),
                "--minimized" => args.minimized = true,
                "-h" | "--help" => {
                    println!(
                        "langis-gui [--demo] [--data-dir DIR] [--minimized]\n\n  --demo          made-up chats in a scratch directory, no network\n  --data-dir DIR  where the account lives (default ~/.local/share/langis-gui)\n  --minimized     start minimized"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(args)
    }
}

/// Logging: the terminal at `RUST_LOG` (warnings by default), and, for a
/// real account, info and up in a file a day under `<data dir>/logs`
/// (three kept, as Desktop keeps three days), for the debug log.
fn init_logging(args: &Args) {
    use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};
    let terminal = tracing_subscriber::fmt::layer()
        .with_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()));
    let file = (!args.demo)
        .then(|| {
            let data_dir = args.data_dir.clone().unwrap_or_else(app::default_data_dir);
            let dir = langis_core::debug_log::log_dir(&data_dir);
            {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).ok()?;
            }
            tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("langis-gui")
                .filename_suffix("log")
                .max_log_files(3)
                .build(&dir)
                .ok()
        })
        .flatten()
        .map(|writer| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(writer))
                .with_filter(tracing_subscriber::EnvFilter::new("info"))
        });
    tracing_subscriber::registry().with(terminal).with(file).init();
}

fn main() -> eframe::Result {
    let args = match Args::parse() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    init_logging(&args);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("langis")
        .with_app_id("langis")
        .with_inner_size([1100., 760.])
        .with_min_inner_size([640., 480.])
        .with_icon(window_icon());
    // The window as it was last time (Desktop's window config).
    if !args.demo
        && let Some(saved) = window_state::load(&args.data_dir.clone().unwrap_or_else(app::default_data_dir))
    {
        viewport = viewport.with_inner_size(saved.size).with_maximized(saved.maximized).with_fullscreen(saved.fullscreen);
        if let Some(pos) = saved.pos {
            viewport = viewport.with_position(pos);
        }
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
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
