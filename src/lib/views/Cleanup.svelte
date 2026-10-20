<script lang="ts">
  import { untrack } from 'svelte';
  import { app } from '../store.svelte';
  import { onMount } from 'svelte';
  import { api, fmt, num, type JunkGroup, type SysJunk } from '../api';
  import { t } from '../i18n.svelte';
  import { confirmDelete } from '../confirm.svelte';
  import { ask } from '../ask.svelte';
  import { failures, rpNote } from '../sys';
  import { IS_WINDOWS } from '../nav';
  import Icon from '../Icon.svelte';

  // ---- system junk: known places, no drive scan needed ----
  const SYS_ICO: Record<string, string> = { user_temp: 'cache', win_temp: 'cache', thumbs: 'image', crash: 'alert', shaders: 'game', wu_cache: 'download', recycle: 'trash', logs: 'doc', app_cache: 'cache', old_downloads: 'download' };
  let sys = $state<SysJunk[] | null>(null);
  let sysOn = $state(new Set<string>());
  let sysOpen = $state<string | null>(null);
  let sysBusy = $state(false);

  async function loadSys() {
    sys = null;
    const items = await api.sysJunk().catch(e => { app.toast(String(e), 'error'); return []; });
    sys = items;
    sysOn = new Set(items.filter(i => i.safe && (!i.admin || app.elevated)).map(i => i.key));
  }
  onMount(loadSys);

  let sysTotal = $derived((sys ?? []).filter(i => sysOn.has(i.key)).reduce((s, i) => s + i.size, 0));
  function sysToggle(k: string) { const s = new Set(sysOn); s.has(k) ? s.delete(k) : s.add(k); sysOn = s; }

  async function cleanSys() {
    const picked = (sys ?? []).filter(i => sysOn.has(i.key));
    const ok = await ask({
      title: t('sysjunk.confirm', { size: fmt(sysTotal) }), text: t('sysjunk.confirmText'), ico: 'trash', confirm: t('sysjunk.clean'),
      items: picked.map(i => ({ name: t('sysjunk.' + i.key), detail: fmt(i.size) })),
    });
    if (!ok) return;
    sysBusy = true;
    try {
      const out = await api.cleanSysJunk(picked.map(i => i.key));
      app.toast(t('sysjunk.cleaned', { size: fmt(out.freed) }), 'success');
      if (out.locked) app.toast(t('sysjunk.locked', { n: out.locked }), 'info');
      failures(out.failed);
      await app.refreshDrives();
      await loadSys();
    } finally { sysBusy = false; }
  }

  // ---- Windows component store (DISM), long-running ----
  let dismBusy = $state(false);
  async function dism() {
    const ok = await ask({ title: t('dism.confirm'), text: t('dism.confirmText'), ico: 'layers', confirm: t('dism.run'), note: rpNote() });
    if (!ok) return;
    dismBusy = true;
    try { const freed = await api.componentCleanup(); app.toast(t('dism.done', { size: fmt(freed) }), 'success'); await app.refreshDrives(); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { dismBusy = false; }
  }

  // ---- regenerable folders found in the scanned tree ----
  let groups = $state<JunkGroup[]>([]);
  let on = $state(new Set<string>());
  let open = $state<string | null>(null);
  let loading = $state(false);

  let loadedVersion = -1;
  $effect(() => {
    const version = app.version;
    void app.liveVersion;
    // summary is replaced on every live tick; reading it untracked keeps this from re-running constantly
    if (!untrack(() => app.summary)) return;
    // A rescan or delete reloads visibly and resets the selection; live changes refresh silently
    const fresh = version !== loadedVersion;
    if (fresh) loading = true;
    api.junk().then(g => {
      groups = g;
      const prev = untrack(() => on);
      on = new Set(g.filter(x => (fresh ? x.safe : prev.has(x.key))).map(x => x.key));
      loadedVersion = version;
    }).finally(() => (loading = false));
  });

  let selected = $derived(groups.filter(g => on.has(g.key)));
  let total = $derived(selected.reduce((s, g) => s + g.size, 0));
  let used = $derived(app.drive ? app.drive.total - app.drive.available : 0);
  let cap = $derived(app.drive?.total ?? 1);

  // Rules are defined in Rust with English text; the UI translates them by key and falls back to that text
  const tr = (key: string, fallback: string) => { const s = t(key); return s === key ? fallback : s; };
  const label = (g: JunkGroup) => tr(`junk.${g.key}`, g.label);
  const hint = (g: JunkGroup) => tr(`junk.${g.key}.hint`, g.hint);

  function toggle(k: string) { const s = new Set(on); s.has(k) ? s.delete(k) : s.add(k); on = s; }
  function clean(ev: MouseEvent) {
    // One line per group in the dialog, but every matched folder id is deleted
    confirmDelete.ask(selected.map((g, i) => ({ id: -1 - i, name: `${label(g)} (${g.ids.length})`, size: g.size })), ev);
    confirmDelete.idsOverride = selected.flatMap(g => g.ids);
  }
</script>

<section class="glass p-4 reveal">
  <div class="flex items-center justify-between gap-2 flex-wrap">
    <div>
      <div class="font-semibold flex items-center gap-2"><Icon name="trash" style="color:var(--color-primary)" />{t('sysjunk.title')}</div>
      <div class="text-xs muted mt-0.5">{t('sysjunk.sub')}</div>
    </div>
    <div class="flex items-center gap-2">
      <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={loadSys} disabled={sysBusy}><Icon name="refresh" size=".9rem" /></button>
      <button class="btn btn-primary btn-sm gap-2" disabled={!sysTotal || sysBusy} onclick={cleanSys}>
        {#if sysBusy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="sparkles" size="1rem" />{/if}
        {t('sysjunk.clean')} <span class="mono">{fmt(sysTotal)}</span>
      </button>
    </div>
  </div>
  {#if sys === null}
    <div class="p-6 text-center"><span class="loading loading-dots"></span></div>
  {:else if !sys.length}
    <div class="muted text-sm p-6 text-center">{t('cleanup.nothing')}</div>
  {:else}
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-2 mt-3">
      {#each sys as j, i (j.key)}
        {@const locked = j.admin && !app.elevated}
        <div class="pick p-3 reveal {sysOn.has(j.key) ? 'on' : ''} {locked ? 'opacity-55' : ''}" style="--i:{i}">
          <div class="flex items-center gap-3 {locked ? '' : 'cursor-pointer'}" role="button" tabindex="0" onclick={() => !locked && sysToggle(j.key)} onkeydown={e => e.key === ' ' && !locked && sysToggle(j.key)}>
            <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={sysOn.has(j.key)} disabled={locked} tabindex="-1" />
            <span class="w-9 h-9 rounded-lg grid place-items-center shrink-0 soft"><Icon name={SYS_ICO[j.key] ?? 'cache'} size="1rem" /></span>
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium truncate flex items-center gap-1.5">{t('sysjunk.' + j.key)}{#if j.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" />{t('opt.admin')}</span>{/if}</div>
              <div class="text-[11px] muted truncate">{t('sysjunk.' + j.key + '.hint')} · {t('common.nItems', { n: num(j.count) })}</div>
            </div>
            <span class="badge badge-soft badge-xs {j.safe ? 'badge-success' : 'badge-warning'}">{j.safe ? t('cleanup.safe') : t('cleanup.review')}</span>
            <span class="mono text-sm font-semibold w-20 text-right">{fmt(j.size)}</span>
            {#if j.sample.length}
              <button class="btn btn-ghost btn-xs btn-square" onclick={e => { e.stopPropagation(); sysOpen = sysOpen === j.key ? null : j.key; }}><Icon name="info" size=".95rem" /></button>
            {/if}
          </div>
          {#if sysOpen === j.key}
            <div class="mt-2 ml-12 flex flex-col gap-0.5">{#each j.sample as s (s)}<div class="text-[11px] muted mono truncate selectable">{s}</div>{/each}</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>

{#if IS_WINDOWS}
  <section class="glass p-4 reveal flex items-center gap-4 flex-wrap" style="--i:1">
    <span class="w-10 h-10 rounded-xl grid place-items-center soft shrink-0" style="color:var(--color-primary)"><Icon name="layers" /></span>
    <div class="flex-1 min-w-56">
      <div class="font-semibold text-sm flex items-center gap-1.5">{t('dism.title')}<span class="tag tag-warn"><Icon name="shield" size=".6rem" />{t('opt.admin')}</span></div>
      <div class="text-xs muted">{t('dism.sub')}</div>
    </div>
    <button class="btn btn-sm gap-1.5" disabled={dismBusy || !app.elevated} onclick={dism}>{#if dismBusy}<span class="loading loading-spinner loading-xs"></span>{t('dism.running')}{:else}<Icon name="sparkles" size=".9rem" />{t('dism.run')}{/if}</button>
  </section>
{/if}

{#if !app.summary}
  <div class="glass p-6 text-center muted text-sm reveal" style="--i:2">{t('cleanup.scanForMore')}</div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-12 gap-4">
    <div class="glass p-4 xl:col-span-8 reveal flex flex-col">
      <div class="flex items-center justify-between gap-2 flex-wrap">
        <div>
          <div class="font-semibold flex items-center gap-2"><Icon name="cleanup" style="color:var(--color-primary)" />{t('cleanup.projectTitle')}</div>
          <div class="text-xs mt-0.5" style={app.permanentDelete ? 'color:var(--color-error)' : ''}>
            <span class={app.permanentDelete ? '' : 'muted'}>{app.permanentDelete ? t('cleanup.permOn') : t('cleanup.sub')}</span>
          </div>
        </div>
        <div class="flex gap-1">
          <button class="btn btn-ghost btn-xs" onclick={() => (on = new Set(groups.map(g => g.key)))}>{t('common.selectAll')}</button>
          <button class="btn btn-ghost btn-xs" onclick={() => (on = new Set())}>{t('common.none')}</button>
        </div>
      </div>
      <div class="flex flex-col gap-2 mt-4">
        {#if loading}
          <div class="p-8 text-center"><span class="loading loading-dots"></span></div>
        {:else if !groups.length}
          <div class="muted text-sm p-8 text-center">{t('cleanup.nothing')}</div>
        {/if}
        {#each groups as g, i (g.key)}
          <div class="reveal rounded-[.9rem] border p-3 transition {on.has(g.key) ? '' : 'hairline'}" style="--i:{i};{on.has(g.key) ? 'border-color:color-mix(in oklch,var(--color-primary) 45%,transparent);background:color-mix(in oklch,var(--color-primary) 7%,transparent)' : ''}">
            <div class="flex items-center gap-3 cursor-pointer" role="button" tabindex="0" onclick={() => toggle(g.key)} onkeydown={e => e.key === ' ' && toggle(g.key)}>
              <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={on.has(g.key)} tabindex="-1" />
              <span class="w-9 h-9 rounded-lg grid place-items-center shrink-0 soft"><Icon name="cache" size="1rem" /></span>
              <div class="flex-1 min-w-0">
                <div class="text-sm font-medium truncate">{label(g)} <span class="muted font-normal">· {t('cleanup.nFolders', { n: g.ids.length })}</span></div>
                <div class="text-[11px] muted truncate">{hint(g)}</div>
              </div>
              <span class="badge badge-soft badge-xs {g.safe ? 'badge-success' : 'badge-warning'}">{g.safe ? t('cleanup.safe') : t('cleanup.review')}</span>
              <span class="mono text-sm font-semibold w-20 text-right">{fmt(g.size)}</span>
              <button class="btn btn-ghost btn-xs btn-square" onclick={e => { e.stopPropagation(); open = open === g.key ? null : g.key; }}><Icon name="info" size=".95rem" /></button>
            </div>
            {#if open === g.key}
              <div class="mt-2 ml-12 flex flex-col gap-0.5">
                {#each g.sample as s (s)}<div class="text-[11px] muted mono truncate selectable">{s}</div>{/each}
                {#if g.ids.length > g.sample.length}<div class="text-[11px] muted">{t('cleanup.more', { n: g.ids.length - g.sample.length })}</div>{/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>

    <div class="xl:col-span-4">
      <div class="glass p-5 reveal sticky top-24" style="--i:1">
        <div class="text-sm muted">{t('cleanup.selected')}</div>
        <div class="text-4xl font-bold mono grad-text">{fmt(total)}</div>
        <div class="text-xs muted mt-1">{t('cleanup.after', { free: fmt(cap - used + total) })}</div>
        {#if app.drive}
          <div class="mt-4">
            <div class="flex justify-between text-xs muted mb-1"><span>{t('cleanup.now')}</span><span class="mono">{Math.round((used / cap) * 100)}%</span></div>
            <div class="bar" style="height:10px"><i style="width:{(used / cap) * 100}%;background:var(--color-warning)"></i></div>
            <div class="flex justify-between text-xs muted mb-1 mt-3"><span>{t('cleanup.afterLbl')}</span><span class="mono">{Math.round(((used - total) / cap) * 100)}%</span></div>
            <div class="bar" style="height:10px"><i class="grad-bg" style="width:{((used - total) / cap) * 100}%"></i></div>
          </div>
        {/if}
        <button class="btn w-full mt-5 shine gap-2 {app.permanentDelete ? 'btn-error' : 'btn-primary'}" disabled={!total} onclick={clean}>
          <Icon name="trash" />{app.permanentDelete ? t('action.deletePerm') : t('cleanup.freeUp')} <span class="mono">{fmt(total)}</span>
        </button>
        <div class="text-[11px] muted text-center mt-2">{t('cleanup.shiftHint')}</div>
      </div>
    </div>
  </section>
{/if}
