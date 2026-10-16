<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, type SnapshotInfo, type DiffRow } from '../api';
  import { t, i18n } from '../i18n.svelte';
  import Icon from '../Icon.svelte';

  let snaps = $state<SnapshotInfo[]>([]);
  let sel = $state<number | null>(null);
  let diff = $state<DiffRow[]>([]);
  let wrap: HTMLDivElement | undefined = $state();
  let W = $state(800);
  const H = 260;
  let hover = $state<number | null>(null);

  $effect(() => {
    void app.version;
    if (!app.summary) return;
    api.historyList(app.summary.root).then(s => {
      snaps = s;
      // Default comparison: the snapshot before the latest one
      sel = s.length > 1 ? s[s.length - 2].ts : null;
    });
  });
  $effect(() => {
    if (sel === null) { diff = []; return; }
    api.historyDiff(sel).then(d => (diff = d)).catch(() => (diff = []));
  });
  $effect(() => {
    if (!wrap) return;
    const ro = new ResizeObserver(() => (W = wrap!.clientWidth));
    ro.observe(wrap);
    return () => ro.disconnect();
  });

  const date = (ts: number) => new Date(ts * 1000).toLocaleString(i18n.locale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  let cap = $derived(app.drive?.total ?? 0);
  let chart = $derived.by(() => {
    if (snaps.length < 2) return null;
    const pad = { l: 56, r: 12, t: 14, b: 26 };
    const t0 = snaps[0].ts, t1 = snaps[snaps.length - 1].ts || t0 + 1;
    const vals = snaps.map(s => s.size);
    const lo = Math.min(...vals) * 0.98, hi = Math.max(...vals, cap && Math.max(...vals) > cap * 0.8 ? cap : 0) * 1.02;
    const X = (ts: number) => pad.l + ((ts - t0) / Math.max(1, t1 - t0)) * (W - pad.l - pad.r);
    const Y = (v: number) => pad.t + (1 - (v - lo) / Math.max(1, hi - lo)) * (H - pad.t - pad.b);
    const pts = snaps.map(s => [X(s.ts), Y(s.size)] as const);
    const line = pts.map((p, i) => (i ? 'L' : 'M') + p[0].toFixed(1) + ' ' + p[1].toFixed(1)).join('');
    const ticks = [0, 0.5, 1].map(f => lo + (hi - lo) * f);
    return { pad, X, Y, pts, line, ticks, capY: cap && cap <= hi ? Y(cap) : null };
  });
  let maxDelta = $derived(Math.max(1, ...diff.map(d => Math.abs(d.delta))));
  let grew = $derived(diff.filter(d => d.delta > 0).reduce((s, d) => s + d.delta, 0));
  let shrank = $derived(diff.filter(d => d.delta < 0).reduce((s, d) => s - d.delta, 0));
</script>

{#if !app.summary}
  <div class="glass p-10 text-center muted">{t('common.scanFirst')}</div>
{:else}
  <section class="glass p-4 reveal">
    <div class="font-semibold">{t('history.usage')}</div>
    <div class="text-xs muted">{t('history.usageSub', { n: snaps.length })}</div>
    <div bind:this={wrap} class="relative mt-3" style="height:{H}px">
      {#if chart}
        <svg viewBox="0 0 {W} {H}" class="w-full h-full overflow-visible" role="img" aria-label={t('history.usage')}
          onmousemove={e => { const b = (e.currentTarget as SVGElement).getBoundingClientRect(); const x = e.clientX - b.left; let best = 0; chart!.pts.forEach((p, i) => { if (Math.abs(p[0] - x) < Math.abs(chart!.pts[best][0] - x)) best = i; }); hover = best; }}
          onmouseleave={() => (hover = null)}>
          <defs>
            <linearGradient id="hFill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" style="stop-color:var(--color-primary);stop-opacity:.35" /><stop offset="1" style="stop-color:var(--color-primary);stop-opacity:0" /></linearGradient>
          </defs>
          {#each chart.ticks as v (v)}
            <line x1={chart.pad.l} x2={W - chart.pad.r} y1={chart.Y(v)} y2={chart.Y(v)} stroke="var(--glass-border)" />
            <text x={chart.pad.l - 8} y={chart.Y(v) + 4} text-anchor="end" font-size="10" fill="var(--muted)">{fmt(v)}</text>
          {/each}
          {#if chart.capY !== null}
            <line x1={chart.pad.l} x2={W - chart.pad.r} y1={chart.capY} y2={chart.capY} stroke="var(--color-error)" stroke-dasharray="4 4" opacity=".7" />
          {/if}
          <path d="{chart.line}L{chart.pts.at(-1)![0]} {H - chart.pad.b}L{chart.pts[0][0]} {H - chart.pad.b}Z" fill="url(#hFill)" />
          <path d={chart.line} fill="none" stroke="var(--color-primary)" stroke-width="2.5" stroke-linejoin="round" />
          {#each chart.pts as p, i (i)}
            <circle cx={p[0]} cy={p[1]} r={snaps[i].ts === sel || hover === i ? 5 : 3} fill="var(--color-primary)" stroke="var(--color-base-100)" stroke-width="2" />
          {/each}
          {#if hover !== null}
            <line x1={chart.pts[hover][0]} x2={chart.pts[hover][0]} y1={chart.pad.t} y2={H - chart.pad.b} stroke="var(--muted)" stroke-dasharray="3 3" />
          {/if}
        </svg>
        {#if hover !== null}
          <div class="tt glass" style="left:{Math.min(chart.pts[hover][0] + 12, W - 190)}px;top:{Math.max(0, chart.pts[hover][1] - 70)}px">
            <div class="muted text-xs">{date(snaps[hover].ts)}</div><div class="mono font-bold">{fmt(snaps[hover].size)}</div>
          </div>
        {/if}
      {:else}
        <div class="h-full grid place-items-center text-center muted text-sm"><div><Icon name="history" size="2rem" /><div class="mt-2">{t('history.needTwo')}</div></div></div>
      {/if}
    </div>
  </section>

  <section class="grid grid-cols-1 xl:grid-cols-12 gap-4">
    <div class="glass p-4 xl:col-span-4 reveal" style="--i:1">
      <div class="font-semibold mb-1">{t('history.snapshots')}</div>
      <div class="text-xs muted mb-2">{t('history.pick')}</div>
      <div class="flex flex-col gap-1 max-h-96 overflow-y-auto">
        {#each [...snaps].reverse() as s, i (s.ts)}
          {@const d = (app.summary?.size ?? 0) - s.size}
          <button class="row-hover {s.ts === sel ? 'hot' : ''} flex items-center gap-3 px-2.5 py-2 text-left" disabled={i === 0} onclick={() => (sel = s.ts)}>
            <span class="w-2 h-2 rounded-full shrink-0" style="background:{s.ts === sel ? 'var(--color-primary)' : 'var(--muted)'}"></span>
            <div class="flex-1 min-w-0"><div class="text-sm font-medium">{i === 0 ? t('history.latest') : date(s.ts)}</div><div class="text-[11px] muted mono">{fmt(s.size)}</div></div>
            {#if i > 0}<span class="mono text-xs" style="color:{d > 0 ? 'var(--color-warning)' : 'var(--color-success)'}">{d > 0 ? '+' : '−'}{fmt(Math.abs(d))}</span>{/if}
          </button>
        {/each}
      </div>
    </div>
    <div class="glass p-4 xl:col-span-8 reveal" style="--i:2">
      <div class="flex items-center justify-between mb-3 flex-wrap gap-2">
        <div><div class="font-semibold">{t('history.changed')}</div><div class="text-xs muted">{sel ? t('history.vs', { date: date(sel) }) : '—'}</div></div>
        <div class="flex gap-3 text-sm mono"><span style="color:var(--color-warning)">+{fmt(grew)}</span><span style="color:var(--color-success)">−{fmt(shrank)}</span></div>
      </div>
      {#if !diff.length}<div class="muted text-sm p-8 text-center">{t('history.noChanges')}</div>{/if}
      {#each diff as r (r.path)}
        <div class="grid items-center gap-3 px-2 py-1.5 text-sm row-hover" style="grid-template-columns:minmax(0,1.4fr) 1fr 80px">
          <span class="truncate mono text-xs selectable">{r.path}</span>
          <div class="flex {r.delta > 0 ? '' : 'justify-end'}"><div class="bar w-full" style="height:8px;background:transparent;{r.delta > 0 ? '' : 'transform:scaleX(-1)'}"><i style="width:{(Math.abs(r.delta) / maxDelta) * 100}%;background:{r.delta > 0 ? 'var(--color-warning)' : 'var(--color-success)'}"></i></div></div>
          <span class="mono text-right text-xs" style="color:{r.delta > 0 ? 'var(--color-warning)' : 'var(--color-success)'}">{r.delta > 0 ? '+' : '−'}{fmt(Math.abs(r.delta))}</span>
        </div>
      {/each}
    </div>
  </section>
{/if}
