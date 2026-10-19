import { listen } from '@tauri-apps/api/event';
import { api, fmt, type DeleteProgress } from './api';
import { app } from './store.svelte';
import { t } from './i18n.svelte';

export interface DeleteItem { id: number; name: string; size: number }

class ConfirmState {
  open = $state(false);
  items = $state<DeleteItem[]>([]);
  permanent = $state(false);
  /** Permanent delete only: overwrite the files first so they can't be recovered. */
  shred = $state(false);
  ack = $state(false);
  busy = $state(false);
  /** Live progress while deleting (null before the first update). */
  progress = $state<DeleteProgress | null>(null);
  stopping = $state(false);
  started = 0;
  /** When set, these ids are deleted instead of `items` (used for grouped cleanup rows). */
  idsOverride: number[] | null = null;
  private onDone: ((freed: number, ids: number[]) => void) | null = null;

  /** Ask before deleting. Shift-click (ev.shiftKey) forces a one-off permanent delete. */
  ask(items: DeleteItem[], ev?: MouseEvent | KeyboardEvent, onDone?: (freed: number, ids: number[]) => void) {
    if (!items.length) return;
    this.items = items;
    this.permanent = app.permanentDelete || !!ev?.shiftKey;
    this.shred = false;
    this.ack = false;
    this.onDone = onDone ?? null;
    this.idsOverride = null;
    this.open = true;
  }

  get total() { return this.items.reduce((s, i) => s + i.size, 0); }

  async run() {
    this.busy = true;
    this.stopping = false;
    this.progress = null;
    this.started = Date.now();
    const unlisten = await listen<DeleteProgress>('delete-progress', e => (this.progress = e.payload));
    try {
      const ids = this.idsOverride ?? this.items.map(i => i.id);
      const res = await api.delete(ids, this.permanent, this.permanent && this.shred);
      if (res.cancelled) app.toast(t('confirm.stopped', { size: fmt(res.freed) }), 'info');
      else if (res.deleted) app.toast(t(this.permanent ? 'toast.deletedPerm' : 'toast.recycled', { size: fmt(res.freed) }), this.permanent ? 'warning' : 'success');
      if (res.deleted) {
        await app.afterDelete(res.freed);
        this.onDone?.(res.freed, ids);
      }
      for (const f of res.failed.slice(0, 3)) app.toast(f, 'error');
    } catch (e) {
      app.toast(String(e), 'error');
    } finally {
      unlisten();
      this.busy = false;
      this.open = false;
      this.progress = null;
    }
  }

  stop() {
    this.stopping = true;
    api.cancelDelete().catch(() => {});
  }

  /** Rough time left from the byte rate so far; null until there is enough to go on. */
  get eta(): number | null {
    const p = this.progress;
    const elapsed = (Date.now() - this.started) / 1000;
    if (!p || p.bytes <= 0 || elapsed < 1.5) return null;
    return Math.max(0, ((p.total_bytes - p.bytes) / p.bytes) * elapsed);
  }
}

export const confirmDelete = new ConfirmState();
