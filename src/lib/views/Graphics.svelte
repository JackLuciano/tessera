<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type GfxConfig, type GfxChange } from '../api';
  import { t } from '../i18n.svelte';
  import { reveal } from '../actions';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  // Per-game graphics presets through the game's own settings file (Unreal Engine games)
  let list = $state<GfxConfig[] | null>(null);
  /** Unsaved edits per settings file. */
  let edits = $state<Record<string, GfxChange>>({});
  let saving = $state<string | null>(null);

  async function load() { list = await api.gfx.list().catch(e => (app.toast(String(e), 'error'), [])); edits = {}; }
  onMount(load);

  const GROUPS = ['ViewDistance', 'AntiAliasing', 'Shadow', 'GlobalIllumination', 'Reflection', 'PostProcess', 'Texture', 'Effects', 'Foliage', 'Shading', 'Landscape'];
  const LEVELS = [0, 1, 2, 3, 4];
  /** Starting points the user can still adjust before saving. */
  const PRESETS: Record<string, { groups: Record<string, number>; vsync?: boolean }> = {
    competitive: { groups: { ViewDistance: 2, AntiAliasing: 0, Shadow: 0, GlobalIllumination: 0, Reflection: 0, PostProcess: 0, Texture: 2, Effects: 0, Foliage: 0, Shading: 1, Landscape: 1 }, vsync: false },
    balanced: { groups: Object.fromEntries(GROUPS.map(g => [g, 2])) },
    quality: { groups: Object.fromEntries(GROUPS.map(g => [g, 3])) },
  };

  const cur = (c: GfxConfig) => edits[c.path];
  const val = (c: GfxConfig, g: string) => cur(c)?.groups[g] ?? c.groups[g];
  const vsync = (c: GfxConfig) => cur(c)?.vsync ?? c.vsync;
  function edit(c: GfxConfig, patch: Partial<GfxChange>) {
    const e = cur(c) ?? { groups: {} };
    edits = { ...edits, [c.path]: { ...e, ...patch, groups: { ...e.groups, ...(patch.groups ?? {}) } } };
  }
  function preset(c: GfxConfig, key: string) {
    const p = PRESETS[key];
    // Only groups this game actually has
    const groups = Object.fromEntries(Object.entries(p.groups).filter(([g]) => g in c.groups));
    edit(c, { groups, ...(p.vsync !== undefined && c.vsync !== null ? { vsync: p.vsync } : {}) });
  }
  async function save(c: GfxConfig) {
    const e = cur(c);
    if (!e) return;
    saving = c.path;
    try { await api.gfx.save(c.path, e); app.toast(t('gfx.saved', { game: c.name }), 'success'); await load(); }
    catch (err) { app.toast(String(err), 'error'); }
    finally { saving = null; }
  }
  async function restore(c: GfxConfig, file: string) {
    try { await api.gfx.restore(c.path, file); app.toast(t('gfx.restored', { game: c.name }), 'success'); await load(); }
    catch (err) { app.toast(String(err), 'error'); }
  }
  let avg = (c: GfxConfig) => {
    const v = GROUPS.filter(g => g in c.groups).map(g => val(c, g));
    return v.length ? v.reduce((s, x) => s + x, 0) / v.length : 0;
  };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="image" title={t('gfx.title')} sub={t('gfx.sub')}>
    <button class="btn btn-sm btn-ghost border hairline gap-1.5" onclick={load}><Icon name="refresh" size=".9rem" />{t('action.refresh')}</button>
  </PageHead>
  <div class="soft rounded-xl p-3 mt-4 text-xs flex gap-2 items-start"><Icon name="info" size=".9rem" style="color:var(--color-info)" />{t('gfx.closeFirst')}</div>
</section>

{#if list === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !list.length}
  <div class="glass p-10 text-center"><Icon name="image" size="1.8rem" style="opacity:.4" /><div class="text-sm mt-2">{t('gfx.none')}</div><div class="text-xs muted mt-1">{t('gfx.noneHint')}</div></div>
{:else}
  {#each list as c, i (c.path)}
    {@const dirty = !!cur(c)}
    <section class="glass p-5 reveal" style="--i:{i + 1}">
      <div class="flex items-start justify-between gap-3 flex-wrap">
        <div class="min-w-0">
          <div class="font-semibold flex items-center gap-2">{c.name}{#if c.name !== c.project}<span class="text-[11px] muted mono">{c.project}</span>{/if}</div>
          <button class="text-[11px] muted mono truncate max-w-xl text-left hover:underline" title={c.path} onclick={() => reveal(c.path)}>{c.path}</button>
        </div>
        <div class="flex gap-1.5 flex-wrap">
          {#each Object.keys(PRESETS) as p (p)}
            <button class="btn btn-xs btn-ghost border hairline gap-1" onclick={() => preset(c, p)}><Icon name={p === 'competitive' ? 'zap' : p === 'quality' ? 'sparkles' : 'scale'} size=".75rem" />{t('gfx.p.' + p)}</button>
          {/each}
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-x-6 gap-y-2.5 mt-4">
        {#each GROUPS.filter(g => g in c.groups) as g (g)}
          {@const v = val(c, g)}
          <div class="flex items-center gap-3">
            <span class="text-xs w-32 shrink-0 truncate">{t('gfx.g.' + g)}</span>
            <div class="join flex-1">
              {#each LEVELS as l (l)}
                <button class="join-item btn btn-xs flex-1 {v === l ? 'btn-primary' : 'btn-ghost border hairline'} {cur(c)?.groups[g] !== undefined && v === l ? 'ring-1 ring-[var(--color-accent)]' : ''}" title={t('gfx.l.' + l)} onclick={() => edit(c, { groups: { [g]: l } })}>{t('gfx.l.' + l).slice(0, 3)}</button>
              {/each}
            </div>
          </div>
        {/each}
      </div>

      <div class="flex items-center gap-5 mt-4 flex-wrap">
        {#if c.vsync !== null}
          <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="toggle toggle-primary toggle-sm" checked={!!vsync(c)} onchange={e => edit(c, { vsync: (e.target as HTMLInputElement).checked })} />V-Sync</label>
        {/if}
        {#if c.frame_limit !== null}
          <label class="flex items-center gap-2 text-sm">{t('gfx.cap')}
            <input type="number" class="input input-xs w-20 mono" min="0" max="1000" value={cur(c)?.frame_limit ?? c.frame_limit} onchange={e => edit(c, { frame_limit: +(e.target as HTMLInputElement).value })} />
            <span class="text-[11px] muted">{t('gfx.capHint')}</span>
          </label>
        {/if}
        <span class="flex-1"></span>
        <span class="text-[11px] muted">{t('gfx.level', { l: t('gfx.l.' + Math.round(avg(c))) })} · {t('gfx.changed', { ago: ago(c.modified, true) })}</span>
        {#if dirty}<button class="btn btn-sm btn-ghost" onclick={() => { const e = { ...edits }; delete e[c.path]; edits = e; }}>{t('common.cancel')}</button>{/if}
        <button class="btn btn-sm btn-primary gap-1.5" disabled={!dirty || saving === c.path} onclick={() => save(c)}>
          {#if saving === c.path}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="check" size=".9rem" />{/if}{t('gfx.save')}
        </button>
      </div>

      {#if c.backups.length}
        <div class="flex items-center gap-2 mt-3 text-xs flex-wrap">
          <Icon name="history" size=".8rem" style="opacity:.6" /><span class="muted">{t('gfx.backups')}</span>
          {#each c.backups.slice(0, 5) as b (b.file)}
            <button class="badge badge-ghost gap-1 hover:badge-primary" title={t('gfx.restore')} onclick={() => restore(c, b.file)}><Icon name="undo" size=".65rem" />{ago(b.at, true)}</button>
          {/each}
        </div>
      {/if}
    </section>
  {/each}
{/if}
