<script lang="ts">
  import { onMount } from 'svelte';
  import { api, fmt, ago, type SensorHistory, type Snapshot } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';
  import Spark from '../Spark.svelte';
  import { app } from '../store.svelte';

  let h = $state<SensorHistory | null>(null);
  /** The optional PawnIO driver for the real CPU temperature. */
  let drv = $state<{ supported: boolean; installed: string | null } | null>(null);
  let installing = $state(false);
  let reboot = $state(false);

  async function installDriver() {
    installing = true;
    try {
      reboot = await api.cpuDriverInstall();
      drv = await api.cpuDriver();
      app.toast(t(reboot ? 'temps.drvReboot' : 'temps.drvDone'), reboot ? 'info' : 'success');
    } catch (e) {
      if (String(e) !== 'cancelled') app.toast(String(e), 'error');
    } finally {
      installing = false;
    }
  }

  onMount(() => {
    api.cpuDriver().then(v => (drv = v)).catch(() => {});
    const tick = () => api.sensorHistory().then(v => (h = v)).catch(() => {});
    tick();
    const timer = setInterval(tick, 2000);
    return () => clearInterval(timer);
  });

  let now = $derived<Snapshot | null>(h?.history.at(-1) ?? null);
  let gpu = $derived(now?.gpus[0] ?? null);
  const series = (pick: (s: Snapshot) => number | null | undefined) => (h?.history ?? []).map(s => pick(s) ?? null);
  let rise = $derived(Math.max(h?.trend.cpu_rise ?? 0, h?.trend.gpu_rise ?? 0));
  const dur = (ms: number) => (ms < 60_000 ? `${Math.max(2, Math.round(ms / 1000))} s` : `${Math.round(ms / 60_000)} min`);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="thermo" title={t('temps.title')} sub={t('temps.sub')} />
  <div class="flex flex-wrap justify-around gap-6 mt-5">
    <Ring value={now?.cpu_temp ?? null} max={100} label={t('temps.cpuTemp')} sub={now?.cpu_temp_approx ? t('temps.approx') : ''} unit="°C" warn={75} bad={90} />
    <Ring value={gpu?.temp ?? null} max={100} label={t('temps.gpuTemp')} sub={gpu?.name?.replace(/NVIDIA GeForce |AMD Radeon /, '') ?? ''} unit="°C" warn={75} bad={85} />
    <Ring value={now?.cpu_load ?? null} label={t('temps.cpuLoad')} unit="%" warn={85} bad={97} />
    <Ring value={gpu?.load ?? null} label={t('temps.gpuLoad')} unit="%" warn={90} bad={99} />
  </div>
  {#if now && (now.cpu_temp === null || now.cpu_temp_approx)}
    <div class="flex flex-col items-center gap-2 mt-3 text-center">
      {#if now.cpu_temp === null}<div class="text-[11px] muted">{t('temps.noCpuTemp')}</div>{/if}
      {#if drv?.supported && !drv.installed && !reboot}
        <button class="btn btn-sm btn-primary gap-1.5" disabled={installing} onclick={installDriver}>
          {#if installing}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="thermo" size=".9rem" />{/if}{t('temps.drvInstall')}
        </button>
        <div class="text-[11px] muted max-w-md">{t('temps.drvNote')}</div>
      {:else if reboot}
        <div class="text-[11px]" style="color:var(--color-info)">{t('temps.drvReboot')}</div>
      {:else if drv?.installed && !app.elevated}
        <div class="text-[11px] muted">{t('temps.drvAdmin')}</div>
        <button class="btn btn-xs btn-ghost gap-1" onclick={() => api.relaunchAsAdmin().catch(e => app.toast(String(e), 'error'))}><Icon name="shield" size=".8rem" />{t('temps.drvRelaunch')}</button>
      {:else if drv?.installed}
        <div class="text-[11px] muted">{t('temps.drvWaiting')}</div>
      {/if}
    </div>
  {/if}
</section>

<section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
  <div class="glass p-5 reveal" style="--i:1">
    <div class="font-semibold">{t('temps.cpu')}</div>
    <div class="grid grid-cols-3 gap-3 mt-3">
      <div class="metric"><b>{now?.cpu_mhz ? (now.cpu_mhz / 1000).toFixed(2) : '—'}<small class="text-xs muted"> GHz</small></b><span>{t('temps.clock')}{now?.cpu_base_mhz ? ` · ${t('temps.base')} ${(now.cpu_base_mhz / 1000).toFixed(1)}` : ''}</span></div>
      <div class="metric"><b style={now?.cpu_limit != null && now.cpu_limit < 90 ? 'color:var(--color-warning)' : ''}>{now?.cpu_limit != null ? Math.round(now.cpu_limit) : '—'}<small class="text-xs muted"> %</small></b><span>{t('temps.limit')}</span></div>
      <div class="metric"><b>{fmt(now?.ram_used ?? 0)}</b><span>{t('temps.ram', { total: fmt(now?.ram_total ?? 0) })}</span></div>
    </div>
    <div class="text-[11px] muted mt-4 mb-1">{t('temps.load10')}</div>
    <Spark values={series(s => s.cpu_load)} max={100} height={60} />
    {#if series(s => s.cpu_temp).some(v => v !== null)}
      <div class="text-[11px] muted mt-3 mb-1">{t('temps.temp10')}</div>
      <Spark values={series(s => s.cpu_temp)} min={20} max={100} height={50} color="var(--color-warning)" />
    {/if}
  </div>

  <div class="glass p-5 reveal" style="--i:2">
    <div class="font-semibold truncate">{gpu?.name ?? t('temps.gpu')}</div>
    <div class="grid grid-cols-3 gap-3 mt-3">
      <div class="metric"><b>{gpu?.mhz != null ? Math.round(gpu.mhz) : '—'}<small class="text-xs muted"> MHz</small></b><span>{t('temps.clock')}{gpu?.max_mhz ? ` · max ${Math.round(gpu.max_mhz)}` : ''}</span></div>
      <div class="metric"><b>{gpu?.power != null ? Math.round(gpu.power) : '—'}<small class="text-xs muted"> W</small></b><span>{t('temps.power')}{gpu?.power_limit ? ` · ${t('temps.of')} ${Math.round(gpu.power_limit)} W` : ''}</span></div>
      <div class="metric"><b>{gpu?.fan != null ? Math.round(gpu.fan) : '—'}<small class="text-xs muted"> %</small></b><span>{t('temps.fan')}</span></div>
    </div>
    {#if gpu?.mem_total}
      <div class="mt-3">
        <div class="flex justify-between text-[11px]"><span class="muted">{t('temps.vram')}</span><span class="mono">{fmt(gpu.mem_used ?? 0)} / {fmt(gpu.mem_total)}</span></div>
        <div class="bar mt-1"><i style="width:{((gpu.mem_used ?? 0) / gpu.mem_total) * 100}%;background:linear-gradient(90deg,var(--color-secondary),var(--color-primary))"></i></div>
      </div>
    {/if}
    <div class="text-[11px] muted mt-4 mb-1">{t('temps.load10')}</div>
    <Spark values={series(s => s.gpus[0]?.load)} max={100} height={60} color="var(--color-accent)" />
    {#if series(s => s.gpus[0]?.temp).some(v => v !== null)}
      <div class="text-[11px] muted mt-3 mb-1">{t('temps.temp10')}</div>
      <Spark values={series(s => s.gpus[0]?.temp)} min={20} max={100} height={50} color="var(--color-warning)" />
    {/if}
    {#if gpu && gpu.temp === null}<div class="text-[11px] muted mt-3">{t('temps.noGpuTemp')}</div>{/if}
  </div>
</section>

<section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
  <div class="glass p-5 reveal" style="--i:3">
    <div class="font-semibold flex items-center gap-2"><Icon name="alert" size="1rem" style="color:var(--color-warning)" />{t('temps.throttle')}</div>
    <div class="text-xs muted">{t('temps.throttleSub')}</div>
    {#if !h?.throttle.length}
      <div class="flex items-center gap-2 text-sm mt-4" style="color:var(--color-success)"><i class="dot-ok"></i>{t('temps.noThrottle')}</div>
    {:else}
      <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
        {#each [...h.throttle].reverse().slice(0, 10) as e (e.start + e.kind)}
          <div class="flex items-center gap-3 py-2">
            <span class="tag tag-warn">{t('temps.ev.' + e.kind)}</span>
            <span class="text-xs muted flex-1">{ago(Math.round(e.start / 1000), true)} · {dur(e.end - e.start)}</span>
            <span class="mono text-xs">{e.kind === 'cpu_limit' ? `${Math.round(e.worst)}%` : e.kind === 'gpu_power' ? `${Math.round(e.worst)} W` : `${Math.round(e.worst)}°C`}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="glass p-5 reveal" style="--i:4">
    <div class="font-semibold flex items-center gap-2"><Icon name="fan" size="1rem" style="color:var(--color-primary)" />{t('temps.trend')}</div>
    <div class="text-xs muted">{t('temps.trendSub')}</div>
    {#if (h?.trend.days.length ?? 0) < 3}
      <div class="text-sm muted mt-4">{t('temps.trendWait')}</div>
    {:else}
      <div class="mt-3"><Spark values={h!.trend.days.map(d => d[1] ?? d[2])} min={20} max={90} height={70} color="var(--color-warning)" dots /></div>
      <div class="flex justify-between text-[10px] muted mt-1"><span>{h!.trend.days[0][0]}</span><span>{h!.trend.days.at(-1)![0]}</span></div>
      {#if rise >= 8}
        <div class="rounded-xl p-3 mt-3 text-sm flex gap-2 items-start" style="background:color-mix(in oklch,var(--color-warning) 12%,transparent)"><Icon name="fan" size="1rem" style="color:var(--color-warning)" />{t('temps.dust', { n: Math.round(rise) })}</div>
      {:else if h!.trend.cpu_rise !== null || h!.trend.gpu_rise !== null}
        <div class="flex items-center gap-2 text-sm mt-3" style="color:var(--color-success)"><i class="dot-ok"></i>{t('temps.trendOk')}</div>
      {/if}
    {/if}
    {#if now && (now.sensors.length || now.fans.length)}
      <div class="text-[11px] uppercase tracking-wider muted font-semibold mt-5">{t('temps.other')}</div>
      <div class="grid grid-cols-2 gap-x-4 gap-y-1 mt-2 text-xs">
        {#each now.sensors as [l, v] (l)}<div class="flex justify-between gap-2"><span class="truncate muted">{l}</span><span class="mono">{v.toFixed(0)}°C</span></div>{/each}
        {#each now.fans as [l, v] (l)}<div class="flex justify-between gap-2"><span class="truncate muted">{l}</span><span class="mono">{Math.round(v)} rpm</span></div>{/each}
      </div>
    {/if}
  </div>
</section>
