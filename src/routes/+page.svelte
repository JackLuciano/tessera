<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { app, type View } from '$lib/store.svelte';
  import { NAV_TOP, NAV_GROUPS, NAV_SETTINGS, NAV_ALL, groupOf, type NavGroup, type NavItem } from '$lib/nav';
  import { api, ago, fmt, num } from '$lib/api';
  import { t, i18n } from '$lib/i18n.svelte';
  import Icon from '$lib/Icon.svelte';
  import Logo from '$lib/Logo.svelte';
  import ConfirmDialog from '$lib/ConfirmDialog.svelte';
  import AskDialog from '$lib/AskDialog.svelte';
  import ContextMenu from '$lib/ContextMenu.svelte';
  import Palette from '$lib/Palette.svelte';
  import UpdateDialog from '$lib/UpdateDialog.svelte';
  import { updater } from '$lib/update.svelte';
  import Care from '$lib/views/Care.svelte';
  import Overview from '$lib/views/Overview.svelte';
  import Explorer from '$lib/views/Explorer.svelte';
  import Files from '$lib/views/Files.svelte';
  import Duplicates from '$lib/views/Duplicates.svelte';
  import History from '$lib/views/History.svelte';
  import Cleanup from '$lib/views/Cleanup.svelte';
  import Privacy from '$lib/views/Privacy.svelte';
  import Registry from '$lib/views/Registry.svelte';
  import Shortcuts from '$lib/views/Shortcuts.svelte';
  import Shredder from '$lib/views/Shredder.svelte';
  import Optimizer from '$lib/views/Optimizer.svelte';
  import Startup from '$lib/views/Startup.svelte';
  import Background from '$lib/views/Background.svelte';
  import Internet from '$lib/views/Internet.svelte';
  import Uninstaller from '$lib/views/Uninstaller.svelte';
  import Updater from '$lib/views/Updater.svelte';
  import DiskHealth from '$lib/views/DiskHealth.svelte';
  import Settings from '$lib/views/Settings.svelte';
  import GameMode from '$lib/views/GameMode.svelte';
  import Library from '$lib/views/Library.svelte';
  import Fps from '$lib/views/Fps.svelte';
  import Display from '$lib/views/Display.svelte';
  import Temps from '$lib/views/Temps.svelte';
  import BatteryView from '$lib/views/Battery.svelte';
  import Crashes from '$lib/views/Crashes.svelte';
  import SecurityView from '$lib/views/Security.svelte';
  import Boot from '$lib/views/Boot.svelte';
  import Drivers from '$lib/views/Drivers.svelte';
  import WinUpdate from '$lib/views/WinUpdate.svelte';
  import Report from '$lib/views/Report.svelte';
  import Slow from '$lib/views/Slow.svelte';
  import Wake from '$lib/views/Wake.svelte';
  import Graphics from '$lib/views/Graphics.svelte';
  import Benchmark from '$lib/views/Benchmark.svelte';
  import Backup from '$lib/views/Backup.svelte';
  import Home from '$lib/views/Home.svelte';
  import MultiPc from '$lib/views/MultiPc.svelte';
  import Overlay from '$lib/windows/Overlay.svelte';
  import Widget from '$lib/windows/Widget.svelte';
  import Prompt from '$lib/windows/Prompt.svelte';
  import { SMALL_KIND } from '$lib/windows/small';

  /** Small extra windows (FPS window, widget, boost prompt) load this page with ?window=… */
  const SMALL = SMALL_KIND;
  const SMALL_VIEWS: Record<string, typeof Overlay> = { overlay: Overlay, widget: Widget, prompt: Prompt };

  const VIEWS: Record<View, typeof Care> = {
    care: Care, overview: Overview, explorer: Explorer, files: Files, dupes: Duplicates, history: History,
    cleanup: Cleanup, privacy: Privacy, registry: Registry, shortcuts: Shortcuts, shredder: Shredder,
    optimizer: Optimizer, startup: Startup, background: Background, internet: Internet,
    uninstaller: Uninstaller, updater: Updater, health: DiskHealth, settings: Settings,
    gamemode: GameMode, library: Library, fps: Fps, display: Display, temps: Temps, battery: BatteryView, crashes: Crashes,
    security: SecurityView, boot: Boot, drivers: Drivers, winupdate: WinUpdate, report: Report, slow: Slow, wake: Wake, graphics: Graphics, bench: Benchmark, backup: Backup, home: Home, multipc: MultiPc,
  };
  /** Pages that show the scanned drive; the header shows its path there and the drives list matters. */
  const DISK_VIEWS: View[] = ['overview', 'explorer', 'files', 'dupes', 'history', 'cleanup', 'shortcuts'];

  // Accordion: one group open at a time, following the page you're on
  let openGroup = $state<string | null>(groupOf(app.view)?.key ?? null);
  $effect(() => { const g = groupOf(app.view); if (g) openGroup = g.key; });

  // Narrow windows: the sidebar is an icon rail and groups open as a flyout next to it
  let fly = $state<{ group: NavGroup; top: number } | null>(null);
  function openFly(g: NavGroup, e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    fly = fly?.group.key === g.key ? null : { group: g, top: Math.min(r.top, window.innerHeight - 60 - g.items.length * 44) };
  }
  function go(v: View) { app.view = v; fly = null; }

  let scanDlg = $state<HTMLDialogElement>();
  /** Estimated scan progress 0..0.95, or null when unknown (folder scan, or past the estimate). */
  let scanPct = $derived.by(() => {
    const d = app.drive;
    const used = d ? d.total - d.available : 0;
    // Only a whole-drive scan can be compared with the drive's used space
    if (!d || !used || app.target.replace(/[\\/]+$/, '').toUpperCase() !== d.mount.replace(/[\\/]+$/, '').toUpperCase()) return null;
    const p = app.progress.bytes / used;
    return p >= 0.95 ? null : p;
  });
  $effect(() => { if (!scanDlg) return; if (app.scanning && !scanDlg.open) scanDlg.showModal(); else if (!app.scanning && scanDlg.open) scanDlg.close(); });

  onMount(() => {
    if (SMALL) {
      document.documentElement.classList.add('small-window');
      return;
    }
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
    const timer = setTimeout(() => updater.check(true), 3000);
    return () => clearTimeout(timer);
  });

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === 'string') app.scan(dir);
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'F5') { e.preventDefault(); app.scan(); }
    if (e.key === 'Escape') fly = null;
    const typing = /INPUT|TEXTAREA|SELECT/.test((document.activeElement as HTMLElement)?.tagName ?? '');
    if (!typing && !e.ctrlKey && !e.altKey && !document.querySelector('dialog[open]') && /^[1-9]$/.test(e.key) && NAV_ALL[+e.key - 1]) app.view = NAV_ALL[+e.key - 1].v;
  }

  let group = $derived(groupOf(app.view));
  let showDrive = $derived(DISK_VIEWS.includes(app.view));
  let Current = $derived(VIEWS[app.view] ?? Care);
</script>

<svelte:window onkeydown={e => !SMALL && onkey(e)} />

{#if SMALL}
  {@const W = SMALL_VIEWS[SMALL]}
  {#if W}<W />{/if}
{:else}
<div class="bg-blobs"><span></span><span></span><span></span></div>

{#snippet item(n: NavItem, child = false)}
  <button class="nav-item max-lg:justify-center {child ? 'nav-child' : ''} {app.view === n.v ? 'active' : ''}" title={t('nav.' + n.v)} onclick={() => go(n.v)}>
    <Icon name={n.ico} size={child ? '1.05rem' : '1.15rem'} /><span class="max-lg:hidden">{t('nav.' + n.v)}</span>
  </button>
{/snippet}

<div class="relative z-10 flex min-h-screen">
  <!-- Below 1024 px the sidebar collapses to an icon rail so the content keeps its room -->
  <aside class="flex flex-col w-[84px] lg:w-64 shrink-0 p-3 lg:p-4 gap-4 sticky top-0 h-screen">
    <div class="glass flex-1 flex flex-col p-2 lg:p-4 gap-5 overflow-y-auto overflow-x-hidden">
      <div class="flex items-center gap-3 px-1 max-lg:justify-center max-lg:pt-2">
        <Logo size={40} />
        <div class="min-w-0 max-lg:hidden">
          <div class="font-bold text-lg leading-tight tracking-tight">Tessera</div>
          <div class="text-xs muted truncate">{t('app.tagline')}</div>
        </div>
      </div>

      <nav class="flex flex-col gap-1 lg:pl-2">
        {#each NAV_TOP as top (top.v)}{@render item(top)}{/each}
        {#each NAV_GROUPS as g (g.key)}
          {@const here = group?.key === g.key}
          {@const expanded = openGroup === g.key}
          <!-- Wide: collapsible group. Narrow: one icon that opens a flyout. The show/hide sits on
               wrappers because the nav classes set `display` themselves and would override it. -->
          <div class="max-lg:hidden">
            <button class="nav-group {here ? 'here' : ''}" aria-expanded={expanded} onclick={() => (openGroup = expanded ? null : g.key)}>
              <Icon name={g.ico} size="1.05rem" /><span>{t('nav.group.' + g.key)}</span>
              <Icon name="chev" size=".85rem" style="transition:transform .25s;transform:rotate({expanded ? 90 : 0}deg);opacity:.6" />
            </button>
            <div class="nav-children {expanded ? 'open' : ''}">
              <div class="min-h-0 flex flex-col gap-0.5">
                {#each g.items as n (n.v)}{@render item(n, true)}{/each}
              </div>
            </div>
          </div>
          <div class="lg:hidden">
            <button class="nav-item justify-center {here ? 'active' : ''} {fly?.group.key === g.key ? 'flying' : ''}" title={t('nav.group.' + g.key)} onclick={e => openFly(g, e)}>
              <Icon name={g.ico} />
            </button>
          </div>
        {/each}
        <div class="h-px my-2 mx-2" style="background:var(--glass-border)"></div>
        {@render item(NAV_SETTINGS)}
      </nav>

      {#if showDrive || app.view === 'care'}
        <div>
          <div class="flex items-center justify-between px-1 mb-2 max-lg:hidden">
            <div class="text-xs uppercase tracking-wider muted font-semibold">{t('nav.drives')}</div>
            <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={() => app.refreshDrives()}><Icon name="refresh" size=".9rem" /></button>
          </div>
          <div class="flex flex-col gap-1.5">
            {#each app.drives as d (d.mount)}
              {@const used = d.total - d.available}
              {@const p = (used / d.total) * 100}
              {@const active = app.drive?.mount === d.mount}
              {@const label = `${d.name || t('drive.local')} (${d.mount.replace(/\\$/, '')})`}
              <button class="rounded-xl p-2.5 text-left transition min-w-0 {active ? '' : 'opacity-75 hover:opacity-100'}"
                style={active ? 'background:color-mix(in oklch,var(--color-primary) 10%,transparent)' : ''}
                title={`${label} · ${t('drive.freeOf', { free: fmt(d.available), total: fmt(d.total) })}`}
                onclick={() => { app.openDrive(d.mount); if (!showDrive) app.view = 'overview'; }} disabled={app.scanning}>
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
      {/if}

      <div class="mt-auto rounded-xl p-3 soft text-xs max-lg:hidden">
        {#if app.elevated}
          <div class="flex items-center gap-2 text-sm font-medium"><span class="w-2 h-2 rounded-full" style="background:var(--color-success)"></span> {t('engine.admin')}</div>
          <div class="muted mt-1">{t('engine.adminHint')}</div>
        {:else}
          <div class="flex items-center gap-2 text-sm font-medium"><span class="w-2 h-2 rounded-full" style="background:var(--color-warning)"></span> {t('engine.user')}</div>
          <div class="muted mt-1">{t('engine.userHint')}</div>
          <button class="btn btn-xs btn-ghost mt-2 -ml-2 gap-1" onclick={() => api.relaunchAsAdmin().catch(e => app.toast(String(e), 'info'))}><Icon name="shield" size=".85rem" />{t('engine.elevateShort')}</button>
        {/if}
      </div>
    </div>
  </aside>

  {#if fly}
    <button class="fixed inset-0 z-40 cursor-default" aria-label="close" onclick={() => (fly = null)}></button>
    <div class="nav-fly glass fixed z-50 left-[88px] p-2 w-60" style="top:{fly.top}px">
      <div class="text-[11px] uppercase tracking-wider muted font-semibold px-3 pt-1 pb-1.5">{t('nav.group.' + fly.group.key)}</div>
      {#each fly.group.items as n (n.v)}
        <button class="nav-item {app.view === n.v ? 'active' : ''}" onclick={() => go(n.v)}><Icon name={n.ico} size="1.05rem" /><span>{t('nav.' + n.v)}</span></button>
      {/each}
    </div>
  {/if}

  <main class="flex-1 min-w-0 p-4 pl-0 flex flex-col gap-4">
    <header class="glass flex items-center gap-3 px-4 py-3 sticky top-4 z-30">
      <div class="min-w-0">
        <div class="text-xs muted truncate">
          {#if showDrive}{app.summary ? app.summary.root : t('header.noScan')}{:else if group}{t('nav.group.' + group.key)}{:else}Tessera{/if}
        </div>
        <div class="font-semibold truncate">{t('nav.' + app.view)}</div>
      </div>
      {#if showDrive && app.summary && app.catchingUp}
        <span class="chip-soft"><span class="loading loading-spinner loading-xs"></span>{t('cache.catchingUp')}</span>
      {:else if showDrive && app.live && app.summary}
        <span class="live-pill" title={t('live.hint', { n: num(app.liveFolders) })}><i></i>{t('live.on')}</span>
      {/if}
      {#if showDrive && app.summary && app.cachedAt}
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
        <button class="btn btn-ghost btn-square btn-sm" title={t('settings.theme')}
          onclick={() => app.setMode(document.documentElement.dataset.theme === 'tessera-dark' ? 'light' : 'dark')}>
          <Icon name={app.mode === 'light' ? 'moon' : 'sun'} />
        </button>
        {#if showDrive}
          <button class="btn btn-ghost btn-sm gap-2" title={t('action.scanFolderPick')} onclick={pickFolder} disabled={app.scanning}><Icon name="folder" size="1rem" /><span class="max-xl:hidden">{t('action.scanFolderPick')}</span></button>
          <button class="btn btn-primary btn-sm shine gap-2" title={app.summary ? t('action.rescan') : t('action.scan')} onclick={() => app.scan()} disabled={app.scanning || !app.target}>
            <Icon name="refresh" /><span class="max-lg:hidden">{app.summary ? t('action.rescan') : t('action.scan')}</span>
          </button>
        {/if}
      </div>
    </header>

    {#key app.view}
      <div class="view-enter flex flex-col gap-4"><Current /></div>
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
<AskDialog />
<ContextMenu />
<Palette />
<UpdateDialog />

<div class="toast toast-end toast-bottom z-[60]">
  {#each app.toasts as tt (tt.id)}
    <div class="alert alert-{tt.kind} alert-soft shadow-lg glass max-w-md"><span class="selectable">{tt.msg}</span></div>
  {/each}
</div>
{/if}
