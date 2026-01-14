use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::types::{
    AssistantRecord, CustomTitleRecord, DisplayMessage, MessageRole, Session, SessionRecord,
    SummaryRecord, UserRecord,
};

/// Load metadata from a session file (full scan for search indexing)
pub fn load_session_metadata(session: &mut Session) -> Result<()> {
    let file = File::open(&session.path)
        .with_context(|| format!("Failed to open {:?}", session.path))?;
    let reader = BufReader::new(file);

    let mut first_timestamp = None;
    let mut first_user_message = None;
    let mut summary = None;
    let mut custom_title = None;
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
            SessionRecord::CustomTitle(CustomTitleRecord { custom_title: t }) => {
                custom_title = Some(t);
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
    session.custom_title = custom_title;
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

/// Get a preview of the session (custom title, first message, or summary)
pub fn get_session_preview(session: &Session) -> String {
    // Custom title takes priority (set via /rename command)
    if let Some(ref title) = session.custom_title {
        return truncate_message(title, 50);
    }
    // First message is more predictable than summary (summaries can be stale)
    if let Some(ref msg) = session.first_message {
        return truncate_message(msg, 50);
    }
    if let Some(ref summary) = session.summary {
        return truncate_message(summary, 50);
    }

    // Fallback: show message count if available
    if let Some(count) = session.message_count {
        if count > 0 {
            return format!("[{} message{}]", count, if count == 1 { "" } else { "s" });
        }
    }

    // Last resort: show truncated session ID
    let short_id = if session.id.len() > 12 {
        format!("{}...", &session.id[..12])
    } else {
        session.id.clone()
    };
    format!("[{}]", short_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_test_session() -> Session {
        Session::new(
            "abc123def456".to_string(),
            "test-project".to_string(),
            "-home-user-test-project".to_string(),
            PathBuf::from("/tmp/test.jsonl"),
            1024,
            Utc::now(),
        )
    }

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

    #[test]
    fn test_preview_custom_title_priority() {
        let mut session = make_test_session();
        session.custom_title = Some("My Custom Title".to_string());
        session.summary = Some("Summary text".to_string());
        session.first_message = Some("First message".to_string());
        assert_eq!(get_session_preview(&session), "My Custom Title");
    }

    #[test]
    fn test_preview_first_message_priority() {
        // First message now takes priority over summary
        let mut session = make_test_session();
        session.summary = Some("Summary text".to_string());
        session.first_message = Some("First message".to_string());
        assert_eq!(get_session_preview(&session), "First message");
    }

    #[test]
    fn test_preview_summary_fallback() {
        // Summary is used when no first_message
        let mut session = make_test_session();
        session.summary = Some("Summary text".to_string());
        assert_eq!(get_session_preview(&session), "Summary text");
    }

    #[test]
    fn test_preview_message_count_fallback() {
        let mut session = make_test_session();
        session.message_count = Some(5);
        assert_eq!(get_session_preview(&session), "[5 messages]");
    }

    #[test]
    fn test_preview_message_count_singular() {
        let mut session = make_test_session();
        session.message_count = Some(1);
        assert_eq!(get_session_preview(&session), "[1 message]");
    }

    #[test]
    fn test_preview_session_id_fallback() {
        let session = make_test_session();
        assert_eq!(get_session_preview(&session), "[abc123def456]");
    }

    #[test]
    fn test_preview_long_session_id_truncated() {
        let mut session = make_test_session();
        session.id = "abcdefghijklmnopqrstuvwxyz".to_string();
        assert_eq!(get_session_preview(&session), "[abcdefghijkl...]");
    }
}
