<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, type Program, type Leftover } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { rpToast, failures, day } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let programs = $state<Program[] | null>(null);
  let q = $state('');
  let sort = $state<'name' | 'size' | 'date'>('name');
  let busy = $state<string | null>(null);
  /** Leftovers of the program just uninstalled, for review. */
  let left = $state<{ program: Program; items: Leftover[]; on: Set<string> } | null>(null);
  let leftDlg: HTMLDialogElement;

  async function load() {
    programs = await api.programs().catch(e => { app.toast(String(e), 'error'); return []; });
  }
  onMount(load);

  let list = $derived.by(() => {
    const f = (programs ?? []).filter(p => !q || `${p.name} ${p.publisher}`.toLowerCase().includes(q.toLowerCase()));
    if (sort === 'size') return [...f].sort((a, b) => b.size - a.size);
    if (sort === 'date') return [...f].sort((a, b) => b.installed - a.installed);
    return f;
  });
  let totalSize = $derived((programs ?? []).reduce((s, p) => s + p.size, 0));

  const HUES = [285, 200, 345, 150, 45, 255, 175, 80];
  const hue = (s: string) => HUES[[...s].reduce((h, c) => h + c.charCodeAt(0), 0) % HUES.length];

  async function uninstall(p: Program) {
    const ok = await ask({ title: t('uninst.confirm', { name: p.name }), text: t('uninst.confirmText'), ico: 'package', confirm: t('uninst.go'), danger: true });
    if (!ok) return;
    busy = p.id;
    try {
      const gone = await api.uninstall(p.id);
      app.toast(gone ? t('uninst.done', { name: p.name }) : t('uninst.maybe', { name: p.name }), gone ? 'success' : 'info');
      const items = await api.leftovers(p);
      if (items.length) {
        left = { program: p, items, on: new Set(items.map(i => i.path)) };
        leftDlg.showModal();
      }
      await load();
    } catch (e) {
      app.toast(String(e) === 'cancelled' ? t('uninst.cancelled') : String(e), String(e) === 'cancelled' ? 'info' : 'error');
    } finally { busy = null; }
  }

  async function removeLeft() {
    if (!left) return;
    const picked = left.items.filter(i => left!.on.has(i.path));
    busy = 'left';
    try {
      const out = await api.removeLeftovers(picked);
      app.toast(t('uninst.leftRemoved', { n: out.removed, size: fmt(out.freed) }), 'success');
      rpToast(out.restore_point);
      failures(out.failed);
      leftDlg.close();
      left = null;
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  function flip(path: string) { if (!left) return; const s = new Set(left.on); s.has(path) ? s.delete(path) : s.add(path); left = { ...left, on: s }; }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="package" title={t('uninst.title')} sub={t('uninst.sub')}>
    <label class="input input-sm w-56 flex items-center gap-2"><Icon name="search" size=".9rem" /><input bind:value={q} placeholder={t('files.filter')} /></label>
    <select class="select select-sm w-36" bind:value={sort}>
      <option value="name">{t('uninst.sort.name')}</option><option value="size">{t('uninst.sort.size')}</option><option value="date">{t('uninst.sort.date')}</option>
    </select>
  </PageHead>
  {#if programs}
    <div class="grid grid-cols-2 gap-2 mt-4 max-w-sm">
      <div class="tile-stat"><div>{t('uninst.count')}</div><div>{programs.length}</div></div>
      <div class="tile-stat"><div>{t('uninst.reported')}</div><div>{fmt(totalSize)}</div></div>
    </div>
  {/if}
</section>

<section class="glass p-3 reveal" style="--i:1">
  {#if programs === null}
    <div class="p-8 text-center"><span class="loading loading-dots"></span></div>
  {:else}
    <div class="flex flex-col divide-y divide-[var(--glass-border)]">
      {#each list as p (p.id)}
        <div class="flex items-center gap-3 py-2.5 px-2">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 font-bold text-sm text-white" style="background:oklch(62% 0.15 {hue(p.name)})">{p.name.slice(0, 1).toUpperCase()}</span>
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{p.name}</div>
            <div class="text-[11px] muted truncate">{[p.publisher, p.version].filter(Boolean).join(' · ') || '—'}</div>
          </div>
          <span class="text-xs muted w-28 text-right max-md:hidden">{day(p.installed)}</span>
          <span class="mono text-xs w-20 text-right">{p.size ? fmt(p.size) : '—'}</span>
          <button class="btn btn-ghost btn-sm gap-1.5" disabled={!!busy} onclick={() => uninstall(p)}>
            {#if busy === p.id}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="trash" size=".95rem" />{/if}
            <span class="max-lg:hidden">{busy === p.id ? t('uninst.waiting') : t('uninst.go')}</span>
          </button>
        </div>
      {:else}
        <div class="p-8 text-center muted text-sm">—</div>
      {/each}
    </div>
  {/if}
</section>

<dialog bind:this={leftDlg} class="modal">
  {#if left}
    <div class="modal-box glass max-w-2xl">
      <div class="flex items-center gap-3">
        <span class="w-10 h-10 rounded-xl grid place-items-center soft" style="color:var(--color-primary)"><Icon name="search" /></span>
        <div><h3 class="text-lg font-semibold">{t('uninst.leftTitle')}</h3><div class="text-xs muted">{t('uninst.leftSub', { name: left.program.name })}</div></div>
      </div>
      <div class="flex flex-col mt-4 max-h-72 overflow-y-auto pr-1">
        {#each left.items as i (i.path)}
          <label class="row-hover flex items-center gap-3 px-2 py-1.5 cursor-pointer">
            <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={left.on.has(i.path)} onchange={() => flip(i.path)} />
            <Icon name={i.kind === 'folder' ? 'folder' : 'registry'} size="1rem" style="opacity:.7" />
            <span class="flex-1 min-w-0 text-sm mono truncate" title={i.path}>{i.path}</span>
            {#if i.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" /></span>{/if}
            <span class="mono text-xs w-16 text-right">{i.kind === 'folder' ? fmt(i.size) : ''}</span>
          </label>
        {/each}
      </div>
      <div class="rounded-xl soft p-3 mt-3 text-xs">{t('uninst.leftNote')}</div>
      <div class="modal-action">
        <button class="btn btn-ghost" onclick={() => leftDlg.close()}>{t('uninst.keep')}</button>
        <button class="btn btn-primary" disabled={!left.on.size || busy === 'left'} onclick={removeLeft}>{#if busy === 'left'}<span class="loading loading-spinner loading-sm"></span>{/if}{t('uninst.leftRemove', { n: left.on.size })}</button>
      </div>
    </div>
  {/if}
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
