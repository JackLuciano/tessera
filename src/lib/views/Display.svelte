<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, type DisplayInfo, type Snapshot } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  let info = $state<DisplayInfo | null>(null);
  let sens = $state<Snapshot | null>(null);
  let busy = $state<string | null>(null);

  async function load() { info = await api.display.info().catch(e => { app.toast(String(e), 'error'); return null; }); }
  onMount(() => {
    load();
    const tick = () => api.sensors().then(s => (sens = s)).catch(() => {});
    tick();
    const timer = setInterval(tick, 2500);
    return () => clearInterval(timer);
  });

  async function fixHz(device: string, hz: number) {
    const ok = await ask({ title: t('disp.fixTitle', { hz }), text: t('disp.fixText'), ico: 'monitor', confirm: t('disp.fix') });
    if (!ok) return;
    busy = device;
    try { await api.display.setRefresh(device, hz); app.toast(t('disp.fixed', { hz }), 'success'); await load(); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function setSwitch(key: string, on: boolean) {
    try { await api.display.setSwitch(key, on); app.toast(t('disp.switchSaved'), 'success'); await load(); } catch (e) { app.toast(String(e), 'error'); }
  }
  /** Before "unpark all", the plan's own value is remembered here so it can be put back. */
  async function setParking(on: boolean) {
    busy = 'park';
    try {
      if (on) {
        try { localStorage.setItem('tessera-parking', String(info?.core_parking ?? 5)); } catch {}
        await api.display.setParking(100);
      } else {
        let prev = 5;
        try { prev = Number(localStorage.getItem('tessera-parking') ?? 5) || 5; } catch {}
        await api.display.setParking(prev);
      }
      await load();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function purge() {
    busy = 'ram';
    try { const freed = await api.display.purgeRam(); app.toast(t('ram.purged', { size: fmt(freed) }), 'success'); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="monitor" title={t('disp.title')} sub={t('disp.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={load}><Icon name="refresh" size="1rem" /></button>
  </PageHead>
</section>

<AdminBanner text={t('disp.needAdmin')} />

{#if info}
  <section class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each info.monitors as m, i (m.device)}
      {@const low = m.hz < m.max_hz}
      <div class="glass p-5 reveal" style="--i:{i + 1}">
        <div class="flex items-center gap-3">
          <span class="w-11 h-11 rounded-xl grid place-items-center soft shrink-0" style="color:var(--color-{low ? 'warning' : 'success'})"><Icon name="monitor" size="1.3rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="font-semibold truncate">{m.name || m.device}{#if m.primary} <span class="tag">{t('disp.primary')}</span>{/if}</div>
            <div class="text-[11px] muted truncate">{m.adapter} · {m.width}×{m.height}</div>
          </div>
        </div>
        <div class="flex items-end gap-6 mt-4">
          <div class="metric"><b style="color:var(--color-{low ? 'warning' : 'success'})">{m.hz} Hz</b><span>{t('disp.current')}</span></div>
          <div class="metric"><b>{m.max_hz} Hz</b><span>{t('disp.max')}</span></div>
          <div class="flex-1"></div>
          {#if low}
            <button class="btn btn-sm btn-primary" disabled={busy === m.device} onclick={() => fixHz(m.device, m.max_hz)}>{t('disp.fixTo', { hz: m.max_hz })}</button>
          {:else}
            <span class="flex items-center gap-1.5 text-xs font-semibold" style="color:var(--color-success)"><i class="dot-ok"></i>{t('disp.ok')}</span>
          {/if}
        </div>
        {#if low}<div class="text-[11px] mt-3" style="color:var(--color-warning)">{t('disp.lowHint', { hz: m.hz, max: m.max_hz })}</div>{/if}
      </div>
    {/each}
  </section>

  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:4">
      <div class="font-semibold">{t('disp.graphics')}</div>
      <div class="text-xs muted">{t('disp.graphicsSub')}</div>
      <div class="divide-y divide-[var(--glass-border)] mt-2">
        {#each [['SwapEffectUpgradeEnable', info.windowed_opt, 'layers'], ['VRROptimizeEnable', info.vrr, 'monitor']] as [key, on, ico] (key)}
          <div class="flex items-center gap-3 py-3">
            <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name={String(ico)} size="1.05rem" /></span>
            <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('disp.sw.' + key)}</div><div class="text-xs muted">{t('disp.sw.' + key + '.hint')}</div></div>
            <input type="checkbox" class="toggle toggle-primary toggle-sm" checked={!!on} onchange={e => setSwitch(String(key), (e.target as HTMLInputElement).checked)} />
          </div>
        {/each}
        <div class="flex items-center gap-3 py-3">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name="cpu" size="1.05rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium flex items-center gap-1.5">{t('disp.parking')}<span class="tag tag-warn"><Icon name="shield" size=".6rem" /></span></div>
            <div class="text-xs muted">{t('disp.parkingHint', { n: info.core_parking ?? '—' })}</div>
          </div>
          {#if busy === 'park'}<span class="loading loading-spinner loading-xs"></span>{/if}
          <input type="checkbox" class="toggle toggle-primary toggle-sm" checked={info.core_parking === 100} disabled={info.core_parking === null || !app.elevated} onchange={e => setParking((e.target as HTMLInputElement).checked)} />
        </div>
        <div class="flex items-center gap-3 py-3">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name="clock" size="1.05rem" /></span>
          <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('disp.timer')}</div><div class="text-xs muted">{t('disp.timerHint')}</div></div>
          <span class="mono text-sm">{info.timer[0].toFixed(1)} ms <span class="muted text-xs">/ {info.timer[1].toFixed(1)}</span></span>
        </div>
      </div>
    </div>

    <div class="glass p-5 reveal" style="--i:5">
      <div class="font-semibold flex items-center gap-2"><Icon name="memory" size="1rem" style="color:var(--color-primary)" />{t('ram.title')}</div>
      <div class="text-xs muted">{t('ram.sub')}</div>
      {#if sens}
        {@const used = sens.ram_used / Math.max(1, sens.ram_total)}
        {@const standby = (sens.ram_standby ?? 0) / Math.max(1, sens.ram_total)}
        <div class="mt-4 h-4 rounded-full overflow-hidden flex soft">
          <i class="block h-full" style="width:{used * 100}%;background:linear-gradient(90deg,var(--color-primary),var(--color-accent))"></i>
          <i class="block h-full" style="width:{Math.min(standby, 1 - used) * 100}%;background:color-mix(in oklch,var(--color-secondary) 55%,transparent)"></i>
        </div>
        <div class="flex flex-wrap gap-4 mt-2 text-xs">
          <span class="flex items-center gap-1.5"><i class="w-2.5 h-2.5 rounded-full grad-bg"></i>{t('ram.inUse')} <b class="mono">{fmt(sens.ram_used)}</b></span>
          <span class="flex items-center gap-1.5"><i class="w-2.5 h-2.5 rounded-full" style="background:var(--color-secondary)"></i>{t('ram.standby')} <b class="mono">{fmt(sens.ram_standby ?? 0)}</b></span>
          <span class="muted">{t('ram.total', { size: fmt(sens.ram_total) })}</span>
        </div>
      {/if}
      <div class="flex items-center gap-3 mt-4">
        <button class="btn btn-sm btn-primary gap-1.5" disabled={busy === 'ram' || !app.elevated} onclick={purge}>{#if busy === 'ram'}<span class="loading loading-spinner loading-xs"></span>{/if}<Icon name="sparkles" size=".9rem" />{t('ram.purge')}</button>
        <span class="text-[11px] muted">{t('ram.hint')}</span>
      </div>
    </div>
  </section>
{/if}
