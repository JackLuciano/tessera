<script lang="ts">
  import type { TreeNode } from './api';
  import { fmt, num, css, nodeName } from './api';
  import { t } from './i18n.svelte';

  let { node, onopen, oncontext }: {
    node: TreeNode;
    onopen: (n: TreeNode, from: Rect | null) => void;
    oncontext: (ev: MouseEvent, n: TreeNode) => void;
  } = $props();

  interface Rect {
    node: TreeNode; x: number; y: number; w: number; h: number; depth: number;
    parent: Rect | null; leaf: boolean; header: number; shade: number;
  }

  const FONT = '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
  const MONO = '"Cascadia Mono", Consolas, monospace';
  const PAD = 3;

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  /** Static scene, re-rendered only on layout/theme change; hover just blits it and draws an overlay. */
  const base = typeof document !== 'undefined' ? document.createElement('canvas') : null;
  let W = 0, H = 0;
  let rects: Rect[] = [];
  let hover: Rect | null = $state(null);
  let tip = $state({ show: false, x: 0, y: 0 });
  let anim: { t0: number; s0: number; sy0: number; tx0: number; ty0: number } | null = null;
  let intro = 0;
  let pendingZoom: Rect | null = null;

  /** Squarified treemap layout (Bruls et al.). */
  function squarify(nodes: TreeNode[], x: number, y: number, w: number, h: number) {
    const items = nodes.filter(n => n.size > 0);
    const total = items.reduce((s, n) => s + n.size, 0);
    const out: { n: TreeNode; x: number; y: number; w: number; h: number }[] = [];
    if (!total || w <= 0 || h <= 0) return out;
    const scale = (w * h) / total;
    let row: { n: TreeNode; a: number }[] = [], rx = x, ry = y, rw = w, rh = h;
    const worst = (r: typeof row, len: number) => {
      const s = r.reduce((t, i) => t + i.a, 0); let mx = 0, mn = Infinity;
      for (const i of r) { mx = Math.max(mx, i.a); mn = Math.min(mn, i.a); }
      return Math.max((len * len * mx) / (s * s), (s * s) / (len * len * mn));
    };
    const lay = (r: typeof row) => {
      const s = r.reduce((t, i) => t + i.a, 0);
      if (rw >= rh) { const cw = s / rh; let cy = ry; for (const i of r) { const ch = i.a / cw; out.push({ n: i.n, x: rx, y: cy, w: cw, h: ch }); cy += ch; } rx += cw; rw -= cw; }
      else { const ch = s / rw; let cx = rx; for (const i of r) { const cw = i.a / ch; out.push({ n: i.n, x: cx, y: ry, w: cw, h: ch }); cx += cw; } ry += ch; rh -= ch; }
    };
    for (const n of items) {
      const it = { n, a: n.size * scale }, len = Math.min(rw, rh);
      if (!row.length || worst([...row, it], len) <= worst(row, len)) row.push(it);
      else { lay(row); row = [it]; }
    }
    if (row.length) lay(row);
    return out;
  }

  /** Nested layout: every folder big enough gets a header strip and its children laid out inside it. */
  function build() {
    const out: Rect[] = [];
    const nest = (children: TreeNode[], x: number, y: number, w: number, h: number, depth: number, parent: Rect | null) => {
      squarify(children, x, y, w, h).forEach((s, i) => {
        const canNest = s.n.children.length > 0 && s.w > 34 && s.h > 30;
        const header = !canNest ? 0 : s.w > 80 && s.h > 46 ? 20 : 5;
        const r: Rect = { node: s.n, x: s.x, y: s.y, w: s.w, h: s.h, depth, parent, leaf: !canNest, header, shade: (i % 4) * .045 };
        out.push(r);
        if (canNest) nest(s.n.children, s.x + PAD, s.y + header, s.w - 2 * PAD, s.h - header - PAD, depth + 1, r);
      });
    };
    nest(node.children, 0, 0, W, H, 1, null);
    rects = out;
  }

  function fit(c: CanvasRenderingContext2D, s: string, maxW: number) {
    if (maxW <= 8) return '';
    if (c.measureText(s).width <= maxW) return s;
    while (s.length > 1 && c.measureText(s + '…').width > maxW) s = s.slice(0, -1);
    return s.length > 1 ? s + '…' : '';
  }

  const top = (r: Rect) => { let x = r; while (x.parent) x = x.parent; return x; };

  /** Paints every block. `reveal` (0..1) fades levels in one after another on first show. */
  function paint(c: CanvasRenderingContext2D, alpha: number, reveal = 1) {
    const text = css('--color-base-content');
    const colors: Record<string, string> = {};
    const col = (cat: string) => (colors[cat] ??= css(`--cat-${cat}`));
    const total = node.size || 1;
    for (const r of rects) {
      const x = r.x + .5, y = r.y + .5, w = r.w - 1, h = r.h - 1;
      if (w < 1 || h < 1) continue;
      const a = alpha * Math.max(0, Math.min(1, reveal * 2.2 - (r.depth - 1) * .3));
      if (a <= 0) continue;
      const color = col(r.node.cat);
      const rad = Math.max(1.5, Math.min(r.leaf ? 6 : 8, w / 3, h / 3) - (r.depth - 1));
      const bucket = r.node.kind === 'bucket';

      if (!r.leaf) {
        // Folder frame: tinted panel with a header strip
        c.globalAlpha = a * (r.depth === 1 ? .2 : .16);
        c.fillStyle = color;
        c.beginPath(); c.roundRect(x, y, w, h, rad); c.fill();
        c.globalAlpha = a * .38; c.strokeStyle = color; c.lineWidth = 1; c.stroke();
        if (r.header >= 20) {
          c.globalAlpha = a;
          c.fillStyle = text;
          c.font = `600 ${r.depth === 1 ? 12.5 : 11.5}px ${FONT}`;
          const s = fmt(r.node.size);
          const label = fit(c, nodeName(r.node), w - 14);
          c.fillText(label, x + 7, y + 14);
          c.font = `500 10.5px ${MONO}`;
          const sw = c.measureText(s).width;
          if (c.measureText(label).width + sw + 30 < w) { c.globalAlpha = a * .6; c.fillText(s, x + w - sw - 7, y + 14); }
        }
        continue;
      }

      // Leaf: cushion-shaded tile (light top-left, darker bottom-right)
      c.globalAlpha = a * (bucket ? .55 : 1);
      c.fillStyle = color;
      c.beginPath(); c.roundRect(x, y, w, h, rad); c.fill();
      const g = c.createLinearGradient(x, y, x + w, y + h);
      g.addColorStop(0, 'rgba(255,255,255,.22)');
      g.addColorStop(.5, 'rgba(255,255,255,0)');
      g.addColorStop(1, `rgba(0,0,0,${.18 + r.shade})`);
      c.fillStyle = g; c.fill();
      if (bucket) { c.globalAlpha = a * .5; c.strokeStyle = color; c.setLineDash([3, 3]); c.stroke(); c.setLineDash([]); }

      if (w > 44 && h > 18) {
        c.globalAlpha = a;
        c.fillStyle = '#fff';
        c.shadowColor = 'rgba(0,0,0,.45)'; c.shadowBlur = 3;
        const big = r.depth === 1 && w > 120 && h > 60;
        c.font = `600 ${big ? 13 : 11.5}px ${FONT}`;
        c.fillText(fit(c, nodeName(r.node), w - 12), x + 7, y + (big ? 19 : 15));
        if (h > 34) {
          c.globalAlpha = a * .85;
          c.font = `500 ${big ? 11.5 : 10.5}px ${MONO}`;
          const full = big ? `${fmt(r.node.size)} · ${(r.node.size / total * 100).toFixed(1)}%` : fmt(r.node.size);
          c.fillText(fit(c, full, w - 12), x + 7, y + (big ? 36 : 29));
        }
        c.shadowBlur = 0; c.shadowColor = 'transparent';
      }
    }
    c.globalAlpha = 1;
  }

  /** Hover overlay: dim everything outside the hovered top-level block, outline the hovered one. */
  function overlay(c: CanvasRenderingContext2D) {
    if (!hover) return;
    const tp = top(hover);
    c.save();
    c.beginPath(); c.rect(0, 0, W, H); c.roundRect(tp.x, tp.y, tp.w, tp.h, 8);
    c.fillStyle = 'rgba(0,0,0,.18)'; c.fill('evenodd');
    c.restore();
    const r = hover, rad = r.leaf ? 6 : 8;
    c.strokeStyle = '#fff'; c.lineWidth = 2; c.globalAlpha = .95;
    c.beginPath(); c.roundRect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, Math.min(rad, r.w / 3, r.h / 3)); c.stroke();
    if (r.leaf) { c.fillStyle = 'rgba(255,255,255,.12)'; c.fill(); }
    c.globalAlpha = 1;
  }

  function renderBase() {
    if (!base || !W) return;
    const dpr = devicePixelRatio || 1;
    base.width = W * dpr; base.height = H * dpr;
    const b = base.getContext('2d')!;
    b.setTransform(dpr, 0, 0, dpr, 0, 0);
    paint(b, 1);
  }

  function draw() {
    if (!W || !canvas) return;
    const c = canvas.getContext('2d')!, dpr = devicePixelRatio || 1;
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.clearRect(0, 0, canvas.width, canvas.height);
    const now = performance.now();
    if (anim) {
      const raw = Math.min(1, (now - anim.t0) / 650);
      const e = raw < .5 ? 4 * raw ** 3 : 1 - (-2 * raw + 2) ** 3 / 2;
      const s = anim.s0 + (1 - anim.s0) * e, sy = anim.sy0 + (1 - anim.sy0) * e;
      c.setTransform(dpr * s, 0, 0, dpr * sy, dpr * anim.tx0 * (1 - e), dpr * anim.ty0 * (1 - e));
      paint(c, .2 + .8 * e, .35 + .65 * e);
      if (raw < 1) requestAnimationFrame(draw); else { anim = null; renderBase(); draw(); }
      return;
    }
    if (intro) {
      const raw = Math.min(1, (now - intro) / 700);
      c.setTransform(dpr, 0, 0, dpr, 0, 0);
      paint(c, 1, 1 - (1 - raw) ** 3);
      if (raw < 1) { requestAnimationFrame(draw); return; }
      intro = 0; renderBase();
      c.setTransform(1, 0, 0, 1, 0, 0); c.clearRect(0, 0, canvas.width, canvas.height);
    }
    if (base) c.drawImage(base, 0, 0);
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    overlay(c);
  }

  function resize() {
    const dpr = devicePixelRatio || 1;
    W = wrap.clientWidth; H = wrap.clientHeight;
    canvas.width = W * dpr; canvas.height = H * dpr;
    build(); renderBase(); draw();
  }

  const animated = () => !document.documentElement.classList.contains('no-anim');

  let shownId = -1;
  // Re-layout whenever the node changes; zoom from the clicked block, fade levels in when navigating,
  // and just repaint when the same folder was refreshed (live changes, deletes)
  $effect(() => {
    void node;
    if (!W) return;
    build();
    const moved = node.id !== shownId;
    shownId = node.id;
    if (pendingZoom && animated()) {
      const r = pendingZoom;
      anim = { t0: performance.now(), s0: r.w / W, sy0: r.h / H, tx0: r.x, ty0: r.y };
    } else if (moved && animated()) intro = performance.now();
    else renderBase();
    pendingZoom = null;
    hover = null;
    requestAnimationFrame(draw);
  });

  $effect(() => {
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    // Redraw when the theme changes (colours are read from CSS variables)
    const mo = new MutationObserver(() => { renderBase(); draw(); });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => { ro.disconnect(); mo.disconnect(); };
  });

  function hit(ev: MouseEvent): Rect | null {
    const b = canvas.getBoundingClientRect(), mx = ev.clientX - b.left, my = ev.clientY - b.top;
    let h: Rect | null = null;
    for (const r of rects) if (mx >= r.x && mx < r.x + r.w && my >= r.y && my < r.y + r.h && (!h || r.depth > h.depth)) h = r;
    return h;
  }
  function onmove(ev: MouseEvent) {
    if (anim || intro) return;
    const h = hit(ev);
    if (h !== hover) { hover = h; draw(); }
    const b = canvas.getBoundingClientRect();
    tip = { show: !!h, x: Math.min(ev.clientX - b.left + 16, W - 270), y: ev.clientY - b.top + 16 > H - 130 ? ev.clientY - b.top - 130 : ev.clientY - b.top + 16 };
  }
  function onclick(ev: MouseEvent) {
    const h = hit(ev);
    if (!h) return;
    // Zoom into the clicked folder; a click on a file zooms into the folder that holds it
    let r: Rect | null = h;
    while (r && r.node.kind !== 'dir') r = r.parent;
    if (!r) return oncontext(ev, h.node);
    pendingZoom = r;
    onopen(r.node, r);
  }
  /** Allow parent lists to highlight a block. */
  export function highlight(id: number | null) {
    hover = id === null ? null : rects.find(r => r.depth === 1 && r.node.id === id) ?? null;
    draw();
  }
  const pathTo = (r: Rect) => { const p: string[] = []; for (let x: Rect | null = r.parent; x; x = x.parent) p.unshift(x.node.name); return p.join(' / '); };
</script>

<div bind:this={wrap} class="relative w-full h-full rounded-[.9rem] overflow-hidden">
  <canvas bind:this={canvas} class="block w-full h-full cursor-pointer"
    onmousemove={onmove} onmouseleave={() => { hover = null; tip.show = false; draw(); }}
    onclick={onclick} oncontextmenu={ev => { const h = hit(ev); if (h) { tip.show = false; oncontext(ev, h.node); } else ev.preventDefault(); }}></canvas>
  {#if tip.show && hover}
    {@const n = hover.node}
    <div class="tt glass" style="left:{tip.x}px;top:{tip.y}px">
      {#if hover.parent}<div class="text-[10.5px] muted truncate max-w-[240px] mb-0.5">{pathTo(hover)}</div>{/if}
      <div class="flex items-center gap-2 font-semibold"><span class="w-2.5 h-2.5 rounded-sm" style="background:var(--cat-{n.cat})"></span><span class="truncate max-w-[220px]">{nodeName(n)}</span></div>
      <div class="mono text-lg font-bold mt-1">{fmt(n.size)}</div>
      <div class="muted text-xs">{t('tm.share', { pct: (n.size / (node.size || 1) * 100).toFixed(1), parent: node.name })} · {t('cat.' + n.cat)}</div>
      {#if n.kind === 'dir'}<div class="text-[11px] mt-1" style="color:var(--color-primary)">{t('tm.clickOpen', { files: num(n.files) })}</div>{/if}
    </div>
  {/if}
</div>
