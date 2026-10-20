<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, fmt, type BootInfo } from '../api';
  import { t } from '../i18n.svelte';
  import { day } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';
  import Spark from '../Spark.svelte';

  let b = $state<BootInfo | null>(null);
  onMount(() => { api.boot().then(v => (b = v)).catch(e => app.toast(String(e), 'error')); });

  const sec = (ms: number) => `${(ms / 1000).toFixed(1)} s`;
  let latest = $derived(b?.history[0] ?? null);
  let avg = $derived(b?.history.length ? b.history.reduce((s, h) => s + h[1], 0) / b.history.length : null);
  let runs = $derived((b?.runs ?? []).slice(0, 8));
  let maxTotal = $derived(Math.max(1, ...runs.map(r => r.total)));
  let open = $state<number | null>(null);
  const KIND_ICON: Record<string, string> = { app: 'app', driver: 'cpu', service: 'layers', other: 'info' };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="clock" title={t('boot.title')} sub={t('boot.sub')}>
    <button class="btn btn-sm gap-1.5" onclick={() => (app.view = 'startup')}><Icon name="power" size=".9rem" />{t('boot.startupApps')}</button>
  </PageHead>
  {#if b}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-5">
      <div class="metric"><b>{latest ? sec(latest[1]) : '—'}</b><span>{t('boot.last')}</span></div>
      <div class="metric"><b>{avg ? sec(avg) : '—'}</b><span>{t('boot.avg')}</span></div>
      <div class="metric"><b>{latest ? sec(latest[2]) : '—'}</b><span>{t('boot.mainPath')}</span></div>
      <div class="metric"><b>{b.last_boot ? ago(b.last_boot, true) : '—'}</b><span>{t('boot.since')}</span></div>
    </div>
  {/if}
</section>

{#if b?.needs_admin}<AdminBanner text={t('boot.needAdmin')} />{/if}

{#if b && b.history.length}
  <section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:2">
      <div class="font-semibold">{t('boot.history')}</div>
      <div class="text-xs muted">{t('boot.historySub')}</div>
      <div class="mt-3"><Spark values={[...b.history].reverse().map(h => h[1] / 1000)} min={0} height={100} dots /></div>
      <div class="flex justify-between text-[10px] muted mt-1"><span>{day(b.history.at(-1)![0])}</span><span>{day(b.history[0][0])}</span></div>
    </div>
    <div class="glass p-5 reveal" style="--i:3">
      <div class="font-semibold">{t('boot.slow')}</div>
      <div class="text-xs muted">{t('boot.slowSub')}</div>
      {#if !b.slow.length}
        <div class="flex items-center gap-2 text-sm mt-4" style="color:var(--color-success)"><i class="dot-ok"></i>{t('boot.noSlow')}</div>
      {:else}
        <div class="flex flex-col gap-2 mt-3">
          {#each b.slow as [name, ms] (name)}
            <div>
              <div class="flex justify-between text-sm gap-2"><span class="truncate">{name}</span><span class="mono text-xs">+{sec(ms)}</span></div>
              <div class="bar mt-1" style="height:5px"><i style="width:{(ms / b.slow[0][1]) * 100}%;background:var(--color-warning)"></i></div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </section>
{/if}

{#if runs.length}
  <section class="glass p-5 reveal" style="--i:4">
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div><div class="font-semibold">{t('boot.timeline')}</div><div class="text-xs muted">{t('boot.timelineSub')}</div></div>
      <div class="flex gap-3 text-[11px] muted">
        <span class="flex items-center gap-1.5"><i class="w-2.5 h-2.5 rounded-sm inline-block" style="background:var(--color-primary)"></i>{t('boot.mainPath')}</span>
        <span class="flex items-center gap-1.5"><i class="w-2.5 h-2.5 rounded-sm inline-block" style="background:var(--color-accent)"></i>{t('boot.post')}</span>
      </div>
    </div>
    <div class="flex flex-col gap-1 mt-4">
      {#each runs as r (r.at)}
        <div>
          <button class="w-full flex items-center gap-3 py-1.5 text-left rounded-lg hover:bg-[var(--soft)] px-1" onclick={() => (open = open === r.at ? null : r.at)}>
            <span class="text-xs muted w-24 shrink-0">{day(r.at)}</span>
            <span class="flex-1 min-w-0 flex h-3.5 rounded-full overflow-hidden" style="background:color-mix(in oklch,var(--color-base-content) 6%,transparent)">
              <i style="width:{(r.main / maxTotal) * 100}%;background:var(--color-primary)"></i>
              <i style="width:{(Math.max(0, r.total - r.main) / maxTotal) * 100}%;background:var(--color-accent);opacity:.75"></i>
            </span>
            <span class="mono text-xs w-14 text-right shrink-0">{sec(r.total)}</span>
            <span class="text-[11px] muted w-16 shrink-0 text-right">{r.items.length ? t('boot.nSlow', { n: r.items.length }) : ''}</span>
          </button>
          {#if open === r.at}
            <div class="pl-28 pr-2 pb-2 flex flex-col gap-1.5">
              {#if !r.items.length}
                <div class="text-xs muted">{t('boot.cleanRun')}</div>
              {:else}
                {#each r.items as [name, ms, kind] (name)}
                  <div class="flex items-center gap-2 text-xs">
                    <Icon name={KIND_ICON[kind] ?? 'info'} size=".8rem" style="opacity:.7" />
                    <span class="flex-1 min-w-0 truncate">{name}</span>
                    <span class="muted">{t('boot.kind.' + kind)}</span>
                    <span class="mono w-16 text-right" style="color:var(--color-warning)">+{sec(ms)}</span>
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

{#if b?.startup?.length}
  <section class="glass p-5 reveal" style="--i:5">
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div><div class="font-semibold">{t('boot.startupNow')}</div><div class="text-xs muted">{t('boot.startupNowSub')}</div></div>
      <button class="btn btn-sm btn-ghost border hairline gap-1.5" onclick={() => (app.view = 'startup')}><Icon name="power" size=".9rem" />{t('boot.manage')}</button>
    </div>
    <div class="relative mt-5 mb-2 h-10">
      {#each b.startup as s, i (s.id)}
        {@const last = Math.max(60, ...b.startup.map(x => x.after))}
        <span class="absolute top-0 -translate-x-1/2 flex flex-col items-center" style="left:{4 + (s.after / last) * 92}%">
          <i class="w-2.5 h-2.5 rounded-full" style="background:var(--color-{i % 2 ? 'accent' : 'primary'})"></i>
          <span class="text-[10px] muted whitespace-nowrap mt-1" style="transform:translateY({(i % 2) * 12}px)">{s.name}</span>
        </span>
      {/each}
      <div class="absolute left-[4%] right-[4%] top-[4px] h-px" style="background:var(--glass-border)"></div>
    </div>
    <div class="flex flex-col mt-4 divide-y divide-[var(--glass-border)]">
      {#each b.startup as s (s.id)}
        <div class="flex items-center gap-3 py-2 text-sm">
          <span class="flex-1 min-w-0 truncate">{s.name}</span>
          <span class="text-xs muted">{t('boot.startedAfter', { s: s.after })}</span>
          <span class="mono text-xs w-20 text-right">{fmt(s.memory)}</span>
        </div>
      {/each}
    </div>
  </section>
{/if}
