<script lang="ts">
  import { app } from '../store.svelte';
  import { api, type OptOutcome, type OptStatus, type Profile } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  const PROFILES: { key: Profile; ico: string; grad: string }[] = [
    { key: 'gaming', ico: 'rocket', grad: 'linear-gradient(135deg, oklch(65% 0.24 350), oklch(62% 0.22 290))' },
    { key: 'work', ico: 'briefcase', grad: 'linear-gradient(135deg, oklch(68% 0.15 230), oklch(64% 0.17 265))' },
    { key: 'balanced', ico: 'scale', grad: 'linear-gradient(135deg, oklch(70% 0.14 170), oklch(66% 0.15 210))' },
    { key: 'battery', ico: 'battery', grad: 'linear-gradient(135deg, oklch(78% 0.16 120), oklch(70% 0.16 160))' },
  ];
  /** What each profile turns on, shown as chips on its card (mirrors PROFILES in optimizer.rs). */
  const PROFILE_TWEAKS: Record<Profile, string[]> = {
    gaming: ['game_mode', 'no_game_dvr', 'raw_mouse', 'focus', 'hags', 'game_priority'],
    work: ['focus', 'no_web_search', 'no_suggestions'],
    balanced: ['game_mode'],
    battery: ['no_background_apps', 'no_transparency', 'perf_visuals'],
  };
  const TWEAK_ICO: Record<string, string> = {
    game_mode: 'game', no_game_dvr: 'record', no_gamebar: 'game', perf_visuals: 'eye', no_transparency: 'layers',
    no_background_apps: 'app', raw_mouse: 'mouse', focus: 'bell', hags: 'cpu', net_throttling: 'network',
    game_priority: 'rocket', no_power_throttling: 'zap', no_web_search: 'search', no_suggestions: 'x',
    no_mpo: 'overview', end_task: 'x', no_activity: 'history', no_update_sharing: 'download',
  };
  /** Situational tweaks are listed apart: profiles never change them. */
  const EXTRA = ['no_mpo', 'end_task', 'no_activity', 'no_update_sharing'];

  let status = $state<OptStatus | null>(null);
  let busy = $state<string | null>(null);
  let restoreDlg: HTMLDialogElement;

  async function refresh() {
    try { status = await api.opt.status(); } catch (e) { app.toast(String(e), 'error'); }
  }
  $effect(() => { refresh(); });

  function report(out: OptOutcome, ok: string) {
    app.toast(ok, 'success');
    if (out.reboot) app.toast(t('opt.reboot'), 'info');
    if (out.failed.length) app.toast(t('opt.failed', { n: out.failed.length, first: out.failed[0] }), 'warning');
  }

  async function activate(p: Profile) {
    busy = p;
    try {
      const out = await api.opt.applyProfile(p);
      report(out, t('opt.applied', { profile: t('opt.profile.' + p), n: out.applied }));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function toggle(key: string, on: boolean) {
    busy = key;
    try {
      const out = await api.opt.setTweak(key, on);
      report(out, t(on ? 'opt.tweakOn' : 'opt.tweakOff', { name: t('opt.tweak.' + key) }));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }
  async function setPlan(guid: string) {
    try { await api.opt.setPowerPlan(guid); await refresh(); } catch (e) { app.toast(String(e), 'error'); }
  }
  async function restore() {
    busy = 'restore';
    try {
      const out = await api.opt.restore();
      restoreDlg.close();
      report(out, t('opt.restored'));
      await refresh();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = null; }
  }

  const tweakOn = (k: string) => status?.tweaks.find(x => x.key === k)?.on ?? false;
</script>

{#if status && !status.supported}
  <div class="glass p-10 text-center muted">{t('opt.windowsOnly')}</div>
{:else}
  <section class="glass p-5 reveal">
    <div class="flex items-start gap-4 flex-wrap mb-4">
      <div class="flex-1 min-w-64">
        <h2 class="text-lg font-semibold flex items-center gap-2"><Icon name="gauge" style="color:var(--color-primary)" />{t('opt.title')}</h2>
        <p class="text-xs muted mt-0.5 max-w-2xl">{t('opt.sub')}</p>
      </div>
      {#if status?.has_backup}
        <button class="btn btn-sm btn-ghost gap-1.5" onclick={() => restoreDlg.showModal()}><Icon name="history" size=".95rem" />{t('opt.restore')}</button>
      {/if}
    </div>
    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3 stagger">
      {#each PROFILES as p (p.key)}
        {@const active = status?.active === p.key}
        <div class="profile-card {active ? 'active' : ''}" style="--grad:{p.grad}">
          <div class="flex items-center gap-3">
            <span class="profile-ico"><Icon name={p.ico} size="1.3rem" /></span>
            <div class="min-w-0 flex-1">
              <div class="font-semibold truncate">{t('opt.profile.' + p.key)}</div>
              {#if active}<div class="text-[11px] font-semibold" style="color:var(--color-success)">● {t('opt.active')}</div>{/if}
            </div>
          </div>
          <p class="text-xs muted mt-3 flex-1">{t('opt.profile.' + p.key + '.desc')}</p>
          <div class="flex flex-wrap gap-1 mt-3 content-start">
            {#each PROFILE_TWEAKS[p.key] as k (k)}<span class="tag"><Icon name={TWEAK_ICO[k]} size=".7rem" />{t('opt.tweak.' + k)}</span>{/each}
          </div>
          <button class="btn btn-sm w-full mt-4 {active ? 'btn-ghost' : 'btn-primary'}" disabled={!!busy || active} onclick={() => activate(p.key)}>
            {#if busy === p.key}<span class="loading loading-spinner loading-xs"></span>{/if}
            {active ? t('opt.active') : t('opt.activate')}
          </button>
        </div>
      {/each}
    </div>
  </section>

  <AdminBanner text={t('opt.needAdmin')} />

  <section class="glass p-5 reveal" style="--i:2">
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div><div class="font-semibold">{t('opt.tweaks')}</div><div class="text-xs muted">{t('opt.tweaksSub')}</div></div>
      {#if status?.power_plans.length}
        <label class="flex items-center gap-2 text-xs muted">
          <Icon name="power" size=".9rem" />{t('opt.powerPlan')}
          <select class="select select-sm w-52" value={status.power_plans.find(p => p.active)?.guid} onchange={e => setPlan((e.target as HTMLSelectElement).value)}>
            {#each status.power_plans as p (p.guid)}<option value={p.guid}>{p.name}</option>{/each}
          </select>
        </label>
      {/if}
    </div>
    {#snippet row(tw: OptStatus['tweaks'][number])}
      {@const locked = tw.admin && !status?.elevated}
      <div class="flex items-center gap-3 py-3 {locked ? 'opacity-55' : ''}">
        <span class="w-9 h-9 rounded-xl grid place-items-center shrink-0 soft" style="color:var(--color-primary)"><Icon name={TWEAK_ICO[tw.key] ?? 'zap'} size="1.05rem" /></span>
        <div class="flex-1 min-w-0">
          <div class="font-medium text-sm flex items-center gap-1.5 flex-wrap">
            {t('opt.tweak.' + tw.key)}
            {#if tw.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" />{t('opt.admin')}</span>{/if}
            {#if tw.reboot}<span class="tag">{t('opt.restart')}</span>{/if}
          </div>
          <div class="text-xs muted">{t('opt.tweak.' + tw.key + '.hint')}</div>
        </div>
        {#if busy === tw.key}<span class="loading loading-spinner loading-xs"></span>{/if}
        <input type="checkbox" class="toggle toggle-primary toggle-sm" checked={tweakOn(tw.key)} disabled={locked || !!busy}
          onchange={e => toggle(tw.key, (e.target as HTMLInputElement).checked)} />
      </div>
    {/snippet}
    <div class="grid grid-cols-1 2xl:grid-cols-2 gap-x-8">
      <div class="divide-y divide-[var(--glass-border)] mt-2">
        {#each (status?.tweaks ?? []).filter(x => !EXTRA.includes(x.key)) as tw (tw.key)}{@render row(tw)}{/each}
      </div>
      <div class="mt-2">
        <div class="text-xs uppercase tracking-wider muted font-semibold mt-3">{t('opt.extras')}</div>
        <div class="text-xs muted">{t('opt.extrasSub')}</div>
        <div class="divide-y divide-[var(--glass-border)] mt-1">
          {#each (status?.tweaks ?? []).filter(x => EXTRA.includes(x.key)) as tw (tw.key)}{@render row(tw)}{/each}
        </div>
      </div>
    </div>
  </section>
{/if}

<dialog bind:this={restoreDlg} class="modal">
  <div class="modal-box glass max-w-md">
    <div class="flex items-center gap-3">
      <span class="w-10 h-10 rounded-xl grid place-items-center soft" style="color:var(--color-primary)"><Icon name="history" /></span>
      <h3 class="text-lg font-semibold">{t('opt.restore')}?</h3>
    </div>
    <p class="text-sm muted mt-3">{t('opt.restoreHint')}</p>
    <div class="modal-action">
      <button class="btn btn-ghost" onclick={() => restoreDlg.close()}>{t('common.cancel')}</button>
      <button class="btn btn-primary" disabled={busy === 'restore'} onclick={restore}>{#if busy === 'restore'}<span class="loading loading-spinner loading-sm"></span>{/if}{t('opt.restoreGo')}</button>
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
