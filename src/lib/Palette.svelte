<script lang="ts">
  import { app, IS_WINDOWS, type View } from './store.svelte';
  import { api, fmt, type FileEntry } from './api';
  import { t } from './i18n.svelte';
  import { updater } from './update.svelte';
  import { reveal } from './actions';
  import Icon from './Icon.svelte';

  interface Cmd { label: string; sub?: string; ico: string; hint?: string; run: () => void }

  let dlg: HTMLDialogElement;
  let input: HTMLInputElement;
  let q = $state('');
  let sel = $state(0);
  let files = $state<FileEntry[]>([]);

  const views: [View, string][] = [['overview', 'overview'], ['explorer', 'explorer'], ['cleanup', 'cleanup'], ['files', 'files'], ['dupes', 'copy'], ['history', 'history'], ...(IS_WINDOWS ? [['optimizer', 'gauge'] as [View, string]] : []), ['settings', 'settings']];

  let commands = $derived<Cmd[]>([
    ...views.map(([v, ico]) => ({ label: t('palette.goto', { view: t('nav.' + v) }), ico, run: () => (app.view = v) })),
    { label: app.summary ? t('action.rescan') : t('action.scan'), ico: 'refresh', hint: 'F5', run: () => app.scan() },
    ...app.drives.map(d => ({ label: t('action.scanDrive', { drive: d.mount.replace(/\\$/, '') }), ico: 'drive', run: () => app.scan(d.mount) })),
    { label: t('palette.toggleTheme'), ico: 'moon', run: () => app.setMode(document.documentElement.dataset.theme === 'tessera-dark' ? 'light' : 'dark') },
    { label: t('update.check'), ico: 'refresh', run: () => updater.check(false) },
    ...files.map(f => ({ label: f.name, sub: f.path, ico: 'files', hint: fmt(f.size), run: () => reveal(f.path) })),
  ]);
  let list = $derived(q ? commands.filter(c => (c.label + ' ' + (c.sub ?? '')).toLowerCase().includes(q.toLowerCase())) : commands);

  $effect(() => {
    if (app.paletteOpen && !dlg.open) {
      q = ''; sel = 0;
      dlg.showModal();
      setTimeout(() => input?.focus(), 20);
      if (app.summary) api.largestFiles(40).then(f => (files = f)).catch(() => {});
    } else if (!app.paletteOpen && dlg.open) dlg.close();
  });

  function run(i: number) { const c = list[i]; if (!c) return; app.paletteOpen = false; setTimeout(c.run, 60); }
  function onkey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { sel = Math.min(list.length - 1, sel + 1); e.preventDefault(); }
    if (e.key === 'ArrowUp') { sel = Math.max(0, sel - 1); e.preventDefault(); }
    if (e.key === 'Enter') run(sel);
  }
</script>

<svelte:window onkeydown={e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); app.paletteOpen = !app.paletteOpen; } }} />

<dialog bind:this={dlg} class="modal modal-top pt-24" onclose={() => (app.paletteOpen = false)}>
  <div class="modal-box glass p-0 max-w-xl overflow-hidden">
    <label class="flex items-center gap-3 px-4 py-3 border-b hairline">
      <Icon name="search" style="opacity:.5" />
      <input bind:this={input} bind:value={q} oninput={() => (sel = 0)} onkeydown={onkey} class="flex-1 bg-transparent outline-none text-base" placeholder={t('palette.placeholder')} autocomplete="off" />
      <kbd class="kbd kbd-xs">Esc</kbd>
    </label>
    <div class="p-2 max-h-96 overflow-y-auto">
      {#each list as c, i (i)}
        <button class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-left {i === sel ? 'text-[var(--color-primary)] bg-[color-mix(in_oklch,var(--color-primary)_15%,transparent)]' : ''}"
          onmouseenter={() => (sel = i)} onclick={() => run(i)}>
          <Icon name={c.ico} />
          <div class="flex-1 min-w-0"><div class="truncate">{c.label}</div>{#if c.sub}<div class="text-[11px] muted mono truncate">{c.sub}</div>{/if}</div>
          {#if c.hint}<span class="text-xs muted mono">{c.hint}</span>{/if}
        </button>
      {:else}
        <div class="p-6 text-center muted text-sm">{t('palette.empty')}</div>
      {/each}
    </div>
  </div>
  <form method="dialog" class="modal-backdrop"><button>close</button></form>
</dialog>
