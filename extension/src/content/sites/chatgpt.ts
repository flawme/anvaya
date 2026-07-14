// ChatGPT adapter.
//
// ChatGPT's DOM changes frequently. Rather than chasing specific article/
// turn selectors (which break on every UI refresh), we take a permissive
// approach: the adapter returns ALL containers that hold formatted text on
// the page, and the shared `agent.ts` scanner decides which code fences are
// actually Anvaya tool calls. This is resilient to cosmetic changes because
// we only care about the `<pre>/<code>` elements, not their parent structure.

import type { AssistantMessage, SiteAdapter } from './types';
import { stableId } from './types';

const HOSTS = ['chat.openai.com', 'chatgpt.com'];

export function matchesChatGpt(): boolean {
  return HOSTS.some((h) => location.hostname === h || location.hostname.endsWith('.' + h));
}

export const ChatGptAdapter: SiteAdapter = {
  name: 'chatgpt',

  conversationRoot(): HTMLElement | null {
    return (
      document.querySelector('main') as HTMLElement | null ??
      document.querySelector('[role="log"]') as HTMLElement | null ??
      document.body
    );
  },

  collectMessages(): AssistantMessage[] {
    // Broadly collect every element that can contain a code fence.
    // Prioritize elements with `<pre>` children (where markdown code blocks
    // render), then fall back to generic prose containers.
    const seen = new Set<Element>();
    const out: AssistantMessage[] = [];

    // 1. Any element containing <pre> tags (code blocks live here).
    document.querySelectorAll<HTMLElement>('pre, pre > code').forEach((el) => {
      const host = el.tagName === 'CODE' ? el.closest('pre') ?? el : el;
      if (!seen.has(host)) {
        seen.add(host);
        out.push({ id: stableId(host), el: host as HTMLElement });
      }
    });

    // 2. Markdown / prose containers (assistant message text).
    document.querySelectorAll<HTMLElement>(
      '.markdown, [class*="markdown"], [data-message-author-role="assistant"]',
    ).forEach((el) => {
      if (!seen.has(el)) {
        seen.add(el);
        out.push({ id: stableId(el), el });
      }
    });

    return out;
  },

  injectResult(message: AssistantMessage, card: HTMLElement): void {
    // Anchor on the parent article/prose so the card follows the message.
    const host = message.el.closest('article') ?? message.el.closest('[class*="turn"]') ?? message.el;
    host.insertAdjacentElement('afterend', card);
  },

  setInputValue(text: string): void {
    const el = document.getElementById('prompt-textarea') as HTMLElement | null;
    if (!el) return;
    el.focus();
    if (el.tagName === 'TEXTAREA') {
      (el as HTMLTextAreaElement).value = text;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    } else {
      document.execCommand('insertText', false, text);
    }
  }
};