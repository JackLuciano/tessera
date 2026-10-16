<script lang="ts">
  import type { TreeNode } from './api';
  import { fmt, num, css } from './api';
  import { t } from './i18n.svelte';

  let { node, onopen, oncontext }: {
    node: TreeNode;
    onopen: (n: TreeNode, from: Rect | null) => void;
    oncontext: (ev: MouseEvent, n: TreeNode) => void;
  } = $props();

  interface Rect { node: TreeNode; x: number; y: number; w: number; h: number; depth: number; top?: TreeNode }

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let W = 0, H = 0;
  let rects: Rect[] = [];
  let hover: Rect | null = $state(null);
  let tip = $state({ show: false, x: 0, y: 0 });
  let anim: { t0: number; s0: number; sy0: number; tx0: number; ty0: number } | null = null;
  let pendingZoom: Rect | null = null;

  /** Squarified treemap layout (Bruls et al.). */
  function squarify(nodes: TreeNode[], x: number, y: number, w: number, h: number): Rect[] {
    const items = nodes.filter(n => n.size > 0);
    const total = items.reduce((s, n) => s + n.size, 0);
    if (!total || w <= 0 || h <= 0) return [];
    const scale = (w * h) / total;
    const out: Rect[] = [];
    let row: { n: TreeNode; a: number }[] = [], rx = x, ry = y, rw = w, rh = h;
    const worst = (r: typeof row, len: number) => {
      const s = r.reduce((t, i) => t + i.a, 0); let mx = 0, mn = Infinity;
      for (const i of r) { mx = Math.max(mx, i.a); mn = Math.min(mn, i.a); }
      return Math.max((len * len * mx) / (s * s), (s * s) / (len * len * mn));
    };
    const lay = (r: typeof row) => {
      const s = r.reduce((t, i) => t + i.a, 0);
      if (rw >= rh) { const cw = s / rh; let cy = ry; for (const i of r) { const ch = i.a / cw; out.push({ node: i.n, x: rx, y: cy, w: cw, h: ch, depth: 1 }); cy += ch; } rx += cw; rw -= cw; }
      else { const ch = s / rw; let cx = rx; for (const i of r) { const cw = i.a / ch; out.push({ node: i.n, x: cx, y: ry, w: cw, h: ch, depth: 1 }); cx += cw; } ry += ch; rh -= ch; }
    };
    for (const n of items) {
      const it = { n, a: n.size * scale }, len = Math.min(rw, rh);
      if (!row.length || worst([...row, it], len) <= worst(row, len)) row.push(it);
      else { lay(row); row = [it]; }
    }
    if (row.length) lay(row);
    return out;
  }

  function build() {
    const out: Rect[] = [];
    const G = 3;
    for (const r of squarify(node.children, 0, 0, W, H)) {
      out.push(r);
      if (r.node.children.length && r.w > 70 && r.h > 56) {
        for (const c of squarify(r.node.children, r.x + G + 3, r.y + 26, r.w - 2 * G - 6, r.h - 26 - G - 3)) out.push({ ...c, depth: 2, top: r.node });
      }
    }
    rects = out;
  }

  const ctx = () => canvas.getContext('2d')!;
  function fit(c: CanvasRenderingContext2D, s: string, maxW: number) {
    if (c.measureText(s).width <= maxW) return s;
    while (s.length > 1 && c.measureText(s + '…').width > maxW) s = s.slice(0, -1);
    return s.length > 1 ? s + '…' : '';
  }

  function drawRects(c: CanvasRenderingContext2D, alpha: number) {
    const text = css('--color-base-content');
    const colors: Record<string, string> = {};
    const col = (cat: string) => (colors[cat] ??= css(`--cat-${cat}`));
    const G = 3, total = node.size || 1;
    for (const r of rects) {
      const x = r.x + G / 2, y = r.y + G / 2, w = r.w - G, h = r.h - G;
      if (w < 1 || h < 1) continue;
      const color = col(r.node.cat);
      const isHover = hover && (hover === r || (r.depth === 1 && hover.top === r.node));
      c.fillStyle = color;
      if (r.depth === 1) {
        const inner = r.node.children.length > 0 && r.w > 70 && r.h > 56;
        c.globalAlpha = alpha * (inner ? (isHover ? .3 : .2) : (isHover ? 1 : .86)) * (r.node.kind === 'bucket' ? .6 : 1);
        c.beginPath(); c.roundRect(x, y, w, h, Math.min(9, w / 2, h / 2)); c.fill();
        if (isHover) { c.globalAlpha = alpha; c.strokeStyle = color; c.lineWidth = 2; c.stroke(); }
        c.globalAlpha = alpha;
        if (w > 60 && h > 22) {
          c.fillStyle = inner ? text : '#fff';
          c.font = '600 12.5px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
          const label = fit(c, r.node.name, w - 16);
          c.fillText(label, x + 9, y + 17);
          c.font = '500 11.5px "Cascadia Mono", Consolas, monospace';
          if (!inner && h > 40) {
            c.globalAlpha = alpha * .85;
            const full = `${fmt(r.node.size)} · ${(r.node.size / total * 100).toFixed(1)}%`;
            c.fillText(c.measureText(full).width <= w - 18 ? full : fit(c, fmt(r.node.size), w - 18), x + 9, y + 33);
          } else if (inner) {
            c.globalAlpha = alpha * .6;
            const s = fmt(r.node.size), sw = c.measureText(s).width;
            if (c.measureText(label).width + sw + 30 < w) c.fillText(s, x + w - sw - 9, y + 17);
          }
        }
      } else {
        const hot = hover === r;
        c.globalAlpha = alpha * (hot ? 1 : .82) * (r.node.kind === 'bucket' ? .6 : 1);
        c.beginPath(); c.roundRect(x, y, w, h, Math.min(6, w / 2, h / 2)); c.fill();
        if (hot) { c.globalAlpha = alpha; c.strokeStyle = '#fff'; c.lineWidth = 2; c.stroke(); }
        if (w > 48 && h > 20) {
          c.globalAlpha = alpha; c.fillStyle = '#fff';
          c.font = '600 11.5px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
          c.fillText(fit(c, r.node.name, w - 12), x + 7, y + 15);
          if (h > 34) { c.globalAlpha = alpha * .8; c.font = '500 10.5px "Cascadia Mono", Consolas, monospace'; c.fillText(fit(c, fmt(r.node.size), w - 12), x + 7, y + 29); }
        }
      }
    }
    c.globalAlpha = 1;
  }

  function draw() {
    if (!W || !canvas) return;
    const c = ctx(), dpr = devicePixelRatio || 1;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.clearRect(0, 0, W, H);
    if (anim) {
      const raw = Math.min(1, (performance.now() - anim.t0) / 600);
      const e = raw < .5 ? 4 * raw ** 3 : 1 - (-2 * raw + 2) ** 3 / 2;
      const s = anim.s0 + (1 - anim.s0) * e, sy = anim.sy0 + (1 - anim.sy0) * e;
      c.setTransform(dpr * s, 0, 0, dpr * sy, dpr * anim.tx0 * (1 - e), dpr * anim.ty0 * (1 - e));
      drawRects(c, .25 + .75 * e);
      if (raw < 1) requestAnimationFrame(draw); else { anim = null; draw(); }
      return;
    }
    drawRects(c, 1);
  }

  function resize() {
    const dpr = devicePixelRatio || 1;
    W = wrap.clientWidth; H = wrap.clientHeight;
    canvas.width = W * dpr; canvas.height = H * dpr;
    build(); draw();
  }

  // Re-layout whenever the node changes; animate from the rect that was clicked
  $effect(() => {
    void node;
    if (!W) return;
    build();
    if (pendingZoom) {
      const r = pendingZoom; pendingZoom = null;
      anim = { t0: performance.now(), s0: r.w / W, sy0: r.h / H, tx0: r.x, ty0: r.y };
    }
    hover = null;
    requestAnimationFrame(draw);
  });

  $effect(() => {
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    // Redraw when the theme changes (colours are read from CSS variables)
    const mo = new MutationObserver(() => draw());
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => { ro.disconnect(); mo.disconnect(); };
  });

  function hit(ev: MouseEvent): Rect | null {
    const b = canvas.getBoundingClientRect(), mx = ev.clientX - b.left, my = ev.clientY - b.top;
    let h: Rect | null = null;
    for (const r of rects) if (mx >= r.x && mx < r.x + r.w && my >= r.y && my < r.y + r.h) if (!h || r.depth > h.depth) h = r;
    return h;
  }
  function onmove(ev: MouseEvent) {
    if (anim) return;
    const h = hit(ev);
    if (h !== hover) { hover = h; draw(); }
    const b = canvas.getBoundingClientRect();
    tip = { show: !!h, x: Math.min(ev.clientX - b.left + 16, W - 260), y: ev.clientY - b.top + 16 > H - 110 ? ev.clientY - b.top - 110 : ev.clientY - b.top + 16 };
  }
  function onclick(ev: MouseEvent) {
    const h = hit(ev);
    if (!h) return;
    const target = h.depth === 2 && h.node.kind === 'dir' ? h.node : (h.top ?? h.node);
    if (target.kind !== 'dir') return oncontext(ev, target);
    pendingZoom = h.depth === 2 && h.node.kind === 'dir' ? h : rects.find(r => r.node === target && r.depth === 1) ?? h;
    onopen(target, pendingZoom);
  }
  /** Allow parent lists to highlight a block. */
  export function highlight(id: number | null) {
    hover = id === null ? null : rects.find(r => r.depth === 1 && r.node.id === id) ?? null;
    draw();
  }
</script>

<div bind:this={wrap} class="relative w-full h-full rounded-[.9rem] overflow-hidden">
  <canvas bind:this={canvas} class="block w-full h-full cursor-pointer"
    onmousemove={onmove} onmouseleave={() => { hover = null; tip.show = false; draw(); }}
    onclick={onclick} oncontextmenu={ev => { const h = hit(ev); if (h) { tip.show = false; oncontext(ev, h.node); } else ev.preventDefault(); }}></canvas>
  {#if tip.show && hover}
    {@const n = hover.node}
    <div class="tt glass" style="left:{tip.x}px;top:{tip.y}px">
      <div class="flex items-center gap-2 font-semibold"><span class="w-2.5 h-2.5 rounded-sm" style="background:var(--cat-{n.cat})"></span><span class="truncate max-w-[220px]">{n.name}</span></div>
      <div class="mono text-lg font-bold mt-1">{fmt(n.size)}</div>
      <div class="muted text-xs">{t('tm.share', { pct: (n.size / (node.size || 1) * 100).toFixed(1), parent: node.name })} · {t('cat.' + n.cat)}</div>
      {#if n.kind === 'dir'}<div class="text-[11px] mt-1" style="color:var(--color-primary)">{t('tm.clickOpen', { files: num(n.files) })}</div>{/if}
    </div>
  {/if}
</div>
