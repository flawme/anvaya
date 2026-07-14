import { useEffect, useState } from 'react';
import { send } from '@/lib/messages';
import type { Reply } from '@/lib/messages';
import type { Action, ActionKind, Response } from '@/lib/protocol';
import type { ConnectionStatus } from '@/lib/storage';
import { getSettings, saveSettings } from '@/lib/storage';
import { Field, Label, Row, StatusDot } from '@/ui/components';

type Tab = 'run' | 'approvals';

const ACTIONS: { kind: ActionKind; label: string; needs: 'path' | 'two' | 'write' | 'edit' | 'grep' | 'batch' | 'glob' }[] = [
  { kind: 'mkdir',  label: 'mkdir',  needs: 'path' },
  { kind: 'write',  label: 'write',  needs: 'write' },
  { kind: 'read',   label: 'read',   needs: 'path' },
  { kind: 'list',   label: 'list',   needs: 'path' },
  { kind: 'delete', label: 'delete', needs: 'path' },
  { kind: 'move',   label: 'move',   needs: 'two' },
  { kind: 'copy',   label: 'copy',   needs: 'two' },
  { kind: 'edit',   label: 'edit',   needs: 'edit' },
  { kind: 'grep',   label: 'grep',   needs: 'grep' },
  { kind: 'tree',   label: 'tree',   needs: 'path' },
  { kind: 'stat',   label: 'stat',   needs: 'path' },
  { kind: 'project_info', label: 'project_info', needs: 'path' },
  { kind: 'batch', label: 'batch', needs: 'batch' },
  { kind: 'glob_list', label: 'glob_list', needs: 'glob' },
];

interface PendingItem {
  id: string;
  kind: string;
  summary: string;
  receivedAt: number;
}

export default function Popup() {
  const [status, setStatus] = useState<ConnectionStatus>('unknown');
  const [baseUrl, setBaseUrl] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [pending, setPending] = useState<PendingItem[]>([]);
  const [tab, setTab] = useState<Tab>('run');

  // Run form state
  const [kind, setKind] = useState<ActionKind>('write');
  const [path, setPath] = useState('Desktop/test.txt');
  const [src, setSrc] = useState('Desktop/a.txt');
  const [dst, setDst] = useState('Desktop/b.txt');
  const [content, setContent] = useState('Hello, Anvaya!');
  const [search, setSearch] = useState('Hello');
  const [replace, setReplace] = useState('Hi');
  const [query, setQuery] = useState('TODO');
  const [pattern, setPattern] = useState('**/*.txt');
  const [actionsText, setActionsText] = useState('[]');
  const [result, setResult] = useState<Response | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refreshStatus() {
    const r = await send<Extract<Reply, { type: 'STATUS' }>>({ type: 'STATUS' });
    setStatus(r.connected ? 'connected' : 'disconnected');
    setBaseUrl(r.baseUrl);
    setPending([]);
    const p = await send<Extract<Reply, { type: 'PENDING' }>>({ type: 'PENDING' });
    setPending(p.items.map((it) => ({ id: it.id, kind: it.kind, summary: it.summary, receivedAt: Date.now() })));
    const s = await getSettings();
    setEnabled(s.extensionEnabled);
  }

  useEffect(() => {
    refreshStatus();
    const id = setInterval(refreshStatus, 4000);
    return () => clearInterval(id as unknown as number);
  }, []);

  const needs = ACTIONS.find((a) => a.kind === kind)!.needs;

  async function run() {
    setErr(null);
    setResult(null);
    setBusy(true);
    try {
      let action: Action;
      switch (needs) {
        case 'path':
          action = { kind, path } as Action;
          break;
        case 'two':
          action = { kind, src, dst } as Action;
          break;
        case 'write':
          action = { kind, path, content } as Action;
          break;
        case 'edit':
          action = { kind, path, search, replace } as Action;
          break;
        case 'grep':
          action = { kind, path, query } as Action;
          break;
        case 'glob':
          action = { kind, pattern } as Action;
          break;
        case 'batch':
          action = { kind, actions: JSON.parse(actionsText) } as Action;
          break;
      }
      const r = await send<Extract<Reply, { type: 'RESULT' }>>({ type: 'EXECUTE', action });
      setResult(r.response);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function approve(id: string) {
    await send<Extract<Reply, { type: 'APPROVED' }>>({ type: 'APPROVE', id });
    await refreshStatus();
  }
  async function deny(id: string) {
    await send<Extract<Reply, { type: 'DENIED' }>>({ type: 'DENY', id });
    await refreshStatus();
  }

  async function toggleEnabled() {
    const next = !enabled;
    setEnabled(next);
    await saveSettings({ extensionEnabled: next });
  }

  return (
    <div className="w-80 p-3 flex flex-col gap-3">
      <Row>
        <div className="flex items-center gap-2">
          <strong className="text-sm">Anvaya</strong>
          <span className="text-xs muted">{baseUrl || '—'}</span>
        </div>
        <button 
          className={`btn text-xs ${enabled ? 'btn-primary' : 'btn-danger'}`} 
          onClick={toggleEnabled}
        >
          {enabled ? 'ON' : 'OFF'}
        </button>
      </Row>

      <Row>
        <span className="text-xs muted flex items-center gap-2">
          <StatusDot status={status} />
          {status === 'connected' ? 'connected' : status === 'disconnected' ? 'disconnected' : '…'}
        </span>
        <Row>
          <button
            className={`btn ${tab === 'run' ? 'btn-primary' : ''}`}
            onClick={() => setTab('run')}
          >run</button>
          <button
            className={`btn ${tab === 'approvals' ? 'btn-primary' : ''}`}
            onClick={() => setTab('approvals')}
          >approvals{pending.length ? ` (${pending.length})` : ''}</button>
          <button className="btn" onClick={refreshStatus}>⟳</button>
        </Row>
      </Row>

      {tab === 'approvals' ? (
        <div className="flex flex-col gap-2">
          {pending.length === 0 ? (
            <div className="card p-3 text-xs muted text-center">Nothing waiting for approval.</div>
          ) : (
            pending.map((p) => (
              <div key={p.id} className="card p-2 flex flex-col gap-2">
                <div className="text-xs"><span className="accent">{p.kind}</span> · {new Date(p.receivedAt).toLocaleTimeString()}</div>
                <pre className="m-0 text-xs surface-2 p-2 overflow-x-auto">{p.summary}</pre>
                <Row>
                  <button className="btn btn-primary" onClick={() => approve(p.id)}>allow</button>
                  <button className="btn btn-danger" onClick={() => deny(p.id)}>deny</button>
                </Row>
              </div>
            ))
          )}
        </div>
      ) : (
        <div className="card p-2 flex flex-col gap-2">
          <Field label="Action">
            <select className="select" value={kind} onChange={(e) => setKind(e.target.value as ActionKind)}>
              {ACTIONS.map((a) => (
                <option key={a.kind} value={a.kind}>{a.label}</option>
              ))}
            </select>
          </Field>

          {(needs === 'path' || needs === 'write' || needs === 'edit') && (
            <Field label="path">
              <input className="input" value={path} onChange={(e) => setPath(e.target.value)} />
            </Field>
          )}
          {needs === 'two' && (
            <>
              <Field label="src"><input className="input" value={src} onChange={(e) => setSrc(e.target.value)} /></Field>
              <Field label="dst"><input className="input" value={dst} onChange={(e) => setDst(e.target.value)} /></Field>
            </>
          )}
          {needs === 'write' && (
            <Field label="content">
              <textarea className="textarea" rows={4} value={content} onChange={(e) => setContent(e.target.value)} />
            </Field>
          )}
          {needs === 'edit' && (
            <>
              <Field label="search"><textarea className="textarea" rows={2} value={search} onChange={(e) => setSearch(e.target.value)} /></Field>
              <Field label="replace"><textarea className="textarea" rows={2} value={replace} onChange={(e) => setReplace(e.target.value)} /></Field>
            </>
          )}
          {needs === 'grep' && (
            <Field label="query"><input className="input" value={query} onChange={(e) => setQuery(e.target.value)} /></Field>
          )}
          {needs === 'glob' && (
            <Field label="pattern"><input className="input" value={pattern} onChange={(e) => setPattern(e.target.value)} /></Field>
          )}
          {needs === 'batch' && (
            <Field label="actions (JSON)"><textarea className="textarea" rows={4} value={actionsText} onChange={(e) => setActionsText(e.target.value)} /></Field>
          )}

          <button className="btn btn-primary" onClick={run} disabled={busy}>
            {busy ? '…' : 'run'}
          </button>
        </div>
      )}

      {err && <div className="card p-2 text-xs danger">{err}</div>}
      {result && (
        <div className="card p-2 text-xs flex flex-col gap-1">
          <Label>result</Label>
          {result.status === 'ok'
            ? <pre className="m-0 ok overflow-x-auto">{JSON.stringify(result.result, null, 2)}</pre>
            : <pre className="m-0 danger overflow-x-auto">{result.error?.code}: {result.error?.message}</pre>}
        </div>
      )}

      <a className="text-xs muted text-center" href="#" onClick={(e) => { e.preventDefault(); chrome.runtime.openOptionsPage(); }}>
        settings
      </a>
    </div>
  );
}