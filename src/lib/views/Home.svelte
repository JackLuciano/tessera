<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type Snapshot, type PingState, type FpsSummary, type CareState, type Battery, type BackupConfig, type BootInfo, type StatsReport } from '../api';
  import { t } from '../i18n.svelte';
  import { dash, AVAILABLE, TILES, type TileKind } from '../dashboard.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';
  import Spark from '../Spark.svelte';

  // Home: tiles pinned from any page, live where it makes sense
  let editing = $state(false);
  let dragging = $state<TileKind | null>(null);

  let sens = $state<Snapshot | null>(null);
  let cpuHist = $state<number[]>([]);
  let ping = $state<PingState | null>(null);
  let fps = $state<FpsSummary | null>(null);
  let care = $state<CareState | null>(null);
  let battery = $state<Battery | null>(null);
  let backup = $state<BackupConfig | null>(null);
  let boot = $state<BootInfo | null>(null);
  let peers = $state<{ online: number; total: number } | null>(null);
  let report = $state<StatsReport | null>(null);

  const on = (k: TileKind) => dash.has(k);
  onMount(() => {
    // Only what the pinned tiles need is asked for
    if (on('care')) api.care.config().then(c => (care = c)).catch(() => {});
    if (on('fps')) api.fps.sessions().then(s => (fps = s[0] ?? null)).catch(() => {});
    if (on('battery')) api.battery().then(b => (battery = b.battery)).catch(() => {});
    if (on('backup')) api.backup.config().then(b => (backup = b)).catch(() => {});
    if (on('boot')) api.boot().then(b => (boot = b)).catch(() => {});
    if (on('report')) api.report(7).then(r => (report = r)).catch(() => {});
    if (on('peers')) api.lan.peers().then(p => (peers = { online: p.filter(x => x.status).length, total: p.length })).catch(() => {});
    const tick = () => {
      if (on('temps') || on('load')) api.sensors().then(s => { sens = s; if (s) cpuHist = [...cpuHist, s.cpu_load].slice(-40); }).catch(() => {});
      if (on('ping')) api.ping().then(p => (ping = p)).catch(() => {});
    };
    tick();
    const timer = setInterval(tick, 3000);
    return () => clearInterval(timer);
  });

  const meta = (k: TileKind) => TILES.find(x => x.kind === k)!;
  let gpu = $derived(sens?.gpus[0]);
  let ram = $derived(sens && sens.ram_total ? (sens.ram_used / sens.ram_total) * 100 : null);
  let lastBackup = $derived(backup?.jobs.reduce((m, j) => Math.max(m, j.last_run), 0) ?? 0);
  let backupErr = $derived(backup?.jobs.some(j => j.last_error) ?? false);
  let unpinned = $derived(AVAILABLE.filter(x => !dash.has(x.kind)));
  const pct = (d: { total: number; available: number }) => ((d.total - d.available) / Math.max(1, d.total)) * 100;
  const toneOf = (temp: number | null | undefined) => (temp == null ? 'var(--muted)' : temp >= 85 ? 'var(--color-error)' : temp >= 72 ? 'var(--color-warning)' : 'var(--color-success)');
</script>

<section class="glass p-5 reveal">
  <PageHead ico="home" title={t('home.title')} sub={t('home.sub')}>
    {#if editing}<button class="btn btn-sm btn-ghost" onclick={() => dash.reset()}>{t('home.reset')}</button>{/if}
    <button class="btn btn-sm {editing ? 'btn-primary' : 'btn-ghost border hairline'} gap-1.5" onclick={() => (editing = !editing)}><Icon name={editing ? 'check' : 'wrench'} size=".9rem" />{editing ? t('home.done') : t('home.edit')}</button>
  </PageHead>
  {#if editing && unpinned.length}
    <div class="flex flex-wrap gap-1.5 mt-4">
      <span class="text-[11px] muted self-center mr-1">{t('home.add')}</span>
      {#each unpinned as u (u.kind)}<button class="badge badge-ghost gap-1 hover:badge-primary" onclick={() => dash.toggle(u.kind)}><Icon name={u.ico} size=".7rem" />{t('home.t.' + u.kind)}</button>{/each}
    </div>
  {/if}
</section>

{#if !dash.tiles.length}
  <div class="glass p-10 text-center"><Icon name="pin" size="1.8rem" style="opacity:.4" /><div class="text-sm mt-2">{t('home.empty')}</div></div>
{/if}

<section class="grid gap-4" style="grid-template-columns:repeat(auto-fill,minmax(230px,1fr))">
  {#each dash.tiles as tile, i (tile.kind)}
    {@const m = meta(tile.kind)}
    <div class="glass p-5 reveal home-tile {tile.wide ? 'wide' : ''} {dragging === tile.kind ? 'opacity-40' : ''} {editing ? 'editing' : ''}" style="--i:{i}"
      draggable={editing} role="button" tabindex="0"
      ondragstart={() => (dragging = tile.kind)} ondragend={() => (dragging = null)}
      ondragover={e => { e.preventDefault(); if (dragging) dash.move(dragging, tile.kind); }}
      onclick={() => !editing && (app.view = m.view)} onkeydown={e => e.key === 'Enter' && !editing && (app.view = m.view)}>
      <div class="flex items-center justify-between gap-2">
        <div class="text-xs muted flex items-center gap-1.5 font-medium"><Icon name={m.ico} size=".85rem" />{t('home.t.' + tile.kind)}</div>
        {#if editing}
          <div class="flex gap-0.5">
            <button class="btn btn-ghost btn-xs btn-square" title={t('home.wide')} onclick={e => { e.stopPropagation(); dash.widen(tile.kind); }}><Icon name="explorer" size=".75rem" /></button>
            <button class="btn btn-ghost btn-xs btn-square" title={t('home.unpin')} onclick={e => { e.stopPropagation(); dash.toggle(tile.kind); }}><Icon name="x" size=".75rem" /></button>
          </div>
        {:else}<Icon name="chev" size=".8rem" style="opacity:.35" />{/if}
      </div>

      <div class="mt-3">
        {#if tile.kind === 'care'}
          <div class="flex items-center gap-4">
            <Ring value={care?.last_score ?? null} label={t('home.score')} size={84} warn={80} bad={60} invert />
            <div class="text-xs muted">{care?.last_run ? t('home.lastScan', { ago: ago(care.last_run, true) }) : t('home.noScan')}{#if care?.last_found}<div class="mt-1">{t('home.toClean', { size: fmt(care.last_found) })}</div>{/if}</div>
          </div>
        {:else if tile.kind === 'temps'}
          <div class="flex gap-6">
            <div class="metric"><b style="color:{toneOf(sens?.cpu_temp)}">{sens?.cpu_temp != null ? Math.round(sens.cpu_temp) + '°' : '—'}</b><span>CPU</span></div>
            <div class="metric"><b style="color:{toneOf(gpu?.temp)}">{gpu?.temp != null ? Math.round(gpu.temp) + '°' : '—'}</b><span>GPU</span></div>
          </div>
        {:else if tile.kind === 'load'}
          <div class="flex gap-6">
            <div class="metric"><b>{sens ? Math.round(sens.cpu_load) : '—'}<small class="text-xs muted">%</small></b><span>CPU</span></div>
            <div class="metric"><b>{ram != null ? Math.round(ram) : '—'}<small class="text-xs muted">%</small></b><span>{t('slow.ram')}</span></div>
          </div>
          {#if cpuHist.length > 2}<div class="mt-2"><Spark values={cpuHist} height={tile.wide ? 50 : 30} min={0} max={100} /></div>{/if}
        {:else if tile.kind === 'ping'}
          <div class="metric"><b>{ping?.avg != null ? Math.round(ping.avg) : '—'}<small class="text-xs muted"> ms</small></b><span>{ping?.host ?? '1.1.1.1'} · {t('ping.jitter')} {ping?.jitter != null ? ping.jitter.toFixed(1) : '—'}</span></div>
          {#if ping?.samples.length}<div class="mt-2"><Spark values={ping.samples} height={28} color="var(--color-secondary)" /></div>{/if}
        {:else if tile.kind === 'fps'}
          {#if fps}
            <div class="metric"><b>{Math.round(fps.avg)}<small class="text-xs muted"> FPS</small></b><span>{fps.game} · 1% {Math.round(fps.low1)} · {ago(fps.start, true)}</span></div>
            {#if fps.timeline.length > 2}<div class="mt-2"><Spark values={fps.timeline} height={28} /></div>{/if}
          {:else}<div class="text-sm muted">{t('fps.noSessions')}</div>{/if}
        {:else if tile.kind === 'space'}
          <div class="flex flex-col gap-2.5">
            {#each app.drives.slice(0, tile.wide ? 6 : 3) as d (d.mount)}
              <div>
                <div class="flex justify-between text-xs"><span class="truncate">{d.name || d.mount} <span class="muted">{d.mount}</span></span><span class="mono">{fmt(d.available)}</span></div>
                <div class="bar mt-1" style="height:5px"><i style="width:{pct(d)}%;{pct(d) > 90 ? 'background:var(--color-error)' : ''}"></i></div>
              </div>
            {/each}
          </div>
        {:else if tile.kind === 'battery'}
          {#if battery?.present}
            <div class="metric"><b>{battery.wear != null ? Math.round(100 - battery.wear) : '—'}<small class="text-xs muted">%</small></b><span>{t('home.batteryHealth')}{battery.charge != null ? ` · ${Math.round(battery.charge)}% ${battery.charging ? '⚡' : ''}` : ''}</span></div>
          {:else}<div class="text-sm muted">{t('home.noBattery')}</div>{/if}
        {:else if tile.kind === 'backup'}
          {#if backup?.jobs.length}
            <div class="metric"><b class="!text-lg" style={backupErr ? 'color:var(--color-warning)' : ''}>{lastBackup ? ago(lastBackup, true) : t('backup.never')}</b><span>{t('home.backups', { n: backup.jobs.length })}</span></div>
          {:else}<div class="text-sm muted">{t('backup.none')}</div>{/if}
        {:else if tile.kind === 'boot'}
          {@const last = boot?.history[0]}
          <div class="metric"><b>{last ? (last[1] / 1000).toFixed(1) : '—'}<small class="text-xs muted"> s</small></b><span>{t('boot.last')}{boot?.last_boot ? ` · ${ago(boot.last_boot, true)}` : ''}</span></div>
        {:else if tile.kind === 'peers'}
          <div class="metric"><b>{peers ? `${peers.online}/${peers.total}` : '—'}</b><span>{t('home.peersOnline')}</span></div>
        {:else if tile.kind === 'report'}
          <div class="metric"><b class="grad-text">{report ? fmt(report.freed) : '—'}</b><span>{t('home.freedWeek')}{report ? ` · ${t('home.games', { n: report.games.length })}` : ''}</span></div>
        {/if}
      </div>
    </div>
  {/each}
</section>
