// Single message-channel type shared between the background service worker,
// the popup, and the content scripts. Every message is a discriminated union
// tagged by `type`.

import type { Response, Request, Action, PendingSummary } from './protocol';

export type Message =
  | { type: 'PING' }
  | { type: 'STATUS' }
  | { type: 'EXECUTE'; action: Action; reason?: string }
  | { type: 'EXECUTE_ENVELOPE'; request: Request }
  | { type: 'PENDING' }
  | { type: 'APPROVE'; id: string }
  | { type: 'DENY'; id: string }
  | { type: 'INJECT_RESULT'; requestId: string; response: Response };

export type Reply =
  | { type: 'PONG' }
  | { type: 'STATUS'; connected: boolean; baseUrl: string; pending: number }
  | { type: 'RESULT'; response: Response }
  | { type: 'ERROR'; message: string }
  | { type: 'PENDING'; items: PendingSummary[] }
  | { type: 'APPROVED'; id: string }
  | { type: 'DENIED'; id: string }
  | { type: 'INJECTED'; ok: boolean };

export async function send<T extends Reply>(msg: Message): Promise<T> {
  return chrome.runtime.sendMessage(msg) as Promise<T>;
}

// Convenience for content scripts that need a reply routed back via a tab
// message (used by the background to push INJECT_RESULT to content scripts).
export async function sendToTab<T extends Reply>(
  tabId: number,
  msg: Message,
): Promise<T> {
  return chrome.tabs.sendMessage(tabId, msg) as Promise<T>;
}