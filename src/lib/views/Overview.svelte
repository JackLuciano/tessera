<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, num, ago, CATS, type TreeNode, type FileEntry } from '../api';
  import { t } from '../i18n.svelte';
  import { nodeMenu } from '../actions';
  import Icon from '../Icon.svelte';
  import Logo from '../Logo.svelte';
  import Donut from '../Donut.svelte';

  let root = $state<TreeNode | null>(null);
  let top = $state<FileEntry[]>([]);

  $effect(() => {
    void app.version;
    if (!app.summary) return;
    api.node(0, 1, 8).then(n => (root = n));
    api.largestFiles(6).then(f => (top = f));
  });

  const RING = 2 * Math.PI * 50;
  let used = $derived(app.drive ? app.drive.total - app.drive.available : app.summary?.size ?? 0);
  let total = $derived(app.drive?.total ?? Math.max(used, 1));
  let pct = $derived(used / total);
  let maxFolder = $derived(root?.children[0]?.size ?? 1);

  function openInExplorer(n: TreeNode) { app.view = 'explorer'; app.explorerTarget = n.id; }
</script>

{#if !app.summary}
  <section class="glass p-10 flex flex-col items-center text-center gap-4 reveal">
    <Logo size={84} />
    <h1 class="text-3xl font-bold tracking-tight mt-2">{t('empty.title')}</h1>
    <p class="muted max-w-md">{t('empty.text')}</p>
    <div class="flex gap-2 flex-wrap justify-center mt-2">
      {#each app.drives as d (d.mount)}
        <button class="btn btn-primary shine gap-2" onclick={() => app.scan(d.mount)}><Icon name="drive" />{t('action.scanDrive', { drive: d.mount.replace(/\\$/, '') })}</button>
      {/each}
    </div>
    <div class="text-xs muted mt-2">{t('empty.hint')}</div>
  </section>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-12 gap-4">
    <div class="glass card-lift p-5 xl:col-span-6 reveal flex items-center gap-6" style="--i:1">
      <div class="relative w-44 h-44 shrink-0">
        <svg viewBox="0 0 120 120" class="w-full h-full -rotate-90">
          <defs><linearGradient id="ringGrad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" style="stop-color:var(--color-primary)" /><stop offset="100%" style="stop-color:var(--color-accent)" /></linearGradient></defs>
          <circle cx="60" cy="60" r="50" fill="none" stroke-width="11" style="stroke:color-mix(in oklch,var(--color-base-content) 9%,transparent)" />
          <circle cx="60" cy="60" r="50" fill="none" stroke="url(#ringGrad)" stroke-width="11" stroke-linecap="round"
            style="stroke-dasharray:{RING};stroke-dashoffset:{RING * (1 - pct)};transition:stroke-dashoffset 1.6s cubic-bezier(.2,.8,.2,1)" />
        </svg>
        <div class="absolute inset-0 grid place-items-center text-center">
          <div><div class="text-4xl font-bold tracking-tight mono">{Math.round(pct * 100)}%</div><div class="text-xs muted">{t('stat.used')}</div></div>
        </div>
      </div>
      <div class="flex-1 min-w-0">
        <div class="text-sm muted">{t('stat.free')}</div>
        <div class="text-4xl font-bold tracking-tight mono grad-text">{fmt(total - used)}</div>
        <div class="text-sm muted mt-1">{t('stat.ofUsed', { total: fmt(total), used: fmt(used) })}</div>
        <div class="grid grid-cols-3 gap-2 mt-4">
          <div class="rounded-xl p-2.5 soft"><div class="text-[11px] muted">{t('stat.files')}</div><div class="font-semibold mono">{num(app.summary.files)}</div></div>
          <div class="rounded-xl p-2.5 soft"><div class="text-[11px] muted">{t('stat.folders')}</div><div class="font-semibold mono">{num(app.summary.dirs)}</div></div>
          <div class="rounded-xl p-2.5 soft"><div class="text-[11px] muted">{t('stat.scanTime')}</div><div class="font-semibold mono">{(app.summary.elapsed_ms / 1000).toFixed(1)}s</div></div>
        </div>
        {#if app.summary.errors > 0}
          <div class="text-[11px] muted mt-2">{t('stat.skipped', { n: num(app.summary.errors) })}</div>
        {/if}
      </div>
    </div>

    <div class="glass p-4 xl:col-span-6 reveal flex gap-4 items-center" style="--i:2">
      <Donut cats={app.summary.cats} />
      <div class="flex flex-col gap-0.5 flex-1 min-w-0">
        <div class="font-semibold mb-1">{t('overview.byType')}</div>
        {#each app.summary.cats as c (c.cat)}
          <div class="row-hover flex items-center gap-2.5 px-2 py-1">
            <span class="w-6 h-6 rounded-lg grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--cat-{c.cat}) 18%,transparent);color:var(--cat-{c.cat})"><Icon name={CATS[c.cat].ico} size=".8rem" /></span>
            <div class="flex-1 min-w-0">
              <div class="flex justify-between text-sm"><span class="truncate">{t('cat.' + c.cat)}</span><span class="mono muted text-xs">{fmt(c.size)}</span></div>
              <div class="bar mt-1" style="height:4px"><i style="width:{(c.size / app.summary.cats[0].size) * 100}%;background:var(--cat-{c.cat})"></i></div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <section class="grid grid-cols-1 xl:grid-cols-12 gap-4">
    <div class="glass p-4 xl:col-span-6 reveal" style="--i:3">
      <div class="flex items-center justify-between mb-2">
        <div class="font-semibold">{t('overview.biggestFolders')}</div>
        <button class="btn btn-ghost btn-xs gap-1" onclick={() => (app.view = 'explorer')}>{t('overview.openExplorer')} <Icon name="chev" size=".85rem" /></button>
      </div>
      {#each root?.children ?? [] as n (n.id)}
        <button class="row-hover w-full text-left flex items-center gap-3 px-2 py-2" onclick={() => n.kind === 'dir' && openInExplorer(n)}
          oncontextmenu={e => nodeMenu(e, n, () => openInExplorer(n))}>
          <span class="w-8 h-8 rounded-lg grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--cat-{n.cat}) 16%,transparent);color:var(--cat-{n.cat})"><Icon name={n.kind === 'dir' ? 'folder' : CATS[n.cat].ico} size="1rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="flex justify-between gap-2 text-sm"><span class="truncate font-medium">{n.name}</span><span class="mono">{fmt(n.size)}</span></div>
            <div class="flex items-center gap-2 mt-1"><div class="bar flex-1" style="height:5px"><i style="width:{(n.size / maxFolder) * 100}%;background:var(--cat-{n.cat})"></i></div><span class="text-[11px] muted mono w-24 text-right truncate">{t('common.nFiles', { n: num(n.files) })}</span></div>
          </div>
        </button>
      {/each}
    </div>

    <div class="glass p-4 xl:col-span-6 reveal" style="--i:4">
      <div class="flex items-center justify-between mb-2">
        <div class="font-semibold">{t('nav.files')}</div>
        <button class="btn btn-ghost btn-xs gap-1" onclick={() => (app.view = 'files')}>{t('common.seeAll')} <Icon name="chev" size=".85rem" /></button>
      </div>
      {#each top as f (f.id)}
        <div class="frow flex items-center gap-3 px-2 py-2 rounded-lg" role="listitem" oncontextmenu={e => nodeMenu(e, { ...f, kind: 'file' })}>
          <span class="w-8 h-8 rounded-lg grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--cat-{f.cat}) 16%,transparent);color:var(--cat-{f.cat})"><Icon name={CATS[f.cat].ico} size="1rem" /></span>
          <div class="flex-1 min-w-0"><div class="text-sm font-medium truncate">{f.name}</div><div class="text-[11px] muted mono truncate">{f.path}</div></div>
          <span class="text-xs muted hidden md:block">{ago(f.modified)}</span>
          <span class="mono text-sm font-semibold w-20 text-right">{fmt(f.size)}</span>
        </div>
      {/each}
    </div>
  </section>
{/if}
