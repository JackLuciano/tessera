<script lang="ts">
  import { app } from './store.svelte';
  import { api } from './api';
  import { t } from './i18n.svelte';
  import Icon from './Icon.svelte';

  let { text, i = 1 }: { text: string; i?: number } = $props();
</script>

{#if !app.elevated}
  <div class="glass p-3 px-4 reveal flex items-center gap-3 text-sm" style="--i:{i}">
    <Icon name="shield" style="color:var(--color-warning)" />
    <span class="flex-1">{text}</span>
    <button class="btn btn-xs btn-ghost" onclick={() => api.relaunchAsAdmin().catch(e => app.toast(String(e), 'info'))}>{t('engine.elevateShort')}</button>
  </div>
{/if}
