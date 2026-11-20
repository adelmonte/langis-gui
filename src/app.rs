//! The app's state, and how it talks to the langis backend: commands go
//! out, events come in on a thread that wakes the UI, and store reads run
//! on the backend's runtime and come back as [`Loaded`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use langis_core::keys::{self, KeySource};
use langis_core::{Backend, CallDevices, CallUpdate, Command, Config, ConnectionState, Event, HistoryProgress, ShareSource};
use langis_store::{Conversation, ConversationId, Message};

use crate::chat::ChatView;
use crate::settings::SettingsView;

/// Where the app is: before the backend runs, linking, or in the account.
pub enum Phase {
    /// A new data directory: how to keep its database key.
    Setup { passphrase: bool, first: String, second: String, error: Option<String> },
    /// A passphrase-protected data directory.
    Unlock { passphrase: String, error: Option<String> },
    /// The key is being made or derived and the backend started.
    Starting,
    /// No account yet: the QR code to scan, once it's here.
    Linking { url: Option<String>, error: Option<String>, qr: Option<egui::TextureHandle> },
    Ready,
    /// The backend couldn't start.
    Failed(String),
}

/// A store read or check that finished on the backend's runtime.
pub enum Loaded {
    Found(crate::search::Found),
    Conversations(Vec<Conversation>),
    Messages { conversation: ConversationId, older: bool, messages: Vec<Message> },
    Message(Message),
    Names(Vec<(String, String)>),
    /// Who's stopping a send (`(service id, name)`), and what to do after.
    Unverified { conversation: ConversationId, people: Vec<(String, String)>, then: PendingSend },
    Settings(Box<crate::settings::Snapshot>),
    Theme(Option<String>),
    /// Our notification settings: shown at all, and what they show.
    NotifyPrefs { on: bool, content: String, nav_collapsed: bool, sound: crate::sounds::Prefs },
    /// Our own profile name and photo.
    Me { name: Option<String>, photo: Option<String> },
    /// A desktop notification to show.
    Notify { title: String, body: String },
    /// Anything else: applied to the app as it comes.
    Apply(Box<dyn FnOnce(&mut App) + Send>),
    /// For the open chat.
    Chat(crate::chat::ChatLoaded),
    /// The reaction bar's emoji.
    Reactions(Vec<String>),
    LinkPreviews(bool),
    /// A chat's photos and videos, oldest first, for the viewer.
    Lightbox { conversation: ConversationId, message_id: i64, index: usize, items: Vec<crate::lightbox::Item> },
}

/// A send held until the user answers the safety number prompt.
#[derive(Clone, Debug)]
pub enum PendingSend {
    Command(Box<Command>),
    Retry(i64),
}

/// What started the backend: the started backend, or why it didn't.
type Started = Result<(Backend, futures::channel::mpsc::UnboundedReceiver<Event>), String>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Chats,
    Calls,
    Stories,
    Settings,
}

pub struct Toast {
    pub text: String,
    pub until: Instant,
    /// Desktop's toast button (Undo and the like): its label and what it sends.
    pub action: Option<(String, langis_core::Command)>,
}

pub struct SafetyPrompt {
    pub people: Vec<(String, String)>,
    pub then: PendingSend,
}

pub struct App {
    pub ctx: egui::Context,
    pub phase: Phase,
    pub data_dir: PathBuf,
    pub demo: bool,
    /// Kept so a demo's scratch directory lives as long as the app.
    _scratch: Option<tempfile::TempDir>,
    pub backend: Option<Backend>,
    started: Option<Receiver<Started>>,
    events: Option<Receiver<Event>>,
    loaded_tx: Sender<Loaded>,
    loaded: Receiver<Loaded>,

    pub aci: String,
    /// Our number, from `Event::Ready`.
    pub phone: Option<String>,
    pub connection: ConnectionState,
    pub history: Option<HistoryProgress>,
    pub tab: Tab,
    pub conversations: Vec<Conversation>,
    pub search: String,
    /// The chat list's message results for `search`.
    pub message_search: crate::search::Global,
    /// The chat header's search button was pressed.
    pub find_requested: bool,
    /// The header menu asked for select mode in the open chat.
    pub select_requested: bool,
    pub selected: Option<ConversationId>,
    pub chat: Option<ChatView>,
    pub names: HashMap<String, String>,
    pub typing: HashMap<ConversationId, HashSet<String>>,
    pub call: Option<CallUpdate>,
    pub call_view: crate::call::CallView,
    /// Voice messages and audio files playing in the chat.
    pub audio: crate::audio::Audio,
    /// Photos at the size they're shown, let go when they're not.
    thumbs: std::sync::Arc<crate::thumbs::Thumbs>,
    last_evict: Instant,
    pub call_devices: CallDevices,
    pub share_sources: Option<Vec<ShareSource>>,
    /// Whether this desktop can share a screen at all (no portal: no
    /// Present button), once asked.
    pub share_available: Option<bool>,
    /// The user pressed Present and we're waiting for the sources.
    pub share_requested: bool,
    pub safety: Option<SafetyPrompt>,
    pub toasts: Vec<Toast>,
    pub settings: SettingsView,
    /// The theme setting: "system", "light" or "dark".
    pub theme: String,
    pub focused: bool,
    /// Desktop notifications on, and what they show ("all", "name", "none").
    pub notify: (bool, String),
    /// Sounds, call notifications and the taskbar flash.
    pub sound: crate::sounds::Prefs,
    /// Files dropped on the window under Wayland (winit gives egui only
    /// X11's); `None` on X11.
    pub drops: Option<crate::wayland_dnd::Drops>,
    /// Attachments downloading: (message id, index) to bytes received and
    /// the total, from langis's DownloadProgress.
    pub downloads: std::collections::HashMap<(i64, usize), (u64, u64)>,
    /// Each person's first visible profile badge, as image files for the
    /// light and dark themes (Desktop draws it on their avatar).
    pub badges: std::collections::HashMap<String, (Option<String>, Option<String>)>,
    /// When the last frame ran, by the wall clock, to notice a sleep.
    last_tick: Option<std::time::SystemTime>,
    /// The composer's settings (Settings, Chats, Text input).
    pub input: InputPrefs,
    /// Desktop's collapsed nav tabs: the toggle moves into the list header.
    pub nav_collapsed: bool,
    momentum: crate::scroll::Momentum,
    /// Our profile name and photo, for the nav avatar and mentions of us.
    pub me: (String, Option<String>),
    /// The calls and stories tabs, chat details, dialogs and menus.
    pub panes: crate::panes::Panes,
    /// Desktop's chat colors: the default, each chat's and the custom ones.
    pub chat_colors: langis_store::ChatColors,
    /// Desktop's custom color editor, open.
    pub color_editor: Option<crate::chat_colors::Editor>,
    /// Desktop's "Close Signal and end the call?", open.
    pub confirm_close: bool,
    /// Closing for good: confirmed, or asked by a signal.
    closing: std::sync::Arc<std::sync::atomic::AtomicBool>,
    diag: crate::diag::Diag,
    /// The reaction bar's emoji (Desktop's preferred reactions).
    pub reactions: Vec<String>,
    /// The photo and video viewer, when open.
    pub lightbox: Option<crate::lightbox::Lightbox>,
    /// Link previews on (Settings, Chats).
    pub link_previews: bool,
    /// Missed calls not looked at yet: the Calls tab's badge.
    pub unseen_calls: u32,
    /// Unread messages mentioning us, by chat: the chat list's @ badge.
    pub unread_mentions: std::collections::HashMap<ConversationId, u32>,
    /// The chat list shows only unread chats (Desktop's search filter).
    pub unread_only: bool,
    /// Emoji most recently used, newest first: the pickers' Recents.
    pub recent_emoji: Vec<String>,
    /// Stickers, GIFs and the sticker manager.
    pub fun: crate::fun::Fun,
}

/// The composer's settings.
#[derive(Clone, Copy, Debug)]
pub struct InputPrefs {
    /// Desktop's "Convert typed emoticons to emoji".
    pub auto_convert: bool,
    /// The selection's formatting toolbar.
    pub formatting: bool,
    pub skin_tone: Option<langis_emoji::SkinTone>,
    /// Spell check in the composer (Desktop's "Spell check text entered").
    pub spell: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> Self {
        let ctx = cc.egui_ctx.clone();
        // Posters a crash left behind.
        crate::video::clear_posters();
        if args.minimized {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        egui_extras::install_image_loaders(&ctx);
        let thumbs = crate::thumbs::Thumbs::install(&ctx);
        crate::theme::install(&ctx);
        let (loaded_tx, loaded) = channel();
        let (data_dir, scratch) = if args.demo {
            let dir = tempfile::Builder::new().prefix("langis-gui-demo-").tempdir().expect("a scratch directory for the demo");
            (dir.path().to_owned(), Some(dir))
        } else {
            (args.data_dir.unwrap_or_else(default_data_dir), None)
        };
        let mut app = Self {
            ctx,
            phase: Phase::Starting,
            data_dir,
            demo: args.demo,
            _scratch: scratch,
            backend: None,
            started: None,
            events: None,
            loaded_tx,
            loaded,
            aci: String::new(),
            phone: None,
            connection: ConnectionState::Offline,
            history: None,
            tab: Tab::Chats,
            conversations: Vec::new(),
            search: String::new(),
            message_search: Default::default(),
            find_requested: false,
            select_requested: false,
            selected: None,
            chat: None,
            names: HashMap::new(),
            typing: HashMap::new(),
            call: None,
            call_view: Default::default(),
            audio: Default::default(),
            thumbs,
            last_evict: Instant::now(),
            call_devices: Default::default(),
            share_sources: None,
            share_available: None,
            share_requested: false,
            safety: None,
            toasts: Vec::new(),
            settings: Default::default(),
            theme: "system".into(),
            focused: true,
            notify: (true, "all".into()),
            nav_collapsed: false,
            momentum: Default::default(),
            me: (String::new(), None),
            panes: Default::default(),
            chat_colors: Default::default(),
            color_editor: None,
            confirm_close: false,
            closing: quit_on_signals(&cc.egui_ctx),
            diag: Default::default(),
            reactions: crate::actions::DEFAULT_REACTIONS.iter().map(|e| e.to_string()).collect(),
            lightbox: None,
            link_previews: true,
            unseen_calls: 0,
            recent_emoji: Vec::new(),
            unread_mentions: Default::default(),
            sound: Default::default(),
            last_tick: None,
            badges: Default::default(),
            downloads: Default::default(),
            drops: crate::wayland_dnd::start(cc),
            input: InputPrefs { auto_convert: true, formatting: true, skin_tone: None, spell: true },
            unread_only: false,
            fun: Default::default(),
        };
        let dir = &app.data_dir;
        if !app.demo && in_use(dir) {
            app.phase = Phase::Failed(ALREADY_RUNNING.into());
        } else if app.demo || dir.join("database.key").exists() {
            app.start(KeySource::File);
        } else if dir.join("database.salt").exists() {
            app.phase = Phase::Unlock { passphrase: String::new(), error: None };
        } else if dir.join("app.db").exists() || dir.join("signal.db").exists() {
            app.phase = Phase::Failed(format!(
                "{} has databases but no key for them (database.key or database.salt).",
                dir.display()
            ));
        } else {
            app.phase = Phase::Setup { passphrase: false, first: String::new(), second: String::new(), error: None };
        }
        app
    }

    /// Finds or makes the database key and starts the backend, off the UI
    /// thread (deriving a passphrase's key takes a moment).
    pub fn start(&mut self, source: KeySource) {
        self.phase = Phase::Starting;
        let (tx, rx) = channel();
        let (dir, demo, ctx) = (self.data_dir.clone(), self.demo, self.ctx.clone());
        std::thread::spawn(move || {
            let started = keys::load_or_create(&dir, &source).map_err(|e| format!("{e:#}")).and_then(|(db_key, _)| {
                let config = Config {
                    data_dir: dir,
                    db_key,
                    device_name: device_name(),
                    demo,
                    former_data_dirs: Vec::new(),
                    call_lines: Default::default(),
                };
                Backend::start(config).map_err(|e| format!("{e:#}"))
            });
            let _ = tx.send(started);
            ctx.request_repaint();
        });
        self.started = Some(rx);
    }

    pub fn send(&self, command: Command) {
        if let Some(backend) = &self.backend {
            backend.send(command);
        }
    }

    /// Runs `read` against the store on the backend's runtime; its result
    /// comes back through [`App::poll`].
    pub fn load<F>(&self, read: impl FnOnce(langis_store::AppDb) -> F)
    where
        F: Future<Output = Option<Loaded>> + Send + 'static,
    {
        let Some(backend) = &self.backend else { return };
        let future = read(backend.db().clone());
        let (tx, ctx) = (self.loaded_tx.clone(), self.ctx.clone());
        // The task runs on the backend's runtime whether or not the handle is
        // awaited.
        drop(backend.spawn(async move {
            if let Some(loaded) = future.await {
                let _ = tx.send(loaded);
                ctx.request_repaint();
            }
        }));
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), until: Instant::now() + Duration::from_secs(4), action: None });
    }

    /// Archives or unarchives a chat, with Desktop's toast: "Chat archived"
    /// with Undo, or "Chat returned to inbox".
    pub fn set_archived(&mut self, conversation: ConversationId, archive: bool) {
        use langis_core::{ChatChange, Command};
        self.send(Command::SetChat { conversation: conversation.clone(), change: ChatChange::Archived(archive) });
        if archive {
            self.toast_action("Chat archived", "Undo", Command::SetChat { conversation, change: ChatChange::Archived(false) });
        } else {
            self.toast("Chat returned to inbox");
        }
    }

    /// A toast with a button, e.g. "Undo", that sends `command`.
    pub fn toast_action(&mut self, text: impl Into<String>, label: impl Into<String>, command: langis_core::Command) {
        self.toasts.push(Toast { text: text.into(), until: Instant::now() + Duration::from_secs(6), action: Some((label.into(), command)) });
    }

    pub fn name(&self, service_id: &str) -> String {
        self.names.get(service_id).cloned().unwrap_or_else(|| "Unknown".into())
    }

    pub fn conversation(&self, id: &ConversationId) -> Option<&Conversation> {
        self.conversations.iter().find(|c| &c.id == id)
    }

    /// Takes in what came from the backend since the last frame.
    pub fn poll(&mut self) {
        if let Some(rx) = &self.started
            && let Ok(started) = rx.try_recv()
        {
            self.started = None;
            match started {
                Ok((backend, events)) => self.attach(backend, events),
                Err(error) => {
                    // A wrong passphrase goes back to its prompt.
                    if error.contains("already in use") {
                        self.phase = Phase::Failed(ALREADY_RUNNING.into());
                    } else if error.contains("wrong passphrase") {
                        self.phase = Phase::Unlock { passphrase: String::new(), error: Some("That passphrase isn't right.".into()) };
                    } else {
                        self.phase = Phase::Failed(error);
                    }
                }
            }
        }
        while let Some(event) = self.events.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.diag.event(&event);
            self.on_event(event);
        }
        while let Ok(loaded) = self.loaded.try_recv() {
            self.diag.loaded();
            self.on_loaded(loaded);
        }
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
    }

    fn attach(&mut self, backend: Backend, mut events: futures::channel::mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("langis-events".into())
            .spawn(move || {
                while let Some(event) = futures::executor::block_on(events.next()) {
                    if tx.send(event).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("the event thread");
        self.backend = Some(backend);
        self.events = Some(rx);
        self.load(|db| async move { Some(Loaded::Theme(db.setting(langis_core::SETTING_THEME).await.ok().flatten())) });
    }

    fn on_event(&mut self, event: Event) {
        if self.fun.on_event(&event) {
            return;
        }
        match event {
            Event::NeedsLinking => {
                self.phase = Phase::Linking { url: None, error: None, qr: None };
                self.send(Command::Link);
            }
            Event::LinkUrl(url) => {
                let qr = crate::startup::qr_texture(&self.ctx, url.as_str());
                self.phase = Phase::Linking { url: Some(url.to_string()), error: None, qr };
            }
            Event::LinkFailed(why) => {
                if let Phase::Linking { error, .. } = &mut self.phase {
                    *error = Some(why);
                }
            }
            Event::Ready { aci, phone } => {
                self.aci = aci;
                self.phone = phone;
                self.phase = Phase::Ready;
                self.reload_conversations();
                self.load_notify_prefs();
                self.load_me();
                self.load_reactions();
                self.load_link_previews();
                self.load_recent_emoji();
                self.load_badges();
                self.load(|db| async move {
                    let get = |v: Option<String>| v.map(|v| v == "true");
                    let auto_convert = get(db.setting(crate::settings::SETTING_AUTO_CONVERT_EMOJI).await.ok().flatten()).unwrap_or(true);
                    let formatting = get(db.setting(crate::settings::SETTING_TEXT_FORMATTING).await.ok().flatten()).unwrap_or(true);
                    let tone = db.setting(crate::settings::SETTING_SKIN_TONE).await.ok().flatten().and_then(|v| v.parse::<usize>().ok()).filter(|i| (1..=5).contains(i));
                    let skin_tone = tone.map(|i| langis_emoji::SkinTone::ALL[i - 1]);
                    let spell = get(db.setting(crate::spell::SETTING_SPELL_CHECK).await.ok().flatten()).unwrap_or(true);
                    let words: Vec<String> = db.setting(crate::spell::SETTING_SPELL_WORDS).await.ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
                    Some(Loaded::Apply(Box::new(move |app: &mut App| {
                        app.input = InputPrefs { auto_convert, formatting, skin_tone, spell };
                        crate::emoji_picker::set_skin_tone(skin_tone);
                        crate::spell::start(&app.ctx, words);
                    })))
                });
                self.load(|db| async move {
                    let gifs = db.setting("ui.recent_gifs").await.ok().flatten().map(|s| crate::fun::parse_recent_gifs(&s)).unwrap_or_default();
                    let stickers = db.setting("ui.recent_stickers").await.ok().flatten().map(|s| crate::fun::parse_recent_stickers(&s)).unwrap_or_default();
                    Some(Loaded::Apply(Box::new(move |app: &mut App| {
                        app.fun.recent_gifs = gifs;
                        app.fun.recent_stickers = stickers;
                    })))
                });
                self.send(Command::LoadCallDevices);
                crate::panes::on_ready(self);
            }
            Event::Unlinked => {
                self.conversations.clear();
                self.chat = None;
                self.selected = None;
                self.phase = Phase::Linking { url: None, error: None, qr: None };
                self.send(Command::Link);
            }
            Event::Connection(state) => self.connection = state,
            Event::History(progress) => self.history = Some(progress),
            Event::ConversationsChanged | Event::ContactsSynced | Event::ProfileChanged(_) => self.reload_conversations(),
            Event::AuthorNamesChanged(pairs) => self.names.extend(pairs),
            Event::Incoming { conversation, message_id, mentions_me, replies_to_me } => {
                self.notify_message(&conversation, message_id, mentions_me || replies_to_me);
                self.on_event(Event::MessageChanged { conversation, message_id });
            }
            Event::MessageChanged { conversation, message_id } => {
                if self.selected.as_ref() == Some(&conversation) {
                    self.load(move |db| async move { db.message(message_id).await.ok().flatten().map(Loaded::Message) });
                    // Only while the newest messages are in view, as Desktop
                    // marks read what's been scrolled to.
                    if self.focused && self.chat.as_ref().is_none_or(|c| !c.scrolled_up) {
                        self.send(Command::MarkRead { conversation });
                    }
                }
            }
            Event::MessageRemoved { conversation, message_id } => {
                if let Some(chat) = self.chat.as_mut().filter(|c| c.id == conversation) {
                    chat.messages.retain(|m| m.id != message_id);
                }
            }
            Event::ConversationCleared(conversation) => {
                if self.selected.as_ref() == Some(&conversation) {
                    self.open(conversation);
                }
            }
            Event::ConversationRemoved(conversation) => {
                if self.selected.as_ref() == Some(&conversation) {
                    self.selected = None;
                    self.chat = None;
                }
                self.reload_conversations();
            }
            Event::Typing { conversation, author, started } => {
                let set = self.typing.entry(conversation).or_default();
                if started { set.insert(author) } else { set.remove(&author) };
            }
            Event::Call(update) => {
                // Ring while a call rings us; a notification when it starts.
                let ringing = update.as_ref().is_some_and(|u| u.incoming && u.status == langis_core::CallStatus::Ringing);
                let was_ringing = self.call.as_ref().is_some_and(|u| u.incoming && u.status == langis_core::CallStatus::Ringing);
                if ringing && !was_ringing {
                    if self.sound.ringtone {
                        crate::sounds::start_ring();
                    }
                    if self.sound.call_notify && self.notify.0 && !self.focused
                        && let Some(u) = update.as_ref()
                    {
                        // Desktop's incoming call notification plays TriTone.
                        if self.sound.notification {
                            crate::sounds::once(crate::sounds::Sound::TriTone);
                        }
                        // Desktop's `callingNotification`: no name when the
                        // content setting hides it.
                        let who = if self.notify.1 == "none" {
                            "langis".to_owned()
                        } else {
                            self.conversation(&u.conversation).map(|c| c.title.clone()).unwrap_or_else(|| "Someone".into())
                        };
                        let body = if u.video { "Incoming video call" } else { "Incoming voice call" };
                        let _ = self.loaded_tx.send(Loaded::Notify { title: who, body: body.into() });
                    }
                    if self.sound.attention {
                        self.ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Critical));
                    }
                } else if !ringing {
                    crate::sounds::stop_ring();
                }
                // Desktop's call tones (`callingTones`): the call ending,
                // a hand going up, someone starting to present.
                if let (Some(before), None) = (self.call.as_ref(), update.as_ref())
                    && !(before.incoming && before.status == langis_core::CallStatus::Ringing)
                {
                    crate::sounds::once(crate::sounds::Sound::HangUp);
                }
                if let (Some(before), Some(now)) = (self.call.as_ref(), update.as_ref()) {
                    let mine = now.local_demux_id;
                    if now.raised_hands.iter().any(|h| !before.raised_hands.contains(h) && Some(*h) != mine) {
                        crate::sounds::once(crate::sounds::Sound::HandRaised);
                    }
                    let presenting = |u: &langis_core::CallUpdate| u.remote_sharing || u.participants.iter().any(|p| p.presenting);
                    if presenting(now) && !presenting(before) {
                        crate::sounds::once(crate::sounds::Sound::Presenting);
                    }
                }
                if update.is_none() {
                    self.call_view = Default::default();
                } else if self.call.is_none() && self.share_available.is_none() {
                    // Learn whether Present can work, to show it or not.
                    self.send(Command::LoadShareSources);
                }
                self.call = update;
            }
            Event::CallDevices(devices) => self.call_devices = devices,
            Event::CameraFailed(why) => self.toast(format!("The camera stopped: {why}")),
            Event::ShareSources(result) => {
                self.share_available = Some(result.as_ref().is_ok_and(|s| !s.is_empty()));
                if !std::mem::take(&mut self.share_requested) {
                    return;
                }
                // On Linux the one source is the desktop portal, which shows
                // its own picker once sharing starts.
                match result {
                    Ok(sources) if sources.len() == 1 => {
                        self.send(Command::StartSharing { source: sources[0].id.clone() });
                    }
                    Ok(sources) if sources.is_empty() => self.toast("Screen sharing isn't available on this desktop."),
                    Ok(sources) => self.share_sources = Some(sources),
                    Err(why) => self.toast(format!("Can't share your screen: {why}")),
                }
            }
            Event::BadgesChanged => self.load_badges(),
            Event::OwnProfileChanged => self.load_me(),
            Event::AccountSettingsChanged => {
                self.settings.stale = true;
                self.load_notify_prefs();
                self.load_reactions();
                self.load_link_previews();
            }
            Event::ViewOnceOpened(opened) => self.open_view_once(opened),
            Event::MessageStickerPack { pack: Some((id, key)), .. } => {
                self.send(Command::PreviewStickerPack { id: id.clone(), key: key.clone() });
                self.fun.preview = Some((id, key, None));
            }
            Event::DownloadProgress { message_id, index, received, total } => {
                if received == 0 {
                    self.downloads.remove(&(message_id, index));
                } else {
                    self.downloads.insert((message_id, index), (received, total));
                }
            }
            Event::LinkPreview { url, preview } => {
                // The story composer's link, or the chat composer's.
                if let Some(c) = self.panes.stories.composer.as_mut()
                    && c.preview_for.as_deref() == Some(url.as_str())
                {
                    c.preview = preview.clone();
                }
                if let Some(chat) = self.chat.as_mut() {
                    chat.preview.arrived(&url, preview);
                }
            }
            Event::PinsChanged(conversation) => {
                if self.selected.as_ref() == Some(&conversation) {
                    self.load_pinned(conversation);
                }
            }
            Event::Error(text) => self.toast(text),
            other => crate::panes::on_event(self, other),
        }
    }

    fn on_loaded(&mut self, loaded: Loaded) {
        match loaded {
            Loaded::Found(found) => self.on_found(found),
            Loaded::Conversations(list) => {
                // Names for group authors and mentions come with the list.
                for c in &list {
                    if c.kind == langis_store::ConversationKind::NoteToSelf {
                        continue;
                    }
                    if let Some(sid) = c.id.as_direct() {
                        self.names.insert(sid.to_owned(), c.title.clone());
                    }
                }
                self.conversations = list;
            }
            Loaded::Messages { conversation, older, messages } => {
                if let Some(chat) = self.chat.as_mut().filter(|c| c.id == conversation) {
                    chat.take_page(messages, older);
                }
            }
            Loaded::Message(message) => {
                self.lightbox_update(&message);
                if let Some(chat) = self.chat.as_mut().filter(|c| c.id == message.conversation_id) {
                    chat.upsert(message);
                }
            }
            Loaded::Names(pairs) => self.names.extend(pairs),
            Loaded::Unverified { conversation, people, then } => {
                if people.is_empty() {
                    self.finish_send(then);
                } else {
                    let _ = conversation;
                    self.safety = Some(SafetyPrompt { people, then });
                }
            }
            Loaded::Settings(snapshot) => self.settings.snapshot = Some(*snapshot),
            Loaded::NotifyPrefs { on, content, nav_collapsed, sound } => {
                self.notify = (on, content);
                self.sound = sound;
                self.nav_collapsed = nav_collapsed;
            }
            Loaded::Me { name, photo } => {
                if let Some(name) = &name {
                    self.names.insert(self.aci.clone(), name.clone());
                }
                self.me = (name.unwrap_or_default(), photo);
            }
            Loaded::Notify { title, body } => {
                std::thread::spawn(move || {
                    let shown = notify_rust::Notification::new().appname("langis").summary(&title).body(&escape_markup(&body)).show();
                    if let Err(error) = shown {
                        tracing::warn!(%error, "couldn't show a notification");
                    }
                });
            }
            Loaded::Apply(apply) => apply(self),
            Loaded::Chat(crate::chat::ChatLoaded::Jump { conversation, message_id }) => self.open_at(conversation, message_id),
            Loaded::Chat(loaded) => {
                if let Some(chat) = self.chat.as_mut() {
                    chat.on_loaded(loaded);
                }
            }
            Loaded::Reactions(list) => self.reactions = list,
            Loaded::LinkPreviews(on) => self.link_previews = on,
            Loaded::Lightbox { conversation, message_id, index, items } => self.lightbox_loaded(conversation, message_id, index, items),
            Loaded::Theme(theme) => {
                self.theme = theme.unwrap_or_else(|| "system".into());
                self.apply_theme();
            }
        }
    }

    pub fn load_me(&self) {
        let aci = self.aci.clone();
        self.load(move |db| async move {
            let profile = langis_core::own_profile(&db, &aci).await;
            Some(Loaded::Me { name: profile.name(), photo: profile.photo.map(|p| p.to_string_lossy().into_owned()) })
        });
    }

    /// The reaction bar's emoji: the account's, with Desktop's defaults
    /// for any missing.
    pub fn load_reactions(&self) {
        self.load(|db| async move {
            let saved: Vec<String> = db
                .setting(langis_core::SETTING_PREFERRED_REACTIONS)
                .await
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            let list = crate::actions::DEFAULT_REACTIONS
                .iter()
                .enumerate()
                .map(|(i, d)| saved.get(i).filter(|e| !e.is_empty()).cloned().unwrap_or_else(|| d.to_string()))
                .collect();
            Some(Loaded::Reactions(list))
        });
    }

    /// Who shows which badge, and its downloaded images.
    pub fn load_badges(&self) {
        self.load(|db| async move {
            let holders = db.badge_holders().await.ok()?;
            let badges = db.badges().await.ok()?;
            let files = db.badge_image_files().await.ok()?;
            let mut out = std::collections::HashMap::new();
            for (sid, ids) in holders {
                let Some(badge) = ids.first().and_then(|id| badges.iter().find(|b| &b.id == id)) else { continue };
                // The second size fits a chat list avatar; else the first.
                let Some(image) = badge.images.get(1).or(badge.images.first()) else { continue };
                let path = |url: &Option<String>| url.as_ref().and_then(|u| files.get(u).cloned());
                let (light, dark) = (path(&image.light), path(&image.dark).or_else(|| path(&image.light)));
                if light.is_some() || dark.is_some() {
                    out.insert(sid, (light, dark));
                }
            }
            Some(Loaded::Apply(Box::new(move |app: &mut App| app.badges = out)))
        });
    }

    /// The badge image to draw on someone's avatar, for the theme.
    pub fn badge_for(&self, aci: &str) -> Option<&str> {
        let (light, dark) = self.badges.get(aci)?;
        let dark_theme = crate::theme::Palette::of(&self.ctx).dark;
        if dark_theme { dark.as_deref().or(light.as_deref()) } else { light.as_deref().or(dark.as_deref()) }
    }

    /// Desktop keeps the last 32 emoji used for its pickers' Recents.
    pub fn load_recent_emoji(&self) {
        self.load(|db| async move {
            let used: Vec<String> = db.emoji_usage().await.ok()?.into_iter().take(32).map(|u| u.emoji).collect();
            Some(Loaded::Apply(Box::new(move |app: &mut App| app.recent_emoji = used)))
        });
    }

    /// Notes an emoji was sent or reacted with, for Recents. The store
    /// keeps this for frontends to write (langis's frontend guide).
    pub fn use_emoji(&self, emoji: &str) {
        let emoji = emoji.to_owned();
        self.load(move |db| async move {
            db.record_emoji_use(&emoji).await.ok()?;
            let used: Vec<String> = db.emoji_usage().await.ok()?.into_iter().take(32).map(|u| u.emoji).collect();
            Some(Loaded::Apply(Box::new(move |app: &mut App| app.recent_emoji = used)))
        });
    }

    pub fn load_link_previews(&self) {
        self.load(|db| async move {
            let on = db.setting(langis_core::SETTING_LINK_PREVIEWS).await.ok().flatten().as_deref() != Some("false");
            Some(Loaded::LinkPreviews(on))
        });
    }

    pub fn load_pinned(&self, conversation: ConversationId) {
        self.load(move |db| async move {
            let messages = db.pinned_messages(&conversation).await.unwrap_or_default();
            Some(Loaded::Chat(crate::chat::ChatLoaded::Pinned { conversation, messages }))
        });
    }

    pub fn load_notify_prefs(&self) {
        self.load(|db| async move {
            let on = db.setting(crate::settings::SETTING_NOTIFICATIONS).await.ok().flatten().as_deref() != Some("false");
            let content = db.setting(crate::settings::SETTING_NOTIFICATION_CONTENT).await.ok().flatten().unwrap_or_else(|| "all".into());
            let nav_collapsed = db.setting(crate::settings::SETTING_NAV_TABS_COLLAPSED).await.ok().flatten().as_deref() == Some("true");
            let mut values: [Option<String>; 5] = Default::default();
            for (slot, key) in values.iter_mut().zip(crate::sounds::KEYS) {
                *slot = db.setting(key).await.ok().flatten();
            }
            let sound = crate::sounds::Prefs::from_values(values);
            Some(Loaded::NotifyPrefs { on, content, nav_collapsed, sound })
        });
    }

    /// A new message, as a desktop notification: not for the chat being
    /// read, and not for muted chats unless it mentions or answers us
    /// (Desktop's rules, without its per-kind exceptions).
    fn notify_message(&self, conversation: &ConversationId, message_id: i64, for_me: bool) {
        let (on, content) = self.notify.clone();
        let Some(chat) = self.conversation(conversation) else { return };
        let reading = self.focused && self.selected.as_ref() == Some(conversation);
        // Desktop's in-chat sound, for the chat being read.
        if reading && self.sound.in_chat && !chat.muted {
            crate::sounds::once(crate::sounds::Sound::Pop);
        }
        if !on || reading || (chat.muted && !for_me) {
            return;
        }
        if self.sound.notification {
            crate::sounds::once(crate::sounds::Sound::Pop);
        }
        if self.sound.attention {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
        let (title, group) = (chat.title.clone(), chat.kind == langis_store::ConversationKind::Group);
        if content == "none" {
            let loaded = Loaded::Notify { title: "langis".into(), body: "New message".into() };
            let _ = self.loaded_tx.send(loaded);
            return;
        }
        self.load(move |db| async move {
            let message = db.message(message_id).await.ok().flatten()?;
            let mut body = if content == "name" {
                "New message".to_owned()
            } else if message.body.is_empty() {
                message.attachments.first().map(|a| langis_store::attachment_preview(a).to_owned()).unwrap_or_else(|| "New message".into())
            } else {
                // Spoilers stay hidden, as Desktop's notification text does.
                langis_store::hide_spoilers(&message.body, &message.styles)
            };
            if group && content == "all" && let Some(author) = &message.author_name {
                body = format!("{author}: {body}");
            }
            Some(Loaded::Notify { title, body })
        });
    }

    pub fn apply_theme(&self) {
        let preference = match self.theme.as_str() {
            "light" => egui::ThemePreference::Light,
            "dark" => egui::ThemePreference::Dark,
            _ => egui::ThemePreference::System,
        };
        self.ctx.set_theme(preference);
    }

    pub fn reload_conversations(&self) {
        self.load(|db| async move { db.conversations().await.ok().map(Loaded::Conversations) });
        // The @ badge: unread mentions in each chat with unread messages.
        let me = self.aci.clone();
        self.load(move |db| async move {
            let mut counts = std::collections::HashMap::new();
            for c in db.conversations().await.ok()?.into_iter().filter(|c| c.unread_count > 0) {
                let n = db.unread_mentions(&c.id, &me).await.map(|m| m.len()).unwrap_or(0);
                if n > 0 {
                    counts.insert(c.id, n as u32);
                }
            }
            Some(Loaded::Apply(Box::new(move |app: &mut App| app.unread_mentions = counts)))
        });
        // Calls are messages: whatever changes the chat list may change this.
        self.load(|db| async move {
            let n = db.unseen_calls().await.ok()?;
            Some(Loaded::Apply(Box::new(move |app: &mut App| app.unseen_calls = n.max(0) as u32)))
        });
    }

    /// Opens a chat: saves the draft of the one being left, loads the
    /// newest messages and marks it read.
    pub fn open(&mut self, id: ConversationId) {
        if let Some(chat) = self.chat.take() {
            chat.save_draft(self);
            if chat.typing_sent {
                self.send(Command::Typing { conversation: chat.id.clone(), started: false });
            }
        }
        let draft = self.conversation(&id).and_then(|c| c.draft.clone()).unwrap_or_default();
        let styles = self.conversation(&id).map(|c| c.draft_styles.clone()).unwrap_or_default();
        let mut chat = ChatView::new(id.clone(), draft.clone());
        chat.unread_at_open = self.conversation(&id).map(|c| c.unread_count).unwrap_or(0);
        chat.spans = crate::composer::Spans::new(&draft, styles, Vec::new());
        self.chat = Some(chat);
        self.selected = Some(id.clone());
        self.tab = Tab::Chats;
        let members: Vec<String> = self.conversation(&id).map(|c| c.members.clone()).unwrap_or_default();
        let page = id.clone();
        self.load(move |db| async move {
            db.messages(&page, None, crate::chat::PAGE).await.ok().map(|messages| Loaded::Messages { conversation: page, older: false, messages })
        });
        if !members.is_empty() {
            self.load(move |db| async move { db.display_names(&members).await.ok().map(Loaded::Names) });
        }
        self.load_pinned(id.clone());
        // Whether its group has a call going, for the call notices' Join.
        if self.conversation(&id).is_some_and(|c| c.kind == langis_store::ConversationKind::Group) {
            self.send(Command::PeekGroupCall { conversation: id.clone() });
        }
        // Unread mentions are read before the chat is marked read, so its @
        // button has them.
        let me = self.aci.clone();
        self.load(move |db| async move {
            let mentions = db.unread_mentions(&id, &me).await.unwrap_or_default();
            Some(Loaded::Apply(Box::new(move |app: &mut App| {
                if let Some(chat) = app.chat.as_mut().filter(|c| c.id == id) {
                    chat.unread_mentions = mentions;
                }
                app.send(Command::MarkRead { conversation: id });
            })))
        });
    }

    /// Closes the open chat (Desktop's Ctrl+Shift+C), keeping its draft.
    pub fn close_chat(&mut self) {
        if let Some(chat) = self.chat.take() {
            chat.save_draft(self);
            if chat.typing_sent {
                self.send(Command::Typing { conversation: chat.id.clone(), started: false });
            }
        }
        self.selected = None;
        self.panes.details = None;
    }

    /// Opens a chat at one of its messages (a search result, a pin, a
    /// quote): the messages around it, scrolled to and flashed.
    pub fn open_at(&mut self, conversation: ConversationId, message_id: i64) {
        let loaded_here = self.chat.as_ref().is_some_and(|c| c.id == conversation && c.messages.iter().any(|m| m.id == message_id));
        if loaded_here {
            if let Some(chat) = self.chat.as_mut() {
                chat.scroll_target = Some(message_id);
            }
            return;
        }
        if self.selected.as_ref() != Some(&conversation) {
            self.open(conversation.clone());
        }
        let around = crate::chat::AROUND;
        self.load(move |db| async move {
            let messages = db.messages_around(&conversation, message_id, around, around).await.ok()?;
            Some(Loaded::Chat(crate::chat::ChatLoaded::Around { conversation, target: message_id, messages }))
        });
    }

    pub fn load_older(&mut self) {
        let Some(chat) = self.chat.as_mut() else { return };
        if chat.loading_older || chat.at_start {
            return;
        }
        let Some(oldest) = chat.messages.first().map(|m| m.id) else { return };
        chat.loading_older = true;
        let id = chat.id.clone();
        self.load(move |db| async move {
            db.messages(&id, Some(oldest), crate::chat::PAGE).await.ok().map(|messages| Loaded::Messages { conversation: id, older: true, messages })
        });
    }

    /// Sends after checking safety numbers, as Desktop does: anyone
    /// verified whose number changed holds the send until the user answers.
    pub fn send_checked(&self, conversation: ConversationId, then: PendingSend) {
        let Some(chat) = self.conversation(&conversation).cloned() else {
            self.finish_send(then);
            return;
        };
        let names = self.names.clone();
        self.load(move |db| async move {
            let unverified = langis_core::unverified_people(&db).await;
            let stopping = langis_core::stopping_send(&chat, &unverified);
            let people = stopping
                .into_iter()
                .map(|sid| {
                    let name = names.get(&sid).cloned().unwrap_or_else(|| chat.title.clone());
                    (sid, name)
                })
                .collect();
            Some(Loaded::Unverified { conversation, people, then })
        });
    }

    pub fn finish_send(&self, then: PendingSend) {
        match then {
            PendingSend::Command(command) => self.send(*command),
            PendingSend::Retry(message_id) => self.send(Command::RetrySend { message_id }),
        }
    }

    /// "Send anyway": accepts each new safety number, then sends.
    pub fn send_anyway(&mut self) {
        let Some(prompt) = self.safety.take() else { return };
        for (sid, _) in &prompt.people {
            self.send(Command::SetVerified { conversation: ConversationId::direct(sid), verified: false });
        }
        self.finish_send(prompt.then);
    }

    /// Desktop's consecutive playback: when a voice message ends, the
    /// next message plays if it's a voice message too.
    fn play_next_voice(&mut self, (conversation, message_id): (ConversationId, i64)) {
        let Some(chat) = self.chat.as_ref().filter(|c| c.id == conversation) else { return };
        let Some(at) = chat.messages.iter().position(|m| m.id == message_id) else { return };
        // Desktop's VoiceNoteEnd when a run of voice messages is over.
        let next_voice = chat.messages.get(at + 1).filter(|n| n.attachments.iter().any(|a| a.voice));
        if next_voice.is_none() && chat.messages[at].attachments.iter().any(|a| a.voice) {
            crate::sounds::once(crate::sounds::Sound::VoiceNoteEnd);
        }
        let Some(next) = next_voice else { return };
        let Some(voice) = next.attachments.iter().find(|a| a.voice) else { return };
        let Some(path) = voice.local_path.clone() else { return };
        let Some(peaks) = self.audio.peaks(&self.ctx, &path) else { return };
        let (next_id, incoming) = (next.id, next.direction == langis_store::Direction::Incoming);
        let title = if incoming { next.author.as_deref().map(|a| self.name(a)).unwrap_or_default() } else { "You".to_owned() };
        self.audio.toggle(next_id, &conversation, title, &path, peaks.duration, None);
        if incoming {
            self.send(Command::MarkViewed { message_id: next_id });
        }
    }

    pub fn shutdown(&mut self) {
        self.audio.stop();
        if let Some(chat) = self.chat.take() {
            chat.save_draft(self);
        }
        if let Some(backend) = self.backend.take() {
            backend.stop(Duration::from_secs(5));
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Photos not drawn for 30 seconds are let go.
        if self.last_evict.elapsed() > Duration::from_secs(5) {
            self.last_evict = Instant::now();
            self.thumbs.evict(ctx, Duration::from_secs(30));
        }
        if let Some(finished) = self.audio.tick(ctx) {
            self.play_next_voice(finished);
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        // Desktop refreshes the remote config when the window becomes active
        // (`maybeRefreshRemoteConfig`; langis waits two hours between).
        if focused && !self.focused && self.backend.is_some() {
            self.send(Command::RefreshRemoteConfig);
        }
        self.focused = focused;
        // The app redraws at least every 30 seconds; a much longer gap on
        // the wall clock means the machine slept. Desktop checks its
        // connections on resume (`powerMonitorResume`).
        let now = std::time::SystemTime::now();
        if let Some(last) = self.last_tick
            && now.duration_since(last).is_ok_and(|gap| gap > Duration::from_secs(90))
            && self.backend.is_some()
        {
            self.send(Command::Resumed);
        }
        self.last_tick = Some(now);
        // Desktop's `requestCloseConfirmation`: closing during a call asks
        // first.
        let closing = self.closing.load(std::sync::atomic::Ordering::Relaxed);
        if ctx.input(|i| i.viewport().close_requested()) && !closing && self.in_call() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        self.poll();
        self.diag.frame(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::view::show(self, ui);
    }

    /// Touchpad scrolling as Chromium's (`scroll.rs`). `LANGIS_SCROLL_SPEED`
    /// overrides the 2.5 times speed.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        static SPEED: std::sync::LazyLock<f32> = std::sync::LazyLock::new(|| {
            std::env::var("LANGIS_SCROLL_SPEED").ok().and_then(|v| v.parse().ok()).filter(|v: &f32| *v > 0.).unwrap_or(2.5)
        });
        self.momentum.apply(raw_input, *SPEED);
        if self.momentum.flinging() {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self) {
        self.shutdown();
        if let Some(drops) = &self.drops {
            drops.stop();
        }
        crate::video::clear_posters();
    }
}

impl App {
    /// In a call, or calling out (not just rung).
    pub fn in_call(&self) -> bool {
        self.call.as_ref().is_some_and(|c| c.status != langis_core::CallStatus::Ringing)
    }

    /// Desktop's in-call close confirmation; Close ends the call and quits.
    pub fn close_dialog(&mut self, ctx: &egui::Context) {
        use crate::dialog::{self, Answer, Variant};
        if !self.confirm_close {
            return;
        }
        let answer = dialog::show(ctx, egui::Id::unique("close-during-call"), "Close langis and end the call?", "", Some("Not now"), &[("Close", Variant::StrongDestructive)]);
        match answer {
            Some(Answer::Action(_)) => {
                self.confirm_close = false;
                self.closing.store(true, std::sync::atomic::Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(Answer::Cancel) => self.confirm_close = false,
            None => {}
        }
    }
}

/// SIGTERM, SIGINT and SIGHUP close the window, so the app stops as on
/// a click of close: the backend hangs up a call and closes its databases.
/// A second signal exits at once.
fn quit_on_signals(ctx: &egui::Context) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    use std::sync::atomic::Ordering;
    let closing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT, signal_hook::consts::SIGHUP]);
    match signals {
        Ok(mut signals) => {
            let (ctx, flag) = (ctx.clone(), closing.clone());
            let _ = std::thread::Builder::new().name("langis-signals".into()).spawn(move || {
                for signal in signals.forever() {
                    if flag.swap(true, Ordering::Relaxed) {
                        std::process::exit(128 + signal);
                    }
                    tracing::info!(signal, "closing on a signal");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx.request_repaint();
                }
            });
        }
        Err(error) => tracing::warn!(%error, "can't listen for signals"),
    }
    closing
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("langis-gui")
}

/// What the phone's Linked devices list shows.
pub fn device_name() -> String {
    let host = std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_owned()).filter(|h| !h.is_empty());
    match host {
        Some(host) => format!("langis on {host}"),
        None => "langis".into(),
    }
}

const ALREADY_RUNNING: &str = "langis-gui is already running for this account. Switch to its window (or close it first).";

/// Whether another running copy holds the data directory (the backend's
/// `langis.lock`). Checked up front so a passphrase isn't asked for first;
/// `Backend::start` still has the final say.
fn in_use(dir: &std::path::Path) -> bool {
    let Ok(file) = std::fs::File::open(dir.join("langis.lock")) else { return false };
    matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Notification servers that render body markup would draw a sender's
/// `<a href>` or `<img>`; escaped, it shows as typed.
fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
