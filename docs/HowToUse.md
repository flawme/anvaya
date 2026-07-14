# How to Use Anvaya

## Quick start (daemon)

Requirements: Rust 1.85+ (edition 2024).

```bash
git clone <this repo>
cd anvaya
cargo build

# Configure a workspace root (required) and run
export ANVAYA_WORKSPACE="$HOME/Desktop"
cargo run -p anvaya-daemon
```

The daemon listens on `127.0.0.1:7878` by default. For local development you
can enable automatic approval (skips the stdin prompt):

```bash
ANVAYA_AUTO_APPROVE=1 cargo run -p anvaya-daemon   # WARNING: no approval prompt
```

### Smoke test

```bash
curl -X POST http://127.0.0.1:7878/api/v1/mkdir \
  -H 'content-type: application/json' -d '{"path":"demo"}'

curl -X POST http://127.0.0.1:7878/api/v1/write \
  -H 'content-type: application/json' \
  -d '{"path":"demo/hello.txt","content":"Hello, Anvaya!\n"}'

curl -X POST http://127.0.0.1:7878/api/v1/read \
  -H 'content-type: application/json' -d '{"path":"demo/hello.txt"}'
```

## Quick start (extension)

Requirements: Node 18+.

```bash
cd extension
npm install
npm run build        # emits dist/ — loadable as an unpacked extension
```

### Load in Chrome / Edge / Brave

1. Open `chrome://extensions`
2. Enable **Developer mode** (top-right)
3. Click **Load unpacked** → select `extension/dist/`
4. Pin the Anvaya toolbar icon, open the popup, and test against the running daemon

### Load in Firefox 121+

Firefox supports `background.service_worker` only for **signed** add-ons.
Temporary installs loaded via `about:debugging` need `background.scripts`
instead. The single `manifest.json` includes **both** keys (`service_worker`
+ `scripts`); Firefox prefers `scripts` (event page), Chrome 121+ uses
`service_worker` and ignores `scripts`.

1. Open `about:debugging#/runtime/this-firefox`
2. Click **Load Temporary Add-on…** → select **`extension/dist/manifest.json`**
3. The Anvaya button appears in the toolbar; the popup works identically to Chrome

## Using Anvaya from ChatGPT / Claude / Gemini

The extension injects a content script into the supported chat pages. When an
assistant emits a fenced code block labeled `anvaya` whose body is a JSON
`Action`, the content script:

1. Replaces the code block with a "running…" card.
2. Sends the action to the background → daemon (queued for approval).

Tell the assistant something like:

> You have access to the local filesystem via Anvaya. To perform a filesystem
> action, emit a fenced code block with the language tag `anvaya` and a JSON
> body. Available actions: `write` `{path, content, location?}`,
> `mkdir` `{path}`, `read` `{path}`, `list` `{path}`, `delete` `{path}`,
> `move` `{src, dst}`, `copy` `{src, dst}`, `edit` `{path, search, replace}`.
> Example:
>
> ```anvaya
> {"kind":"write","path":"Desktop/notes.md","content":"# notes\nhello"}
> ```

When the assistant emits that block, you'll see a red badge on the Anvaya
icon; open the popup, switch to the **approvals** tab, and choose **allow**
or **deny**. Supported hosts: `chat.openai.com`, `chatgpt.com`, `claude.ai`,
`gemini.google.com`. Adding a new host is one adapter file (~40 lines) in
`extension/src/content/sites/`.

### Note on Claude Models (Opus & Sonnet)

While Claude Haiku is generally very cooperative, Claude Opus and Sonnet may initially refuse to output the Anvaya tool block due to their safety training (e.g., claiming they cannot execute code or access your local filesystem). 

If they refuse, you will need to tweak your prompts or push back slightly. A good workaround is to insist: *"You don't need to execute anything yourself. Just output the requested `anvaya` JSON block, and my local extension will handle the actual execution."* Usually, a little back-and-forth is all it takes to get them to comply.

## Configuration

All configuration is via environment variables (a config file will follow in
v0.2):

| Variable                  | Default         | Description                                   |
|---------------------------|-----------------|-----------------------------------------------|
| `ANVAYA_BIND`             | `127.0.0.1:7878`| `host:port` to bind                           |
| `ANVAYA_WORKSPACE`        | — (required)    | `;`-separated list of workspace roots          |
| `ANVAYA_AUTO_APPROVE`     | `0`             | `1`/`true` approves everything (dev only)      |
| `ANVAYA_BODY_LIMIT`       | `16777216`      | max request body in bytes                       |
| `ANVAYA_ALLOWED_ORIGINS`  | —               | `;`-separated CORS allow-list                   |
