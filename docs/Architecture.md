# Architecture

This document describes how Anvaya's **core** is structured, how a request
flows from the browser extension to the filesystem, and the stable seams where
community extensions plug in without forking the project.

## High level

Anvaya is a workspace of two Rust crates plus a TypeScript browser extension:

```
shared/   protocol types (serde) shared by the daemon and any Rust client
daemon/   the long-running HTTP server + filesystem engine + approval gate
extension/  cross-browser extension (Chrome + Firefox 121+)
            - popup (approval UI + manual actions)
            - background service worker (relay + pending polling)
            - content scripts for ChatGPT / Claude / Gemini
```

Only `localhost` HTTP connects the extension to the daemon. No IPC, no
sockets, no shared memory — one well-understood transport, one auditable
boundary.

## Daemon module map

```
daemon/src/
├── main.rs        entry: config, tracing, approval wiring, serve
├── api/           HTTP handlers + routing (+ /pending /approve /deny)
├── actions/       dispatch: resolve → approve → execute
├── permissions/   PathPolicy (traversal guard) + async Approver trait
│                  ├─ ConsoleApprover / AutoApprover / ExtensionApprover
│                  └─ PendingStore (oneshot channels, per-request)
├── filesystem/    pure std::fs/tokio::fs operations (no shell, ever)
├── models/        Config, WorkspaceRoot, AppState, DaemonError
└── server/        axum app assembly, CORS, graceful shutdown
```

Each module has a single responsibility and a clear dependency direction:

```
            ┌──────────┐                                                     
            │  main.rs │                                                     
            └────┬─────┘                                                     
                 ▼
            ┌──────────┐    owns    ┌──────────────┐
            │  server  │ ─────────▶ │   api        │
            └──────────┘            └──────┬───────┘
                                           │ calls
                                           ▼
                                    ┌──────────────┐
                                    │   actions    │
                                    └──────┬───────┘
                               uses        │  uses
                      ┌────────────────────┴────────┐
                      ▼                             ▼
               ┌──────────────┐              ┌──────────────┐
               │ permissions  │              │  filesystem  │
               └──────────────┘              └──────────────┘
    
```

`filesystem` depends on nothing inside the daemon except `models` (for error
types). It never imports `axum`; it never opens a network socket. This keeps
the security-critical code small and reviewable in isolation.

## Request lifecycle

For every request the daemon runs the same three stages, in order, in
[`actions::execute`](./daemon/src/actions/mod.rs):

1. **Resolve.** [`PathPolicy`](./daemon/src/permissions/policy.rs) turns the
   client path string into an absolute path and verifies it lives under one of
   the configured [`WorkspaceRoot`](./daemon/src/models/config.rs)s. Relative
   paths resolve against the first root; absolute paths must match a root.
   Lexical `..` is collapsed *before* the root check, so `../../etc/passwd` is
   rejected without touching disk.

2. **Approve.** The configured async [`Approver`](./daemon/src/permissions/approver.rs)
   decides `Allow` or `Deny`. Three implementations:
   - [`ConsoleApprover`] prompts on stdin/out.
   - [`AutoApprover`] is a development shortcut gated behind
     `ANVAYA_AUTO_APPROVE=1`.
   - [`ExtensionApprover`] is the default in production: the request is added
     to a [`PendingStore`] and the future suspends on a `oneshot` channel
     until the extension popup calls `POST /api/v1/approve/{id}` or
     `/deny/{id}`. The pending list is observable via `GET /api/v1/pending`,
     which the background service worker polls every 2 seconds.

3. **Execute.** [`FileSystem`](./daemon/src/filesystem/ops.rs) performs the
   operation with `tokio::fs` (and `spawn_blocking` for recursive
   delete/copy). Binary files are rejected on read; symlinks are reported as
   their own entry kind but not followed during traversal checks.

The handler in [`api`](./daemon/src/api/mod.rs) is intentionally thin: it
parses the body, builds a [`Request`](./shared/src/lib.rs), and calls
`actions::execute`. There is no per-handler logic that touches the
filesystem, so the audit surface for "who can write files" is one function.

## API shape

The wire protocol is defined in [`shared/src`](./shared/src). Each request
carries:

- `id` — UUID; echoed back in the response.
- `kind` — one of `write`, `mkdir`, `read`, `list`, `delete`, `move`, `copy`, `edit`.
- `action` — the typed payload (a single tagged enum, so the dispatcher can
  `match` exhaustively).
- `reason` — optional human-readable rationale supplied by the agent.

Responses are `ok` (with a typed `result`) or `error` (with a stable
`ErrorCode`). Error codes never reveal absolute paths across the boundary to
the caller; messages are the daemon's own `Display` strings.

## HTTP API

All endpoints accept `POST` with a JSON body and return a JSON
[`Response`](../shared/src/lib.rs). Paths are resolved against the configured
workspace root; relative paths are preferred.

| Endpoint                | Body                                  | Result                          |
|-------------------------|---------------------------------------|---------------------------------|
| `POST /api/v1/mkdir`    | `{"path":"..."}`                      | `{kind:"mkdir", path}`          |
| `POST /api/v1/write`    | `{"path":"...","content":"...","location":"overwrite\|append"}` | `{kind:"write", bytes}` |
| `POST /api/v1/read`     | `{"path":"..."}`                      | `{kind:"read", content, bytes}`  |
| `POST /api/v1/list`     | `{"path":"..."}`                      | `{kind:"list", entries:[...]}`  |
| `POST /api/v1/delete`   | `{"path":"..."}`                      | `{kind:"delete", removed}`       |
| `POST /api/v1/move`     | `{"src":"...","dst":"..."}`           | `{kind:"move", src, dst}`        |
| `POST /api/v1/copy`     | `{"src":"...","dst":"..."}`           | `{kind:"copy", src, dst}`        |
| `POST /api/v1/edit`     | `{"path":"...","search":"...","replace":"..."}` | `{kind:"edit", bytes, matches}` |
| `POST /api/v1`          | full `Request` envelope                | any of the above                |
| `GET  /api/v1/pending`  | —                                     | `{pending: [{id,kind,summary}]}` |
| `POST /api/v1/approve/{id}` | —                                 | `{approved: true}`             |
| `POST /api/v1/deny/{id}`    | —                                 | `{denied: true}`               |
| `POST /health`          | —                                     | `200 OK`                         |

Example error response (path traversal blocked):

```json
{
  "id": "cb79bbb8-33ef-4a7d-9e4e-5634a58a1ccd",
  "status": "error",
  "error": { "code": "path_traversal", "message": "..." }
}
```

## What can never happen

The [`filesystem`](./daemon/src/filesystem/mod.rs) module is the **only** code
permitted to mutate disk, and it is built exclusively on `std::fs`/`tokio::fs`.
There is:

- no `std::process::Command` anywhere in the dependency graph,
- no `exec`/`spawn` syscall path,
- no script interpreter load,
- no network client inside the daemon that could be redirected into a shell.

The `permissions` module never imports `filesystem`; it only reasons about
paths. This separation means a future change to the approval UX cannot
accidentally widen what the filesystem layer is able to do.

## Extension architecture

```
extension/src/
├── popup/          approval UI + manual action runner (React)
├── options/        settings page (base URL, timeout, autoApprove)
├── background/     service worker / event page
│                   - relays EXECUTE messages to the daemon
│                   - polls /api/v1/pending every 2 s
│                   - badge count, session-storage cache
├── content/        content scripts injected into AI chat pages
│   ├── agent.ts      ```anvaya fence parser + result summarizer
│   ├── index.ts      adapter selection + MutationObserver driver
│   └── sites/        per-host DOM adapters
│       ├── types.ts    SiteAdapter interface
│       ├── chatgpt.ts  chat.openai.com / chatgpt.com
│       ├── claude.ts   claude.ai
│       └── gemini.ts   gemini.google.com
└── lib/            protocol mirror, DaemonClient, storage, messages
```

The content script is the vendor-agnostic layer: the `agent.ts` parser only
cares about ```` ```anvaya ```` fenced blocks, and each `SiteAdapter`
implements `collectMessages()` + `injectResult()`. Adding a new chat host
is one ~40-line adapter file plus a `matches` check.

## Approval message flow

```
Content script ──EXECUTE──► Background worker ──POST /api/v1──► Daemon
                                                            │
                                            ExtensionApprover suspends
                                            pending request added to store
                                                            │
Background worker ◄──GET /api/v1/pending── Daemon (every 2 s)
        │
        └── updates badge + session storage
                │
Popup (user clicks "allow")
        │
        └─APPROVE message──► Background ──POST /approve/{id}──► PendingStore
                                                            │
                                            oneshot::Sender fires
                                                            │
                                            FileSystem executes
                                                            │
Content script ◄──response── Background ◄──Response── Daemon
        │
        └── injects result card into the chat DOM
```

Under `ANVAYA_AUTO_APPROVE=1` (daemon) or `autoApprove: true` (extension
options), the popup step is skipped: the daemon auto-approves immediately,
or the background worker auto-approves every entry it sees in `/pending`.

## Core boundary and extension points

Anvaya ships as a **core**: a closed set of filesystem primitives, one
approval gate, one HTTP transport, and a vendor-agnostic content-script
scanner. Everything beyond that is intentionally left to the community. The
interfaces below are the stable seams where new functionality plugs in
without forking:

### 1. New filesystem actions

Add one variant to `shared::Action` + `shared::ActionResponse`, implement the
operation in `daemon/src/filesystem/ops.rs`, dispatch it in
`daemon/src/actions/mod.rs`, and expose one new HTTP route in
`daemon/src/api/mod.rs`. Path policy, approval, wire envelope, extension
client, and content-script parser pick it up automatically once `ActionKind`
is extended — no other glue. Tools like `chmod`, `stat`, `touch`, or
`grep`-style searches fit here.

### 2. New approvers

Implement the async `Approver` trait in `daemon/src/permissions/approver.rs`
and pass the boxed impl to `AppState::new` in `main.rs`. The default
`ExtensionApprover` suspends each request on a `oneshot` channel until the
popup decides; alternative implementations (native desktop window via Wry,
Slack-style chat approval, YubiKey tap) drop in identically. The actions
layer never calls the filesystem before the approver returns `Allow`.

### 3. New AI chat sites

Add one file in `extension/src/content/sites/` (~40 lines) implementing
`SiteAdapter` (`collectMessages` + `injectResult`), add a `matchesFoo()`
check, and add the host to `content_scripts.matches` in `manifest.json`.
The shared `agent.ts` scanner does the parsing; the adapter only finds DOM
containers. Past ChatGPT/Claude/Gemini, candidates include HuggingChat,
Perplexity, Mistral Le Chat, local LLM front-ends.

### 4. New transports

The HTTP API at `127.0.0.1:7878` is the only surface that knows about Axum.
Anything that can `POST` JSON works: an MCP server that exposes Anvaya as
model-context tools, a native messaging host that bridges to a desktop app,
a CLI client for scripting, an LSP-style assistant. The `shared` crate is
the wire contract — there is no SDK to maintain.

### 5. New tool-call syntaxes

The parser in `extension/src/content/agent.ts` only inspects fenced code
blocks labeled `anvaya` (strict) or `json`/empty (candidate, accepted only
if the body's `kind`/`action` field matches a known action). New labels can
be added by extending `STRICT_LABELS` / `CANDIDATE_LABELS`. XML-style tool
calls would need a new parser hook in `scanFences`.

### 6. Per-agent permissions (not in core)

The wire `Request` carries an optional `reason` but no `agent_id`. A
community extension could add an `agent_id` field to the protocol and a
per-agent policy table in the daemon's `permissions` module, scoping which
roots and which action kinds each agent may use — without touching any
action, filesystem, or approval code.

### What core will not grow

To stay auditable, the core declines a few classes of contribution:

- **Shell execution / process spawning.** `std::process::Command` (or any
  equivalent) anywhere in the daemon violates the central security
  guarantee. Forks that add this are explicitly not Anvaya.
- **Network-facing transports.** The daemon binds `127.0.0.1`. A remote
  transport is an upstream problem best solved by a separate proxy with its
  own auth, not by widening the daemon's bind address.
- **Silent approval.** New approvers must surface a real decision step;
  `AutoApprover` is gated behind an env var and a startup warning by design.