<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type Upgrade } from '../api';
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
      try { await api.installUpdate(i.id); status = { ...status, [i.id]: 'done' }; done++; }
      catch (e) { status = { ...status, [i.id]: 'failed' }; errors = { ...errors, [i.id]: String(e) }; }
    }
    running = false;
    app.toast(t('upd.finished', { n: done, total: picked.length }), done === picked.length ? 'success' : 'warning');
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
