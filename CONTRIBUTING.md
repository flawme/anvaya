# Contributing to Anvaya

Thanks for considering a contribution. Anvaya is a security-sensitive project
("gives an AI a filesystem"), so the bar for changes that touch the filesystem
or permissions modules is correspondingly high.

## Getting started

```bash
git clone <repo>
cd anvaya
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Node is only needed for the extension:

```bash
cd extension
npm install
npm run build
```

## Project layout

See [`docs/Architecture.md`](./docs/Architecture.md). The short version:

- `shared/` — wire protocol. Changing it is a protocol bump.
- `daemon/src/filesystem/` — the only code that touches disk.
- `daemon/src/permissions/` — path policy + approval; never imports
  `filesystem`.
- `daemon/src/actions/` — the single chokepoint orchestrating resolve → approve
  → execute.

## Before you open a PR

1. **No shell, no process spawning.** Do not add `std::process::Command`,
   `exec`, or any dependency that can spawn a process. A reviewer should be
   able to `grep -R process src/` and find nothing relevant.
2. **Keep the filesystem module the sole disk writer.** New endpoints route
   through `actions::execute`. Don't read/write files from handlers.
3. **Path checks stay in `permissions`.** If you need to validate a path, use
   `PathPolicy`; never hand-roll normalization.
4. **Tests.** Add a unit test for any new resolution or filesystem behavior.
   Prefer tests that build a temp directory under `std::env::temp_dir()`.
5. **Clippy & rustfmt clean.** CI rejects warnings. Keep edition 2024 idioms.
6. **Docs.** Update `README.md`/`ROADMAP.md` and any relevant doc when the
   user-visible API changes.

## Commit style

- Small, focused commits.
- Imperative-mood subject, ≤ 72 chars: `feat(daemon): add /api/v1/copy`.
- Reference issues in the body when relevant.

## Security-sensitive changes

Anything touching `permissions/` or `filesystem/` requires a reviewer to
explicitly call out the change in the PR description. For protocol changes,
bump `PROTOCOL_VERSION` and document the delta in `docs/Architecture.md`. See
[`SECURITY.md`](./docs/SECURITY.md) for the threat model.

## Code of conduct

Be kind, be precise, assume good faith. Disagreements go to the issue
tracker, not private channels.