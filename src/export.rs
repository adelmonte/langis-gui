//! Chat history export, as Desktop's plain-text export (`PlaintextExport`):
//! each chat a text file of its messages, oldest first, and its files copied
//! into a folder beside it. Read from langis's store, which a frontend may
//! read freely; nothing is sent anywhere.

use std::path::{Path, PathBuf};

use langis_store::{AppDb, Conversation, Message};

/// A file name safe on any disk, from a chat's title.
fn safe_name(title: &str) -> String {
    let cleaned: String = title.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    let trimmed = cleaned.trim().trim_matches('.').to_owned();
    if trimmed.is_empty() { "chat".into() } else { trimmed.chars().take(80).collect() }
}

/// Every message of a chat, oldest first.
async fn all_messages(db: &AppDb, chat: &Conversation) -> Vec<Message> {
    let mut pages: Vec<Vec<Message>> = Vec::new();
    let mut before = None;
    loop {
        let page = db.messages(&chat.id, before, 500).await.unwrap_or_default();
        if page.is_empty() {
            break;
        }
        before = page.first().map(|m| m.id);
        let short = page.len() < 500;
        pages.push(page);
        if short {
            break;
        }
    }
    pages.into_iter().rev().flatten().collect()
}

/// One message as a line (or a few): its time, who sent it, its text, and
/// its files, named as copied.
fn line(m: &Message, author: &str, files: &[String]) -> String {
    let when = chrono::DateTime::from_timestamp_millis(m.sent_at)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let mut out = format!("[{when}] {author}: ");
    if m.deleted {
        out.push_str("(deleted)");
    } else if let Some(notice) = &m.notice {
        out = format!("[{when}] {}", crate::notices::notice_text(notice, |sid| sid.to_owned()));
    } else {
        out.push_str(&m.body);
    }
    for f in files {
        out.push_str(&format!("\n    [file: {f}]"));
    }
    out
}

/// Writes every chat into `dir`; returns how many chats and messages.
pub async fn export_all(db: &AppDb, dir: &Path, names: &std::collections::HashMap<String, String>) -> std::io::Result<(usize, usize)> {
    let mut chats = db.conversations().await.unwrap_or_default();
    chats.extend(db.archived_conversations().await.unwrap_or_default());
    std::fs::create_dir_all(dir)?;
    let (mut chat_count, mut message_count) = (0, 0);
    let mut used: std::collections::HashSet<String> = Default::default();
    for chat in &chats {
        let messages = all_messages(db, chat).await;
        if messages.is_empty() {
            continue;
        }
        let mut base = safe_name(&chat.title);
        let mut k = 2;
        while !used.insert(base.clone()) {
            base = format!("{} ({k})", safe_name(&chat.title));
            k += 1;
        }
        let files_dir: PathBuf = dir.join(format!("{base} files"));
        let mut text = format!("{}\n\n", chat.title);
        for m in &messages {
            let author = match &m.author {
                None => "You".to_owned(),
                Some(sid) => names.get(sid).cloned().or_else(|| m.author_name.clone()).unwrap_or_else(|| "Unknown".into()),
            };
            let mut copied = Vec::new();
            for (i, a) in m.attachments.iter().enumerate() {
                let Some(path) = &a.local_path else {
                    copied.push(format!("{} (not downloaded)", a.file_name.clone().unwrap_or_else(|| a.content_type.clone())));
                    continue;
                };
                std::fs::create_dir_all(&files_dir)?;
                let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("bin");
                let name = a.file_name.clone().map(|n| safe_name(&n)).unwrap_or_else(|| format!("{}-{i}.{ext}", m.sent_at));
                let target = files_dir.join(format!("{}-{name}", m.sent_at));
                if std::fs::copy(path, &target).is_ok() {
                    copied.push(format!("{base} files/{}", target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                }
            }
            text.push_str(&line(m, &author, &copied));
            text.push('\n');
            message_count += 1;
        }
        std::fs::write(dir.join(format!("{base}.txt")), text)?;
        chat_count += 1;
    }
    Ok((chat_count, message_count))
}

#[cfg(test)]
mod tests {
    #[test]
    fn chat_titles_become_safe_file_names() {
        assert_eq!(super::safe_name("Saturday hikes 🥾"), "Saturday hikes _");
        assert_eq!(super::safe_name("a/b\\c:d"), "a_b_c_d");
        assert_eq!(super::safe_name("..."), "chat");
    }
}
