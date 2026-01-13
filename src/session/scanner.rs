use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;

use super::types::{Project, Session};

/// Discover all Claude Code sessions from ~/.claude/projects/
pub fn scan_sessions() -> Result<Vec<Session>> {
    let projects_dir = get_projects_dir()?;
    let mut sessions = Vec::new();

    if !projects_dir.exists() {
        return Ok(sessions);
    }

    for entry in fs::read_dir(&projects_dir)
        .with_context(|| format!("Failed to read {:?}", projects_dir))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip hidden directories
        if dir_name.starts_with('.') {
            continue;
        }

        let project = Project::from_dir_name(&dir_name, path.clone());
        let project_sessions = scan_project_sessions(&project)?;
        sessions.extend(project_sessions);
    }

    // Sort by modification time (newest first)
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(sessions)
}

/// Get the Claude Code projects directory
fn get_projects_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".claude").join("projects"))
}

/// Scan sessions within a project directory
fn scan_project_sessions(project: &Project) -> Result<Vec<Session>> {
    let mut sessions = Vec::new();

    for entry in fs::read_dir(&project.path)? {
        let entry = entry?;
        let path = entry.path();

        // Only look at .jsonl files
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let session_id = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Get file metadata
        let metadata = fs::metadata(&path)?;
        let size_bytes = metadata.len();
        let modified: DateTime<Utc> = metadata
            .modified()
            .map(|t| t.into())
            .unwrap_or_else(|_| Utc::now());

        let session = Session::new(
            session_id,
            project.name.clone(),
            project.raw_name.clone(),
            path,
            size_bytes,
            modified,
        );

        sessions.push(session);
    }

    Ok(sessions)
}

/// Get all unique project names from sessions
pub fn get_project_names(sessions: &[Session]) -> Vec<String> {
    let mut names: Vec<String> = sessions
        .iter()
        .map(|s| s.project.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_from_dir_name() {
        let project = Project::from_dir_name(
            "-home-pknull-Projects-threshold",
            PathBuf::from("/test"),
        );
        assert_eq!(project.name, "threshold");
        assert_eq!(project.raw_name, "-home-pknull-Projects-threshold");
    }

    #[test]
    fn test_project_from_simple_name() {
        let project = Project::from_dir_name("myproject", PathBuf::from("/test"));
        assert_eq!(project.name, "myproject");
    }
}
