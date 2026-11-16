# langis-gui

A desktop Signal client for Linux, drawn with [egui](https://github.com/emilk/egui)
on top of [langis](../langis). It is laid out like Signal Desktop: the nav
tabs, the chat list, the conversation with its bubbles, the composer, calls
and settings, in Desktop's colors and font.

It is unofficial and not affiliated with Signal. It links to your phone as
a secondary device, as Signal Desktop does, so your phone keeps its
messages.

## Status

Early. Checked so far:

- It builds and runs on demo data (`--demo`), and its screenshots
  (below) are compared against Signal Desktop by eye.
- Nothing has run against a real account yet.

Works: linking with a QR code; a passphrase or key file for the database;
the chat list with search, unread counts, drafts and typing; the timeline
with grouped bubbles, day headers, delivery status, reactions, quotes,
photos, files, link previews, mentions and text styles; sending text,
photos and files, replies and reactions; the safety number prompt
("Send anyway"); incoming calls, the call screen with video, mute,
camera, present and hang up; group call tiles; desktop notifications;
settings for theme, chats, calls (devices, screen share quality),
notifications, privacy and auto-download.

Not yet: stories, the calls tab, new chat and group creation, chat
details and menus, editing and deleting messages, voice messages, stickers
and GIF pickers, polls, the image viewer, message search results, and
anything else Desktop does past the list above.

## Building

langis-gui expects langis next to it (`../langis`), and needs what langis
needs (`protoc`, `clang`, `cmake`, PulseAudio and PipeWire headers; see
langis's README). For color emoji, install Noto Color Emoji
(Arch: `noto-fonts-emoji`); without it, emoji are drawn in one color.

```sh
cargo run --release -- --demo       # made-up chats, no network
cargo run --release                 # your account, in ~/.local/share/langis-gui
cargo run --release -- --data-dir DIR
```

The first build downloads Signal's prebuilt WebRTC into `target/webrtc`.

### Installing

`./install.sh` builds a release and installs it for your user: the
program in `~/.cargo/bin`, a launcher entry and the icon under
`~/.local/share`, so it shows up in app launchers (fuzzel, wofi, rofi and
the like on niri). `./install.sh --uninstall` removes them; your data in
`~/.local/share/langis-gui` stays.

The icon (`assets/app/`) is langis-gui's own: a bubble turned the wrong
way, around a backwards L. It is not Signal's logo.

## Screenshots

```sh
cargo test screenshots -- --ignored --nocapture
```

renders the demo to `target/screenshots/*.png`. It needs a Vulkan or GL
driver; Mesa's software Vulkan driver (`vulkan-swrast`) is enough.

## License

AGPL-3.0-only. See LICENSE and NOTICE.
