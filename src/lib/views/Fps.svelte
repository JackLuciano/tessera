<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../store.svelte';
  import { api, ago, type FpsSample, type FpsSummary, type PingState, type GameStatus } from '../api';
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
        <div class="flex items-center gap-3 py-2.5">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={pick.includes(s.start)} onchange={() => togglePick(s)} title={t('fps.compare')} />
          <div class="w-40 min-w-0">
            <div class="text-sm font-medium truncate">{s.game}</div>
            <div class="text-[11px] muted">{ago(s.start, true)} · {Math.round(s.seconds / 60)} min</div>
          </div>
          <div class="flex-1 min-w-0 max-md:hidden"><Spark values={s.timeline} height={28} /></div>
          <div class="text-right mono text-sm w-36">
            <span class="font-bold">{Math.round(s.avg)}</span> <span class="text-xs muted">avg</span> · <span>{Math.round(s.low1)}</span> <span class="text-xs muted">1%</span>
          </div>
          <button class="btn btn-ghost btn-xs btn-square" onclick={() => remove(s)} aria-label="delete"><Icon name="x" size=".85rem" /></button>
        </div>
      {/each}
    </div>
  {/if}
</section>
