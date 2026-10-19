<script lang="ts">
  import { app } from '../store.svelte';
  import { t, i18n, LOCALES } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import Logo from '../Logo.svelte';
  import { updater } from '../update.svelte';
  import { save as saveDialog, open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '../api';
  import { IS_WINDOWS } from '../nav';

  let permDlg: HTMLDialogElement;
  let ack = $state(false);

  // ---- hotkeys: click a field, press the combination ----
  const HOTKEYS = ['fps', 'boost', 'care', 'widget'].filter(k => IS_WINDOWS || (k !== 'fps' && k !== 'boost'));
  function captureKey(action: string, e: KeyboardEvent) {
    e.preventDefault();
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;
    if (e.key === 'Escape' || e.key === 'Backspace' || e.key === 'Delete') {
      app.setPrefs({ hotkeys: { ...app.prefs.hotkeys, [action]: '' } });
      (e.target as HTMLInputElement).blur();
      return;
    }
    const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean);
    // A global shortcut without a modifier would swallow that key everywhere: require one
    if (!mods.length) return;
    const key = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.key.length === 1 ? e.key.toUpperCase() : e.code;
    app.setPrefs({ hotkeys: { ...app.prefs.hotkeys, [action]: [...mods, key].join('+') } });
    (e.target as HTMLInputElement).blur();
  }

  // ---- settings file ----
  const LOCAL_KEYS = () => { const out: Record<string, string> = {}; try { for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i)!; if (k.startsWith('tessera-')) out[k] = localStorage.getItem(k) ?? ''; } } catch {} return out; };
  async function exportSettings() {
    const path = await saveDialog({ defaultPath: 'tessera-settings.json', filters: [{ name: 'Tessera', extensions: ['json'] }] });
    if (!path) return;
    try { await api.exportSettings(path, LOCAL_KEYS()); app.toast(t('io.exported'), 'success'); } catch (e) { app.toast(String(e), 'error'); }
  }
  async function importSettings() {
    const path = await openDialog({ multiple: false, filters: [{ name: 'Tessera', extensions: ['json'] }] });
    if (typeof path !== 'string') return;
    try {
      const ui = await api.importSettings(path);
      try { for (const [k, v] of Object.entries(ui ?? {})) if (k.startsWith('tessera-') && typeof v === 'string') localStorage.setItem(k, v); } catch {}
      app.toast(t('io.imported'), 'success');
      setTimeout(() => location.reload(), 900);
    } catch (e) { app.toast(String(e), 'error'); }
  }

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
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.widget')}</div><div class="text-xs muted">{t('settings.widgetHint')}</div></div>
      <label class="flex items-center gap-1.5 text-xs muted"><input type="checkbox" class="checkbox checkbox-xs" checked={app.prefs.widget_on_top} onchange={e => app.setPrefs({ widget_on_top: (e.target as HTMLInputElement).checked })} />{t('settings.onTop')}</label>
      <input type="checkbox" class="toggle toggle-primary" checked={app.prefs.widget} onchange={e => { const on = (e.target as HTMLInputElement).checked; app.prefs = { ...app.prefs, widget: on }; api.widget(on).catch(() => {}); }} />
    </div>
    <div class="flex items-center gap-4 py-4">
      <div class="flex-1"><div class="font-medium text-sm">{t('settings.weekly')}</div><div class="text-xs muted">{t('settings.weeklyHint')}</div></div>
      <input type="checkbox" class="toggle toggle-primary" checked={app.prefs.weekly} onchange={e => app.setPrefs({ weekly: (e.target as HTMLInputElement).checked })} />
    </div>
  </div>

  <h2 class="text-lg font-semibold mt-6 mb-1 flex items-center gap-2">{t('hotkeys.title')}
    <input type="checkbox" class="toggle toggle-primary toggle-sm ml-auto" checked={app.prefs.hotkeys_enabled} onchange={e => app.setPrefs({ hotkeys_enabled: (e.target as HTMLInputElement).checked })} />
  </h2>
  <div class="text-xs muted mb-2">{t('hotkeys.sub')}</div>
  <div class="divide-y divide-[var(--glass-border)] {app.prefs.hotkeys_enabled ? '' : 'opacity-50 pointer-events-none'}">
    {#each HOTKEYS as k (k)}
      <div class="flex items-center gap-4 py-3">
        <div class="flex-1 text-sm">{t('hotkeys.' + k)}</div>
        <input class="input input-sm w-44 hotkey" readonly value={app.prefs.hotkeys[k] || t('hotkeys.none')} onkeydown={e => captureKey(k, e)} title={t('hotkeys.hint')} />
      </div>
    {/each}
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

<section class="glass p-5 reveal" style="--i:3">
  <div class="font-semibold">{t('io.title')}</div>
  <div class="text-xs muted">{t('io.sub')}</div>
  <div class="flex gap-2 mt-3">
    <button class="btn btn-sm gap-1.5" onclick={exportSettings}><Icon name="export" size=".9rem" />{t('io.export')}</button>
    <button class="btn btn-sm gap-1.5" onclick={importSettings}><Icon name="import" size=".9rem" />{t('io.import')}</button>
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
