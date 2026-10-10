<script lang="ts">
  import uPlot from 'uplot';
  import { onDestroy } from 'svelte';
  import type { ChartDef, LineDef } from '../lib/charts';
  import type { SeriesField } from '../lib/types';
  import { alpha } from '../lib/color';
  import { clockTime, absTime } from '../lib/format';
  import { dash } from '../lib/store.svelte';
  import { cssVar, theme } from '../lib/theme.svelte';

  let { def }: { def: ChartDef } = $props();

  let plotEl: HTMLDivElement;
  let u: uPlot | null = null;
  let width = 0;
  let height = 0;
  /** Cursor index, or null for the latest value. */
  let idx = $state<number | null>(null);

  interface Drawn {
    line: LineDef;
    max: boolean;
  }
  let drawn: Drawn[] = [];

  function lines(rollup: boolean): Drawn[] {
    const out: Drawn[] = [];
    for (const l of def.lines) {
      out.push({ line: l, max: false });
      if (rollup && l.maxOf) out.push({ line: l, max: true });
    }
    return out;
  }

  function data(): uPlot.AlignedData {
    const b = dash.chartBuf;
    const cols = drawn.map((d) => b.cols[d.max && d.line.maxOf ? d.line.maxOf : d.line.field]);
    return [b.t, ...cols] as uPlot.AlignedData;
  }

  const hm = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
  const md = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' });

  /** One-line time tick. Shows the date at midnight and for steps of a day or more. */
  function xTick(v: number, incr: number): string {
    const d = new Date(v);
    if (incr >= 86_400_000) return md.format(d);
    if (incr >= 60_000 && d.getHours() === 0 && d.getMinutes() === 0) return md.format(d);
    if (incr < 60_000) {
      const sec = d.getSeconds();
      return sec === 0 ? hm.format(d) : `:${String(sec).padStart(2, '0')}`;
    }
    return hm.format(d);
  }

  function niceCeil(v: number): number {
    if (v <= 0) return 1;
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    const n = v / p;
    const m = n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10;
    return m * p;
  }

  function build(): void {
    u?.destroy();
    u = null;
    if (!plotEl || width < 10 || height < 10) return;
    const rollup = dash.source === 'rollup';
    drawn = lines(rollup);
    const font = `12px ${cssVar('--font') || 'sans-serif'}`;
    const faint = cssVar('--faint');
    const grid = cssVar('--grid');
    const line = cssVar('--line');

    const series: uPlot.Series[] = [{}];
    const bands: uPlot.Band[] = [];
    drawn.forEach((d, i) => {
      if (!d.max) return;
      const meanIdx = drawn.findIndex((o) => !o.max && o.line.field === d.line.field);
      if (meanIdx >= 0) bands.push({ series: [i + 1, meanIdx + 1], fill: alpha(cssVar(d.line.color), 0.2) });
    });
    for (const d of drawn) {
      const color = cssVar(d.line.color);
      const s: uPlot.Series = {
        label: d.max ? `${d.line.label} max` : d.line.label,
        scale: d.line.scale,
        stroke: d.max ? alpha(color, 0.35) : color,
        width: d.max ? 1 : 1.6,
        spanGaps: false,
        points: { show: false },
      };
      if (d.line.stepped) s.paths = uPlot.paths.stepped!({ align: 1 });
      if (d.line.fill && !d.max && !rollup) {
        s.fill = (pl: uPlot) => {
          const g = pl.ctx.createLinearGradient(0, pl.bbox.top, 0, pl.bbox.top + pl.bbox.height);
          g.addColorStop(0, alpha(color, 0.22));
          g.addColorStop(1, alpha(color, 0.01));
          return g;
        };
      }
      series.push(s);
    }

    const scales: uPlot.Scales = {
      x: { time: true, range: () => [dash.xFrom, dash.xTo] },
    };
    const axes: uPlot.Axis[] = [
      {
        stroke: faint,
        font,
        size: 26,
        gap: 4,
        space: 52,
        grid: { stroke: grid, width: 1 },
        ticks: { stroke: line, width: 1, size: 4 },
        values: (_u, splits, _ax, _space, incr) => splits.map((v) => xTick(v, incr)),
      },
    ];
    for (const a of def.axes) {
      scales[a.scale] = {
        range: (_u, _min, max) => {
          if (a.range) return a.range;
          let hi = Math.max(a.minMax ?? 0, Number.isFinite(max) ? max : 0);
          if (a.unit === '%') hi = Math.ceil(hi * 1.04 * 10) / 10;
          else hi = a.integer ? Math.max(1, Math.ceil(hi * 1.1)) : niceCeil(hi * 1.08);
          return [0, hi];
        },
      };
      axes.push({
        scale: a.scale,
        side: a.side === 'left' ? 3 : 1,
        stroke: faint,
        font,
        size: 50,
        gap: 4,
        grid: a.side === 'left' ? { stroke: grid, width: 1 } : { show: false },
        ticks: { show: false },
        values: (_u, splits) => splits.map((v) => a.tick(v)),
        incrs: a.integer ? [1, 2, 5, 10, 20, 50, 100] : undefined,
      });
    }

    const opts: uPlot.Options = {
      width,
      height,
      ms: 1,
      pxAlign: 0,
      legend: { show: false },
      padding: [8, def.axes.some((a) => a.side === 'right') ? 4 : 14, 0, 0],
      cursor: {
        sync: { key: 'halogen', setSeries: false },
        y: false,
        drag: { x: false, y: false, setScale: false },
        points: { size: 6, width: 1.5 },
        focus: { prox: -1 },
      },
      scales,
      axes,
      series,
      bands,
      hooks: {
        drawClear: [drawBands],
        draw: [drawRestarts],
        setCursor: [
          (pl) => {
            const i = pl.cursor.idx;
            idx = i === undefined || i === null ? null : i;
          },
        ],
      },
    };
    u = new uPlot(opts, data(), plotEl);
  }

  function drawBands(pl: uPlot): void {
    const { left, top, width: w, height: h } = pl.bbox;
    const ctx = pl.ctx;
    ctx.save();
    ctx.beginPath();
    ctx.rect(left, top, w, h);
    ctx.clip();
    const paint = (list: { from: number; to: number }[], color: string) => {
      ctx.fillStyle = color;
      for (const b of list) {
        const x0 = pl.valToPos(b.from, 'x', true);
        const x1 = pl.valToPos(b.to, 'x', true);
        if (x1 < left || x0 > left + w) continue;
        ctx.fillRect(x0, top, Math.max(3 * (devicePixelRatio || 1), x1 - x0), h);
      }
    };
    paint(dash.bands.degraded, cssVar('--band-degraded'));
    paint(dash.bands.outage, cssVar('--band-outage'));
    ctx.restore();
  }

  function drawRestarts(pl: uPlot): void {
    if (!dash.restarts.length) return;
    const { left, top, width: w, height: h } = pl.bbox;
    const ctx = pl.ctx;
    const dpr = devicePixelRatio || 1;
    ctx.save();
    ctx.strokeStyle = cssVar('--marker');
    ctx.fillStyle = cssVar('--marker');
    ctx.lineWidth = dpr;
    ctx.setLineDash([4 * dpr, 3 * dpr]);
    for (const t of dash.restarts) {
      const x = Math.round(pl.valToPos(t, 'x', true)) + 0.5;
      if (x < left || x > left + w) continue;
      ctx.beginPath();
      ctx.moveTo(x, top);
      ctx.lineTo(x, top + h);
      ctx.stroke();
      ctx.beginPath();
      const s = 4 * dpr;
      ctx.moveTo(x - s, top);
      ctx.lineTo(x + s, top);
      ctx.lineTo(x, top + s * 1.4);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }

  // Rebuild on theme change, on raw/rollup change, and on first size.
  $effect(() => {
    void theme.resolved;
    void dash.source;
    build();
  });

  // Redraw on new data.
  $effect(() => {
    void dash.version;
    if (u) u.setData(data(), true);
  });

  $effect(() => {
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r) return;
      const w = Math.floor(r.width);
      const h = Math.floor(r.height);
      if (w === width && h === height) return;
      width = w;
      height = h;
      if (u) u.setSize({ width, height });
      else build();
    });
    ro.observe(plotEl);
    document.fonts?.ready.then(() => u?.redraw(false, true));
    return () => ro.disconnect();
  });

  onDestroy(() => u?.destroy());

  function valueAt(f: SeriesField, i: number | null): number | null {
    void dash.version;
    const b = dash.chartBuf;
    const col = b.cols[f];
    // Without a cursor, show the newest row. It is null during an Outage, so the legend does not show stale values.
    return col[i ?? col.length - 1] ?? null;
  }

  const rollup = $derived(dash.source === 'rollup');

  const cursorTime = $derived.by(() => {
    void dash.version;
    if (idx === null) return null;
    return dash.chartBuf.t[idx] ?? null;
  });

  const hasData = $derived.by(() => {
    void dash.version;
    const b = dash.chartBuf;
    return def.lines.some((l) => b.cols[l.field].some((v) => v !== null));
  });

  const longRange = $derived(dash.rangeDef.ms > 24 * 3600_000);
</script>

<figure class="chart card" aria-labelledby="title-{def.id}">
  <figcaption class="head">
    <h2 id="title-{def.id}">{def.title}</h2>
    <span class="when num">
      {#if cursorTime !== null}
        {longRange ? absTime(cursorTime) : clockTime(cursorTime)}
      {:else}
        Latest
      {/if}
    </span>
  </figcaption>
  <ul class="legend">
    {#each def.lines as l (l.field)}
      <li>
        <span class="sw" style="--c: var({l.color})" aria-hidden="true"></span>
        <span class="lbl">{l.label}</span>
        <span class="val num">{l.fmt(valueAt(l.field, idx))}</span>
        {#if rollup && l.maxOf}
          <span class="mx num"><span class="sw band" style="--c: var({l.color})" aria-hidden="true"></span>max {l.fmt(valueAt(l.maxOf, idx))}</span>
        {/if}
      </li>
    {/each}
  </ul>
  <div class="plot" bind:this={plotEl} aria-hidden="true"></div>
  {#if !hasData}
    <p class="empty">
      {#if dash.loading}Loading data for {dash.rangeDef.name.toLowerCase()}…{:else if dash.isLive}Waiting for Samples from the Collector…{:else}No Samples in this range. Select a shorter range or check the Collector.{/if}
    </p>
  {/if}
</figure>

<style>
  .chart {
    position: relative;
    margin: 0;
    padding: var(--sp-3) var(--sp-3) var(--sp-2);
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: 0 var(--sp-1);
  }
  h2 {
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .when {
    font-size: var(--fs-xs);
    color: var(--faint);
    white-space: nowrap;
  }
  .legend {
    list-style: none;
    margin: var(--sp-1) 0 var(--sp-2);
    padding: 0 var(--sp-1);
    display: flex;
    flex-wrap: wrap;
    gap: 2px var(--sp-4);
    font-size: var(--fs-xs);
    min-height: 1.2rem;
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  .mx {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }
  .sw {
    width: 12px;
    height: 3px;
    border-radius: 2px;
    background: var(--c);
    flex: none;
  }
  .sw.band {
    width: 8px;
    height: 8px;
    opacity: 0.4;
  }
  .val {
    color: var(--text);
    font-weight: 500;
    min-width: 6ch;
  }
  .plot {
    height: var(--plot-h, 210px);
    width: 100%;
    min-width: 0;
  }
  .empty {
    position: absolute;
    inset: 64px var(--sp-4) var(--sp-5);
    display: grid;
    place-items: center;
    text-align: center;
    color: var(--muted);
    font-size: var(--fs-sm);
    pointer-events: none;
  }
</style>
