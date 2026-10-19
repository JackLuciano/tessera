<script lang="ts">
  import { asker as a } from './ask.svelte';
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';

  let dlg: HTMLDialogElement;
  $effect(() => { if (a.open && !dlg.open) dlg.showModal(); else if (!a.open && dlg.open) dlg.close(); });
  let tone = $derived(a.opts?.danger ? 'error' : 'primary');
</script>

<dialog bind:this={dlg} class="modal" onclose={() => a.open && a.done(false)}>
  {#if a.opts}
    {@const o = a.opts}
    <div class="modal-box glass max-w-lg">
      <div class="flex items-center gap-3">
        <span class="w-10 h-10 rounded-xl grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--color-{tone}) 16%,transparent);color:var(--color-{tone})">
          <Icon name={o.ico ?? (o.danger ? 'alert' : 'info')} />
        </span>
        <h3 class="text-lg font-semibold">{o.title}</h3>
      </div>
      {#if o.text}<p class="text-sm muted mt-3">{o.text}</p>{/if}
      {#if o.items?.length}
        <ul class="text-sm mt-3 flex flex-col gap-1 max-h-56 overflow-y-auto pr-1">
          {#each o.items as it, i (i)}
            <li class="flex justify-between gap-3"><span class="truncate">{it.name}</span>{#if it.detail}<span class="mono muted shrink-0 text-xs">{it.detail}</span>{/if}</li>
          {/each}
        </ul>
      {/if}
      {#if o.note}
        <div class="rounded-xl soft p-3 mt-3 text-xs flex gap-2 items-start"><Icon name="shield" size=".95rem" style="color:var(--color-primary)" /><span>{o.note}</span></div>
      {/if}
      {#if o.ack}
        <label class="flex items-center gap-2 mt-4 text-sm cursor-pointer"><input type="checkbox" class="checkbox checkbox-{tone} checkbox-sm" bind:checked={a.ack} /> {o.ack}</label>
      {/if}
      <div class="modal-action">
        <button class="btn btn-ghost" onclick={() => a.done(false)}>{t('common.cancel')}</button>
        <button class="btn btn-{tone}" disabled={!!o.ack && !a.ack} onclick={() => a.done(true)}>{o.confirm}</button>
      </div>
    </div>
  {/if}
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
