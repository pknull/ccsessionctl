use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::session::Session;

/// Delete a session file and its associated directory (if any)
pub fn delete_session(session: &Session) -> Result<()> {
    // Delete the JSONL file
    fs::remove_file(&session.path)
        .with_context(|| format!("Failed to delete {:?}", session.path))?;

    // Delete associated directory if it exists
    let dir_path = session.path.with_extension("");
    if dir_path.is_dir() {
        fs::remove_dir_all(&dir_path)
            .with_context(|| format!("Failed to delete directory {:?}", dir_path))?;
    }

    Ok(())
}

/// Delete multiple sessions
pub fn delete_sessions(sessions: &[&Session]) -> Result<usize> {
    let mut deleted = 0;
    for session in sessions {
        if delete_session(session).is_ok() {
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// Check if a path can be deleted (exists and is writable)
pub fn can_delete(path: &Path) -> bool {
    path.exists()
        && fs::metadata(path)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
}
