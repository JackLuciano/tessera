<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let { ico, title, sub, children }: { ico: string; title: string; sub?: string; children?: Snippet } = $props();
</script>

<div class="flex items-start gap-3 flex-wrap">
  <span class="page-ico"><Icon name={ico} size="1.2rem" /></span>
  <div class="flex-1 min-w-48">
    <h2 class="text-lg font-semibold leading-tight">{title}</h2>
    {#if sub}<p class="text-xs muted mt-0.5 max-w-2xl">{sub}</p>{/if}
  </div>
  {#if children}<div class="flex items-center gap-2 flex-wrap">{@render children()}</div>{/if}
</div>
