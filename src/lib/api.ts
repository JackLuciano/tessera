import { invoke } from '@tauri-apps/api/core';

export type Cat = 'video' | 'games' | 'dev' | 'apps' | 'system' | 'images' | 'cache' | 'archives' | 'docs';
export type Kind = 'dir' | 'file' | 'bucket';

export interface Drive { name: string; mount: string; fs: string; total: number; available: number; removable: boolean; kind: string }
export interface DupGroup { size: number; wasted: number; files: { id: number; path: string; modified: number }[] }
export interface SnapshotInfo { ts: number; size: number; files: number }
export interface DiffRow { path: string; before: number; after: number; delta: number }
export interface ScanSummary { engine: 'mft' | 'walk'; root: string; size: number; files: number; dirs: number; elapsed_ms: number; errors: number; cats: { cat: Cat; size: number }[] }
export interface TreeNode { id: number; name: string; path: string; size: number; files: number; dirs: number; kind: Kind; cat: Cat; modified: number; child_count: number; children: TreeNode[] }
export interface FileEntry { id: number; name: string; path: string; size: number; cat: Cat; modified: number }
export interface JunkGroup { key: string; label: string; hint: string; safe: boolean; size: number; ids: number[]; sample: string[] }
export interface DeleteResult { freed: number; deleted: number; failed: string[] }

export const api = {
  drives: () => invoke<Drive[]>('list_drives'),
  scan: (path: string) => invoke<ScanSummary>('scan', { path }),
  cancel: () => invoke<void>('cancel_scan'),
  node: (id: number, depth = 2, maxChildren = 80) => invoke<TreeNode>('get_node', { id, depth, maxChildren }),
  ancestors: (id: number) => invoke<[number, string][]>('get_ancestors', { id }),
  largestFiles: (limit = 300) => invoke<FileEntry[]>('largest_files', { limit }),
  junk: () => invoke<JunkGroup[]>('find_junk'),
  delete: (ids: number[], permanent: boolean) => invoke<DeleteResult>('delete_nodes', { ids, permanent }),
  reveal: (path: string) => invoke<void>('reveal', { path }),
  isElevated: () => invoke<boolean>('is_elevated'),
  relaunchAsAdmin: () => invoke<void>('relaunch_as_admin'),
  duplicates: () => invoke<DupGroup[]>('find_duplicates'),
  cancelDuplicates: () => invoke<void>('cancel_duplicates'),
  historyList: (root: string) => invoke<SnapshotInfo[]>('history_list', { root }),
  historyDiff: (ts: number) => invoke<DiffRow[]>('history_diff', { ts }),
};

export const CATS: Record<Cat, { label: string; ico: string }> = {
  video: { label: 'Video', ico: 'video' },
  games: { label: 'Games', ico: 'game' },
  dev: { label: 'Dev & code', ico: 'code' },
  apps: { label: 'Applications', ico: 'app' },
  system: { label: 'System', ico: 'cpu' },
  images: { label: 'Images', ico: 'image' },
  cache: { label: 'Caches & temp', ico: 'cache' },
  archives: { label: 'Archives & ISO', ico: 'archive' },
  docs: { label: 'Documents & other', ico: 'doc' },
};

/** Human-readable size; binary units shown with the familiar GB/MB labels like Windows Explorer. */
export function fmt(bytes: number): string {
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = bytes, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}
export const num = (n: number) => n.toLocaleString('en-US');
export function ago(unixSecs: number): string {
  if (!unixSecs) return '—';
  const days = Math.floor((Date.now() / 1000 - unixSecs) / 86400);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  if (days < 14) return `${days} days ago`;
  if (days < 60) return `${Math.round(days / 7)} weeks ago`;
  if (days < 365) return `${Math.round(days / 30)} months ago`;
  const y = days / 365;
  return `${y < 2 ? 1 : y.toFixed(1)} year${y >= 2 ? 's' : ''} ago`;
}
export const css = (v: string) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
export const catColor = (c: Cat) => `var(--cat-${c})`;
