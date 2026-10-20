import type { View } from './store.svelte';

/** True in the Windows WebView2 (the optimizer, registry, uninstaller and updater only exist there). */
export const IS_WINDOWS = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);

export interface NavItem { v: View; ico: string }
export interface NavGroup { key: string; ico: string; items: NavItem[] }

/** Top pages, above the groups: the pinned-tile dashboard and Care. */
export const NAV_TOP: NavItem[] = [{ v: 'home', ico: 'home' }, { v: 'care', ico: 'heart' }];

export const NAV_GROUPS: NavGroup[] = [
  ...(IS_WINDOWS ? [{ key: 'gaming', ico: 'game', items: [
    { v: 'gamemode' as View, ico: 'rocket' }, { v: 'library' as View, ico: 'app' }, { v: 'graphics' as View, ico: 'image' },
    { v: 'fps' as View, ico: 'activity' }, { v: 'display' as View, ico: 'monitor' },
  ] }] : []),
  { key: 'disk', ico: 'drive', items: [
    { v: 'overview', ico: 'overview' }, { v: 'explorer', ico: 'explorer' }, { v: 'files', ico: 'files' },
    { v: 'dupes', ico: 'copy' }, { v: 'history', ico: 'history' },
  ] },
  { key: 'clean', ico: 'cleanup', items: [
    { v: 'cleanup', ico: 'trash' }, { v: 'privacy', ico: 'eyeOff' },
    ...(IS_WINDOWS ? [{ v: 'registry' as View, ico: 'registry' }] : []),
    { v: 'shortcuts', ico: 'unlink' }, { v: 'shredder', ico: 'shred' },
  ] },
  ...(IS_WINDOWS ? [{ key: 'optimize', ico: 'gauge', items: [
    { v: 'optimizer' as View, ico: 'zap' }, { v: 'startup' as View, ico: 'power' },
    { v: 'background' as View, ico: 'layers' }, { v: 'internet' as View, ico: 'globe' },
  ] }] : []),
  { key: 'health', ico: 'heartPulse', items: [
    { v: 'slow', ico: 'gauge' }, { v: 'temps', ico: 'thermo' }, { v: 'health', ico: 'drive' }, { v: 'battery', ico: 'battery' },
    ...(IS_WINDOWS ? [
      { v: 'crashes' as View, ico: 'alert' }, { v: 'security' as View, ico: 'shield' }, { v: 'boot' as View, ico: 'clock' },
      { v: 'wake' as View, ico: 'moon' },
    ] : []),
  ] },
  { key: 'tools', ico: 'tools', items: [
    ...(IS_WINDOWS ? [
      { v: 'uninstaller' as View, ico: 'package' }, { v: 'updater' as View, ico: 'upgrade' },
      { v: 'drivers' as View, ico: 'cpu' }, { v: 'winupdate' as View, ico: 'download' },
    ] : []),
    { v: 'backup', ico: 'archive' }, { v: 'bench', ico: 'gauge' }, { v: 'multipc', ico: 'pcs' }, { v: 'report', ico: 'chart' },
  ] },
];

export const NAV_SETTINGS: NavItem = { v: 'settings', ico: 'settings' };

/** Every page in sidebar order (keyboard shortcuts 1–9 and the command palette use this). */
export const NAV_ALL: NavItem[] = [...NAV_TOP, ...NAV_GROUPS.flatMap(g => g.items), NAV_SETTINGS];

export const groupOf = (v: View) => NAV_GROUPS.find(g => g.items.some(i => i.v === v)) ?? null;
export const iconOf = (v: View) => NAV_ALL.find(i => i.v === v)?.ico ?? 'overview';
