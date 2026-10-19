<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, type AppProcess, type OptOutcome } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let stats = $state<{ memory: { total: number; used: number }; cpu: number } | null>(null);
  let procs = $state<AppProcess[] | null>(null);
  let picked = $state(new Set<string>());

  async function loadProcs() {
    const p = await api.opt.processes().catch(() => []);
    procs = p;
    // Pre-select what is safe to close before a game; browsers, media and Office stay unless picked
    picked = new Set(p.filter(x => ['chat', 'launcher', 'sync', 'dev', 'server', 'torrent'].includes(x.group)).map(x => x.name));
  }

  $effect(() => {
    loadProcs();
    const tick = () => api.opt.stats().then(s => (stats = s)).catch(() => {});
    tick();
    const timer = setInterval(tick, 2500);
    return () => clearInterval(timer);
  });

  function report(out: OptOutcome, ok: string) {
    app.toast(ok, 'success');
    if (out.failed.length) app.toast(t('opt.failed', { n: out.failed.length, first: out.failed[0] }), 'warning');
  }
  async function closeApps() {
    const pids = (procs ?? []).filter(p => picked.has(p.name)).flatMap(p => p.pids);
    const out = await api.opt.kill(pids);
    report(out, t('opt.closed', { n: picked.size }));
    setTimeout(loadProcs, 800);
  }

  let memPct = $derived(stats ? (stats.memory.used / Math.max(1, stats.memory.total)) * 100 : 0);
  let closeSize = $derived((procs ?? []).filter(p => picked.has(p.name)).reduce((s, p) => s + p.memory, 0));
</script>

<section class="glass p-5 reveal">
  <PageHead ico="activity" title={t('opt.apps')} sub={t('opt.appsSub')} />
  <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
    <div class="rounded-xl soft p-4">
      <div class="flex justify-between text-sm"><span class="muted">{t('opt.cpu')}</span><span class="mono font-semibold">{(stats?.cpu ?? 0).toFixed(0)}%</span></div>
      <div class="bar mt-2" style="height:8px"><i style="width:{stats?.cpu ?? 0}%;background:linear-gradient(90deg,var(--color-primary),var(--color-accent))"></i></div>
    </div>
    <div class="rounded-xl soft p-4">
      <div class="flex justify-between text-sm"><span class="muted">{t('opt.ram')}</span><span class="mono font-semibold">{fmt(stats?.memory.used ?? 0)} / {fmt(stats?.memory.total ?? 0)}</span></div>
      <div class="bar mt-2" style="height:8px"><i style="width:{memPct}%;background:{memPct > 85 ? 'var(--color-warning)' : 'linear-gradient(90deg,var(--color-secondary),var(--color-primary))'}"></i></div>
    </div>
  </div>
</section>

<section class="glass p-5 reveal" style="--i:1">
  <div class="flex items-center justify-between gap-2">
    <div class="font-semibold">{t('background.running')}</div>
    <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={loadProcs}><Icon name="refresh" size=".9rem" /></button>
  </div>
  {#if procs === null}
    <div class="p-8 text-center"><span class="loading loading-dots"></span></div>
  {:else if !procs.length}
    <div class="text-sm muted py-6 text-center">{t('opt.appsNone')}</div>
  {:else}
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-x-6 gap-y-0.5 mt-2">
      {#each procs as p (p.name)}
        <label class="row-hover flex items-center gap-3 px-2 py-2 cursor-pointer">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={picked.has(p.name)}
            onchange={e => { const s = new Set(picked); (e.target as HTMLInputElement).checked ? s.add(p.name) : s.delete(p.name); picked = s; }} />
          <span class="flex-1 min-w-0 truncate text-sm">{p.name.replace(/\.exe$/i, '')}</span>
          <span class="tag">{t('opt.group.' + p.group)}</span>
          <span class="mono text-xs w-16 text-right">{fmt(p.memory)}</span>
        </label>
      {/each}
    </div>
    <button class="btn btn-sm btn-primary mt-4" disabled={!picked.size} onclick={closeApps}>{t('opt.close', { size: fmt(closeSize) })}</button>
  {/if}
</section>
