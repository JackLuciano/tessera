<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { api, fmt, type FpsSample, type Snapshot } from '../api';
  import { t } from '../i18n.svelte';
  import Spark from '../Spark.svelte';

  // Desktop widget: CPU / GPU / RAM bars with a short history, temperatures and FPS when a game runs
  let now = $state<Snapshot | null>(null);
  let cpuHist = $state<number[]>([]);
  let gpuHist = $state<number[]>([]);
  let fps = $state<number | null>(null);

  onMount(() => {
    let off: (() => void) | undefined;
    let fpsAt = 0;
    listen<FpsSample>('fps-sample', e => { fps = e.payload.fps; fpsAt = Date.now(); }).then(u => (off = u));
    const tick = () => api.sensors().then(s => {
      if (!s) return;
      now = s;
      cpuHist = [...cpuHist, s.cpu_load].slice(-40);
      gpuHist = [...gpuHist, s.gpus[0]?.load ?? 0].slice(-40);
      if (Date.now() - fpsAt > 4000) fps = null;
    }).catch(() => {});
    tick();
    const timer = setInterval(tick, 2000);
    return () => { clearInterval(timer); off?.(); };
  });

  let gpu = $derived(now?.gpus[0]);
  let ramPct = $derived(now ? (now.ram_used / Math.max(1, now.ram_total)) * 100 : 0);
</script>

<div class="mini-win" data-tauri-drag-region>
  <div class="flex items-center justify-between" data-tauri-drag-region>
    <div class="text-[10px] uppercase tracking-wider muted font-semibold" data-tauri-drag-region>Tessera</div>
    <div class="flex items-center gap-2" data-tauri-drag-region>
      {#if fps !== null}<span class="text-[11px] mono font-semibold" style="color:var(--color-success)">{Math.round(fps)} FPS</span>{/if}
      <button class="mini-x" title={t('common.close')} onclick={() => api.widget(false)}>×</button>
    </div>
  </div>
  {#snippet row(label: string, pct: number, extra: string, hist: number[] | null, color: string)}
    <div class="mt-2" data-tauri-drag-region>
      <div class="flex justify-between text-[11px]" data-tauri-drag-region><span class="muted">{label}</span><span class="mono">{Math.round(pct)}%{extra}</span></div>
      {#if hist}<Spark values={hist} max={100} height={18} {color} />{:else}<div class="bar mt-1" style="height:5px"><i style="width:{pct}%;background:{color}"></i></div>{/if}
    </div>
  {/snippet}
  {@render row('CPU', now?.cpu_load ?? 0, now?.cpu_temp != null ? ` · ${Math.round(now.cpu_temp)}°` : '', cpuHist, 'var(--color-primary)')}
  {@render row('GPU', gpu?.load ?? 0, gpu?.temp != null ? ` · ${Math.round(gpu.temp)}°` : '', gpuHist, 'var(--color-accent)')}
  {@render row('RAM', ramPct, ` · ${fmt(now?.ram_used ?? 0)}`, null, ramPct > 85 ? 'var(--color-warning)' : 'var(--color-secondary)')}
</div>
