import type { View } from './store.svelte';

/** True in the Windows WebView2 (the optimizer, registry, uninstaller and updater only exist there). */
export const IS_WINDOWS = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);

export interface NavItem { v: View; ico: string }
export interface NavGroup { key: string; ico: string; items: NavItem[] }

/** Home page, above the groups. */
export const NAV_HOME: NavItem = { v: 'care', ico: 'heart' };

export const NAV_GROUPS: NavGroup[] = [
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
    { v: 'optimizer' as View, ico: 'zap' }, { v: 'startup' as View, ico: 'rocket' },
    { v: 'background' as View, ico: 'activity' }, { v: 'internet' as View, ico: 'globe' },
  ] }] : []),
  { key: 'tools', ico: 'tools', items: [
    ...(IS_WINDOWS ? [{ v: 'uninstaller' as View, ico: 'package' }, { v: 'updater' as View, ico: 'upgrade' }] : []),
    { v: 'health', ico: 'thermo' },
  ] },
];

export const NAV_SETTINGS: NavItem = { v: 'settings', ico: 'settings' };

/** Every page in sidebar order (keyboard shortcuts 1–9 and the command palette use this). */
export const NAV_ALL: NavItem[] = [NAV_HOME, ...NAV_GROUPS.flatMap(g => g.items), NAV_SETTINGS];

export const groupOf = (v: View) => NAV_GROUPS.find(g => g.items.some(i => i.v === v)) ?? null;
export const iconOf = (v: View) => NAV_ALL.find(i => i.v === v)?.ico ?? 'overview';
