<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, ago, CATS, type Cat, type FileEntry } from '../api';
  import { t } from '../i18n.svelte';
  import { nodeMenu, reveal } from '../actions';
  import { confirmDelete } from '../confirm.svelte';
  import Icon from '../Icon.svelte';

  let files = $state<FileEntry[]>([]);
  let q = $state('');
  let cat = $state<Cat | 'all'>('all');
  let minAgeDays = $state(0);
  let sort = $state<'size' | 'name' | 'modified'>('size');
  let dir = $state(-1);
  let sel = $state(new Set<number>());

  $effect(() => {
    void app.version;
    if (app.summary) api.largestFiles(1000).then(f => { files = f; sel = new Set(); });
  });

  const now = Date.now() / 1000;
  let list = $derived.by(() => {
    const ql = q.toLowerCase();
    return files
      .filter(f => (cat === 'all' || f.cat === cat) && (!minAgeDays || now - f.modified >= minAgeDays * 86400) && (!ql || f.path.toLowerCase().includes(ql)))
      .sort((a, b) => (sort === 'name' ? a.name.localeCompare(b.name) : a[sort] - b[sort]) * dir);
  });
  let maxSize = $derived(files[0]?.size ?? 1);
  let selSize = $derived(files.filter(f => sel.has(f.id)).reduce((s, f) => s + f.size, 0));
  let presentCats = $derived([...new Set(files.map(f => f.cat))]);

  function toggle(id: number) { const s = new Set(sel); s.has(id) ? s.delete(id) : s.add(id); sel = s; }
  function sortBy(k: typeof sort) { dir = sort === k ? -dir : k === 'name' ? 1 : -1; sort = k; }
  function del(ids: number[], ev: MouseEvent) {
    const items = files.filter(f => ids.includes(f.id)).map(f => ({ id: f.id, name: f.name, size: f.size }));
    confirmDelete.ask(items, ev);
  }
</script>

{#if !app.summary}
  <div class="glass p-10 text-center muted">{t('common.scanFirst')}</div>
{:else}
  <section class="glass p-4 reveal">
    <div class="flex items-center gap-2 flex-wrap mb-3">
      <label class="input input-sm w-72"><Icon name="search" size="1rem" style="opacity:.5" /><input type="search" placeholder={t('files.filter')} bind:value={q} /></label>
      <select class="select select-sm w-48" bind:value={minAgeDays}>
        <option value={0}>{t('files.anyAge')}</option><option value={180}>{t('files.age6m')}</option><option value={365}>{t('files.age1y')}</option>
      </select>
      <span class="text-xs muted ml-auto">{t('files.count', { n: list.length, size: fmt(list.reduce((s, f) => s + f.size, 0)) })}</span>
    </div>
    <div class="flex gap-1.5 flex-wrap mb-3">
      <button class="btn btn-xs {cat === 'all' ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => (cat = 'all')}>{t('files.allTypes')}</button>
      {#each presentCats as c (c)}
        <button class="btn btn-xs gap-1.5 {cat === c ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => (cat = c)}><span class="w-2 h-2 rounded-full" style="background:var(--cat-{c})"></span>{t('cat.' + c)}</button>
      {/each}
    </div>
    <table class="w-full text-sm">
      <thead>
        <tr class="text-left text-xs muted">
          <th class="py-2 px-2 w-8"></th>
          <th class="font-medium py-2 px-2 cursor-pointer" onclick={() => sortBy('name')}>{t('files.name')} {sort === 'name' ? (dir < 0 ? '↓' : '↑') : ''}</th>
          <th class="font-medium py-2 px-2 cursor-pointer w-36" onclick={() => sortBy('modified')}>{t('files.modified')} {sort === 'modified' ? (dir < 0 ? '↓' : '↑') : ''}</th>
          <th class="font-medium py-2 px-2 text-right cursor-pointer w-28" onclick={() => sortBy('size')}>{t('stat.size')} {sort === 'size' ? (dir < 0 ? '↓' : '↑') : ''}</th>
          <th class="py-2 px-2 w-24"></th>
        </tr>
      </thead>
      <tbody>
        {#each list.slice(0, 400) as f (f.id)}
          <tr class="frow border-t hairline cursor-pointer" style={sel.has(f.id) ? 'background:color-mix(in oklch,var(--color-primary) 8%,transparent)' : ''}
            onclick={() => toggle(f.id)} oncontextmenu={e => nodeMenu(e, { ...f, kind: 'file' })}>
            <td class="py-2 px-2"><input type="checkbox" class="checkbox checkbox-xs checkbox-primary" checked={sel.has(f.id)} tabindex="-1" /></td>
            <td class="py-2 px-2">
              <div class="flex items-center gap-3 min-w-0">
                <span class="w-8 h-8 rounded-lg grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--cat-{f.cat}) 16%,transparent);color:var(--cat-{f.cat})"><Icon name={CATS[f.cat].ico} size="1rem" /></span>
                <div class="min-w-0">
                  <div class="font-medium truncate">{f.name}</div>
                  <div class="text-[11px] muted mono truncate max-w-[640px]">{f.path}</div>
                  <div class="bar mt-1 w-28" style="height:3px"><i style="width:{(f.size / maxSize) * 100}%;background:var(--cat-{f.cat})"></i></div>
                </div>
              </div>
            </td>
            <td class="py-2 px-2">{#if now - f.modified > 180 * 86400}<span class="badge badge-soft badge-warning badge-sm whitespace-nowrap">{ago(f.modified)}</span>{:else}<span class="muted">{ago(f.modified)}</span>{/if}</td>
            <td class="py-2 px-2 text-right mono font-semibold whitespace-nowrap">{fmt(f.size)}</td>
            <td class="py-2 px-2">
              <div class="acts flex justify-end gap-1">
                <button class="btn btn-ghost btn-xs btn-square" title={t('action.reveal')} onclick={e => { e.stopPropagation(); reveal(f.path); }}><Icon name="folder" size=".95rem" /></button>
                <button class="btn btn-ghost btn-xs btn-square" style="color:var(--color-error)" title={app.permanentDelete ? t('action.deletePerm') : t('action.recycle')} onclick={e => { e.stopPropagation(); del([f.id], e); }}><Icon name="trash" size=".95rem" /></button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>

  {#if sel.size}
    <div class="fixed left-1/2 -translate-x-1/2 bottom-5 z-40 glass px-4 py-2.5 flex items-center gap-3" style="background:var(--glass-solid)">
      <span class="text-sm whitespace-nowrap">{t('files.selected', { n: sel.size, size: fmt(selSize) })}</span>
      <button class="btn btn-ghost btn-sm" onclick={() => (sel = new Set())}>{t('common.clear')}</button>
      <button class="btn btn-error btn-sm gap-1" onclick={e => del([...sel], e)}><Icon name="trash" size="1rem" />{app.permanentDelete ? t('action.deletePerm') : t('action.recycle')}</button>
    </div>
  {/if}
{/if}
