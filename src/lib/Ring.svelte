<script lang="ts">
  /** Circular gauge: `value` out of `max`, colored by thresholds (warn / bad). */
  /** `invert`: higher is better (health, security score); then values below `warn` / `bad` are flagged. */
  let { value, max = 100, label, sub = '', unit = '', size = 120, warn = 70, bad = 85, decimals = 0, invert = false }:
    { value: number | null; max?: number; label: string; sub?: string; unit?: string; size?: number; warn?: number; bad?: number; decimals?: number; invert?: boolean } = $props();

  const R = 52;
  const C = 2 * Math.PI * R;
  let frac = $derived(value === null ? 0 : Math.max(0, Math.min(1, value / max)));
  let tone = $derived(
    value === null ? 'var(--muted)'
      : invert ? (value < bad ? 'var(--color-error)' : value < warn ? 'var(--color-warning)' : 'var(--color-success)')
      : value >= bad ? 'var(--color-error)' : value >= warn ? 'var(--color-warning)' : 'var(--color-success)',
  );
</script>

<div class="relative shrink-0" style="width:{size}px;height:{size}px">
  <svg viewBox="0 0 120 120" class="w-full h-full -rotate-90">
    <circle cx="60" cy="60" r={R} fill="none" stroke-width="9" style="stroke:color-mix(in oklch,var(--color-base-content) 8%,transparent)" />
    <circle cx="60" cy="60" r={R} fill="none" stroke-width="9" stroke-linecap="round"
      style="stroke:{tone};stroke-dasharray:{C};stroke-dashoffset:{C * (1 - frac)};transition:stroke-dashoffset .8s cubic-bezier(.2,.8,.2,1),stroke .4s" />
  </svg>
  <div class="absolute inset-0 grid place-items-center text-center">
    <div>
      <div class="font-bold mono leading-none" style="font-size:{size / 4.6}px">{value === null ? '—' : value.toFixed(decimals)}<span class="text-xs muted font-normal">{value === null ? '' : unit}</span></div>
      <div class="text-[11px] muted mt-1">{label}</div>
      {#if sub}<div class="text-[10px] muted">{sub}</div>{/if}
    </div>
  </div>
</div>
