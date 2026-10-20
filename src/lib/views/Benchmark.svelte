<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../store.svelte';
  import { api, ago, type BenchRun } from '../api';
  import { t } from '../i18n.svelte';
  import { gpuBench } from '../gpubench';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Spark from '../Spark.svelte';

  // Quick, repeatable benchmark: compare before and after a tweak or driver update
  let runs = $state<BenchRun[]>([]);
  let running = $state(false);
  let phase = $state<string | null>(null);
  let withGpu = $state(true);
  let drive = $state('');

  onMount(() => {
    api.bench.history().then(r => (runs = r)).catch(() => {});
    const off = listen<string>('bench-progress', e => (phase = e.payload));
    return () => { off.then(u => u()); };
  });

  const PHASES = ['gpu', 'cpu_single', 'cpu_multi', 'memory', 'disk'];
  async function start() {
    running = true;
    try {
      let gpu: number | null = null;
      if (withGpu) { phase = 'gpu'; gpu = await gpuBench(6); }
      phase = 'cpu_single';
      const r = await api.bench.run(drive || null, gpu);
      runs = [r, ...runs];
      app.toast(t('bench.done'), 'success');
    } catch (e) { app.toast(String(e), 'error'); }
    finally { running = false; phase = null; }
  }
  async function remove(r: BenchRun) { await api.bench.remove(r.at).catch(() => {}); runs = runs.filter(x => x.at !== r.at); }

  type Key = 'cpu_single' | 'cpu_multi' | 'memory' | 'disk_read' | 'disk_write' | 'disk_iops' | 'gpu';
  const METRICS: { key: Key; ico: string; unit: string }[] = [
    { key: 'cpu_single', ico: 'cpu', unit: 'pts' }, { key: 'cpu_multi', ico: 'cpu', unit: 'pts' }, { key: 'gpu', ico: 'monitor', unit: 'fps' },
    { key: 'memory', ico: 'memory', unit: 'GB/s' }, { key: 'disk_read', ico: 'drive', unit: 'MB/s' }, { key: 'disk_write', ico: 'drive', unit: 'MB/s' },
    { key: 'disk_iops', ico: 'drive', unit: 'IOPS' },
  ];
  let last = $derived(runs[0] ?? null);
  let prev = $derived(runs[1] ?? null);
  const v = (r: BenchRun | null, k: Key) => (r ? (r[k] ?? null) : null);
  const delta = (k: Key) => { const a = v(prev, k), b = v(last, k); return a && b ? ((b - a) / a) * 100 : null; };
  const n = (x: number | null, k: Key) => (x === null ? '—' : k === 'memory' || k === 'gpu' ? x.toFixed(1) : Math.round(x).toLocaleString());
  /** What changed on the PC between two runs, so a difference has an explanation. */
  const changes = (a: BenchRun, b: BenchRun) => [
    ...(a.driver !== b.driver && b.driver ? [t('bench.chDriver', { from: a.driver || '?', to: b.driver })] : []),
    ...(a.profile !== b.profile ? [t('bench.chProfile', { to: b.profile ? t('opt.profile.' + b.profile) : t('bench.noProfile') })] : []),
    ...(a.drive !== b.drive ? [t('bench.chDrive', { to: b.drive })] : []),
  ];
</script>

<section class="glass p-5 reveal">
  <PageHead ico="gauge" title={t('bench.title')} sub={t('bench.sub')}>
    <select class="select select-sm w-40" bind:value={drive} disabled={running} title={t('bench.drive')}>
      <option value="">{t('bench.driveDefault')}</option>
      {#each app.drives.filter(d => !d.mount.toUpperCase().startsWith('C')) as d (d.mount)}<option value={d.mount}>{t('bench.driveOn', { d: d.mount })}</option>{/each}
    </select>
    <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="checkbox checkbox-primary checkbox-sm" bind:checked={withGpu} disabled={running} />GPU</label>
    <button class="btn btn-sm btn-primary gap-1.5" disabled={running} onclick={start}>
      {#if running}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="play" size=".9rem" />{/if}{t('bench.run')}
    </button>
  </PageHead>
  {#if running}
    <div class="flex flex-wrap gap-2 mt-4">
      {#each PHASES.filter(p => withGpu || p !== 'gpu') as p, i (p)}
        {@const idx = PHASES.indexOf(phase ?? '')}
        {@const state = PHASES.indexOf(p) < idx ? 'done' : p === phase ? 'now' : 'wait'}
        <span class="badge gap-1.5 {state === 'now' ? 'badge-primary' : state === 'done' ? 'badge-success badge-soft' : 'badge-ghost'}" style="--i:{i}">
          {#if state === 'now'}<span class="loading loading-spinner loading-xs"></span>{:else if state === 'done'}<Icon name="check" size=".7rem" />{/if}{t('bench.m.' + p)}
        </span>
      {/each}
    </div>
    <div class="text-[11px] muted mt-2">{t('bench.runningHint')}</div>
  {/if}
</section>

{#if !runs.length}
  <div class="glass p-10 text-center reveal" style="--i:1"><Icon name="gauge" size="1.8rem" style="opacity:.4" /><div class="text-sm mt-2">{t('bench.none')}</div><div class="text-xs muted mt-1">{t('bench.noneHint')}</div></div>
{:else}
  <section class="grid grid-cols-2 md:grid-cols-4 xl:grid-cols-7 gap-3">
    {#each METRICS as m, i (m.key)}
      {@const d = delta(m.key)}
      {@const series = [...runs].reverse().map(r => v(r, m.key)).filter((x): x is number => x !== null)}
      <div class="glass p-4 reveal" style="--i:{i + 1}">
        <div class="text-[11px] muted flex items-center gap-1.5"><Icon name={m.ico} size=".8rem" />{t('bench.m.' + m.key)}</div>
        <div class="mono text-xl font-bold mt-1">{n(v(last, m.key), m.key)} <small class="text-[10px] muted font-normal">{m.unit}</small></div>
        {#if d !== null && Math.abs(d) >= 0.5}
          <div class="text-[11px] font-semibold" style="color:var(--color-{d > 0 ? 'success' : 'error'})">{d > 0 ? '+' : ''}{d.toFixed(1)}% {t('bench.vsPrev')}</div>
        {:else}<div class="text-[11px] muted">{prev ? t('bench.same') : ' '}</div>{/if}
        {#if series.length > 1}<div class="mt-2"><Spark values={series} height={28} min={0} /></div>{/if}
      </div>
    {/each}
  </section>

  <section class="glass p-5 reveal" style="--i:8">
    <div class="font-semibold">{t('bench.history')}</div>
    <div class="text-xs muted">{t('bench.historySub')}</div>
    <div class="overflow-x-auto mt-3">
      <table class="table table-sm">
        <thead><tr><th>{t('bench.when')}</th>{#each METRICS as m (m.key)}<th class="text-right">{t('bench.m.' + m.key)}</th>{/each}<th></th></tr></thead>
        <tbody>
          {#each runs as r, i (r.at)}
            {@const ch = runs[i + 1] ? changes(runs[i + 1], r) : []}
            <tr>
              <td class="whitespace-nowrap"><div class="text-sm">{ago(r.at, true)}</div><div class="text-[10px] muted">{r.driver || '—'}{r.profile ? ` · ${t('opt.profile.' + r.profile)}` : ''}</div>
                {#each ch as c (c)}<div class="text-[10px]" style="color:var(--color-info)">↳ {c}</div>{/each}</td>
              {#each METRICS as m (m.key)}<td class="text-right mono text-xs">{n(v(r, m.key), m.key)}</td>{/each}
              <td><button class="btn btn-ghost btn-xs btn-square" onclick={() => remove(r)} aria-label="delete"><Icon name="x" size=".8rem" /></button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
{/if}
