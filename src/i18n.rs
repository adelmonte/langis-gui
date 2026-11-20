//! The app's language: Desktop's translations (`_locales`, AGPL as the
//! icons), chosen in Settings > General > Language. `tr` turns a string the
//! code shows in English into the chosen language's; `tf` formats one of
//! Desktop's ICU messages (arguments, plurals, selects) by its key.
//! `scripts/i18n.py` builds the tables in `assets/i18n` from Desktop's.

use std::collections::HashMap;
use std::sync::RwLock;

pub use crate::i18n_locales::LOCALES;

struct Table {
    locale: &'static str,
    plain: HashMap<&'static str, &'static str>,
    icu: HashMap<&'static str, &'static str>,
    /// Each language's name in this one.
    names: HashMap<&'static str, &'static str>,
}

static TABLE: RwLock<Option<Table>> = RwLock::new(None);

/// The locale for a setting: the one chosen, else the system's (`LANG`,
/// `LC_ALL`, `LC_MESSAGES`) when the app has it, else English.
pub fn resolve(setting: &str) -> &'static str {
    let wanted = if setting.is_empty() {
        std::env::var("LC_ALL").ok().filter(|v| !v.is_empty()).or_else(|| std::env::var("LC_MESSAGES").ok().filter(|v| !v.is_empty())).or_else(|| std::env::var("LANG").ok()).unwrap_or_default()
    } else {
        setting.to_owned()
    };
    let wanted = wanted.split(['.', '@']).next().unwrap_or("").replace('_', "-");
    let lower = wanted.to_lowercase();
    // An exact locale, then its language alone (de-AT is de), then a locale
    // of that language (pt is pt-BR).
    let language = lower.split('-').next().unwrap_or("");
    let found = LOCALES.iter().find(|(l, ..)| l.to_lowercase() == lower)
        .or_else(|| LOCALES.iter().find(|(l, ..)| l.to_lowercase() == language))
        .or_else(|| {
            // Chinese: Taiwan's and Hong Kong's are traditional.
            if language == "zh" {
                let traditional = lower.contains("tw") || lower.contains("hant");
                let code = if lower.contains("hk") { "zh-HK" } else if traditional { "zh-Hant" } else { "zh-CN" };
                return LOCALES.iter().find(|(l, ..)| *l == code);
            }
            LOCALES.iter().find(|(l, ..)| l.to_lowercase().split('-').next() == Some(language))
        });
    found.map(|(l, ..)| *l).unwrap_or("en")
}

/// Switches the app to `locale` (one of [`LOCALES`]).
pub fn set_locale(locale: &str) {
    let Some((code, _, _, json)) = LOCALES.iter().find(|(l, ..)| *l == locale) else { return };
    if TABLE.read().unwrap().as_ref().is_some_and(|t| t.locale == *code) {
        return;
    }
    let Ok(file) = serde_json::from_str::<serde_json::Value>(json) else { return };
    let leak = |s: &str| -> &'static str { Box::leak(s.to_owned().into_boxed_str()) };
    let map = |name: &str| -> HashMap<&'static str, &'static str> {
        file.get(name).and_then(|m| m.as_object()).map(|m| m.iter().filter_map(|(k, v)| Some((leak(k), leak(v.as_str()?)))).collect()).unwrap_or_default()
    };
    let (plain, icu, names) = (map("plain"), map("icu"), map("names"));
    *TABLE.write().unwrap() = Some(Table { locale: code, plain, icu, names });
}

/// `of`'s name in the language in use (Desktop's `getLocaleDisplayName`).
pub fn display_name(of: &str) -> String {
    let table = TABLE.read().unwrap();
    table.as_ref().and_then(|t| t.names.get(of).map(|n| n.to_string())).or_else(|| LOCALES.iter().find(|(l, ..)| *l == of).map(|(_, _, en, _)| en.to_string())).unwrap_or_else(|| of.to_owned())
}

/// Where the choice is kept: a file of the app's own, read before the
/// account is unlocked (the setup and unlock screens are translated too).
fn saved_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("langis-gui").join("language"))
}

/// The language chosen in Settings: a locale, or empty for the system's.
pub fn saved() -> String {
    saved_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_owned()).filter(|s| s.is_empty() || LOCALES.iter().any(|(l, ..)| l == s)).unwrap_or_default()
}

/// Keeps the language chosen (empty for the system's) and switches to it.
pub fn choose(setting: &str) {
    if let Some(path) = saved_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, setting);
    }
    set_locale(resolve(setting));
}

/// The locale in use.
pub fn locale() -> &'static str {
    TABLE.read().unwrap().as_ref().map(|t| t.locale).unwrap_or("en")
}

/// `english` in the app's language (itself when there's no translation).
pub fn tr(english: &'static str) -> &'static str {
    let table = TABLE.read().unwrap();
    let Some(table) = table.as_ref() else { return english };
    if table.locale == "en" {
        return english;
    }
    // Desktop writes ’ and “” where the code may have ' and ".
    if let Some(found) = table.plain.get(english) {
        return found;
    }
    english
}

/// Desktop's ICU message `key` in the app's language, its arguments
/// filled in: `{name}`, `{n, plural, one {…} other {…}}` (with `#`),
/// `{x, select, a {…} other {…}}`. `fallback` is the English, used when
/// the key's missing.
pub fn tf(key: &str, fallback: &str, args: &[(&str, Arg)]) -> String {
    let table = TABLE.read().unwrap();
    let message = table.as_ref().and_then(|t| t.icu.get(key).copied()).unwrap_or(fallback);
    let locale = table.as_ref().map(|t| t.locale).unwrap_or("en");
    format_icu(message, args, locale)
}

/// An argument to an ICU message.
#[derive(Clone, Debug)]
pub enum Arg {
    Text(String),
    Number(i64),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self {
        Arg::Text(s.to_owned())
    }
}
impl From<String> for Arg {
    fn from(s: String) -> Self {
        Arg::Text(s)
    }
}
impl From<i64> for Arg {
    fn from(n: i64) -> Self {
        Arg::Number(n)
    }
}
impl From<usize> for Arg {
    fn from(n: usize) -> Self {
        Arg::Number(n as i64)
    }
}
impl From<u32> for Arg {
    fn from(n: u32) -> Self {
        Arg::Number(n as i64)
    }
}
impl From<i32> for Arg {
    fn from(n: i32) -> Self {
        Arg::Number(n as i64)
    }
}

fn arg<'a>(args: &'a [(&str, Arg)], name: &str) -> Option<&'a Arg> {
    args.iter().find(|(n, _)| *n == name).map(|(_, a)| a)
}

/// CLDR's cardinal plural category for `n` in `locale` (the integer rules).
pub fn plural(locale: &str, n: i64) -> &'static str {
    let lang = locale.split('-').next().unwrap_or(locale);
    let n = n.unsigned_abs();
    let (i10, i100) = (n % 10, n % 100);
    match lang {
        // No plural forms.
        "ja" | "ko" | "zh" | "yue" | "vi" | "id" | "ms" | "th" | "my" | "km" => "other",
        "fr" | "pt" if lang == "fr" || locale == "pt-BR" => {
            if n <= 1 {
                "one"
            } else {
                "other"
            }
        }
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" => {
            if i10 == 1 && i100 != 11 {
                "one"
            } else if (2..=4).contains(&i10) && !(12..=14).contains(&i100) {
                "few"
            } else {
                "many"
            }
        }
        "pl" => {
            if n == 1 {
                "one"
            } else if (2..=4).contains(&i10) && !(12..=14).contains(&i100) {
                "few"
            } else {
                "many"
            }
        }
        "cs" | "sk" => match n {
            1 => "one",
            2..=4 => "few",
            _ => "other",
        },
        "lt" => {
            if i10 == 1 && !(11..=19).contains(&i100) {
                "one"
            } else if (2..=9).contains(&i10) && !(11..=19).contains(&i100) {
                "few"
            } else {
                "other"
            }
        }
        "lv" => {
            if i10 == 0 || (11..=19).contains(&i100) {
                "zero"
            } else if i10 == 1 && i100 != 11 {
                "one"
            } else {
                "other"
            }
        }
        "sl" => match i100 {
            1 => "one",
            2 => "two",
            3 | 4 => "few",
            _ => "other",
        },
        "ro" => {
            if n == 1 {
                "one"
            } else if n == 0 || (1..=19).contains(&i100) {
                "few"
            } else {
                "other"
            }
        }
        "ga" => match n {
            1 => "one",
            2 => "two",
            3..=6 => "few",
            7..=10 => "many",
            _ => "other",
        },
        "mk" => {
            if i10 == 1 && i100 != 11 {
                "one"
            } else {
                "other"
            }
        }
        "is" => {
            if i10 == 1 && i100 != 11 {
                "one"
            } else {
                "other"
            }
        }
        _ => {
            if n == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

/// Formats an ICU message: arguments, plural (with `=N` and `#`), select,
/// and `''` quotes. Tags like `<bold>` are dropped, keeping their text.
pub fn format_icu(message: &str, args: &[(&str, Arg)], locale: &str) -> String {
    let chars: Vec<char> = message.chars().collect();
    let mut out = String::new();
    format_into(&chars, args, locale, None, &mut out);
    out
}

fn format_into(chars: &[char], args: &[(&str, Arg)], locale: &str, hash: Option<i64>, out: &mut String) {
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' if chars.get(i + 1) == Some(&'\'') => {
                out.push('\'');
                i += 2;
            }
            '#' if hash.is_some() => {
                out.push_str(&hash.unwrap().to_string());
                i += 1;
            }
            '<' => {
                // A tag (`<bold>`, `</bold>`, `<emojify>`): kept as its text.
                match chars[i..].iter().position(|c| *c == '>') {
                    Some(end) if chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == '/') => i += end + 1,
                    _ => {
                        out.push('<');
                        i += 1;
                    }
                }
            }
            '{' => {
                let end = matching(chars, i);
                let inner: String = chars[i + 1..end].iter().collect();
                format_arg(&inner, args, locale, out);
                i = end + 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
}

/// The index of the `}` closing the `{` at `open`.
fn matching(chars: &[char], open: usize) -> usize {
    let mut depth = 0;
    for (k, c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    chars.len().saturating_sub(1)
}

fn format_arg(inner: &str, args: &[(&str, Arg)], locale: &str, out: &mut String) {
    let mut parts = inner.splitn(3, ',');
    let name = parts.next().unwrap_or("").trim();
    let kind = parts.next().map(str::trim);
    let rest = parts.next().unwrap_or("");
    let value = arg(args, name);
    match kind {
        None | Some("number") | Some("date") | Some("time") => match value {
            Some(Arg::Text(s)) => out.push_str(s),
            Some(Arg::Number(n)) => out.push_str(&n.to_string()),
            None => {}
        },
        Some("plural") | Some("selectordinal") => {
            let n = match value {
                Some(Arg::Number(n)) => *n,
                Some(Arg::Text(s)) => s.parse().unwrap_or(0),
                None => 0,
            };
            let cases = cases(rest);
            let exact = format!("={n}");
            let category = plural(locale, n);
            let chosen = cases.iter().find(|(k, _)| *k == exact).or_else(|| cases.iter().find(|(k, _)| *k == category)).or_else(|| cases.iter().find(|(k, _)| *k == "other"));
            if let Some((_, body)) = chosen {
                let body: Vec<char> = body.chars().collect();
                format_into(&body, args, locale, Some(n), out);
            }
        }
        Some("select") => {
            let wanted = match value {
                Some(Arg::Text(s)) => s.clone(),
                Some(Arg::Number(n)) => n.to_string(),
                None => String::new(),
            };
            let cases = cases(rest);
            if let Some((_, body)) = cases.iter().find(|(k, _)| *k == wanted).or_else(|| cases.iter().find(|(k, _)| *k == "other")) {
                let body: Vec<char> = body.chars().collect();
                format_into(&body, args, locale, None, out);
            }
        }
        Some(_) => {}
    }
}

/// `one {…} other {…}` as (selector, body) pairs.
fn cases(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && chars[i] != '{' && !chars[i].is_whitespace() {
            i += 1;
        }
        let selector: String = chars[start..i].iter().collect();
        while i < chars.len() && chars[i] != '{' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let end = matching(&chars, i);
        out.push((selector, chars[i + 1..end].iter().collect()));
        i = end + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icu_messages_format() {
        let m = "{count, plural, one {# member} other {# members}}";
        assert_eq!(format_icu(m, &[("count", 1.into())], "en"), "1 member");
        assert_eq!(format_icu(m, &[("count", 5.into())], "en"), "5 members");
        let ru = "{count, plural, one {# участник} few {# участника} many {# участников} other {# участника}}";
        assert_eq!(format_icu(ru, &[("count", 3.into())], "ru"), "3 участника");
        assert_eq!(format_icu(ru, &[("count", 11.into())], "ru"), "11 участников");
        assert_eq!(format_icu("Hi {name}, it''s <bold>here</bold>", &[("name", "Ana".into())], "en"), "Hi Ana, it's here");
        assert_eq!(format_icu("{x, select, a {A} other {O}}", &[("x", "b".into())], "en"), "O");
        assert_eq!(format_icu("{n, plural, =0 {none} other {#}}", &[("n", 0.into())], "en"), "none");
    }

    #[test]
    fn every_locale_parses_and_resolves() {
        for (code, ..) in LOCALES {
            set_locale(code);
            assert_eq!(locale(), *code);
        }
        assert_eq!(resolve("de_AT.UTF-8"), "de");
        assert_eq!(resolve("pt_BR.UTF-8"), "pt-BR");
        assert_eq!(resolve("zh_TW.UTF-8"), "zh-Hant");
        assert_eq!(resolve("xx"), "en");
        set_locale("en");
    }
}
