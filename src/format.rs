//! Times, sizes and names as Signal Desktop writes them.

use chrono::{DateTime, Datelike as _, Local, TimeZone as _};
use crate::i18n::tr;

fn local(millis: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(millis).single()
}

/// The chat list's time: "now", "5m", "3:41 PM", "Mon", "Mar 3", "3/3/24".
pub fn short_timestamp(millis: i64) -> String {
    if millis <= 0 {
        return String::new();
    }
    let Some(time) = local(millis) else { return String::new() };
    let now = Local::now();
    let age = now.signed_duration_since(time);
    if age.num_seconds() < 60 {
        tr("now").into()
    } else if age.num_minutes() < 60 {
        crate::i18n::tf("icu:minutesAgo", &format!("{}m", age.num_minutes()), &[("minutes", crate::i18n::Arg::from(age.num_minutes()))])
    } else if time.date_naive() == now.date_naive() {
        localized(&time, "%-I:%M %p")
    } else if (now.date_naive() - time.date_naive()).num_days() < 7 {
        localized(&time, "%a")
    } else if time.year() == now.year() {
        localized(&time, "%b %-d")
    } else {
        localized(&time, "%-m/%-d/%y")
    }
}

/// A message's time under its text.
pub fn message_time(millis: i64) -> String {
    let Some(time) = local(millis) else { return String::new() };
    let age = Local::now().signed_duration_since(time);
    if age.num_seconds() < 60 {
        tr("Now").into()
    } else if age.num_minutes() < 60 {
        crate::i18n::tf("icu:minutesAgo", &format!("{}m", age.num_minutes()), &[("minutes", crate::i18n::Arg::from(age.num_minutes()))])
    } else {
        localized(&time, "%-I:%M %p")
    }
}

/// The timeline's day separator: "Today", "Yesterday", "Monday",
/// "Mon, Mar 3", "Mar 3, 2024".
pub fn day_label(millis: i64) -> String {
    let Some(time) = local(millis) else { return String::new() };
    let today = Local::now().date_naive();
    let day = time.date_naive();
    match (today - day).num_days() {
        0 => tr("Today").into(),
        1 => tr("Yesterday").into(),
        2..=6 => localized(&time, "%A"),
        _ if day.year() == today.year() => localized(&time, "%a, %b %-d"),
        _ => localized(&time, "%b %-d, %Y"),
    }
}

pub fn same_day(a: i64, b: i64) -> bool {
    match (local(a), local(b)) {
        (Some(a), Some(b)) => a.date_naive() == b.date_naive(),
        _ => false,
    }
}

const MINUTE: u32 = 60;
const HOUR: u32 = 60 * MINUTE;
const DAY: u32 = 24 * HOUR;
const WEEK: u32 = 7 * DAY;
const UNITS: [(u32, &str); 5] = [(1, "second"), (MINUTE, "minute"), (HOUR, "hour"), (DAY, "day"), (WEEK, "week")];

/// A disappearing message timer: "1 day", "5 minutes", "Off".
pub fn timer_long(seconds: u32) -> String {
    if seconds == 0 {
        return tr("Off").into();
    }
    let (n, unit) = UNITS.iter().rev().map(|(size, unit)| (seconds / size, *unit)).find(|(n, _)| *n > 0).unwrap_or((seconds, "second"));
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

pub fn file_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.),
        b if b < 1024 * 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024. * 1024.)),
        b => format!("{:.2} GB", b as f64 / (1024. * 1024. * 1024.)),
    }
}

/// An avatar's initials, as Desktop's `getInitials`: the first letters of
/// the first two words, or none for a name with no letters (a phone number).
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_alphabetic)).collect();
    match words.as_slice() {
        [] => String::new(),
        [one] => one.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default(),
        [first, .., last] => [first, last].iter().filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect(),
    }
}

/// Lines and runs of spaces as single spaces, for one-line previews.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A call's length: "4:05", "1:02:09".
pub fn duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_like_desktop() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("Grace Brewster Hopper"), "GH");
        assert_eq!(initials("cher"), "C");
        assert_eq!(initials("+1 555 0100"), "");
    }

    #[test]
    fn timers_and_durations() {
        assert_eq!(timer_long(86_400), "1 day");
        assert_eq!(timer_long(300), "5 minutes");
        assert_eq!(timer_long(0), "Off");
        assert_eq!(duration(245), "4:05");
        assert_eq!(duration(3729), "1:02:09");
    }
}

/// chrono's locale for the app's language.
fn chrono_locale() -> Option<chrono::Locale> {
    let code = crate::i18n::locale();
    let name = match code {
        "en" => return None,
        "zh-Hant" => "zh_TW".to_owned(),
        "yue" => "yue_HK".to_owned(),
        "tl-PH" => "fil_PH".to_owned(),
        c if c.contains('-') => c.replace('-', "_"),
        "ja" => "ja_JP".into(),
        "ko" => "ko_KR".into(),
        "cs" => "cs_CZ".into(),
        "da" => "da_DK".into(),
        "el" => "el_GR".into(),
        "nb" => "nb_NO".into(),
        "sv" => "sv_SE".into(),
        "sr" => "sr_RS".into(),
        "ms" => "ms_MY".into(),
        "uk" => "uk_UA".into(),
        "vi" => "vi_VN".into(),
        "sw" => "sw_KE".into(),
        "ca" | "eu" | "gl" => format!("{c}_ES", c = code),
        c => format!("{c}_{}", c.to_uppercase()),
    };
    chrono::Locale::try_from(name.as_str()).ok()
}

/// `time` as `english` (a chrono format) would show it, in the app's
/// language: names translated, and a 24-hour clock outside English.
pub fn localized<Tz: chrono::TimeZone>(time: &chrono::DateTime<Tz>, english: &str) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match chrono_locale() {
        None => time.format(english).to_string(),
        Some(locale) => {
            let f = english.replace("%-I:%M %p", "%H:%M").replace(" at ", " ");
            time.format_localized(&f, locale).to_string()
        }
    }
}
