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
    Conversations(Vec<Conversation>),
    Messages { conversation: ConversationId, older: bool, messages: Vec<Message> },
    Message(Message),
    Names(Vec<(String, String)>),
    /// Who's stopping a send (`(service id, name)`), and what to do after.
    Unverified { conversation: ConversationId, people: Vec<(String, String)>, then: PendingSend },
    Settings(Box<crate::settings::Snapshot>),
    Theme(Option<String>),
    /// Our notification settings: shown at all, and what they show.
    NotifyPrefs { on: bool, content: String },
    /// A desktop notification to show.
    Notify { title: String, body: String },
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
    Settings,
}

pub struct Toast {
    pub text: String,
    pub until: Instant,
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
    pub connection: ConnectionState,
    pub history: Option<HistoryProgress>,
    pub tab: Tab,
    pub conversations: Vec<Conversation>,
    pub search: String,
    pub selected: Option<ConversationId>,
    pub chat: Option<ChatView>,
    pub names: HashMap<String, String>,
    pub typing: HashMap<ConversationId, HashSet<String>>,
    pub call: Option<CallUpdate>,
    pub call_view: crate::call::CallView,
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
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> Self {
        let ctx = cc.egui_ctx.clone();
        egui_extras::install_image_loaders(&ctx);
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
            connection: ConnectionState::Offline,
            history: None,
            tab: Tab::Chats,
            conversations: Vec::new(),
            search: String::new(),
            selected: None,
            chat: None,
            names: HashMap::new(),
            typing: HashMap::new(),
            call: None,
            call_view: Default::default(),
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
        };
        let dir = &app.data_dir;
        if app.demo || dir.join("database.key").exists() {
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
        self.toasts.push(Toast { text: text.into(), until: Instant::now() + Duration::from_secs(4) });
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
                    if error.contains("wrong passphrase") {
                        self.phase = Phase::Unlock { passphrase: String::new(), error: Some("That passphrase isn't right.".into()) };
                    } else {
                        self.phase = Phase::Failed(error);
                    }
                }
            }
        }
        while let Some(event) = self.events.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.on_event(event);
        }
        while let Ok(loaded) = self.loaded.try_recv() {
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
            Event::Ready { aci, .. } => {
                self.aci = aci;
                self.phase = Phase::Ready;
                self.reload_conversations();
                self.load_notify_prefs();
                self.send(Command::LoadCallDevices);
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
                    if self.focused {
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
            Event::AccountSettingsChanged => {
                self.settings.stale = true;
                self.load_notify_prefs();
            }
            Event::Error(text) => self.toast(text),
            _ => {}
        }
    }

    fn on_loaded(&mut self, loaded: Loaded) {
        match loaded {
            Loaded::Conversations(list) => {
                // Names for group authors and mentions come with the list.
                for c in &list {
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
            Loaded::NotifyPrefs { on, content } => self.notify = (on, content),
            Loaded::Notify { title, body } => {
                std::thread::spawn(move || {
                    let shown = notify_rust::Notification::new().appname("langis-gui").summary(&title).body(&body).show();
                    if let Err(error) = shown {
                        tracing::warn!(%error, "couldn't show a notification");
                    }
                });
            }
            Loaded::Theme(theme) => {
                self.theme = theme.unwrap_or_else(|| "system".into());
                self.apply_theme();
            }
        }
    }

    pub fn load_notify_prefs(&self) {
        self.load(|db| async move {
            let on = db.setting(crate::settings::SETTING_NOTIFICATIONS).await.ok().flatten().as_deref() != Some("false");
            let content = db.setting(crate::settings::SETTING_NOTIFICATION_CONTENT).await.ok().flatten().unwrap_or_else(|| "all".into());
            Some(Loaded::NotifyPrefs { on, content })
        });
    }

    /// A new message, as a desktop notification: not for the chat being
    /// read, and not for muted chats unless it mentions or answers us
    /// (Desktop's rules, without its per-kind exceptions).
    fn notify_message(&self, conversation: &ConversationId, message_id: i64, for_me: bool) {
        let (on, content) = self.notify.clone();
        let Some(chat) = self.conversation(conversation) else { return };
        let reading = self.focused && self.selected.as_ref() == Some(conversation);
        if !on || reading || (chat.muted && !for_me) {
            return;
        }
        let (title, group) = (chat.title.clone(), chat.kind == langis_store::ConversationKind::Group);
        if content == "none" {
            let loaded = Loaded::Notify { title: "langis-gui".into(), body: "New message".into() };
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
                message.body.clone()
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
        self.chat = Some(ChatView::new(id.clone(), draft));
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
        self.send(Command::MarkRead { conversation: id });
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

    pub fn shutdown(&mut self) {
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
        self.focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        self.poll();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::view::show(self, ui);
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("langis-gui")
}

/// What the phone's Linked devices list shows.
fn device_name() -> String {
    let host = std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_owned()).filter(|h| !h.is_empty());
    match host {
        Some(host) => format!("langis-gui on {host}"),
        None => "langis-gui".into(),
    }
}
