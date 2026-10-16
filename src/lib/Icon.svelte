<script lang="ts">
  import { ICONS } from './icons';
  let { name, size = '1.15rem', style = '' }: { name: string; size?: string; style?: string } = $props();
</script>

<svg class="ico" style="width:{size};height:{size};{style}" viewBox="0 0 24 24">{@html ICONS[name] ?? ''}</svg>
