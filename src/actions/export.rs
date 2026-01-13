use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::session::{load_session_messages, MessageRole, Session};

/// Export a session to Markdown format
pub fn export_session_markdown(session: &Session, output_dir: &Path) -> Result<PathBuf> {
    let messages = load_session_messages(&session.path)?;

    let output_name = format!(
        "{}_{}.md",
        session.project,
        session.id
    );
    let output_path = output_dir.join(&output_name);

    let mut file = File::create(&output_path)
        .with_context(|| format!("Failed to create {:?}", output_path))?;

    // Write header
    writeln!(file, "# Session: {}", session.id)?;
    writeln!(file, "")?;
    writeln!(file, "**Project:** {}", session.project)?;
    writeln!(file, "**Date:** {}", session.modified.format("%Y-%m-%d %H:%M:%S UTC"))?;
    if let Some(ref summary) = session.summary {
        writeln!(file, "**Summary:** {}", summary)?;
    }
    writeln!(file, "")?;
    writeln!(file, "---")?;
    writeln!(file, "")?;

    // Write messages
    for msg in messages {
        let role_label = match msg.role {
            MessageRole::User => "**User**",
            MessageRole::Assistant => "**Assistant**",
            MessageRole::System => "**System**",
        };

        writeln!(
            file,
            "### {} ({})",
            role_label,
            msg.timestamp.format("%H:%M:%S")
        )?;
        writeln!(file, "")?;
        writeln!(file, "{}", msg.content)?;
        writeln!(file, "")?;
    }

    Ok(output_path)
}

/// Export multiple sessions to Markdown files
pub fn export_sessions_markdown(sessions: &[&Session], output_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for session in sessions {
        let path = export_session_markdown(session, output_dir)?;
        paths.push(path);
    }

    Ok(paths)
}

/// Get default export directory (~/claude-sessions-export/)
pub fn get_default_export_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let export_dir = home.join("claude-sessions-export");

    if !export_dir.exists() {
        std::fs::create_dir_all(&export_dir)?;
    }

    Ok(export_dir)
}

/// Export session to a string (for preview)
pub fn export_session_to_string(session: &Session) -> Result<String> {
    let messages = load_session_messages(&session.path)?;
    let mut output = String::new();

    output.push_str(&format!("# Session: {}\n\n", session.id));
    output.push_str(&format!("**Project:** {}\n", session.project));
    output.push_str(&format!(
        "**Date:** {}\n",
        session.modified.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    if let Some(ref summary) = session.summary {
        output.push_str(&format!("**Summary:** {}\n", summary));
    }
    output.push_str("\n---\n\n");

    for msg in messages {
        let role_label = match msg.role {
            MessageRole::User => "**User**",
            MessageRole::Assistant => "**Assistant**",
            MessageRole::System => "**System**",
        };

        output.push_str(&format!(
            "### {} ({})\n\n",
            role_label,
            msg.timestamp.format("%H:%M:%S")
        ));
        output.push_str(&msg.content);
        output.push_str("\n\n");
    }

    Ok(output)
}
