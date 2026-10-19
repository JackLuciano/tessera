<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { app } from '../store.svelte';
  import { api, fmt, num, type ShredTarget, type DeleteProgress } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { failures } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let queue = $state<ShredTarget[]>([]);
  let passes = $state(1);
  let over = $state(false);
  let busy = $state(false);
  let progress = $state<DeleteProgress | null>(null);

  async function add(paths: string[]) {
    const fresh = paths.filter(p => !queue.some(q => q.path === p));
    if (!fresh.length) return;
    const info = await api.shredInspect(fresh).catch(e => { app.toast(String(e), 'error'); return []; });
    queue = [...queue, ...info];
  }

  onMount(() => {
    // Files dragged in from Explorer / Finder
    let off: (() => void) | undefined;
    try {
      getCurrentWebview().onDragDropEvent(e => {
        if (e.payload.type === 'over' || e.payload.type === 'enter') over = true;
        else if (e.payload.type === 'leave') over = false;
        else if (e.payload.type === 'drop') { over = false; add(e.payload.paths); }
      }).then(u => (off = u)).catch(() => {});
    } catch {}
    return () => off?.();
  });

  async function pick(directory: boolean) {
    const r = await open({ directory, multiple: true });
    if (r) add(Array.isArray(r) ? r : [r]);
  }

  let total = $derived(queue.reduce((s, q) => s + q.size, 0));
  let files = $derived(queue.reduce((s, q) => s + q.files, 0));

  async function shred() {
    const ok = await ask({
      title: t('shred.confirm', { n: queue.length, size: fmt(total) }), text: t('shred.confirmText'), ico: 'shred', danger: true,
      confirm: t('shred.go'), ack: t('confirm.ack'), items: queue.map(q => ({ name: q.path, detail: fmt(q.size) })),
    });
    if (!ok) return;
    busy = true;
    progress = null;
    const unlisten = await listen<DeleteProgress>('delete-progress', e => (progress = e.payload));
    try {
      const out = await api.shred(queue.map(q => q.path), passes);
      if (out.cancelled) app.toast(t('confirm.stopped', { size: fmt(out.freed) }), 'info');
      else app.toast(t('shred.done', { n: out.deleted, size: fmt(out.freed) }), 'success');
      failures(out.failed);
      queue = [];
      await app.refreshDrives();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { unlisten(); busy = false; progress = null; }
  }

  let pct = $derived(progress && progress.total_bytes ? Math.min(100, (progress.bytes / progress.total_bytes) * 100) : 0);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="shred" title={t('shred.title')} sub={t('shred.sub')}>
    <select class="select select-sm w-56" bind:value={passes} disabled={busy}>
      <option value={1}>{t('shred.pass1')}</option>
      <option value={3}>{t('shred.pass3')}</option>
    </select>
  </PageHead>

  <div class="dropzone mt-4 p-8 text-center {over ? 'over' : ''}">
    <Icon name="shred" size="2.2rem" style="color:var(--color-error);opacity:.8" />
    <div class="font-medium mt-2">{t('shred.drop')}</div>
    <div class="text-xs muted">{t('shred.dropHint')}</div>
    <div class="flex gap-2 justify-center mt-4">
      <button class="btn btn-sm gap-1.5" disabled={busy} onclick={() => pick(false)}><Icon name="file" size=".95rem" />{t('shred.addFiles')}</button>
      <button class="btn btn-sm gap-1.5" disabled={busy} onclick={() => pick(true)}><Icon name="folder" size=".95rem" />{t('shred.addFolder')}</button>
    </div>
  </div>

  <div class="rounded-xl soft p-3 mt-3 text-xs flex gap-2 items-start"><Icon name="info" size=".95rem" style="color:var(--color-primary)" /><span>{t('shred.ssd')}</span></div>
</section>

{#if queue.length}
  <section class="glass p-5 reveal" style="--i:1">
    <div class="flex items-center gap-3 flex-wrap">
      <div class="flex-1 min-w-48">
        <div class="font-semibold">{t('shred.queue')}</div>
        <div class="text-xs muted">{t('shred.queueSub', { n: num(files), size: fmt(total) })}</div>
      </div>
      <button class="btn btn-ghost btn-sm" disabled={busy} onclick={() => (queue = [])}>{t('common.clear')}</button>
      <button class="btn btn-error btn-sm gap-2" disabled={busy} onclick={shred}>
        {#if busy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="shred" size="1rem" />{/if}{t('shred.go')}
      </button>
    </div>
    {#if busy}
      <div class="mt-4">
        <div class="flex justify-between text-sm"><span>{t('shred.working')}</span><span class="mono font-semibold">{pct.toFixed(0)}%</span></div>
        <div class="progress-flow mt-2 {progress ? '' : 'indeterminate'}" style="--color-primary:var(--color-error)"><i style="width:{pct}%"></i></div>
        <div class="flex justify-between text-[11px] muted mono mt-1.5">
          <span>{t('confirm.items', { done: progress?.done ?? 0, total: progress?.total ?? queue.length })}</span>
          <button class="btn btn-ghost btn-xs" onclick={() => api.cancelDelete()}>{t('confirm.stop')}</button>
        </div>
      </div>
    {/if}
    <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
      {#each queue as q (q.path)}
        <div class="flex items-center gap-3 py-2">
          <Icon name={q.is_dir ? 'folder' : 'file'} size="1rem" style="opacity:.7" />
          <span class="flex-1 min-w-0 text-sm mono truncate selectable" title={q.path}>{q.path}</span>
          {#if q.is_dir}<span class="tag">{t('common.nFiles', { n: num(q.files) })}</span>{/if}
          <span class="mono text-xs w-20 text-right">{fmt(q.size)}</span>
          <button class="btn btn-ghost btn-xs btn-square" disabled={busy} onclick={() => (queue = queue.filter(x => x.path !== q.path))} aria-label="remove"><Icon name="x" size=".85rem" /></button>
        </div>
      {/each}
    </div>
  </section>
{/if}
