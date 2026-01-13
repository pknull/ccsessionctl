mod actions;
mod session;
mod ui;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, Write};

use actions::delete_session;
use session::{get_session_preview, load_session_metadata, scan_sessions};
use ui::{App, UiState};

/// Handle broken pipe errors gracefully (e.g., when piping to head)
fn writeln_safe(s: &str) -> bool {
    if writeln!(io::stdout(), "{}", s).is_err() {
        return false;
    }
    true
}

/// Truncate project name for display
fn truncate_project(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

/// Format token count with K/M suffix
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum SortField {
    #[default]
    Date,
    Size,
    Project,
    Name,
}

#[derive(Parser)]
#[command(
    name = "ccsessionctl",
    version,
    about = "TUI for managing Claude Code CLI sessions"
)]
struct Cli {
    /// List sessions without TUI (non-interactive)
    #[arg(long)]
    list: bool,

    /// Show session count only
    #[arg(long)]
    count: bool,

    /// Delete all sessions without a name/summary
    #[arg(long)]
    prune_empty: bool,

    /// Preview what would be deleted (use with --prune-empty)
    #[arg(long)]
    dry_run: bool,

    /// Sort by field (date, size, project, name)
    #[arg(long, short, value_enum, default_value_t = SortField::Date)]
    sort: SortField,

    /// Reverse sort order
    #[arg(long, short)]
    reverse: bool,

    /// Filter by project name (case-insensitive substring match)
    #[arg(long, short)]
    project: Option<String>,

    /// Show usage statistics by project
    #[arg(long)]
    stats: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Scan sessions
    let mut sessions = scan_sessions()?;

    // Filter by project if specified
    if let Some(ref proj_filter) = cli.project {
        let filter_lower = proj_filter.to_lowercase();
        sessions.retain(|s| s.project.to_lowercase().contains(&filter_lower));
    }

    // Sort sessions
    match cli.sort {
        SortField::Date => sessions.sort_by(|a, b| b.modified.cmp(&a.modified)),
        SortField::Size => sessions.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes)),
        SortField::Project => sessions.sort_by(|a, b| a.project.cmp(&b.project)),
        SortField::Name => {
            // Need to load metadata for name sorting
            for session in &mut sessions {
                let _ = load_session_metadata(session);
            }
            sessions.sort_by(|a, b| {
                let name_a = a.summary.as_deref().or(a.first_message.as_deref()).unwrap_or("");
                let name_b = b.summary.as_deref().or(b.first_message.as_deref()).unwrap_or("");
                name_a.cmp(name_b)
            });
        }
    }

    // Reverse if requested
    if cli.reverse {
        sessions.reverse();
    }

    if cli.count {
        println!("{}", sessions.len());
        return Ok(());
    }

    if cli.stats {
        // Load metadata for all sessions to get token counts
        for session in &mut sessions {
            let _ = load_session_metadata(session);
        }

        // Aggregate by project
        use std::collections::HashMap;
        let mut project_stats: HashMap<String, (usize, u64, usize)> = HashMap::new(); // (count, size, tokens)

        for session in &sessions {
            let entry = project_stats.entry(session.project.clone()).or_insert((0, 0, 0));
            entry.0 += 1;
            entry.1 += session.size_bytes;
            entry.2 += session.token_count.unwrap_or(0);
        }

        // Convert to vec and sort by size
        let mut stats: Vec<_> = project_stats.into_iter().collect();
        stats.sort_by(|a, b| b.1.1.cmp(&a.1.1)); // Sort by size descending

        // Print header
        println!("{:<20} {:>8} {:>12} {:>12}", "Project", "Sessions", "Size", "Tokens");
        println!("{}", "-".repeat(56));

        let mut total_sessions = 0;
        let mut total_size = 0u64;
        let mut total_tokens = 0usize;

        for (project, (count, size, tokens)) in &stats {
            println!(
                "{:<20} {:>8} {:>12} {:>12}",
                truncate_project(project, 20),
                count,
                humansize::format_size(*size, humansize::BINARY),
                format_tokens(*tokens)
            );
            total_sessions += count;
            total_size += size;
            total_tokens += tokens;
        }

        println!("{}", "-".repeat(56));
        println!(
            "{:<20} {:>8} {:>12} {:>12}",
            "TOTAL",
            total_sessions,
            humansize::format_size(total_size, humansize::BINARY),
            format_tokens(total_tokens)
        );

        return Ok(());
    }

    if cli.prune_empty {
        let mut empty_sessions = Vec::new();

        // Find all empty sessions
        for session in &mut sessions {
            let _ = load_session_metadata(session);
            let preview = get_session_preview(session);
            if preview == "(empty)" {
                empty_sessions.push(session.clone());
            }
        }

        if empty_sessions.is_empty() {
            println!("No empty sessions found.");
            return Ok(());
        }

        if cli.dry_run {
            println!("Would delete {} empty session(s):", empty_sessions.len());
            for session in &empty_sessions {
                println!(
                    "  {} / {} ({})",
                    session.project,
                    session.id,
                    humansize::format_size(session.size_bytes, humansize::BINARY)
                );
            }
            return Ok(());
        }

        // Actually delete
        println!("Deleting {} empty session(s)...", empty_sessions.len());
        let mut deleted = 0;
        let mut total_size = 0u64;
        for session in &empty_sessions {
            if delete_session(session).is_ok() {
                deleted += 1;
                total_size += session.size_bytes;
            }
        }
        println!(
            "Deleted {} session(s), freed {}",
            deleted,
            humansize::format_size(total_size, humansize::BINARY)
        );
        return Ok(());
    }

    if cli.list {
        for session in &mut sessions {
            // Load metadata to get summary/first message
            let _ = load_session_metadata(session);
            let preview = get_session_preview(session);
            let line = format!(
                "{}\t{}\t{}\t{}\t{}",
                session.project,
                session.id,
                session.modified.format("%Y-%m-%d %H:%M"),
                humansize::format_size(session.size_bytes, humansize::BINARY),
                preview
            );
            if !writeln_safe(&line) {
                break; // Stop on broken pipe
            }
        }
        return Ok(());
    }

    // Run TUI
    run_tui(sessions)
}

fn run_tui(sessions: Vec<session::Session>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let state = UiState::new(sessions);
    let mut app = App::new(state);

    // Run app
    let result = app.run(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}
