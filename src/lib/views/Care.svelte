<script lang="ts">
  import { onMount } from 'svelte';
  import { Tween } from 'svelte/motion';
  import { cubicOut } from 'svelte/easing';
  import { app, IS_WINDOWS, type View } from '../store.svelte';
  import { api, fmt, ago, type CareState, type CareReport } from '../api';
  import { t } from '../i18n.svelte';
  import { care } from '../care.svelte';
  import { ask } from '../ask.svelte';
  import { rpNote, rpToast, failures } from '../sys';
  import Icon from '../Icon.svelte';

  type Mod = 'junk' | 'privacy' | 'registry' | 'shortcuts' | 'startup' | 'disk' | 'updates';
  interface ModDef { key: Mod; ico: string; view: View; fix: boolean; color: string }
  const MODS: ModDef[] = [
    { key: 'junk', ico: 'trash', view: 'cleanup', fix: true, color: 'var(--cat-cache)' },
    { key: 'privacy', ico: 'eyeOff', view: 'privacy', fix: true, color: 'var(--cat-video)' },
    ...(IS_WINDOWS ? [{ key: 'registry' as Mod, ico: 'registry', view: 'registry' as View, fix: true, color: 'var(--cat-apps)' }] : []),
    { key: 'shortcuts', ico: 'unlink', view: 'shortcuts', fix: true, color: 'var(--cat-docs)' },
    ...(IS_WINDOWS ? [{ key: 'startup' as Mod, ico: 'rocket', view: 'startup' as View, fix: false, color: 'var(--cat-games)' }] : []),
    { key: 'disk', ico: 'thermo', view: 'health', fix: false, color: 'var(--cat-archives)' },
    ...(IS_WINDOWS ? [{ key: 'updates' as Mod, ico: 'upgrade', view: 'updater' as View, fix: false, color: 'var(--cat-dev)' }] : []),
  ];
  const PRIVACY_KINDS = ['cache', 'cookies', 'history', 'session', 'recent', ...(IS_WINDOWS ? ['run_history', 'typed_paths', 'search_history', 'clipboard'] : [])];

  let cfg = $state<CareState | null>(null);
  /** Fixable modules included in Fix (all by default). */
  let include = $state(new Set<Mod>(['junk', 'privacy', 'registry', 'shortcuts']));
  let showSettings = $state(false);

  onMount(() => {
    care.listen();
    api.care.config().then(c => { cfg = c; if (c.running) care.scanning = true; }).catch(() => {});
  });

  const r = $derived(care.report);
  const score = new Tween(0, { duration: 1100, easing: cubicOut });
  $effect(() => { score.target = r?.score ?? cfg?.last_score ?? 0; });

  const RING = 2 * Math.PI * 52;
  let shown = $derived(r?.score ?? cfg?.last_score ?? null);
  let tone = $derived(shown === null ? 'primary' : shown >= 85 ? 'success' : shown >= 60 ? 'warning' : 'error');
  let status = $derived(shown === null ? 'care.status.none' : shown >= 90 ? 'care.status.great' : shown >= 75 ? 'care.status.good' : shown >= 55 ? 'care.status.fair' : 'care.status.poor');

  const on = (m: Mod) => cfg?.modules.includes(m) ?? true;

  /** One line of numbers per module, plus whether it needs attention. */
  function summary(m: Mod, r: CareReport): { value: string; detail: string; state: 'ok' | 'fix' | 'review' | 'bad' } {
    switch (m) {
      case 'junk': {
        const size = r.junk.filter(j => r.plan.junk.includes(j.key)).reduce((s, j) => s + j.size, 0);
        const review = r.junk.filter(j => !j.safe).reduce((s, j) => s + j.size, 0);
        return { value: fmt(size), detail: review ? t('care.junkReview', { size: fmt(review) }) : t('care.junkDetail', { n: r.plan.junk.length }), state: size > 50 * 1024 ** 2 ? 'fix' : 'ok' };
      }
      case 'privacy': {
        const n = r.plan.privacy.length;
        const size = r.privacy.filter(p => r.plan.privacy.includes(p.id)).reduce((s, p) => s + p.size, 0);
        return { value: t('care.traces', { n }), detail: size ? fmt(size) : t('care.privacyDetail'), state: n ? 'fix' : 'ok' };
      }
      case 'registry':
        return { value: t('care.entries', { n: r.registry.length }), detail: t('care.registryDetail'), state: r.registry.length ? 'fix' : 'ok' };
      case 'shortcuts':
        return { value: t('care.broken', { n: r.shortcuts.length }), detail: t('care.shortcutsDetail'), state: r.shortcuts.length ? 'fix' : 'ok' };
      case 'startup':
        return { value: t('care.startupApps', { n: r.startup }), detail: r.startup > 8 ? t('care.startupMany') : t('care.startupOk'), state: r.startup > 8 ? 'review' : 'ok' };
      case 'disk': {
        const bad = r.disks.find(d => d.health === 'unhealthy') ?? r.disks.find(d => d.health === 'warning');
        const hot = r.disks.find(d => (d.temperature ?? 0) >= 60);
        return {
          value: bad ? t('health.' + bad.health) : t('care.disksOk', { n: r.disks.length }),
          detail: bad ? bad.name : hot ? t('care.hot', { name: hot.name, temp: hot.temperature ?? 0 }) : t('care.diskDetail'),
          state: bad?.health === 'unhealthy' ? 'bad' : bad || hot ? 'review' : 'ok',
        };
      }
      case 'updates':
        return { value: t('care.updates', { n: r.updates.length }), detail: r.updates.slice(0, 3).map(u => u.name).join(', ') || t('care.updatesOk'), state: r.updates.length ? 'review' : 'ok' };
    }
  }

  /** What Fix would do with the current selection. */
  let plan = $derived.by(() => {
    if (!r) return null;
    return {
      junk: include.has('junk') ? r.plan.junk : [],
      privacy: include.has('privacy') ? r.plan.privacy : [],
      registry: include.has('registry') ? r.plan.registry : [],
      shortcuts: include.has('shortcuts') ? r.plan.shortcuts : [],
    };
  });
  let planBytes = $derived(r && plan ? r.junk.filter(j => plan.junk.includes(j.key)).reduce((s, j) => s + j.size, 0) + r.privacy.filter(p => plan.privacy.includes(p.id)).reduce((s, p) => s + p.size, 0) : 0);
  let planCount = $derived(plan ? plan.junk.length + plan.privacy.length + plan.registry.length + plan.shortcuts.length : 0);

  async function scan() {
    try { await care.scan(); } catch (e) { app.toast(String(e), 'error'); }
  }

  async function fix() {
    if (!r || !plan || !planCount) return;
    const items = [
      plan.junk.length && { name: t('care.mod.junk'), detail: fmt(r.junk.filter(j => plan.junk.includes(j.key)).reduce((s, j) => s + j.size, 0)) },
      plan.privacy.length && { name: t('care.mod.privacy'), detail: t('care.traces', { n: plan.privacy.length }) },
      plan.registry.length && { name: t('care.mod.registry'), detail: t('care.entries', { n: plan.registry.length }) },
      plan.shortcuts.length && { name: t('care.mod.shortcuts'), detail: t('care.broken', { n: plan.shortcuts.length }) },
    ].filter(Boolean) as { name: string; detail: string }[];
    const ok = await ask({
      title: t('care.fixTitle'), text: t('care.fixText'), items, ico: 'sparkles', confirm: t('care.fixGo'),
      note: plan.registry.length ? `${rpNote()} ${t('care.regBackupNote')}` : undefined,
    });
    if (!ok) return;
    care.fixing = true;
    try {
      const out = await api.care.fix(plan);
      app.toast(t('care.fixed', { n: out.fixed, size: fmt(out.freed) }), 'success');
      rpToast(out.restore_point);
      if (out.running.length) app.toast(t('privacy.closeFirst', { names: out.running.join(', ') }), 'warning');
      failures(out.failed);
      await app.refreshDrives();
      await care.scan();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { care.fixing = false; }
  }

  function toggleInclude(m: Mod) { const s = new Set(include); s.has(m) ? s.delete(m) : s.add(m); include = s; }

  async function saveCfg(patch: Partial<CareState>) {
    if (!cfg) return;
    const next = { ...cfg, ...patch };
    cfg = next;
    try { await api.care.setConfig(next); } catch (e) { app.toast(String(e), 'error'); }
  }
  function toggleList(key: 'modules' | 'privacy_kinds', v: string) {
    if (!cfg) return;
    const list = cfg[key].includes(v) ? cfg[key].filter(x => x !== v) : [...cfg[key], v];
    saveCfg(key === 'modules' ? { modules: list } : { privacy_kinds: list });
  }
</script>

<section class="glass p-6 reveal relative overflow-hidden">
  <div class="flex items-center gap-8 flex-wrap">
    <div class="relative w-44 h-44 shrink-0">
      <svg viewBox="0 0 120 120" class="w-full h-full -rotate-90">
        <defs>
          <linearGradient id="careGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" style="stop-color:var(--color-{tone})" /><stop offset="100%" style="stop-color:var(--color-accent)" />
          </linearGradient>
        </defs>
        <circle cx="60" cy="60" r="52" fill="none" stroke-width="10" style="stroke:color-mix(in oklch,var(--color-base-content) 8%,transparent)" />
        <circle cx="60" cy="60" r="52" fill="none" stroke="url(#careGrad)" stroke-width="10" stroke-linecap="round"
          style="stroke-dasharray:{RING};stroke-dashoffset:{RING * (1 - score.current / 100)}" />
      </svg>
      {#if care.scanning}<div class="absolute inset-3 rounded-full scan-orb-lite"></div>{/if}
      <div class="absolute inset-0 grid place-items-center text-center">
        <div>
          <div class="text-5xl font-bold tracking-tight mono gauge-num">{shown === null && !care.scanning ? '—' : Math.round(score.current)}</div>
          <div class="text-xs muted">{t('care.score')}</div>
        </div>
      </div>
    </div>

    <div class="flex-1 min-w-64">
      <div class="text-xs uppercase tracking-wider font-semibold" style="color:var(--color-{tone})">{care.scanning ? t('care.scanning') : t(status)}</div>
      <h1 class="text-3xl font-bold tracking-tight mt-1">{r ? (planCount ? t('care.headIssues', { n: planCount }) : t('care.headClean')) : t('care.headIntro')}</h1>
      <p class="muted text-sm mt-1 max-w-xl">
        {#if r && planBytes}{t('care.subFree', { size: fmt(planBytes) })}{:else if r}{t('care.subNothing')}{:else}{t('care.subIntro')}{/if}
      </p>
      <div class="flex gap-2 mt-4 flex-wrap items-center">
        {#if r && planCount}
          <button class="btn btn-primary shine gap-2" disabled={care.scanning || care.fixing} onclick={fix}>
            {#if care.fixing}<span class="loading loading-spinner loading-sm"></span>{:else}<Icon name="sparkles" />{/if}
            {t('care.fix', { n: planCount })}{#if planBytes}<span class="mono opacity-80">· {fmt(planBytes)}</span>{/if}
          </button>
          <button class="btn btn-ghost gap-2" disabled={care.scanning || care.fixing} onclick={scan}><Icon name="refresh" />{t('care.rescan')}</button>
        {:else}
          <button class="btn btn-primary shine gap-2 btn-wide" disabled={care.scanning || care.fixing} onclick={scan}>
            {#if care.scanning}<span class="loading loading-spinner loading-sm"></span>{:else}<Icon name="heart" />{/if}
            {care.scanning ? t('care.scanning') : t('care.scan')}
          </button>
        {/if}
        <span class="text-xs muted flex items-center gap-1.5 flex-wrap">
          {#if r}{t('care.lastRun', { when: ago(r.at, true) })}{:else if cfg?.last_run}{t('care.lastRun', { when: ago(cfg.last_run, true) })}{/if}
          {#if cfg && cfg.schedule !== 'off'}<span class="chip-soft"><Icon name="clock" size=".75rem" />{t('care.sched.' + cfg.schedule)}</span>{/if}
        </span>
      </div>
    </div>
  </div>
</section>

<section class="grid grid-cols-1 md:grid-cols-2 2xl:grid-cols-4 gap-3">
  {#each MODS.filter(m => on(m.key)) as m, i (m.key)}
    {@const s = r ? summary(m.key, r) : null}
    {@const picked = m.fix && include.has(m.key) && s?.state === 'fix'}
    <div class="care-module reveal {picked ? 'on' : ''}" style="--i:{i + 1}">
      {#if care.scanning}<div class="scan-sweep"></div>{/if}
      <div class="flex items-center gap-3">
        <span class="ico-tile" style="background:color-mix(in oklch,{m.color} 18%,transparent);color:{m.color}"><Icon name={m.ico} size="1.1rem" /></span>
        <div class="min-w-0 flex-1">
          <div class="font-semibold text-sm truncate">{t('care.mod.' + m.key)}</div>
          <div class="text-[11px] muted truncate">{t('care.modSub.' + m.key)}</div>
        </div>
        {#if s && m.fix && s.state === 'fix'}
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={include.has(m.key)} onchange={() => toggleInclude(m.key)} title={t('care.include')} />
        {/if}
      </div>
      <div class="flex items-end justify-between gap-2 mt-1 min-h-11">
        {#if care.scanning}
          <span class="text-sm muted flex items-center gap-2"><span class="loading loading-dots loading-xs"></span>{t('care.checking')}</span>
        {:else if s}
          <div class="min-w-0">
            <div class="text-xl font-bold mono truncate">{s.value}</div>
            <div class="text-[11px] muted truncate" title={s.detail}>{s.detail}</div>
          </div>
          <span class="flex items-center gap-1.5 text-[11px] font-semibold shrink-0" style="color:var(--color-{s.state === 'ok' ? 'success' : s.state === 'bad' ? 'error' : 'warning'})">
            <i class={s.state === 'ok' ? 'dot-ok' : s.state === 'bad' ? 'dot-bad' : 'dot-warn'}></i>{t('care.state.' + s.state)}
          </span>
        {:else}
          <span class="text-sm muted">{t('care.notScanned')}</span>
        {/if}
      </div>
      <button class="btn btn-ghost btn-xs self-start -ml-2 gap-1" onclick={() => (app.view = m.view)}>{t('care.details')} <Icon name="chev" size=".8rem" /></button>
    </div>
  {/each}
</section>

<section class="glass reveal" style="--i:8">
  <button class="w-full flex items-center gap-3 p-4 text-left" onclick={() => (showSettings = !showSettings)}>
    <Icon name="settings" style="color:var(--color-primary)" />
    <div class="flex-1 min-w-0"><div class="font-semibold">{t('care.settings')}</div><div class="text-xs muted truncate">{t('care.settingsSub')}</div></div>
    <Icon name="chev" style="transition:transform .25s;transform:rotate({showSettings ? 90 : 0}deg)" />
  </button>
  {#if showSettings && cfg}
    <div class="px-4 pb-5 grid grid-cols-1 xl:grid-cols-3 gap-6">
      <div>
        <div class="text-xs uppercase tracking-wider muted font-semibold mb-2">{t('care.checks')}</div>
        <div class="flex flex-wrap gap-1.5">
          {#each MODS as m (m.key)}
            <button class="btn btn-xs {cfg.modules.includes(m.key) ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => toggleList('modules', m.key)}><Icon name={m.ico} size=".8rem" />{t('care.mod.' + m.key)}</button>
          {/each}
        </div>
      </div>
      <div>
        <div class="text-xs uppercase tracking-wider muted font-semibold mb-2">{t('care.privacyKinds')}</div>
        <div class="flex flex-wrap gap-1.5">
          {#each PRIVACY_KINDS as k (k)}
            <button class="btn btn-xs {cfg.privacy_kinds.includes(k) ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => toggleList('privacy_kinds', k)}>{t('privacy.kind.' + k)}</button>
          {/each}
        </div>
        <div class="text-[11px] muted mt-2">{t('care.privacyKindsHint')}</div>
      </div>
      <div>
        <div class="text-xs uppercase tracking-wider muted font-semibold mb-2">{t('care.schedule')}</div>
        {#if cfg.can_schedule}
          <div class="flex gap-2 flex-wrap items-center">
            <select class="select select-sm w-44" value={cfg.schedule} onchange={e => saveCfg({ schedule: (e.target as HTMLSelectElement).value as CareState['schedule'] })}>
              {#each ['off', 'daily', 'weekly', 'logon'] as s (s)}<option value={s}>{t('care.sched.' + s)}</option>{/each}
            </select>
            {#if cfg.schedule === 'daily' || cfg.schedule === 'weekly'}
              <input type="time" class="input input-sm w-28" value={cfg.time} onchange={e => saveCfg({ time: (e.target as HTMLInputElement).value })} />
            {/if}
          </div>
          <label class="flex items-start gap-3 mt-3 cursor-pointer">
            <input type="checkbox" class="toggle toggle-primary toggle-sm mt-0.5" checked={cfg.auto_clean} disabled={cfg.schedule === 'off'} onchange={e => saveCfg({ auto_clean: (e.target as HTMLInputElement).checked })} />
            <span class="text-sm">{t('care.autoClean')}<span class="block text-[11px] muted">{t('care.autoCleanHint')}</span></span>
          </label>
        {:else}
          <div class="text-sm muted">{t('care.scheduleWindows')}</div>
        {/if}
      </div>
    </div>
  {/if}
</section>
