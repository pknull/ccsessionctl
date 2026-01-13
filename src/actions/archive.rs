use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::Builder;

use crate::session::Session;

/// Archive a session to a tar.gz file
pub fn archive_session(session: &Session, output_dir: &Path) -> Result<PathBuf> {
    let archive_name = format!(
        "{}_{}.tar.gz",
        session.project,
        session.id
    );
    let archive_path = output_dir.join(&archive_name);

    let file = File::create(&archive_path)
        .with_context(|| format!("Failed to create archive {:?}", archive_path))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);

    // Add the JSONL file
    let file_name = session.path.file_name().unwrap().to_str().unwrap();
    archive
        .append_path_with_name(&session.path, file_name)
        .with_context(|| format!("Failed to add {:?} to archive", session.path))?;

    // Add associated directory if it exists
    let dir_path = session.path.with_extension("");
    if dir_path.is_dir() {
        let dir_name = dir_path.file_name().unwrap().to_str().unwrap();
        archive
            .append_dir_all(dir_name, &dir_path)
            .with_context(|| format!("Failed to add directory {:?} to archive", dir_path))?;
    }

    archive.finish()?;

    Ok(archive_path)
}

/// Archive multiple sessions to a single tar.gz file
pub fn archive_sessions(sessions: &[&Session], output_path: &Path) -> Result<()> {
    let file = File::create(output_path)
        .with_context(|| format!("Failed to create archive {:?}", output_path))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);

    for session in sessions {
        // Create a subdirectory for each project
        let prefix = format!("{}/{}", session.project, session.id);

        // Add the JSONL file
        let file_name = format!("{}.jsonl", prefix);
        archive
            .append_path_with_name(&session.path, &file_name)
            .with_context(|| format!("Failed to add {:?} to archive", session.path))?;

        // Add associated directory if it exists
        let dir_path = session.path.with_extension("");
        if dir_path.is_dir() {
            archive
                .append_dir_all(&prefix, &dir_path)
                .with_context(|| format!("Failed to add directory {:?} to archive", dir_path))?;
        }
    }

    archive.finish()?;

    Ok(())
}

/// Get default archive directory (~/claude-sessions-archive/)
pub fn get_default_archive_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let archive_dir = home.join("claude-sessions-archive");

    if !archive_dir.exists() {
        std::fs::create_dir_all(&archive_dir)?;
    }

    Ok(archive_dir)
}
