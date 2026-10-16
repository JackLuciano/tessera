<script lang="ts">
  import { updater as u } from './update.svelte';
  import { fmt } from './api';
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';

  let dlg: HTMLDialogElement;
  $effect(() => { if (u.modal && !dlg.open) dlg.showModal(); else if (!u.modal && dlg.open) dlg.close(); });
  let pct = $derived(u.total ? Math.round((u.downloaded / u.total) * 100) : 0);
</script>

<dialog bind:this={dlg} class="modal" onclose={() => (u.modal = false)}>
  <div class="modal-box glass max-w-lg">
    <div class="flex items-center gap-4">
      <div class="w-14 h-14 rounded-2xl grid place-items-center grad-bg shine shrink-0 text-white"><Icon name="refresh" size="1.6rem" /></div>
      <div>
        <div class="text-lg font-semibold">{u.state === 'ready' ? t('update.readyTitle', { version: u.version }) : t('update.availableTitle', { version: u.version })}</div>
        <div class="text-sm muted">{t('update.current', { version: __APP_VERSION__ })}</div>
      </div>
    </div>
    {#if u.notes}
      <div class="rounded-xl border hairline p-3 mt-4 text-sm max-h-56 overflow-y-auto whitespace-pre-line selectable">{u.notes}</div>
    {/if}
    {#if u.state === 'downloading'}
      <div class="mt-4">
        <div class="flex justify-between text-xs muted mb-1"><span>{t('update.downloading')} · {fmt(u.downloaded)}{u.total ? ` / ${fmt(u.total)}` : ''}</span><span class="mono">{pct}%</span></div>
        <progress class="progress progress-primary w-full" value={pct} max="100"></progress>
      </div>
    {/if}
    <div class="text-[11px] muted mt-3 flex items-center gap-1.5"><Icon name="check" size=".85rem" style="color:var(--color-success)" />{t('update.signed')}</div>
    <div class="modal-action">
      <button class="btn btn-sm" onclick={() => (u.modal = false)}>{t('update.later')}</button>
      {#if u.state === 'ready'}
        <button class="btn btn-primary btn-sm" onclick={() => u.installAndRestart()}>{t('update.restart')}</button>
      {:else}
        <button class="btn btn-primary btn-sm" disabled={u.state === 'downloading'} onclick={() => u.download()}>{t('update.install')}</button>
      {/if}
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
