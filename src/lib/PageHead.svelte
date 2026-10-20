<script lang="ts">
  import type { Snippet } from 'svelte';
  import { app } from './store.svelte';
  import { dash, tileFor } from './dashboard.svelte';
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';

  let { ico, title, sub, children }: { ico: string; title: string; sub?: string; children?: Snippet } = $props();
  // Pages with a Home tile can pin it from their header
  let tile = $derived(tileFor(app.view));
</script>

<div class="flex items-start gap-3 flex-wrap">
  <span class="page-ico"><Icon name={ico} size="1.2rem" /></span>
  <div class="flex-1 min-w-48">
    <h2 class="text-lg font-semibold leading-tight flex items-center gap-2">{title}
      {#if tile}
        {@const on = dash.has(tile)}
        <button class="btn btn-ghost btn-xs btn-square {on ? '' : 'opacity-40 hover:opacity-100'}" title={on ? t('home.unpin') : t('home.pin')} onclick={() => dash.toggle(tile!)} style={on ? 'color:var(--color-primary)' : ''}>
          <Icon name="pin" size=".85rem" />
        </button>
      {/if}
    </h2>
    {#if sub}<p class="text-xs muted mt-0.5 max-w-2xl">{sub}</p>{/if}
  </div>
  {#if children}<div class="flex items-center gap-2 flex-wrap">{@render children()}</div>{/if}
</div>
