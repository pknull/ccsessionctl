use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::types::{
    AssistantRecord, DisplayMessage, MessageRole, Session, SessionRecord, SummaryRecord,
    UserRecord,
};

/// Load metadata from a session file (full scan for search indexing)
pub fn load_session_metadata(session: &mut Session) -> Result<()> {
    let file = File::open(&session.path)
        .with_context(|| format!("Failed to open {:?}", session.path))?;
    let reader = BufReader::new(file);

    let mut first_timestamp = None;
    let mut first_user_message = None;
    let mut summary = None;
    let mut message_count = 0;
    let mut all_content = Vec::new();
    let mut total_chars = 0usize;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        let record: SessionRecord = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        match record {
            SessionRecord::Summary(SummaryRecord { summary: s, .. }) => {
                all_content.push(s.clone());
                total_chars += s.len();
                summary = Some(s);
            }
            SessionRecord::User(UserRecord {
                timestamp,
                message,
                ..
            }) => {
                message_count += 1;
                if first_timestamp.is_none() {
                    first_timestamp = Some(timestamp);
                }
                let text = message.content.as_text();
                if !text.is_empty() {
                    all_content.push(text.clone());
                    total_chars += text.len();
                    if first_user_message.is_none() && !message.content.is_system_content() {
                        first_user_message = Some(truncate_message(&text, 100));
                    }
                }
            }
            SessionRecord::Assistant(AssistantRecord { message, .. }) => {
                message_count += 1;
                let text = message.as_text();
                if !text.is_empty() {
                    all_content.push(text.clone());
                    total_chars += text.len();
                }
            }
            SessionRecord::System(_) => {
                // System records don't contain searchable content
            }
            _ => {}
        }
    }

    session.created = first_timestamp;
    session.summary = summary;
    session.first_message = first_user_message;
    session.message_count = Some(message_count);
    session.search_content = Some(all_content.join(" ").to_lowercase());
    // Rough token estimate: ~4 chars per token
    session.token_count = Some(total_chars / 4);

    Ok(())
}

/// Load all messages from a session file for preview
pub fn load_session_messages(path: &Path) -> Result<Vec<DisplayMessage>> {
    let file = File::open(path).with_context(|| format!("Failed to open {:?}", path))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        let record: SessionRecord = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        match record {
            SessionRecord::User(UserRecord {
                timestamp,
                message,
                ..
            }) => {
                let content = message.content.as_text();
                // Skip system injected content
                if !message.content.is_system_content() && !content.is_empty() {
                    messages.push(DisplayMessage {
                        role: MessageRole::User,
                        timestamp,
                        content,
                    });
                }
            }
            SessionRecord::Assistant(AssistantRecord {
                timestamp,
                message,
                ..
            }) => {
                let content = message.as_text();
                if !content.is_empty() {
                    messages.push(DisplayMessage {
                        role: MessageRole::Assistant,
                        timestamp,
                        content,
                    });
                }
            }
            SessionRecord::System(ref sys) => {
                if let Some(ts) = sys.timestamp {
                    messages.push(DisplayMessage {
                        role: MessageRole::System,
                        timestamp: ts,
                        content: "[System]".to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(messages)
}

/// Truncate a message to a maximum length
fn truncate_message(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    // Remove newlines for preview
    let s: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();

    let char_count = s.chars().count();
    if char_count <= max_chars {
        s
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Get a preview of the session (summary or first message)
pub fn get_session_preview(session: &Session) -> String {
    if let Some(ref summary) = session.summary {
        return truncate_message(summary, 50);
    }
    if let Some(ref msg) = session.first_message {
        return truncate_message(msg, 50);
    }
    "(empty)".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_message() {
        assert_eq!(truncate_message("short", 10), "short");
        assert_eq!(truncate_message("this is a long message", 10), "this is...");
        assert_eq!(truncate_message("  spaced  ", 20), "spaced");
    }

    #[test]
    fn test_truncate_with_newlines() {
        assert_eq!(truncate_message("line1\nline2", 20), "line1 line2");
    }
}
