// Google Gemini (gemini.google.com) adapter.
//
// Gemini renders messages inside `conversation-turn` custom elements and
// `.model-response-text` containers for assistant turns. As with the other
// adapters, the selectors are deliberately broad and the shared tool-call
// scanner in `agent.ts` is the source of truth on whether a block is an
// Anvaya invocation.

import type { AssistantMessage, SiteAdapter } from './types';
import { stableId } from './types';

const HOSTS = ['gemini.google.com'];

export function matchesGemini(): boolean {
  return HOSTS.some((h) => location.hostname === h || location.hostname.endsWith('.' + h));
}

export const GeminiAdapter: SiteAdapter = {
  name: 'gemini',

  conversationRoot(): HTMLElement | null {
    return (
      document.querySelector('chat-window') as HTMLElement | null ??
      document.querySelector('main') as HTMLElement | null
    );
  },

  collectMessages(): AssistantMessage[] {
    const candidates = document.querySelectorAll<HTMLElement>(
      '.model-response-text, ' +
      'model-response .response-container, ' +
      'conversation-turn[model-response] .response-container, ' +
      'message-content[class*="model-response"]',
    );
    if (candidates.length === 0) {
      const fallback = document.querySelectorAll<HTMLElement>(
        '[class*="model-response"], [data-turn*="model"]',
      );
      return Array.from(fallback).map((el) => ({ id: stableId(el), el }));
    }
    return Array.from(candidates).map((el) => ({ id: stableId(el), el }));
  },

  injectResult(message: AssistantMessage, card: HTMLElement): void {
    const turn = message.el.closest('conversation-turn, [class*="turn"]') ?? message.el;
    turn.insertAdjacentElement('afterend', card);
  },

  setInputValue(text: string): void {
    const el = document.querySelector('rich-textarea > div[contenteditable="true"], .chat-input, textarea') as HTMLElement | null;
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