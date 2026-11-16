# What langis-gui would like from langis

Things the GUI works around today. None block it.

1. **docs/frontend-guide.md says `messages()` returns newest first.** It
   returns the page oldest first (`out.reverse()` in `AppDb::messages`).
   langis-gui relies on the actual order.
2. **No commands for local-only settings.** The theme
   (`SETTING_THEME`), "Enable incoming calls" (`SETTING_INCOMING_CALLS`) and
   "Always relay calls" (`SETTING_ALWAYS_RELAY_CALLS`) have no `Command`, so
   langis-gui writes them with `AppDb::set_setting`. The core reads them as
   it goes, so it works, but the guide says to change state through commands.
   A `Command::SetLocalSetting` (or one per setting) would settle it.
