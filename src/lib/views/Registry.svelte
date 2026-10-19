<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, ago, type RegIssue, type RegBackup } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { rpNote, rpToast, failures } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import AdminBanner from '../AdminBanner.svelte';

  const CATS = ['uninstall', 'app_paths', 'startup', 'shared_dlls', 'installer_folders', 'mui_cache'];
  const CAT_ICO: Record<string, string> = { uninstall: 'package', app_paths: 'app', startup: 'rocket', shared_dlls: 'layers', installer_folders: 'folder', mui_cache: 'doc' };

  let issues = $state<RegIssue[] | null>(null);
  let backups = $state<RegBackup[]>([]);
  let on = $state(new Set<string>());
  let open = $state(new Set<string>());
  let busy = $state(false);

  async function load() {
    issues = null;
    const list = await api.registry().catch(e => { app.toast(String(e), 'error'); return []; });
    issues = list;
    on = new Set(list.filter(i => !i.admin || app.elevated).map(i => i.id));
    backups = await api.registryBackups().catch(() => []);
  }
  onMount(load);

  let byCat = $derived(CATS.map(c => [c, (issues ?? []).filter(i => i.cat === c)] as const).filter(([, l]) => l.length));
  let picked = $derived((issues ?? []).filter(i => on.has(i.id)));

  const usable = (i: RegIssue) => !i.admin || app.elevated;
  function toggle(id: string) { const s = new Set(on); s.has(id) ? s.delete(id) : s.add(id); on = s; }
  function toggleCat(list: readonly RegIssue[]) {
    const ids = list.filter(usable).map(i => i.id);
    const all = ids.every(id => on.has(id));
    const s = new Set(on);
    for (const id of ids) all ? s.delete(id) : s.add(id);
    on = s;
  }
  function toggleOpen(c: string) { const s = new Set(open); s.has(c) ? s.delete(c) : s.add(c); open = s; }

  async function fix() {
    const counts = CATS.map(c => [c, picked.filter(i => i.cat === c).length] as const).filter(([, n]) => n);
    const ok = await ask({
      title: t('registry.confirm', { n: picked.length }), text: t('registry.confirmText'), ico: 'registry', confirm: t('registry.fix'),
      items: counts.map(([c, n]) => ({ name: t('registry.cat.' + c), detail: String(n) })), note: rpNote(),
    });
    if (!ok) return;
    busy = true;
    try {
      const out = await api.fixRegistry(picked.map(i => i.id));
      app.toast(t('registry.fixed', { n: out.fixed }), 'success');
      rpToast(out.restore_point);
      failures(out.failed);
      await load();
    } catch (e) { app.toast(String(e), 'error'); }
    finally { busy = false; }
  }

  async function undo(b: RegBackup) {
    const ok = await ask({ title: t('registry.undoTitle'), text: t('registry.undoText', { n: b.items, when: ago(b.created, true) }), ico: 'undo', confirm: t('registry.undo') });
    if (!ok) return;
    try {
      const n = await api.undoRegistry(b.name);
      app.toast(t('registry.undone', { n }), 'success');
      await load();
    } catch (e) { app.toast(String(e), 'error'); }
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="registry" title={t('registry.title')} sub={t('registry.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={load} disabled={busy}><Icon name="refresh" size="1rem" /></button>
    <button class="btn btn-primary btn-sm gap-2" disabled={!picked.length || busy} onclick={fix}>
      {#if busy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="wrench" size="1rem" />{/if}
      {t('registry.fixN', { n: picked.length })}
    </button>
  </PageHead>
  {#if issues?.length}
    <div class="flex flex-wrap gap-2 mt-4">
      {#each byCat as [c, list] (c)}<span class="chip-soft"><Icon name={CAT_ICO[c]} size=".8rem" />{t('registry.cat.' + c)} · {list.length}</span>{/each}
    </div>
  {/if}
</section>

<AdminBanner text={t('registry.needAdmin')} />

{#if issues === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span><div class="text-xs muted mt-2">{t('registry.scanning')}</div></div>
{:else if !issues.length}
  <div class="glass p-10 text-center reveal"><Icon name="check" size="2rem" style="color:var(--color-success)" /><div class="mt-2 font-medium">{t('registry.clean')}</div></div>
{:else}
  <section class="flex flex-col gap-3">
    {#each byCat as [c, list], ci (c)}
      {@const ids = list.filter(usable).map(i => i.id)}
      {@const all = ids.length > 0 && ids.every(id => on.has(id))}
      <div class="glass reveal" style="--i:{ci + 2}">
        <div class="flex items-center gap-3 p-4">
          <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={all} indeterminate={!all && ids.some(id => on.has(id))} disabled={!ids.length} onchange={() => toggleCat(list)} />
          <span class="w-9 h-9 rounded-lg grid place-items-center soft shrink-0"><Icon name={CAT_ICO[c]} size="1rem" /></span>
          <button class="flex-1 min-w-0 text-left" onclick={() => toggleOpen(c)}>
            <div class="font-medium text-sm">{t('registry.cat.' + c)} <span class="muted font-normal">· {list.length}</span></div>
            <div class="text-[11px] muted truncate">{t('registry.catHint.' + c)}</div>
          </button>
          <button class="btn btn-ghost btn-xs btn-square" onclick={() => toggleOpen(c)} aria-label="toggle"><Icon name="chev" size=".9rem" style="transition:transform .25s;transform:rotate({open.has(c) ? 90 : 0}deg)" /></button>
        </div>
        {#if open.has(c)}
          <div class="px-4 pb-3 flex flex-col">
            {#each list as i (i.id)}
              <label class="row-hover flex items-center gap-3 px-2 py-1.5 {usable(i) ? 'cursor-pointer' : 'opacity-50'}">
                <input type="checkbox" class="checkbox checkbox-primary checkbox-xs" checked={on.has(i.id)} disabled={!usable(i)} onchange={() => toggle(i.id)} />
                <div class="flex-1 min-w-0">
                  <div class="text-sm truncate">{i.name}</div>
                  <div class="text-[11px] muted mono truncate selectable" title={i.missing}>{t('registry.missing')}: {i.missing}</div>
                </div>
                {#if i.admin}<span class="tag tag-warn"><Icon name="shield" size=".65rem" />{t('opt.admin')}</span>{/if}
              </label>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </section>
{/if}

{#if backups.length}
  <section class="glass p-4 reveal" style="--i:9">
    <div class="font-semibold flex items-center gap-2"><Icon name="history" size="1rem" />{t('registry.backups')}</div>
    <div class="text-xs muted">{t('registry.backupsSub')}</div>
    <div class="flex flex-col mt-2 divide-y divide-[var(--glass-border)]">
      {#each backups as b (b.name)}
        <div class="flex items-center gap-3 py-2">
          <div class="flex-1 min-w-0 text-sm">{ago(b.created, true)} <span class="muted">· {t('registry.entries', { n: b.items })}</span></div>
          <button class="btn btn-ghost btn-xs gap-1" onclick={() => undo(b)}><Icon name="undo" size=".85rem" />{t('registry.undo')}</button>
        </div>
      {/each}
    </div>
  </section>
{/if}
