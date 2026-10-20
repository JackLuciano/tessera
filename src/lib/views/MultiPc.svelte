<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, ago, type LanInfo, type PeerState, type PcStatus } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';

  // Other PCs at home: share this one with a pairing code, add others by their code. LAN only.
  let info = $state<LanInfo | null>(null);
  let peers = $state<PeerState[] | null>(null);
  let found = $state<[string, string][] | null>(null);
  let searching = $state(false);
  let addr = $state('');
  let code = $state('');
  let adding = $state(false);

  async function loadInfo() { info = await api.lan.info().catch(() => null); }
  async function loadPeers() { peers = await api.lan.peers().catch(() => []); }
  onMount(() => {
    loadInfo();
    loadPeers();
    const timer = setInterval(loadPeers, 15000);
    return () => clearInterval(timer);
  });

  async function share(on: boolean, fresh = false) {
    try { await api.lan.set(on, fresh); await loadInfo(); } catch (e) { app.toast(String(e), 'error'); }
  }
  async function search() {
    searching = true;
    found = await api.lan.discover().catch(() => []);
    searching = false;
  }
  async function add() {
    adding = true;
    try {
      const st = await api.lan.add(addr.trim(), code.replace(/\s/g, ''));
      app.toast(t('pcs.added', { name: st.name }), 'success');
      addr = ''; code = ''; found = null;
      await loadPeers();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { adding = false; }
  }
  async function remove(p: PeerState) { await api.lan.remove(p.peer.addr).catch(() => {}); await loadPeers(); }

  const ram = (s: PcStatus) => (s.ram_total ? (s.ram_used / s.ram_total) * 100 : 0);
  const upFor = (s: number) => (s >= 86400 ? `${Math.floor(s / 86400)} d` : `${Math.floor(s / 3600)} h`);
  const toneOf = (temp: number | null) => (temp == null ? 'var(--muted)' : temp >= 85 ? 'var(--color-error)' : temp >= 72 ? 'var(--color-warning)' : 'var(--color-success)');
</script>

<section class="glass p-5 reveal">
  <PageHead ico="pcs" title={t('pcs.title')} sub={t('pcs.sub')} />
</section>

<section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
  <div class="glass p-5 reveal" style="--i:1">
    <div class="flex items-center justify-between gap-3">
      <div><div class="font-semibold">{t('pcs.shareTitle')}</div><div class="text-xs muted">{t('pcs.shareSub')}</div></div>
      {#if info}<input type="checkbox" class="toggle toggle-primary" checked={info.enabled} onchange={e => share((e.target as HTMLInputElement).checked)} />{/if}
    </div>
    {#if info?.enabled}
      <div class="soft rounded-xl p-4 mt-4 flex items-center gap-5 flex-wrap">
        <div>
          <div class="text-[11px] muted">{t('pcs.code')}</div>
          <div class="mono text-3xl font-bold tracking-[.2em] grad-text">{info.code.slice(0, 3)} {info.code.slice(3)}</div>
        </div>
        <div class="text-xs">
          <div><span class="muted">{t('pcs.thisPc')}</span> {info.name}</div>
          {#each info.addresses as a (a)}<div class="mono">{a}:{info.port}</div>{/each}
        </div>
        <button class="btn btn-xs btn-ghost border hairline ml-auto gap-1" onclick={() => share(true, true)}><Icon name="refresh" size=".75rem" />{t('pcs.newCode')}</button>
      </div>
      <div class="text-[11px] muted mt-2">{t('pcs.firewall')}</div>
    {/if}
  </div>

  <div class="glass p-5 reveal" style="--i:2">
    <div class="flex items-center justify-between gap-3">
      <div><div class="font-semibold">{t('pcs.addTitle')}</div><div class="text-xs muted">{t('pcs.addSub')}</div></div>
      <button class="btn btn-sm btn-ghost border hairline gap-1.5" disabled={searching} onclick={search}>
        {#if searching}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="search" size=".85rem" />{/if}{t('pcs.find')}
      </button>
    </div>
    {#if found}
      <div class="flex flex-wrap gap-1.5 mt-3">
        {#each found as [name, a] (a)}<button class="badge badge-ghost gap-1 hover:badge-primary" onclick={() => (addr = a)}><Icon name="pcs" size=".7rem" />{name} · <span class="mono">{a}</span></button>
        {:else}<span class="text-xs muted">{t('pcs.noneFound')}</span>{/each}
      </div>
    {/if}
    <div class="flex gap-2 mt-3 flex-wrap">
      <input class="input input-sm flex-1 min-w-40 mono" placeholder="192.168.0.12" bind:value={addr} />
      <input class="input input-sm w-32 mono tracking-widest" placeholder="000 000" maxlength="7" bind:value={code} onkeydown={e => e.key === 'Enter' && add()} />
      <button class="btn btn-sm btn-primary gap-1.5" disabled={adding || !addr.trim() || code.replace(/\s/g, '').length !== 6} onclick={add}>
        {#if adding}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="plus" size=".85rem" />{/if}{t('pcs.pair')}
      </button>
    </div>
  </div>
</section>

{#if peers === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !peers.length}
  <div class="glass p-10 text-center reveal" style="--i:3"><Icon name="pcs" size="1.8rem" style="opacity:.4" /><div class="text-sm mt-2">{t('pcs.none')}</div><div class="text-xs muted mt-1">{t('pcs.noneHint')}</div></div>
{:else}
  <section class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each peers as p, i (p.peer.addr)}
      {@const s = p.status}
      <div class="glass p-5 reveal" style="--i:{i + 3}">
        <div class="flex items-start justify-between gap-3">
          <div class="flex items-center gap-3 min-w-0">
            <span class="status-ico shrink-0" style="background:color-mix(in oklch,var(--color-{s ? 'success' : 'error'}) 14%,transparent);color:var(--color-{s ? 'success' : 'error'})"><Icon name="pcs" size="1.05rem" /></span>
            <div class="min-w-0">
              <div class="font-semibold truncate">{s?.name ?? p.peer.name}</div>
              <div class="text-[11px] muted truncate">{s ? `${s.os} · ${t('pcs.up', { d: upFor(s.uptime) })} · Tessera ${s.version}` : p.error}</div>
            </div>
          </div>
          <button class="btn btn-ghost btn-xs btn-square" title={t('rules.delete')} onclick={() => remove(p)}><Icon name="x" size=".8rem" /></button>
        </div>
        {#if s}
          <div class="flex items-center gap-5 mt-4 flex-wrap">
            <Ring value={s.care_score} label={t('home.score')} size={88} warn={80} bad={60} invert />
            <div class="grid grid-cols-2 gap-x-6 gap-y-2 flex-1 min-w-48">
              <div class="metric"><b class="!text-lg">{Math.round(s.cpu_load)}%{#if s.cpu_temp != null}<small class="text-xs" style="color:{toneOf(s.cpu_temp)}"> {Math.round(s.cpu_temp)}°</small>{/if}</b><span>CPU</span></div>
              <div class="metric"><b class="!text-lg">{s.gpu_load != null ? Math.round(s.gpu_load) + '%' : '—'}{#if s.gpu_temp != null}<small class="text-xs" style="color:{toneOf(s.gpu_temp)}"> {Math.round(s.gpu_temp)}°</small>{/if}</b><span class="truncate">{s.gpu_name || 'GPU'}</span></div>
              <div class="metric"><b class="!text-lg">{Math.round(ram(s))}%</b><span>{t('slow.ram')} · {fmt(s.ram_total)}</span></div>
              <div class="metric"><b class="!text-lg">{s.care_at ? ago(s.care_at, true) : '—'}</b><span>{t('pcs.lastCare')}</span></div>
            </div>
          </div>
          <div class="flex flex-col gap-2 mt-4">
            {#each s.disks.slice(0, 4) as [mount, total, free] (mount)}
              {@const used = ((total - free) / Math.max(1, total)) * 100}
              <div>
                <div class="flex justify-between text-xs"><span class="mono">{mount}</span><span class="mono">{t('backup.free', { size: fmt(free) })}</span></div>
                <div class="bar mt-1" style="height:5px"><i style="width:{used}%;{used > 90 ? 'background:var(--color-error)' : ''}"></i></div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </section>
{/if}
