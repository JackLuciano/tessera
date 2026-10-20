<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type Driver } from '../api';
  import { t } from '../i18n.svelte';
  import { day, openSystem } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let list = $state<Driver[] | null>(null);
  onMount(() => { api.drivers().then(v => (list = v)).catch(e => { app.toast(String(e), 'error'); list = []; }); });

  const age = (d: Driver) => (d.date ? Math.floor((Date.now() / 1000 - d.date) / 86400) : null);
  const VENDOR: Record<string, string> = { nvidia: 'oklch(70% 0.19 140)', amd: 'oklch(62% 0.22 25)', intel: 'oklch(62% 0.15 245)', other: 'var(--color-primary)' };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="cpu" title={t('drv.title')} sub={t('drv.sub')}>
    <button class="btn btn-sm gap-1.5" onclick={() => openSystem('ms-settings:windowsupdate-optionalupdates')}><Icon name="download" size=".9rem" />{t('drv.optional')}</button>
  </PageHead>
</section>

{#if list === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else}
  <section class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each list as d, i (d.name + d.version)}
      {@const a = age(d)}
      {@const old = a !== null && a > 120}
      <div class="glass p-5 reveal" style="--i:{i + 1}">
        <div class="flex items-center gap-3">
          <span class="w-11 h-11 rounded-xl grid place-items-center text-white font-bold shrink-0" style="background:{VENDOR[d.vendor]}">{d.vendor === 'other' ? 'GPU' : d.vendor.toUpperCase().slice(0, 3)}</span>
          <div class="flex-1 min-w-0">
            <div class="font-semibold truncate">{d.name}</div>
            <div class="text-[11px] muted">{t('drv.graphics')}</div>
          </div>
          <span class="text-xs font-semibold flex items-center gap-1.5" style="color:var(--color-{old ? 'warning' : 'success'})"><i class={old ? 'dot-warn' : 'dot-ok'}></i>{old ? t('drv.old') : t('drv.recent')}</span>
        </div>
        <div class="grid grid-cols-3 gap-3 mt-4">
          <div class="metric"><b class="!text-lg">{d.display_version}</b><span>{t('drv.version')}</span></div>
          <div class="metric"><b class="!text-lg">{day(d.date ?? 0)}</b><span>{t('drv.date')}</span></div>
          <div class="metric"><b class="!text-lg">{a !== null ? t('drv.days', { n: a }) : '—'}</b><span>{t('drv.age')}</span></div>
        </div>
        {#if old}<div class="text-xs mt-3" style="color:var(--color-warning)">{t('drv.oldHint', { n: a ?? 0 })}</div>{/if}
        <div class="flex justify-end mt-3">
          <button class="btn btn-sm {old ? 'btn-primary' : 'btn-ghost'} gap-1.5" onclick={() => openSystem(d.download)}>{t('drv.get')}<Icon name="external" size=".8rem" /></button>
        </div>
      </div>
    {:else}
      <div class="glass p-10 text-center muted">{t('drv.none')}</div>
    {/each}
  </section>
{/if}
