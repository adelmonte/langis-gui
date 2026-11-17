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

What's in place (each screen checked by screenshot on demo data; clicks
in a real window and a real account are not tested yet):

- **Start:** a passphrase or key file for the database, linking by QR
  code, a clear message if another copy is already running.
- **Chats:** the chat list with search (chats and message results),
  folders, archive, pin, mute, mark read, message requests, megaphones;
  new chat by name, number or username, new group, pasted group and call
  links.
- **Conversation:** Desktop's bubbles, day headers, delivery status, the
  disappearing timer, reactions, quotes, photos, files, voice messages,
  stickers, polls, link previews, mentions and text styles; hover bar and
  context menu (reply, react, copy, forward, edit, delete for me or
  everyone, pin, info, save); search in the chat (Ctrl+F); the photo and
  video viewer; view-once.
- **Composer:** formatting, @mentions, link previews, attachments,
  view-once, polls, voice recording (needs `ffmpeg`), emoji, sticker and
  GIF picker, drafts, editing.
- **Chat details:** members and admins, permissions, group link and
  requests, disappearing timer, mute, block, nickname, safety number with
  its QR code, leave or end group, delete.
- **Calls:** 1:1 and group calls, the lobby, video tiles, mute, camera,
  screen sharing through the desktop's picker (quality in Settings), raise
  hand, reactions, call links, the calls tab with history.
- **Stories:** the list, the viewer, replies and reactions, posting text
  and photo stories, story privacy.
- **Profile and settings:** name, about, photo, username and its link;
  appearance, chats, calls, notifications, privacy, data usage, chat
  folders, and unlinking this device. Desktop notifications.

Not yet: video stories don't play in the viewer, chat folders can't be
reordered, and Signal links from other apps aren't handed over (paste
them into New chat).

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

The icon (`assets/app/`) is langis's own: a round bubble in Signal's blue
with its tail to the right, a ring of dots and an l. It is not Signal's logo.

## Screenshots

```sh
cargo test screenshots -- --ignored --nocapture
```

renders the demo to `target/screenshots/*.png`. It needs a Vulkan or GL
driver; Mesa's software Vulkan driver (`vulkan-swrast`) is enough.

## License

AGPL-3.0-only. See LICENSE and NOTICE.
