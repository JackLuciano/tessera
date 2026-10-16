<script lang="ts">
  // "Tile T" mark on the gradient app-icon tile
  let { size = 40 }: { size?: number } = $props();
</script>

<div class="grad-bg shine grid place-items-center shrink-0" style="width:{size}px;height:{size}px;border-radius:{size * 0.26}px">
  <svg viewBox="0 0 64 64" style="width:{size * 0.66}px;height:{size * 0.66}px" fill="#fff">
    <rect x="7" y="7" width="14" height="14" rx="4" />
    <rect x="25" y="7" width="14" height="14" rx="4" />
    <rect x="43" y="7" width="14" height="14" rx="4" />
    <rect x="25" y="25" width="14" height="14" rx="4" opacity=".8" />
    <rect x="25" y="43" width="14" height="14" rx="4" opacity=".6" />
  </svg>
</div>
