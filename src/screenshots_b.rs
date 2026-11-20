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
        .build_eframe(|cc| App::new(cc, crate::Args { demo: true, data_dir: None, minimized: false }));
    wait(&mut harness, "the demo account", |app| matches!(app.phase, Phase::Ready) && !app.conversations.is_empty());
    harness.state().ctx.set_theme(egui::ThemePreference::Light);

    // The Calls tab, then a new call link's details.
    harness.state_mut().tab = Tab::Calls;
    harness.state_mut().panes.calls_loaded = false;
    wait(&mut harness, "call history", |app| !app.panes.calls.is_empty() || !app.panes.adhoc.is_empty());
    save(&mut harness, "calls-tab");
    // A call's details.
    let first = harness.state().panes.calls.iter().find(|m| m.conversation_id.as_call_link().is_none()).map(|m| (m.conversation_id.clone(), m.sent_at));
    if let Some((conversation, at)) = first {
        harness.state_mut().panes.call_details = Some((conversation, vec![("Incoming voice call".into(), at, false), ("Missed voice call".into(), at - 3_600_000, true)]));
        wait(&mut harness, "call details", |_| true);
        save(&mut harness, "call-details");
        harness.state_mut().panes.call_details = None;
    }
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
    harness.state_mut().panes.stories.composer = Some(crate::stories::Composer { text: "Trail run at 7?".into(), background: 2, font: langis_store::StoryFont::Bold, ..Default::default() });
    wait(&mut harness, "story composer", |_| true);
    save(&mut harness, "story-composer");
    harness.state_mut().panes.stories.composer = Some(crate::stories::Composer { text: "New trail map is up".into(), background: 10, link: Some("https://example.com/trails".into()), ..Default::default() });
    wait(&mut harness, "story composer link", |_| true);
    save(&mut harness, "story-composer-link");
    harness.state_mut().panes.stories.composer.as_mut().unwrap().sending = true;
    wait(&mut harness, "story send modal", |_| true);
    save(&mut harness, "story-send");
    harness.state_mut().panes.stories.composer = None;
    if harness.state().panes.stories.list.iter().any(|s| s.author.is_none()) {
        harness.state_mut().panes.stories.my_stories = true;
        wait(&mut harness, "my stories", |_| true);
        save(&mut harness, "my-stories");
        harness.state_mut().panes.stories.my_stories = false;
        let mine = harness.state().panes.stories.list.iter().filter(|s| s.author.is_none()).count();
        harness.state_mut().panes.stories.viewer = Some(("me".into(), mine - 1, Instant::now()));
        let id = harness.state().panes.stories.list.iter().filter(|s| s.author.is_none()).max_by_key(|s| s.sent_at).map(|s| s.id);
        harness.state_mut().panes.stories.views_of = id;
        wait(&mut harness, "story views", |_| true);
        save(&mut harness, "story-views");
        harness.state_mut().panes.stories.views_of = None;
        harness.state_mut().panes.stories.viewer = None;
    }
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
    use crate::media_gallery::Tab as MediaTab;
    for (tab, name) in [(MediaTab::Media, "media-gallery"), (MediaTab::Audio, "media-audio"), (MediaTab::Links, "media-links"), (MediaTab::Files, "media-files")] {
        if let Some(d) = harness.state_mut().panes.details.as_mut() {
            d.page = crate::details::Page::AllMedia;
            d.media_tab = tab;
        }
        wait(&mut harness, name, |app| app.panes.details.as_ref().is_some_and(|d| d.media.is_some()));
        save(&mut harness, name);
    }
    for (page, name) in [(crate::details::Page::GroupLink, "details-group-link"), (crate::details::Page::ChatColor, "details-chat-color")] {
        if let Some(d) = harness.state_mut().panes.details.as_mut() {
            d.page = page;
        }
        wait(&mut harness, name, |_| true);
        save(&mut harness, name);
    }
    let direct = harness.state().conversations.iter().find(|c| c.kind == ConversationKind::Direct && c.accepted).map(|c| c.id.clone()).unwrap();
    harness.state_mut().open(direct.clone());
    wait(&mut harness, "direct", |app| app.chat.as_ref().is_some_and(|c| c.loaded));
    crate::details::open(harness.state_mut(), direct.clone());
    wait(&mut harness, "direct details", |app| app.panes.details.as_ref().is_some_and(|d| d.contact.is_some()));
    save(&mut harness, "details-direct");
    harness.state_mut().panes.chat_confirm = Some((direct.clone(), crate::panes::ChatConfirm::Block));
    wait(&mut harness, "block confirm", |_| true);
    save(&mut harness, "confirm-block");
    harness.state_mut().panes.chat_confirm = None;
    harness.state_mut().panes.custom_timer = Some((crate::panes::TimerTarget::Chat(direct.clone()), 2, 3));
    wait(&mut harness, "custom timer", |_| true);
    save(&mut harness, "custom-timer");
    harness.state_mut().panes.custom_timer = None;
    harness.state_mut().panes.mute_until = Some((direct.clone(), "2030-01-01".into(), "09:00".into()));
    wait(&mut harness, "mute until", |_| true);
    save(&mut harness, "mute-until");
    harness.state_mut().panes.mute_until = None;
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
        harness.state_mut().panes.collision_review = harness.state().selected.clone();
        wait(&mut harness, "collision review", |_| true);
        save(&mut harness, "collision-review");
        harness.state_mut().panes.collision_review = None;
        let sid = harness.state().selected.clone().and_then(|c| c.as_direct().map(str::to_owned));
        if let Some(sid) = sid {
            crate::contact_modal::open(harness.state_mut(), None, sid);
            if let Some(c) = harness.state_mut().panes.contact.as_mut() {
                c.about_open = true;
            }
            wait(&mut harness, "about contact", |_| true);
            save(&mut harness, "about-contact");
            harness.state_mut().panes.contact = None;
        }
    }

    // New chat, and a new group's second step.
    harness.state_mut().tab = Tab::Chats;
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Chat { query: String::new(), searching: false, found: None });
    wait(&mut harness, "new chat", |_| true);
    save(&mut harness, "new-chat");
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Chat { query: "ma".into(), searching: false, found: None });
    wait(&mut harness, "new chat search", |_| true);
    save(&mut harness, "new-chat-search");
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Find { phone: false, query: String::new(), searching: false, found: None, country: String::new() });
    wait(&mut harness, "find by username", |_| true);
    save(&mut harness, "new-chat-username");
    harness.state_mut().panes.compose = Some(crate::panes::Compose::Find { phone: true, query: "0151 2345678".into(), searching: false, found: None, country: "DE".into() });
    wait(&mut harness, "find by phone", |_| true);
    save(&mut harness, "new-chat-phone");
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
    let params = langis_store::ChatFolderParams { name: "Family".into(), include_all_group_chats: true, included: harness.state().conversations.iter().take(2).map(|c| c.id.clone()).collect(), ..langis_store::ChatFolderParams::defaults() };
    harness.state_mut().settings.folder = Some((crate::settings::new_id(), false, params));
    wait(&mut harness, "folder page", |_| true);
    save(&mut harness, "settings-folder-edit");
    harness.state_mut().settings.folder = None;
    harness.state_mut().settings.page = Page::General;
    harness.state_mut().settings.help = Some(crate::settings::Help::About);
    wait(&mut harness, "about", |_| true);
    save(&mut harness, "about");
    harness.state_mut().settings.help = Some(crate::settings::Help::DebugLog { log: Some("========= System info =========\nApp version: demo\n".into()), uploading: false, link: None });
    wait(&mut harness, "debug log", |_| true);
    save(&mut harness, "debug-log");
    harness.state_mut().settings.help = None;
    harness.state_mut().settings.page = Page::ChatColor;
    harness.state_mut().color_editor = Some(crate::chat_colors::Editor::sample());
    wait(&mut harness, "color editor", |_| true);
    save(&mut harness, "custom-color-editor");
    harness.state_mut().color_editor = None;
    harness.state_mut().settings.page = Page::Profile;
    crate::profile::open(harness.state_mut());
    wait(&mut harness, "profile", |app| app.panes.profile.as_ref().is_some_and(|p| p.loaded));
    save(&mut harness, "profile");
    harness.state_mut().settings.page = Page::ProfileAbout;
    wait(&mut harness, "profile about", |_| true);
    save(&mut harness, "profile-about");
    harness.state_mut().settings.page = Page::Profile;
    harness.state_mut().panes.profile.as_mut().unwrap().avatar_editor = Some(crate::avatar_editor::AvatarEditor::new(None, "Riley Park"));
    wait(&mut harness, "avatar editor", |_| true);
    save(&mut harness, "avatar-editor");
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
        .map(|(i, aci)| langis_core::CallParticipant { aci: aci.clone(), demux_id: i as u32 + 1, video: false, audio: i != 1, presenting: false, speaker_time: None })
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
    // A call link we run, with people asking to join.
    let asking: Vec<String> = group.members.iter().skip(1).take(3).cloned().collect();
    if let Some(call) = harness.state_mut().call.as_mut() {
        call.link = Some(langis_core::CallLinkCall { admin: true, pending: asking, waiting: false });
    }
    harness.state_mut().panes.participants_open = false;
    harness.state_mut().panes.pending_expanded = true;
    wait(&mut harness, "pending participants", |_| true);
    save(&mut harness, "call-pending");
    harness.state_mut().panes.pending_expanded = false;
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
