<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, fmt, num, type PrivacyItem } from '../api';
  import { t } from '../i18n.svelte';
  import { ask } from '../ask.svelte';
  import { BROWSERS, failures } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  const KIND_ICO: Record<string, string> = {
    cache: 'cache', cookies: 'lock', history: 'history', session: 'layers',
    recent: 'files', run_history: 'play', typed_paths: 'folder', search_history: 'search', clipboard: 'copy',
  };
  /** Ticked by default: nothing here signs you out or loses your history. */
  const SAFE = new Set(['cache', 'session', 'recent', 'run_history', 'typed_paths', 'search_history']);

  let items = $state<PrivacyItem[] | null>(null);
  let on = $state(new Set<string>());
  let busy = $state(false);

  async function load() {
    items = null;
    const list = await api.privacy().catch(e => { app.toast(String(e), 'error'); return []; });
    items = list;
    on = new Set(list.filter(i => SAFE.has(i.kind) && (!i.running || i.kind === 'cache')).map(i => i.id));
  }
  onMount(load);

  let groups = $derived.by(() => {
    const m = new Map<string, PrivacyItem[]>();
    for (const i of items ?? []) m.set(i.group, [...(m.get(i.group) ?? []), i]);
    // Browsers first (alphabetically by name), the system's own traces last
    return [...m].sort(([a], [b]) => (a === 'system' ? 1 : b === 'system' ? -1 : (BROWSERS[a]?.name ?? a).localeCompare(BROWSERS[b]?.name ?? b)));
  });
  let picked = $derived((items ?? []).filter(i => on.has(i.id)));
  let total = $derived(picked.reduce((s, i) => s + i.size, 0));
  let risky = $derived(picked.some(i => i.kind === 'cookies' || i.kind === 'history'));

  const blocked = (i: PrivacyItem) => i.running && i.kind !== 'cache';
  function toggle(i: PrivacyItem) { if (blocked(i)) return; const s = new Set(on); s.has(i.id) ? s.delete(i.id) : s.add(i.id); on = s; }
  const name = (g: string) => (g === 'system' ? t('privacy.system') : BROWSERS[g]?.name ?? g);
  const amount = (i: PrivacyItem) => (i.size ? fmt(i.size) : i.count ? t('common.nItems', { n: num(i.count) }) : '');

  async function clean() {
    const ok = await ask({
      title: t('privacy.confirm', { n: picked.length }), ico: 'eyeOff', confirm: t('privacy.clean'), danger: risky,
      text: risky ? t('privacy.confirmRisky') : t('privacy.confirmText'),
      items: picked.map(i => ({ name: `${name(i.group)} · ${t('privacy.kind.' + i.kind)}`, detail: amount(i) })),
    });
    if (!ok) return;
    busy = true;
    try {
      const out = await api.cleanPrivacy(picked.map(i => i.id));
      app.toast(t('privacy.cleaned', { n: out.cleaned, size: fmt(out.freed) }), 'success');
      if (out.running.length) app.toast(t('privacy.closeFirst', { names: out.running.map(name).join(', ') }), 'warning');
      failures(out.failed);
      await load();
    } finally { busy = false; }
  }
</script>

<section class="glass p-5 reveal">
  <PageHead ico="eyeOff" title={t('privacy.title')} sub={t('privacy.sub')}>
    <button class="btn btn-ghost btn-sm btn-square" title={t('action.refresh')} onclick={load} disabled={busy}><Icon name="refresh" size="1rem" /></button>
    <button class="btn btn-sm {risky ? 'btn-error' : 'btn-primary'} gap-2" disabled={!picked.length || busy} onclick={clean}>
      {#if busy}<span class="loading loading-spinner loading-xs"></span>{:else}<Icon name="sparkles" size="1rem" />{/if}
      {t('privacy.cleanN', { n: picked.length })}{#if total}<span class="mono">· {fmt(total)}</span>{/if}
    </button>
  </PageHead>
</section>

{#if items === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !items.length}
  <div class="glass p-10 text-center muted">{t('privacy.nothing')}</div>
{:else}
  <section class="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-4 items-start">
    {#each groups as [g, list], gi (g)}
      {@const running = list.some(i => i.running)}
      <div class="glass p-4 reveal" style="--i:{gi + 1}">
        <div class="flex items-center gap-3">
          {#if g === 'system'}
            <span class="w-10 h-10 rounded-xl grid place-items-center soft" style="color:var(--color-primary)"><Icon name="cpu" /></span>
          {:else}
            <span class="w-10 h-10 rounded-xl grid place-items-center font-bold text-white" style="background:{BROWSERS[g]?.color ?? 'var(--color-primary)'}">{name(g).replace(/^(Google|Microsoft) /, '')[0]}</span>
          {/if}
          <div class="flex-1 min-w-0">
            <div class="font-semibold truncate">{name(g)}</div>
            {#if running}<div class="text-[11px] flex items-center gap-1.5" style="color:var(--color-warning)"><i class="dot-warn"></i>{t('privacy.running')}</div>
            {:else}<div class="text-[11px] muted">{t('common.nItems', { n: list.length })}</div>{/if}
          </div>
        </div>
        <div class="flex flex-col gap-1 mt-3">
          {#each list as i (i.id)}
            {@const lock = blocked(i)}
            <label class="row-hover flex items-center gap-3 px-2 py-2 {lock ? 'opacity-50' : 'cursor-pointer'}" title={lock ? t('privacy.closeToClean', { name: name(g) }) : ''}>
              <input type="checkbox" class="checkbox checkbox-primary checkbox-sm" checked={on.has(i.id)} disabled={lock} onchange={() => toggle(i)} />
              <Icon name={KIND_ICO[i.kind] ?? 'file'} size="1rem" style="opacity:.7" />
              <div class="flex-1 min-w-0">
                <div class="text-sm truncate">{t('privacy.kind.' + i.kind)}</div>
                <div class="text-[11px] muted truncate">{t('privacy.kindHint.' + i.kind)}</div>
              </div>
              {#if i.kind === 'cookies' || i.kind === 'history'}<span class="tag tag-warn">{t('privacy.warn.' + i.kind)}</span>{/if}
              <span class="mono text-xs min-w-20 text-right shrink-0 whitespace-nowrap">{amount(i)}</span>
            </label>
          {/each}
        </div>
      </div>
    {/each}
  </section>
{/if}
