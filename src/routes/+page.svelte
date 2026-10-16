<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, type View } from '$lib/store.svelte';
  import { api, fmt, num } from '$lib/api';
  import { t } from '$lib/i18n.svelte';
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
    { v: 'files', ico: 'files' }, { v: 'dupes', ico: 'copy' }, { v: 'history', ico: 'history' }, { v: 'settings', ico: 'settings' },
  ];

  let scanDlg: HTMLDialogElement;
  $effect(() => { if (app.scanning && !scanDlg.open) scanDlg.showModal(); else if (!app.scanning && scanDlg.open) scanDlg.close(); });

  onMount(() => {
    app.init();
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
    if (!typing && !document.querySelector('dialog[open]') && /^[1-7]$/.test(e.key)) app.view = NAV[+e.key - 1].v;
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="bg-blobs"><span></span><span></span><span></span></div>

<div class="relative z-10 flex min-h-screen">
  <aside class="flex flex-col w-60 shrink-0 p-4 gap-4 sticky top-0 h-screen">
    <div class="glass flex-1 flex flex-col p-4 gap-6 overflow-y-auto">
      <div class="flex items-center gap-3 px-1">
        <Logo size={40} />
        <div>
          <div class="font-bold text-lg leading-tight tracking-tight">Tessera</div>
          <div class="text-xs muted">{t('app.tagline')}</div>
        </div>
      </div>

      <nav class="flex flex-col gap-1 pl-3">
        {#each NAV as n (n.v)}
          <button class="nav-item {app.view === n.v ? 'active' : ''}" onclick={() => (app.view = n.v)}>
            <Icon name={n.ico} /><span>{t('nav.' + n.v)}</span>
          </button>
        {/each}
      </nav>

      <div>
        <div class="flex items-center justify-between px-1 mb-2">
          <div class="text-xs uppercase tracking-wider muted font-semibold">{t('nav.drives')}</div>
          <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={() => app.refreshDrives()}><Icon name="refresh" size=".9rem" /></button>
        </div>
        <div class="flex flex-col gap-2">
          {#each app.drives as d (d.mount)}
            {@const used = d.total - d.available}
            {@const p = (used / d.total) * 100}
            {@const active = app.drive?.mount === d.mount}
            <button class="rounded-xl p-2.5 text-left transition {active ? '' : 'opacity-75 hover:opacity-100'}"
              style={active ? 'background:color-mix(in oklch,var(--color-primary) 10%,transparent)' : ''}
              onclick={() => app.scan(d.mount)} disabled={app.scanning}>
              <div class="flex items-center gap-2 text-sm font-medium"><Icon name={d.removable ? 'usb' : 'drive'} size="1rem" /><span class="truncate">{d.name || t('drive.local')} ({d.mount.replace(/\\$/, '')})</span></div>
              <div class="bar mt-2"><i style="width:{p}%;background:{p > 90 ? 'var(--color-warning)' : 'linear-gradient(90deg,var(--color-primary),var(--color-accent))'}"></i></div>
              <div class="text-[11px] muted mt-1 mono">{t('drive.freeOf', { free: fmt(d.available), total: fmt(d.total) })}</div>
            </button>
          {/each}
        </div>
      </div>

      <div class="mt-auto rounded-xl p-3 soft text-xs">
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
      <div class="ml-auto flex items-center gap-2">
        <button class="btn btn-ghost btn-sm gap-3 font-normal muted border hairline" onclick={() => (app.paletteOpen = true)}>
          <Icon name="search" size="1rem" />{t('palette.short')}<span class="flex gap-1"><kbd class="kbd kbd-xs">Ctrl</kbd><kbd class="kbd kbd-xs">K</kbd></span>
        </button>
        {#if updater.state === 'available' || updater.state === 'downloading' || updater.state === 'ready'}
          <button class="btn btn-sm btn-soft btn-success gap-1.5 shine" onclick={() => (updater.modal = true)}>
            {#if updater.state === 'downloading'}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name={updater.state === 'ready' ? 'refresh' : 'download'} size=".95rem" />{/if}
            {updater.state === 'ready' ? t('update.restartShort') : t('update.pill', { version: updater.version })}
          </button>
        {/if}
        <button class="btn btn-ghost btn-sm gap-2" onclick={pickFolder} disabled={app.scanning}><Icon name="folder" size="1rem" />{t('action.scanFolderPick')}</button>
        <button class="btn btn-ghost btn-square btn-sm" title={t('settings.theme')}
          onclick={() => app.setMode(document.documentElement.dataset.theme === 'tessera-dark' ? 'light' : 'dark')}>
          <Icon name={app.mode === 'light' ? 'moon' : 'sun'} />
        </button>
        <button class="btn btn-primary btn-sm shine gap-2" onclick={() => app.scan()} disabled={app.scanning || !app.target}>
          <Icon name="refresh" />{app.summary ? t('action.rescan') : t('action.scan')}
        </button>
      </div>
    </header>

    {#if app.view === 'overview'}<Overview />
    {:else if app.view === 'explorer'}<Explorer />
    {:else if app.view === 'cleanup'}<Cleanup />
    {:else if app.view === 'files'}<Files />
    {:else if app.view === 'dupes'}<Duplicates />
    {:else if app.view === 'history'}<History />
    {:else}<Settings />{/if}
  </main>
</div>

<dialog bind:this={scanDlg} class="modal" oncancel={e => e.preventDefault()}>
  <div class="modal-box glass text-center max-w-md">
    <div class="scan-orb mb-5"></div>
    <div class="text-lg font-semibold">{t('scan.title', { target: app.target })}</div>
    <div class="text-xs muted mb-4">{app.elevated ? t('scan.subMft') : t('scan.sub')}</div>
    <div class="grid grid-cols-3 gap-2 mb-3">
      <div class="rounded-xl p-2 soft"><div class="text-[11px] muted">{t('stat.files')}</div><div class="font-semibold mono">{num(app.progress.files)}</div></div>
      <div class="rounded-xl p-2 soft"><div class="text-[11px] muted">{t('stat.folders')}</div><div class="font-semibold mono">{num(app.progress.dirs)}</div></div>
      <div class="rounded-xl p-2 soft"><div class="text-[11px] muted">{t('scan.indexed')}</div><div class="font-semibold mono">{fmt(app.progress.bytes)}</div></div>
    </div>
    {#if app.drive}
      <progress class="progress progress-primary w-full" value={app.progress.bytes} max={app.drive.total - app.drive.available}></progress>
    {:else}
      <progress class="progress progress-primary w-full"></progress>
    {/if}
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
