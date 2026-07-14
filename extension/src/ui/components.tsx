import type { ConnectionStatus } from '@/lib/storage';

export function StatusDot({ status }: { status: ConnectionStatus }) {
  const color =
    status === 'connected' ? 'var(--color-ok)'
    : status === 'disconnected' ? 'var(--color-danger)'
    : 'var(--color-muted)';
  return (
    <span
      className="dot"
      style={{ color, background: color }}
      role="img"
      aria-label={status}
    />
  );
}

export function Label({ children }: { children: React.ReactNode }) {
  return <label className="text-xs uppercase tracking-wider muted">{children}</label>;
}

export function Row({ children }: { children: React.ReactNode }) {
  return <div className="flex items-center justify-between gap-3">{children}</div>;
}

export function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <Label>{label}</Label>
      {children}
    </div>
  );
}