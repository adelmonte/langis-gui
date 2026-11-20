//! The README's picture: the demo account with Mystery Inc. swapped in for
//! its people and chats, in memory only (langis's demo data and the other
//! screenshots stay as they are). Run with
//! `cargo test readme_screenshot -- --ignored`; it writes
//! `target/screenshots/readme.png`.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use langis_store::{ConversationKind, Direction, Reaction};

use crate::app::{App, Phase};

fn settle(harness: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
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

const PEOPLE: [&str; 6] = ["Shaggy Rogers", "Velma Dinkley", "Daphne Blake", "Scooby-Doo", "Scrappy-Doo", "Old Man Jenkins"];
const PREVIEWS: [&str; 6] = [
    "Like, zoinks! Did you hear that?",
    "Jinkies! I found a clue in the library",
    "Jeepers, my scarf is stuck on the hedge maze",
    "Ruh-roh! Rooby Racks?",
    "Lemme at 'em! Lemme at 'em!",
    "You meddling kids...",
];
const GROUPS: [&str; 3] = ["Mystery Inc. 🔍", "Coolsville Diner Crew", "Haunted Lighthouse Stakeout"];

/// The group chat's lines: (who, text). `None` is us (Fred).
const SCRIPT: [(Option<usize>, &str); 9] = [
    (Some(1), "Okay gang, the ghost only shows up after midnight at the old amusement park."),
    (Some(0), "Like, why does it ALWAYS have to be midnight?"),
    (Some(3), "Rhat's rhe point? 😨"),
    (None, "Let's split up. Daphne and I will check the funhouse."),
    (Some(2), "Velma, your glasses fell behind the carousel again"),
    (Some(1), "My glasses! I can't see without my glasses! 👓"),
    (Some(0), "Scoob and I will, like, guard the snack bar. For safety."),
    (Some(3), "Scooby snacks! 🦴"),
    (None, "Looks like it's time to set a trap. Meet at the Mystery Machine at 11:30."),
];

/// Mystery Inc. in place of the demo's people and chats; again on every
/// frame, since the chat list reloads from the store. Returns the group's
/// members, whose names it set.
fn rename(app: &mut App) -> Vec<String> {
    app.panes.megaphones.clear();
    app.me.0 = "Fred Jones".into();
    let (mut person, mut group) = (0, 0);
    let mut members: Vec<String> = Vec::new();
    for c in app.conversations.iter_mut() {
        c.avatar_path = None;
        c.draft = None;
        match c.kind {
            ConversationKind::Direct => {
                let i = person % PEOPLE.len();
                c.title = PEOPLE[i].into();
                c.last_message_preview = Some(PREVIEWS[i].into());
                c.accepted = true;
                person += 1;
            }
            ConversationKind::Group => {
                c.title = GROUPS[group % GROUPS.len()].into();
                if group == 0 {
                    members = c.members.clone();
                    c.last_message_preview = Some(format!("You: {}", SCRIPT[SCRIPT.len() - 1].1));
                } else {
                    c.last_message_preview = Some(["Daphne: Who ordered the 12-layer sandwich?", "Shaggy: Like, the light just went out, man"][(group - 1) % 2].into());
                }
                c.accepted = true;
                group += 1;
            }
            ConversationKind::NoteToSelf => c.last_message_preview = Some("Clues: rubber mask, glow paint, footprints in the flour".into()),
            _ => {}
        }
    }
    for (i, sid) in members.iter().enumerate() {
        app.names.insert(sid.clone(), PEOPLE[i % 4].into());
    }
    for c in app.conversations.iter_mut() {
        c.member_labels.clear();
        c.pinned = c.title == GROUPS[0] || c.kind == ConversationKind::NoteToSelf;
        c.unread_count = if c.title == PEOPLE[3] { 2 } else if c.title == GROUPS[1] { 5 } else { 0 };
        c.marked_unread = false;
        c.muted = false;
    }
    app.unread_mentions.clear();
    app.panes.active_calls.clear();
    app.badges.clear();
    if let Some(chat) = app.chat.as_mut() {
        chat.unread_mentions.clear();
    }
    let gang = app.conversations.iter().position(|c| c.title == GROUPS[0]);
    if let Some(i) = gang {
        let c = app.conversations.remove(i);
        app.conversations.insert(0, c);
    }
    members
}

#[test]
#[ignore = "renders with a GPU driver; run explicitly"]
fn readme_screenshot() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200., 760.))
        .with_theme(egui::Theme::Dark)
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::new(cc, crate::Args { demo: true, data_dir: None, minimized: false }));
    settle(&mut harness, "the demo account", |app| matches!(app.phase, Phase::Ready) && !app.conversations.is_empty());
    harness.state().ctx.set_theme(egui::ThemePreference::Dark);

    let members = rename(harness.state_mut());
    let app = harness.state_mut();
    let mystery = app.conversations.iter().find(|c| c.kind == ConversationKind::Group).map(|c| c.id.clone()).unwrap();
    app.open(mystery.clone());
    settle(&mut harness, "messages", |app| app.chat.as_ref().is_some_and(|c| c.loaded));

    // The chat's history replaced with the script, on the demo's own
    // messages (so ids and times stay real).
    let app = harness.state_mut();
    let now = chrono::Utc::now().timestamp_millis();
    let chat = app.chat.as_mut().unwrap();
    let template = chat.messages.iter().find(|m| m.notice.is_none() && m.direction == Direction::Incoming).cloned().unwrap();
    let mut messages = Vec::new();
    for (i, (who, text)) in SCRIPT.iter().enumerate() {
        let mut m = template.clone();
        m.id = 900_000 + i as i64;
        m.sent_at = now - (SCRIPT.len() - i) as i64 * 4 * 60 * 1000;
        m.received_at = m.sent_at;
        m.body = (*text).into();
        m.styles.clear();
        m.mentions.clear();
        m.quote = None;
        m.attachments.clear();
        m.link_preview = None;
        m.reactions.clear();
        m.edited = false;
        m.deleted = false;
        m.poll = None;
        m.content = None;
        m.view_once = None;
        m.long_text = None;
        m.cta = None;
        match who {
            Some(p) => {
                m.direction = Direction::Incoming;
                m.author = members.get(*p).cloned();
                m.author_name = Some(PEOPLE[*p].into());
            }
            None => {
                m.direction = Direction::Outgoing;
                m.author = None;
                m.status = langis_store::DeliveryStatus::Read;
            }
        }
        messages.push(m);
    }
    messages[2].reactions = vec![Reaction { emoji: "😂".into(), author: members.first().cloned(), sent_at: now }];
    messages[7].reactions = vec![
        Reaction { emoji: "🦴".into(), author: members.first().cloned(), sent_at: now },
        Reaction { emoji: "🦴".into(), author: None, sent_at: now },
        Reaction { emoji: "😂".into(), author: members.get(2).cloned(), sent_at: now },
    ];
    chat.messages = messages;
    chat.unread_marker = None;
    chat.pinned.clear();
    chat.scroll_to_bottom = 10;
    app.typing.clear();
    for _ in 0..60 {
        rename(harness.state_mut());
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    rename(harness.state_mut());
    harness.step();

    let image = harness.render().expect("rendering");
    let dir = std::path::PathBuf::from("target/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    image.save(dir.join("readme.png")).unwrap();
}
