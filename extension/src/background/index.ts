// Background service worker / event page.
//
// Responsibilities:
//   - keep the connection indicator fresh by periodically health-checking
//     the daemon and on demand from the popup,
//   - receive EXECUTE / EXECUTE_ENVELOPE messages from the popup or content
//     scripts and forward them to the daemon,
//   - poll the daemon's pending queue and reflect it on the badge + session
//     storage so the popup can render it,
//   - approve / deny requests on behalf of the popup (POST /approve|deny/:id).
//
// Messaging pattern: the `onMessage` listener returns a Promise<Reply> for
// any message it handles, undefined otherwise. This is the promise-based
// style that works in both Chrome and Firefox.

import { DaemonClient } from '@/lib/client';
import {
  getSettings,
  setConnectionStatus,
  setQueue,
  type PendingRequest,
} from '@/lib/storage';
import type { Message, Reply } from '@/lib/messages';
import type { PendingSummary } from '@/lib/protocol';

let pollTimer: number | null = null;
let pollInProgress = false;

chrome.runtime.onInstalled.addListener(async () => {
  await setQueue([]);
  startPolling();
  await checkConnection();
});

chrome.runtime.onStartup.addListener(() => {
  startPolling();
});

function startPolling() {
  if (pollTimer !== null) return;
  chrome.alarms.create('anvaya.tick', { periodInMinutes: 1 });
  // Also poll the pending queue every 2 seconds (cheap localhost GET).
  pollTimer = 1;
  setInterval(pollPending, 2000) as unknown as number;
}

chrome.alarms?.onAlarm.addListener(async (a: { name: string }) => {
  if (a.name !== 'anvaya.tick') return;
  await checkConnection();
});

export async function checkConnection(): Promise<boolean> {
  const s = await getSettings();
  const client = new DaemonClient(s.baseUrl, s.timeoutMs);
  const ok = await client.health();
  await setConnectionStatus(ok ? 'connected' : 'disconnected');
  return ok;
}

async function pollPending(): Promise<void> {
  if (pollInProgress) return;
  pollInProgress = true;
  try {
    const s = await getSettings();
    const client = new DaemonClient(s.baseUrl, s.timeoutMs);
    const { pending } = await client.pending();
    const queue: PendingRequest[] = pending.map(mapPending);
    await setQueue(queue);
    // Auto-approve path: when the user opts in, approve everything fresh.
    if (s.autoApprove && pending.length > 0) {
      for (const p of pending) {
        await client.approve(p.id);
      }
    }
  } catch {
    // daemon down or unreachable — leave the queue as-is
  } finally {
    pollInProgress = false;
  }
}

function mapPending(p: PendingSummary): PendingRequest {
  return { id: p.id, kind: p.kind, summary: p.summary, receivedAt: Date.now() };
}

// Promise-based message handler. Returning a Reply (or Promise<Reply>) from
// the listener resolves `chrome.runtime.sendMessage(msg)` on the caller side.
chrome.runtime.onMessage.addListener(
  (msg: Message): Promise<Reply> | undefined => {
    switch (msg.type) {
      case 'PING':
        return Promise.resolve({ type: 'PONG' });

      case 'STATUS':
        return handleStatus();

      case 'EXECUTE':
        return handleExecute(msg.action, msg.reason);

      case 'EXECUTE_ENVELOPE':
        return handleExecuteEnvelope(msg.request);

      case 'PENDING':
        return handlePending();

      case 'APPROVE':
        return handleApprove(msg.id);

      case 'DENY':
        return handleDeny(msg.id);

      default:
        return undefined;
    }
  },
);

async function handleStatus(): Promise<Reply> {
  const s = await getSettings();
  const connected = await checkConnection();
  // Trigger a poll so the queue is fresh when the popup opens.
  await pollPending();
  const { getQueue } = await import('@/lib/storage');
  const pending = (await getQueue()).length;
  return { type: 'STATUS', connected, baseUrl: s.baseUrl, pending };
}

async function handleExecute(action: import('@/lib/protocol').Action, reason?: string): Promise<Reply> {
  try {
    const s = await getSettings();
    const client = new DaemonClient(s.baseUrl, s.timeoutMs);
    const env = DaemonClient.envelope(action.kind, action);
    if (reason) env.reason = reason;
    const response = await client.send(env);
    return { type: 'RESULT', response };
  } catch (e) {
    return { type: 'ERROR', message: e instanceof Error ? e.message : String(e) };
  }
}

async function handleExecuteEnvelope(request: import('@/lib/protocol').Request): Promise<Reply> {
  try {
    const s = await getSettings();
    const client = new DaemonClient(s.baseUrl, s.timeoutMs);
    const response = await client.send(request);
    return { type: 'RESULT', response };
  } catch (e) {
    return { type: 'ERROR', message: e instanceof Error ? e.message : String(e) };
  }
}

async function handlePending(): Promise<Reply> {
  await pollPending();
  const { getQueue } = await import('@/lib/storage');
  const queue = await getQueue();
  // Convert PendingRequest[] (storage) → PendingSummary[] (wire). The wire
  // type is stricter (kind: ActionKind, no receivedAt).
  const items = queue.map((q) => ({ id: q.id, kind: q.kind as import('@/lib/protocol').ActionKind, summary: q.summary }));
  return { type: 'PENDING', items };
}

async function handleApprove(id: string): Promise<Reply> {
  try {
    const s = await getSettings();
    const client = new DaemonClient(s.baseUrl, s.timeoutMs);
    await client.approve(id);
    // Refresh the queue so the badge is accurate.
    await pollPending();
    return { type: 'APPROVED', id };
  } catch (e) {
    return { type: 'ERROR', message: e instanceof Error ? e.message : String(e) };
  }
}

async function handleDeny(id: string): Promise<Reply> {
  try {
    const s = await getSettings();
    const client = new DaemonClient(s.baseUrl, s.timeoutMs);
    await client.deny(id);
    await pollPending();
    return { type: 'DENIED', id };
  } catch (e) {
    return { type: 'ERROR', message: e instanceof Error ? e.message : String(e) };
  }
}