---
version: "1.3"
lastUpdated: "2026-01-15 UTC"
lifecycle: "active"
stakeholder: "all"
changeTrigger: "Session save - xclip blocking fix"
validatedBy: "user"
dependencies: ["communicationStyle.md"]
---

# activeContext

## Current Project Status

**Primary Focus**: TUI session manager for Claude Code sessions

**Active Work**:
- UI polish for terminal multiplexer usage
- Maximizing content area

**Recent Activities** (last 7 days):
- **2026-01-15**: Fixed xclip clipboard blocking
  - Root cause: `child.wait()` blocked on xclip which waits for paste event
  - Fix: Don't wait for clipboard tool to exit, just write and move on
  - Process diagnosed via `/proc/{pid}/wchan` showing `do_wait` state

- **2026-01-14 (session 2)**: Minimal UI chrome
  - Removed title line
  - Removed table borders
  - Condensed footer to single contextual line (status/selection/"?:help")
  - Reduced chrome from ~7 lines to 2 lines

- **2026-01-14 (session 1)**: Major TUI improvements
  - Fixed preview fallback chain: custom_title → first_message → summary → message count → session ID
  - Smarter system content detection (specific tags, not all `<` prefixes)
  - Enhanced yank (`y`) to include `cd` to project directory
  - Added progress counter for refresh operation
  - Fixed `--prune-empty` CLI to check actual message count

## Critical Reference Information

**Session Structure**:
- Sessions stored in `~/.claude/projects/{encoded-path}/{session-id}.jsonl`
- Path encoding: `/home/user/project` → `-home-user-project`
- JSONL contains: user/assistant/system records, summaries, custom titles

**Key Files**:
- `src/session/parser.rs` - Metadata loading, preview generation
- `src/session/types.rs` - Session structs, system content detection
- `src/ui/app.rs` - TUI event handling, rendering

## Next Steps

**Immediate**:
- [x] Test minimal UI with real usage
- [x] Fix clipboard yank functionality
- [ ] Consider `--compact` flag if user wants toggle

**Deferred**:
- Preserve sort/filter state across refresh (user declined)
