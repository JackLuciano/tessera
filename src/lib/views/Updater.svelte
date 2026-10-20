<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type Upgrade, type UpdateDone } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  type St = 'queued' | 'running' | 'done' | 'failed';
  let items = $state<Upgrade[] | null>(null);
  let available = $state(true);
  let on = $state(new Set<string>());
  let status = $state<Record<string, St>>({});
  let errors = $state<Record<string, string>>({});
  let running = $state(false);

  async function load() {
    items = null;
    try {
      const r = await api.updates();
      available = r.available;
      items = r.items;
      on = new Set(r.items.map(i => i.id));
      status = {};
      errors = {};
    } catch (e) { app.toast(String(e), 'error'); items = []; }
  }
  onMount(load);

  async function run() {
    const picked = (items ?? []).filter(i => on.has(i.id));
    const ok = await ask({ title: t('upd.confirm', { n: picked.length }), text: t('upd.confirmText'), ico: 'upgrade', confirm: t('upd.go'), items: picked.map(i => ({ name: i.name, detail: `${i.version} → ${i.available}` })) });
    if (!ok) return;
    running = true;
    status = Object.fromEntries(picked.map(i => [i.id, 'queued' as St]));
    let done = 0;
    // One at a time: installers of different apps must not run in parallel
    for (const i of picked) {
      status = { ...status, [i.id]: 'running' };
      try { await api.installUpdate(i); status = { ...status, [i.id]: 'done' }; done++; }
      catch (e) { status = { ...status, [i.id]: 'failed' }; errors = { ...errors, [i.id]: String(e) }; }
    }
    running = false;
    app.toast(t('upd.finished', { n: done, total: picked.length }), done === picked.length ? 'success' : 'warning');
    loadHistory();
  }

  // ---- updates done through Tessera: roll back, keep a version ----
  let hist = $state<UpdateDone[]>([]);
  let pins = $state<string[]>([]);
  let rolling = $state<number | null>(null);
  let keepOld = $state(true);
  async function loadHistory() {
    const h = await api.updateHistory().catch(() => null);
    if (h) { hist = h.done; pins = h.pins; }
  }
  onMount(loadHistory);
  async function rollback(d: UpdateDone) {
    const ok = await ask({ title: t('upd.rbConfirm', { name: d.name }), text: t('upd.rbText', { from: d.from, to: d.to }), note: keepOld ? t('upd.rbPinNote') : undefined, ico: 'undo', confirm: t('upd.rbGo') });
    if (!ok) return;
    rolling = d.at;
    try { await api.rollbackUpdate(d.at, keepOld); app.toast(t('upd.rbDone', { name: d.name, v: d.from }), 'success'); }
    catch (e) { app.toast(String(e), 'error'); }
    finally { rolling = null; loadHistory(); }
  }
  async function unpin(id: string) {
    try { await api.pinUpdate(id, false); pins = pins.filter(p => p !== id); } catch (e) { app.toast(String(e), 'error'); }
  }
  function flip(id: string) { const s = new Set(on); s.has(id) ? s.delete(id) : s.add(id); on = s; }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="upgrade" title={t('upd.title')} sub={t('upd.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={load} disabled={running}><Icon name="refresh" size="1rem" /></button>
    <button class="btn btn-primary btn-sm gap-2" disabled={!on.size || running} onclick={run}>
      {#if running}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="download" size="1rem" />{/if}{t('upd.goN', { n: on.size })}
    </button>
  </PageHead>
</section>

<section class="glass p-3 reveal" style="--i:1">
  {#if items === null}
    <div class="p-8 text-center"><span class="loading loading-dots"></span><div class="text-xs muted mt-2">{t('upd.checking')}</div></div>
  {:else if !available}
    <div class="p-8 text-center muted text-sm">{t('upd.noWinget')}</div>
  {:else if !items.length}
    <div class="p-8 text-center"><Icon name="check" size="1.8rem" style="color:var(--color-success)" /><div class="mt-2 font-medium">{t('upd.none')}</div></div>
  {:else}
    <div class="flex flex-col divide-y divide-[var(--glass-border)]">
      {#each items as i (i.id)}
        {@const s = status[i.id]}
        <label class="flex items-center gap-3 py-2.5 px-2 cursor-pointer">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={on.has(i.id)} disabled={running} onchange={() => flip(i.id)} />
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{i.name}</div>
            <div class="text-[11px] muted mono truncate" title={errors[i.id] ?? i.id}>{errors[i.id] ?? i.id}</div>
          </div>
          <span class="mono text-xs muted max-md:hidden">{i.version}</span>
          <Icon name="chev" size=".8rem" style="opacity:.5" />
          <span class="mono text-xs font-semibold" style="color:var(--color-primary)">{i.available}</span>
          <span class="w-24 text-right text-xs">
            {#if s === 'running'}<span class="loading loading-spinner loading-xs"></span>
            {:else if s === 'done'}<span style="color:var(--color-success)">✓ {t('upd.st.done')}</span>
            {:else if s === 'failed'}<span style="color:var(--color-error)">{t('upd.st.failed')}</span>
            {:else if s === 'queued'}<span class="muted">{t('upd.st.queued')}</span>{/if}
          </span>
        </label>
      {/each}
    </div>
  {/if}
</section>

{#if hist.length || pins.length}
  <section class="glass p-5 reveal" style="--i:2">
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div><div class="font-semibold">{t('upd.history')}</div><div class="text-xs muted">{t('upd.historySub')}</div></div>
      <label class="flex items-center gap-2 text-xs"><input type="checkbox" class="checkbox checkbox-primary checkbox-xs" bind:checked={keepOld} />{t('upd.rbPin')}</label>
    </div>
    <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
      {#each hist.slice(0, 20) as d (d.at)}
        <div class="flex items-center gap-3 py-2.5">
          <Icon name={d.rolled_back ? 'undo' : 'upgrade'} size="1rem" style="color:var(--color-{d.rolled_back ? 'warning' : 'success'})" />
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{d.name}</div>
            <div class="text-[11px] muted">{ago(d.at, true)}{d.rolled_back ? ` · ${t('upd.rolledBack')}` : ''}</div>
          </div>
          <span class="mono text-xs muted">{d.from || '?'}</span>
          <Icon name="chev" size=".8rem" style="opacity:.5" />
          <span class="mono text-xs">{d.to}</span>
          <span class="w-28 text-right">
            {#if !d.rolled_back}
              <button class="btn btn-xs btn-ghost border hairline gap-1" disabled={rolling !== null || !/^\d/.test(d.from.replace(/^[<> ]+/, ''))} onclick={() => rollback(d)}>
                {#if rolling === d.at}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="undo" size=".75rem" />{/if}{t('upd.rbGo')}
              </button>
            {/if}
          </span>
        </div>
      {/each}
    </div>
    {#if pins.length}
      <div class="text-[11px] muted mt-4 mb-1.5">{t('upd.pinned')}</div>
      <div class="flex flex-wrap gap-1.5">
        {#each pins as p (p)}
          <span class="badge badge-ghost gap-1.5 mono text-xs">{p}<button class="opacity-60 hover:opacity-100" title={t('upd.unpin')} onclick={() => unpin(p)}>×</button></span>
        {/each}
      </div>
    {/if}
  </section>
{/if}
