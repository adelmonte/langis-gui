# Working on langis-gui

langis-gui is a desktop Signal client for Linux in egui/eframe 0.37 (wgpu),
on top of langis (`../langis`, the UI-free backend). It copies Signal
Desktop's look and layout. It is the user's daily-driver replacement for
the Electron Signal Desktop, for niri (Wayland) on Arch.

## How it talks to langis

Follow `../langis/docs/frontend-guide.md`. The UI thread never blocks:
- `Command`s go out with `App::send`.
- `Event`s are polled each frame in `App::poll`.
- Store reads run on the backend's runtime through `App::load`, and come
  back as `Loaded` values.

The app may only write its own settings (`ui.` keys) and drafts directly.
Anything else goes through a `Command`. If the GUI needs something langis
doesn't offer, add it to langis (its Command/Event/store API, documented
in its guide), not a workaround here.

## Layout of src/

| File | What it covers |
|---|---|
| `main.rs` | Arguments (`--demo`, `--data-dir`), the window and its icon. |
| `app.rs` | App state, the start flow (key, lock check, link), `poll`, `Loaded`. |
| `startup.rs` | Setup, unlock, link QR and failure screens. |
| `view.rs` | Top-level layout, the safety-number prompt. |
| `sidebar.rs` | Nav rail, chat list. |
| `panes.rs` | Calls and stories tabs, dialogs, menus, message requests, new chat and group. |
| `chat.rs` | Header, timeline, bubbles. |
| `actions.rs` | Message hover bar, menu, reactions, dialogs. |
| `composer.rs` | Input, formatting, mentions, link previews, attachments. |
| `fun.rs`, `emoji_picker.rs` | Sticker/GIF/emoji picker, sticker manager. |
| `polls.rs` | Polls. |
| `voice.rs` | Voice messages (recording uses `ffmpeg`). |
| `lightbox.rs` | Photo/video viewer. |
| `search.rs` | Message search. |
| `details.rs` | Chat details, safety number. |
| `media_gallery.rs` | All media: photos and videos, audio, links, files. |
| `chat_colors.rs` | Chat colors: presets, custom colors, bubble gradients, the picker. |
| `profile.rs` | Profile, username. |
| `stories.rs`, `calls_tab.rs`, `call.rs` | Stories, call history and links, the call screen. |
| `settings.rs` | Settings. |
| `backups.rs` | Settings > Backups, the backup media download banner and its settings row. |
| `notification_profiles.rs` | Notification profiles: settings pages, the chat list menu and indicator. |
| `menu.rs`, `dialog.rs`, `axo_list.rs` | Desktop's Axo menus, dialogs and settings lists. |
| `theme.rs`, `widgets.rs`, `icons.rs`, `glyphs.rs`, `format.rs`, `notices.rs` | Shared drawing and text. |

Colors and sizes come from Signal Desktop's stylesheets (its Axo tokens).
Icons in `assets/icons` are Signal Desktop's v3 SVGs (AGPL, credited in
NOTICE). Fonts are Inter (OFL). The app icon in `assets/app` is our own;
never use Signal's logo or name as the app's identity.

## Testing

- `cargo test` runs the unit tests.
- `cargo test screenshots -- --ignored --test-threads=1` renders every
  screen on demo data into `target/screenshots/`. It needs Mesa's
  `vulkan-swrast` when there's no GPU. Look at the PNGs after UI changes.
- Run the app on demo data with `cargo run -- --demo`. Never point a test
  or a demo at a real account's data directory.
- Not tested yet: clicking through a real window, and a real account.

## Notes

- `[profile.release]` has no LTO: RingRTC's cubeb-sys links a prebuilt
  Rust static library with its own core, and LTO makes the symbols clash.
- Inter lacks some symbols (✕, ✓). A missing glyph crashed the
  screenshot test, so use words or "×".
- Commits: atomic, as your own git identity (adelmonte),
  with no Co-Authored-By or Claude attribution lines. Don't run
  `cargo fmt`. No em dashes in docs.
- The repo is public on GitHub (adelmonte/langis-gui, as is
  adelmonte/langis). Push only when the user asks. No screenshots in the
  repo except the one the user adds to the README.
