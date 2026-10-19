<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type NetStatus, type Latency } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { rpNote } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  const PRESETS: { key: string; name: string; servers: string; color: string }[] = [
    { key: 'auto', name: '', servers: '', color: 'var(--color-secondary)' },
    { key: 'cloudflare', name: 'Cloudflare', servers: '1.1.1.1 · 1.0.0.1', color: 'oklch(72% 0.17 55)' },
    { key: 'google', name: 'Google', servers: '8.8.8.8 · 8.8.4.4', color: 'oklch(66% 0.16 250)' },
    { key: 'quad9', name: 'Quad9', servers: '9.9.9.9 · 149.112.112.112', color: 'oklch(64% 0.2 330)' },
    { key: 'opendns', name: 'OpenDNS', servers: '208.67.222.222 · 208.67.220.220', color: 'oklch(70% 0.16 140)' },
  ];

  let st = $state<NetStatus | null>(null);
  let lat = $state<Latency[] | null>(null);
  let testing = $state(false);
  let busy = $state<string | null>(null);

  async function load() {
    try { st = await api.net.status(); } catch (e) { app.toast(String(e), 'error'); }
  }
  async function test() {
    testing = true;
    lat = null;
    // The router's (or ISP's) DNS is measured too, as the baseline
    const current = st?.adapters.find(a => !a.manual)?.servers.find(s => s.includes('.'));
    try { lat = await api.net.latency(current); } finally { testing = false; }
  }
  onMount(async () => { await load(); test(); });

  let current = $derived(st?.adapters[0]?.preset ?? 'auto');
  const ms = (key: string) => lat?.find(l => l.key === (key === 'auto' ? 'current' : key))?.ms ?? null;
  let fastest = $derived.by(() => {
    const ok = (lat ?? []).filter(l => l.ms !== null);
    return ok.length ? ok.reduce((a, b) => (a.ms! <= b.ms! ? a : b)).key : null;
  });
  let worst = $derived(Math.max(1, ...(lat ?? []).map(l => l.ms ?? 0)));

  async function use(key: string) {
    busy = key;
    try {
      const n = await api.net.setDns(key);
      app.toast(t('net.dnsSet', { name: key === 'auto' ? t('net.auto') : PRESETS.find(p => p.key === key)?.name ?? key, n }), 'success');
      await load();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function flush() {
    busy = 'flush';
    try { await api.net.flush(); app.toast(t('net.flushed'), 'success'); } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function tcp(key: string, on: boolean) {
    busy = key;
    try { await api.net.setTcp(key, on); app.toast(t('net.tcpSet'), 'success'); } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; await load(); }
  }
  async function restore() {
    const ok = await ask({ title: t('net.restoreTitle'), text: t('net.restoreText'), ico: 'undo', confirm: t('opt.restoreGo') });
    if (!ok) return;
    busy = 'restore';
    try { await api.net.restore(); app.toast(t('net.restored'), 'success'); await load(); } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function repair() {
    const ok = await ask({ title: t('net.repairTitle'), text: t('net.repairText'), ico: 'wrench', confirm: t('net.repair'), note: rpNote() });
    if (!ok) return;
    busy = 'repair';
    try { await api.net.repair(); app.toast(t('net.repaired'), 'success'); } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="globe" title={t('net.title')} sub={t('net.sub')}>
    {#if st?.has_backup}<button class="btn btn-ghost btn-sm gap-1.5" disabled={!!busy} onclick={restore}><Icon name="undo" size=".95rem" />{t('net.restore')}</button>{/if}
    <button class="btn btn-sm gap-1.5" disabled={testing} onclick={test}>
      {#if testing}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="activity" size=".95rem" />{/if}{t('net.test')}
    </button>
  </PageHead>
  {#if st}
    <div class="flex flex-wrap gap-2 mt-4">
      {#each st.adapters as a (a.guid)}
        <span class="chip-soft" title={a.description}><Icon name="network" size=".8rem" />{a.name} · {a.servers.filter(s => s.includes('.')).join(', ') || '—'}</span>
      {/each}
    </div>
  {/if}
</section>

<AdminBanner text={t('net.needAdmin')} />

<section class="grid grid-cols-1 md:grid-cols-2 2xl:grid-cols-5 gap-3">
  {#each PRESETS as p, i (p.key)}
    {@const active = current === p.key}
    {@const m = ms(p.key)}
    <div class="profile-card reveal {active ? 'active' : ''}" style="--i:{i + 1};--grad:linear-gradient(135deg,{p.color},var(--color-primary))">
      <div class="flex items-center gap-3">
        <span class="w-10 h-10 rounded-xl grid place-items-center text-white font-bold" style="background:{p.color}">{p.key === 'auto' ? '' : p.name[0]}{#if p.key === 'auto'}<Icon name="server" />{/if}</span>
        <div class="min-w-0 flex-1">
          <div class="font-semibold truncate">{p.key === 'auto' ? t('net.auto') : p.name}</div>
          <div class="text-[11px] muted mono truncate">{p.key === 'auto' ? t('net.autoSub') : p.servers}</div>
        </div>
      </div>
      <div class="mt-4">
        <div class="flex justify-between items-baseline text-xs">
          <span class="muted">{t('net.latency')}</span>
          {#if testing && !lat}<span class="loading loading-dots loading-xs"></span>
          {:else}<span class="mono font-semibold">{m === null ? (lat ? t('net.noAnswer') : '—') : `${m.toFixed(0)} ms`}</span>{/if}
        </div>
        <div class="lat mt-1.5"><i style="width:{m === null ? 0 : Math.max(6, (m / worst) * 100)}%;background:{fastest === (p.key === 'auto' ? 'current' : p.key) ? 'var(--color-success)' : p.color}"></i></div>
        <div class="h-5 mt-1">{#if fastest && fastest === (p.key === 'auto' ? 'current' : p.key)}<span class="tag" style="color:var(--color-success)"><Icon name="zap" size=".65rem" />{t('net.fastest')}</span>{/if}</div>
      </div>
      <button class="btn btn-sm w-full mt-2 {active ? 'btn-ghost' : 'btn-primary'}" disabled={active || !!busy || !st?.elevated} onclick={() => use(p.key)}>
        {#if busy === p.key}<span class="loading loading-spinner loading-xs"></span>{/if}{active ? t('opt.active') : t('net.use')}
      </button>
    </div>
  {/each}
</section>

<section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
  <div class="glass p-5 reveal" style="--i:6">
    <div class="font-semibold">{t('net.tcp')}</div>
    <div class="text-xs muted">{t('net.tcpSub')}</div>
    <div class="divide-y divide-[var(--glass-border)] mt-2">
      {#each st?.tcp ?? [] as s (s.key)}
        <div class="flex items-center gap-3 py-3">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name={s.key === 'nagle' ? 'game' : s.key === 'rss' ? 'cpu' : 'network'} size="1.05rem" /></span>
          <div class="flex-1 min-w-0">
            <div class="font-medium text-sm flex items-center gap-1.5">{t('net.tcp.' + s.key)}<span class="tag tag-warn"><Icon name="shield" size=".65rem" />{t('opt.admin')}</span></div>
            <div class="text-xs muted">{t('net.tcp.' + s.key + '.hint')}{#if s.value} · <span class="mono">{s.value}</span>{/if}</div>
          </div>
          {#if busy === s.key}<span class="loading loading-spinner loading-xs"></span>{/if}
          {#if s.key === 'nagle'}
            <input type="checkbox" class="toggle toggle-primary toggle-sm" checked={s.on} disabled={!st?.elevated || !!busy} onchange={e => tcp(s.key, (e.target as HTMLInputElement).checked)} />
          {:else if s.on}
            <span class="flex items-center gap-1.5 text-xs font-semibold" style="color:var(--color-success)"><i class="dot-ok"></i>{t('care.state.ok')}</span>
          {:else}
            <button class="btn btn-xs btn-primary" disabled={!st?.elevated || !!busy} onclick={() => tcp(s.key, true)}>{t('net.fixIt')}</button>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <div class="glass p-5 reveal" style="--i:7">
    <div class="font-semibold">{t('net.tools')}</div>
    <div class="flex flex-col gap-3 mt-3">
      <div class="flex items-center gap-3">
        <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft"><Icon name="refresh" size="1rem" /></span>
        <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('net.flush')}</div><div class="text-xs muted">{t('net.flushHint')}</div></div>
        <button class="btn btn-sm" disabled={!!busy} onclick={flush}>{#if busy === 'flush'}<span class="loading loading-spinner loading-xs"></span>{/if}{t('net.run')}</button>
      </div>
      <div class="flex items-center gap-3">
        <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft"><Icon name="wrench" size="1rem" /></span>
        <div class="flex-1 min-w-0"><div class="text-sm font-medium flex items-center gap-1.5">{t('net.repair')}<span class="tag">{t('opt.restart')}</span></div><div class="text-xs muted">{t('net.repairHint')}</div></div>
        <button class="btn btn-sm" disabled={!!busy || !st?.elevated} onclick={repair}>{#if busy === 'repair'}<span class="loading loading-spinner loading-xs"></span>{/if}{t('net.run')}</button>
      </div>
    </div>
  </div>
</section>
