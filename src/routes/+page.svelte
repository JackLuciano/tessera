<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, IS_WINDOWS, type View } from '$lib/store.svelte';
  import { ago } from '$lib/api';
  import Optimizer from '$lib/views/Optimizer.svelte';
  import { api, fmt, num } from '$lib/api';
  import { t, i18n } from '$lib/i18n.svelte';
  import Icon from '$lib/Icon.svelte';
  import Logo from '$lib/Logo.svelte';
  import ConfirmDialog from '$lib/ConfirmDialog.svelte';
  import ContextMenu from '$lib/ContextMenu.svelte';
  import Overview from '$lib/views/Overview.svelte';
  import Explorer from '$lib/views/Explorer.svelte';
  import Cleanup from '$lib/views/Cleanup.svelte';
  import Files from '$lib/views/Files.svelte';
  import Settings from '$lib/views/Settings.svelte';
  import Duplicates from '$lib/views/Duplicates.svelte';
  import History from '$lib/views/History.svelte';
  import Palette from '$lib/Palette.svelte';
  import UpdateDialog from '$lib/UpdateDialog.svelte';
  import { updater } from '$lib/update.svelte';

  const NAV: { v: View; ico: string }[] = [
    { v: 'overview', ico: 'overview' }, { v: 'explorer', ico: 'explorer' }, { v: 'cleanup', ico: 'cleanup' },
    { v: 'files', ico: 'files' }, { v: 'dupes', ico: 'copy' }, { v: 'history', ico: 'history' },
    ...(IS_WINDOWS ? [{ v: 'optimizer' as View, ico: 'gauge' }] : []),
    { v: 'settings', ico: 'settings' },
  ];

  let scanDlg: HTMLDialogElement;
  /** Estimated scan progress 0..0.95, or null when unknown (folder scan, or past the estimate). */
  let scanPct = $derived.by(() => {
    const d = app.drive;
    const used = d ? d.total - d.available : 0;
    // Only a whole-drive scan can be compared with the drive's used space
    if (!d || !used || app.target.replace(/[\\/]+$/, '').toUpperCase() !== d.mount.replace(/[\\/]+$/, '').toUpperCase()) return null;
    const p = app.progress.bytes / used;
    return p >= 0.95 ? null : p;
  });
  $effect(() => { if (app.scanning && !scanDlg.open) scanDlg.showModal(); else if (!app.scanning && scanDlg.open) scanDlg.close(); });

  onMount(() => {
    app.init().then(() => {
      // Dev preview only (`pnpm dev` + mock backend): ?view=explorer&theme=light&lang=en opens a view after a fake scan
      if (!import.meta.env.DEV) return;
      const q = new URLSearchParams(location.search);
      if (q.get('lang')) i18n.setLocale(q.get('lang')!);
      if (q.get('theme')) app.setMode(q.get('theme') as 'light' | 'dark');
      if (q.get('anim') === '0') document.documentElement.classList.add('no-anim');
      if (q.get('view')) app.scan('C:\\').then(() => (app.view = q.get('view') as View));
    });
    // Check GitHub Releases shortly after launch; downloads in the background if auto-update is on
    const t = setTimeout(() => updater.check(true), 3000);
    return () => clearTimeout(t);
  });

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === 'string') app.scan(dir);
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'F5') { e.preventDefault(); app.scan(); }
    const typing = /INPUT|TEXTAREA|SELECT/.test((document.activeElement as HTMLElement)?.tagName ?? '');
    if (!typing && !document.querySelector('dialog[open]') && /^[1-9]$/.test(e.key) && NAV[+e.key - 1]) app.view = NAV[+e.key - 1].v;
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="bg-blobs"><span></span><span></span><span></span></div>

<div class="relative z-10 flex min-h-screen">
  <!-- Below 1024 px the sidebar collapses to an icon rail so the content keeps its room -->
  <aside class="flex flex-col w-[84px] lg:w-64 shrink-0 p-3 lg:p-4 gap-4 sticky top-0 h-screen">
    <div class="glass flex-1 flex flex-col p-2 lg:p-4 gap-6 overflow-y-auto overflow-x-hidden">
      <div class="flex items-center gap-3 px-1 max-lg:justify-center max-lg:pt-2">
        <Logo size={40} />
        <div class="min-w-0 max-lg:hidden">
          <div class="font-bold text-lg leading-tight tracking-tight">Tessera</div>
          <div class="text-xs muted truncate">{t('app.tagline')}</div>
        </div>
      </div>

      <nav class="flex flex-col gap-1 lg:pl-2 stagger">
        {#each NAV as n (n.v)}
          <button class="nav-item max-lg:justify-center {app.view === n.v ? 'active' : ''}" title={t('nav.' + n.v)} onclick={() => (app.view = n.v)}>
            <Icon name={n.ico} /><span class="max-lg:hidden">{t('nav.' + n.v)}</span>
          </button>
        {/each}
      </nav>

      <div>
        <div class="flex items-center justify-between px-1 mb-2 max-lg:hidden">
          <div class="text-xs uppercase tracking-wider muted font-semibold">{t('nav.drives')}</div>
          <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={() => app.refreshDrives()}><Icon name="refresh" size=".9rem" /></button>
        </div>
        <div class="flex flex-col gap-2">
          {#each app.drives as d (d.mount)}
            {@const used = d.total - d.available}
            {@const p = (used / d.total) * 100}
            {@const active = app.drive?.mount === d.mount}
            {@const label = `${d.name || t('drive.local')} (${d.mount.replace(/\\$/, '')})`}
            <button class="rounded-xl p-2.5 text-left transition min-w-0 {active ? '' : 'opacity-75 hover:opacity-100'}"
              style={active ? 'background:color-mix(in oklch,var(--color-primary) 10%,transparent)' : ''}
              title={`${label} · ${t('drive.freeOf', { free: fmt(d.available), total: fmt(d.total) })}`}
              onclick={() => app.openDrive(d.mount)} disabled={app.scanning}>
              <div class="flex items-center gap-2 text-sm font-medium max-lg:justify-center">
                <Icon name={d.removable ? 'usb' : 'drive'} size="1rem" />
                <span class="truncate max-lg:hidden">{label}</span>
                <span class="lg:hidden text-xs mono">{d.mount.replace(/[:\\/]+$/, '')}</span>
              </div>
              <div class="bar mt-2"><i style="width:{p}%;background:{p > 90 ? 'var(--color-warning)' : 'linear-gradient(90deg,var(--color-primary),var(--color-accent))'}"></i></div>
              <div class="text-[11px] muted mt-1 mono truncate max-lg:hidden">{t('drive.freeOf', { free: fmt(d.available), total: fmt(d.total) })}</div>
            </button>
          {/each}
        </div>
      </div>

      <div class="mt-auto rounded-xl p-3 soft text-xs max-lg:hidden">
        {#if app.elevated}
          <div class="flex items-center gap-2 text-sm font-medium"><span class="w-2 h-2 rounded-full" style="background:var(--color-success)"></span> {t('engine.mft')}</div>
          <div class="muted mt-1">{t('engine.mftHint')}</div>
        {:else}
          <div class="flex items-center gap-2 text-sm font-medium"><span class="w-2 h-2 rounded-full" style="background:var(--color-warning)"></span> {t('engine.parallel')}</div>
          <div class="muted mt-1">{t('engine.hint')}</div>
          <button class="btn btn-xs btn-ghost mt-2 -ml-2 gap-1" onclick={() => api.relaunchAsAdmin().catch(e => app.toast(String(e), 'info'))}><Icon name="shield" size=".85rem" />{t('engine.elevate')}</button>
        {/if}
      </div>
    </div>
  </aside>

  <main class="flex-1 min-w-0 p-4 pl-0 flex flex-col gap-4">
    <header class="glass flex items-center gap-3 px-4 py-3 sticky top-4 z-30">
      <div class="min-w-0">
        <div class="text-xs muted truncate">{app.summary ? app.summary.root : t('header.noScan')}</div>
        <div class="font-semibold truncate">{t('nav.' + app.view)}</div>
      </div>
      {#if app.summary && app.catchingUp}
        <span class="chip-soft"><span class="loading loading-spinner loading-xs"></span>{t('cache.catchingUp')}</span>
      {:else if app.live && app.summary}
        <span class="live-pill" title={t('live.hint', { n: num(app.liveFolders) })}><i></i>{t('live.on')}</span>
      {/if}
      {#if app.summary && app.cachedAt}
        <span class="chip-soft" title={`${t('cache.saved', { when: ago(app.cachedAt, true) })} — ${t('cache.savedHint')}`}><Icon name="history" size=".8rem" /><span class="max-2xl:hidden">{t('cache.saved', { when: ago(app.cachedAt, true) })}</span></span>
      {/if}
      <!-- Button labels give way to icons as the window narrows, so the page title never gets squeezed -->
      <div class="ml-auto flex items-center gap-2 shrink-0">
        <button class="btn btn-ghost btn-sm gap-3 font-normal muted border hairline" title="{t('palette.short')} (Ctrl+K)" onclick={() => (app.paletteOpen = true)}>
          <Icon name="search" size="1rem" /><span class="max-xl:hidden">{t('palette.short')}</span><span class="flex gap-1 max-xl:hidden"><kbd class="kbd kbd-xs">Ctrl</kbd><kbd class="kbd kbd-xs">K</kbd></span>
        </button>
        {#if updater.state === 'available' || updater.state === 'downloading' || updater.state === 'ready'}
          <button class="btn btn-sm btn-soft btn-success gap-1.5 shine" onclick={() => (updater.modal = true)}>
            {#if updater.state === 'downloading'}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name={updater.state === 'ready' ? 'refresh' : 'download'} size=".95rem" />{/if}
            {updater.state === 'ready' ? t('update.restartShort') : t('update.pill', { version: updater.version })}
          </button>
        {/if}
        <button class="btn btn-ghost btn-sm gap-2" title={t('action.scanFolderPick')} onclick={pickFolder} disabled={app.scanning}><Icon name="folder" size="1rem" /><span class="max-xl:hidden">{t('action.scanFolderPick')}</span></button>
        <button class="btn btn-ghost btn-square btn-sm" title={t('settings.theme')}
          onclick={() => app.setMode(document.documentElement.dataset.theme === 'tessera-dark' ? 'light' : 'dark')}>
          <Icon name={app.mode === 'light' ? 'moon' : 'sun'} />
        </button>
        <button class="btn btn-primary btn-sm shine gap-2" title={app.summary ? t('action.rescan') : t('action.scan')} onclick={() => app.scan()} disabled={app.scanning || !app.target}>
          <Icon name="refresh" /><span class="max-lg:hidden">{app.summary ? t('action.rescan') : t('action.scan')}</span>
        </button>
      </div>
    </header>

    {#key app.view}
      <div class="view-enter flex flex-col gap-4">
        {#if app.view === 'overview'}<Overview />
        {:else if app.view === 'explorer'}<Explorer />
        {:else if app.view === 'cleanup'}<Cleanup />
        {:else if app.view === 'files'}<Files />
        {:else if app.view === 'dupes'}<Duplicates />
        {:else if app.view === 'history'}<History />
        {:else if app.view === 'optimizer'}<Optimizer />
        {:else}<Settings />{/if}
      </div>
    {/key}
  </main>
</div>

<dialog bind:this={scanDlg} class="modal" oncancel={e => e.preventDefault()}>
  <div class="modal-box glass text-center max-w-md">
    <div class="scan-orb mb-5"><span class="dot"></span></div>
    <div class="text-lg font-semibold">{t('scan.title', { target: app.target })}</div>
    <div class="text-xs muted mb-4">{app.elevated ? t('scan.subMft') : t('scan.sub')}</div>
    <div class="grid grid-cols-3 gap-2 mb-4">
      <div class="tile-stat"><div>{t('stat.files')}</div><div>{num(app.progress.files)}</div></div>
      <div class="tile-stat"><div>{t('stat.folders')}</div><div>{num(app.progress.dirs)}</div></div>
      <div class="tile-stat"><div>{t('scan.indexed')}</div><div>{fmt(app.progress.bytes)}</div></div>
    </div>
    <!-- Bytes found vs. used space is only an estimate (hard links and compressed files make the sum
         exceed it), so the bar never claims "done" and turns into a flowing bar past 95% -->
    <div class="progress-flow {scanPct === null ? 'indeterminate' : ''}"><i style="width:{(scanPct ?? 0) * 100}%"></i></div>
    {#if scanPct !== null}<div class="text-[11px] muted mono mt-1.5">~{Math.round(scanPct * 100)}%</div>{/if}
    <button class="btn btn-ghost btn-sm mt-3" onclick={() => api.cancel()}>{t('common.cancel')}</button>
  </div>
</dialog>

<ConfirmDialog />
<ContextMenu />
<Palette />
<UpdateDialog />

<div class="toast toast-end toast-bottom z-[60]">
  {#each app.toasts as tt (tt.id)}
    <div class="alert alert-{tt.kind} alert-soft shadow-lg glass max-w-md"><span class="selectable">{tt.msg}</span></div>
  {/each}
</div>
