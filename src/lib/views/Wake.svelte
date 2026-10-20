<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type WakeInfo } from '../api';
  import { t } from '../i18n.svelte';
  import { clock, day } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  // What woke the PC, what may wake it, and what keeps it awake right now
  let w = $state<WakeInfo | null>(null);
  let busy = $state<string | null>(null);

  async function load() { w = await api.wake.info().catch(e => (app.toast(String(e), 'error'), null)); }
  onMount(load);

  async function deviceOff(name: string) {
    busy = name;
    try { await api.wake.setDevice(name, false); app.toast(t('wake.deviceOff', { name }), 'success'); await load(); }
    catch (e) { app.toast(app.elevated ? String(e) : t('wake.needAdmin'), 'error'); }
    finally { busy = null; }
  }
  async function taskOff(path: string, name: string) {
    busy = path + name;
    try { await api.wake.taskOff(path, name); app.toast(t('wake.taskOff', { name }), 'success'); await load(); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }

  const hours = (s: number) => (s >= 3600 ? `${(s / 3600).toFixed(1)} h` : `${Math.round(s / 60)} min`);
  /** Rough kind of a wake source, for the icon: device, timer, button, other. */
  function kind(src: string) {
    const s = src.toLowerCase();
    if (s.includes('timer') || s.includes('időzítő') || s.includes('zeitgeber') || s.includes('temporizador') || s.includes('minuteur') || s.includes('czasomierz')) return 'clock';
    if (s.includes('button') || s.includes('gomb') || s.includes('taste') || s.includes('botón') || s.includes('bouton') || s.includes('przycisk')) return 'power';
    if (s.includes('device') || s.includes('eszköz') || s.includes('gerät') || s.includes('dispositivo') || s.includes('périphérique') || s.includes('urządzenie') || s.includes('usb') || s.includes('hid')) return 'mouse';
    return 'moon';
  }
  let topSource = $derived.by(() => {
    const m = new Map<string, number>();
    for (const x of w?.wakes ?? []) if (x.source) m.set(x.source, (m.get(x.source) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => b[1] - a[1])[0] ?? null;
  });
</script>

<section class="glass p-5 reveal">
  <PageHead ico="moon" title={t('wake.title')} sub={t('wake.sub')}>
    <button class="btn btn-sm btn-ghost border hairline gap-1.5" onclick={load}><Icon name="refresh" size=".9rem" />{t('action.refresh')}</button>
  </PageHead>
  {#if w}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mt-5">
      <div class="metric"><b>{w.wakes.length}</b><span>{t('wake.wakes')}</span></div>
      <div class="metric"><b>{w.devices.length}</b><span>{t('wake.devices')}</span></div>
      <div class="metric"><b>{w.tasks.length}</b><span>{t('wake.tasks')}</span></div>
      <div class="metric"><b style={w.blockers.length ? 'color:var(--color-warning)' : ''}>{w.blockers_need_admin ? '—' : w.blockers.length}</b><span>{t('wake.blockers')}</span></div>
    </div>
  {/if}
</section>

{#if !w}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:1">
      <div class="font-semibold">{t('wake.recent')}</div>
      <div class="text-xs muted">{t('wake.recentSub')}</div>
      {#if topSource && w.wakes.length > 2}
        <div class="soft rounded-xl p-3 mt-3 text-xs">{t('wake.mostly', { src: topSource[0], n: topSource[1] })}</div>
      {/if}
      {#if !w.wakes.length}
        <div class="text-sm muted text-center py-8">{t('wake.noWakes')}</div>
      {:else}
        <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)] max-h-96 overflow-auto">
          {#each w.wakes as x (x.at)}
            <div class="flex items-center gap-3 py-2">
              <Icon name={kind(x.source)} size="1rem" style="color:var(--color-primary)" />
              <div class="flex-1 min-w-0">
                <div class="text-sm truncate">{x.source || t('wake.unknown')}</div>
                <div class="text-[11px] muted">{day(x.at)} {clock(x.at)}{x.slept ? ` · ${t('wake.slept', { d: hours(x.slept) })}` : ''}{x.timer_owner ? ` · ${x.timer_owner}` : ''}</div>
              </div>
              <span class="text-[11px] muted shrink-0">{ago(x.at, true)}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="flex flex-col gap-4">
      <div class="glass p-5 reveal" style="--i:2">
        <div class="font-semibold">{t('wake.armed')}</div>
        <div class="text-xs muted">{t('wake.armedSub')}</div>
        {#if !w.devices.length}
          <div class="flex items-center gap-2 text-sm mt-3" style="color:var(--color-success)"><i class="dot-ok"></i>{t('wake.noDevices')}</div>
        {:else}
          <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
            {#each w.devices as d (d)}
              <div class="flex items-center gap-3 py-2">
                <Icon name="mouse" size=".95rem" style="opacity:.7" />
                <span class="text-sm flex-1 min-w-0 truncate">{d}</span>
                <button class="btn btn-xs btn-ghost border hairline" disabled={busy === d} onclick={() => deviceOff(d)}>
                  {#if busy === d}<span class="loading loading-spinner loading-xs"></span>{/if}{t('wake.stopWaking')}
                </button>
              </div>
            {/each}
          </div>
          {#if !app.elevated}<div class="text-[11px] muted mt-2">{t('wake.needAdmin')}</div>{/if}
        {/if}
      </div>

      <div class="glass p-5 reveal" style="--i:3">
        <div class="font-semibold">{t('wake.taskTitle')}</div>
        <div class="text-xs muted">{t('wake.taskSub')}</div>
        {#if !w.tasks.length}
          <div class="flex items-center gap-2 text-sm mt-3" style="color:var(--color-success)"><i class="dot-ok"></i>{t('wake.noTasks')}</div>
        {:else}
          <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
            {#each w.tasks as k (k.path + k.name)}
              <div class="flex items-center gap-3 py-2">
                <Icon name="clock" size=".95rem" style="opacity:.7" />
                <div class="flex-1 min-w-0">
                  <div class="text-sm truncate">{k.name}</div>
                  <div class="text-[11px] muted truncate">{k.path}</div>
                </div>
                {#if k.system}
                  <span class="badge badge-sm badge-ghost">{t('wake.windowsTask')}</span>
                {:else}
                  <button class="btn btn-xs btn-ghost border hairline" disabled={busy === k.path + k.name} onclick={() => taskOff(k.path, k.name)}>{t('wake.stopWaking')}</button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="glass p-5 reveal" style="--i:4">
        <div class="font-semibold">{t('wake.awake')}</div>
        <div class="text-xs muted">{t('wake.awakeSub')}</div>
        {#if w.blockers_need_admin}
          <div class="mt-3"><AdminBanner text={t('wake.blockersAdmin')} /></div>
        {:else if !w.blockers.length}
          <div class="flex items-center gap-2 text-sm mt-3" style="color:var(--color-success)"><i class="dot-ok"></i>{t('wake.noBlockers')}</div>
        {:else}
          <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
            {#each w.blockers as b, i (i)}
              <div class="py-2">
                <div class="flex items-center gap-2 text-sm"><span class="badge badge-sm badge-warning badge-soft">{t('wake.k.' + b.kind.toLowerCase())}</span><span class="truncate">{b.who}</span></div>
                {#if b.reason}<div class="text-[11px] muted mt-0.5">{b.reason}</div>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </section>
{/if}
