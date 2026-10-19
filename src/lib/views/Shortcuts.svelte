<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type BrokenShortcut, type EmptyFolder } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { confirmDelete } from '../confirm.svelte';
  import { failures } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  const LOC_ICO: Record<string, string> = { desktop: 'overview', start_menu: 'app', taskbar: 'layers', applications: 'app' };

  let links = $state<BrokenShortcut[] | null>(null);
  let linkOn = $state(new Set<string>());
  let empty = $state<EmptyFolder[] | null>(null);
  let emptyOn = $state(new Set<number>());
  let busy = $state(false);

  async function loadLinks() {
    links = null;
    const l = await api.shortcuts().catch(e => { app.toast(String(e), 'error'); return []; });
    links = l;
    linkOn = new Set(l.filter(x => !x.admin || app.elevated).map(x => x.path));
  }
  onMount(loadLinks);

  // Empty folders come from the scanned tree; refetch when it changes
  $effect(() => {
    void app.version;
    if (!untrack(() => app.summary)) { empty = []; return; }
    api.emptyFolders().then(f => { empty = f; emptyOn = new Set(f.map(x => x.id)); }).catch(() => (empty = []));
  });

  function flip<T>(s: Set<T>, v: T) { const n = new Set(s); n.has(v) ? n.delete(v) : n.add(v); return n; }

  async function fixLinks() {
    const picked = (links ?? []).filter(l => linkOn.has(l.path));
    const ok = await ask({
      title: t('shortcuts.confirm', { n: picked.length }), text: t('shortcuts.confirmText'), ico: 'unlink', confirm: t('shortcuts.remove'),
      items: picked.map(l => ({ name: l.name, detail: t('shortcuts.loc.' + l.location) })),
    });
    if (!ok) return;
    busy = true;
    try {
      const out = await api.fixShortcuts(picked.map(l => l.path));
      app.toast(t('shortcuts.removed', { n: out.fixed }), 'success');
      failures(out.failed);
      await loadLinks();
    } finally { busy = false; }
  }

  function removeEmpty(ev: MouseEvent) {
    const picked = (empty ?? []).filter(f => emptyOn.has(f.id));
    confirmDelete.ask(picked.map(f => ({ id: f.id, name: f.path, size: 0 })), ev, () => {
      empty = (empty ?? []).filter(f => !emptyOn.has(f.id));
    });
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="unlink" title={t('shortcuts.title')} sub={t('shortcuts.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={loadLinks} disabled={busy}><Icon name="refresh" size="1rem" /></button>
    <button class="btn btn-primary btn-sm gap-2" disabled={!linkOn.size || busy} onclick={fixLinks}>
      {#if busy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="trash" size="1rem" />{/if}
      {t('shortcuts.removeN', { n: linkOn.size })}
    </button>
  </PageHead>
  {#if links === null}
    <div class="p-6 text-center"><span class="loading loading-dots"></span></div>
  {:else if !links.length}
    <div class="p-6 text-center"><Icon name="check" size="1.6rem" style="color:var(--color-success)" /><div class="text-sm mt-1">{t('shortcuts.none')}</div></div>
  {:else}
    <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
      {#each links as l (l.path)}
        {@const lock = l.admin && !app.elevated}
        <label class="flex items-center gap-3 py-2 px-1 {lock ? 'opacity-50' : 'cursor-pointer'}">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={linkOn.has(l.path)} disabled={lock} onchange={() => (linkOn = flip(linkOn, l.path))} />
          <span class="w-8 h-8 rounded-lg grid place-items-center soft shrink-0"><Icon name={LOC_ICO[l.location] ?? 'file'} size=".95rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{l.name}</div>
            <div class="text-[11px] muted mono truncate selectable" title={l.target}>{t('shortcuts.target')}: {l.target}</div>
          </div>
          <span class="tag">{t('shortcuts.loc.' + l.location)}</span>
          {#if l.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" /></span>{/if}
        </label>
      {/each}
    </div>
  {/if}
</section>

<section class="glass p-5 reveal" style="--i:1">
  <div class="flex items-center gap-3 flex-wrap">
    <div class="flex-1 min-w-48">
      <div class="font-semibold flex items-center gap-2"><Icon name="folder" style="color:var(--color-primary)" />{t('emptyf.title')}</div>
      <div class="text-xs muted mt-0.5">{t('emptyf.sub')}</div>
    </div>
    {#if empty?.length}
      <button class="btn btn-ghost btn-xs" onclick={() => (emptyOn = new Set(empty?.map(f => f.id)))}>{t('common.selectAll')}</button>
      <button class="btn btn-ghost btn-xs" onclick={() => (emptyOn = new Set())}>{t('common.none')}</button>
      <button class="btn btn-sm gap-2 {app.permanentDelete ? 'btn-error' : 'btn-primary'}" disabled={!emptyOn.size} onclick={removeEmpty}><Icon name="trash" size="1rem" />{t('emptyf.removeN', { n: emptyOn.size })}</button>
    {/if}
  </div>
  {#if !app.summary}
    <div class="text-sm muted text-center p-6">{t('emptyf.needScan')}</div>
  {:else if empty === null}
    <div class="p-6 text-center"><span class="loading loading-dots"></span></div>
  {:else if !empty.length}
    <div class="text-sm muted text-center p-6">{t('emptyf.none')}</div>
  {:else}
    <div class="flex flex-col mt-3 max-h-[28rem] overflow-y-auto pr-1">
      {#each empty as f (f.id)}
        <label class="row-hover flex items-center gap-3 px-2 py-1.5 cursor-pointer">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-xs" checked={emptyOn.has(f.id)} onchange={() => (emptyOn = flip(emptyOn, f.id))} />
          <span class="flex-1 min-w-0 text-sm mono truncate selectable" title={f.path}>{f.path}</span>
          {#if f.nested}<span class="tag">{t('emptyf.nested', { n: f.nested })}</span>{/if}
        </label>
      {/each}
    </div>
  {/if}
</section>
