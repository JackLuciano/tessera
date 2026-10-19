<script lang="ts">
  import { untrack } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, num, CATS, type TreeNode } from '../api';
  import { t } from '../i18n.svelte';
  import { nodeMenu, reveal } from '../actions';
  import Icon from '../Icon.svelte';
  import Treemap from '../Treemap.svelte';

  let node = $state<TreeNode | null>(null);
  let crumbs = $state<[number, string][]>([]);
  let tm: Treemap | undefined = $state();

  async function load(id: number) {
    try {
      // Deep enough for the nested treemap; blocks under 0.05% of the view are pruned in Rust
      const n = await api.node(id, 6, 150, 0.0005);
      crumbs = await api.ancestors(id);
      node = n;
      app.explorerTarget = id;
    } catch (e) {
      if (id !== 0) return load(0);
      app.toast(String(e), 'error');
    }
  }

  $effect(() => {
    void app.version; void app.liveVersion;
    // Only re-run on rescans/deletes/live changes, not when load() updates explorerTarget or summary ticks
    untrack(() => { if (app.summary) load(app.explorerTarget); });
  });

  const up = () => crumbs.length > 1 && load(crumbs[crumbs.length - 2][0]);
  const ctx = (ev: MouseEvent, n: TreeNode) => nodeMenu(ev, n, () => load(n.id), () => load(node!.id));
</script>

<svelte:window onkeydown={e => { if (e.key === 'Backspace' && !/INPUT|TEXTAREA/.test((document.activeElement as HTMLElement)?.tagName ?? '')) up(); }} />

{#if !app.summary || !node}
  <div class="glass p-10 text-center muted">{t('common.scanFirst')}</div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-12 gap-4">
    <div class="glass p-4 xl:col-span-9 reveal flex flex-col">
      <div class="flex items-center gap-2 mb-3 flex-wrap">
        <button class="btn btn-ghost btn-xs btn-square" aria-label={t('explorer.up')} disabled={crumbs.length <= 1} onclick={up}><Icon name="back" /></button>
        <div class="flex items-center gap-0.5 text-sm flex-wrap min-w-0">
          {#each crumbs as [id, name], i (id)}
            {#if i}<span class="muted opacity-50">/</span>{/if}
            <button class="px-2 py-0.5 rounded-md hover:bg-[var(--soft)] font-medium {i === crumbs.length - 1 ? '' : 'muted'} truncate max-w-56" onclick={() => load(id)}>{name}</button>
          {/each}
          <span class="badge badge-ghost badge-sm mono ml-2">{fmt(node.size)}</span>
        </div>
        <span class="text-xs muted ml-auto">{t('explorer.hint')}</span>
      </div>
      <div style="height:max(460px, calc(100vh - 210px))">
        <Treemap bind:this={tm} {node} onopen={n => load(n.id)} oncontext={ctx} />
      </div>
    </div>

    <div class="glass p-4 xl:col-span-3 reveal flex flex-col gap-3" style="--i:1">
      <div class="flex items-center gap-3">
        <span class="w-10 h-10 rounded-xl grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--cat-{node.cat}) 16%,transparent);color:var(--cat-{node.cat})"><Icon name="folder" /></span>
        <div class="min-w-0"><div class="font-semibold truncate">{crumbs.at(-1)?.[1]}</div><div class="text-xs muted mono truncate selectable">{node.path}</div></div>
      </div>
      <div class="grid grid-cols-2 gap-2">
        <div class="tile-stat"><div>{t('stat.size')}</div><div>{fmt(node.size)}</div></div>
        <div class="tile-stat"><div>{t('stat.files')}</div><div>{num(node.files)}</div></div>
      </div>
      <button class="btn btn-sm" onclick={() => reveal(node!.path)}><Icon name="folder" size="1rem" />{t('action.reveal')}</button>
      <div class="text-xs uppercase tracking-wider muted font-semibold mt-1">{t('explorer.contents')}</div>
      {#key node.id}
        <div class="flex flex-col gap-0.5 overflow-y-auto -mx-1 stagger" style="max-height:calc(100vh - 380px)">
          {#each node.children as c (c.id)}
            {@const pct = (c.size / (node.size || 1)) * 100}
            <button class="row-hover relative flex items-center gap-2.5 px-2 py-1.5 text-left overflow-hidden" onclick={() => c.kind === 'dir' && load(c.id)}
              oncontextmenu={e => ctx(e, c)} onmouseenter={() => tm?.highlight(c.id)} onmouseleave={() => tm?.highlight(null)}>
              <!-- Share-of-parent bar behind the row -->
              <span class="share-bar" style="width:{pct}%;background:var(--cat-{c.cat})"></span>
              <span class="w-2.5 h-2.5 rounded-sm shrink-0" style="background:var(--cat-{c.cat})"></span>
              <Icon name={c.kind === 'dir' ? 'folder' : CATS[c.cat].ico} size=".9rem" style="opacity:.6" />
              <span class="flex-1 truncate text-sm">{c.name}</span>
              <span class="text-[11px] muted mono">{pct.toFixed(0)}%</span>
              <span class="mono text-xs w-16 text-right">{fmt(c.size)}</span>
            </button>
          {/each}
        </div>
      {/key}
    </div>
  </section>
{/if}
