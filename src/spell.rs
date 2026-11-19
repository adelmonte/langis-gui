//! Spell check in the composer, as Desktop's (Chromium's) underlines
//! misspelled words: the system's Hunspell dictionary for the locale
//! (`hunspell-en_us` and the like), read once in the background, and our
//! own added words, kept in the `ui.spell_words` setting.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

pub const SETTING_SPELL_CHECK: &str = "ui.spell_check";
pub const SETTING_SPELL_WORDS: &str = "ui.spell_words";

static DICTIONARY: OnceLock<Option<spellbook::Dictionary>> = OnceLock::new();
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static OWN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// The last text checked and what was wrong in it, as the composer lays
/// its text out every frame.
static LAST: Mutex<Option<(String, Vec<Range<usize>>)>> = Mutex::new(None);

/// The dictionary files for the locale (`LANG`), then US English.
fn dictionary_files() -> Option<(String, String)> {
    let lang = std::env::var("LANG").ok().and_then(|l| l.split('.').next().map(str::to_owned)).filter(|l| !l.is_empty());
    let dirs = ["/usr/share/hunspell", "/usr/share/myspell/dicts", "/usr/share/myspell"];
    for name in lang.iter().map(String::as_str).chain(["en_US"]) {
        for dir in dirs {
            let (aff, dic) = (format!("{dir}/{name}.aff"), format!("{dir}/{name}.dic"));
            if let (Ok(aff), Ok(dic)) = (std::fs::read_to_string(&aff), std::fs::read_to_string(&dic)) {
                return Some((aff, dic));
            }
        }
    }
    None
}

/// Starts reading the dictionary, once; `ctx` repaints when it's ready.
pub fn start(ctx: &egui::Context, own_words: Vec<String>) {
    *OWN.lock().unwrap() = Some(own_words.into_iter().collect());
    *LAST.lock().unwrap() = None;
    if STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let dictionary = dictionary_files().and_then(|(aff, dic)| spellbook::Dictionary::new(&aff, &dic).ok());
        if dictionary.is_none() {
            tracing::info!("no Hunspell dictionary found; spell check is off");
        }
        let _ = DICTIONARY.set(dictionary);
        ctx.request_repaint();
    });
}

/// Words in `text` with where each is: letters and apostrophes, skipping
/// links, @mentions, and words with digits or underscores.
fn words(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    for (start, token) in text.split_whitespace().map(|t| (t.as_ptr() as usize - text.as_ptr() as usize, t)) {
        if token.contains("://") || token.starts_with('@') || token.contains('@') || token.starts_with("www.") {
            continue;
        }
        let mut word_start = None;
        for (i, c) in token.char_indices().chain([(token.len(), ' ')]) {
            let part = c.is_alphabetic() || ((c == '\'' || c == '’') && word_start.is_some());
            match (part, word_start) {
                (true, None) => word_start = Some(i),
                (false, Some(s)) => {
                    let word = token[s..i].trim_end_matches(['\'', '’']);
                    let next_bad = token[i..].chars().next().is_some_and(|n| n.is_ascii_digit() || n == '_');
                    let prev_bad = token[..s].chars().next_back().is_some_and(|p| p.is_ascii_digit() || p == '_');
                    if word.chars().count() > 1 && !next_bad && !prev_bad {
                        out.push((start + s..start + s + word.len(), word));
                    }
                    word_start = None;
                }
                _ => {}
            }
        }
    }
    out
}

fn known(dictionary: &spellbook::Dictionary, own: &HashSet<String>, word: &str) -> bool {
    own.contains(word) || own.contains(&word.to_lowercase()) || dictionary.check(word)
}

/// Where `text`'s misspelled words are (byte ranges); none until the
/// dictionary is read, or if there isn't one.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    let Some(Some(dictionary)) = DICTIONARY.get() else { return Vec::new() };
    let mut last = LAST.lock().unwrap();
    if let Some((checked, ranges)) = last.as_ref()
        && checked == text
    {
        return ranges.clone();
    }
    let own = OWN.lock().unwrap().clone().unwrap_or_default();
    let ranges: Vec<Range<usize>> = words(text).into_iter().filter(|(_, w)| !known(dictionary, &own, w)).map(|(r, _)| r).collect();
    *last = Some((text.to_owned(), ranges.clone()));
    ranges
}

/// Up to five suggestions for a word.
pub fn suggestions(word: &str) -> Vec<String> {
    let Some(Some(dictionary)) = DICTIONARY.get() else { return Vec::new() };
    let mut out = Vec::new();
    dictionary.suggest(word, &mut out);
    out.truncate(5);
    out
}

/// Adds a word to ours; returns the list to save.
pub fn add_word(word: &str) -> Vec<String> {
    let mut own = OWN.lock().unwrap();
    let set = own.get_or_insert_with(Default::default);
    set.insert(word.to_owned());
    *LAST.lock().unwrap() = None;
    let mut list: Vec<String> = set.iter().cloned().collect();
    list.sort();
    list
}

#[cfg(test)]
mod tests {
    #[test]
    fn words_skip_links_mentions_and_numbers() {
        let text = "Meet at the trailhed, see https://exampel.org @mayaa and room 4b or abc_def. Don't worry";
        let found: Vec<&str> = super::words(text).into_iter().map(|(_, w)| w).collect();
        assert_eq!(found, ["Meet", "at", "the", "trailhed", "see", "and", "room", "or", "Don't", "worry"]);
    }

    #[test]
    fn checks_with_a_dictionary_when_one_is_installed() {
        let Some((aff, dic)) = super::dictionary_files() else { return };
        let d = spellbook::Dictionary::new(&aff, &dic).unwrap();
        let own = std::collections::HashSet::from(["langis".to_owned()]);
        assert!(super::known(&d, &own, "hello"));
        assert!(super::known(&d, &own, "langis"));
        assert!(!super::known(&d, &own, "trailhed"));
    }
}
