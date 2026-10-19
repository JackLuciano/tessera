<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type UpdateStatus } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { openSystem } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  let s = $state<UpdateStatus | null>(null);
  let pending = $state<string[] | null>(null);
  let checking = $state(false);
  let resetting = $state(false);

  async function load() { s = await api.winupdate.status().catch(e => { app.toast(String(e), 'error'); return null; }); }
  onMount(load);

  async function check() {
    checking = true;
    try { pending = await api.winupdate.pending(); } catch (e) { app.toast(String(e), 'error'); }
    finally { checking = false; }
  }
  async function reset() {
    const ok = await ask({ title: t('wu.resetTitle'), text: t('wu.resetText'), ico: 'wrench', confirm: t('wu.reset') });
    if (!ok) return;
    resetting = true;
    try { await api.winupdate.reset(); app.toast(t('wu.resetDone'), 'success'); await load(); } catch (e) { app.toast(String(e), 'error'); }
    finally { resetting = false; }
  }
  let days = $derived(s?.last_install ? Math.floor((Date.now() / 1000 - s.last_install) / 86400) : null);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="download" title={t('wu.title')} sub={t('wu.sub')}>
    <button class="btn btn-sm btn-primary gap-1.5" onclick={() => openSystem('ms-settings:windowsupdate')}>{t('wu.open')}<Icon name="external" size=".8rem" /></button>
  </PageHead>
</section>

<AdminBanner text={t('wu.needAdmin')} />

{#if s}
  <section class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:1">
      <div class="divide-y divide-[var(--glass-border)]">
        <div class="status-row">
          <span class="status-ico soft" style="color:var(--color-{days !== null && days > 45 ? 'warning' : 'success'})"><Icon name="check" size="1.05rem" /></span>
          <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('wu.last')}</div><div class="text-xs muted truncate">{s.last_title ?? '—'}</div></div>
          <span class="text-sm mono">{s.last_install ? ago(s.last_install) : '—'}</span>
        </div>
        <div class="status-row">
          <span class="status-ico soft" style="color:var(--color-{s.reboot_pending ? 'warning' : 'success'})"><Icon name="refresh" size="1.05rem" /></span>
          <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('wu.reboot')}</div><div class="text-xs muted">{s.reboot_pending ? t('wu.rebootYes') : t('wu.rebootNo')}</div></div>
        </div>
        <div class="status-row">
          <span class="status-ico soft" style="color:var(--color-primary)"><Icon name="cpu" size="1.05rem" /></span>
          <div class="flex-1 min-w-0"><div class="text-sm font-medium">{t('wu.service')}</div><div class="text-xs muted">{t('wu.serviceHint')}</div></div>
          <span class="text-xs muted">{s.service_running === null ? '—' : s.service_running ? t('wu.running') : t('wu.idle')}</span>
        </div>
      </div>
      {#if days !== null && days > 45}<div class="rounded-xl p-3 mt-3 text-sm" style="background:color-mix(in oklch,var(--color-warning) 12%,transparent)">{t('wu.stale', { n: days })}</div>{/if}
    </div>

    <div class="glass p-5 reveal" style="--i:2">
      <div class="flex items-center justify-between gap-2">
        <div><div class="font-semibold">{t('wu.pending')}</div><div class="text-xs muted">{t('wu.pendingSub')}</div></div>
        <button class="btn btn-sm gap-1.5" disabled={checking} onclick={check}>{#if checking}<span class="loading loading-spinner loading-xs"></span>{/if}{t('wu.check')}</button>
      </div>
      {#if pending}
        {#if !pending.length}<div class="flex items-center gap-2 text-sm mt-4" style="color:var(--color-success)"><i class="dot-ok"></i>{t('wu.upToDate')}</div>
        {:else}<ul class="mt-3 flex flex-col gap-1.5">{#each pending as p (p)}<li class="text-sm flex gap-2"><Icon name="download" size=".9rem" style="opacity:.6" /><span>{p}</span></li>{/each}</ul>{/if}
      {/if}
      <div class="soft rounded-xl p-4 mt-4">
        <div class="text-sm font-medium flex items-center gap-1.5">{t('wu.stuck')}<span class="tag tag-warn"><Icon name="shield" size=".6rem" /></span></div>
        <div class="text-xs muted mt-0.5">{t('wu.stuckHint')}</div>
        <button class="btn btn-sm mt-3 gap-1.5" disabled={resetting || !app.elevated} onclick={reset}>{#if resetting}<span class="loading loading-spinner loading-xs"></span>{/if}<Icon name="wrench" size=".9rem" />{t('wu.reset')}</button>
      </div>
    </div>
  </section>
{/if}
