<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../store.svelte';
  import { api, ago, OVERLAY_DEFAULT, type FpsSample, type FpsSummary, type PingState, type GameStatus, type OverlayCfg } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';
  import Spark from '../Spark.svelte';

  let st = $state<{ tool_ready: boolean; recording: string | null; elevated: boolean; overlay: boolean } | null>(null);
  let live = $state<FpsSample | null>(null);
  let liveTimeline = $state<number[]>([]);
  let error = $state<string | null>(null);
  let sessions = $state<FpsSummary[]>([]);
  let game = $state<GameStatus | null>(null);
  let exe = $state('');
  let pick = $state<number[]>([]);
  let ping = $state<PingState | null>(null);
  let host = $state('1.1.1.1');
  let gateway = $state<string | null>(null);

  async function refresh() {
    st = await api.fps.status().catch(() => null);
    sessions = await api.fps.sessions().catch(() => []);
    game = await api.gamemode.status().catch(() => null);
  }

  onMount(() => {
    refresh();
    api.gateway().then(g => (gateway = g)).catch(() => {});
    const offs: (() => void)[] = [];
    listen<FpsSample>('fps-sample', e => { live = e.payload; error = null; liveTimeline = [...liveTimeline, e.payload.fps].slice(-120); }).then(u => offs.push(u));
    listen<string>('fps-error', e => (error = e.payload)).then(u => offs.push(u));
    listen('fps-saved', () => refresh()).then(u => offs.push(u));
    const tick = () => api.ping(host).then(p => (ping = p)).catch(() => {});
    tick();
    const timer = setInterval(tick, 1000);
    return () => { clearInterval(timer); offs.forEach(o => o()); };
  });

  function setHost(h: string) { host = h; ping = null; api.ping(h).then(p => (ping = p)).catch(() => {}); }

  async function start() {
    const name = exe.trim();
    if (!name) return;
    error = null;
    liveTimeline = [];
    await api.fps.start(name.endsWith('.exe') ? name : `${name}.exe`, name.replace(/\.exe$/i, ''));
    setTimeout(refresh, 800);
  }
  async function stop() {
    const s = await api.fps.stop().catch(() => null);
    if (s) app.toast(t('fps.saved', { avg: Math.round(s.avg), low: Math.round(s.low1) }), 'success');
    live = null;
    refresh();
  }
  async function overlay(on: boolean) { await api.fps.overlay(on); setTimeout(refresh, 300); }
  async function remove(s: FpsSummary) { await api.fps.remove(s.start); sessions = sessions.filter(x => x.start !== s.start); pick = pick.filter(p => p !== s.start); }
  function togglePick(s: FpsSummary) { pick = pick.includes(s.start) ? pick.filter(p => p !== s.start) : [...pick.slice(-1), s.start]; }

  // ---- FPS window look ----
  const FIELDS = ['fps', 'low1', 'frametime', 'graph', 'cpu', 'gpu', 'temps', 'ping'];
  const CORNERS = ['tl', 'tr', 'bl', 'br'] as const;
  let ov = $derived(app.prefs.overlay ?? OVERLAY_DEFAULT);
  function setOv(p: Partial<OverlayCfg>) { app.setPrefs({ overlay: { ...ov, ...p } }); }
  function toggleField(f: string) { setOv({ fields: ov.fields.includes(f) ? ov.fields.filter(x => x !== f) : FIELDS.filter(x => x === f || ov.fields.includes(x)) }); }
  // Moving to another corner needs the window to be placed again
  async function setCorner(c: OverlayCfg['corner']) { setOv({ corner: c }); if (st?.overlay) { await api.fps.overlay(false); setTimeout(() => api.fps.overlay(true), 250); } }

  // ---- session details: latency check and stutters ----
  let open = $state<number | null>(null);
  const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  function tips(s: FpsSummary): { key: string; vars?: Record<string, string | number>; tone: string }[] {
    const l = s.latency;
    if (!l) return [];
    const out: { key: string; vars?: Record<string, string | number>; tone: string }[] = [];
    const frame = s.avg ? 1000 / s.avg : 0;
    if (l.vsync > 0.5) out.push({ key: 'lat.tipVsync', vars: { cap: Math.max(30, (s.refresh || 60) - 3) }, tone: 'warning' });
    if (/composed/i.test(l.present_mode)) out.push({ key: 'lat.tipComposed', tone: 'warning' });
    if ((l.gpu_busy ?? 0) > 0.95) out.push({ key: 'lat.tipGpu', vars: { cap: Math.max(30, Math.round(s.low1 * 0.95)) }, tone: 'warning' });
    if (l.dropped > 0.05) out.push({ key: 'lat.tipDropped', vars: { pct: Math.round(l.dropped * 100) }, tone: 'error' });
    if (s.refresh && s.avg > s.refresh * 1.5 && l.vsync <= 0.5) out.push({ key: 'lat.tipCap', vars: { hz: s.refresh }, tone: 'info' });
    if (!out.length && frame) out.push({ key: 'lat.tipGood', tone: 'success' });
    return out;
  }
  function causes(s: FpsSummary) {
    const m = new Map<string, number>();
    for (const x of s.stutters ?? []) m.set(x.cause, (m.get(x.cause) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }
  const CAUSE_ICON: Record<string, string> = { thermal: 'thermo', disk: 'drive', background: 'layers', cpu: 'cpu', gpu: 'monitor', game: 'game' };

  let compare = $derived(pick.length === 2 ? pick.map(p => sessions.find(s => s.start === p)).filter(Boolean) as FpsSummary[] : null);
  const delta = (a: number, b: number) => (a ? ((b - a) / a) * 100 : 0);
  const PRESETS = $derived([...(gateway ? [{ key: 'router', host: gateway }] : []), { key: 'cloudflare', host: '1.1.1.1' }, { key: 'google', host: '8.8.8.8' }]);
  let tone = $derived(!live ? 'var(--color-primary)' : live.fps >= 100 ? 'var(--color-success)' : live.fps >= 50 ? 'var(--color-warning)' : 'var(--color-error)');
</script>

<section class="glass p-5 reveal">
  <PageHead ico="activity" title={t('fps.title')} sub={t('fps.sub')}>
    {#if st}
      <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="toggle toggle-primary" checked={st.overlay} onchange={e => overlay((e.target as HTMLInputElement).checked)} />{t('fps.window')}</label>
    {/if}
  </PageHead>
</section>

<AdminBanner text={t('fps.needAdmin')} />

<section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-4 items-start">
  <div class="glass p-5 reveal" style="--i:1">
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div class="font-semibold">{t('fps.live')}</div>
      {#if st?.recording}
        <button class="btn btn-sm btn-error gap-1.5" onclick={stop}><span class="w-2 h-2 rounded-full bg-current animate-pulse"></span>{t('fps.stop')}</button>
      {:else}
        <div class="flex gap-2">
          <input class="input input-sm w-48" placeholder={game?.game ? game.game.name : t('fps.exePlaceholder')} bind:value={exe} onkeydown={e => e.key === 'Enter' && start()} />
          <button class="btn btn-sm btn-primary gap-1.5" disabled={!exe.trim() || !st?.elevated} onclick={start}><Icon name="record" size=".9rem" />{t('fps.record')}</button>
        </div>
      {/if}
    </div>
    {#if error}<div class="text-sm mt-3" style="color:var(--color-warning)">{error === 'needs_admin' ? t('fps.needAdmin') : error}</div>{/if}
    <div class="grid grid-cols-3 gap-4 mt-4">
      <div class="metric"><b style="color:{tone}">{live ? Math.round(live.fps) : '—'}</b><span>{t('fps.fps')}</span></div>
      <div class="metric"><b>{live ? Math.round(live.low1) : '—'}</b><span>{t('fps.low1')}</span></div>
      <div class="metric"><b>{live ? live.frametime.toFixed(1) : '—'}</b><span>{t('fps.frametime')}</span></div>
    </div>
    {#if live}
      <div class="mt-4">
        <div class="text-[11px] muted mb-1">{t('fps.timeline')}</div>
        <Spark values={liveTimeline} height={90} color={tone} />
      </div>
      <div class="mt-3">
        <div class="text-[11px] muted mb-1">{t('fps.frametimes')}</div>
        <Spark values={live.recent} height={50} color="var(--color-accent)" fill={false} />
      </div>
    {:else}
      <div class="soft rounded-xl p-6 mt-4 text-center">
        <Icon name="activity" size="1.6rem" style="opacity:.45" />
        <div class="text-sm mt-2">{t('fps.idle')}</div>
        <div class="text-[11px] muted mt-1">{t('fps.idleHint')}</div>
      </div>
    {/if}
    {#if !st?.tool_ready}<div class="text-[11px] muted mt-3 flex gap-1.5 items-start"><Icon name="info" size=".85rem" />{t('fps.download')}</div>{/if}
  </div>

  <div class="glass p-5 reveal" style="--i:2">
    <div class="font-semibold flex items-center gap-2"><Icon name="ping" size="1rem" style="color:var(--color-primary)" />{t('ping.title')}</div>
    <div class="flex flex-wrap gap-1.5 mt-3">
      {#each PRESETS as p (p.key)}<button class="btn btn-xs {host === p.host ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => setHost(p.host)}>{t('ping.' + p.key)}</button>{/each}
      <input class="input input-xs w-36" placeholder={t('ping.custom')} onkeydown={e => e.key === 'Enter' && setHost((e.target as HTMLInputElement).value)} />
    </div>
    <div class="grid grid-cols-3 gap-3 mt-4">
      <div class="metric"><b>{ping?.avg != null ? Math.round(ping.avg) : '—'}<small class="text-xs muted"> ms</small></b><span>{t('ping.avg')}</span></div>
      <div class="metric"><b>{ping?.jitter != null ? ping.jitter.toFixed(1) : '—'}<small class="text-xs muted"> ms</small></b><span>{t('ping.jitter')}</span></div>
      <div class="metric"><b style={ping && ping.loss > 1 ? 'color:var(--color-error)' : ''}>{ping ? ping.loss.toFixed(0) : '—'}<small class="text-xs muted"> %</small></b><span>{t('ping.loss')}</span></div>
    </div>
    <div class="mt-4"><Spark values={ping?.samples ?? []} height={70} color="var(--color-secondary)" dots /></div>
    <div class="text-[11px] muted mt-2">{t('ping.hint', { host: ping?.host ?? host })}</div>
  </div>
</section>

<section class="glass p-5 reveal" style="--i:3">
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <div><div class="font-semibold">{t('ov.title')}</div><div class="text-xs muted">{t('ov.sub', { key: app.prefs.hotkeys?.fps || '—' })}</div></div>
    <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="toggle toggle-primary toggle-sm" checked={ov.compact} onchange={e => setOv({ compact: (e.target as HTMLInputElement).checked })} />{t('ov.compact')}</label>
  </div>
  <div class="grid grid-cols-1 lg:grid-cols-[auto_minmax(0,1fr)_minmax(0,1fr)] gap-6 mt-4 items-start">
    <div>
      <div class="text-[11px] muted mb-1.5">{t('ov.corner')}</div>
      <div class="ov-screen">
        {#each CORNERS as c (c)}
          <button class="ov-corner {c}" class:on={ov.corner === c} onclick={() => setCorner(c)} aria-label={t('ov.' + c)} title={t('ov.' + c)}></button>
        {/each}
      </div>
    </div>
    <div class="flex flex-col gap-4">
      <label class="block">
        <div class="flex justify-between text-[11px] muted mb-1"><span>{t('ov.size')}</span><span class="mono">{Math.round(ov.scale * 100)}%</span></div>
        <input type="range" class="range range-primary range-xs" min="0.7" max="1.6" step="0.05" value={ov.scale} onchange={e => setOv({ scale: +(e.target as HTMLInputElement).value })} />
      </label>
      <label class="block">
        <div class="flex justify-between text-[11px] muted mb-1"><span>{t('ov.opacity')}</span><span class="mono">{Math.round(ov.opacity * 100)}%</span></div>
        <input type="range" class="range range-primary range-xs" min="0.2" max="1" step="0.05" value={ov.opacity} onchange={e => setOv({ opacity: +(e.target as HTMLInputElement).value })} />
      </label>
    </div>
    <div>
      <div class="text-[11px] muted mb-1.5">{t('ov.show')}</div>
      <div class="flex flex-wrap gap-1.5">
        {#each FIELDS as f (f)}
          <button class="btn btn-xs {ov.fields.includes(f) ? 'btn-primary' : 'btn-ghost border hairline'}" onclick={() => toggleField(f)}>{t('ov.f.' + f)}</button>
        {/each}
      </div>
    </div>
  </div>
</section>

<section class="glass p-5 reveal" style="--i:4">
  <div class="flex items-center justify-between gap-2 flex-wrap">
    <div><div class="font-semibold">{t('fps.sessions')}</div><div class="text-xs muted">{t('fps.sessionsSub')}</div></div>
  </div>
  {#if compare}
    <div class="soft rounded-xl p-4 mt-3 grid grid-cols-1 md:grid-cols-3 gap-4">
      {#each [['avg', t('fps.avg')], ['low1', t('fps.low1')], ['low01', t('fps.low01')]] as [k, label] (k)}
        {@const a = compare[0][k as 'avg']}
        {@const b = compare[1][k as 'avg']}
        {@const d = delta(a, b)}
        <div>
          <div class="text-[11px] muted">{label}</div>
          <div class="flex items-baseline gap-2 mono"><span class="text-lg">{Math.round(a)}</span><Icon name="chev" size=".8rem" /><span class="text-lg font-bold">{Math.round(b)}</span>
            <span class="text-xs font-semibold" style="color:var(--color-{d >= 0 ? 'success' : 'error'})">{d >= 0 ? '+' : ''}{d.toFixed(1)}%</span></div>
        </div>
      {/each}
    </div>
  {/if}
  {#if !sessions.length}
    <div class="text-sm muted text-center py-8">{t('fps.noSessions')}</div>
  {:else}
    <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
      {#each sessions as s (s.start)}
        <div>
        <div class="flex items-center gap-3 py-2.5">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={pick.includes(s.start)} onchange={() => togglePick(s)} title={t('fps.compare')} />
          <button class="w-40 min-w-0 text-left" onclick={() => (open = open === s.start ? null : s.start)}>
            <div class="text-sm font-medium truncate">{s.game}</div>
            <div class="text-[11px] muted">{ago(s.start, true)} · {Math.round(s.seconds / 60)} min</div>
          </button>
          <div class="flex-1 min-w-0 max-md:hidden"><Spark values={s.timeline} height={28} /></div>
          {#if s.stutters?.length}<span class="badge badge-sm badge-warning badge-soft max-md:hidden">{t('stut.count', { n: s.stutters.length })}</span>{/if}
          <div class="text-right mono text-sm w-36">
            <span class="font-bold">{Math.round(s.avg)}</span> <span class="text-xs muted">avg</span> · <span>{Math.round(s.low1)}</span> <span class="text-xs muted">1%</span>
          </div>
          <button class="btn btn-ghost btn-xs btn-square" onclick={() => (open = open === s.start ? null : s.start)} aria-label={t('stut.details')}><Icon name="chev" size=".85rem" style="transform:rotate({open === s.start ? 90 : 0}deg);transition:transform .2s" /></button>
          <button class="btn btn-ghost btn-xs btn-square" onclick={() => remove(s)} aria-label="delete"><Icon name="x" size=".85rem" /></button>
        </div>
        {#if open === s.start}
          <div class="grid grid-cols-1 lg:grid-cols-2 gap-4 pb-4">
            <div class="soft rounded-xl p-4">
              <div class="font-semibold text-sm flex items-center gap-2"><Icon name="mouse" size=".95rem" style="color:var(--color-primary)" />{t('lat.title')}</div>
              {#if s.latency}
                {@const l = s.latency}
                <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 mt-3">
                  <div class="metric"><b>{l.display_ms ? l.display_ms.toFixed(1) : '—'}<small class="text-xs muted"> ms</small></b><span>{t('lat.display')}</span></div>
                  <div class="metric"><b>{l.vsync > 0.5 ? t('sec.on') : t('sec.off')}</b><span>V-Sync</span></div>
                  <div class="metric"><b>{l.gpu_busy != null ? Math.round(l.gpu_busy * 100) + '%' : '—'}</b><span>{t('lat.gpuBusy')}</span></div>
                  <div class="metric"><b>{Math.round(l.dropped * 100)}%</b><span>{t('lat.dropped')}</span></div>
                </div>
                {#if l.present_mode}<div class="text-[11px] muted mt-2">{t('lat.mode')}: <span class="mono">{l.present_mode}</span></div>{/if}
                <div class="flex flex-col gap-1.5 mt-3">
                  {#each tips(s) as tip (tip.key)}
                    <div class="text-xs flex gap-2 items-start"><span class="w-1.5 h-1.5 rounded-full mt-1.5 shrink-0" style="background:var(--color-{tip.tone})"></span>{t(tip.key, tip.vars)}</div>
                  {/each}
                </div>
              {:else}
                <div class="text-xs muted mt-2">{t('lat.none')}</div>
              {/if}
            </div>
            <div class="soft rounded-xl p-4">
              <div class="font-semibold text-sm flex items-center gap-2"><Icon name="activity" size=".95rem" style="color:var(--color-warning)" />{t('stut.title')}</div>
              {#if !s.stutters?.length}
                <div class="text-xs muted mt-2">{s.latency ? t('stut.none') : t('lat.none')}</div>
              {:else}
                <div class="flex flex-wrap gap-1.5 mt-3">
                  {#each causes(s) as [c, k] (c)}<span class="badge badge-sm badge-ghost gap-1"><Icon name={CAUSE_ICON[c] ?? 'info'} size=".75rem" />{t('stut.c.' + c)} · {k}</span>{/each}
                </div>
                <div class="text-xs muted mt-2">{t('stut.h.' + causes(s)[0][0])}</div>
                <div class="flex flex-col mt-3 max-h-52 overflow-auto divide-y divide-[var(--glass-border)]">
                  {#each s.stutters as x (x.sec)}
                    <div class="flex items-center gap-3 py-1.5 text-xs">
                      <span class="mono muted w-12">{mmss(x.sec)}</span>
                      <span class="mono font-semibold w-16" style="color:var(--color-{x.ms > 100 ? 'error' : 'warning'})">{Math.round(x.ms)} ms</span>
                      <span class="flex items-center gap-1.5 min-w-0 truncate"><Icon name={CAUSE_ICON[x.cause] ?? 'info'} size=".8rem" />{t('stut.c.' + x.cause)}{x.detail ? ` · ${x.detail}` : ''}</span>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>
