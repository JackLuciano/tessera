/** Helpers shared by the system-tool pages (Care, cleaners, uninstaller, internet…). */
import { app } from './store.svelte';
import { t, i18n } from './i18n.svelte';
import { api, type RestorePoint } from './api';

/** What will happen about a restore point, for confirmation dialogs. */
export const rpNote = () => (app.elevated ? t('rp.willCreate') : t('rp.needsAdmin'));

/** Report a restore point result after a change. */
export function rpToast(r: RestorePoint | null | undefined) {
  if (!r || r === 'session' || r === 'unsupported') return;
  if (r === 'created') app.toast(t('rp.created'), 'success');
  else if (r === 'recent') app.toast(t('rp.recent'), 'info');
  else app.toast(t(r === 'needs_admin' ? 'rp.skippedAdmin' : 'rp.skipped'), 'info');
}

/** Show failures without flooding the screen: the first two, then a count. */
export function failures(failed: string[]) {
  for (const f of failed.slice(0, 2)) app.toast(f, 'error');
  if (failed.length > 2) app.toast(t('common.moreFailed', { n: failed.length - 2 }), 'error');
}

export const BROWSERS: Record<string, { name: string; color: string }> = {
  chrome: { name: 'Google Chrome', color: 'oklch(70% 0.17 145)' },
  chromium: { name: 'Chromium', color: 'oklch(68% 0.14 240)' },
  edge: { name: 'Microsoft Edge', color: 'oklch(68% 0.15 200)' },
  brave: { name: 'Brave', color: 'oklch(68% 0.2 40)' },
  vivaldi: { name: 'Vivaldi', color: 'oklch(62% 0.2 25)' },
  opera: { name: 'Opera', color: 'oklch(60% 0.22 25)' },
  opera_gx: { name: 'Opera GX', color: 'oklch(62% 0.25 0)' },
  firefox: { name: 'Firefox', color: 'oklch(70% 0.19 50)' },
};

/** "C:" → the drive's label from the drives list, for nicer headings. */
export const driveLabel = (mount: string) => {
  const d = app.drives.find(x => x.mount.toUpperCase().startsWith(mount.toUpperCase()));
  return d ? `${d.name || t('drive.local')} (${mount})` : mount;
};

/** Open a Windows Settings / Security page or a vendor download page (the backend allows only those). */
export function openSystem(uri: string) {
  api.openUri(uri).catch(e => app.toast(String(e), 'error'));
}

/** Date like "12 Mar 2025" in the UI language. */
export const day = (unix: number) => (unix ? new Date(unix * 1000).toLocaleDateString(i18n.locale, { year: 'numeric', month: 'short', day: 'numeric' }) : '—');
/** Time like "14:05" in the UI language. */
export const clock = (unix: number) => new Date(unix * 1000).toLocaleTimeString(i18n.locale, { hour: '2-digit', minute: '2-digit' });
