// Content script entry.
//
// Runs on ChatGPT / Claude / Gemini pages. Selects a `SiteAdapter` based on
// the host, sets up a debounced `MutationObserver` on the conversation
// root, and scans every newly-settled assistant message for Anvaya tool
// fences. When one is found:
//   1. The fence is replaced with a "running…" card.
//   2. `executeAction` sends the request to the background, which relays it
//      to the daemon (the daemon's ExtensionApprover queues it for the popup).
//   3. When the daemon responds (either after approval or immediately under
//      auto-approve), the card is replaced with the result summary.

import { scanFences, executeAction, summarizeAction, describeResponse, type ToolCall } from './agent';
import type { SiteAdapter } from './sites/types';
import { matchesChatGpt, ChatGptAdapter } from './sites/chatgpt';
import { matchesClaude, ClaudeAdapter } from './sites/claude';
import { matchesGemini, GeminiAdapter } from './sites/gemini';

function pickAdapter(): SiteAdapter | null {
  if (matchesChatGpt()) return ChatGptAdapter;
  if (matchesClaude())  return ClaudeAdapter;
  if (matchesGemini())  return GeminiAdapter;
  return null;
}
// `SiteAdapter` import keeps the return type non-`any`.
export type { SiteAdapter };

function injectCss() {
  const id = 'anvaya-content-styles';
  if (document.getElementById(id)) return;
  const css = `
    .anv-card {
      font: 13px/1.45 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
      max-width: 760px;
      margin: 0.6em 0;
      padding: 0.6em 0.85em;
      border: 1px solid oklch(36% 0.02 270);
      border-radius: 0.6rem;
      background: oklch(22% 0.02 270);
      color: oklch(96% 0.01 270);
      overflow: auto;
      white-space: pre-wrap;
    }
    .anv-card .anv-title { font-weight: 600; opacity: 0.9; }
    .anv-card .anv-err { color: oklch(64% 0.22 25); }
    .anv-card .anv-ok  { color: oklch(74% 0.17 150); }
    .anv-card .anv-sub { display: block; opacity: 0.75; margin-top: 0.3em; }
    .anv-running { opacity: 0.6; }
    .anv-running::before { content: "running… "; animation: anv-spin 1s linear infinite; }
    @keyframes anv-spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }
  `;
  const style = document.createElement('style');
  style.id = id;
  style.textContent = css;
  document.head.appendChild(style);
}

function runCard(action: import('@/lib/protocol').Action): HTMLElement {
  const div = document.createElement('div');
  div.className = 'anv-card anv-running';
  div.innerHTML = `<span class="anv-title">Anvaya</span> · ${summarizeAction(action)}`;
  return div;
}

function resultCard(action: import('@/lib/protocol').Action, text: string, ok: boolean): HTMLElement {
  const div = document.createElement('div');
  div.className = 'anv-card';
  const cls = ok ? 'anv-ok' : 'anv-err';
  div.innerHTML = `<span class="anv-title">Anvaya</span> · ${summarizeAction(action)}\n<span class="${cls}">${text}</span>`;
  return div;
}

async function handleToolCall(call: ToolCall, adapter: SiteAdapter): Promise<void> {
  call.fenceEl.dataset.anvaya = 'done';
  // Replace the raw code block with a styled card so it doesn't look like code.
  const pre = call.fenceEl.closest('pre') ?? call.fenceEl;
  const placeholder = runCard(call.action!);
  pre.replaceWith(placeholder);
  try {
    const response = await executeAction(call.action!);
    const summary = describeResponse(response);
    const okFilled = response.status === 'ok';
    placeholder.replaceWith(resultCard(call.action!, summary, okFilled));
    
    // Inject the full JSON back into the chat input so the AI can read it.
    const fullJson = JSON.stringify(response, null, 2);
    const snippet = `Anvaya Result:\n\`\`\`json\n${fullJson}\n\`\`\``;
    adapter.setInputValue(snippet);
  } catch (e) {
    const errText = e instanceof Error ? e.message : String(e);
    placeholder.replaceWith(resultCard(call.action!, errText, false));
    adapter.setInputValue(`Anvaya Error:\n${errText}`);
  }
}

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let seenIds = new WeakSet<Element>();

function onSettle(adapter: SiteAdapter) {
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(async () => {
    debounceTimer = null;
    
    // Check if the extension is globally enabled before doing any work
    const { getSettings } = await import('@/lib/storage');
    const settings = await getSettings();
    if (!settings.extensionEnabled) return;

    const messages = adapter.collectMessages();
    console.log('[anvaya] settled, found', messages.length, 'messages');
    for (const m of messages) {
      if (seenIds.has(m.el)) continue;
      seenIds.add(m.el);
      const calls = scanFences(m.el);
      console.log('[anvaya] message', m.id, '→', calls.length, 'fences');
      for (const c of calls) {
        if (c.action) {
          console.log('[anvaya] executing', c.action.kind, c.body.slice(0, 80));
          void handleToolCall(c, adapter);
        } else if (c.label === 'anvaya' || c.label === 'anv') {
          console.warn('[anvaya] strict label but parse failed:', c.body.slice(0, 200));
        }
      }
    }
  }, 800);
}

function installObserver(adapter: SiteAdapter) {
  const root = adapter.conversationRoot();
  const target = root ?? document.body;
  const obs = new MutationObserver(() => onSettle(adapter));
  obs.observe(target, { childList: true, subtree: true, characterData: true });
  
  // Mark all existing messages as seen on load, so we don't re-execute historical tool calls
  const initialMessages = adapter.collectMessages();
  for (const m of initialMessages) {
    seenIds.add(m.el);
    // Mark their fences as done so they aren't parsed during later mutations
    m.el.querySelectorAll('pre > code, pre').forEach(code => {
      (code as HTMLElement).dataset.anvaya = 'done';
    });
  }
}

function bootstrap() {
  const adapter = pickAdapter();
  if (!adapter) {
    console.warn('[anvaya] no adapter matched host', location.hostname);
    return;
  }
  console.log('[anvaya] content script active on', location.hostname);
  injectCss();
  installObserver(adapter);
}

// Some chats hydrate after `document_idle`; defer until the root exists.
const adapter = pickAdapter();
if (adapter) {
  if (document.readyState === 'loading' || !adapter.conversationRoot()) {
    document.addEventListener('readystatechange', () => bootstrap(), { once: true });
    setTimeout(bootstrap, 500);
  } else {
    bootstrap();
  }
}