// Persistent settings held in chrome.storage.sync. Survives browser restarts
// and syncs across the signed-in profile.

export interface Settings {
  baseUrl: string;
  timeoutMs: number;
  showReasonInPrompt: boolean;
  // When true, the extension auto-approves any request without showing the
  // popup. Equivalent to the daemon's ANVAYA_AUTO_APPROVE env var, but scoped
  // to this extension instance.
  autoApprove: boolean;
  // Global kill switch for the content scripts.
  extensionEnabled: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  baseUrl: 'http://127.0.0.1:7878',
  timeoutMs: 300_000, // 5 min — approval may take time
  showReasonInPrompt: true,
  autoApprove: false,
  extensionEnabled: true,
};

const KEY = 'anvaya.settings';

export async function getSettings(): Promise<Settings> {
  const stored = await chrome.storage.sync.get(KEY);
  return { ...DEFAULT_SETTINGS, ...(stored[KEY] ?? {}) };
}

export async function saveSettings(patch: Partial<Settings>): Promise<Settings> {
  const current = await getSettings();
  const next: Settings = { ...current, ...patch };
  await chrome.storage.sync.set({ [KEY]: next });
  return next;
}

// Connection status is ephemeral runtime state (kept in session storage so
// popups opened moments apart agree, without touching disk).
export type ConnectionStatus = 'unknown' | 'connected' | 'disconnected';

const STATUS_KEY = 'anvaya.connection';

export async function getConnectionStatus(): Promise<ConnectionStatus> {
  const v = await chrome.storage.session.get(STATUS_KEY);
  return (v[STATUS_KEY] as ConnectionStatus) ?? 'unknown';
}

export async function setConnectionStatus(s: ConnectionStatus): Promise<void> {
  await chrome.storage.session.set({ [STATUS_KEY]: s });
}

// Pending approval snapshot (session-only cache so popups opened moments apart
// agree). The daemon's pending queue is the source of truth; this is just a
// freshness hint for the badge.
export interface PendingRequest {
  id: string;
  kind: string;
  summary: string;
  receivedAt: number;
}

const QUEUE_KEY = 'anvaya.queue';

export async function getQueue(): Promise<PendingRequest[]> {
  const v = await chrome.storage.session.get(QUEUE_KEY);
  return (v[QUEUE_KEY] as PendingRequest[]) ?? [];
}

export async function setQueue(q: PendingRequest[]): Promise<void> {
  await chrome.storage.session.set({ [QUEUE_KEY]: q });
  await refreshBadge();
}

export async function refreshBadge(): Promise<void> {
  const q = await getQueue();
  const text = q.length === 0 ? '' : String(q.length);
  await chrome.action.setBadgeText({ text });
  if (text) await chrome.action.setBadgeBackgroundColor({ color: '#ef4444' });
}