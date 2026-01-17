---
version: "1.4"
lastUpdated: "2026-01-17 UTC"
lifecycle: "active"
stakeholder: "all"
changeTrigger: "Session save - audit review implementation"
validatedBy: "user"
dependencies: ["communicationStyle.md"]
---

# activeContext

## Current Project Status

**Primary Focus**: TUI session manager for Claude Code sessions

**Active Work**:
- Code quality improvements from audit review
- Cross-platform compatibility

**Recent Activities** (last 7 days):
- **2026-01-17**: Comprehensive audit review fixes
  - **Critical fixes**: Replaced `unwrap()` panics in archive.rs with proper error handling
  - **Path decoding**: Implemented intelligent decode that preserves dashes in project names after known parent dirs (Code, Projects, etc.)
  - **UTF-8 safety**: Created `src/utils.rs` with `truncate_string()` using `.chars()` instead of byte slicing
  - **Code consolidation**: Unified duplicate `format_tokens()` from main.rs and app.rs into utils module
  - **Dynamic terminal sizing**: Now calls `set_visible_height()` with actual terminal dimensions
  - **Cross-platform clipboard**: Added macOS (pbcopy) and Windows (clip) support, properly waits for completion
  - **Dead code removal**: Removed unused `can_delete()`, `ArchiveSelected`/`ExportSelected` variants
  - **Session struct**: Added `project_path` field with decoded path, improved display name to show 2 segments
  - All 35 tests passing

- **2026-01-15**: Fixed xclip clipboard blocking
  - Root cause: `child.wait()` blocked on xclip which waits for paste event
  - Fix: Don't wait for clipboard tool to exit, just write and move on

- **2026-01-14**: Minimal UI chrome and TUI improvements
  - Reduced chrome from ~7 lines to 2 lines
  - Fixed preview fallback chain
  - Enhanced yank to include `cd` to project directory

## Critical Reference Information

**Session Structure**:
- Sessions stored in `~/.claude/projects/{encoded-path}/{session-id}.jsonl`
- Path encoding: `/home/user/project` → `-home-user-project`
- JSONL contains: user/assistant/system records, summaries, custom titles

**Key Files**:
- `src/session/parser.rs` - Metadata loading, preview generation
- `src/session/types.rs` - Session structs, system content detection, path decoding
- `src/ui/app.rs` - TUI event handling, rendering
- `src/utils.rs` - Shared utilities (format_tokens, truncate_string)

## Next Steps

**Immediate**:
- [ ] Consider `--compact` flag if user wants toggle
- [ ] Consider `--days` flag for delete-older functionality

**Deferred**:
- Preserve sort/filter state across refresh (user declined)
- Integration tests for file system operations
