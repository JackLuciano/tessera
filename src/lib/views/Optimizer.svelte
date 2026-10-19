<script lang="ts">
  import { app } from '../store.svelte';
  import { api, fmt, type AppProcess, type OptOutcome, type OptStatus, type Profile, type StartupApp } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';

  const PROFILES: { key: Profile; ico: string; grad: string }[] = [
    { key: 'gaming', ico: 'rocket', grad: 'linear-gradient(135deg, oklch(65% 0.24 350), oklch(62% 0.22 290))' },
    { key: 'work', ico: 'briefcase', grad: 'linear-gradient(135deg, oklch(68% 0.15 230), oklch(64% 0.17 265))' },
    { key: 'balanced', ico: 'scale', grad: 'linear-gradient(135deg, oklch(70% 0.14 170), oklch(66% 0.15 210))' },
    { key: 'battery', ico: 'battery', grad: 'linear-gradient(135deg, oklch(78% 0.16 120), oklch(70% 0.16 160))' },
  ];
  /** What each profile turns on, shown as chips on its card (mirrors PROFILES in optimizer.rs). */
  const PROFILE_TWEAKS: Record<Profile, string[]> = {
    gaming: ['game_mode', 'no_game_dvr', 'raw_mouse', 'focus', 'hags', 'game_priority'],
    work: ['focus', 'no_web_search', 'no_suggestions'],
    balanced: ['game_mode'],
    battery: ['no_background_apps', 'no_transparency', 'perf_visuals'],
  };
  const TWEAK_ICO: Record<string, string> = {
    game_mode: 'game', no_game_dvr: 'record', no_gamebar: 'game', perf_visuals: 'eye', no_transparency: 'layers',
    no_background_apps: 'app', raw_mouse: 'mouse', focus: 'bell', hags: 'cpu', net_throttling: 'network',
    game_priority: 'rocket', no_power_throttling: 'zap', no_web_search: 'search', no_suggestions: 'x',
    no_mpo: 'overview', end_task: 'x', no_activity: 'history', no_update_sharing: 'download',
  };
  /** Situational tweaks are listed apart: profiles never change them. */
  const EXTRA = ['no_mpo', 'end_task', 'no_activity', 'no_update_sharing'];

  let status = $state<OptStatus | null>(null);
  let stats = $state<{ memory: { total: number; used: number }; cpu: number } | null>(null);
  let startup = $state<StartupApp[]>([]);
  let procs = $state<AppProcess[]>([]);
  let picked = $state(new Set<string>());
  let busy = $state<string | null>(null);
  let restoreDlg: HTMLDialogElement;

  async function refresh() {
    try {
      status = await api.opt.status();
      stats = { memory: status.memory, cpu: status.cpu };
    } catch (e) { app.toast(String(e), 'error'); }
    api.opt.startup().then(s => (startup = s)).catch(() => {});
    loadProcs();
  }
  async function loadProcs() {
    procs = await api.opt.processes().catch(() => []);
    // Pre-select what is safe to close before a game; browsers, media and Office stay unless picked
    picked = new Set(procs.filter(p => ['chat', 'launcher', 'sync', 'dev', 'server', 'torrent'].includes(p.group)).map(p => p.name));
  }

  $effect(() => {
    refresh();
    const timer = setInterval(() => api.opt.stats().then(s => (stats = s)).catch(() => {}), 2500);
    return () => clearInterval(timer);
  });

  function report(out: OptOutcome, ok: string) {
    app.toast(ok, 'success');
    if (out.reboot) app.toast(t('opt.reboot'), 'info');
    if (out.failed.length) app.toast(t('opt.failed', { n: out.failed.length, first: out.failed[0] }), 'warning');
  }

  async function activate(p: Profile) {
    busy = p;
    try {
      const out = await api.opt.applyProfile(p);
      report(out, t('opt.applied', { profile: t('opt.profile.' + p), n: out.applied }));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function toggle(key: string, on: boolean) {
    busy = key;
    try {
      const out = await api.opt.setTweak(key, on);
      report(out, t(on ? 'opt.tweakOn' : 'opt.tweakOff', { name: t('opt.tweak.' + key) }));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function setPlan(guid: string) {
    try { await api.opt.setPowerPlan(guid); await refresh(); } catch (e) { app.toast(String(e), 'error'); }
  }
  async function setStartup(a: StartupApp, enabled: boolean) {
    try {
      await api.opt.setStartup(a.id, enabled);
      a.enabled = enabled;
    } catch (e) { app.toast(String(e), 'error'); startup = [...startup]; }
  }
  async function closeApps() {
    const pids = procs.filter(p => picked.has(p.name)).flatMap(p => p.pids);
    const out = await api.opt.kill(pids);
    report(out, t('opt.closed', { n: picked.size }));
    setTimeout(loadProcs, 800);
  }
  async function restore() {
    busy = 'restore';
    try {
      const out = await api.opt.restore();
      restoreDlg.close();
      report(out, t('opt.restored'));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }

  const tweakOn = (k: string) => status?.tweaks.find(x => x.key === k)?.on ?? false;
  let memPct = $derived(stats ? (stats.memory.used / Math.max(1, stats.memory.total)) * 100 : 0);
  let closeSize = $derived(procs.filter(p => picked.has(p.name)).reduce((s, p) => s + p.memory, 0));
</script>

{#if status && !status.supported}
  <div class="glass p-10 text-center muted">{t('opt.windowsOnly')}</div>
{:else}
  <section class="glass p-5 reveal">
    <div class="flex items-start gap-4 flex-wrap mb-4">
      <div class="flex-1 min-w-64">
        <h2 class="text-lg font-semibold flex items-center gap-2"><Icon name="gauge" style="color:var(--color-primary)" />{t('opt.title')}</h2>
        <p class="text-xs muted mt-0.5 max-w-2xl">{t('opt.sub')}</p>
      </div>
      {#if status?.has_backup}
        <button class="btn btn-sm btn-ghost gap-1.5" onclick={() => restoreDlg.showModal()}><Icon name="history" size=".95rem" />{t('opt.restore')}</button>
      {/if}
    </div>
    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3 stagger">
      {#each PROFILES as p (p.key)}
        {@const active = status?.active === p.key}
        <div class="profile-card {active ? 'active' : ''}" style="--grad:{p.grad}">
          <div class="flex items-center gap-3">
            <span class="profile-ico"><Icon name={p.ico} size="1.3rem" /></span>
            <div class="min-w-0 flex-1">
              <div class="font-semibold truncate">{t('opt.profile.' + p.key)}</div>
              {#if active}<div class="text-[11px] font-semibold" style="color:var(--color-success)">● {t('opt.active')}</div>{/if}
            </div>
          </div>
          <p class="text-xs muted mt-3 flex-1">{t('opt.profile.' + p.key + '.desc')}</p>
          <div class="flex flex-wrap gap-1 mt-3 content-start">
            {#each PROFILE_TWEAKS[p.key] as k (k)}<span class="tag"><Icon name={TWEAK_ICO[k]} size=".7rem" />{t('opt.tweak.' + k)}</span>{/each}
          </div>
          <button class="btn btn-sm w-full mt-4 {active ? 'btn-ghost' : 'btn-primary'}" disabled={!!busy || active} onclick={() => activate(p.key)}>
            {#if busy === p.key}<span class="loading loading-spinner loading-xs"></span>{/if}
            {active ? t('opt.active') : t('opt.activate')}
          </button>
        </div>
      {/each}
    </div>
  </section>

  {#if status && !status.elevated}
    <div class="glass p-3 px-4 reveal flex items-center gap-3 text-sm" style="--i:1">
      <Icon name="shield" style="color:var(--color-warning)" />
      <span class="flex-1">{t('opt.needAdmin')}</span>
      <button class="btn btn-xs btn-ghost" onclick={() => api.relaunchAsAdmin().catch(e => app.toast(String(e), 'info'))}>{t('engine.elevate')}</button>
    </div>
  {/if}

  <section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:2">
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <div><div class="font-semibold">{t('opt.tweaks')}</div><div class="text-xs muted">{t('opt.tweaksSub')}</div></div>
        {#if status?.power_plans.length}
          <label class="flex items-center gap-2 text-xs muted">
            <Icon name="power" size=".9rem" />{t('opt.powerPlan')}
            <select class="select select-sm w-52" value={status.power_plans.find(p => p.active)?.guid} onchange={e => setPlan((e.target as HTMLSelectElement).value)}>
              {#each status.power_plans as p (p.guid)}<option value={p.guid}>{p.name}</option>{/each}
            </select>
          </label>
        {/if}
      </div>
      {#snippet row(tw: OptStatus['tweaks'][number])}
        {@const locked = tw.admin && !status?.elevated}
        <div class="flex items-center gap-3 py-3 {locked ? 'opacity-55' : ''}">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name={TWEAK_ICO[tw.key] ?? 'zap'} size="1.05rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="font-medium text-sm flex items-center gap-1.5 flex-wrap">
              {t('opt.tweak.' + tw.key)}
              {#if tw.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" />{t('opt.admin')}</span>{/if}
              {#if tw.reboot}<span class="tag">{t('opt.restart')}</span>{/if}
            </div>
            <div class="text-xs muted">{t('opt.tweak.' + tw.key + '.hint')}</div>
          </div>
          {#if busy === tw.key}<span class="loading loading-spinner loading-xs"></span>{/if}
          <input type="checkbox" class="toggle toggle-primary toggle-sm" checked={tweakOn(tw.key)} disabled={locked || !!busy}
            onchange={e => toggle(tw.key, (e.target as HTMLInputElement).checked)} />
        </div>
      {/snippet}
      <div class="divide-y divide-[var(--glass-border)] mt-2">
        {#each (status?.tweaks ?? []).filter(x => !EXTRA.includes(x.key)) as tw (tw.key)}{@render row(tw)}{/each}
      </div>
      <div class="text-xs uppercase tracking-wider muted font-semibold mt-5">{t('opt.extras')}</div>
      <div class="text-xs muted">{t('opt.extrasSub')}</div>
      <div class="divide-y divide-[var(--glass-border)] mt-1">
        {#each (status?.tweaks ?? []).filter(x => EXTRA.includes(x.key)) as tw (tw.key)}{@render row(tw)}{/each}
      </div>
    </div>

    <div class="flex flex-col gap-4">
      <div class="glass p-5 reveal" style="--i:3">
        <div class="font-semibold mb-3">{t('opt.system')}</div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <div class="flex justify-between text-xs"><span class="muted">{t('opt.cpu')}</span><span class="mono font-semibold">{(stats?.cpu ?? 0).toFixed(0)}%</span></div>
            <div class="bar mt-1.5"><i style="width:{stats?.cpu ?? 0}%;background:linear-gradient(90deg,var(--color-primary),var(--color-accent))"></i></div>
          </div>
          <div>
            <div class="flex justify-between text-xs"><span class="muted">{t('opt.ram')}</span><span class="mono font-semibold">{fmt(stats?.memory.used ?? 0)} / {fmt(stats?.memory.total ?? 0)}</span></div>
            <div class="bar mt-1.5"><i style="width:{memPct}%;background:{memPct > 85 ? 'var(--color-warning)' : 'linear-gradient(90deg,var(--color-secondary),var(--color-primary))'}"></i></div>
          </div>
        </div>
      </div>

      <div class="glass p-5 reveal" style="--i:4">
        <div class="flex items-center justify-between gap-2">
          <div><div class="font-semibold">{t('opt.apps')}</div><div class="text-xs muted">{t('opt.appsSub')}</div></div>
          <button class="btn btn-ghost btn-xs btn-square" title={t('action.refresh')} onclick={loadProcs}><Icon name="refresh" size=".9rem" /></button>
        </div>
        {#if !procs.length}
          <div class="text-sm muted py-6 text-center">{t('opt.appsNone')}</div>
        {:else}
          <div class="flex flex-col gap-0.5 mt-2">
            {#each procs as p (p.name)}
              <label class="row-hover flex items-center gap-3 px-2 py-1.5 cursor-pointer">
                <input type="checkbox" class="checkbox checkbox-primary checkbox-xs" checked={picked.has(p.name)}
                  onchange={e => { const s = new Set(picked); (e.target as HTMLInputElement).checked ? s.add(p.name) : s.delete(p.name); picked = s; }} />
                <span class="flex-1 min-w-0 truncate text-sm">{p.name.replace(/\.exe$/i, '')}</span>
                <span class="tag">{t('opt.group.' + p.group)}</span>
                <span class="mono text-xs w-16 text-right">{fmt(p.memory)}</span>
              </label>
            {/each}
          </div>
          <button class="btn btn-sm btn-primary w-full mt-3" disabled={!picked.size} onclick={closeApps}>{t('opt.close', { size: fmt(closeSize) })}</button>
        {/if}
      </div>

      <div class="glass p-5 reveal" style="--i:5">
        <div class="font-semibold">{t('opt.startup')}</div>
        <div class="text-xs muted">{t('opt.startupSub')}</div>
        <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
          {#each startup as a (a.id)}
            {@const locked = a.scope === 'machine' && !status?.elevated}
            <div class="flex items-center gap-3 py-2 {locked ? 'opacity-55' : ''}">
              <div class="flex-1 min-w-0">
                <div class="text-sm font-medium truncate">{a.name}</div>
                <div class="text-[11px] muted mono truncate" title={a.command}>{a.command}</div>
              </div>
              {#if a.scope === 'machine'}<span class="tag" title={t('opt.startupMachine')}><Icon name="shield" size=".65rem" /></span>{/if}
              <input type="checkbox" class="toggle toggle-sm toggle-primary" checked={a.enabled} disabled={locked}
                onchange={e => setStartup(a, (e.target as HTMLInputElement).checked)} />
            </div>
          {:else}
            <div class="text-sm muted py-4 text-center">—</div>
          {/each}
        </div>
      </div>
    </div>
  </section>
{/if}

<dialog bind:this={restoreDlg} class="modal">
  <div class="modal-box glass max-w-md">
    <div class="flex items-center gap-3">
      <span class="w-10 h-10 rounded-xl grid place-items-center soft" style="color:var(--color-primary)"><Icon name="history" /></span>
      <h3 class="text-lg font-semibold">{t('opt.restore')}?</h3>
    </div>
    <p class="text-sm muted mt-3">{t('opt.restoreHint')}</p>
    <div class="modal-action">
      <button class="btn btn-ghost" onclick={() => restoreDlg.close()}>{t('common.cancel')}</button>
      <button class="btn btn-primary" disabled={busy === 'restore'} onclick={restore}>{#if busy === 'restore'}<span class="loading loading-spinner loading-sm"></span>{/if}{t('opt.restoreGo')}</button>
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
