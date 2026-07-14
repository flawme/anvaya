// Claude (claude.ai) adapter.
//
// Claude's chat DOM uses `[data-testid="user-message"]` for user turns and
// `.font-claude-message` / `.prose` containers for assistant turns. We collect
// the latter and inject result cards after the parent message bubble.
//
// Selectors mirror the public-facing chat UI as of Claude 3.5 / 4 and are
// intentionally broad so cosmetic tweaks don't break scanning.

import type { AssistantMessage, SiteAdapter } from './types';
import { stableId } from './types';

const HOSTS = ['claude.ai'];

export function matchesClaude(): boolean {
  return HOSTS.some((h) => location.hostname === h || location.hostname.endsWith('.' + h));
}

export const ClaudeAdapter: SiteAdapter = {
  name: 'claude',

  conversationRoot(): HTMLElement | null {
    return (
      document.querySelector('[class*="conversation"]') as HTMLElement | null ??
      document.querySelector('main') as HTMLElement | null
    );
  },

  collectMessages(): AssistantMessage[] {
    const candidates = document.querySelectorAll<HTMLElement>(
      '.font-claude-message, ' +
      'div[class*="prose"]::-webkit-scrollbar, ' +
      '[data-testid*="assistant"] .prose, ' +
      'div[class*="message"] div[class*="prose"]',
    );
    if (candidates.length === 0) {
      // Fallback: any `.prose` that isn't inside a user message bubble.
      const proses = document.querySelectorAll<HTMLElement>('.prose, [class*="markdown"]');
      const out: AssistantMessage[] = [];
      proses.forEach((el) => {
        const user = el.closest('[data-testid="user-message"]');
        if (user) return;
        out.push({ id: stableId(el), el });
      });
      return out;
    }
    return Array.from(candidates).map((el) => ({ id: stableId(el), el }));
  },

  injectResult(message: AssistantMessage, card: HTMLElement): void {
    const host = message.el.closest('div[class*="message"]') ?? message.el;
    host.insertAdjacentElement('afterend', card);
  },

  setInputValue(text: string): void {
    const el = document.querySelector('div[contenteditable="true"].ProseMirror') as HTMLElement | null;
    if (!el) return;
    el.focus();
    document.execCommand('insertText', false, text);
  }
};