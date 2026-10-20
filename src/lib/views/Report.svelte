<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, type StatsReport } from '../api';
  import { t } from '../i18n.svelte';
  import { day } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Spark from '../Spark.svelte';

  let days = $state(7);
  let r = $state<StatsReport | null>(null);
  $effect(() => { const d = days; api.report(d).then(v => (r = v)).catch(e => app.toast(String(e), 'error')); });

  let maxDay = $derived(Math.max(1, ...(r?.daily_freed ?? [1])));
  let hours = $derived((r?.games ?? []).reduce((s, g) => s + g[1], 0) / 60);
  let score = $derived(r?.care.at(-1)?.[1] ?? null);
  const TOOL_ICO: Record<string, string> = { junk: 'trash', privacy: 'eyeOff', care: 'heart', files: 'files', shredder: 'shred', uninstaller: 'package', windows: 'cpu', games: 'game' };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="chart" title={t('rep.title')} sub={t('rep.sub')}>
    <div class="join">
      {#each [7, 30] as d (d)}<button class="join-item btn btn-sm {days === d ? 'btn-primary' : ''}" onclick={() => (days = d)}>{t('crash.days', { n: d })}</button>{/each}
    </div>
  </PageHead>
  {#if r}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-5">
      <div class="metric"><b class="grad-text">{fmt(r.freed)}</b><span>{t('rep.freed')}</span></div>
      <div class="metric"><b>{r.games.length}</b><span>{t('rep.games')}</span></div>
      <div class="metric"><b>{hours.toFixed(1)} h</b><span>{t('rep.played')}</span></div>
      <div class="metric"><b>{score !== null ? Math.round(score) : '—'}</b><span>{t('rep.score')}</span></div>
    </div>
  {/if}
</section>

{#if r}
  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:1">
      <div class="font-semibold">{t('rep.daily')}</div>
      <div class="flex items-end gap-1 h-32 mt-4">
        {#each r.daily_freed as b, i (i)}
          <div class="flex-1 rounded-t-md grad-bg share-bar-v" style="height:{Math.max(3, (b / maxDay) * 100)}%;opacity:{b ? 1 : 0.18}" title={fmt(b)}></div>
        {/each}
      </div>
      <div class="flex justify-between text-[10px] muted mt-1"><span>{day(r.from)}</span><span>{day(r.to)}</span></div>
      {#if r.freed_by.length}
        <div class="flex flex-col gap-2 mt-5">
          {#each r.freed_by as [tool, bytes] (tool)}
            <div class="flex items-center gap-3">
              <Icon name={TOOL_ICO[tool] ?? 'sparkles'} size="1rem" style="opacity:.7" />
              <span class="text-sm w-28 truncate">{t('rep.tool.' + tool)}</span>
              <div class="bar flex-1"><i style="width:{(bytes / r.freed_by[0][1]) * 100}%;background:linear-gradient(90deg,var(--color-primary),var(--color-accent))"></i></div>
              <span class="mono text-xs w-20 text-right">{fmt(bytes)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="text-sm muted mt-4">{t('rep.nothingFreed')}</div>
      {/if}
    </div>

    <div class="flex flex-col gap-4">
      <div class="glass p-5 reveal" style="--i:2">
        <div class="font-semibold">{t('rep.gamesTitle')}</div>
        {#if !r.games.length}
          <div class="text-sm muted mt-3">{t('rep.noGames')}</div>
        {:else}
          <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
            {#each r.games as [name, minutes, fps] (name)}
              <div class="flex items-center gap-3 py-2">
                <Icon name="game" size="1rem" style="opacity:.7" />
                <span class="flex-1 min-w-0 truncate text-sm">{name}</span>
                <span class="text-xs muted">{minutes >= 60 ? `${(minutes / 60).toFixed(1)} h` : `${Math.round(minutes)} min`}</span>
                {#if fps}<span class="mono text-sm font-semibold w-20 text-right">{Math.round(fps)} <span class="text-xs muted">FPS</span></span>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      <div class="glass p-5 reveal" style="--i:3">
        <div class="font-semibold">{t('rep.careHistory')}</div>
        {#if r.care.length > 1}
          <div class="mt-3"><Spark values={r.care.map(c => c[1])} min={0} max={100} height={80} color="var(--color-success)" dots /></div>
        {:else}
          <div class="text-sm muted mt-3">{t('rep.careWait')}</div>
        {/if}
      </div>
    </div>
  </section>
{/if}
