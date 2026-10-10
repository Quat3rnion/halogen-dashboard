// Columnar time buffer in the shape that uPlot reads. It is not reactive on
// purpose: the store bumps a version number after each change, and the charts
// redraw from these arrays. This keeps 1 Hz updates free of proxy overhead.

import {
  SERIES_FIELDS,
  type Column,
  type Sample,
  type Series,
  type SeriesField,
  type Status,
} from './types';

export interface Interval {
  from: number;
  to: number;
}

export class TimeBuffer {
  t: number[] = [];
  status: (Status | null)[] = [];
  cols: Record<SeriesField, Column>;
  step = 1000;
  source: 'raw' | 'rollup' = 'raw';

  constructor() {
    this.cols = TimeBuffer.emptyCols();
  }

  static emptyCols(): Record<SeriesField, Column> {
    const c = {} as Record<SeriesField, Column>;
    for (const f of SERIES_FIELDS) c[f] = [];
    return c;
  }

  get length(): number {
    return this.t.length;
  }

  lastT(): number | null {
    return this.t.length ? (this.t[this.t.length - 1] ?? null) : null;
  }

  clear(): void {
    this.t = [];
    this.status = [];
    this.cols = TimeBuffer.emptyCols();
  }

  /** Replace the content with a Series. Inserts null rows where points are missing, so lines break. */
  load(s: Series): void {
    this.clear();
    this.step = Math.max(1000, s.step || 1000);
    this.source = s.source;
    const n = s.t.length;
    for (let i = 0; i < n; i++) {
      const t = s.t[i];
      if (t === undefined) continue;
      this.gapBefore(t);
      this.t.push(t);
      this.status.push(s.status?.[i] ?? null);
      for (const f of SERIES_FIELDS) this.cols[f].push(s[f]?.[i] ?? null);
    }
  }

  /** Append one live Sample. `kvPool` converts kv_reserved to a share. */
  append(x: Sample, kvPool: number | null): boolean {
    const last = this.lastT();
    if (last !== null && x.t <= last) return false;
    this.gapBefore(x.t);
    this.t.push(x.t);
    this.status.push(x.status);
    const reservedUse = x.kv_reserved !== null && kvPool ? x.kv_reserved / kvPool : null;
    const row: Record<SeriesField, number | null> = {
      busy_slots: x.busy_slots,
      queued: x.queued,
      kv_use: x.kv_use,
      kv_reserved_use: reservedUse,
      gen_speed: x.gen_speed,
      throughput: x.throughput,
      prefill_speed: x.prefill_speed,
      prefill_throughput: x.prefill_throughput,
      requests_per_min: x.requests_per_min,
      mean_service_time: x.mean_service_time,
      prompt_cache_hit_rate: x.prompt_cache_hit_rate,
      draft_acceptance: x.draft_acceptance,
      busy_slots_max: x.busy_slots,
      queued_max: x.queued,
      kv_use_max: x.kv_use,
    };
    for (const f of SERIES_FIELDS) this.cols[f].push(row[f]);
    return true;
  }

  /** Drop points older than `minT`. Keeps memory flat in live mode. */
  trim(minT: number): void {
    let k = 0;
    const n = this.t.length;
    while (k < n && (this.t[k] ?? 0) < minT) k++;
    if (k === 0) return;
    this.t.splice(0, k);
    this.status.splice(0, k);
    for (const f of SERIES_FIELDS) this.cols[f].splice(0, k);
  }

  /** If the time since the last point is much more than one step, add a null row so the line breaks. */
  private gapBefore(t: number): void {
    const last = this.lastT();
    if (last === null) return;
    if (t - last > this.step * 2.5) {
      this.t.push(last + this.step);
      this.status.push(null);
      for (const f of SERIES_FIELDS) this.cols[f].push(null);
    }
  }

  /** Runs of one Status as time intervals. A run ends at the time of the next point. */
  runs(which: Status): Interval[] {
    const out: Interval[] = [];
    let start: number | null = null;
    const n = this.t.length;
    for (let i = 0; i < n; i++) {
      const s = this.status[i];
      const t = this.t[i] ?? 0;
      if (s === which) {
        if (start === null) start = t;
      } else if (start !== null) {
        out.push({ from: start, to: t });
        start = null;
      }
    }
    if (start !== null) out.push({ from: start, to: (this.lastT() ?? start) + this.step });
    return out;
  }
}

export function mergeIntervals(list: Interval[]): Interval[] {
  const s = [...list].sort((a, b) => a.from - b.from);
  const out: Interval[] = [];
  for (const iv of s) {
    const last = out[out.length - 1];
    if (last && iv.from <= last.to) last.to = Math.max(last.to, iv.to);
    else out.push({ ...iv });
  }
  return out;
}
