//! Times, sizes and names as Signal Desktop writes them.

use chrono::{DateTime, Datelike as _, Local, TimeZone as _};

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
        "now".into()
    } else if age.num_minutes() < 60 {
        format!("{}m", age.num_minutes())
    } else if time.date_naive() == now.date_naive() {
        time.format("%-I:%M %p").to_string()
    } else if (now.date_naive() - time.date_naive()).num_days() < 7 {
        time.format("%a").to_string()
    } else if time.year() == now.year() {
        time.format("%b %-d").to_string()
    } else {
        time.format("%-m/%-d/%y").to_string()
    }
}

/// A message's time under its text.
pub fn message_time(millis: i64) -> String {
    let Some(time) = local(millis) else { return String::new() };
    let age = Local::now().signed_duration_since(time);
    if age.num_seconds() < 60 {
        "Now".into()
    } else if age.num_minutes() < 60 {
        format!("{}m", age.num_minutes())
    } else {
        time.format("%-I:%M %p").to_string()
    }
}

/// The timeline's day separator: "Today", "Yesterday", "Monday",
/// "Mon, Mar 3", "Mar 3, 2024".
pub fn day_label(millis: i64) -> String {
    let Some(time) = local(millis) else { return String::new() };
    let today = Local::now().date_naive();
    let day = time.date_naive();
    match (today - day).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => time.format("%A").to_string(),
        _ if day.year() == today.year() => time.format("%a, %b %-d").to_string(),
        _ => time.format("%b %-d, %Y").to_string(),
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
        return "Off".into();
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
