# Roadmap

Anvaya ships incrementally. Each version is small, testable, and does not
break the previous protocol without bumping `PROTOCOL_VERSION`.

## Core (v0.1 → v0.6) — complete ✅

This is the **stable foundation**. Future integrations and adaptations are
left to the community via the extension points described in
[`docs/Architecture.md`](./docs/Architecture.md).

## v0.1 — foundation ✅

- Workspace + `shared` protocol crate
- Daemon: `write`, `mkdir`
- `PathPolicy` traversal protection
- `ConsoleApprover` + `AutoApprover`
- Config via environment

## v0.2 — read side ✅

- `read`, `list`, `delete`
- Binary-file rejection on read
- Recursive directory delete (cross-fs safe)

## v0.3 — edit / rename / move ✅

- `move` (rename with cross-device copy+delete fallback)
- `copy` (recursive)
- `edit` primitive (search-and-replace, built on read+write)

## v0.4 — approval UI ✅

- Async `Approver` trait (`async-trait`)
- `ExtensionApprover`: daemon blocks until the extension decides
- `PendingStore`: in-memory queue of un-approved requests
- Daemon endpoints: `GET /api/v1/pending`, `POST /api/v1/approve/{id}`,
  `POST /api/v1/deny/{id}`
- Extension popup as the real approval surface (queue, approve/deny buttons)
- Background service worker polls `/pending` every 2 s, badge count, session
  storage cache
- Optional `autoApprove` toggle in the extension options page

## v0.5 — AI integrations ✅

- Vendor-agnostic content-script architecture:
  - `SiteAdapter` interface (`sites/types.ts`)
  - `ChatGptAdapter`, `ClaudeAdapter`, `GeminiAdapter`
  - `agent.ts` tool-call parser (```` ```anvaya ``` ```` fenced blocks)
- Content script injected on `chat.openai.com`, `chatgpt.com`, `claude.ai`,
  `gemini.google.com`
- `MutationObserver` scans settling assistant messages for Anvaya fences
- Tool calls are forwarded to the background → daemon; the result card is
  injected back into the conversation DOM
- No vendor lock-in: any Markdown-emitting chat works with a new adapter
  file (~40 lines)

## v0.6 — advanced features ✅

- **Multi-action transactions:** Atomic requests (`batch`) for multiple operations (e.g., mkdir + writes) to avoid partial failures.
- **Patch-based editing enhancements:** `edit` action supports regex, replace all, insert before/after, and append/prepend.
- **Search in files:** A `grep` action to search contents without reading every file.
- **Glob support:** A `glob_list` action to allow operations on `**/*.rs` or `src/**/*.py` for project-wide actions.
- **Directory tree:** A `tree` action that returns a nested directory structure to better understand projects.
- **Read partial files:** Supported `offset` and `length` in `read` to avoid sending 5000-line files.
- **File metadata:** A `stat` action that returns size, modified time, permissions, and hash.
- **Diff support:** Show diffs in the approval card (via `dry_run_diff`).
- **Workspace context:** A `project_info` action returning git repo status, language, package manager, framework, and build system.
- **Validation before writing:** Return structured errors via a custom Axum extractor for malformed JSON or invalid paths before attempting writes.

## Future / ideas (community contributions welcome)

These are intentionally **out of core**. Pick one and ship it as a plugin,
a separate crate, or a fork — the extension points in `docs/Architecture.md`
are the seams where they fit without modifying core.
- Per-agent permissions (scoping which roots/kinds each agent may use)
- File watches → push notifications to the assistant
- Undo journal for delete/overwrite
- Multi-root policies and per-root quotas
- Optional signed extension build for the Chrome Web Store / AMO
- Native messaging transport as an alternative to localhost HTTP
- **MCP server** wrapper (`anvaya-mcp`) so Claude Desktop and other
  MCP-compatible agents use Anvaya as native tools, bypassing the
  chat-injection pattern entirely (works model-agnostically, even on Opus)
- Wry-based native approval window as an alternative to the popup