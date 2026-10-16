<script lang="ts">
  import { menu } from './menu.svelte';
  import Icon from './Icon.svelte';

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  // Keep the menu inside the window
  $effect(() => {
    if (!menu.open || !el) return;
    const w = el.offsetWidth, h = el.offsetHeight;
    pos = { left: Math.min(menu.x, innerWidth - w - 8), top: Math.min(menu.y, innerHeight - h - 8) };
  });
</script>

<svelte:window onclick={() => menu.close()} onblur={() => menu.close()} onkeydown={e => e.key === 'Escape' && menu.close()} onscrollcapture={() => menu.close()} />

{#if menu.open}
  <div bind:this={el} class="fixed z-[100] min-w-56 p-1.5 rounded-xl border hairline" role="menu" tabindex="-1"
    style="left:{pos.left}px;top:{pos.top}px;background:var(--glass-solid);box-shadow:var(--shadow);backdrop-filter:blur(18px)"
    onclick={e => e.stopPropagation()} onkeydown={() => {}}>
    {#each menu.items as it, i (i)}
      {#if it === '-'}
        <hr class="my-1 mx-1 border-0 border-t hairline" />
      {:else}
        <button class="w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-sm text-left hover:bg-[color-mix(in_oklch,var(--color-primary)_14%,transparent)] disabled:opacity-40 disabled:pointer-events-none"
          style={it.danger ? 'color:var(--color-error)' : ''} disabled={it.disabled}
          onclick={ev => { menu.close(); it.run(ev); }}>
          <Icon name={it.ico} size="1rem" /><span>{it.label}</span>
        </button>
      {/if}
    {/each}
  </div>
{/if}
