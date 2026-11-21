# langis-gui

Signal Desktop, 1:1, in native Rust. No Electron.

Same layout, colors, fonts, icons and menus as Signal Desktop, drawn with
[egui](https://github.com/emilk/egui) on top of
[langis](https://github.com/adelmonte/langis). It links to your phone as a
secondary device, just like Desktop: chats, groups, calls, stories,
reactions, polls, voice messages, stickers and the rest.

Unofficial, and not affiliated with Signal.

![langis-gui in dark mode: the chat list and a group chat](docs/screenshot.png)

The picture is the demo account with made-up chats
(`cargo test readme_screenshot -- --ignored` renders it).

<details>
<summary><b>What it does</b></summary>

Everything a linked Signal Desktop does, with its layout and wording
(checked against Desktop 8.39):

- Chats, groups, message requests, chat folders, pinned and archived chats,
  search (also within a chat), drafts, mentions, formatting, link previews,
  quotes, private replies, edits, deletes, reactions, polls, view-once
  media, disappearing messages, voice messages, stickers, GIFs and emoji.
- Group admin: members, admins, labels, permissions, group links, join
  requests, invites.
- Calls: 1:1 and group video and voice calls, call links, screen sharing,
  reactions, raised hands, a picture-in-picture window.
- Stories: viewing, replies and reactions, posting text and photo stories,
  story privacy and lists.
- Settings: profile and username, privacy, notifications and notification
  profiles, per-chat mute options, chat colors, appearance, 51 languages,
  backups (Signal Secure Backups status, and Desktop backups to a folder),
  linked device info, debug log.
- Sticker pack creator, media editor, photo and video viewer, all media.
- System tray, Desktop's keyboard shortcuts (Ctrl+/ lists them), Signal
  links from other apps, one copy per account.

</details>

<details>
<summary><b>Not there yet</b></summary>

- Desktop backups work, but no backup made here has been restored on a
  phone yet.
- Sent media quality (Standard/High) is left out by design, and donations
  on purpose.
- Languages written right to left, and others that need complex text
  shaping, aren't offered: egui can't draw them yet. Chinese, Japanese and
  Korean need a system font.
- Hiding the window to the tray may not work on every Wayland compositor,
  and dropping files on the window may not either (winit's support).

Calls work on Linux in real use. Not yet tried on a real account: sounds,
paste and drag-and-drop, chat export, notification profiles, backups, the
sticker pack creator, linking errors and relinking, removing a contact, and
opening links from other apps.

</details>

## Install

Needs [langis](https://github.com/adelmonte/langis) checked out next to it
(`../langis`) and its build tools: `protoc`, `clang`, `cmake`, PulseAudio
and PipeWire headers. At runtime, voice messages need `ffmpeg` (recording
and playback) and PipeWire's `pw-play` (playback). For color emoji, install
Noto Color Emoji.

```sh
./install.sh               # builds and installs for your user
./install.sh --uninstall   # removes it; your data stays
```

Or run it in place:

```sh
cargo run --release
cargo run --release -- --demo   # made-up chats, no account
```

Your data lives in `~/.local/share/langis-gui`, encrypted.

The installed launcher entry opens `sgnl://` and `signalcaptcha://` links
(install.sh makes langis their default handler, in place of Signal
Desktop). Browsers keep https signal.me, signal.group, signal.link and
signal.art links for themselves, but `langis-gui <link>` opens those too.
As in Signal Desktop, one copy runs per account: launching it again shows
the running window and hands it the link.

## License

AGPL-3.0-only. See LICENSE and NOTICE.
