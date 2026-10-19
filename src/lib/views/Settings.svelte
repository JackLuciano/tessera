<script lang="ts">
  import { app } from '../store.svelte';
  import { t, i18n, LOCALES } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import Logo from '../Logo.svelte';
  import { updater } from '../update.svelte';

  let permDlg: HTMLDialogElement;
  let ack = $state(false);

  function onPermToggle(e: Event) {
    const el = e.target as HTMLInputElement;
    if (!el.checked) { app.setPermanent(false); return; }
    // Turning permanent delete on always needs an explicit confirmation
    el.checked = false;
    ack = false;
    permDlg.showModal();
  }
</script>

<div class="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">
<section class="glass p-6 reveal">
  <h2 class="text-lg font-semibold mb-1">{t('settings.general')}</h2>
  <div class="divide-y divide-[var(--glass-border)]">
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.language')}</div><div class="text-xs muted">{t('settings.languageHint')}</div></div>
      <select class="select select-sm w-44" value={i18n.locale} onchange={e => { i18n.setLocale((e.target as HTMLSelectElement).value); app.sendTrayLabels(); }}>
        {#each Object.entries(LOCALES) as [code, l] (code)}<option value={code}>{l.name}</option>{/each}
      </select>
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.theme')}</div><div class="text-xs muted">{t('settings.themeHint')}</div></div>
      <div class="join">
        {#each ['light', 'dark', 'system'] as const as m (m)}
          <button class="join-item btn btn-sm {app.mode === m ? 'btn-primary' : ''}" onclick={() => app.setMode(m)}>{t('settings.mode.' + m)}</button>
        {/each}
      </div>
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.animations')}</div><div class="text-xs muted">{t('settings.animationsHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.animations} onchange={e => app.setAnimations((e.target as HTMLInputElement).checked)} />
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.reopen')}</div><div class="text-xs muted">{t('settings.reopenHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.reopen} onchange={e => app.setReopen((e.target as HTMLInputElement).checked)} />
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.live')}</div><div class="text-xs muted">{t('settings.liveHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.live} onchange={e => app.setLive((e.target as HTMLInputElement).checked)} />
    </div>
  </div>

  <h2 class="text-lg font-semibold mt-6 mb-1">{t('settings.tray')}</h2>
  <div class="divide-y divide-[var(--glass-border)]">
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.closeToTray')}</div><div class="text-xs muted">{t('settings.closeToTrayHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.prefs.close_to_tray} onchange={e => app.setPrefs({ close_to_tray: (e.target as HTMLInputElement).checked })} />
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.trayMeter')}</div><div class="text-xs muted">{t('settings.trayMeterHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.prefs.tray_meter} onchange={e => app.setPrefs({ tray_meter: (e.target as HTMLInputElement).checked })} />
    </div>
  </div>

  <h2 class="text-lg font-semibold mt-6 mb-1">{t('settings.updates')}</h2>
  <div class="divide-y divide-[var(--glass-border)]">
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.autoUpdate')}</div><div class="text-xs muted">{t('settings.autoUpdateHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={updater.auto} onchange={e => updater.setAuto((e.target as HTMLInputElement).checked)} />
    </div>
  </div>
</section>

<div class="flex flex-col gap-4">
<section class="glass p-6 reveal" style="--i:1">
  <h2 class="text-lg font-semibold mb-1">{t('settings.deletion')}</h2>
  <div class="divide-y divide-[var(--glass-border)]">
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1">
        <div class="font-medium text-sm">{t('settings.perm')}</div>
        <div class="text-xs muted">{t('settings.permHint')} <b style="color:var(--color-error)">{t('settings.permWarn')}</b></div>
      </div>
      <input type="checkbox" class="toggle toggle-error" checked={app.permanentDelete} onchange={onPermToggle} />
    </div>
    {#if app.permanentDelete}
      <div class="danger-banner rounded-xl p-3 text-sm flex gap-2 items-start my-2"><Icon name="alert" style="color:var(--color-error)" /><div>{t('settings.permBanner')}</div></div>
    {/if}
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.shift')}</div><div class="text-xs muted">{t('settings.shiftHint')}</div></div>
      <span class="badge badge-ghost">{t('common.always')}</span>
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.protect')}</div><div class="text-xs muted">{t('settings.protectHint')}</div></div>
      <span class="badge badge-ghost">{t('common.always')}</span>
    </div>
  </div>
</section>

<section class="glass p-5 reveal" style="--i:2">
  <div class="flex items-center gap-4">
    <Logo size={52} />
    <div class="flex-1"><div class="font-semibold">Tessera {__APP_VERSION__}</div><div class="text-xs muted">{t('settings.about')}</div></div>
    <button class="btn btn-sm" disabled={updater.state === 'checking' || updater.state === 'downloading'} onclick={() => updater.check(false)}>
      {#if updater.state === 'checking'}<span class="loading loading-spinner loading-xs"></span>{/if}{t('update.check')}
    </button>
  </div>
</section>
</div>
</div>

<dialog bind:this={permDlg} class="modal">
  <div class="modal-box glass max-w-md">
    <div class="flex items-center gap-3">
      <span class="w-10 h-10 rounded-xl grid place-items-center" style="background:color-mix(in oklch,var(--color-error) 16%,transparent);color:var(--color-error)"><Icon name="alert" /></span>
      <h3 class="text-lg font-semibold">{t('settings.permConfirmTitle')}</h3>
    </div>
    <p class="text-sm muted mt-3">{t('settings.permConfirmText')}</p>
    <label class="flex items-center gap-2 mt-4 text-sm cursor-pointer"><input type="checkbox" class="checkbox checkbox-error checkbox-sm" bind:checked={ack} /> {t('confirm.ack')}</label>
    <div class="modal-action">
      <button class="btn btn-ghost" onclick={() => permDlg.close()}>{t('common.cancel')}</button>
      <button class="btn btn-error" disabled={!ack} onclick={() => { app.setPermanent(true); permDlg.close(); app.toast(t('settings.permOnToast'), 'warning'); }}>{t('settings.turnOn')}</button>
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
