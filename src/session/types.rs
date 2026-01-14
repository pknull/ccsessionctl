use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A Claude Code project (directory under ~/.claude/projects/)
#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub raw_name: String,
}

impl Project {
    /// Parse project name from directory name
    /// e.g., "-home-pknull-Projects-threshold" -> "threshold"
    pub fn from_dir_name(raw_name: &str, path: PathBuf) -> Self {
        let name = raw_name
            .rsplit('-')
            .next()
            .unwrap_or(raw_name)
            .to_string();

        Self {
            name,
            path,
            raw_name: raw_name.to_string(),
        }
    }
}

/// A session file with metadata
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub project: String,
    pub project_raw: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: DateTime<Utc>,
    pub created: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub first_message: Option<String>,
    pub message_count: Option<usize>,
    pub is_agent: bool,
    pub has_directory: bool,
    /// User-provided session name via /rename command
    pub custom_title: Option<String>,
    /// Full searchable content (all messages concatenated)
    pub search_content: Option<String>,
    /// Token count estimate
    pub token_count: Option<usize>,
}

impl Session {
    pub fn new(
        id: String,
        project: String,
        project_raw: String,
        path: PathBuf,
        size_bytes: u64,
        modified: DateTime<Utc>,
    ) -> Self {
        let is_agent = id.starts_with("agent-");
        let dir_path = path.with_extension("");
        let has_directory = dir_path.is_dir();

        Self {
            id,
            project,
            project_raw,
            path,
            size_bytes,
            modified,
            created: None,
            summary: None,
            first_message: None,
            message_count: None,
            is_agent,
            has_directory,
            custom_title: None,
            search_content: None,
            token_count: None,
        }
    }
}

/// JSONL record types from Claude Code sessions
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SessionRecord {
    Summary(SummaryRecord),
    CustomTitle(CustomTitleRecord),
    FileHistorySnapshot(FileHistorySnapshot),
    User(UserRecord),
    Assistant(AssistantRecord),
    System(SystemRecord),
    QueueOperation(QueueOperationRecord),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SummaryRecord {
    pub summary: String,
    #[serde(rename = "leafUuid")]
    pub leaf_uuid: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomTitleRecord {
    #[serde(rename = "customTitle")]
    pub custom_title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileHistorySnapshot {
    #[serde(rename = "messageId")]
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserRecord {
    pub uuid: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub message: Message,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(rename = "isMeta")]
    pub is_meta: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantRecord {
    pub uuid: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub message: AssistantMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemRecord {
    pub uuid: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueOperationRecord {
    #[serde(rename = "queueOperations")]
    pub queue_operations: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Structured(Vec<ContentBlock>),
}

impl MessageContent {
    /// Extract plain text from message content
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Structured(blocks) => {
                blocks
                    .iter()
                    .filter_map(|b| b.as_text())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    /// Check if content starts with system tags (not real user input)
    pub fn is_system_content(&self) -> bool {
        let text = self.as_text();
        if text.is_empty() {
            return true;
        }

        // Check for known system tag patterns injected by Claude Code
        const SYSTEM_TAGS: &[&str] = &[
            "<system-reminder>",
            "<system>",
            "<context>",
            "<env>",
            "<claude_background_info>",
            "<user_privacy>",
            "<critical_",
            "<injection_",
            "<meta_safety",
            "<social_engineering",
            "<mandatory_",
            "<copyright_",
            "<download_",
            "<harmful_",
            "<action_types>",
            "<claudeMd>",
        ];

        let text_lower = text.to_lowercase();
        SYSTEM_TAGS.iter().any(|tag| text_lower.starts_with(tag))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolResult { content: serde_json::Value },
    ToolUse { name: String, input: Option<serde_json::Value> },
    Thinking { thinking: String },
    #[serde(other)]
    Other,
}

impl ContentBlock {
    pub fn as_text(&self) -> Option<String> {
        match self {
            ContentBlock::Text { text } => Some(text.clone()),
            ContentBlock::Thinking { thinking } => Some(format!("💭 {}", thinking)),
            ContentBlock::ToolUse { name, input } => {
                let input_preview = input
                    .as_ref()
                    .and_then(|v| v.get("command").or(v.get("pattern")).or(v.get("file_path")))
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        let char_count = s.chars().count();
                        if char_count > 60 {
                            let truncated: String = s.chars().take(57).collect();
                            format!(" \"{}...\"", truncated)
                        } else {
                            format!(" \"{}\"", s)
                        }
                    })
                    .unwrap_or_default();
                Some(format!("🔧 {}{}", name, input_preview))
            }
            ContentBlock::ToolResult { content } => {
                let result_text = Self::format_tool_result(content);
                Some(format!("📋 {}", result_text))
            }
            ContentBlock::Other => None,
        }
    }

    fn format_tool_result(content: &serde_json::Value) -> String {
        // Handle array of content blocks (common format)
        if let Some(arr) = content.as_array() {
            let texts: Vec<&str> = arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect();
            if !texts.is_empty() {
                let combined = texts.join("\n");
                return Self::truncate_result(&combined, 200);
            }
        }

        // Handle direct string
        if let Some(s) = content.as_str() {
            return Self::truncate_result(s, 200);
        }

        // Fallback
        "(result)".to_string()
    }

    fn truncate_result(s: &str, max_chars: usize) -> String {
        let char_count = s.chars().count();
        if char_count <= max_chars {
            s.to_string()
        } else {
            let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
            format!("{}...", truncated)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
}

impl AssistantMessage {
    pub fn as_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| b.as_text())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Parsed message for display
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: MessageRole,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_system_content_empty() {
        let content = MessageContent::Text(String::new());
        assert!(content.is_system_content());
    }

    #[test]
    fn test_is_system_content_system_reminder() {
        let content = MessageContent::Text("<system-reminder>hook success</system-reminder>".to_string());
        assert!(content.is_system_content());
    }

    #[test]
    fn test_is_system_content_context_tag() {
        let content = MessageContent::Text("<context>some context here</context>".to_string());
        assert!(content.is_system_content());
    }

    #[test]
    fn test_is_system_content_regular_user_message() {
        let content = MessageContent::Text("Hello, can you help me with this code?".to_string());
        assert!(!content.is_system_content());
    }

    #[test]
    fn test_is_system_content_html_from_user() {
        // User pasting HTML should NOT be treated as system content
        let content = MessageContent::Text("<html><body>test</body></html>".to_string());
        assert!(!content.is_system_content());
    }

    #[test]
    fn test_is_system_content_xml_from_user() {
        // User pasting XML should NOT be treated as system content
        let content = MessageContent::Text("<config><setting>value</setting></config>".to_string());
        assert!(!content.is_system_content());
    }

    #[test]
    fn test_is_system_content_angle_bracket_text() {
        // Casual use of < should NOT be treated as system content
        let content = MessageContent::Text("< 5 means less than five".to_string());
        assert!(!content.is_system_content());
    }

    #[test]
    fn test_is_system_content_case_insensitive() {
        let content = MessageContent::Text("<SYSTEM-REMINDER>test</SYSTEM-REMINDER>".to_string());
        assert!(content.is_system_content());
    }
}
