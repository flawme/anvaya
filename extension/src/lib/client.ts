// Daemon HTTP client. Thin wrapper around fetch() targeting the configured
// localhost base URL. All methods return the typed `Response` envelope; they
// never throw on daemon-logical errors (those arrive as `status: "error"`).

import type {
  ActionResponse,
  OnePathBody,
  PendingSummary,
  Response,
  TwoPathBody,
  WriteBody,
  Action,
  ActionKind,
  Request,
} from './protocol';

export interface EditBody {
  path: string;
  search: string;
  replace: string;
}

export class DaemonClient {
  constructor(public baseUrl: string = 'http://127.0.0.1:7878', public timeoutMs = 10_000) {}

  private async post<T>(path: string, body: unknown): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const res = await fetch(`${this.baseUrl}${path}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!res.ok) {
        throw new DaemonTransportError(`HTTP ${res.status} ${res.statusText}`);
      }
      return (await res.json()) as T;
    } finally {
      clearTimeout(timer);
    }
  }

  private async getJson<T>(path: string): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const res = await fetch(`${this.baseUrl}${path}`, { signal: controller.signal });
      if (!res.ok) throw new DaemonTransportError(`HTTP ${res.status}`);
      return (await res.json()) as T;
    } finally {
      clearTimeout(timer);
    }
  }

  // REST-style endpoints (bare bodies).
  mkdir(body: OnePathBody): Promise<Response> { return this.post('/api/v1/mkdir', body); }
  write(body: WriteBody): Promise<Response> { return this.post('/api/v1/write', body); }
  read(body: OnePathBody): Promise<Response> { return this.post('/api/v1/read', body); }
  list(body: OnePathBody): Promise<Response> { return this.post('/api/v1/list', body); }
  delete(body: OnePathBody): Promise<Response> { return this.post('/api/v1/delete', body); }
  move(body: TwoPathBody): Promise<Response> { return this.post('/api/v1/move', body); }
  copy(body: TwoPathBody): Promise<Response> { return this.post('/api/v1/copy', body); }
  edit(body: EditBody): Promise<Response> { return this.post('/api/v1/edit', body); }

  // Approval channel.
  pending(): Promise<{ pending: PendingSummary[] }> { return this.getJson('/api/v1/pending'); }
  approve(id: string): Promise<{ approved: boolean }> { return this.post(`/api/v1/approve/${id}`, {}); }
  deny(id: string): Promise<{ denied: boolean }> { return this.post(`/api/v1/deny/${id}`, {}); }

  // Uniform envelope endpoint.
  send(req: Request): Promise<Response> { return this.post<Response>('/api/v1', req); }

  async health(): Promise<boolean> {
    try {
      const res = await fetch(`${this.baseUrl}/health`, { method: 'POST' });
      return res.ok;
    } catch {
      return false;
    }
  }

  // Convenience builder: mint a Request envelope from an Action.
  static envelope(kind: ActionKind, action: Action): Request {
    return {
      id: crypto.randomUUID(),
      kind,
      action,
      reason: undefined,
    };
  }
}

export class DaemonTransportError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DaemonTransportError';
  }
}

export type { ActionResponse, Response };