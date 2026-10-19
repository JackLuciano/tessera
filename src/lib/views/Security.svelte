<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../store.svelte';
  import { api, type Security } from '../api';
  import { t } from '../i18n.svelte';
  import { openSystem } from '../sys';
  import Icon from '../Icon.svelte';
  import PageHead from '../PageHead.svelte';
  import Ring from '../Ring.svelte';

  let s = $state<Security | null>(null);
  onMount(() => { api.security().then(v => (s = v)).catch(e => app.toast(String(e), 'error')); });

  type Row = { key: string; ok: boolean | null; ico: string; link: string; detail?: string };
  let rows = $derived<Row[]>(s ? [
    { key: 'defender', ok: s.defender, ico: 'shield', link: 'windowsdefender:' },
    { key: 'realtime', ok: s.realtime, ico: 'eye', link: 'windowsdefender://threat' },
    { key: 'signatures', ok: s.signature_age === null ? null : s.signature_age <= 3, ico: 'refresh', link: 'windowsdefender://threat', detail: s.signature_age !== null ? t('sec.age', { n: s.signature_age }) : undefined },
    { key: 'firewall', ok: s.firewall, ico: 'lock', link: 'windowsdefender://network' },
    { key: 'bitlocker', ok: s.bitlocker, ico: 'drive', link: 'ms-settings:deviceencryption' },
    { key: 'secure_boot', ok: s.secure_boot, ico: 'power', link: 'ms-settings:recovery' },
    { key: 'uac', ok: s.uac, ico: 'shield', link: 'ms-settings:signinoptions' },
  ] : []);
  let known = $derived(rows.filter(r => r.ok !== null));
  let score = $derived(known.length ? (known.filter(r => r.ok).length / known.length) * 100 : null);
</script>

<section class="glass p-5 reveal">
  <PageHead ico="shield" title={t('sec.title')} sub={t('sec.sub')} />
</section>

{#if !s}
  <div class="glass p-10 text-center"><span class="loading loading-dots"></span></div>
{:else}
  <section class="grid grid-cols-1 xl:grid-cols-[auto_minmax(0,1fr)] gap-4 items-start">
    <div class="glass p-6 reveal flex flex-col items-center gap-2" style="--i:1">
      <Ring value={score} label={t('sec.score')} unit="%" size={150} warn={100} bad={70} invert />
      <div class="text-sm font-medium mt-1">{score === 100 ? t('sec.allGood') : t('sec.issues', { n: known.filter(r => !r.ok).length })}</div>
    </div>
    <div class="glass p-5 reveal" style="--i:2">
      <div class="divide-y divide-[var(--glass-border)]">
        {#each rows as r (r.key)}
          {@const tone = r.ok === null ? 'info' : r.ok ? 'success' : 'error'}
          <div class="status-row">
            <span class="status-ico" style="background:color-mix(in oklch,var(--color-{tone}) 14%,transparent);color:var(--color-{tone})"><Icon name={r.ico} size="1.05rem" /></span>
            <div class="flex-1 min-w-0">
              <div class="text-sm font-medium">{t('sec.' + r.key)}</div>
              <div class="text-xs muted">{r.detail ?? t('sec.' + r.key + '.hint')}</div>
            </div>
            <span class="text-xs font-semibold flex items-center gap-1.5" style="color:var(--color-{tone})">
              <i class={r.ok === null ? 'dot-warn' : r.ok ? 'dot-ok' : 'dot-bad'}></i>{r.ok === null ? t('sec.unknown') : r.ok ? t('sec.on') : t('sec.off')}
            </span>
            {#if r.ok !== true}<button class="btn btn-ghost btn-xs gap-1" onclick={() => openSystem(r.link)}>{t('sec.open')}<Icon name="external" size=".75rem" /></button>{/if}
          </div>
        {/each}
      </div>
      {#if s.bitlocker === null && !app.elevated}<div class="text-[11px] muted mt-2">{t('sec.bitlockerAdmin')}</div>{/if}
    </div>
  </section>
{/if}
