<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type Game, type GameLeftover, type GameStatus } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { failures } from '../sys';
  import { reveal } from '../actions';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let games = $state<Game[] | null>(null);
  let leftovers = $state<GameLeftover[]>([]);
  let status = $state<GameStatus | null>(null);
  let q = $state('');
  let sort = $state<'recent' | 'size' | 'name'>('recent');
  let launcher = $state('all');
  let sel = $state<Game | null>(null);
  let dlg: HTMLDialogElement;

  async function load() {
    games = await api.games.list().catch(e => { app.toast(String(e), 'error'); return []; });
    leftovers = await api.games.leftovers().catch(() => []);
  }
  onMount(() => { load(); api.gamemode.status().then(s => (status = s)).catch(() => {}); });

  const DAY = 86400;
  let launchers = $derived([...new Set((games ?? []).map(g => g.launcher))]);
  let list = $derived.by(() => {
    let l = (games ?? []).filter(g => (launcher === 'all' || g.launcher === launcher) && (!q || g.name.toLowerCase().includes(q.toLowerCase())));
    if (sort === 'size') l = [...l].sort((a, b) => b.size - a.size);
    else if (sort === 'name') l = [...l].sort((a, b) => a.name.localeCompare(b.name));
    return l;
  });
  let total = $derived((games ?? []).reduce((s, g) => s + g.size, 0));
  let unplayed = $derived((games ?? []).filter(g => g.last_played && Date.now() / 1000 - g.last_played > 90 * DAY));
  let oldShaders = $derived((games ?? []).filter(g => g.shader_cache && g.shader_cache[1] > 0 && (!g.last_played || Date.now() / 1000 - g.last_played > 30 * DAY)));

  const HUES = [285, 200, 345, 150, 45, 255, 175, 80];
  const hue = (s: string) => HUES[[...s].reduce((h, c) => h + c.charCodeAt(0), 0) % HUES.length];
  const hours = (m: number) => (m >= 60 ? `${Math.round(m / 60)} h` : `${m} min`);

  function show(g: Game) { sel = g; dlg.showModal(); }
  async function launch(g: Game) {
    if (!g.launch) return;
    try { await api.games.launch(g.launch); app.toast(t('lib.launching', { game: g.name }), 'info'); dlg.close(); } catch (e) { app.toast(String(e), 'error'); }
  }

  async function clean(paths: string[], title: string) {
    const ok = await ask({ title, text: t('lib.cleanText'), ico: 'trash', confirm: t('lib.clean'), items: paths.map(p => ({ name: p })) });
    if (!ok) return;
    try {
      const out = await api.games.remove(paths);
      app.toast(t('lib.cleaned', { size: fmt(out.freed) }), 'success');
      failures(out.failed);
      await load();
      await app.refreshDrives();
    } catch (e) { app.toast(String(e), 'error'); }
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="app" title={t('lib.title')} sub={t('lib.sub')}>
    <label class="input input-sm w-52 flex items-center gap-2"><Icon name="search" size=".9rem" /><input bind:value={q} placeholder={t('files.filter')} /></label>
    <select class="select select-sm w-36" bind:value={sort}>
      {#each ['recent', 'size', 'name'] as s (s)}<option value={s}>{t('lib.sort.' + s)}</option>{/each}
    </select>
  </PageHead>
  {#if games}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-5">
      <div class="metric"><b>{games.length}</b><span>{t('lib.games')}</span></div>
      <div class="metric"><b>{fmt(total)}</b><span>{t('lib.onDisk')}</span></div>
      <div class="metric"><b>{unplayed.length}</b><span>{t('lib.unplayed')}</span></div>
      <div class="metric"><b>{fmt(leftovers.reduce((s, l) => s + l.size, 0) + oldShaders.reduce((s, g) => s + (g.shader_cache?.[1] ?? 0), 0))}</b><span>{t('lib.reclaimable')}</span></div>
    </div>
    {#if launchers.length > 1}
      <div class="flex flex-wrap gap-1.5 mt-4">
        <button class="btn btn-xs {launcher === 'all' ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => (launcher = 'all')}>{t('lib.all')}</button>
        {#each launchers as l (l)}<button class="btn btn-xs {launcher === l ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => (launcher = l)}>{t('lib.launcher.' + l)}</button>{/each}
      </div>
    {/if}
  {/if}
</section>

{#if games === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !games.length}
  <div class="glass p-10 text-center muted">{t('lib.none')}</div>
{:else}
  <section class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(150px,1fr))">
    {#each list as g, i (g.id)}
      {@const run = status?.game?.id === g.id}
      <button class="game-tile reveal text-left {run ? 'running' : ''}" style="--i:{Math.min(i, 12)}" onclick={() => show(g)}>
        <div class="absolute inset-0" style="background:linear-gradient(160deg,oklch(55% 0.16 {hue(g.name)}),oklch(30% 0.1 {hue(g.name) + 40}))"></div>
        <div class="letter">{g.name.replace(/[^\p{L}\p{N}]/gu, '')[0] ?? '?'}</div>
        {#if g.cover}<img src={g.cover} alt="" loading="lazy" onerror={e => ((e.target as HTMLImageElement).style.display = 'none')} />{/if}
        <div class="shade"></div>
        <div class="info">
          <div class="text-sm font-semibold leading-tight line-clamp-2">{g.name}</div>
          <div class="text-[11px] opacity-80 mt-1 flex justify-between gap-1">
            <span>{fmt(g.size)}</span>
            <span>{run ? t('lib.running') : g.last_played ? ago(g.last_played) : t('lib.launcher.' + g.launcher)}</span>
          </div>
        </div>
      </button>
    {/each}
  </section>
{/if}

{#if leftovers.length || oldShaders.length}
  <section class="glass p-5 reveal" style="--i:3">
    <div class="font-semibold flex items-center gap-2"><Icon name="trash" style="color:var(--color-primary)" />{t('lib.cleanup')}</div>
    <div class="text-xs muted">{t('lib.cleanupSub')}</div>
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4 mt-3">
      {#if leftovers.length}
        <div class="soft rounded-xl p-4">
          <div class="flex items-center justify-between gap-2">
            <div><div class="text-sm font-medium">{t('lib.leftovers')}</div><div class="text-[11px] muted">{t('lib.leftoversSub')}</div></div>
            <button class="btn btn-xs btn-primary" onclick={() => clean(leftovers.map(l => l.path), t('lib.leftovers'))}>{t('lib.cleanN', { size: fmt(leftovers.reduce((s, l) => s + l.size, 0)) })}</button>
          </div>
          {#each leftovers as l (l.path)}<div class="flex justify-between gap-3 text-xs mt-2"><span class="mono truncate" title={l.path}>{l.path}</span><span class="mono shrink-0">{fmt(l.size)}</span></div>{/each}
        </div>
      {/if}
      {#if oldShaders.length}
        <div class="soft rounded-xl p-4">
          <div class="flex items-center justify-between gap-2">
            <div><div class="text-sm font-medium">{t('lib.shaders')}</div><div class="text-[11px] muted">{t('lib.shadersSub')}</div></div>
            <button class="btn btn-xs btn-primary" onclick={() => clean(oldShaders.map(g => g.shader_cache![0]), t('lib.shaders'))}>{t('lib.cleanN', { size: fmt(oldShaders.reduce((s, g) => s + (g.shader_cache?.[1] ?? 0), 0)) })}</button>
          </div>
          {#each oldShaders as g (g.id)}<div class="flex justify-between gap-3 text-xs mt-2"><span class="truncate">{g.name}</span><span class="mono shrink-0">{fmt(g.shader_cache?.[1] ?? 0)}</span></div>{/each}
        </div>
      {/if}
    </div>
  </section>
{/if}

<dialog bind:this={dlg} class="modal">
  {#if sel}
    <div class="modal-box glass max-w-lg p-0 overflow-hidden">
      <div class="relative h-40" style="background:linear-gradient(160deg,oklch(55% 0.16 {hue(sel.name)}),oklch(30% 0.1 {hue(sel.name) + 40}))">
        {#if sel.cover}<img src={sel.cover.replace('library_600x900', 'library_hero')} alt="" class="absolute inset-0 w-full h-full object-cover" onerror={e => ((e.target as HTMLImageElement).style.display = 'none')} />{/if}
        <div class="absolute inset-0" style="background:linear-gradient(180deg,transparent 30%,var(--glass-solid))"></div>
        <div class="absolute left-5 bottom-3 right-5">
          <div class="text-xl font-bold leading-tight">{sel.name}</div>
          <div class="text-xs muted">{t('lib.launcher.' + sel.launcher)}</div>
        </div>
      </div>
      <div class="p-5">
        <div class="grid grid-cols-3 gap-2">
          <div class="tile-stat"><div>{t('lib.size')}</div><div>{fmt(sel.size)}</div></div>
          <div class="tile-stat"><div>{t('lib.lastPlayed')}</div><div>{sel.last_played ? ago(sel.last_played) : '—'}</div></div>
          <div class="tile-stat"><div>{t('lib.playtime')}</div><div>{sel.playtime ? hours(sel.playtime) : '—'}</div></div>
        </div>
        <div class="text-[11px] muted mono truncate mt-3" title={sel.dir}>{sel.dir}</div>
        <div class="modal-action flex-wrap">
          <button class="btn btn-ghost btn-sm gap-1.5" onclick={() => reveal(sel!.dir)}><Icon name="folder" size=".95rem" />{t('action.reveal')}</button>
          <button class="btn btn-ghost btn-sm gap-1.5" onclick={() => { dlg.close(); app.view = 'gamemode'; }}><Icon name="rocket" size=".95rem" />{t('lib.boostSettings')}</button>
          {#if sel.launch}<button class="btn btn-primary btn-sm gap-1.5 shine" onclick={() => launch(sel!)}><Icon name="play" size=".9rem" />{t('lib.play')}</button>{/if}
        </div>
      </div>
    </div>
  {/if}
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
