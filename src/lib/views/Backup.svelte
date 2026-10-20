<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { open as pick } from '@tauri-apps/plugin-dialog';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type BackupConfig, type BackupJob } from '../api';
  import { t } from '../i18n.svelte';
  import { reveal } from '../actions';
  import { IS_WINDOWS } from '../nav';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  // Folder backups to another drive: incremental snapshots, keep the last N, on a schedule
  let cfg = $state<BackupConfig | null>(null);
  let draft = $state<BackupJob | null>(null);
  let suggestions = $state<[string, string, number][]>([]);
  let busy = $state<string | null>(null);
  let snaps = $state<Record<string, { name: string; path: string }[]>>({});

  async function load() {
    cfg = await api.backup.config().catch(() => ({ jobs: [], time: '20:00' }));
    for (const j of cfg.jobs) api.backup.snapshots(j.id).then(s => (snaps = { ...snaps, [j.id]: s })).catch(() => {});
  }
  onMount(() => {
    load();
    api.backup.suggestions().then(s => (suggestions = s)).catch(() => {});
    const off = listen('backup-done', () => load());
    return () => { off.then(u => u()); };
  });

  async function persist(next: BackupConfig) {
    try { await api.backup.setConfig(next); cfg = next; return true; } catch (e) { app.toast(String(e), 'error'); return false; }
  }
  const blank = (): BackupJob => ({ id: Math.random().toString(36).slice(2, 10), name: t('backup.defaultName'), sources: [], target: '', keep: 5, schedule: 'weekly', last_run: 0, last_error: '', last_size: 0 });
  async function addSource() {
    const p = await pick({ directory: true, multiple: true });
    const list = Array.isArray(p) ? p : p ? [p] : [];
    if (draft) draft.sources = [...new Set([...draft.sources, ...list])];
  }
  async function pickTarget() {
    const p = await pick({ directory: true, multiple: false });
    if (typeof p === 'string' && draft) draft.target = p;
  }
  async function saveDraft() {
    if (!cfg || !draft) return;
    const d = $state.snapshot(draft);
    const jobs = cfg.jobs.some(j => j.id === d.id) ? cfg.jobs.map(j => (j.id === d.id ? d : j)) : [...cfg.jobs, d];
    if (await persist({ ...cfg, jobs })) { draft = null; app.toast(t('backup.saved'), 'success'); load(); }
  }
  async function remove(j: BackupJob) { if (cfg) await persist({ ...cfg, jobs: cfg.jobs.filter(x => x.id !== j.id) }); }
  async function runNow(j: BackupJob) {
    busy = j.id;
    try {
      const o = await api.backup.run(j.id);
      app.toast(t('backup.done', { n: o.copied, linked: o.linked, size: fmt(o.bytes) }), o.failed.length ? 'warning' : 'success');
      await load();
    } catch (e) { app.toast(String(e), 'error'); await load(); }
    finally { busy = null; }
  }
  const leaf = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const SUG_ICON: Record<string, string> = { documents: 'doc', pictures: 'image', desktop: 'app', saves: 'game' };
</script>

<section class="glass p-5 reveal">
  <PageHead ico="archive" title={t('backup.title')} sub={t('backup.sub')}>
    {#if cfg && IS_WINDOWS}
      <label class="flex items-center gap-2 text-sm">{t('backup.at')}<input type="time" class="input input-sm w-28" value={cfg.time || '20:00'} onchange={e => persist({ ...cfg!, time: (e.target as HTMLInputElement).value })} /></label>
    {/if}
    {#if !draft}<button class="btn btn-sm btn-primary gap-1.5" onclick={() => (draft = blank())}><Icon name="plus" size=".9rem" />{t('backup.add')}</button>{/if}
  </PageHead>
</section>

{#if draft}
  <section class="glass p-5 reveal" style="--i:1">
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div class="flex flex-col gap-3">
        <label class="block"><span class="text-[11px] muted">{t('backup.name')}</span><input class="input input-sm w-full" bind:value={draft.name} /></label>
        <div>
          <div class="flex items-center justify-between"><span class="text-[11px] muted">{t('backup.sources')}</span><button class="btn btn-xs btn-ghost gap-1" onclick={addSource}><Icon name="plus" size=".75rem" />{t('backup.addFolder')}</button></div>
          <div class="flex flex-col gap-1 mt-1">
            {#each draft.sources as s (s)}
              <div class="flex items-center gap-2 text-xs soft rounded-lg px-2 py-1.5"><Icon name="folder" size=".8rem" /><span class="mono truncate flex-1" title={s}>{s}</span><button class="opacity-60 hover:opacity-100" onclick={() => (draft!.sources = draft!.sources.filter(x => x !== s))}>×</button></div>
            {:else}<div class="text-xs muted">{t('backup.noSources')}</div>{/each}
          </div>
          {#if suggestions.length}
            <div class="text-[11px] muted mt-3 mb-1">{t('backup.suggested')}</div>
            <div class="flex flex-wrap gap-1.5">
              {#each suggestions.filter(([, p]) => !draft!.sources.includes(p)).slice(0, 10) as [kind, p, size] (p)}
                <button class="badge badge-ghost gap-1 hover:badge-primary" title={p} onclick={() => (draft!.sources = [...draft!.sources, p])}><Icon name={SUG_ICON[kind] ?? 'folder'} size=".7rem" />{kind === 'saves' && !p.endsWith('Saved Games') && !p.endsWith('My Games') ? leaf(p.replace(/[\\/]Saved[\\/]SaveGames$/i, '')) : leaf(p)} · {fmt(size)}</button>
              {/each}
            </div>
          {/if}
        </div>
      </div>
      <div class="flex flex-col gap-3">
        <div>
          <span class="text-[11px] muted">{t('backup.target')}</span>
          <div class="flex gap-2 mt-1">
            <select class="select select-sm flex-1" value={app.drives.some(d => d.mount === draft!.target) ? draft.target : ''} onchange={e => { const v = (e.target as HTMLSelectElement).value; if (v) draft!.target = v; }}>
              <option value="">{draft.target && !app.drives.some(d => d.mount === draft!.target) ? draft.target : t('backup.pickDrive')}</option>
              {#each app.drives as d (d.mount)}<option value={d.mount}>{d.name || d.mount} ({d.mount}) · {t('backup.free', { size: fmt(d.available) })}</option>{/each}
            </select>
            <button class="btn btn-sm" onclick={pickTarget}><Icon name="folder" size=".9rem" /></button>
          </div>
          <div class="text-[11px] muted mt-1">{t('backup.targetHint')}</div>
        </div>
        <div class="flex gap-4 flex-wrap">
          <label class="flex items-center gap-2 text-sm">{t('backup.keep')}<input type="number" min="1" max="100" class="input input-sm w-20" bind:value={draft.keep} /></label>
          <label class="flex items-center gap-2 text-sm">{t('backup.schedule')}
            <select class="select select-sm w-32" bind:value={draft.schedule}>
              {#each ['off', 'daily', 'weekly'] as s (s)}<option value={s}>{t('backup.s.' + s)}</option>{/each}
            </select></label>
        </div>
        <div class="flex gap-2 justify-end mt-auto">
          <button class="btn btn-sm btn-ghost" onclick={() => (draft = null)}>{t('common.cancel')}</button>
          <button class="btn btn-sm btn-primary" disabled={!draft.sources.length || !draft.target || !draft.name.trim()} onclick={saveDraft}>{t('backup.save')}</button>
        </div>
      </div>
    </div>
  </section>
{/if}

{#if cfg && !cfg.jobs.length && !draft}
  <div class="glass p-10 text-center reveal" style="--i:1"><Icon name="archive" size="1.8rem" style="opacity:.4" /><div class="text-sm mt-2">{t('backup.none')}</div><div class="text-xs muted mt-1">{t('backup.noneHint')}</div></div>
{/if}

{#each cfg?.jobs ?? [] as j, i (j.id)}
  {@const s = snaps[j.id] ?? []}
  <section class="glass p-5 reveal" style="--i:{i + 2}">
    <div class="flex items-start justify-between gap-3 flex-wrap">
      <div class="flex items-start gap-3 min-w-0">
        <span class="status-ico shrink-0" style="background:color-mix(in oklch,var(--color-{j.last_error ? 'warning' : 'primary'}) 14%,transparent);color:var(--color-{j.last_error ? 'warning' : 'primary'})"><Icon name="archive" size="1.05rem" /></span>
        <div class="min-w-0">
          <div class="font-semibold">{j.name}</div>
          <div class="text-xs muted">{j.sources.map(leaf).join(', ')} → <span class="mono">{j.target}</span></div>
          <div class="text-[11px] mt-0.5" style={j.last_error ? 'color:var(--color-warning)' : ''}>
            {#if j.last_run}{t('backup.last', { ago: ago(j.last_run, true) })}{j.last_error ? ` · ${j.last_error}` : ` · ${t('backup.lastSize', { size: fmt(j.last_size) })}`}{:else}<span class="muted">{t('backup.never')}</span>{/if}
            · <span class="muted">{t('backup.s.' + j.schedule)} · {t('backup.keeps', { n: j.keep })}</span>
          </div>
        </div>
      </div>
      <div class="flex gap-1.5">
        <button class="btn btn-sm btn-primary gap-1.5" disabled={busy !== null} onclick={() => runNow(j)}>
          {#if busy === j.id}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="play" size=".85rem" />{/if}{t('backup.now')}
        </button>
        <button class="btn btn-sm btn-ghost btn-square" title={t('rules.edit')} onclick={() => (draft = { ...j, sources: [...j.sources] })}><Icon name="wrench" size=".9rem" /></button>
        <button class="btn btn-sm btn-ghost btn-square" title={t('rules.delete')} onclick={() => remove(j)}><Icon name="x" size=".9rem" /></button>
      </div>
    </div>
    {#if s.length}
      <div class="flex items-center gap-2 mt-3 flex-wrap text-xs">
        <Icon name="history" size=".8rem" style="opacity:.6" /><span class="muted">{t('backup.snapshots')}</span>
        {#each s as x (x.path)}<button class="badge badge-ghost hover:badge-primary mono" title={x.path} onclick={() => reveal(x.path)}>{x.name}</button>{/each}
      </div>
    {/if}
  </section>
{/each}
