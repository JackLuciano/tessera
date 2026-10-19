<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type DupGroup } from '../api';
  import { t } from '../i18n.svelte';
  import { confirmDelete } from '../confirm.svelte';
  import { reveal } from '../actions';
  import Icon from '../Icon.svelte';

  type Strategy = 'newest' | 'oldest' | 'shortest';
  let groups = $state<DupGroup[] | null>(null);
  let running = $state(false);
  let prog = $state({ hashed: 0, total: 0 });
  /** Index of the copy to keep, per group. */
  let keep = $state<number[]>([]);
  let strategy = $state<Strategy>('newest');
  let scannedVersion = -1;

  $effect(() => {
    const un = listen<{ hashed: number; total: number }>('dup-progress', e => (prog = e.payload));
    return () => { un.then(f => f()); };
  });
  // Results belong to one scan; a rescan or delete invalidates them
  $effect(() => { if (app.version !== scannedVersion) groups = null; });

  async function run() {
    running = true; prog = { hashed: 0, total: 0 };
    try {
      groups = await api.duplicates();
      scannedVersion = app.version;
      apply(strategy);
    } catch (e) {
      if (String(e) !== 'cancelled') app.toast(String(e), 'error');
    } finally { running = false; }
  }
  function apply(s: Strategy) {
    strategy = s;
    keep = (groups ?? []).map(g => {
      const idx = g.files.map((_, i) => i);
      if (s === 'newest') return idx.reduce((b, i) => (g.files[i].modified > g.files[b].modified ? i : b), 0);
      if (s === 'oldest') return idx.reduce((b, i) => (g.files[i].modified < g.files[b].modified ? i : b), 0);
      return idx.reduce((b, i) => (g.files[i].path.length < g.files[b].path.length ? i : b), 0);
    });
  }
  let wasted = $derived((groups ?? []).reduce((s, g) => s + g.wasted, 0));
  let toRemove = $derived((groups ?? []).flatMap((g, gi) => g.files.filter((_, i) => i !== keep[gi]).map(f => ({ id: f.id, name: f.path, size: g.size }))));

  function removeAll(ev: MouseEvent) {
    confirmDelete.ask(toRemove, ev, () => { groups = null; });
  }

  let linkDlg: HTMLDialogElement;
  let linking = $state(false);
  /** Replace every non-kept copy with a hard link to the kept one: paths stay, space is freed. */
  async function linkAll() {
    if (!groups) return;
    linking = true;
    const pairs = groups.flatMap((g, gi) => g.files.filter((_, i) => i !== keep[gi]).map(f => [g.files[keep[gi]].id, f.id] as [number, number]));
    try {
      const res = await api.hardlinkDupes(pairs);
      linkDlg.close();
      app.toast(t('dupes.linked', { n: res.deleted, size: fmt(res.freed) }), 'success');
      if (res.failed.length) app.toast(t('dupes.linkFailed', { n: res.failed.length, first: res.failed[0] }), 'warning');
      await app.refreshDrives();
      run();
    } catch (e) {
      app.toast(String(e), 'error');
    } finally { linking = false; }
  }
</script>

{#if !app.summary}
  <div class="glass p-10 text-center muted">{t('common.scanFirst')}</div>
{:else}
  <section class="grid grid-cols-3 gap-4">
    <div class="glass p-4 reveal"><div class="text-xs muted">{t('dupes.wasted')}</div><div class="text-2xl font-bold mono grad-text">{groups ? fmt(wasted) : '—'}</div></div>
    <div class="glass p-4 reveal" style="--i:1"><div class="text-xs muted">{t('dupes.groups')}</div><div class="text-2xl font-bold mono">{groups ? groups.length : '—'}</div></div>
    <div class="glass p-4 reveal" style="--i:2"><div class="text-xs muted">{t('dupes.verified')}</div><div class="text-2xl font-bold">BLAKE3</div></div>
  </section>

  <section class="glass p-4 reveal" style="--i:3">
    {#if running}
      <div class="py-10 flex flex-col items-center gap-3">
        <div class="scan-orb" style="width:96px;height:96px"></div>
        <div class="font-semibold">{t('dupes.hashing')}</div>
        <div class="text-xs muted mono">{prog.total ? `${fmt(prog.hashed)} / ${fmt(prog.total)}` : t('dupes.grouping')}</div>
        {#if prog.total}<progress class="progress progress-primary w-80" value={prog.hashed} max={prog.total}></progress>{/if}
        <button class="btn btn-ghost btn-sm" onclick={() => api.cancelDuplicates()}>{t('common.cancel')}</button>
      </div>
    {:else if !groups}
      <div class="py-10 flex flex-col items-center gap-3 text-center">
        <Icon name="copy" size="2.5rem" style="color:var(--color-primary)" />
        <div class="font-semibold text-lg">{t('dupes.title')}</div>
        <p class="muted text-sm max-w-md">{t('dupes.explain')}</p>
        <button class="btn btn-primary shine" onclick={run}>{t('dupes.find')}</button>
      </div>
    {:else}
      <div class="flex items-center gap-2 flex-wrap mb-3">
        <div class="font-semibold">{t('dupes.groupsTitle')}</div>
        <div class="ml-auto flex items-center gap-2">
          <span class="text-xs muted">{t('dupes.autoSelect')}</span>
          <select class="select select-xs w-48" value={strategy} onchange={e => apply((e.target as HTMLSelectElement).value as Strategy)}>
            <option value="newest">{t('dupes.keepNewest')}</option><option value="oldest">{t('dupes.keepOldest')}</option><option value="shortest">{t('dupes.keepShortest')}</option>
          </select>
          <button class="btn btn-ghost btn-xs" onclick={run}><Icon name="refresh" size=".9rem" /></button>
          <button class="btn btn-xs btn-soft btn-secondary" disabled={!toRemove.length} title={t('dupes.linkHint')} onclick={() => linkDlg.showModal()}><Icon name="copy" size=".85rem" />{t('dupes.link')}</button>
          <button class="btn btn-xs shine {app.permanentDelete ? 'btn-error' : 'btn-primary'}" disabled={!toRemove.length} onclick={removeAll}>{t('dupes.remove', { size: fmt(wasted) })}</button>
        </div>
      </div>
      {#if !groups.length}<div class="muted text-sm p-10 text-center">{t('dupes.none')}</div>{/if}
      <div class="flex flex-col gap-2">
        {#each groups.slice(0, 300) as g, gi (gi)}
          <details class="rounded-[.9rem] border hairline overflow-hidden" open={gi < 3}>
            <summary class="flex items-center gap-3 p-3 cursor-pointer list-none">
              <Icon name="chev" size=".9rem" />
              <div class="flex-1 min-w-0"><div class="font-medium truncate">{g.files[0].path.split(/[\\/]/).pop()}</div><div class="text-[11px] muted">{t('dupes.copies', { n: g.files.length, size: fmt(g.size) })}</div></div>
              <span class="mono text-sm font-semibold" style="color:var(--color-warning)">{fmt(g.wasted)}</span>
            </summary>
            <div class="px-3 pb-3 flex flex-col gap-1">
              {#each g.files as f, i (f.id)}
                <label class="row-hover flex items-center gap-3 px-2 py-1.5 cursor-pointer">
                  <input type="radio" name="dup{gi}" class="radio radio-primary radio-xs" checked={keep[gi] === i} onchange={() => (keep[gi] = i)} />
                  <span class="flex-1 mono text-xs truncate selectable">{f.path}</span>
                  <span class="text-[11px] muted">{ago(f.modified)}</span>
                  <button class="btn btn-ghost btn-xs btn-square" onclick={e => { e.preventDefault(); reveal(f.path); }}><Icon name="folder" size=".85rem" /></button>
                  {#if keep[gi] === i}<span class="badge badge-soft badge-success badge-xs">{t('dupes.keep')}</span>{:else}<span class="badge badge-soft badge-xs {app.permanentDelete ? 'badge-error' : 'badge-warning'}">{t('dupes.removeOne')}</span>{/if}
                </label>
              {/each}
            </div>
          </details>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<dialog bind:this={linkDlg} class="modal">
  <div class="modal-box glass max-w-md">
    <div class="flex items-center gap-3">
      <span class="w-10 h-10 rounded-xl grid place-items-center" style="background:color-mix(in oklch,var(--color-secondary) 16%,transparent);color:var(--color-secondary)"><Icon name="copy" /></span>
      <h3 class="text-lg font-semibold">{t('dupes.linkTitle', { n: toRemove.length })}</h3>
    </div>
    <p class="text-sm muted mt-3">{t('dupes.linkText', { size: fmt(wasted) })}</p>
    <p class="text-xs muted mt-2">{t('dupes.linkWarn')}</p>
    <div class="modal-action">
      <button class="btn btn-ghost" onclick={() => linkDlg.close()}>{t('common.cancel')}</button>
      <button class="btn btn-secondary" disabled={linking} onclick={linkAll}>{#if linking}<span class="loading loading-spinner loading-sm"></span>{/if}{t('dupes.link')}</button>
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
