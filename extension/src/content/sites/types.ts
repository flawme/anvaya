// Site adapter: a per-host object that knows how to find assistant messages
// in the chat UI and how to inject result snippets back into the conversation.
//
// The content script entry selects an adapter based on `location.hostname` and
// drives it with the shared tool-call scanner in `agent.ts`. Each adapter
// stays small — just enough DOM knowledge to integrate with one vendor's UI.

/// A node that holds an assistant message's text content.
export interface AssistantMessage {
  /// Stable identifier for deduplication (e.g. element id or indexed path-in-tree).
  id: string;
  /// Element whose `innerText` contains the full assistant message.
  el: HTMLElement;
}

/// Inspect the page DOM for assistant messages. Called after each mutation
/// debounced settle.
export interface SiteAdapter {
  /// Human-friendly name.
  readonly name: string;
  /// Return the container whose subtree should be observed for new messages.
  conversationRoot(): HTMLElement | null;
  /// Find every code-bearing container currently visible in the DOM.
  /// Adapters should be permissive here — the parser decides what's an Anvaya
  /// call, not the selector.
  collectMessages(): AssistantMessage[];
  /// Inject a result card immediately after the given message element.
  injectResult(message: AssistantMessage, card: HTMLElement): void;
  /// Set the text value of the site's chat input area (so the AI can read it).
  setInputValue(text: string): void;
}

/// Try to locate a stable id for an element.
export function stableId(el: Element): string {
  if (el.id) return el.id;
  // Fall back to a path-from-root signature.
  const parts: string[] = [];
  let cur: Element | null = el;
  while (cur && cur.parentElement) {
    const idx = Array.from(cur.parentElement.children).indexOf(cur);
    parts.unshift(`${cur.tagName.toLowerCase()}${idx}`);
    cur = cur.parentElement;
  }
  return parts.join('/');
}