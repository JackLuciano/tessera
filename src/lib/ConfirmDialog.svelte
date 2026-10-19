<script lang="ts">
  import { confirmDelete as c } from './confirm.svelte';
  import { fmt } from './api';
  import Icon from './Icon.svelte';
  import { t } from './i18n.svelte';

  let dlg: HTMLDialogElement;
  $effect(() => { if (c.open && !dlg.open) dlg.showModal(); else if (!c.open && dlg.open) dlg.close(); });
</script>

<!-- While deleting, Esc and the backdrop must not close the dialog (the work keeps running) -->
<dialog bind:this={dlg} class="modal" onclose={() => (c.open = false)} oncancel={e => c.busy && e.preventDefault()}>
  <div class="modal-box glass max-w-md">
    <div class="flex items-center gap-3">
      <span class="w-10 h-10 rounded-xl grid place-items-center shrink-0"
        style="background:color-mix(in oklch,var(--color-{c.permanent ? 'error' : 'primary'}) 16%,transparent);color:var(--color-{c.permanent ? 'error' : 'primary'})">
        <Icon name={c.permanent ? 'alert' : 'trash'} />
      </span>
      <h3 class="text-lg font-semibold">
        {c.permanent ? t('confirm.titlePerm') : t('confirm.title')}
        <span class="mono {c.permanent ? '' : 'grad-text'}" style={c.permanent ? 'color:var(--color-error)' : ''}>{fmt(c.total)}</span>?
      </h3>
    </div>
    <p class="text-sm muted mt-3">
      {#if c.permanent}
        <b style="color:var(--color-error)">{t('confirm.permWarn')}</b> {t('confirm.permText')}
      {:else}
        {t('confirm.binText')}
      {/if}
    </p>
    <ul class="text-sm mt-3 flex flex-col gap-1 max-h-48 overflow-y-auto">
      {#each c.items as it (it.id)}
        <li class="flex justify-between gap-3"><span class="muted truncate">{it.name}</span><span class="mono shrink-0">{fmt(it.size)}</span></li>
      {/each}
    </ul>
    {#if c.busy}
      {@const p = c.progress}
      {@const pct = p && p.total_bytes ? Math.min(100, (p.bytes / p.total_bytes) * 100) : 0}
      {@const eta = c.eta}
      <div class="mt-4">
        <div class="flex justify-between items-baseline text-sm">
          <span class="font-medium">{c.stopping ? t('confirm.stopping') : c.permanent ? t('confirm.deleting') : t('confirm.recycling')}</span>
          <span class="mono font-semibold">{pct.toFixed(0)}%</span>
        </div>
        <div class="progress-flow mt-2 {p ? '' : 'indeterminate'}" style={c.permanent ? '--color-primary:var(--color-error)' : ''}><i style="width:{pct}%"></i></div>
        <div class="flex justify-between gap-3 text-[11px] muted mono mt-1.5">
          <span>{t('confirm.items', { done: (p?.done ?? 0).toLocaleString(), total: (p?.total ?? c.items.length).toLocaleString() })}</span>
          <span>{fmt(p?.bytes ?? 0)} / {fmt(p?.total_bytes ?? c.total)}{#if eta !== null} · {t('confirm.left', { time: eta < 60 ? `${Math.ceil(eta)} s` : `${Math.ceil(eta / 60)} min` })}{/if}</span>
        </div>
      </div>
    {:else if c.permanent}
      <label class="flex items-center gap-2 mt-4 text-sm cursor-pointer">
        <input type="checkbox" class="checkbox checkbox-error checkbox-sm" bind:checked={c.ack} /> {t('confirm.ack')}
      </label>
    {/if}
    <div class="modal-action">
      {#if c.busy}
        <button class="btn btn-ghost" onclick={() => c.stop()} disabled={c.stopping}>{t('confirm.stop')}</button>
      {:else}
        <button class="btn btn-ghost" onclick={() => (c.open = false)}>{t('common.cancel')}</button>
      {/if}
      <button class="btn {c.permanent ? 'btn-error' : 'btn-primary'}" disabled={c.busy || (c.permanent && !c.ack)} onclick={() => c.run()}>
        {#if c.busy}<span class="loading loading-spinner loading-sm"></span>{/if}
        {c.permanent ? t('action.deletePerm') : t('action.recycle')}
      </button>
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button disabled={c.busy}>close</button></form>
</dialog>
