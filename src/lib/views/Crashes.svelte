<script lang="ts">
  import { app } from '../store.svelte';
  import { api, type Crash } from '../api';
  import { t } from '../i18n.svelte';
  import { day, clock } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';

  let days = $state(30);
  let list = $state<Crash[] | null>(null);

  $effect(() => {
    const d = days;
    list = null;
    api.crashes(d).then(v => (list = v)).catch(e => { app.toast(String(e), 'error'); list = []; });
  });

  const KIND: Record<string, { ico: string; tone: string }> = {
    bsod: { ico: 'alert', tone: 'error' }, power_loss: { ico: 'power', tone: 'warning' }, app_crash: { ico: 'x', tone: 'warning' }, app_hang: { ico: 'clock', tone: 'info' },
  };
  let counts = $derived(['bsod', 'power_loss', 'app_crash', 'app_hang'].map(k => [k, (list ?? []).filter(c => c.kind === k).length] as const));
  /** The apps and drivers that show up most, so the user knows what to update or reinstall. */
  let top = $derived.by(() => {
    const m = new Map<string, number>();
    for (const c of list ?? []) {
      const key = c.culprit ?? (c.kind === 'app_crash' || c.kind === 'app_hang' ? c.app : '');
      if (key) m.set(key, (m.get(key) ?? 0) + 1);
    }
    return [...m].sort((a, b) => b[1] - a[1]).slice(0, 5);
  });
  let byDay = $derived.by(() => {
    const g: [string, Crash[]][] = [];
    for (const c of list ?? []) {
      const d = day(c.at);
      const last = g.at(-1);
      if (last && last[0] === d) last[1].push(c); else g.push([d, [c]]);
    }
    return g;
  });
  const time = clock;
</script>

<section class="glass p-5 reveal">
  <PageHead ico="alert" title={t('crash.title')} sub={t('crash.sub')}>
    <div class="join">
      {#each [7, 30, 90] as d (d)}<button class="join-item btn btn-sm {days === d ? 'btn-primary' : ''}" onclick={() => (days = d)}>{t('crash.days', { n: d })}</button>{/each}
    </div>
  </PageHead>
  <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mt-5">
    {#each counts as [k, n] (k)}
      <div class="rounded-xl soft p-3 flex items-center gap-3">
        <span class="status-ico" style="background:color-mix(in oklch,var(--color-{KIND[k].tone}) 15%,transparent);color:var(--color-{KIND[k].tone})"><Icon name={KIND[k].ico} size="1rem" /></span>
        <div class="metric"><b>{list ? n : '—'}</b><span>{t('crash.kind.' + k)}</span></div>
      </div>
    {/each}
  </div>
</section>

{#if list === null}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else if !list.length}
  <div class="glass p-10 text-center reveal"><Icon name="check" size="2rem" style="color:var(--color-success)" /><div class="mt-2 font-medium">{t('crash.none', { n: days })}</div></div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-4 items-start">
    <div class="glass p-5 reveal" style="--i:1">
      <div class="font-semibold">{t('crash.top')}</div>
      <div class="text-xs muted">{t('crash.topSub')}</div>
      <div class="flex flex-col gap-2 mt-3">
        {#each top as [name, n] (name)}
          <div>
            <div class="flex justify-between text-sm gap-2"><span class="truncate">{name}</span><span class="mono">{n}×</span></div>
            <div class="bar mt-1" style="height:5px"><i style="width:{(n / top[0][1]) * 100}%;background:var(--color-warning)"></i></div>
          </div>
        {/each}
      </div>
      {#if list.some(c => c.kind === 'bsod')}<div class="rounded-xl p-3 mt-4 text-xs" style="background:color-mix(in oklch,var(--color-error) 10%,transparent)">{t('crash.bsodHint')}</div>{/if}
    </div>
    <div class="glass p-5 reveal" style="--i:2">
      {#each byDay as [d, items] (d)}
        <div class="text-[11px] uppercase tracking-wider muted font-semibold mt-2 first:mt-0">{d}</div>
        <div class="flex flex-col divide-y divide-[var(--glass-border)]">
          {#each items as c (c.at + c.kind + c.app)}
            <div class="flex items-center gap-3 py-2">
              <span class="w-8 h-8 rounded-lg grid place-items-center shrink-0" style="background:color-mix(in oklch,var(--color-{KIND[c.kind].tone}) 14%,transparent);color:var(--color-{KIND[c.kind].tone})"><Icon name={KIND[c.kind].ico} size=".9rem" /></span>
              <div class="flex-1 min-w-0">
                <div class="text-sm font-medium truncate">{c.kind === 'bsod' ? t('crash.kind1.bsod') : c.kind === 'power_loss' ? t('crash.kind1.power_loss') : c.app}</div>
                <div class="text-[11px] muted truncate">
                  {#if c.culprit}{c.culprit}{:else if c.module}{c.module}{:else}{t('crash.kind1.' + c.kind)}{/if}{#if c.code} · <span class="mono">{c.code}</span>{/if}
                </div>
              </div>
              <span class="text-xs muted mono">{time(c.at)}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  </section>
{/if}
