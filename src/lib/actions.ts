import { api } from './api';
import { app } from './store.svelte';
import { menu } from './menu.svelte';
import { confirmDelete } from './confirm.svelte';
import { t } from './i18n.svelte';

interface Target { id: number; name: string; path: string; size: number; kind: string }

export async function copyPath(p: string) {
  try { await navigator.clipboard.writeText(p); app.toast(t('toast.pathCopied'), 'success'); } catch { app.toast(p, 'info'); }
}
export function reveal(p: string) { api.reveal(p).catch(e => app.toast(String(e), 'error')); }

/** Right-click menu shared by the treemap, folder lists and the files table. */
export function nodeMenu(ev: MouseEvent, n: Target, open?: () => void, onDeleted?: () => void) {
  const bucket = n.kind === 'bucket';
  menu.show(ev, [
    ...(open && n.kind === 'dir' ? [{ label: t('action.open'), ico: 'search', run: open }] : []),
    { label: t('action.reveal'), ico: 'folder', disabled: bucket, run: () => reveal(n.path) },
    { label: t('action.copyPath'), ico: 'copy', run: () => copyPath(n.path) },
    ...(n.kind === 'dir' ? [{ label: t('action.scanFolder'), ico: 'refresh', run: () => app.scan(n.path) }] : []),
    '-',
    {
      label: app.permanentDelete ? t('action.deletePerm') : t('action.recycle'), ico: 'trash', danger: true, disabled: bucket,
      run: (e: MouseEvent) => confirmDelete.ask([{ id: n.id, name: n.name, size: n.size }], e, () => onDeleted?.()),
    },
  ]);
}
