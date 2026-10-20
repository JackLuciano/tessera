<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api, OVERLAY_DEFAULT, type FpsSample, type OverlayCfg, type Prefs, type Snapshot } from '../api';
  import { t } from '../i18n.svelte';
  import Spark from '../Spark.svelte';

  // The FPS window: live frame rate and frame times from PresentMon, plus CPU/GPU and ping. Its
  // look (corner, size, opacity, values, one-line mode) comes from the settings, and it reports its
  // measured size so the backend can fit the window around it.
  let s = $state<FpsSample | null>(null);
  let sens = $state<Snapshot | null>(null);
  let ping = $state<number | null>(null);
  let cfg = $state<OverlayCfg>({ ...OVERLAY_DEFAULT });
  // What the recorder is doing while there are no frames yet: downloading, waiting, an error...
  let phase = $state<string | null>(null);
  let recording = $state(false);
  let box = $state<HTMLElement | null>(null);

  const PHASES: Record<string, string> = {
    needs_admin: 'fps.ovAdmin',
    downloading: 'fps.ovDownloading',
    waiting: 'fps.ovWaiting',
    no_game: 'fps.ovNoGame',
    no_frames: 'fps.ovNoFrames',
  };
  let note = $derived(phase ? (PHASES[phase] ? t(PHASES[phase]) : phase) : !s && !recording ? t('fps.ovIdle') : null);
  let on = $derived((f: string) => cfg.fields.includes(f));

  onMount(() => {
    const offs: (() => void)[] = [];
    const useCfg = (p: Prefs | null) => { if (p?.overlay) cfg = { ...OVERLAY_DEFAULT, ...p.overlay }; };
    api.prefs().then(useCfg).catch(() => {});
    // The recording may have started (or failed) before this window loaded
    api.fps.status().then(st => { phase = st.phase; recording = !!st.recording; }).catch(() => {});
    listen<FpsSample>('fps-sample', e => { s = e.payload; phase = null; recording = true; }).then(u => offs.push(u));
    listen<string | null>('fps-state', e => { phase = e.payload; if (e.payload === 'waiting') recording = true; }).then(u => offs.push(u));
    listen<Prefs>('prefs', e => useCfg(e.payload)).then(u => offs.push(u));
    const tick = () => {
      api.sensors().then(v => (sens = v)).catch(() => {});
      if (cfg.fields.includes('ping')) api.ping().then(p => (ping = p.samples.at(-1) ?? null)).catch(() => {});
    };
    tick();
    const timer = setInterval(tick, 2000);
    // Fit the window to the content whenever it changes size (values, one-line mode, scale)
    let last = '';
    const ro = new ResizeObserver(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      const key = `${Math.ceil(r.width)}x${Math.ceil(r.height)}`;
      if (key === last) return;
      last = key;
      api.fps.fit(Math.ceil(r.width) + 8, Math.ceil(r.height) + 8).catch(() => {});
    });
    if (box) ro.observe(box);
    return () => { clearInterval(timer); ro.disconnect(); offs.forEach(o => o()); };
  });

  let gpu = $derived(sens?.gpus[0]);
  let tone = $derived(!s ? 'var(--muted)' : s.fps >= 100 ? 'var(--color-success)' : s.fps >= 50 ? 'var(--color-warning)' : 'var(--color-error)');
  let bg = $derived(`color-mix(in oklch, var(--color-base-200) ${Math.round(cfg.opacity * 100)}%, transparent)`);
  const n = (v: number | null | undefined, d = 0) => (v == null ? '—' : d ? v.toFixed(d) : String(Math.round(v)));
</script>

<div class="mini-win mini-fit" class:compact={cfg.compact} style="zoom:{cfg.scale}; background:{bg}" bind:this={box} data-tauri-drag-region>
  {#if cfg.compact}
    <div class="flex items-center gap-3 mono text-[12px] whitespace-nowrap" data-tauri-drag-region>
      {#if on('fps')}<span data-tauri-drag-region><b class="text-[15px]" style="color:{tone}">{n(s?.fps)}</b> <span class="muted">FPS</span></span>{/if}
      {#if on('low1')}<span data-tauri-drag-region><span class="muted">1%</span> {n(s?.low1)}</span>{/if}
      {#if on('frametime')}<span data-tauri-drag-region><span class="muted">ms</span> {n(s?.frametime, 1)}</span>{/if}
      {#if on('cpu')}<span data-tauri-drag-region><span class="muted">CPU</span> {n(sens?.cpu_load)}%</span>{/if}
      {#if on('gpu')}<span data-tauri-drag-region><span class="muted">GPU</span> {n(gpu?.load)}%</span>{/if}
      {#if on('temps')}<span data-tauri-drag-region>{n(sens?.cpu_temp)}°/{n(gpu?.temp)}°</span>{/if}
      {#if on('ping')}<span data-tauri-drag-region><span class="muted">ping</span> {n(ping)}</span>{/if}
      {#if note}<span class="muted text-[11px]" data-tauri-drag-region>{note}</span>{/if}
      <button class="mini-x shrink-0" title={t('common.close')} onclick={() => api.fps.overlay(false)}>×</button>
    </div>
  {:else}
    <div class="flex items-center justify-between gap-2" data-tauri-drag-region>
      <div class="text-[10px] uppercase tracking-wider muted font-semibold truncate" data-tauri-drag-region>{s?.game || 'FPS'}</div>
      <button class="mini-x shrink-0" title={t('common.close')} onclick={() => api.fps.overlay(false)}>×</button>
    </div>
    {#if on('fps') || on('low1') || on('frametime')}
      <div class="flex items-end justify-between gap-3" data-tauri-drag-region>
        {#if on('fps')}
          <div class="flex items-baseline gap-1.5" data-tauri-drag-region>
            <span class="text-3xl font-bold mono leading-none" style="color:{tone}" data-tauri-drag-region>{n(s?.fps)}</span>
            <span class="text-[11px] muted" data-tauri-drag-region>FPS</span>
          </div>
        {/if}
        <div class="text-right text-[11px] leading-tight mono" data-tauri-drag-region>
          {#if on('low1')}<div><span class="muted">1%</span> {n(s?.low1)}</div>{/if}
          {#if on('frametime')}<div><span class="muted">ms</span> {n(s?.frametime, 1)}</div>{/if}
        </div>
      </div>
    {/if}
    {#if note}
      <div class="text-[11px] mt-1 leading-tight" style="color:{phase === 'waiting' || phase === 'downloading' || !phase ? 'var(--muted)' : 'var(--color-warning)'}" data-tauri-drag-region>{note}</div>
    {:else if on('graph')}
      <div class="mt-1"><Spark values={s?.recent ?? []} height={30} color={tone} min={0} max={Math.max(20, ...(s?.recent ?? [16]).map(v => v * 1.3))} /></div>
    {/if}
    {#if on('cpu') || on('gpu') || on('temps') || on('ping')}
      <div class="flex justify-between gap-3 mt-1.5 text-[11px] mono whitespace-nowrap" data-tauri-drag-region>
        {#if on('cpu')}<span data-tauri-drag-region><span class="muted">CPU</span> {n(sens?.cpu_load)}%{#if on('temps') && sens?.cpu_temp != null}/{n(sens.cpu_temp)}°{/if}</span>{/if}
        {#if on('gpu')}<span data-tauri-drag-region><span class="muted">GPU</span> {n(gpu?.load)}%{#if on('temps') && gpu?.temp != null}/{n(gpu.temp)}°{/if}</span>{/if}
        {#if on('temps') && !on('cpu') && !on('gpu')}<span data-tauri-drag-region>{n(sens?.cpu_temp)}° / {n(gpu?.temp)}°</span>{/if}
        {#if on('ping')}<span data-tauri-drag-region><span class="muted">ping</span> {n(ping)}</span>{/if}
      </div>
    {/if}
  {/if}
</div>
