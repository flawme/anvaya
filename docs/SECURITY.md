# Security Policy

Anvaya is designed to let an AI assistant touch your filesystem **without**
giving it a shell. This document is the threat model and the guarantees.

## Why

AI assistants are useful, but giving them a shell is dangerous. Anvaya trades
a little latency for a lot of safety:

- The assistant can only ask for things Anvaya knows how to do.
- Every request is shown to you before anything touches disk.
- Every path is validated against an allow-list of workspace roots, so
  `../../../etc/passwd` can never escape.

## Supported versions

Security fixes target the latest `main` and the most recent tagged release.
Pre-1.0 releases may change the protocol without ceremony.

## Reporting a vulnerability

Please report security issues privately by opening a GitHub Security Advisory
("Report a vulnerability" on the repo's Security tab) rather than a public
issue. Include reproduction steps and, if possible, a minimal request payload.

## Threat model

| Asset                | Threat                                            | Mitigation                                              |
|----------------------|---------------------------------------------------|---------------------------------------------------------|
| Filesystem contents | Prompt-injected AI asks to exfiltrate `/etc`      | Path traversal rejected; paths confined to a root        |
| Arbitrary code exec | AI asks daemon to run `curl \| sh`                | No shell/process spawning anywhere in the daemon         |
| Drive-by requests   | Random web page POSTs to `127.0.0.1:7878`        | CORS allow-list; localhost-only bind                     |
| Silent mutation     | AI writes/deletes without user noticing           | Every request requires explicit approval                 |
| Path confusion      | `demo/../../etc/passwd` or absolute paths         | Lexical normalization + root containment check           |
| Large payloads      | OOM / disk fill via giant write                   | Body size limit (`ANVAYA_BODY_LIMIT`, default 16 MiB)     |
| Symlink escape      | Symlink inside root points outside                | (v0.1–v0.2) symlinks are listed, not followed on write; see "Open issues" |

## Hard guarantees

1. **No process spawning.** The `filesystem` module is the only disk-touching
   code and it is built exclusively on `std::fs`/`tokio::fs`. There is no
   `std::process::Command` import anywhere in the daemon source tree, and
   adding one should fail review.
2. **No code execution.** Anvaya cannot interpret, compile, or `eval`
   anything. It has a fixed set of operations; everything else is out of scope.
3. **localhost only.** The server binds `127.0.0.1`; cross-origin callers must
   be on the configured origin allow-list.
4. **Approval is mandatory** unless `ANVAYA_AUTO_APPROVE=1` (which logs a
   warning on startup and is intended only for local development).

## What Anvaya is not

- It is not a sandbox escape tool. If you don't trust the daemon process, run
  it under a separate OS user with a restricted filesystem view.
- It is not a remote server. Exposing it beyond `127.0.0.1` is unsupported
  and unsafe.
- It is not a substitute for OS permissions. The daemon runs with the privs
  of its user; it can do anything that user can do within a workspace root.

## Open issues / known limitations (pre-1.0)

- Symlink handling is conservative: `list` reports symlinks, but `write`/`edit`
  follow symlinks by default and could in principle escape a root if a
  symlink inside the root points outward. A future hardening pass will
  resolve the target and reject writes that would escape.
- The content-script integration trusts that the assistant's emitted ```` ```anvaya ````
  block contains a well-formed `Action`. The parser rejects malformed JSON
  and unknown `kind` values, but cannot prevent a malicious agent from
  *lying* in the action body. The approval popup is the human's chance to
  inspect the actual paths before the daemon executes.
- `autoApprove` (either `ANVAYA_AUTO_APPROVE=1` on the daemon or
  `autoApprove: true` in the extension options) removes the human gate.
  Treat it as "any local process or any browser tab that can reach
  `127.0.0.1:7878` can ask for any file in the workspace root".