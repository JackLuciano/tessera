import { api, fmt } from './api';
import { app } from './store.svelte';
import { t } from './i18n.svelte';

export interface DeleteItem { id: number; name: string; size: number }

class ConfirmState {
  open = $state(false);
  items = $state<DeleteItem[]>([]);
  permanent = $state(false);
  ack = $state(false);
  busy = $state(false);
  /** When set, these ids are deleted instead of `items` (used for grouped cleanup rows). */
  idsOverride: number[] | null = null;
  private onDone: ((freed: number, ids: number[]) => void) | null = null;

  /** Ask before deleting. Shift-click (ev.shiftKey) forces a one-off permanent delete. */
  ask(items: DeleteItem[], ev?: MouseEvent | KeyboardEvent, onDone?: (freed: number, ids: number[]) => void) {
    if (!items.length) return;
    this.items = items;
    this.permanent = app.permanentDelete || !!ev?.shiftKey;
    this.ack = false;
    this.onDone = onDone ?? null;
    this.idsOverride = null;
    this.open = true;
  }

  get total() { return this.items.reduce((s, i) => s + i.size, 0); }

  async run() {
    this.busy = true;
    try {
      const ids = this.idsOverride ?? this.items.map(i => i.id);
      const res = await api.delete(ids, this.permanent);
      if (res.deleted) {
        app.toast(t(this.permanent ? 'toast.deletedPerm' : 'toast.recycled', { size: fmt(res.freed) }), this.permanent ? 'warning' : 'success');
        await app.afterDelete(res.freed);
        this.onDone?.(res.freed, ids);
      }
      for (const f of res.failed.slice(0, 3)) app.toast(f, 'error');
    } catch (e) {
      app.toast(String(e), 'error');
    } finally {
      this.busy = false;
      this.open = false;
    }
  }
}

export const confirmDelete = new ConfirmState();
