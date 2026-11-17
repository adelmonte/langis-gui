//! Screenshots of the Calls and Stories tabs, chat details, dialogs and the
//! extra settings, on demo data: `cargo test screenshots_panes -- --ignored
//! --nocapture`. Pictures go where `screenshots` puts its own.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use langis_core::Command;
use langis_store::ConversationKind;

use crate::app::{App, Phase, Tab};

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("LANGIS_GUI_SHOTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/screenshots"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wait(harness: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(harness.state()) {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {what}");
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
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
fn screenshots_panes() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200., 800.))
        .with_theme(egui::Theme::Light)
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::new(cc, crate::Args { demo: true, data_dir: None }));
    wait(&mut harness, "the demo account", |app| matches!(app.phase, Phase::Ready) && !app.conversations.is_empty());
    harness.state().ctx.set_theme(egui::ThemePreference::Light);

    // The Calls tab, then a new call link's details.
    harness.state_mut().tab = Tab::Calls;
    harness.state_mut().panes.calls_loaded = false;
    wait(&mut harness, "call history", |app| !app.panes.calls.is_empty() || !app.panes.adhoc.is_empty());
    save(&mut harness, "calls-tab");
    harness.state().send(Command::CreateCallLink);
    wait(&mut harness, "a call link", |app| app.panes.link_details.as_ref().is_some_and(|r| app.panes.links.iter().any(|l| &l.room_id == r)));
    save(&mut harness, "call-link");
    harness.state_mut().panes.new_call = Some(String::new());
    wait(&mut harness, "new call", |_| true);
    save(&mut harness, "calls-new-call");
    harness.state_mut().panes.new_call = None;
    harness.state_mut().panes.clear_calls = true;
    wait(&mut harness, "clear calls", |_| true);
    save(&mut harness, "calls-clear-dialog");
    harness.state_mut().panes.clear_calls = false;

    // Stories: the list, a story open, the composer, privacy.
    harness.state_mut().tab = Tab::Stories;
    wait(&mut harness, "stories", |app| !app.panes.stories.list.is_empty());
    save(&mut harness, "stories-tab");
    let first = harness.state().panes.stories.list.iter().find(|s| s.author.is_some() && s.group.is_none()).map(|s| (format!("p:{}", s.author.clone().unwrap()), 0));
    if let Some((key, index)) = first {
        harness.state_mut().panes.stories.viewer = Some((key, index, Instant::now()));
        wait(&mut harness, "story viewer", |_| true);
        save(&mut harness, "story-viewer");
        harness.state_mut().panes.stories.viewer = None;
    }
    harness.state_mut().panes.stories.composer = Some(crate::stories::Composer { text: "Trail run at 7?".into(), background: 2, media: None, targets: vec![langis_core::StoryTarget::MyStory] });
    wait(&mut harness, "story composer", |_| true);
    save(&mut harness, "story-composer");
    harness.state_mut().panes.stories.composer = None;
    harness.state_mut().panes.stories.privacy = true;
    wait(&mut harness, "story privacy", |_| true);
    save(&mut harness, "story-privacy");
    harness.state_mut().panes.stories.privacy = false;

    // Chats: a group's details, its permissions, a person's safety number.
    harness.state_mut().tab = Tab::Chats;
    let group = harness.state().conversations.iter().find(|c| c.kind == ConversationKind::Group && !c.left).map(|c| c.id.clone()).unwrap();
    harness.state_mut().open(group.clone());
    wait(&mut harness, "group", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
    crate::details::open(harness.state_mut(), group);
    wait(&mut harness, "group details", |_| true);
    save(&mut harness, "details-group");
    harness.state().ctx.set_theme(egui::ThemePreference::Dark);
    wait(&mut harness, "dark", |_| true);
    save(&mut harness, "details-group-dark");
    harness.state().ctx.set_theme(egui::ThemePreference::Light);
    if let Some(d) = harness.state_mut().panes.details.as_mut() {
        d.page = crate::details::Page::Permissions;
    }
    wait(&mut harness, "permissions", |_| true);
    save(&mut harness, "details-permissions");
    let direct = harness.state().conversations.iter().find(|c| c.kind == ConversationKind::Direct && c.accepted).map(|c| c.id.clone()).unwrap();
    harness.state_mut().open(direct.clone());
    wait(&mut harness, "direct", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
    crate::details::open(harness.state_mut(), direct.clone());
    wait(&mut harness, "direct details", |app| app.panes.details.as_ref().is_some_and(|d| d.contact.is_some()));
    save(&mut harness, "details-direct");
    harness.state().send(Command::LoadSafetyNumber { conversation: direct });
    if let Some(d) = harness.state_mut().panes.details.as_mut() {
        d.page = crate::details::Page::Safety;
    }
    wait(&mut harness, "safety number", |app| app.panes.details.as_ref().is_some_and(|d| d.safety.is_some()));
    save(&mut harness, "details-safety");
    harness.state_mut().panes.details = None;

    // A message request.
    if let Some(request) = harness.state().conversations.iter().find(|c| !c.accepted && !c.blocked).map(|c| c.id.clone()) {
        harness.state_mut().open(request);
        wait(&mut harness, "request", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
        save(&mut harness, "message-request");
    }

    // New chat, and a new group's second step.
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Chat { query: "ma".into(), searching: false, found: None });
    wait(&mut harness, "new chat", |_| true);
    save(&mut harness, "new-chat");
    let members: Vec<String> = harness.state().conversations.iter().filter(|c| c.kind == ConversationKind::Direct).filter_map(|c| c.id.as_direct().map(str::to_owned)).take(2).collect();
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Group { picking: true, query: String::new(), members: members.clone(), title: String::new(), timer: 0, avatar: None });
    wait(&mut harness, "new group", |_| true);
    save(&mut harness, "new-group-members");
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Group { picking: false, query: String::new(), members, title: "Climbing crew".into(), timer: 0, avatar: None });
    wait(&mut harness, "new group name", |_| true);
    save(&mut harness, "new-group-name");
    harness.state_mut().panes.compose = None;

    // Settings pages, and the profile editor.
    harness.state_mut().tab = Tab::Settings;
    use crate::settings::Page;
    for (page, name) in [
        (Page::Account, "settings-account"),
        (Page::General, "settings-general"),
        (Page::Appearance, "settings-appearance"),
        (Page::ChatColor, "settings-chat-color"),
        (Page::Chats, "settings-chats"),
        (Page::ChatFolders, "settings-folders"),
        (Page::Calls, "settings-calls"),
        (Page::Notifications, "settings-notifications"),
        (Page::WhileMuted, "settings-while-muted"),
        (Page::Privacy, "settings-privacy"),
        (Page::PhoneNumber, "settings-phone-number"),
        (Page::Blocked, "settings-blocked"),
        (Page::DataUsage, "settings-data-usage"),
    ] {
        harness.state_mut().settings.page = page;
        harness.state_mut().settings.stale = true;
        wait(&mut harness, name, |app| app.settings.snapshot.is_some());
        save(&mut harness, name);
    }
    crate::profile::open(harness.state_mut());
    wait(&mut harness, "profile", |app| app.panes.profile.as_ref().is_some_and(|p| p.loaded));
    save(&mut harness, "profile");
    harness.state_mut().panes.profile = None;
    harness.state_mut().tab = Tab::Chats;

    // A group call's lobby, then the call with its extras.
    let group = harness.state().conversations.iter().find(|c| c.kind == ConversationKind::Group && !c.left).cloned().unwrap();
    crate::panes::open_lobby(harness.state_mut(), group.id.clone(), false);
    wait(&mut harness, "lobby", |_| true);
    save(&mut harness, "call-lobby");
    harness.state_mut().panes.lobby = None;
    let participants: Vec<langis_core::CallParticipant> = group
        .members
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, aci)| langis_core::CallParticipant { aci: aci.clone(), demux_id: i as u32 + 1, video: false, audio: i != 1, presenting: false })
        .collect();
    let first = participants.first().map(|p| p.aci.clone()).unwrap_or_default();
    harness.state_mut().call = Some(langis_core::CallUpdate {
        conversation: group.id.clone(),
        status: langis_core::CallStatus::Connected { since: chrono::Utc::now().timestamp_millis() - 125_000 },
        video: false,
        incoming: false,
        remote_video: false,
        remote_audio: true,
        sharing: false,
        remote_sharing: false,
        group: true,
        participants,
        ringer: None,
        raised_hands: vec![2],
        local_demux_id: Some(9),
        link: None,
    });
    harness.state_mut().panes.participants_open = true;
    harness.state_mut().panes.call_reactions.push((langis_core::CallReaction { aci: first, demux_id: 1, emoji: "👍".into() }, Instant::now()));
    wait(&mut harness, "group call", |_| true);
    save(&mut harness, "call-group-extras");
    harness.state_mut().call = None;
    harness.state_mut().panes.participants_open = false;

    // The captcha dialog.
    harness.state_mut().panes.captcha = Some(String::new());
    wait(&mut harness, "captcha", |_| true);
    save(&mut harness, "captcha");
    harness.state_mut().panes.captcha = None;

    harness.state_mut().shutdown();
    // As in `screenshots`: lavapipe's teardown crashes; the pictures are written.
    std::mem::forget(harness);
}
