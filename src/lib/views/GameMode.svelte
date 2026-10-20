<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type GameModeConfig, type GameStatus, type Game, type RunningApp, type GameSession, type ExeSettings } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  let cfg = $state<GameModeConfig | null>(null);
  let status = $state<GameStatus | null>(null);
  let games = $state<Game[]>([]);
  let running = $state<RunningApp[]>([]);
  let sessions = $state<GameSession[]>([]);
  let open = $state<string | null>(null);
  let exes = $state<Record<string, [string, ExeSettings][]>>({});
  let addClose = $state('');
  let addKeep = $state('');

  onMount(() => {
    api.gamemode.config().then(c => (cfg = c)).catch(e => app.toast(String(e), 'error'));
    api.gamemode.status().then(s => (status = s)).catch(() => {});
    api.games.list().then(g => (games = g)).catch(() => {});
    api.gamemode.runningApps().then(r => (running = r)).catch(() => {});
    api.gamemode.sessions().then(s => (sessions = s)).catch(() => {});
    let offs: (() => void)[] = [];
    listen<GameStatus>('game-status', e => (status = e.payload)).then(u => offs.push(u));
    listen<GameSession>('game-ended', e => (sessions = [e.payload, ...sessions])).then(u => offs.push(u));
    return () => offs.forEach(o => o());
  });

  async function save(patch: Partial<GameModeConfig>) {
    if (!cfg) return;
    cfg = { ...cfg, ...patch };
    try { await api.gamemode.setConfig(cfg); } catch (e) { app.toast(String(e), 'error'); }
  }

  const ACTIONS: { key: keyof GameModeConfig; ico: string; admin?: boolean }[] = [
    { key: 'switch_plan', ico: 'power' }, { key: 'apply_profile', ico: 'gauge', admin: true }, { key: 'clear_standby', ico: 'memory', admin: true },
    { key: 'timer', ico: 'clock' }, { key: 'unpark', ico: 'cpu', admin: true }, { key: 'reopen', ico: 'refresh' },
    { key: 'record_fps', ico: 'activity', admin: true }, { key: 'overlay', ico: 'widget' },
  ];

  function addTo(list: 'close' | 'keep', raw: string) {
    if (!cfg) return;
    const id = raw.trim().toLowerCase().replace(/\.exe$/, '');
    if (!id || cfg[list].includes(id)) return;
    const other = list === 'close' ? 'keep' : 'close';
    // An app is on one list only: adding it to one removes it from the other
    save({ [list]: [...cfg[list], id], [other]: cfg[other].filter(x => x !== id) } as Partial<GameModeConfig>);
    if (list === 'close') addClose = ''; else addKeep = '';
  }
  function removeFrom(list: 'close' | 'keep', id: string) {
    if (cfg) save({ [list]: cfg[list].filter(x => x !== id) } as Partial<GameModeConfig>);
  }

  const boostOf = (g: Game) => cfg?.per_game[g.id]?.boost ?? 'default';
  function setBoost(g: Game, v: string) {
    if (!cfg) return;
    const per = { ...cfg.per_game };
    if (v === 'default') {
      const { [g.id]: _, ...rest } = per;
      save({ per_game: rest });
    } else {
      per[g.id] = { ...(per[g.id] ?? { close: [], keep: [] }), boost: v as 'ask' | 'always' | 'never' };
      save({ per_game: per });
    }
  }
  function perList(g: Game, list: 'close' | 'keep', id: string, add: boolean) {
    if (!cfg) return;
    const cur = cfg.per_game[g.id] ?? { boost: cfg.default_boost, close: [], keep: [] };
    const next = add ? [...new Set([...cur[list], id])] : cur[list].filter(x => x !== id);
    save({ per_game: { ...cfg.per_game, [g.id]: { ...cur, [list]: next } } });
  }

  async function toggleOpen(g: Game) {
    open = open === g.id ? null : g.id;
    if (open && !exes[g.id]) exes = { ...exes, [g.id]: await api.games.exes(g.dir).catch(() => []) };
  }
  async function setExe(g: Game, exe: string, s: ExeSettings) {
    try {
      await api.games.setExe(exe, s);
      exes = { ...exes, [g.id]: (exes[g.id] ?? []).map(([e, old]) => [e, e === exe ? s : old] as [string, ExeSettings]) };
      app.toast(t('gm.exeSaved'), 'success');
    } catch (e) { app.toast(String(e), 'error'); }
  }

  async function boostNow() {
    try { await api.gamemode.boostNow(); } catch (e) { app.toast(String(e), 'error'); }
  }

  const appName = (id: string) => running.find(r => r.id === id)?.name ?? id;
  const minutes = (s: GameSession) => Math.max(1, Math.round((s.end - s.start) / 60));
  let suggestions = $derived(running.filter(r => cfg && !cfg.close.includes(r.id) && !cfg.keep.includes(r.id)));
</script>

<section class="glass p-5 reveal relative overflow-hidden">
  <PageHead ico="rocket" title={t('gm.title')} sub={t('gm.sub')}>
    {#if cfg}
      <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="toggle toggle-primary" checked={cfg.enabled} onchange={e => save({ enabled: (e.target as HTMLInputElement).checked })} />{t('gm.enabled')}</label>
    {/if}
  </PageHead>

  <div class="mt-5 grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_auto] gap-4 items-center">
    <div class="flex items-center gap-4 min-w-0">
      <div class="gm-status {status?.boosted ? 'on' : status?.game ? 'idle' : ''}">
        <Icon name={status?.boosted ? 'zap' : 'game'} size="1.6rem" />
      </div>
      <div class="min-w-0">
        {#if status?.game}
          <div class="text-xs uppercase tracking-wider font-semibold" style="color:var(--color-{status.boosted ? 'success' : 'warning'})">{status.boosted ? t('gm.boosted') : t('gm.notBoosted')}</div>
          <div class="text-2xl font-bold truncate">{status.game.name}</div>
          <div class="text-xs muted">{t('gm.since', { when: ago(status.since, true) })}{#if status.closed.length} · {t('gm.closedN', { n: status.closed.length })}{/if}</div>
        {:else if status?.pending}
          <div class="text-xs uppercase tracking-wider font-semibold" style="color:var(--color-primary)">{t('gm.asking')}</div>
          <div class="text-2xl font-bold truncate">{status.pending.name}</div>
        {:else}
          <div class="text-xs uppercase tracking-wider muted font-semibold">{t('gm.waiting')}</div>
          <div class="text-2xl font-bold">{t('gm.noGame')}</div>
          <div class="text-xs muted">{t('gm.noGameSub', { n: games.length })}</div>
        {/if}
      </div>
    </div>
    <div class="flex items-center gap-2 flex-wrap">
      {#if cfg}
        <label class="flex items-center gap-2 text-xs muted">{t('gm.default')}
          <select class="select select-sm w-40" value={cfg.default_boost} onchange={e => save({ default_boost: (e.target as HTMLSelectElement).value as GameModeConfig['default_boost'] })}>
            {#each ['ask', 'always', 'never'] as b (b)}<option value={b}>{t('gm.boost.' + b)}</option>{/each}
          </select>
        </label>
      {/if}
      {#if status?.game && !status.boosted}<button class="btn btn-primary btn-sm shine gap-1.5" onclick={boostNow}><Icon name="zap" size=".95rem" />{t('gm.boostNow')}</button>{/if}
    </div>
  </div>
</section>

<AdminBanner text={t('gm.needAdmin')} />

{#if cfg}
  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:2">
      <div class="font-semibold">{t('gm.actions')}</div>
      <div class="text-xs muted">{t('gm.actionsSub')}</div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mt-3">
        {#each ACTIONS as a (a.key)}
          {@const on = !!cfg[a.key]}
          <label class="pick p-3 flex items-start gap-3 cursor-pointer {on ? 'on' : ''}">
            <input type="checkbox" class="toggle toggle-primary toggle-sm mt-0.5" checked={on} onchange={e => save({ [a.key]: (e.target as HTMLInputElement).checked } as Partial<GameModeConfig>)} />
            <span class="min-w-0">
              <span class="text-sm font-medium flex items-center gap-1.5"><Icon name={a.ico} size=".9rem" style="opacity:.7" />{t('gm.act.' + a.key)}{#if a.admin}<span class="tag tag-warn"><Icon name="shield" size=".6rem" /></span>{/if}</span>
              <span class="block text-[11px] muted">{t('gm.act.' + a.key + '.hint')}</span>
            </span>
          </label>
        {/each}
      </div>
    </div>

    <div class="glass p-5 reveal" style="--i:3">
      <div class="font-semibold">{t('gm.apps')}</div>
      <div class="text-xs muted">{t('gm.appsSub')}</div>
      {#snippet appList(list: 'close' | 'keep', value: string, set: (v: string) => void)}
        <div class="mt-4">
          <div class="text-xs uppercase tracking-wider font-semibold flex items-center gap-1.5" style="color:var(--color-{list === 'close' ? 'error' : 'success'})">
            <Icon name={list === 'close' ? 'x' : 'lock'} size=".8rem" />{t('gm.list.' + list)}
          </div>
          <div class="flex flex-wrap gap-1.5 mt-2 min-h-8">
            {#each cfg?.[list] ?? [] as id (id)}
              <span class="chip-soft gap-1">{appName(id)}<button class="opacity-60 hover:opacity-100" onclick={() => removeFrom(list, id)} aria-label="remove">×</button></span>
            {:else}
              <span class="text-xs muted">{t('gm.listEmpty')}</span>
            {/each}
          </div>
          <div class="flex gap-2 mt-2">
            <input class="input input-sm flex-1" list="running-apps" placeholder={t('gm.addPlaceholder')} {value} oninput={e => set((e.target as HTMLInputElement).value)} onkeydown={e => e.key === 'Enter' && addTo(list, value)} />
            <button class="btn btn-sm" disabled={!value.trim()} onclick={() => addTo(list, value)}><Icon name="plus" size=".9rem" /></button>
          </div>
        </div>
      {/snippet}
      {@render appList('close', addClose, v => (addClose = v))}
      {@render appList('keep', addKeep, v => (addKeep = v))}
      <datalist id="running-apps">{#each suggestions as r (r.id)}<option value={r.id}>{r.name} · {fmt(r.memory)}</option>{/each}</datalist>
      {#if suggestions.length}
        <div class="text-[11px] muted mt-3">{t('gm.runningNow')}</div>
        <div class="flex flex-wrap gap-1.5 mt-1">
          {#each suggestions.slice(0, 12) as r (r.id)}
            <span class="tag gap-1">{r.name}<button title={t('gm.list.close')} onclick={() => addTo('close', r.id)} style="color:var(--color-error)">−</button><button title={t('gm.list.keep')} onclick={() => addTo('keep', r.id)} style="color:var(--color-success)">+</button></span>
          {/each}
        </div>
      {/if}
    </div>
  </section>

  <section class="glass p-5 reveal" style="--i:4">
    <div class="font-semibold">{t('gm.perGame')}</div>
    <div class="text-xs muted">{t('gm.perGameSub')}</div>
    {#if !games.length}
      <div class="text-sm muted text-center py-8">{t('lib.none')}</div>
    {:else}
      <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
        {#each games as g (g.id)}
          {@const per = cfg.per_game[g.id]}
          <div class="py-2.5">
            <div class="flex items-center gap-3">
              <button class="flex items-center gap-3 flex-1 min-w-0 text-left" onclick={() => toggleOpen(g)}>
                <span class="w-9 h-9 rounded-lg overflow-hidden shrink-0 soft grid place-items-center font-bold">
                  {#if g.cover}<img src={g.cover} alt="" class="w-full h-full object-cover" onerror={e => ((e.target as HTMLImageElement).style.display = 'none')} />{:else}{g.name[0]}{/if}
                </span>
                <span class="min-w-0">
                  <span class="block text-sm font-medium truncate">{g.name}</span>
                  <span class="block text-[11px] muted">{t('lib.launcher.' + g.launcher)}{#if per?.close.length || per?.keep.length} · {t('gm.custom')}{/if}</span>
                </span>
              </button>
              <select class="select select-sm w-40" value={boostOf(g)} onchange={e => setBoost(g, (e.target as HTMLSelectElement).value)}>
                <option value="default">{t('gm.boost.default', { v: t('gm.boost.' + cfg.default_boost) })}</option>
                {#each ['ask', 'always', 'never'] as b (b)}<option value={b}>{t('gm.boost.' + b)}</option>{/each}
              </select>
              <button class="btn btn-ghost btn-xs btn-square" onclick={() => toggleOpen(g)} aria-label="details"><Icon name="chev" size=".9rem" style="transition:transform .25s;transform:rotate({open === g.id ? 90 : 0}deg)" /></button>
            </div>
            {#if open === g.id}
              <div class="ml-12 mt-3 grid grid-cols-1 lg:grid-cols-2 gap-4">
                <div>
                  <div class="text-[11px] uppercase tracking-wider muted font-semibold">{t('gm.gameApps')}</div>
                  <div class="flex flex-wrap gap-1.5 mt-2">
                    {#each per?.close ?? [] as id (id)}<span class="chip-soft" style="color:var(--color-error)">− {appName(id)}<button onclick={() => perList(g, 'close', id, false)}>×</button></span>{/each}
                    {#each per?.keep ?? [] as id (id)}<span class="chip-soft" style="color:var(--color-success)">+ {appName(id)}<button onclick={() => perList(g, 'keep', id, false)}>×</button></span>{/each}
                  </div>
                  <div class="flex flex-wrap gap-1 mt-2">
                    {#each running.slice(0, 10) as r (r.id)}
                      <span class="tag gap-1">{r.name}<button onclick={() => perList(g, 'close', r.id, true)} style="color:var(--color-error)">−</button><button onclick={() => perList(g, 'keep', r.id, true)} style="color:var(--color-success)">+</button></span>
                    {/each}
                  </div>
                </div>
                <div>
                  <div class="text-[11px] uppercase tracking-wider muted font-semibold">{t('gm.exeTweaks')}</div>
                  {#each exes[g.id] ?? [] as [exe, s] (exe)}
                    <div class="soft rounded-xl p-3 mt-2">
                      <div class="text-xs mono truncate" title={exe}>{exe.split('\\').pop()}</div>
                      <div class="flex flex-wrap gap-3 mt-2 items-center text-xs">
                        <label class="flex items-center gap-1.5">{t('gm.priority')}
                          <select class="select select-xs" value={s.priority} disabled={!app.elevated} onchange={e => setExe(g, exe, { ...s, priority: (e.target as HTMLSelectElement).value as ExeSettings['priority'] })}>
                            {#each ['normal', 'above', 'high'] as p (p)}<option value={p}>{t('gm.prio.' + p)}</option>{/each}
                          </select>
                        </label>
                        <label class="flex items-center gap-1.5 cursor-pointer"><input type="checkbox" class="toggle toggle-xs toggle-primary" checked={s.gpu_high} onchange={e => setExe(g, exe, { ...s, gpu_high: (e.target as HTMLInputElement).checked })} />{t('gm.gpuHigh')}</label>
                        <label class="flex items-center gap-1.5 cursor-pointer"><input type="checkbox" class="toggle toggle-xs toggle-primary" checked={s.no_fso} onchange={e => setExe(g, exe, { ...s, no_fso: (e.target as HTMLInputElement).checked })} />{t('gm.noFso')}</label>
                      </div>
                    </div>
                  {:else}
                    <div class="text-xs muted mt-2">{exes[g.id] ? t('gm.noExe') : '…'}</div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </section>

  {#if sessions.length}
    <section class="glass p-5 reveal" style="--i:5">
      <div class="font-semibold">{t('gm.sessions')}</div>
      <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
        {#each sessions.slice(0, 12) as s (s.start)}
          <div class="flex items-center gap-3 py-2.5">
            <span class="w-8 h-8 rounded-lg grid place-items-center soft shrink-0" style="color:var(--color-{s.boosted ? 'success' : 'base-content'})"><Icon name={s.boosted ? 'zap' : 'game'} size=".95rem" /></span>
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium truncate">{s.game}</div>
              <div class="text-[11px] muted">{ago(s.end, true)} · {t('gm.minutes', { n: minutes(s) })}{#if s.closed.length} · {t('gm.closedN', { n: s.closed.length })}{/if}</div>
            </div>
            {#if s.throttle.length}<span class="tag tag-warn" title={s.throttle.map(x => t('temps.ev.' + x.kind)).join(', ')}><Icon name="thermo" size=".65rem" />{t('gm.throttled', { n: s.throttle.length })}</span>{/if}
            {#if s.fps}<span class="mono text-sm font-semibold">{Math.round(s.fps.avg)} <span class="text-xs muted">FPS</span></span>{/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}
{/if}
