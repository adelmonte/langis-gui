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
    harness.state_mut().settings.page = crate::settings::Page::Calls;
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
    // Minimized: the chats, with the call in a corner, its controls shown.
    harness.state_mut().call_view.pip = true;
    for _ in 0..30 {
        harness.step();
    }
    harness.hover_at(egui::pos2(1100., 200.));
    for _ in 0..40 {
        harness.step();
    }
    save(&mut harness, "call-pip");
    harness.state_mut().call = None;
    harness.state_mut().call_view = Default::default();

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

fn demo_harness() -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200., 800.))
        .with_theme(egui::Theme::Light)
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::new(cc, crate::Args { demo: true, data_dir: None }));
    wait(&mut harness, "the demo account", |app| matches!(app.phase, Phase::Ready) && !app.conversations.is_empty());
    harness.state().ctx.set_theme(egui::ThemePreference::Light);
    harness
}

fn open_chat(harness: &mut Harness<'_, App>, kind: langis_store::ConversationKind) -> langis_store::ConversationId {
    let id = harness.state().conversations.iter().find(|c| c.kind == kind).map(|c| c.id.clone()).unwrap();
    harness.state_mut().open(id.clone());
    wait(harness, "messages", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
    id
}

/// The last message matching `pick` in the open chat.
fn message_id(harness: &Harness<'_, App>, pick: impl Fn(&langis_store::Message) -> bool) -> i64 {
    harness.state().chat.as_ref().unwrap().messages.iter().rev().find(|m| pick(m)).map(|m| m.id).expect("a matching message")
}

/// Every message of a chat, oldest first, straight from the store.
fn all_messages(harness: &Harness<'_, App>, chat: &langis_store::ConversationId) -> Vec<langis_store::Message> {
    let backend = harness.state().backend.as_ref().unwrap();
    let (db, id) = (backend.db().clone(), chat.clone());
    futures::executor::block_on(backend.spawn(async move { db.messages(&id, None, 1000).await.unwrap() }))
}

fn finish(mut harness: Harness<'_, App>) {
    harness.state_mut().shutdown();
    std::mem::forget(harness);
}

/// Message actions: the menu, reaction bar, dialogs and pinned bar.
#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_message_actions() {
    use crate::actions::{Dialog, Popup};
    let mut harness = demo_harness();
    open_chat(&mut harness, langis_store::ConversationKind::Direct);
    let theirs = message_id(&harness, |m| m.direction == langis_store::Direction::Incoming && m.notice.is_none() && !m.body.is_empty());
    let ours = message_id(&harness, |m| m.direction == langis_store::Direction::Outgoing && m.notice.is_none() && !m.deleted);

    harness.state_mut().chat.as_mut().unwrap().popup = Some(Popup::Menu { message: ours, at: egui::pos2(700., 420.) });
    wait(&mut harness, "menu", |_| true);
    save(&mut harness, "menu");

    harness.state_mut().chat.as_mut().unwrap().popup = Some(Popup::Reactions { message: theirs, at: egui::pos2(420., 500.), full: false, picker: Default::default() });
    wait(&mut harness, "reaction bar", |_| true);
    save(&mut harness, "reaction-bar");
    harness.state_mut().chat.as_mut().unwrap().popup = None;

    harness.state_mut().chat.as_mut().unwrap().dialog = Some(Dialog::Delete { messages: vec![ours] });
    wait(&mut harness, "delete dialog", |_| true);
    save(&mut harness, "delete-dialog");

    harness.state_mut().chat.as_mut().unwrap().dialog = Some(Dialog::Pin { message: theirs, choice: 1 });
    wait(&mut harness, "pin dialog", |_| true);
    save(&mut harness, "pin-dialog");

    harness.state_mut().chat.as_mut().unwrap().dialog = Some(Dialog::Info { message: ours, details: None });
    crate::actions::load_details(harness.state(), ours);
    wait(&mut harness, "info dialog", |app| {
        matches!(app.chat.as_ref().and_then(|c| c.dialog.as_ref()), Some(Dialog::Info { details: Some(_), .. }))
    });
    save(&mut harness, "info-dialog");

    harness.state_mut().chat.as_mut().unwrap().dialog = Some(Dialog::Forward { messages: vec![theirs], query: String::new(), chosen: Vec::new() });
    wait(&mut harness, "forward dialog", |_| true);
    save(&mut harness, "forward-dialog");
    harness.state_mut().chat.as_mut().unwrap().dialog = None;

    harness.state_mut().chat.as_mut().unwrap().selected = Some(vec![theirs, ours]);
    wait(&mut harness, "select mode", |_| true);
    save(&mut harness, "select-mode");
    harness.state_mut().chat.as_mut().unwrap().dialog = Some(Dialog::Delete { messages: vec![theirs, ours] });
    wait(&mut harness, "delete selected", |_| true);
    save(&mut harness, "select-mode-delete");
    let chat = harness.state_mut().chat.as_mut().unwrap();
    chat.dialog = None;
    chat.selected = None;

    harness.state().send(langis_core::Command::PinMessage { message_id: theirs, duration: None });
    wait(&mut harness, "pinned bar", |app| app.chat.as_ref().is_some_and(|c| !c.pinned.is_empty()));
    let ours_msg = harness.state().chat.as_ref().unwrap().messages.iter().find(|m| m.id == ours).cloned().unwrap();
    let app = harness.state_mut();
    let mut chat = app.chat.take().unwrap();
    chat.start_editing(app, ours_msg);
    app.chat = Some(chat);
    wait(&mut harness, "editing", |_| true);
    save(&mut harness, "pinned-and-editing");
    finish(harness);
}

/// The photo viewer, and a view-once message opened in it.
#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_media() {
    let mut harness = demo_harness();
    let chat = open_chat(&mut harness, langis_store::ConversationKind::Direct);
    // Older pages may hold them: load around the view-once message.
    let once = all_messages(&harness, &chat).iter().find(|m| m.view_once.is_some()).map(|m| m.id).expect("a view-once message");
    harness.state_mut().open_at(chat.clone(), once);
    wait(&mut harness, "view-once bubble", |app| app.chat.as_ref().is_some_and(|c| c.messages.iter().any(|m| m.id == once)));
    save(&mut harness, "view-once-bubble");

    let notes = open_chat(&mut harness, langis_store::ConversationKind::NoteToSelf);
    let m = all_messages(&harness, &notes)
        .into_iter()
        .rev()
        .find(|m| m.attachments.iter().any(|a| a.content_type.starts_with("image/") && !a.sticker) && m.view_once.is_none())
        .expect("a photo");
    harness.state_mut().open_lightbox(&m, 0);
    wait(&mut harness, "lightbox", |app| app.lightbox.as_ref().is_some_and(|l| l.items.len() > 1));
    save(&mut harness, "lightbox");
    harness.state_mut().close_lightbox();

    harness.state_mut().open_at(chat.clone(), once);
    wait(&mut harness, "back to the view-once chat", |app| app.chat.as_ref().is_some_and(|c| c.messages.iter().any(|m| m.id == once)));
    harness.state().send(langis_core::Command::OpenViewOnce { message_id: once });
    wait(&mut harness, "view-once viewer", |app| app.lightbox.as_ref().is_some_and(|l| l.view_once.is_some()));
    save(&mut harness, "view-once-open");
    harness.state_mut().close_lightbox();
    wait(&mut harness, "viewed", |app| app.chat.as_ref().is_some_and(|c| c.messages.iter().any(|m| m.id == once && m.view_once == Some(langis_store::ViewOnce::Viewed))));
    save(&mut harness, "view-once-viewed");
    finish(harness);
}

/// The composer: @mentions, the formatting toolbar, styled text, a link
/// preview, the attachment menu and the poll dialog.
#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_composer() {
    let mut harness = demo_harness();
    let group = open_chat(&mut harness, langis_store::ConversationKind::Group);
    let id = crate::composer::input_id(&group);
    let ctx = harness.state().ctx.clone();

    harness.state_mut().chat.as_mut().unwrap().composer = "Who has the map? @".into();
    wait(&mut harness, "composer", |_| true);
    crate::composer::set_cursor(&ctx, id, 18);
    wait(&mut harness, "mention list", |_| true);
    save(&mut harness, "composer-mentions");

    // Pick the first suggestion, then format a word.
    harness.key_press(egui::Key::Enter);
    wait(&mut harness, "mention", |app| app.chat.as_ref().is_some_and(|c| !c.spans.mentions.is_empty()));
    let text = {
        let chat = harness.state_mut().chat.as_mut().unwrap();
        chat.composer.push_str("bring it on Saturday");
        chat.composer.clone()
    };
    wait(&mut harness, "typed", |_| true);
    let chars = text.chars().count();
    crate::composer::select(&ctx, id, chars - 8, chars);
    wait(&mut harness, "selection", |_| true);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    wait(&mut harness, "bold", |app| app.chat.as_ref().is_some_and(|c| !c.spans.styles.is_empty()));
    crate::composer::select(&ctx, id, 0, 3);
    wait(&mut harness, "toolbar", |_| true);
    save(&mut harness, "composer-formatting");

    // A link preview, as one arrives.
    {
        let chat = harness.state_mut().chat.as_mut().unwrap();
        chat.composer = "Trail notes: https://example.org/trails".into();
        chat.spans.clear();
        chat.preview.arrived("https://example.org/trails", None);
        chat.preview.draft = Some(langis_core::LinkPreviewDraft {
            url: "https://example.org/trails".into(),
            title: "Ridge loop trail guide".into(),
            description: Some("Seven miles, 1,800 feet of climbing, views from the top.".into()),
            date: None,
            image: None,
        });
    }
    crate::composer::set_cursor(&ctx, id, 10);
    wait(&mut harness, "preview", |_| true);
    save(&mut harness, "composer-link-preview");

    {
        let chat = harness.state_mut().chat.as_mut().unwrap();
        chat.composer.clear();
        chat.preview = Default::default();
    }
    {
        let ctx = harness.state().ctx.clone();
        crate::menu::open_below(&ctx, egui::Id::unique("composer-add-menu"), egui::Rect::from_center_size(egui::pos2(1170., 770.), egui::vec2(32., 32.)));
    }
    wait(&mut harness, "attach menu", |_| true);
    save(&mut harness, "composer-attach-menu");

    {
        let ctx = harness.state().ctx.clone();
        crate::menu::close(&ctx, egui::Id::unique("composer-add-menu"));
        let chat = harness.state_mut().chat.as_mut().unwrap();
        let mut poll = crate::polls::NewPoll::default();
        poll.question = "Which trail on Saturday?".into();
        poll.options = vec!["Ridge loop".into(), "Lakeside".into(), String::new()];
        chat.dialog = Some(crate::actions::Dialog::Poll(poll));
    }
    wait(&mut harness, "poll dialog", |_| true);
    save(&mut harness, "poll-dialog");
    finish(harness);
}

/// A poll: voted in, and who voted for what.
#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_polls() {
    let mut harness = demo_harness();
    let chat = harness.state().conversations.iter().find(|c| c.title.contains("Maple")).map(|c| c.id.clone()).expect("the demo's poll chat");
    harness.state_mut().open(chat.clone());
    wait(&mut harness, "messages", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
    let poll = message_id(&harness, |m| m.poll.is_some());
    harness.state_mut().open_at(chat, poll);
    wait(&mut harness, "the poll", |app| app.chat.as_ref().is_some_and(|c| c.scroll_target.is_none()));
    harness.state().send(langis_core::Command::VotePoll { message_id: poll, options: vec![1] });
    wait(&mut harness, "our vote", |app| {
        app.chat.as_ref().and_then(|c| c.messages.iter().find(|m| m.id == poll)).and_then(|m| m.poll.as_ref()).is_some_and(|p| p.votes.iter().any(|v| v.author.is_none()))
    });
    save(&mut harness, "poll");
    harness.state_mut().chat.as_mut().unwrap().dialog = Some(crate::actions::Dialog::PollVotes { message: poll });
    wait(&mut harness, "votes", |_| true);
    save(&mut harness, "poll-votes");
    finish(harness);
}

/// Recording a voice message (from a test tone, through ffmpeg).
#[test]
#[ignore = "renders with a GPU driver and needs ffmpeg; run explicitly"]
fn screenshots_voice() {
    // SAFETY: set before anything else reads it.
    unsafe { std::env::set_var("LANGIS_GUI_VOICE_INPUT", "lavfi:sine=frequency=300:sample_rate=48000,volume=0.6") };
    let mut harness = demo_harness();
    open_chat(&mut harness, langis_store::ConversationKind::Direct);
    harness.state_mut().chat.as_mut().unwrap().recording = Some(crate::voice::Recording::start().unwrap());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
    save(&mut harness, "voice-recording");
    harness.state_mut().chat.as_mut().unwrap().recording = None;
    finish(harness);
}

/// The fun picker's tabs, the sticker manager and a pack.
#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_fun() {
    let mut harness = demo_harness();
    open_chat(&mut harness, langis_store::ConversationKind::Direct);
    harness.state_mut().chat.as_mut().unwrap().emoji_open = true;
    harness.state_mut().fun.tab = crate::fun::Tab::Stickers;
    wait(&mut harness, "stickers", |app| !app.fun.packs.is_empty());
    save(&mut harness, "fun-stickers");
    harness.state_mut().fun.tab = crate::fun::Tab::Gifs;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
    save(&mut harness, "fun-gifs");
    harness.state_mut().chat.as_mut().unwrap().emoji_open = false;

    harness.state_mut().fun.manager_open = true;
    harness.state().send(langis_core::Command::LoadStickerManager);
    wait(&mut harness, "sticker manager", |app| app.fun.manager.is_some());
    save(&mut harness, "sticker-manager");
    let pack = harness.state().fun.manager.as_ref().unwrap().installed.first().cloned().expect("an installed pack");
    harness.state_mut().fun.manager_open = false;
    harness.state().send(langis_core::Command::PreviewStickerPack { id: pack.id.clone(), key: pack.key.clone() });
    harness.state_mut().fun.preview = Some((pack.id, pack.key, None));
    wait(&mut harness, "pack preview", |app| app.fun.preview.as_ref().is_some_and(|p| p.2.is_some()));
    save(&mut harness, "sticker-pack");
    finish(harness);
}

#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_nav_collapsed() {
    let mut harness = demo_harness();
    open_chat(&mut harness, langis_store::ConversationKind::Direct);
    harness.state_mut().nav_collapsed = true;
    wait(&mut harness, "the collapsed tabs", |_| true);
    save(&mut harness, "nav-collapsed");
    harness.state_mut().tab = Tab::Calls;
    wait(&mut harness, "the calls tab", |_| true);
    save(&mut harness, "nav-collapsed-calls");
}

#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn screenshots_menus() {
    let mut harness = demo_harness();
    harness.state().ctx.set_theme(egui::ThemePreference::Dark);
    open_chat(&mut harness, langis_store::ConversationKind::Direct);
    let ctx = harness.state().ctx.clone();
    let more = egui::Rect::from_center_size(egui::pos2(1172., 26.), egui::vec2(28., 28.));
    crate::menu::open_below(&ctx, egui::Id::unique("chat-header-menu"), more);
    wait(&mut harness, "the header menu", |_| true);
    save(&mut harness, "menu-header");
    // Hover the first row, the disappearing messages submenu.
    let panel = ctx.memory(|m| m.area_rect(egui::Id::unique("chat-header-menu").with("panel"))).expect("menu panel");
    harness.event(egui::Event::PointerMoved(egui::pos2(panel.left() + 60., panel.top() + 21.)));
    wait(&mut harness, "the submenu", |_| true);
    save(&mut harness, "menu-header-sub");
    crate::menu::close(&ctx, egui::Id::unique("chat-header-menu"));
    let row = egui::pos2(150., 420.);
    let id = harness.state().conversations.iter().find(|c| c.kind == langis_store::ConversationKind::Direct).map(|c| c.id.clone()).unwrap();
    crate::menu::open_at(&ctx, egui::Id::unique("chat-list-menu").with(&id.0), row);
    wait(&mut harness, "the chat menu", |_| true);
    save(&mut harness, "menu-chat-list");
    harness.state_mut().panes.delete_chat = Some((id, false));
    crate::menu::close(&ctx, egui::Id::unique("chat-list-menu"));
    wait(&mut harness, "the delete dialog", |_| true);
    save(&mut harness, "dialog-delete-chat");
}

