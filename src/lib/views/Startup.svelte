<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type StartupApp } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let apps = $state<StartupApp[] | null>(null);
  let q = $state('');

  onMount(async () => { apps = await api.opt.startup().catch(e => { app.toast(String(e), 'error'); return []; }); });

  async function set(a: StartupApp, enabled: boolean) {
    try {
      await api.opt.setStartup(a.id, enabled);
      a.enabled = enabled;
      app.toast(t(enabled ? 'startup.on' : 'startup.off', { name: a.name }), 'success');
    } catch (e) { app.toast(String(e), 'error'); apps = [...(apps ?? [])]; }
  }

  let list = $derived((apps ?? []).filter(a => !q || (a.name + a.command).toLowerCase().includes(q.toLowerCase())));
  let enabled = $derived((apps ?? []).filter(a => a.enabled).length);
  const source = (a: StartupApp) => (a.id.startsWith('folder') ? 'folder' : 'registry');
</script>

<section class="glass p-5 reveal">
  <PageHead ico="rocket" title={t('opt.startup')} sub={t('opt.startupSub')}>
    <label class="input input-sm w-56 flex items-center gap-2"><Icon name="search" size=".9rem" /><input bind:value={q} placeholder={t('files.filter')} /></label>
  </PageHead>
  {#if apps}
    <div class="grid grid-cols-3 gap-2 mt-4 max-w-md">
      <div class="tile-stat"><div>{t('startup.total')}</div><div>{apps.length}</div></div>
      <div class="tile-stat"><div>{t('startup.enabled')}</div><div style="color:var(--color-primary)">{enabled}</div></div>
      <div class="tile-stat"><div>{t('startup.disabled')}</div><div>{apps.length - enabled}</div></div>
    </div>
  {/if}
</section>

<section class="glass p-4 reveal" style="--i:1">
  {#if apps === null}
    <div class="p-8 text-center"><span class="loading loading-dots"></span></div>
  {:else if !list.length}
    <div class="p-8 text-center muted text-sm">—</div>
  {:else}
    <div class="flex flex-col divide-y divide-[var(--glass-border)]">
      {#each list as a (a.id)}
        {@const locked = a.scope === 'machine' && !app.elevated}
        <div class="flex items-center gap-3 py-2.5 px-1 {locked ? 'opacity-55' : ''}">
          <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 font-semibold text-sm soft" style="color:{a.enabled ? 'var(--color-primary)' : 'var(--muted)'}">{a.name.slice(0, 1).toUpperCase()}</span>
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{a.name}</div>
            <div class="text-[11px] muted mono truncate selectable" title={a.command}>{a.command}</div>
          </div>
          <span class="tag max-md:hidden">{t('startup.src.' + source(a))}</span>
          {#if a.scope === 'machine'}<span class="tag tag-warn" title={t('opt.startupMachine')}><Icon name="shield" size=".65rem" />{t('startup.allUsers')}</span>{/if}
          <input type="checkbox" class="toggle toggle-sm toggle-primary" checked={a.enabled} disabled={locked} onchange={e => set(a, (e.target as HTMLInputElement).checked)} />
        </div>
      {/each}
    </div>
  {/if}
</section>
