<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../Icon.svelte';

  // "Boost <game>?" — asked when a game starts and its setting is "ask". No answer = not now.
  const game = new URLSearchParams(location.search).get('game') ?? '';
  const SECONDS = 20;
  let left = $state(SECONDS);
  let remember = $state(false);
  let done = false;

  function answer(yes: boolean) {
    if (done) return;
    done = true;
    api.gamemode.answer(yes, remember).catch(() => {});
  }
  onMount(() => {
    const timer = setInterval(() => { left -= 1; if (left <= 0) { clearInterval(timer); answer(false); } }, 1000);
    return () => clearInterval(timer);
  });
</script>

<div class="mini-win prompt-win" data-tauri-drag-region>
  <div class="flex items-center gap-3" data-tauri-drag-region>
    <span class="w-10 h-10 rounded-xl grid place-items-center text-white shrink-0" style="background:linear-gradient(135deg,var(--color-primary),var(--color-accent))"><Icon name="rocket" /></span>
    <div class="min-w-0 flex-1" data-tauri-drag-region>
      <div class="font-semibold text-sm truncate" data-tauri-drag-region>{t('prompt.title', { game })}</div>
      <div class="text-[11px] muted" data-tauri-drag-region>{t('prompt.sub')}</div>
    </div>
  </div>
  <div class="flex items-center gap-2 mt-3">
    <label class="flex items-center gap-1.5 text-[11px] muted flex-1 cursor-pointer"><input type="checkbox" class="checkbox checkbox-xs checkbox-primary" bind:checked={remember} />{t('prompt.remember')}</label>
    <button class="btn btn-ghost btn-xs" onclick={() => answer(false)}>{t('prompt.no')}</button>
    <button class="btn btn-primary btn-xs gap-1" onclick={() => answer(true)}><Icon name="zap" size=".8rem" />{t('prompt.yes')}</button>
  </div>
  <div class="prompt-timer"><i style="width:{(left / SECONDS) * 100}%"></i></div>
</div>
