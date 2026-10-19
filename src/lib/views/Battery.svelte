<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Battery } from '../api';
  import { t } from '../i18n.svelte';
  import { day } from '../sys';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';
  import Spark from '../Spark.svelte';
  import Icon from '../Icon.svelte';

  let b = $state<Battery | null>(null);
  let history = $state<[number, number, number][]>([]);
  let loaded = $state(false);

  onMount(async () => {
    const r = await api.battery().catch(() => null);
    b = r?.battery ?? null;
    history = r?.history ?? [];
    loaded = true;
  });

  let health = $derived(b?.wear != null ? 100 - b.wear : null);
  const wh = (mwh: number) => `${(mwh / 1000).toFixed(1)} Wh`;
</script>

<section class="glass p-5 reveal">
  <PageHead ico="battery" title={t('bat.title')} sub={t('bat.sub')} />
</section>

{#if !loaded}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !b?.present}
  <div class="glass p-10 text-center reveal"><Icon name="power" size="2rem" style="opacity:.5" /><div class="mt-2 font-medium">{t('bat.none')}</div><div class="text-xs muted">{t('bat.noneSub')}</div></div>
{:else}
  <section class="glass p-6 reveal flex flex-wrap items-center gap-8" style="--i:1">
    <Ring value={health} label={t('bat.health')} unit="%" size={150} warn={80} bad={60} invert />
    <div class="flex-1 min-w-64 grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="metric"><b>{wh(b.full)}</b><span>{t('bat.full')}</span></div>
      <div class="metric"><b>{wh(b.design)}</b><span>{t('bat.design')}</span></div>
      <div class="metric"><b>{b.cycles ?? '—'}</b><span>{t('bat.cycles')}</span></div>
      <div class="metric"><b>{b.charge != null ? `${Math.round(b.charge)}%` : '—'}</b><span>{b.charging ? t('bat.charging') : t('bat.charge')}</span></div>
    </div>
  </section>
  <section class="glass p-5 reveal" style="--i:2">
    <div class="font-semibold">{t('bat.history')}</div>
    <div class="text-xs muted">{b.name}</div>
    {#if history.length > 1}
      <div class="mt-3"><Spark values={history.map(h => (h[1] / Math.max(1, h[2])) * 100)} min={50} max={100} height={90} color="var(--color-success)" dots /></div>
      <div class="flex justify-between text-[10px] muted mt-1"><span>{day(history[0][0])}</span><span>{day(history.at(-1)![0])}</span></div>
    {:else}
      <div class="text-sm muted mt-3">{t('bat.historyWait')}</div>
    {/if}
    {#if (b.wear ?? 0) >= 30}<div class="rounded-xl p-3 mt-4 text-sm" style="background:color-mix(in oklch,var(--color-warning) 12%,transparent)">{t('bat.worn', { n: Math.round(b.wear ?? 0) })}</div>{/if}
  </section>
{/if}
