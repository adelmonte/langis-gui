# langis-gui

Signal Desktop, 1:1, in native Rust. No Electron.

Same layout, colors, fonts, icons and menus as Signal Desktop, drawn with
[egui](https://github.com/emilk/egui) on top of
[langis](https://github.com/adelmonte/langis). It links to your phone as a
secondary device, just like Desktop: chats, groups, calls, stories,
reactions, polls, voice messages, stickers and the rest.

Calls minimize to a small window so you can keep chatting, and Desktop's
keyboard shortcuts work (Ctrl+/ lists them).

Unofficial, and not affiliated with Signal.

## Not there yet

Checked against Signal Desktop 8.38's source in November 2026. Items
marked (langis) need backend work first.

Messages and the composer:
- Several photos in one message stack instead of sitting in a grid, and
  there's no "Download N items".
- Files send the moment they're picked: no staged list, no paste or
  drag-and-drop, no view-once toggle in the composer.
- Videos show a gray box until opened (langis: no poster frame), GIFs
  don't autoplay, and undownloaded media has no blurhash (langis).
- No download progress or cancel (langis).
- Stickers and view-once messages don't show reactions, the author's
  name in groups, or their send status.
- Notices have no buttons (Call back, Join, View safety number, View
  poll, Go to message), release-note buttons don't show, and some notice
  kinds aren't produced (langis).
- Partly sent, paused, edit failed and delete failed aren't shown.
- "Edited" doesn't open the edit history; pinned messages have no pin
  mark; the pinned bar has no menu or pinned list.
- Clicking a reaction doesn't open who reacted; no Customize reactions.
- No sticker favorite in the message menu.
- Quotes are one line with no type icons or "Original message not
  found"; the reply bar is text only.
- No jump to unread mentions, and no unread count on the scroll-down
  button.
- Member labels don't show beside names in groups.
- No :emoji completion and no expanded composer.
- Bare domains (example.com) aren't links, and links stay clickable in
  unaccepted message requests.
- One typing bubble per person instead of one shared.
- Message info has no preview of the message or per-person icons.
- Search in a chat is a find bar, not Desktop's results list.
- Chats are marked read even when scrolled far up.
- "Now" and "5m" don't update while the window sits idle.
- A pending request to join a group can't be cancelled.

Chat list, details and settings:
- Search skips archived chats and only matches titles; no unread
  filter, clear button or "search in" chip.
- Chat folders can't be reordered, have no menu or unread counts, and
  the folder editor has no exclusions.
- Chat rows lack the Message request and Blocked labels, the @ badge
  (langis) and the Pinned and Chats headers; the archive has no help or
  empty text.
- The history transfer from the phone can't be skipped.
- Message request Block, Delete and Report act without asking first.
- Group invitations and pending join requests aren't shown as such.
- Chat details: no Notifications section (per-chat while-muted needs
  langis), no Groups in common, no call history, member photos missing
  in the list, no member search, no Share on the group link page, a
  simpler Requests and invites page, and no emoji in member labels.
- Timer pickers have no "Custom time".
- The member-label permission isn't there (langis).
- Photos withheld in message requests can't be downloaded.
- The contact card lacks Add to a group, in-call Mute and Remove, and
  the About view.
- No username QR code screen or link color; avatar and About editing
  are basic.
- Settings lack Add blocked user, notification sounds and ringtone,
  tray and autostart, spell check, emoji conversion, skin tone, media
  quality (langis), a Giphy key field (langis), and the Backups,
  Notification profiles and Export pages (langis).
- About 15 of Desktop's keyboard shortcuts are missing.
- The safety number change prompt has no per-person Verify.
- Profile badges aren't drawn, and waking from sleep isn't reported to
  langis.

Media, stories and calls:
- Videos don't play in the app (the bubble, viewer and media gallery
  open them in another player), and video stories don't play.
- The GIF picker's tiles are blank, and it has no categories or recents.
- Animated stickers and images show their first frame only.
- The media viewer stops at the newest 500 items (langis).
- In All media, audio and files open in other apps, and items have no
  menu.
- Stories: a fixed 6 seconds each, three reaction emoji, no viewers
  list, no My Stories page, no hiding, no thumbnails, basic creators,
  and no group story management.
- No ringtone or notification for incoming calls.
- 1:1 calls skip the lobby, and the lobby has no camera preview or mic
  toggle.
- No speaking indicators (langis), and people past the overflow column
  aren't shown.
- The Calls tab has no missed-calls filter or Join for active calls;
  call links have no Share via Signal and delete without asking.
- Emoji have no skin tones; stickers have no recents and packs can't be
  reordered.
- Signal links from other apps aren't handed over (paste them into New
  chat).

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

## License

AGPL-3.0-only. See LICENSE and NOTICE.
