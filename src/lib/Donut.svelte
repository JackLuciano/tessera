<script lang="ts">
  import { onMount } from 'svelte';
  import { fmt, type Cat } from './api';
  import { t } from './i18n.svelte';

  let { cats }: { cats: { cat: Cat; size: number }[] } = $props();
  let hot = $state<Cat | null>(null);
  let grow = $state(0);

  let total = $derived(cats.reduce((s, c) => s + c.size, 0) || 1);

  // Sweep in once when the chart appears. Data updates afterwards (rescans, live changes) just
  // redraw: reading `cats` here would re-run this on every live tick and restart the animation.
  onMount(() => {
    if (document.documentElement.classList.contains('no-anim')) { grow = 1; return; }
    const t0 = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const p = Math.min(1, (now - t0) / 1200);
      grow = 1 - (1 - p) ** 3;
      if (p < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  });

  function arc(r0: number, r1: number, a0: number, a1: number) {
    const p = (r: number, a: number) => [110 + r * Math.cos(a - Math.PI / 2), 110 + r * Math.sin(a - Math.PI / 2)];
    const large = a1 - a0 > Math.PI ? 1 : 0;
    const [x0, y0] = p(r1, a0), [x1, y1] = p(r1, a1), [x2, y2] = p(r0, a1), [x3, y3] = p(r0, a0);
    return `M${x0} ${y0}A${r1} ${r1} 0 ${large} 1 ${x1} ${y1}L${x2} ${y2}A${r0} ${r0} 0 ${large} 0 ${x3} ${y3}Z`;
  }

  let segs = $derived.by(() => {
    let a = 0;
    return cats.map(c => {
      const span = (c.size / total) * Math.PI * 2;
      const s = { ...c, a0: a, a1: a + span };
      a += span;
      return s;
    });
  });
  let shown = $derived(hot ? cats.find(c => c.cat === hot) : null);
</script>

<div class="relative shrink-0" style="width:210px;height:210px">
  <svg viewBox="0 0 220 220" class="w-full h-full">
    {#each segs as s (s.cat)}
      {@const a0 = s.a0 * grow}
      {@const a1 = s.a1 * grow}
      {@const g = Math.min(0.012, (a1 - a0) / 3)}
      {#if a1 - a0 > 0.001}
        <path d={arc(62, 98, a0 + g, a1 - g)} fill="var(--cat-{s.cat})" role="img" aria-label={s.cat}
          style="transition:opacity .25s, transform .25s; transform-origin:110px 110px; opacity:{hot && hot !== s.cat ? .28 : 1}; transform:scale({hot === s.cat ? 1.04 : 1}); cursor:pointer"
          onmouseenter={() => (hot = s.cat)} onmouseleave={() => (hot = null)} />
      {/if}
    {/each}
  </svg>
  <div class="absolute inset-0 grid place-items-center pointer-events-none text-center">
    <div>
      <div class="text-xs muted max-w-[110px] truncate">{shown ? t('cat.' + shown.cat) : t('overview.totalScanned')}</div>
      <div class="text-xl font-bold mono">{fmt(shown ? shown.size : total)}</div>
      {#if shown}<div class="text-xs muted mono">{((shown.size / total) * 100).toFixed(1)}%</div>{/if}
    </div>
  </div>
</div>
