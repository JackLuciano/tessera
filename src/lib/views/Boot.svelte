<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type BootInfo } from '../api';
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
