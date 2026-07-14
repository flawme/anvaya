// Protocol mirror of `shared/src`. The Rust crate is the source of truth;
// this file is hand-synced so the extension has a typed view of the wire
// format without a build-time codegen step (kept minimal for v0.1).

export const PROTOCOL_VERSION = 1 as const;
export const API_PREFIX = '/api/v1' as const;

export type ActionKind =
  | 'write'
  | 'mkdir'
  | 'read'
  | 'list'
  | 'delete'
  | 'move'
  | 'copy'
  | 'edit'
  | 'grep'
  | 'tree'
  | 'stat'
  | 'project_info'
  | 'batch'
  | 'glob_list';

export type WriteLocation = 'overwrite' | 'append';

export type Action =
  | { kind: 'write'; path: string; content: string; location?: WriteLocation }
  | { kind: 'mkdir'; path: string }
  | { kind: 'read'; path: string; offset?: number; length?: number }
  | { kind: 'list'; path: string }
  | { kind: 'delete'; path: string }
  | { kind: 'move'; src: string; dst: string }
  | { kind: 'copy'; src: string; dst: string }
  | { kind: 'edit'; path: string; search?: string; replace?: string; regex?: boolean; replace_all?: boolean; insert_before?: string; insert_after?: string; append?: boolean; prepend?: boolean; dry_run_diff?: boolean }
  | { kind: 'grep'; path: string; query: string }
  | { kind: 'tree'; path: string }
  | { kind: 'stat'; path: string }
  | { kind: 'project_info'; path: string }
  | { kind: 'batch'; actions: Action[] }
  | { kind: 'glob_list'; pattern: string };

export interface Request {
  id: string;
  kind: ActionKind;
  action: Action;
  reason?: string;
}

export type ResponseStatus = 'ok' | 'error';

export type ErrorCode =
  | 'bad_request'
  | 'unauthorized'
  | 'forbidden'
  | 'not_found'
  | 'conflict'
  | 'invalid_path'
  | 'path_traversal'
  | 'io'
  | 'internal';

export interface ResponseError {
  code: ErrorCode;
  message: string;
}

export type EntryKind = 'file' | 'dir' | 'symlink';

export interface ListEntry {
  name: string;
  kind: EntryKind;
  bytes: number;
}

export type ActionResponse =
  | { kind: 'write'; bytes: number }
  | { kind: 'mkdir'; path: string }
  | { kind: 'delete'; removed: number }
  | { kind: 'move'; src: string; dst: string }
  | { kind: 'copy'; src: string; dst: string }
  | { kind: 'read'; content: string; bytes: number }
  | { kind: 'list'; entries: ListEntry[] }
  | { kind: 'edit'; bytes: number; matches: number; diff?: string }
  | { kind: 'grep'; matches: { file: string; line: number; content: string }[] }
  | { kind: 'tree'; root: { name: string; is_dir: boolean; children?: any[] } }
  | { kind: 'stat'; size: number; modified_secs: number; is_dir: boolean; is_file: boolean; is_symlink: boolean; unix_mode?: number }
  | { kind: 'project_info'; is_git: boolean; language?: string; build_system?: string }
  | { kind: 'batch'; responses: ActionResponse[] }
  | { kind: 'glob_list'; paths: string[] };

export interface Response {
  id: string;
  status: ResponseStatus;
  result?: ActionResponse;
  error?: ResponseError;
}

// Bare REST-style request bodies (one per endpoint). Mirrors the Rust
// `OnePath` / `TwoPath` / `WriteBody` extractors in `daemon/src/api/mod.rs`.
export type OnePathBody = { path: string };
export type TwoPathBody = { src: string; dst: string };
export type WriteBody = {
  path: string;
  content?: string;
  location?: WriteLocation;
};

// Pending request summary, returned by GET /api/v1/pending.
export interface PendingSummary {
  id: string;
  kind: ActionKind;
  summary: string;
  reason?: string;
}