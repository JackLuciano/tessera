<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, type Driver, type DriverStore } from '../api';
  import { ask } from '../ask.svelte';
  import AdminBanner from '../AdminBanner.svelte';
  import { t } from '../i18n.svelte';
  import { day, openSystem } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let list = $state<Driver[] | null>(null);
  onMount(() => { api.drivers().then(v => (list = v)).catch(e => { app.toast(String(e), 'error'); list = []; }); });

  // ---- driver store: old packages Windows keeps after every driver update ----
  let store = $state<DriverStore | null>(null);
  let scanning = $state(false);
  let removing = $state(false);
  let showAll = $state(false);
  async function scanStore() {
    scanning = true;
    try { store = await api.driverStore(); } catch (e) { app.toast(String(e), 'error'); } finally { scanning = false; }
  }
  async function cleanStore() {
    const old = store?.packages.filter(p => p.removable) ?? [];
    const ok = await ask({ title: t('drv.storeConfirm', { n: old.length }), text: t('drv.storeConfirmText'), ico: 'trash', confirm: t('drv.storeGo', { size: fmt(store?.removable_bytes ?? 0) }), items: old.map(p => ({ name: `${p.provider} · ${p.original}`, detail: `${p.version} · ${fmt(p.size)}` })) });
    if (!ok) return;
    removing = true;
    try {
      const r = await api.driverStoreRemove(old.map(p => p.inf));
      const freed = old.filter(p => r.removed.includes(p.inf)).reduce((s, p) => s + p.size, 0);
      app.toast(t('drv.storeDone', { n: r.removed.length, size: fmt(freed) }), r.failed.length ? 'warning' : 'success');
      await scanStore();
    } catch (e) { app.toast(String(e), 'error'); } finally { removing = false; }
  }
  let shown = $derived(store ? (showAll ? store.packages : store.packages.filter(p => p.removable)) : []);

  const age = (d: Driver) => (d.date ? Math.floor((Date.now() / 1000 - d.date) / 86400) : null);
  const VENDOR: Record<string, string> = { nvidia: 'oklch(70% 0.19 140)', amd: 'oklch(62% 0.22 25)', intel: 'oklch(62% 0.15 245)', other: 'var(--color-primary)' };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="cpu" title={t('drv.title')} sub={t('drv.sub')}>
    <button class="btn btn-sm gap-1.5" onclick={() => openSystem('ms-settings:windowsupdate-optionalupdates')}><Icon name="download" size=".9rem" />{t('drv.optional')}</button>
  </PageHead>
</section>

{#if list === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else}
  <section class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each list as d, i (d.name + d.version)}
      {@const a = age(d)}
      {@const old = a !== null && a > 120}
      <div class="glass p-5 reveal" style="--i:{i + 1}">
        <div class="flex items-center gap-3">
          <span class="w-11 h-11 rounded-xl grid place-items-center text-white font-bold shrink-0" style="background:{VENDOR[d.vendor]}">{d.vendor === 'other' ? 'GPU' : d.vendor.toUpperCase().slice(0, 3)}</span>
          <div class="flex-1 min-w-0">
            <div class="font-semibold truncate">{d.name}</div>
            <div class="text-[11px] muted">{t('drv.graphics')}</div>
          </div>
          <span class="text-xs font-semibold flex items-center gap-1.5" style="color:var(--color-{old ? 'warning' : 'success'})"><i class={old ? 'dot-warn' : 'dot-ok'}></i>{old ? t('drv.old') : t('drv.recent')}</span>
        </div>
        <div class="grid grid-cols-3 gap-3 mt-4">
          <div class="metric"><b class="!text-lg">{d.display_version}</b><span>{t('drv.version')}</span></div>
          <div class="metric"><b class="!text-lg">{day(d.date ?? 0)}</b><span>{t('drv.date')}</span></div>
          <div class="metric"><b class="!text-lg">{a !== null ? t('drv.days', { n: a }) : '—'}</b><span>{t('drv.age')}</span></div>
        </div>
        {#if old}<div class="text-xs mt-3" style="color:var(--color-warning)">{t('drv.oldHint', { n: a ?? 0 })}</div>{/if}
        <div class="flex justify-end mt-3">
          <button class="btn btn-sm {old ? 'btn-primary' : 'btn-ghost'} gap-1.5" onclick={() => openSystem(d.download)}>{t('drv.get')}<Icon name="external" size=".8rem" /></button>
        </div>
      </div>
    {:else}
      <div class="glass p-10 text-center muted">{t('drv.none')}</div>
    {/each}
  </section>
{/if}

<section class="glass p-5 reveal" style="--i:4">
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <div class="flex items-center gap-3">
      <span class="status-ico" style="background:color-mix(in oklch,var(--color-primary) 14%,transparent);color:var(--color-primary)"><Icon name="archive" size="1.05rem" /></span>
      <div><div class="font-semibold">{t('drv.store')}</div><div class="text-xs muted">{t('drv.storeSub')}</div></div>
    </div>
    <div class="flex gap-2">
      {#if store && !store.needs_admin}
        <button class="btn btn-sm btn-ghost border hairline" onclick={() => (showAll = !showAll)}>{showAll ? t('drv.storeOld') : t('drv.storeAll', { n: store.packages.length })}</button>
      {/if}
      {#if store?.removable_bytes}
        <button class="btn btn-sm btn-primary gap-1.5" disabled={removing} onclick={cleanStore}>
          {#if removing}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="trash" size=".9rem" />{/if}{t('drv.storeGo', { size: fmt(store.removable_bytes) })}
        </button>
      {:else}
        <button class="btn btn-sm gap-1.5" disabled={scanning} onclick={scanStore}>
          {#if scanning}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="search" size=".9rem" />{/if}{store ? t('action.refresh') : t('drv.storeScan')}
        </button>
      {/if}
    </div>
  </div>
  {#if store?.needs_admin}
    <div class="mt-3"><AdminBanner text={t('drv.storeAdmin')} /></div>
  {:else if store}
    {#if !shown.length}
      <div class="flex items-center gap-2 text-sm mt-4" style="color:var(--color-success)"><i class="dot-ok"></i>{t('drv.storeClean')}</div>
    {:else}
      <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)] max-h-96 overflow-auto">
        {#each shown as p (p.inf)}
          <div class="flex items-center gap-3 py-2">
            <div class="flex-1 min-w-0">
              <div class="text-sm truncate">{p.provider || '—'} · <span class="mono text-xs">{p.original}</span></div>
              <div class="text-[11px] muted">{p.class} · {p.version} · {day(p.date)}</div>
            </div>
            {#if p.removable}<span class="badge badge-sm badge-warning badge-soft">{t('drv.pkgOld')}</span>
            {:else if p.in_use}<span class="badge badge-sm badge-success badge-soft">{t('drv.pkgUsed')}</span>
            {:else}<span class="badge badge-sm badge-ghost">{t('drv.pkgKept')}</span>{/if}
            <span class="mono text-xs w-20 text-right">{fmt(p.size)}</span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</section>
