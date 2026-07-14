// Anvaya tool-call parser.
//
// AI assistants emit Anvaya tool calls as fenced code blocks whose labeled
// language is `anvaya`. Example, embedded in any assistant message:
//
//   ```anvaya
//   {"kind":"write","path":"Desktop/notes.md","content":"hello"}
//   ```
//
// The content is parsed as an `Action`. Tools that successfully parse are
// forwarded to the background service worker; the resulting `Response`
// replaces the fenced block with a styled result card.
//
// Inserting a code block labeled `anvaya` is the only contract we ask of the
// agent, which keeps the design vendor-agnostic: any chat that emits Markdown
// works, including future AI tools we don't ship an adapter for yet.

import type { Action, ActionKind, Response } from '@/lib/protocol';
import { send } from '@/lib/messages';
import type { Reply } from '@/lib/messages';

// Code-fence labels that are unambiguously Anvaya tool calls.
const STRICT_LABELS = new Set(['anvaya', 'anv']);
// Labels that we ALSO scan, but only when the parsed body's `kind` (or
// `action`) matches a known Anvaya action. ChatGPT often emits JSON inside
// ```json blocks; we treat those as tool calls only when the shape matches.
const CANDIDATE_LABELS = new Set(['json', '']);

export interface ToolCall {
  /// Action parsed from the code block body; null if parse failed.
  action: Action | null;
  /// Raw text of the code block body, for diagnostics.
  body: string;
  /// First-line label extracted from the info string, if any.
  label: string;
  /// Element of the `<pre>`/`<code>` block so scannners can mark it done.
  fenceEl: HTMLElement;
}

/// Whether a given info string (the bit after the opening triple backtick)
/// is an explicit Anvaya tool-call label.
export function isAnvayaLabel(label: string): boolean {
  return STRICT_LABELS.has(label.trim().toLowerCase());
}

/// Parse a fenced-block body as an Anvaya `Action`. Returns `null` when the
/// JSON is invalid or the `kind` is not one of the known action tags.
///
/// Accepts either `kind` or `action` as the discriminator key (agents are
/// inconsistent); either string variants are accepted.
export function parseAction(body: string): Action | null {
  try {
    const parsed = JSON.parse(body) as Record<string, unknown>;
    const kindRaw = (parsed.kind ?? parsed.action) as string | undefined;
    if (typeof kindRaw !== 'string') return null;
    if (!isKnownKind(kindRaw)) return null;
    // Normalize so downstream code sees `kind`.
    const normalized = { ...parsed, kind: kindRaw };
    return normalized as unknown as Action;
  } catch {
    return null;
  }
}

/// Classify a fence's label: 'strict' (always try to parse),
/// 'candidate' (only if parse succeeds and is a known action), 'skip'.
export function classifyLabel(label: string): 'strict' | 'candidate' | 'skip' {
  const l = label.trim().toLowerCase();
  if (STRICT_LABELS.has(l)) return 'strict';
  if (CANDIDATE_LABELS.has(l)) return 'candidate';
  return 'skip';
}

function isKnownKind(k: string): boolean {
  return ['write', 'mkdir', 'read', 'list', 'delete', 'move', 'copy', 'edit', 'grep', 'tree', 'stat', 'project_info', 'batch', 'glob_list'].includes(k);
}

/// Scan an assistant message element for Anvaya fences that haven't been
/// processed yet. Mutates each fence's dataset (`anvaya="pending"|"done"`)
/// so re-scans are cheap.
///
/// Two classes of fences are picked up:
///   - `strict` (label `anvaya`/`anv`): always treated as a tool call; parse
///     failures still produce a card (with an error).
///   - `candidate` (label `json` or empty): scanned, but only treated as a
///     tool call when the body parses and has a known `kind`/`action`. This
///     is what makes natural ChatGPT output (which uses ```json) work.
export function scanFences(messageEl: HTMLElement): ToolCall[] {
  const fences = messageEl.querySelectorAll<HTMLElement>('pre > code, pre');
  const out: ToolCall[] = [];
  fences.forEach((code) => {
    if (code.dataset.anvaya) return;
    const lang = (
      code.className?.match(/language-([\w-]+)/)?.[1] ??
      code.getAttribute('data-language') ??
      code.parentElement?.getAttribute('data-language') ??
      ''
    ).toLowerCase();
    const cls = classifyLabel(lang);
    if (cls === 'skip') return;
    const body = (code.textContent ?? '').trim();
    const action = parseAction(body);
    if (cls === 'candidate' && action === null) return;
    code.dataset.anvaya = 'pending';
    out.push({ action, body, label: lang, fenceEl: code });
  });
  return out;
}

/// Execute a parsed action via the background service worker and return the
/// wire `Response`. Propagates transport errors as a synthetic error envelope.
export async function executeAction(action: Action): Promise<Response> {
  const reply = await send<Reply>({ type: 'EXECUTE', action });
  if (reply.type === 'ERROR') {
    throw new Error(reply.message);
  }
  return (reply as Extract<Reply, { type: 'RESULT' }>).response;
}

export function summarizeAction(a: Action): string {
  switch (a.kind) {
    case 'write': return `write ${(a as unknown as { path: string }).path}`;
    case 'mkdir': return `mkdir ${(a as unknown as { path: string }).path}`;
    case 'read':  return `read ${(a as unknown as { path: string }).path}`;
    case 'list':  return `list ${(a as unknown as { path: string }).path}`;
    case 'delete': return `delete ${(a as unknown as { path: string }).path}`;
    case 'move': {
      const x = a as unknown as { src: string; dst: string };
      return `move ${x.src} → ${x.dst}`;
    }
    case 'copy': {
      const x = a as unknown as { src: string; dst: string };
      return `copy ${x.src} → ${x.dst}`;
    }
    case 'edit': return `edit ${(a as unknown as { path: string }).path}`;
    case 'grep': {
      const x = a as unknown as { path: string; query: string };
      return `grep "${x.query}" in ${x.path}`;
    }
    case 'tree': return `tree ${(a as unknown as { path: string }).path}`;
    case 'stat': return `stat ${(a as unknown as { path: string }).path}`;
    case 'project_info': return `project_info ${(a as unknown as { path: string }).path}`;
    case 'batch': {
      const x = a as unknown as { actions: Action[] };
      return `batch (${x.actions?.length ?? 0} actions)`;
    }
    case 'glob_list': return `glob_list ${(a as unknown as { pattern: string }).pattern}`;
  }
}

export function describeResponse(r: Response): string {
  if (r.status === 'error') return `✗ ${r.error?.code}: ${r.error?.message}`;
  const res = r.result as unknown as { kind: ActionKind } & Record<string, unknown> | undefined;
  if (!res) return '✓ ok';
  switch (res.kind) {
    case 'write':  return `✓ wrote ${(res as unknown as { bytes: number }).bytes} bytes`;
    case 'mkdir':  return `✓ created ${(res as unknown as { path: string }).path}`;
    case 'read': {
      const rs = res as unknown as { content: string; bytes: number };
      const clipped = rs.content.length > 200 ? rs.content.slice(0, 200) + '…' : rs.content;
      return `✓ read (${rs.bytes} bytes)\n${clipped}`;
    }
    case 'list': {
      const rs = res as unknown as { entries: { name: string; kind: string; bytes: number }[] };
      return `✓ ${rs.entries.length} entries\n${rs.entries.map((e) => `${e.kind.padEnd(8)} ${e.bytes}  ${e.name}`).join('\n')}`;
    }
    case 'delete': return `✓ removed ${(res as unknown as { removed: number }).removed} entries`;
    case 'move': {
      const rs = res as unknown as { src: string; dst: string };
      return `✓ moved ${rs.src} → ${rs.dst}`;
    }
    case 'copy': {
      const rs = res as unknown as { src: string; dst: string };
      return `✓ copied ${rs.src} → ${rs.dst}`;
    }
    case 'edit': {
      const rs = res as unknown as { bytes: number; matches: number; diff?: string };
      if (rs.diff) {
        return `✓ edited ${rs.bytes} bytes (matches: ${rs.matches})\n${rs.diff}`;
      }
      return `✓ edited ${rs.bytes} bytes (matches: ${rs.matches})`;
    }
    case 'grep': {
      const rs = res as unknown as { matches: { file: string; line: number; content: string }[] };
      return `✓ found ${rs.matches.length} matches`;
    }
    case 'tree': {
      return `✓ tree retrieved`;
    }
    case 'stat': {
      const rs = res as unknown as { size: number; is_dir: boolean };
      return `✓ stat: ${rs.is_dir ? 'directory' : 'file'}, ${rs.size} bytes`;
    }
    case 'project_info': {
      const rs = res as unknown as { is_git: boolean; language?: string; build_system?: string };
      return `✓ project info: ${rs.language || 'unknown'} (${rs.build_system || 'unknown'})`;
    }
    case 'batch': {
      const rs = res as unknown as { responses: any[] };
      return `✓ batch (${rs.responses?.length ?? 0} responses)`;
    }
    case 'glob_list': {
      const rs = res as unknown as { paths: string[] };
      return `✓ glob_list (${rs.paths?.length ?? 0} paths)\n${rs.paths?.join('\n') ?? ''}`;
    }
  }
}