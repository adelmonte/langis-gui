//! Renders the app on demo data to PNGs, to compare against Signal Desktop
//! by eye: `cargo test screenshots -- --ignored --nocapture`. Pictures go to
//! `target/screenshots` (or `$LANGIS_GUI_SHOTS`). Needs a Vulkan or GL
//! driver; Mesa's software one (lavapipe) is enough.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;

use crate::app::{App, Phase, Tab};

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("LANGIS_GUI_SHOTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/screenshots"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Steps the app until `done` holds, giving the backend real time.
fn wait(harness: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(harness.state()) {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {what}");
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
    // Let images load and layout settle.
    for _ in 0..30 {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn save(harness: &mut Harness<'_, App>, name: &str) {
    let image = harness.render().expect("rendering");
    let path = out_dir().join(format!("{name}.png"));
    image.save(&path).unwrap();
    println!("wrote {}", path.display());
}

#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200., 800.))
        .with_theme(egui::Theme::Light)
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::new(cc, crate::Args { demo: true, data_dir: None }));
    wait(&mut harness, "the demo account", |app| matches!(app.phase, Phase::Ready) && !app.conversations.is_empty());
    harness.state().ctx.set_theme(egui::ThemePreference::Light);

    // A 1:1 chat and a group, in light and dark.
    let direct = harness.state().conversations.iter().find(|c| c.kind == langis_store::ConversationKind::Direct).map(|c| c.id.clone()).unwrap();
    let group = harness.state().conversations.iter().find(|c| c.kind == langis_store::ConversationKind::Group).map(|c| c.id.clone()).unwrap();
    for (name, id) in [("direct", direct), ("group", group)] {
        harness.state_mut().open(id);
        wait(&mut harness, "messages", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
        save(&mut harness, &format!("{name}-light"));
        harness.state().ctx.set_theme(egui::ThemePreference::Dark);
        wait(&mut harness, "dark theme", |_| true);
        save(&mut harness, &format!("{name}-dark"));
        harness.state().ctx.set_theme(egui::ThemePreference::Light);
    }

    // The composer with text and the emoji picker open.
    if let Some(chat) = harness.state_mut().chat.as_mut() {
        chat.composer = "On my way ❤️".into();
        chat.emoji_open = true;
    }
    wait(&mut harness, "emoji picker", |_| true);
    save(&mut harness, "emoji-picker");
    if let Some(chat) = harness.state_mut().chat.as_mut() {
        chat.composer.clear();
        chat.emoji_open = false;
    }

    // The safety number prompt, for a made-up held send.
    harness.state_mut().safety = Some(crate::app::SafetyPrompt {
        people: vec![("demo-aci".into(), "Maya Chen".into())],
        then: crate::app::PendingSend::Retry(0),
    });
    wait(&mut harness, "safety prompt", |_| true);
    save(&mut harness, "safety-number");
    harness.state_mut().safety = None;

    // Settings.
    harness.state_mut().tab = Tab::Settings;
    harness.state_mut().settings.section = crate::settings::Section::Calls;
    wait(&mut harness, "settings", |app| app.settings.snapshot.is_some());
    save(&mut harness, "settings-calls");
    harness.state_mut().tab = Tab::Chats;

    // A call, from the demo's made-up update.
    let chat = harness.state().conversations.iter().find(|c| c.kind == langis_store::ConversationKind::Direct).map(|c| c.id.clone()).unwrap();
    harness.state_mut().call = Some(langis_core::CallUpdate {
        conversation: chat.clone(),
        status: langis_core::CallStatus::Ringing,
        video: true,
        incoming: true,
        remote_video: false,
        remote_audio: true,
        sharing: false,
        remote_sharing: false,
        group: false,
        participants: Vec::new(),
        ringer: None,
        raised_hands: Vec::new(),
        local_demux_id: None,
        link: None,
    });
    wait(&mut harness, "incoming call", |_| true);
    save(&mut harness, "call-incoming");
    if let Some(call) = harness.state_mut().call.as_mut() {
        call.incoming = false;
        call.status = langis_core::CallStatus::Connected { since: chrono::Utc::now().timestamp_millis() - 65_000 };
    }
    wait(&mut harness, "call", |_| true);
    save(&mut harness, "call");
    harness.state_mut().call = None;

    // The link screen, with a made-up provisioning URL.
    let qr = crate::startup::qr_texture(&harness.state().ctx, "sgnl://linkdevice?uuid=demo-only&pub_key=not-a-real-key");
    harness.state_mut().phase = Phase::Linking { url: Some("demo".into()), error: None, qr };
    wait(&mut harness, "link screen", |_| true);
    save(&mut harness, "link");
    harness.state_mut().phase = Phase::Setup { passphrase: true, first: String::new(), second: String::new(), error: None };
    wait(&mut harness, "setup screen", |_| true);
    save(&mut harness, "setup");

    harness.state_mut().shutdown();
    // Tearing down the software renderer crashes under lavapipe; the
    // pictures are written, so skip it.
    std::mem::forget(harness);
}
