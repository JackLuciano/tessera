<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './store.svelte';
  import { api, fmt, type JunkRule, type RulePreview } from './api';
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';

  // The user's own Smart cleanup rules: list, editor with a live preview against the current scan
  let { onsaved }: { onsaved?: () => void } = $props();

  let rules = $state<JunkRule[]>([]);
  let draft = $state<JunkRule | null>(null);
  let preview = $state<RulePreview | null>(null);
  let previewErr = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => { api.junkRules.get().then(r => (rules = r)).catch(() => {}); });

  const blank = (): JunkRule => ({ id: Math.random().toString(36).slice(2, 10), name: '', kind: 'name', pattern: '', min_age_days: 0, safe: false, enabled: true });
  function edit(r?: JunkRule) { draft = r ? { ...r } : blank(); runPreview(); }

  // Preview while typing, a moment after the last keystroke
  function runPreview() {
    clearTimeout(timer);
    preview = null;
    previewErr = null;
    if (!draft?.pattern.trim() || !app.summary) return;
    const d = $state.snapshot(draft);
    timer = setTimeout(() => api.junkRules.preview(d).then(p => (preview = p)).catch(e => (previewErr = String(e))), 300);
  }

  async function persist(next: JunkRule[]) {
    try { await api.junkRules.set(next); rules = next; onsaved?.(); return true; } catch (e) { app.toast(String(e), 'error'); return false; }
  }
  async function saveDraft() {
    if (!draft) return;
    const d = { ...draft, name: draft.name.trim() || draft.pattern.trim() };
    const next = rules.some(r => r.id === d.id) ? rules.map(r => (r.id === d.id ? d : r)) : [...rules, d];
    if (await persist(next)) { draft = null; app.toast(t('rules.saved'), 'success'); }
  }
  const remove = (r: JunkRule) => persist(rules.filter(x => x.id !== r.id));
  const toggle = (r: JunkRule) => persist(rules.map(x => (x.id === r.id ? { ...x, enabled: !x.enabled } : x)));
</script>

<section class="glass p-4 reveal" style="--i:3">
  <div class="flex items-center justify-between gap-2 flex-wrap">
    <div>
      <div class="font-semibold flex items-center gap-2"><Icon name="wrench" style="color:var(--color-primary)" />{t('rules.title')}</div>
      <div class="text-xs muted mt-0.5">{t('rules.sub')}</div>
    </div>
    {#if !draft}<button class="btn btn-sm gap-1.5" onclick={() => edit()}><Icon name="plus" size=".9rem" />{t('rules.add')}</button>{/if}
  </div>

  {#if rules.length}
    <div class="flex flex-col mt-3 divide-y divide-[var(--glass-border)]">
      {#each rules as r (r.id)}
        <div class="flex items-center gap-3 py-2" class:opacity-50={!r.enabled}>
          <input type="checkbox" class="toggle toggle-primary toggle-xs" checked={r.enabled} onchange={() => toggle(r)} title={t('rules.enabled')} />
          <div class="flex-1 min-w-0">
            <div class="text-sm truncate">{r.name}</div>
            <div class="text-[11px] muted mono truncate">{t('rules.kind.' + r.kind)}: {r.pattern}{r.min_age_days ? ` · ${t('rules.olderThan', { n: r.min_age_days })}` : ''}</div>
          </div>
          <span class="badge badge-soft badge-xs {r.safe ? 'badge-success' : 'badge-warning'}">{r.safe ? t('cleanup.safe') : t('cleanup.review')}</span>
          <button class="btn btn-ghost btn-xs btn-square" title={t('rules.edit')} onclick={() => edit(r)}><Icon name="wrench" size=".8rem" /></button>
          <button class="btn btn-ghost btn-xs btn-square" title={t('rules.delete')} onclick={() => remove(r)}><Icon name="x" size=".8rem" /></button>
        </div>
      {/each}
    </div>
  {:else if !draft}
    <div class="text-xs muted mt-3">{t('rules.none')}</div>
  {/if}

  {#if draft}
    <div class="soft rounded-xl p-4 mt-3 grid grid-cols-1 lg:grid-cols-2 gap-4">
      <div class="flex flex-col gap-3">
        <label class="block"><span class="text-[11px] muted">{t('rules.name')}</span>
          <input class="input input-sm w-full" bind:value={draft.name} placeholder={t('rules.namePh')} /></label>
        <div class="flex gap-2">
          <select class="select select-sm w-36" bind:value={draft.kind} onchange={runPreview}>
            <option value="name">{t('rules.kind.name')}</option>
            <option value="path">{t('rules.kind.path')}</option>
          </select>
          <input class="input input-sm flex-1 mono" bind:value={draft.pattern} oninput={runPreview} placeholder={draft.kind === 'name' ? 'obj, *.egg-info, Logs' : 'C:\\Projects\\*\\bin'} />
        </div>
        <div class="text-[11px] muted">{t('rules.wildcards')}</div>
        <div class="flex items-center gap-4 flex-wrap">
          <label class="flex items-center gap-2 text-sm">{t('rules.age')}
            <input type="number" min="0" max="3650" class="input input-xs w-20 mono" bind:value={draft.min_age_days} oninput={runPreview} />
            <span class="text-[11px] muted">{t('rules.days')}</span></label>
          <label class="flex items-center gap-2 text-sm"><input type="checkbox" class="checkbox checkbox-primary checkbox-sm" bind:checked={draft.safe} />{t('rules.safe')}</label>
        </div>
        <div class="flex gap-2 justify-end">
          <button class="btn btn-sm btn-ghost" onclick={() => { draft = null; preview = null; }}>{t('common.cancel')}</button>
          <button class="btn btn-sm btn-primary" disabled={!draft.pattern.trim() || !!previewErr} onclick={saveDraft}>{t('rules.save')}</button>
        </div>
      </div>
      <div class="min-w-0">
        <div class="text-[11px] muted mb-1.5">{t('rules.preview')}</div>
        {#if !app.summary}
          <div class="text-xs muted">{t('rules.scanFirst')}</div>
        {:else if previewErr}
          <div class="text-xs" style="color:var(--color-error)">{previewErr}</div>
        {:else if !draft.pattern.trim()}
          <div class="text-xs muted">{t('rules.typePattern')}</div>
        {:else if !preview}
          <span class="loading loading-dots loading-sm"></span>
        {:else}
          <div class="text-sm"><b class="mono">{fmt(preview.size)}</b> <span class="muted">· {t('cleanup.nFolders', { n: preview.count })}</span></div>
          <div class="flex flex-col gap-0.5 mt-2">{#each preview.sample as s (s)}<div class="text-[11px] muted mono truncate selectable" title={s}>{s}</div>{/each}</div>
        {/if}
      </div>
    </div>
  {/if}
</section>
