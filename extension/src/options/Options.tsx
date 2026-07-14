import { useEffect, useState } from 'react';
import { getSettings, saveSettings, DEFAULT_SETTINGS, type Settings } from '@/lib/storage';
import { send } from '@/lib/messages';
import { Field, Label, Row, StatusDot } from '@/ui/components';
import type { ConnectionStatus } from '@/lib/storage';

export default function Options() {
  const [s, setS] = useState<Settings>(DEFAULT_SETTINGS);
  const [saved, setSaved] = useState(false);
  const [status, setStatus] = useState<ConnectionStatus>('unknown');

  useEffect(() => {
    getSettings().then(setS);
    send({ type: 'STATUS' }).then((r) => {
      if (r.type === 'STATUS') setStatus(r.connected ? 'connected' : 'disconnected');
    });
  }, []);

  async function persist(patch: Partial<Settings>) {
    const next = await saveSettings(patch);
    setS(next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  return (
    <div className="max-w-xl mx-auto p-6 flex flex-col gap-5">
      <Row>
        <strong className="text-lg">Anvaya — settings</strong>
        <span className="text-sm muted flex items-center gap-2">
          <StatusDot status={status} />
          {status === 'connected' ? 'daemon connected' : status === 'disconnected' ? 'daemon down' : '…'}
        </span>
      </Row>

      <div className="card p-4 flex flex-col gap-3">
        <Field label="Daemon base URL">
          <input
            className="input"
            value={s.baseUrl}
            onChange={(e) => persist({ baseUrl: e.target.value })}
          />
        </Field>
        <Field label="Request timeout (ms)">
          <input
            className="input"
            type="number"
            value={s.timeoutMs}
            onChange={(e) => persist({ timeoutMs: Number(e.target.value) })}
          />
        </Field>
        <Row>
          <Label>Show agent reason in approval prompt</Label>
          <input
            type="checkbox"
            checked={s.showReasonInPrompt}
            onChange={(e) => persist({ showReasonInPrompt: e.target.checked })}
          />
        </Row>
        <Row>
          <Label>Auto-approve every request (danger)</Label>
          <input
            type="checkbox"
            checked={s.autoApprove}
            onChange={(e) => persist({ autoApprove: e.target.checked })}
          />
        </Row>
      </div>

      {saved && <div className="text-xs ok">saved</div>}

      <div className="card p-4 text-xs muted leading-relaxed">
        <Label>How it works</Label>
        <p className="m-0 mt-1">
          The extension forwards approved filesystem requests to the Anvaya
          daemon at <code className="accent">{s.baseUrl}</code>. The daemon
          restricts every path to the configured workspace roots and never
          runs shell commands. See the <code>README</code> and
          <code> docs/SECURITY.md </code> in the repository for the threat
          model.
        </p>
      </div>
    </div>
  );
}