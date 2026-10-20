import type { View } from './store.svelte';
import { IS_WINDOWS } from './nav';

/**
 * Home dashboard tiles: which are pinned, in what order and how wide. Kept in the browser storage
 * under a `tessera-` key, so it travels with the settings export.
 */
export type TileKind = 'care' | 'temps' | 'load' | 'ping' | 'fps' | 'space' | 'battery' | 'backup' | 'boot' | 'peers' | 'report';
export interface Tile { kind: TileKind; wide: boolean }

/** Every tile and the page it opens. Windows-only pages only offer their tiles on Windows. */
export const TILES: { kind: TileKind; view: View; ico: string; windows?: boolean }[] = [
  { kind: 'care', view: 'care', ico: 'heart' },
  { kind: 'temps', view: 'temps', ico: 'thermo' },
  { kind: 'load', view: 'slow', ico: 'gauge' },
  { kind: 'ping', view: 'fps', ico: 'ping', windows: true },
  { kind: 'fps', view: 'fps', ico: 'activity', windows: true },
  { kind: 'space', view: 'overview', ico: 'drive' },
  { kind: 'battery', view: 'battery', ico: 'battery' },
  { kind: 'backup', view: 'backup', ico: 'archive' },
  { kind: 'boot', view: 'boot', ico: 'clock', windows: true },
  { kind: 'peers', view: 'multipc', ico: 'pcs' },
  { kind: 'report', view: 'report', ico: 'chart' },
];
export const AVAILABLE = TILES.filter(t => !t.windows || IS_WINDOWS);

const KEY = 'tessera-dashboard';
const DEFAULT: Tile[] = [
  { kind: 'care', wide: false }, { kind: 'temps', wide: false }, { kind: 'load', wide: false }, { kind: 'space', wide: true },
  ...(IS_WINDOWS ? [{ kind: 'fps' as TileKind, wide: false }] : []), { kind: 'report', wide: false },
];

function load(): Tile[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (Array.isArray(v)) return v.filter(t => AVAILABLE.some(a => a.kind === t?.kind)).map(t => ({ kind: t.kind, wide: !!t.wide }));
  } catch {}
  return DEFAULT;
}

class Dashboard {
  tiles = $state<Tile[]>(load());

  #save() { try { localStorage.setItem(KEY, JSON.stringify(this.tiles)); } catch {} }
  has(kind: TileKind) { return this.tiles.some(t => t.kind === kind); }
  toggle(kind: TileKind) {
    this.tiles = this.has(kind) ? this.tiles.filter(t => t.kind !== kind) : [...this.tiles, { kind, wide: false }];
    this.#save();
  }
  widen(kind: TileKind) { this.tiles = this.tiles.map(t => (t.kind === kind ? { ...t, wide: !t.wide } : t)); this.#save(); }
  /** Move `kind` to where `before` is (drag and drop). */
  move(kind: TileKind, before: TileKind) {
    if (kind === before) return;
    const t = this.tiles.find(x => x.kind === kind);
    if (!t) return;
    const rest = this.tiles.filter(x => x.kind !== kind);
    const i = rest.findIndex(x => x.kind === before);
    rest.splice(i < 0 ? rest.length : i, 0, t);
    this.tiles = rest;
    this.#save();
  }
  reset() { this.tiles = DEFAULT; this.#save(); }
}

export const dash = new Dashboard();

/** The tile a page can pin to Home, if any. */
export const tileFor = (v: View) => AVAILABLE.find(t => t.view === v && t.kind !== 'ping')?.kind ?? null;
