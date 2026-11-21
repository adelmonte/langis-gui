//! Desktop's unread reminder notification text
//! (`ts/util/unreadReminders.std.ts`, `getUnreadReminderNotificationContent`).

use crate::i18n::{Arg, tf};
use langis_store::ReminderData;

fn plural(n: u32, one: &str, other: &str) -> String {
    if n == 1 { one.to_owned() } else { format!("{n} {other}") }
}

fn messages(n: u32) -> String {
    plural(n, "1 unread message", "unread messages")
}

/// A mention or reply summary: `kind` is "mention" or "reply".
fn summary(kind: &str, count: u32, people: &[String]) -> Option<String> {
    let (key, count_arg) = if kind == "mention" { ("mentionSummary", "mentionCount") } else { ("replySummary", "replyCount") };
    let (one_word, other_word, tail) = if kind == "mention" { ("mention", "mentions", " of you by") } else { ("reply", "replies", " from") };
    let args = |n: usize| -> Vec<(&str, Arg)> {
        let mut a = vec![(count_arg, Arg::from(count))];
        for (i, person) in people.iter().take(n).enumerate() {
            a.push((["person1", "person2"][i], Arg::from(person.clone())));
        }
        a
    };
    let counted = |few: bool| if count == 1 { if few { format!("a {one_word}") } else { format!("1 {one_word}") } } else { format!("{count} {other_word}") };
    match people {
        [] => None,
        [a] => Some(tf(&format!("icu:UnreadReminders__notification__{key}--1"), &format!("{}{tail} {a}", counted(true)), &args(1))),
        [a, b] => Some(tf(&format!("icu:UnreadReminders__notification__{key}--2"), &format!("{}{tail} {a} and {b}", counted(false)), &args(2))),
        [a, ..] => Some(tf(&format!("icu:UnreadReminders__notification__{key}--many"), &format!("{}{tail} {a} and others", counted(false)), &args(1))),
    }
}

/// The notification's title and body. `full` is false for Desktop's
/// "countsOnly" (notifications showing no name or content); `name` turns a
/// service id into a short name.
pub fn content(r: &ReminderData, chat_title: &str, full: bool, name: impl Fn(&str) -> String) -> (String, String) {
    let n = r.unread;
    let counts_only = || {
        tf("icu:UnreadReminders__notification--counts--messages", &format!("You have {}.", messages(n)), &[("messageCount", Arg::from(n))])
    };
    if !full {
        return ("langis".into(), counts_only());
    }
    let title = if chat_title.is_empty() { "langis".to_owned() } else { chat_title.to_owned() };
    let names = |ids: &[String]| ids.iter().map(|id| name(id)).collect::<Vec<_>>();
    let mentions = if r.mentions > 0 { summary("mention", r.mentions, &names(&r.mentioners)) } else { None };
    let replies = if r.replies > 0 { summary("reply", r.replies, &names(&r.repliers)) } else { None };
    // Desktop prefers the mention summary to the reply summary.
    let body = if let Some(m) = mentions {
        tf(
            "icu:UnreadReminders__notification--messages-including-mentions",
            &format!("You have {}, including {m}.", messages(n)),
            &[("messageCount", Arg::from(n)), ("mentionSummary", Arg::from(m.clone()))],
        )
    } else if let Some(re) = replies {
        tf(
            "icu:UnreadReminders__notification--messages-including-replies",
            &format!("You have {}, including {re}.", messages(n)),
            &[("messageCount", Arg::from(n)), ("replySummary", Arg::from(re.clone()))],
        )
    } else {
        let senders = names(&r.senders);
        let arg = |i: usize| (["person1", "person2"][i], Arg::from(senders[i].clone()));
        match senders.as_slice() {
            [] => counts_only(),
            [a] => tf("icu:UnreadReminders__notification--messages--1", &format!("You have {} from {a}.", messages(n)), &[("messageCount", Arg::from(n)), arg(0)]),
            [a, b] => tf(
                "icu:UnreadReminders__notification--messages--2",
                &format!("You have {} from {a} and {b}.", messages(n)),
                &[("messageCount", Arg::from(n)), arg(0), arg(1)],
            ),
            [a, b, ..] => tf(
                "icu:UnreadReminders__notification--messages--many",
                &format!("You have {} from {a}, {b}, and others.", messages(n)),
                &[("messageCount", Arg::from(n)), arg(0), arg(1)],
            ),
        }
    };
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(unread: u32, senders: &[&str]) -> ReminderData {
        ReminderData { unread, senders: senders.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn reads_as_desktop() {
        let name = |id: &str| id.to_uppercase();
        assert_eq!(content(&data(1, &["a"]), "Book club", true, name).1, "You have 1 unread message from A.");
        assert_eq!(content(&data(4, &["a", "b"]), "", true, name), ("langis".into(), "You have 4 unread messages from A and B.".into()));
        assert_eq!(content(&data(9, &["a", "b", "c"]), "x", true, name).1, "You have 9 unread messages from A, B, and others.");
        assert_eq!(content(&data(9, &["a"]), "x", false, name), ("langis".into(), "You have 9 unread messages.".into()));
        let mentioned = ReminderData { mentions: 1, mentioners: vec!["b".into()], replies: 2, repliers: vec!["c".into()], ..data(5, &["a"]) };
        assert_eq!(content(&mentioned, "x", true, name).1, "You have 5 unread messages, including a mention of you by B.");
        let replied = ReminderData { mentions: 0, ..mentioned.clone() };
        assert_eq!(content(&replied, "x", true, name).1, "You have 5 unread messages, including 2 replies from C.");
        let many = ReminderData { replies: 3, repliers: vec!["c".into(), "d".into(), "e".into()], ..replied };
        assert_eq!(content(&many, "x", true, name).1, "You have 5 unread messages, including 3 replies from C and others.");
    }
}
