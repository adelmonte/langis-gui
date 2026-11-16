//! langis-gui: a desktop Signal client for Linux, drawn with egui on top of
//! the langis library, laid out like Signal Desktop.

mod app;
mod call;
mod chat;
mod emoji_picker;
mod format;
mod icons;
mod notices;
#[cfg(test)]
mod screenshots;
mod settings;
mod sidebar;
mod startup;
mod theme;
mod view;
mod widgets;

use std::path::PathBuf;

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
            .with_title("langis-gui")
            .with_app_id("langis-gui")
            .with_inner_size([1100., 760.])
            .with_min_inner_size([640., 480.]),
        ..Default::default()
    };
    eframe::run_native("langis-gui", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)))))
}
