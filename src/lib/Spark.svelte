<script lang="ts">
  /** Small line/area chart; `null` values leave gaps (lost pings, missing samples). */
  let { values, max, min = 0, color = 'var(--color-primary)', height = 40, fill = true, dots = false }:
    { values: (number | null)[]; max?: number; min?: number; color?: string; height?: number; fill?: boolean; dots?: boolean } = $props();

  const W = 100;
  let top = $derived(max ?? Math.max(1, ...values.filter((v): v is number => v !== null)) * 1.1);
  let pts = $derived(values.map((v, i) => (v === null ? null : [values.length < 2 ? W : (i / (values.length - 1)) * W, height - ((Math.min(v, top) - min) / Math.max(1e-6, top - min)) * height] as const)));
  let line = $derived.by(() => {
    let d = '';
    let pen = false;
    for (const p of pts) {
      if (!p) { pen = false; continue; }
      d += `${pen ? 'L' : 'M'}${p[0].toFixed(2)},${p[1].toFixed(2)} `;
      pen = true;
    }
    return d;
  });
  let area = $derived.by(() => {
    const ok = pts.filter(p => p) as (readonly [number, number])[];
    if (ok.length < 2) return '';
    return `M${ok[0][0]},${height} ` + ok.map(p => `L${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(' ') + ` L${ok[ok.length - 1][0]},${height} Z`;
  });
  const id = `sg${Math.random().toString(36).slice(2, 8)}`;
</script>

<svg viewBox="0 0 {W} {height}" preserveAspectRatio="none" class="w-full block overflow-visible" style="height:{height}px">
  <defs>
    <linearGradient {id} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" style="stop-color:{color};stop-opacity:.35" /><stop offset="100%" style="stop-color:{color};stop-opacity:0" />
    </linearGradient>
  </defs>
  {#if fill && area}<path d={area} fill="url(#{id})" />{/if}
  <path d={line} fill="none" style="stroke:{color}" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round" />
  {#if dots}
    {#each pts as p, i (i)}{#if p}<circle cx={p[0]} cy={p[1]} r="1.4" style="fill:{color}" vector-effect="non-scaling-stroke" />{/if}{/each}
  {/if}
</svg>
