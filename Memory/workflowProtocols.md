---
version: "1.1"
lastUpdated: "2026-01-15 UTC"
lifecycle: "active"
stakeholder: "technical"
changeTrigger: "Added Linux process debugging pattern"
validatedBy: "user"
dependencies: ["activeContext.md", "techEnvironment.md"]
---

# workflowProtocols

## Memory Location and Tool Scope

- Memory path: [Relative and absolute paths]
- Access rule: [Which tools for which directories]

## Technical Verification

- [Verification type]: [Command or process]

## Infrastructure Validation Protocol

**BEFORE recommending new capabilities, commands, or infrastructure**:

1. **Check existing infrastructure** against proposed enhancement
2. **Compare proposed vs existing**: What's genuinely new?
3. **Validate transferability**: Does this pattern work in our domain?

**Pitfall**: Recommending duplicative infrastructure without checking existing capabilities.

**Prevention**: Always ask "How does this compare to what we already have?"

## Documentation Update Triggers

**>=25% Change Threshold**:
- Major implementation changes
- New patterns discovered
- Significant direction shifts
- User explicit request

**Update Process**:
1. Full Memory re-read before updating
2. Edit relevant files with new patterns/context
3. Update version numbers and lastUpdated timestamps
4. Document changeTrigger reasoning

## Authority Verification Workflow

**Before Making Claims**:
1. Check if statement requires verification marker
2. Apply appropriate label: [Inference], [Speculation], [Unverified]
3. When correction needed: "Authority correction: Previous statement contained unverified claims"
4. When unverifiable: "Data insufficient" / "Knowledge boundaries reached"

## Project-Specific Protocols

[Add protocols specific to your project domain]

- **[Domain]**: [How to handle it]

## Validated Patterns

### Linux Process Freeze Debugging

**When to Use**: Application appears frozen/unresponsive
**Process**:
1. `pgrep -a <name>` - Find PID
2. `cat /proc/{pid}/wchan` - What kernel function is it waiting in
3. `pstree -p {pid}` - Show child process tree
4. `ls -la /proc/{pid}/fd/` - Check open file descriptors

**Why This Works**: `wchan` reveals if process is in `do_wait` (waiting on child), `poll_schedule` (waiting on I/O), etc. Child tree shows what subprocess might be blocking.

**Anti-Pattern**: Blindly killing processes without diagnosing—loses the learning opportunity.

### Clipboard Tools (xclip/xsel) Behavior

**When to Use**: Calling clipboard tools from applications
**Process**: Don't call `child.wait()` after writing to stdin—clipboard tools may wait for paste event before exiting.

**Why This Works**: `xclip` by default stays alive to serve the clipboard selection. Waiting blocks the caller indefinitely.

**Anti-Pattern**: `child.wait().map(|s| s.success())` after clipboard write.
