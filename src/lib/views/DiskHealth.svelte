<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, num, type HealthReport } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  let rep = $state<HealthReport | null>(null);
  let busy = $state<string | null>(null);

  async function load() {
    rep = null;
    rep = await api.diskHealth().catch(e => { app.toast(String(e), 'error'); return { disks: [], counters_need_admin: false, can_optimize: false }; });
  }
  onMount(load);

  async function optimize(v: string, media: string) {
    busy = v;
    app.toast(t(media === 'ssd' ? 'health.trimming' : 'health.defragging', { v }), 'info');
    try { await api.diskOptimize(v); app.toast(t('health.optimized', { v }), 'success'); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }

  const tone = (h: string) => (h === 'healthy' ? 'success' : h === 'warning' ? 'warning' : h === 'unhealthy' ? 'error' : 'info');
  const volume = (v: string) => app.drives.find(d => d.mount.toUpperCase().startsWith(v.toUpperCase()) || d.mount === v);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="thermo" title={t('health.title')} sub={t('health.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={load}><Icon name="refresh" size="1rem" /></button>
  </PageHead>
</section>

{#if rep?.counters_need_admin}<AdminBanner text={t('health.needAdmin')} />{/if}

{#if rep === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !rep.disks.length}
  <div class="glass p-10 text-center muted">{t('health.none')}</div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    {#each rep.disks as d, i (d.id)}
      <div class="glass p-5 reveal" style="--i:{i + 2}">
        <div class="flex items-center gap-3">
          <span class="w-11 h-11 rounded-xl grid place-items-center soft shrink-0" style="color:var(--color-{tone(d.health)})"><Icon name="drive" size="1.3rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="font-semibold truncate">{d.name}</div>
            <div class="flex gap-1 mt-0.5 flex-wrap">
              <span class="tag">{d.media === 'unknown' ? '?' : d.media.toUpperCase()}</span>
              {#if d.bus}<span class="tag">{d.bus}</span>{/if}
              <span class="tag">{fmt(d.size)}</span>
            </div>
          </div>
          <span class="badge badge-soft badge-{tone(d.health)} gap-1.5"><i class={d.health === 'healthy' ? 'dot-ok' : d.health === 'unhealthy' ? 'dot-bad' : 'dot-warn'}></i>{t('health.' + d.health)}</span>
        </div>
        <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 mt-4">
          <div class="tile-stat"><div>{t('health.temp')}</div><div style={d.temperature && d.temperature >= 60 ? 'color:var(--color-warning)' : ''}>{d.temperature !== null ? `${d.temperature} °C` : '—'}</div></div>
          <div class="tile-stat"><div>{t('health.wear')}</div><div>{d.wear !== null ? `${d.wear}%` : '—'}</div></div>
          <div class="tile-stat"><div>{t('health.hours')}</div><div>{d.power_on_hours !== null ? num(d.power_on_hours) : '—'}</div></div>
          <div class="tile-stat"><div>{t('health.errors')}</div><div>{d.read_errors !== null ? num(d.read_errors) : '—'}</div></div>
        </div>
        {#if d.volumes.length}
          <div class="flex flex-col gap-2 mt-4">
            {#each d.volumes as v (v)}
              {@const dr = volume(v)}
              {@const p = dr ? ((dr.total - dr.available) / dr.total) * 100 : 0}
              <div class="flex items-center gap-3">
                <span class="mono text-sm w-16 truncate" title={v}>{v}</span>
                <div class="flex-1">
                  <div class="bar"><i style="width:{p}%;background:{p > 90 ? 'var(--color-warning)' : 'linear-gradient(90deg,var(--color-primary),var(--color-accent))'}"></i></div>
                  {#if dr}<div class="text-[11px] muted mono mt-0.5">{t('drive.freeOf', { free: fmt(dr.available), total: fmt(dr.total) })}</div>{/if}
                </div>
                {#if rep.can_optimize && d.media !== 'unknown'}
                  <button class="btn btn-xs gap-1" disabled={!!busy} onclick={() => optimize(v, d.media)}>
                    {#if busy === v}<span class="loading loading-spinner loading-xs"></span>{/if}{d.media === 'ssd' ? t('health.trim') : t('health.defrag')}
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </section>
{/if}
