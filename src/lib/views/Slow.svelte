<script lang="ts">
  import { onMount } from 'svelte';
  import { app, type View } from '../store.svelte';
  import { api, fmt, type SlowReport, type SlowProc, type SlowFinding } from '../api';
  import { t } from '../i18n.svelte';
  import { IS_WINDOWS } from '../nav';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';

  // "Why is my PC slow right now": one snapshot of CPU, memory, disk and network users, explained
  let r = $state<SlowReport | null>(null);
  let busy = $state(false);
  let at = $state<number | null>(null);

  async function run() {
    busy = true;
    try { r = await api.slowCheck(); at = Date.now(); } catch (e) { app.toast(String(e), 'error'); } finally { busy = false; }
  }
  onMount(run);

  async function end(p: SlowProc) {
    const out = await api.opt.kill([p.pid]).catch(e => (app.toast(String(e), 'error'), null));
    if (out?.applied) { app.toast(t('slow.ended', { name: p.name }), 'success'); setTimeout(run, 600); }
  }

  const ICON: Record<string, string> = { cpu: 'cpu', memory: 'memory', disk: 'drive', thermal: 'thermo', known: 'info', startup: 'power', fine: 'check' };
  const TONE = ['success', 'warning', 'error'];
  function text(f: SlowFinding) {
    if (f.kind === 'known') return { title: t('slow.known.' + f.subject), body: t('slow.knownHint.' + f.subject) };
    return { title: t('slow.f.' + f.kind, { name: f.subject, n: Math.round(f.value) }), body: t('slow.h.' + f.kind) };
  }
  let ram = $derived(r && r.ram_total ? (r.ram_used / r.ram_total) * 100 : null);
  const rate = (b: number) => `${fmt(b)}/s`;
  const cols = $derived(r ? [
    { key: 'cpu', ico: 'cpu', list: r.top_cpu, val: (p: SlowProc) => `${p.cpu.toFixed(1)}%`, w: (p: SlowProc) => p.cpu / Math.max(1, r!.top_cpu[0]?.cpu ?? 1) },
    { key: 'memory', ico: 'memory', list: r.top_mem, val: (p: SlowProc) => fmt(p.memory), w: (p: SlowProc) => p.memory / Math.max(1, r!.top_mem[0]?.memory ?? 1) },
    { key: 'disk', ico: 'drive', list: r.top_disk, val: (p: SlowProc) => rate(p.disk), w: (p: SlowProc) => p.disk / Math.max(1, r!.top_disk[0]?.disk ?? 1) },
  ] : []);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="gauge" title={t('slow.title')} sub={t('slow.sub')}>
    <button class="btn btn-sm btn-primary gap-1.5" disabled={busy} onclick={run}>
      {#if busy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="refresh" size=".9rem" />{/if}{t('slow.again')}
    </button>
  </PageHead>
  {#if r}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-5 justify-items-center">
      <Ring value={r.cpu} label="CPU" unit="%" size={112} />
      <Ring value={ram} label={t('slow.ram')} unit="%" size={112} sub={`${fmt(r.ram_used)} / ${fmt(r.ram_total)}`} />
      <Ring value={r.disk_busy} label={t('slow.disk')} unit="%" size={112} />
      <div class="flex flex-col items-center justify-center gap-1" style="width:112px;height:112px">
        <div class="mono text-sm flex items-center gap-1.5 whitespace-nowrap"><Icon name="download" size=".8rem" style="color:var(--color-primary)" />{rate(r.net_down)}</div>
        <div class="mono text-sm flex items-center gap-1.5 whitespace-nowrap"><Icon name="upgrade" size=".8rem" style="color:var(--color-accent)" />{rate(r.net_up)}</div>
        <div class="text-[11px] muted mt-1">{t('slow.net')}</div>
      </div>
    </div>
  {/if}
</section>

{#if !r}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span><div class="text-xs muted mt-2">{t('slow.measuring')}</div></div>
{:else}
  <section class="flex flex-col gap-3">
    {#each r.findings as f, i (f.kind + f.subject)}
      {@const x = text(f)}
      <div class="glass p-4 reveal flex items-start gap-4" style="--i:{i + 1}">
        <span class="status-ico shrink-0" style="background:color-mix(in oklch,var(--color-{TONE[f.level]}) 14%,transparent);color:var(--color-{TONE[f.level]})"><Icon name={ICON[f.kind] ?? 'info'} size="1.1rem" /></span>
        <div class="flex-1 min-w-0">
          <div class="font-semibold text-sm">{x.title}</div>
          <div class="text-xs muted mt-0.5">{x.body}</div>
        </div>
        {#if f.page && (IS_WINDOWS || f.page === 'temps')}
          <button class="btn btn-sm btn-ghost border hairline gap-1 shrink-0" onclick={() => (app.view = f.page as View)}>{t('slow.go.' + f.page)}<Icon name="chev" size=".8rem" /></button>
        {/if}
      </div>
    {/each}
  </section>

  <section class="grid grid-cols-1 xl:grid-cols-3 gap-4 items-start">
    {#each cols as c, ci (c.key)}
      <div class="glass p-5 reveal" style="--i:{ci + 4}">
        <div class="font-semibold flex items-center gap-2"><Icon name={c.ico} size="1rem" style="color:var(--color-primary)" />{t('slow.top.' + c.key)}</div>
        {#if !c.list.length}
          <div class="text-sm muted mt-3">{t('slow.quiet')}</div>
        {:else}
          <div class="flex flex-col gap-2.5 mt-3">
            {#each c.list as p (p.name)}
              <div class="group">
                <div class="flex items-center justify-between gap-2 text-sm">
                  <span class="truncate min-w-0">{p.name}{#if p.known}<span class="text-[11px] muted"> · {t('slow.known.' + p.known)}</span>{/if}</span>
                  <span class="flex items-center gap-1.5 shrink-0">
                    <span class="mono text-xs">{c.val(p)}</span>
                    {#if IS_WINDOWS && !p.known}<button class="btn btn-ghost btn-xs opacity-0 group-hover:opacity-100 transition-opacity" title={t('slow.end')} onclick={() => end(p)}><Icon name="x" size=".75rem" /></button>{/if}
                  </span>
                </div>
                <div class="bar mt-1" style="height:4px"><i style="width:{Math.max(2, c.w(p) * 100)}%"></i></div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </section>
  {#if at}<div class="text-[11px] muted text-center">{t('slow.snapshot')}</div>{/if}
{/if}
