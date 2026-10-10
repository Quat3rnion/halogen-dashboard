// The dashboard state: one SSE connection for live values, and one series
// buffer for the selected time range.

import { fetchEvents, fetchSeries } from './api';
import { mergeIntervals, TimeBuffer, type Interval } from './buffer';
import type { CacheInfo, HEvent, Info, Sample, Series, Snapshot, Totals } from './types';

export type RangeId = 'live' | '15m' | '1h' | '6h' | '24h' | '7d' | '30d';

export interface RangeDef {
  id: RangeId;
  label: string;
  /** Accessible name. */
  name: string;
  ms: number;
  /** Re-fetch interval for historical ranges, ms. */
  refresh: number;
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

export const RANGES: RangeDef[] = [
  { id: 'live', label: 'Live 5m', name: 'Live, last 5 minutes', ms: 5 * MIN, refresh: 30_000 },
  { id: '15m', label: '15m', name: 'Last 15 minutes', ms: 15 * MIN, refresh: 5_000 },
  { id: '1h', label: '1h', name: 'Last hour', ms: HOUR, refresh: 10_000 },
  { id: '6h', label: '6h', name: 'Last 6 hours', ms: 6 * HOUR, refresh: 20_000 },
  { id: '24h', label: '24h', name: 'Last 24 hours', ms: DAY, refresh: 30_000 },
  { id: '7d', label: '7d', name: 'Last 7 days', ms: 7 * DAY, refresh: 60_000 },
  { id: '30d', label: '30d', name: 'Last 30 days', ms: 30 * DAY, refresh: 120_000 },
];

/** Window of the sparklines and of Live mode. */
const LIVE_MS = 5 * MIN;

export type Conn = 'connecting' | 'live' | 'reconnecting';

const TOTAL_KEYS: (keyof Totals)[] = [
  'requests',
  'prompt_tokens',
  'predicted_tokens',
  'cached_tokens',
  'draft_tokens',
  'draft_accepted',
  'structured_requests',
];

function addDelta(base: Totals, now: Totals | null, then: Totals | null): Totals {
  if (!now || !then) return base;
  const out = { ...base };
  for (const k of TOTAL_KEYS) out[k] = base[k] + Math.max(0, now[k] - then[k]);
  return out;
}

function readRange(): RangeId {
  const q = new URLSearchParams(location.search).get('range');
  return RANGES.some((r) => r.id === q) ? (q as RangeId) : 'live';
}

class Dashboard {
  info = $state.raw<Info | null>(null);
  cache = $state.raw<CacheInfo | null>(null);
  latest = $state.raw<Sample | null>(null);
  /** Newest events, oldest first, from the snapshot and the stream. */
  events = $state.raw<HEvent[]>([]);
  conn = $state<Conn>('connecting');
  /** Time of the last message on the stream. */
  lastMessage = $state(0);
  range = $state<RangeId>(readRange());
  /** Bumped when chart data changes. Charts and sparklines redraw on it. */
  version = $state(0);
  /** Bumped when the live 5-minute buffer changes. */
  liveVersion = $state(0);
  source = $state<'raw' | 'rollup'>('raw');
  rangeTotals = $state.raw<Totals | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  /** 1 Hz clock for ages and Uptime. */
  now = $state(Date.now());

  /** Last 5 minutes from the stream. Drives sparklines and Live mode charts. */
  readonly live = new TimeBuffer();
  /** Data of a historical range. */
  readonly hist = new TimeBuffer();
  /** Events of a historical range, oldest first. */
  histEvents = $state.raw<HEvent[]>([]);

  /** Current chart x domain. */
  xFrom = 0;
  xTo = 0;
  bands: { outage: Interval[]; degraded: Interval[] } = { outage: [], degraded: [] };
  restarts: number[] = [];

  private es: EventSource | null = null;
  private everConnected = false;
  private retryMs = 1000;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private refreshTimer: ReturnType<typeof setInterval> | undefined;
  private clockTimer: ReturnType<typeof setInterval> | undefined;
  private abort: AbortController | null = null;
  private liveAbort: AbortController | null = null;
  private liveLoading = false;
  private pending: Sample[] = [];
  private liveTotalsBase: Totals | null = null;
  private liveTotalsAt: Totals | null = null;
  private frame = 0;

  get rangeDef(): RangeDef {
    return RANGES.find((r) => r.id === this.range) ?? RANGES[0]!;
  }

  get isLive(): boolean {
    return this.range === 'live';
  }

  /** Events for the event log, newest first. */
  get logEvents(): HEvent[] {
    return [...(this.isLive ? this.events : this.histEvents)].reverse();
  }

  /** Buffer that the charts draw. */
  get chartBuf(): TimeBuffer {
    return this.isLive ? this.live : this.hist;
  }

  start(): void {
    this.connect();
    this.loadLive();
    this.applyRange();
    this.clockTimer = setInterval(() => {
      this.now = Date.now();
      // The stream sends a keepalive every 15 s. Silence for longer means a dead connection.
      if (this.conn === 'live' && this.now - this.lastMessage > 20_000) this.reconnect();
    }, 1000);
    document.addEventListener('visibilitychange', this.onVisible);
  }

  stop(): void {
    this.es?.close();
    clearTimeout(this.retryTimer);
    clearInterval(this.refreshTimer);
    clearInterval(this.clockTimer);
    document.removeEventListener('visibilitychange', this.onVisible);
  }

  setRange(id: RangeId): void {
    if (id === this.range) return;
    this.range = id;
    const u = new URL(location.href);
    if (id === 'live') u.searchParams.delete('range');
    else u.searchParams.set('range', id);
    history.replaceState(null, '', u);
    this.applyRange();
  }

  private onVisible = (): void => {
    if (document.visibilityState === 'visible') {
      if (!this.isLive) void this.loadHist();
      else this.scheduleDraw();
    }
  };

  // ---------------------------------------------------------------- stream

  private connect(): void {
    this.es?.close();
    const es = new EventSource('/api/stream');
    this.es = es;
    es.onopen = () => {
      this.conn = 'live';
      this.lastMessage = Date.now();
      this.retryMs = 1000;
      if (this.everConnected) {
        // Samples were lost while the stream was down. Fetch them again.
        void this.loadLive();
        if (!this.isLive) void this.loadHist();
      }
      this.everConnected = true;
    };
    es.onerror = () => {
      this.conn = this.everConnected ? 'reconnecting' : 'connecting';
      if (es.readyState === EventSource.CLOSED) this.scheduleRetry();
    };
    es.onmessage = () => {
      this.lastMessage = Date.now();
    };
    es.addEventListener('snapshot', (e) => this.onSnapshot(parse<Snapshot>(e)));
    es.addEventListener('sample', (e) => this.onSample(parse<Sample>(e)));
    es.addEventListener('info', (e) => {
      this.lastMessage = Date.now();
      const v = parse<Info>(e);
      if (v) this.info = v;
    });
    es.addEventListener('cache', (e) => {
      this.lastMessage = Date.now();
      const v = parse<CacheInfo>(e);
      if (v) this.cache = v;
    });
    es.addEventListener('event', (e) => {
      this.lastMessage = Date.now();
      const v = parse<HEvent>(e);
      if (v) this.addEvents([v]);
    });
  }

  private reconnect(): void {
    this.conn = 'reconnecting';
    this.es?.close();
    this.es = null;
    this.scheduleRetry();
  }

  private scheduleRetry(): void {
    clearTimeout(this.retryTimer);
    this.retryTimer = setTimeout(() => this.connect(), this.retryMs);
    this.retryMs = Math.min(this.retryMs * 2, 15_000);
  }

  private onSnapshot(s: Snapshot | null): void {
    this.lastMessage = Date.now();
    if (!s) return;
    this.info = s.info;
    this.cache = s.cache;
    this.events = [...s.events].sort((a, b) => a.t - b.t);
    if (s.latest) this.onSample(s.latest);
  }

  private onSample(x: Sample | null): void {
    this.lastMessage = Date.now();
    if (!x) return;
    this.latest = x;
    if (this.liveLoading) {
      this.pending.push(x);
      return;
    }
    if (this.live.append(x, this.info?.kv_pool ?? null)) {
      this.live.trim(x.t - LIVE_MS - this.live.step);
      this.updateLiveTotals();
      this.scheduleDraw();
    }
  }

  private addEvents(list: HEvent[]): void {
    const all = [...this.events, ...list].sort((a, b) => a.t - b.t);
    this.events = all.slice(-200);
    this.scheduleDraw();
  }

  // ---------------------------------------------------------------- data

  /** Fetch the last 5 minutes for the live buffer, then add Samples that came during the fetch. */
  private async loadLive(): Promise<void> {
    this.liveAbort?.abort();
    const ac = new AbortController();
    this.liveAbort = ac;
    this.liveLoading = true;
    const to = Date.now();
    try {
      const s = await fetchSeries(to - LIVE_MS, to, 1000, ac.signal);
      if (ac.signal.aborted) return;
      this.live.load(s);
      this.liveTotalsBase = s.totals_in_range;
      this.liveTotalsAt = this.latest?.totals ?? null;
      if (this.isLive) {
        this.source = s.source;
        this.error = null;
      }
    } catch (err) {
      if (ac.signal.aborted) return;
      if (this.isLive) this.error = errText(err);
    } finally {
      if (this.liveAbort === ac) {
        this.liveLoading = false;
        const kv = this.info?.kv_pool ?? null;
        for (const p of this.pending) this.live.append(p, kv);
        this.pending = [];
        const last = this.live.lastT();
        if (last !== null) this.live.trim(last - LIVE_MS - this.live.step);
        this.updateLiveTotals();
        this.scheduleDraw();
      }
    }
  }

  private updateLiveTotals(): void {
    if (!this.isLive || !this.liveTotalsBase) return;
    this.rangeTotals = addDelta(this.liveTotalsBase, this.latest?.totals ?? null, this.liveTotalsAt);
  }

  private async loadHist(): Promise<void> {
    const def = this.rangeDef;
    if (def.id === 'live') return;
    this.abort?.abort();
    const ac = new AbortController();
    this.abort = ac;
    this.loading = true;
    const to = Date.now();
    const from = to - def.ms;
    try {
      const [s, ev] = await Promise.all([
        fetchSeries(from, to, undefined, ac.signal),
        fetchEvents(from, to, ac.signal),
      ]);
      if (ac.signal.aborted || this.range !== def.id) return;
      this.hist.load(s as Series);
      this.histEvents = [...ev].sort((a, b) => a.t - b.t);
      this.source = s.source;
      this.rangeTotals = s.totals_in_range;
      this.xFrom = s.from || from;
      this.xTo = s.to || to;
      this.error = null;
      this.scheduleDraw();
    } catch (err) {
      if (!ac.signal.aborted) this.error = errText(err);
    } finally {
      if (this.abort === ac) this.loading = false;
    }
  }

  private applyRange(): void {
    clearInterval(this.refreshTimer);
    this.abort?.abort();
    this.error = null;
    if (this.isLive) {
      this.loading = false;
      this.source = this.live.source;
      this.rangeTotals = null;
      this.updateLiveTotals();
      // Resynchronise the live buffer and totals now and then.
      this.refreshTimer = setInterval(() => {
        if (document.visibilityState === 'visible' && this.conn === 'live') void this.loadLive();
      }, this.rangeDef.refresh);
      this.scheduleDraw();
    } else {
      this.hist.clear();
      this.histEvents = [];
      this.rangeTotals = null;
      this.scheduleDraw();
      void this.loadHist();
      this.refreshTimer = setInterval(() => {
        if (document.visibilityState === 'visible') void this.loadHist();
      }, this.rangeDef.refresh);
    }
  }

  // ---------------------------------------------------------------- draw

  /** Coalesce redraws to one per animation frame. */
  private scheduleDraw(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.prepare();
      this.liveVersion++;
      this.version++;
    });
  }

  private prepare(): void {
    const buf = this.chartBuf;
    if (this.isLive) {
      const end = this.live.lastT() ?? Date.now();
      this.xTo = end;
      this.xFrom = end - LIVE_MS;
    }
    const events = this.isLive ? this.events : this.histEvents;
    const outage = buf.runs('down');
    let open: number | null = null;
    for (const e of events) {
      if (e.kind === 'outage_start') open = e.t;
      else if (e.kind === 'outage_end') {
        outage.push({ from: open ?? this.xFrom, to: e.t });
        open = null;
      }
    }
    if (open !== null && this.latest?.status === 'down') outage.push({ from: open, to: this.xTo });
    this.bands = { outage: mergeIntervals(outage), degraded: buf.runs('degraded') };
    this.restarts = events
      .filter((e) => e.kind === 'restart' && e.t >= this.xFrom && e.t <= this.xTo)
      .map((e) => e.t);
  }
}

function parse<T>(e: Event): T | null {
  try {
    return JSON.parse((e as MessageEvent<string>).data) as T;
  } catch {
    return null;
  }
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const dash = new Dashboard();
