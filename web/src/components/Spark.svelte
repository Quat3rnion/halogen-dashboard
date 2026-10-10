<script lang="ts">
  // Tiny line of the last 5 minutes from the live buffer. Lines break at null values.
  import { dash } from '../lib/store.svelte';
  import { cssVar, theme } from '../lib/theme.svelte';
  import { alpha } from '../lib/color';
  import type { SeriesField } from '../lib/types';

  let { field, max = null, tone = 'none' }: { field: SeriesField; max?: number | null; tone?: string } = $props();

  let canvas: HTMLCanvasElement;
  let w = $state(0);
  let h = $state(0);

  function draw(): void {
    if (!canvas || w === 0 || h === 0) return;
    const dpr = devicePixelRatio || 1;
    const W = Math.round(w * dpr);
    const H = Math.round(h * dpr);
    if (canvas.width !== W || canvas.height !== H) {
      canvas.width = W;
      canvas.height = H;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, W, H);
    const b = dash.live;
    const t = b.t;
    const col = b.cols[field];
    const n = t.length;
    if (n < 2) return;
    const t1 = t[n - 1] ?? 0;
    const t0 = t1 - 5 * 60_000;
    let hi = max ?? 0;
    if (max === null) for (const v of col) if (v !== null && v > hi) hi = v;
    if (hi <= 0) hi = 1;
    const pad = 2 * dpr;
    const x = (tt: number) => ((tt - t0) / (t1 - t0)) * W;
    const y = (v: number) => H - pad - (Math.min(v, hi) / hi) * (H - pad * 2);
    const color = cssVar(tone === 'crit' ? '--down' : tone === 'warn' ? '--degraded' : '--s1');

    // area
    ctx.fillStyle = alpha(color, 0.14);
    let open = false;
    let startX = 0;
    let lastX = 0;
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const v = col[i];
      const tt = t[i] ?? 0;
      if (tt < t0) continue;
      if (v === null || v === undefined) {
        if (open) {
          ctx.lineTo(lastX, H);
          ctx.lineTo(startX, H);
          ctx.closePath();
          open = false;
        }
        continue;
      }
      const px = x(tt);
      if (!open) {
        ctx.moveTo(px, H);
        ctx.lineTo(px, y(v));
        startX = px;
        open = true;
      } else ctx.lineTo(px, y(v));
      lastX = px;
    }
    if (open) {
      ctx.lineTo(lastX, H);
      ctx.lineTo(startX, H);
      ctx.closePath();
    }
    ctx.fill();

    // line
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.25 * dpr;
    ctx.lineJoin = 'round';
    ctx.beginPath();
    let pen = false;
    for (let i = 0; i < n; i++) {
      const v = col[i];
      const tt = t[i] ?? 0;
      if (tt < t0) continue;
      if (v === null || v === undefined) {
        pen = false;
        continue;
      }
      const px = x(tt);
      if (pen) ctx.lineTo(px, y(v));
      else ctx.moveTo(px, y(v));
      pen = true;
    }
    ctx.stroke();
  }

  $effect(() => {
    void dash.liveVersion;
    void theme.resolved;
    void tone;
    void w;
    void h;
    draw();
  });
</script>

<div class="spark" bind:clientWidth={w} bind:clientHeight={h} aria-hidden="true">
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .spark {
    height: 26px;
    width: 100%;
    position: relative;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
