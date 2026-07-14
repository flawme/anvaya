# Anvaya

![Anvaya Demo](./demo.gif)




**A secure bridge between AI assistants and your local computer.**

Anvaya lets an AI assistant request filesystem actions through a browser
extension, but every action is executed locally by a Rust daemon only **after
explicit user approval**. It exposes a small, safe filesystem API — it can
**never** run shell commands, spawn processes, or execute arbitrary code.

> **Status:** **Core complete (v0.6).** This release is the stable foundation;
> future integrations and adaptations are left to the community. See
> [`ROADMAP.md`](./ROADMAP.md) and [`docs/Architecture.md`](./docs/Architecture.md)
> for how to build on top of Anvaya.

## Platforms

| Component                 | Linux | macOS | Windows | Notes                                  |
|---------------------------|:-----:|:-----:|:-------:|----------------------------------------|
| Rust daemon               |   ✅  |  ✅   |   ✅    | Pure `tokio::fs`/`std::fs`; no shell   |
| Extension popup/options   |   ✅  |  ✅   |   ✅    | Any Chrome/Firefox build               |
| Approval over extension   |   ✅  |  ✅   |   ✅    | Polls `127.0.0.1` over HTTP             |
| ChatGPT content script    |   ✅  |  ✅   |   ✅    | Site-side, OS-agnostic                  |
| Gemini content script     |   ✅  |  ✅   |   ✅    | Site-side, OS-agnostic                  |
| Claude content script     |   ✅  |  ✅   |   ✅    | Site-side, OS-agnostic                  |
| `~` path expansion        |   ✅  |  ✅   |   ✅    | `HOME` on Unix, `USERPROFILE` on Win   |

Path separators are normalized lexically; clients may use `/` on every OS.
The daemon compiles to a single static binary per target with no extra runtime
dependencies.

---



## Getting Started

Please see [`docs/HowToUse.md`](./docs/HowToUse.md) for full instructions on how to install and use the daemon and extension, as well as how to prompt AI assistants to use Anvaya.

For details on the HTTP API, see [`docs/Architecture.md`](./docs/Architecture.md).


## Security model

See [`docs/SECURITY.md`](./docs/SECURITY.md). The short version:

- **No shell, no process spawning, ever.** The daemon only calls `std::fs`.
- **Path traversal is blocked.** `..` is collapsed lexically and the result
  must stay under a configured root.
- **Explicit approval required.** Unless `ANVAYA_AUTO_APPROVE=1`, every
  request prompts the operator.
- **localhost only.** The server binds `127.0.0.1`.

## Roadmap

See [`ROADMAP.md`](./ROADMAP.md).

## Extension points (for community contributions)

Anvaya is intentionally a **core**, not a finished product. The interfaces
below are stable seams where new functionality plugs in without forking:

- **New filesystem actions.** Add a variant to `shared::Action` and
  `ActionResponse`, implement it in `daemon/src/filesystem/ops.rs`, dispatch
  it in `daemon/src/actions/mod.rs`, expose it at one new HTTP route in
  `daemon/src/api/mod.rs`. The approval gate, path policy, and wire envelope
  pick it up automatically — no other glue.

- **New approvers.** Implement the `Approver` async trait in
  `daemon/src/permissions/approver.rs` and pass it to `AppState::new` in
  `main.rs`. The default `ExtensionApprover` waits on a `oneshot` channel; a
  future `WryApprover` (native desktop window) or `SlackApprover` (chat
  approval) drops in the same way.

- **New AI chat sites.** Add one file in `extension/src/content/sites/`
  (~40 lines) implementing `SiteAdapter`. Add a `matchesFoo()` check and a
  `content_scripts` match in `manifest.json`. The shared `agent.ts` scanner
  does the parsing; the adapter only finds DOM containers.

- **New transports.** The daemon's HTTP API is the only surface that knows
  about Axum. Anything that can `POST` JSON to `127.0.0.1:7878` works — MCP
  servers, native messaging hosts, LSP-style assistants, CLI clients. There
  is no SDK to maintain; the `shared` crate is the contract.

- **New tool-call syntaxes.** The parser in `extension/src/content/agent.ts`
  only looks for fenced code blocks labeled `anvaya` (strict) or `json`/empty
  (candidate, parsed and accepted only if the body's `kind` matches a known
  action). New labels can be added by extending `STRICT_LABELS` /
  `CANDIDATE_LABELS`. XML-style tool calls would need a separate parser hook
  in `scanFences`.

- **Per-agent permissions.** Not yet in core. The wire `Request` carries an
  optional `reason`; a future `agent_id` field plus a per-agent policy table
  in the daemon's `permissions` module would scope which roots/kinds each
  agent may use, without changing any action or filesystem code.

See `docs/Architecture.md` for the full module map and dependency direction.
See `CONTRIBUTING.md` for the bar on security-sensitive changes.

## Support the Project ❤️

Anvaya is open-source and free to use. If you find it valuable and it saves you time, consider supporting its development! 

You can donate via Wise on my website:
👉 **[flawme.sbs/donate](https://flawme.sbs/donate)**

## License

Dual-licensed under MIT or Apache-2.0, at your option. See [`LICENSE`](./LICENSE).