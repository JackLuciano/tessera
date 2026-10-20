<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api, type FpsSample, type Snapshot } from '../api';
  import { t } from '../i18n.svelte';
  import Spark from '../Spark.svelte';

  // The FPS window: live frame rate and frame times from PresentMon, plus CPU/GPU and ping
  let s = $state<FpsSample | null>(null);
  let sens = $state<Snapshot | null>(null);
  let ping = $state<number | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    const offs: (() => void)[] = [];
    listen<FpsSample>('fps-sample', e => { s = e.payload; error = null; }).then(u => offs.push(u));
    listen<string>('fps-error', e => (error = e.payload)).then(u => offs.push(u));
    const tick = () => {
      api.sensors().then(v => (sens = v)).catch(() => {});
      api.ping().then(p => (ping = p.samples.at(-1) ?? null)).catch(() => {});
    };
    tick();
    const timer = setInterval(tick, 2000);
    return () => { clearInterval(timer); offs.forEach(o => o()); };
  });

  let gpu = $derived(sens?.gpus[0]);
  let tone = $derived(!s ? 'var(--muted)' : s.fps >= 100 ? 'var(--color-success)' : s.fps >= 50 ? 'var(--color-warning)' : 'var(--color-error)');
</script>

<div class="mini-win" data-tauri-drag-region>
  <div class="flex items-center justify-between gap-2" data-tauri-drag-region>
    <div class="text-[10px] uppercase tracking-wider muted font-semibold truncate" data-tauri-drag-region>{s?.game || 'FPS'}</div>
    <button class="mini-x shrink-0" title={t('common.close')} onclick={() => api.fps.overlay(false)}>×</button>
  </div>
  <div class="flex items-end justify-between" data-tauri-drag-region>
    <div class="flex items-baseline gap-1.5" data-tauri-drag-region>
      <span class="text-3xl font-bold mono leading-none" style="color:{tone}" data-tauri-drag-region>{s ? Math.round(s.fps) : '—'}</span>
      <span class="text-[11px] muted" data-tauri-drag-region>FPS</span>
    </div>
    <div class="text-right text-[11px] leading-tight mono" data-tauri-drag-region>
      <div><span class="muted">1%</span> {s ? Math.round(s.low1) : '—'}</div>
      <div><span class="muted">ms</span> {s ? s.frametime.toFixed(1) : '—'}</div>
    </div>
  </div>
  {#if error}
    <div class="text-[11px] mt-1" style="color:var(--color-warning)">{error === 'needs_admin' ? t('fps.needAdmin') : error}</div>
  {:else}
    <div class="mt-1"><Spark values={s?.recent ?? []} height={30} color={tone} min={0} max={Math.max(20, ...(s?.recent ?? [16]).map(v => v * 1.3))} /></div>
  {/if}
  <div class="flex justify-between gap-2 mt-1.5 text-[11px] mono whitespace-nowrap" data-tauri-drag-region>
    <span data-tauri-drag-region><span class="muted">CPU</span> {sens ? Math.round(sens.cpu_load) : '—'}%</span>
    <span data-tauri-drag-region><span class="muted">GPU</span> {gpu?.load != null ? Math.round(gpu.load) : '—'}%{#if gpu?.temp != null}/{Math.round(gpu.temp)}°{/if}</span>
    <span data-tauri-drag-region><span class="muted">ping</span> {ping != null ? Math.round(ping) : '—'}</span>
  </div>
</div>
