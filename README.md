# langis-gui

Signal Desktop, 1:1, in native Rust. No Electron.

Same layout, colors, fonts, icons and menus as Signal Desktop, drawn with
[egui](https://github.com/emilk/egui) on top of
[langis](https://github.com/adelmonte/langis). It links to your phone as a
secondary device, just like Desktop: chats, groups, calls, stories,
reactions, polls, voice messages, stickers and the rest.

Unofficial, and not affiliated with Signal.

## Not there yet

- Video stories don't play.
- Chat folders can't be reordered.
- Signal links from other apps aren't handed over (paste them into New chat).

## Install

Needs [langis](https://github.com/adelmonte/langis) checked out next to it
(`../langis`) and its build tools: `protoc`, `clang`, `cmake`, PulseAudio
and PipeWire headers. For color emoji, install Noto Color Emoji.

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

## License

AGPL-3.0-only. See LICENSE and NOTICE.
