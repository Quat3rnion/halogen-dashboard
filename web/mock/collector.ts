// Dev-only mock of the Collector API (docs/collector-api.md). It is loaded by
// vite.config.ts in `--mode mock` only, and is never part of `vite build`.
//
// Values are a deterministic function of time, so every range query agrees
// with the live stream. The timeline holds Outages, Restarts and Degraded
// periods: some in the past (for 24h/7d/30d) and some just before the mock
// starts (for Live 5m), plus a short Outage every 10 minutes after start.
//
// Extra dev endpoints:
//   GET /__mock/drop      closes all streams and refuses new ones for 6 s ("Reconnecting").
//   GET /__mock/outage    starts a 20 s Outage now, followed by a Restart.
//   GET /__mock/degraded  starts a 30 s Degraded period now (NPU error).

import type { IncomingMessage, ServerResponse } from 'node:http';

type Status = 'up' | 'degraded' | 'down';
type EventKind = 'restart' | 'outage_start' | 'outage_end' | 'status_change';
interface HEvent {
  t: number;
  kind: EventKind;
  from?: Status;
  to?: Status;
  detail?: string;
}
interface Totals {
  requests: number;
  prompt_tokens: number;
  predicted_tokens: number;
  cached_tokens: number;
  draft_tokens: number;
  draft_accepted: number;
  structured_requests: number;
}
type Next = (err?: unknown) => void;

const SEC = 1000;
const MIN = 60 * SEC;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

const SLOTS = 4;
const KV_POOL = 1_048_576;
const NPU_MODELS = ['decider-0.8b', 'qwen3-embedding-0.6b', 'qwen3-reranker-0.6b', 'qwen3.5-2b', 'qwen3guard-gen-0.6b'];

const START = Math.floor(Date.now() / SEC) * SEC;
const ORIGIN = START - 31 * DAY;

interface Span {
  from: number;
  to: number;
  detail?: string;
}

// Past and near-start timeline, relative to START.
const OUTAGES: Span[] = [
  { from: START - 6 * DAY - 3 * HOUR, to: START - 6 * DAY - 2 * HOUR, detail: 'connect: connection refused' },
  { from: START - 26 * HOUR, to: START - 26 * HOUR + 12 * MIN, detail: '/metrics timed out after 2 s' },
  { from: START - 3 * HOUR, to: START - 3 * HOUR + 4 * MIN, detail: 'connect: connection refused' },
  { from: START - 95 * SEC, to: START - 62 * SEC, detail: 'connect: connection refused' },
];
const RESTARTS: number[] = [START - 13 * DAY, START - 6 * DAY - 2 * HOUR + 20 * SEC, START - 3 * HOUR + 4 * MIN + 2 * SEC, START - 60 * SEC];
const DEGRADED: Span[] = [
  { from: START - 2 * DAY - 5 * HOUR, to: START - 2 * DAY - 3 * HOUR, detail: 'NPU error: context pool exhausted' },
  { from: START - 9 * HOUR, to: START - 9 * HOUR + 40 * MIN, detail: 'engine does not respond to the probe' },
  { from: START - 200 * SEC, to: START - 140 * SEC, detail: 'NPU error: rknn_run timed out' },
];

// A 20 s Outage plus Restart every 10 minutes after start, and a 45 s Degraded period.
function periodic(k: number): { outage: Span; restart: number; degraded: Span } {
  const base = START + k * 10 * MIN;
  return {
    degraded: { from: base + 4 * MIN, to: base + 4 * MIN + 45 * SEC, detail: 'NPU error: rknn_run timed out' },
    outage: { from: base + 7 * MIN, to: base + 7 * MIN + 20 * SEC, detail: 'connect: connection refused' },
    restart: base + 7 * MIN + 22 * SEC,
  };
}

function inSpans(t: number, list: Span[]): Span | null {
  for (const s of list) if (t >= s.from && t < s.to) return s;
  return null;
}

function outageAt(t: number): Span | null {
  const hit = inSpans(t, OUTAGES);
  if (hit) return hit;
  if (t < START) return null;
  const p = periodic(Math.floor((t - START) / (10 * MIN)));
  return t >= p.outage.from && t < p.outage.to ? p.outage : null;
}

function degradedAt(t: number): Span | null {
  const hit = inSpans(t, DEGRADED);
  if (hit) return hit;
  if (t < START) return null;
  const p = periodic(Math.floor((t - START) / (10 * MIN)));
  return t >= p.degraded.from && t < p.degraded.to ? p.degraded : null;
}

function statusAt(t: number): Status {
  if (outageAt(t)) return 'down';
  if (degradedAt(t)) return 'degraded';
  return 'up';
}

function lastRestartBefore(t: number): number | null {
  let best: number | null = null;
  const all = [...RESTARTS];
  if (t >= START) {
    const n = Math.floor((t - START) / (10 * MIN));
    for (let k = 0; k <= n; k++) all.push(periodic(k).restart);
  }
  for (const r of all) if (r <= t && (best === null || r > best)) best = r;
  return best;
}

function lastOutageEndBefore(t: number): number | null {
  let best: number | null = null;
  const all = [...OUTAGES];
  if (t >= START) {
    const n = Math.floor((t - START) / (10 * MIN));
    for (let k = 0; k <= n; k++) all.push(periodic(k).outage);
  }
  for (const o of all) if (o.to <= t && (best === null || o.to > best)) best = o.to;
  return best;
}

function eventsBetween(from: number, to: number): HEvent[] {
  const ev: HEvent[] = [];
  const outages = [...OUTAGES];
  const degraded = [...DEGRADED];
  const restarts = [...RESTARTS];
  if (to >= START) {
    const n = Math.floor((to - START) / (10 * MIN));
    for (let k = 0; k <= n; k++) {
      const p = periodic(k);
      outages.push(p.outage);
      degraded.push(p.degraded);
      restarts.push(p.restart);
    }
  }
  for (const o of outages) {
    ev.push({ t: o.from, kind: 'outage_start', detail: o.detail });
    ev.push({ t: o.from, kind: 'status_change', from: 'up', to: 'down' });
    ev.push({ t: o.to, kind: 'outage_end' });
    ev.push({ t: o.to, kind: 'status_change', from: 'down', to: 'up' });
  }
  for (const d of degraded) {
    ev.push({ t: d.from, kind: 'status_change', from: 'up', to: 'degraded', detail: d.detail });
    ev.push({ t: d.to, kind: 'status_change', from: 'degraded', to: 'up' });
  }
  for (const r of restarts) ev.push({ t: r, kind: 'restart', detail: 'Counters went down' });
  return ev.filter((e) => e.t >= from && e.t <= to).sort((a, b) => a.t - b.t);
}

// ------------------------------------------------------------------ values

function hash(n: number): number {
  let x = Math.imul(n ^ 0x9e3779b9, 0x85ebca6b);
  x ^= x >>> 13;
  x = Math.imul(x, 0xc2b2ae35);
  x ^= x >>> 16;
  return (x >>> 0) / 4294967296;
}

/** Smooth noise in 0..1, period about `scale` ms. */
function smooth(t: number, scale: number, seed: number): number {
  const x = t / scale;
  const i = Math.floor(x);
  const f = x - i;
  const a = hash(i * 7919 + seed);
  const b = hash((i + 1) * 7919 + seed);
  const u = f * f * (3 - 2 * f);
  return a + (b - a) * u;
}

interface Values {
  busy_slots: number;
  queued: number;
  kv_used: number;
  kv_reserved: number;
  kv_use: number;
  gen_speed: number | null;
  throughput: number;
  prefill_speed: number | null;
  prefill_throughput: number;
  requests_per_min: number;
  mean_service_time: number | null;
  prompt_cache_hit_rate: number | null;
  draft_acceptance: number | null;
}

function load(t: number): number {
  const day = new Date(t).getHours() + new Date(t).getMinutes() / 60;
  const diurnal = 0.55 + 0.45 * Math.sin(((day - 9) / 24) * 2 * Math.PI);
  const slow = smooth(t, 9 * MIN, 1);
  const mid = smooth(t, 70 * SEC, 2);
  const fast = smooth(t, 9 * SEC, 3);
  // A 50 s burst every 10 minutes after start: all Slots busy, requests queued, KV pool near full.
  const burst = t >= START && (t - START) % (10 * MIN) >= 1 * MIN && (t - START) % (10 * MIN) < 1 * MIN + 50 * SEC ? 4.5 : 0;
  return Math.max(0, diurnal * 2.6 + slow * 1.7 + mid * 1.3 + fast * 0.5 - 1.1 + burst);
}

function values(t: number): Values {
  const raw = load(t);
  const busy = Math.min(SLOTS, Math.floor(raw));
  const queued = raw > SLOTS + 0.25 ? Math.floor((raw - SLOTS - 0.25) * 3) : 0;
  const n1 = smooth(t, 6 * SEC, 11);
  const n2 = smooth(t, 13 * SEC, 12);
  const n3 = smooth(t, 40 * SEC, 13);
  const gen = busy > 0 ? 46 - busy * 3.2 + (n1 - 0.5) * 6 : null;
  const throughput = gen === null ? 0 : gen * busy * (0.86 + n2 * 0.1);
  const prefill = busy > 0 || n3 > 0.4 ? 2350 - busy * 160 + (n2 - 0.5) * 500 : null;
  const prefillTp = prefill === null ? 0 : prefill * (0.05 + busy * 0.07) * (0.6 + n1 * 0.8);
  const kvUse = Math.min(0.985, 0.06 + busy * 0.17 + queued * 0.035 + n3 * 0.12);
  const kvUsed = Math.round(kvUse * KV_POOL);
  const rpm = busy * 1.4 + n2 * 1.8 + (queued > 0 ? 0.6 : 0);
  return {
    busy_slots: busy,
    queued,
    kv_used: kvUsed,
    kv_reserved: Math.min(KV_POOL * 1.2, Math.round(kvUsed * (1.12 + n1 * 0.15) + busy * 8192)),
    kv_use: kvUsed / KV_POOL,
    gen_speed: gen,
    throughput,
    prefill_speed: prefill,
    prefill_throughput: prefillTp,
    requests_per_min: rpm,
    mean_service_time: rpm > 0.2 ? 18 + busy * 7 + n3 * 22 : null,
    prompt_cache_hit_rate: prefill === null ? null : 0.32 + n3 * 0.42,
    draft_acceptance: gen === null ? null : 0.62 + n2 * 0.2,
  };
}

/** Adjusted counters: monotonic, closed form. */
function totals(t: number): Totals {
  const s = Math.max(0, (t - ORIGIN) / SEC);
  const w = Math.sin(s / 517) * 400;
  const requests = Math.floor(s * 0.034 + w * 0.01);
  const predicted = Math.floor(s * 86 + w * 30);
  const prompt = Math.floor(s * 410 + w * 120);
  const cached = Math.floor(s * 205 + w * 40);
  const draft = Math.floor(s * 70 + w * 20);
  return {
    requests,
    prompt_tokens: prompt,
    predicted_tokens: predicted,
    cached_tokens: cached,
    draft_tokens: draft,
    draft_accepted: Math.floor(draft * 0.71),
    structured_requests: Math.floor(requests * 0.08),
  };
}

function totalsDelta(a: Totals, b: Totals): Totals {
  const out = { ...b };
  for (const k of Object.keys(b) as (keyof Totals)[]) out[k] = Math.max(0, b[k] - a[k]);
  return out;
}

function sample(t: number) {
  const status = statusAt(t);
  if (status === 'down') {
    return {
      t,
      status,
      busy_slots: null,
      queued: null,
      kv_used: null,
      kv_reserved: null,
      kv_use: null,
      gen_speed: null,
      throughput: null,
      prefill_speed: null,
      prefill_throughput: null,
      requests_per_min: null,
      mean_service_time: null,
      prompt_cache_hit_rate: null,
      draft_acceptance: null,
      totals: null,
    };
  }
  return { t, status, ...values(t), totals: totals(t) };
}

function info(now: number) {
  const d = degradedAt(now);
  const down = outageAt(now) !== null;
  const npuBad = d?.detail?.startsWith('NPU') ?? false;
  const engineBad = d !== null && !npuBad;
  const restart = lastRestartBefore(now);
  const outEnd = lastOutageEndBefore(now);
  let lastOk = Math.floor(now / SEC) * SEC;
  if (down) lastOk = (outageAt(now)?.from ?? now) - SEC;
  const loadedCount = Math.floor(smooth(now, 2 * MIN, 21) * 3);
  return {
    halogen_url: 'http://frame:8731',
    model: 'halogen-v2-abliterated',
    version_api: '0.17.3',
    version_engine: '0.17.3',
    slots: SLOTS,
    slot_ctx: 262144,
    kv_pool: KV_POOL,
    drafter_default: 'mtp',
    prompt_cache_cap_mb: 110,
    engine_responds: !engineBad,
    engine_probe_s: engineBad ? 3.0 : 0.4,
    npu: {
      status: npuBad ? 'error' : 'ok',
      models: NPU_MODELS,
      loaded: NPU_MODELS.slice(0, loadedCount),
      queued: npuBad ? 3 : 0,
      passes: Math.floor((now - ORIGIN) / 9000),
      contexts: 4,
      ...(npuBad ? { error: d?.detail?.replace(/^NPU error: /, '') } : {}),
    },
    uptime_since: Math.max(restart ?? ORIGIN, outEnd ?? ORIGIN),
    last_scrape_ok: lastOk,
  };
}

function cache(now: number) {
  const t = Math.floor(now / (5 * SEC)) * 5 * SEC;
  const v = values(t);
  const s = (t - ORIGIN) / SEC;
  const entries = Math.min(24, 3 + Math.floor(smooth(t, 4 * MIN, 31) * 22));
  const hits = Math.floor(s * 0.021);
  const misses = Math.floor(s * 0.013);
  return {
    t,
    entries,
    max_entries: 24,
    bytes: Math.round(entries * 4.6 * 1024 * 1024 * (0.9 + smooth(t, 30 * SEC, 32) * 0.15)),
    hits,
    misses,
    evicted: Math.floor(s * 0.004),
    refused: Math.floor(s * 0.0003),
    hit_rate: hits / (hits + misses),
    token_hit_rate: 0.47,
    prompt_tokens_saved: Math.floor(s * 205),
    pool: {
      positions: KV_POOL,
      used: v.kv_used,
      busy_regions: v.busy_slots,
      held_regions: Math.max(1, entries - 1),
      waiting_for_room: v.kv_use > 0.9 ? v.queued : 0,
      relocated: Math.floor(s * 0.002),
    },
  };
}

// ------------------------------------------------------------------ series

const FIELDS = [
  'busy_slots',
  'queued',
  'kv_use',
  'kv_reserved_use',
  'gen_speed',
  'throughput',
  'prefill_speed',
  'prefill_throughput',
  'requests_per_min',
  'mean_service_time',
  'prompt_cache_hit_rate',
  'draft_acceptance',
] as const;
const RANK: Record<Status, number> = { up: 0, degraded: 1, down: 2 };

function series(from: number, to: number, stepIn: number | null) {
  let step = stepIn ?? Math.ceil((to - from) / 1000);
  step = Math.max(SEC, step);
  const raw = from >= Date.now() - 48 * HOUR && step < MIN;
  if (raw) step = Math.ceil(step / SEC) * SEC;
  else step = Math.max(MIN, Math.ceil(step / MIN) * MIN);
  const start = Math.ceil(Math.max(from, ORIGIN) / step) * step;
  const end = Math.min(to, Date.now());
  const cols: Record<string, (number | null)[]> = {};
  for (const f of FIELDS) cols[f] = [];
  cols.busy_slots_max = [];
  cols.queued_max = [];
  cols.kv_use_max = [];
  const ts: number[] = [];
  const status: (Status | null)[] = [];
  const subN = raw ? 1 : Math.min(40, Math.max(4, Math.round(step / (3 * SEC))));
  for (let t = start; t <= end; t += step) {
    ts.push(t);
    let worst: Status = 'up';
    const acc: Record<string, { sum: number; n: number; max: number }> = {};
    for (let j = 0; j < subN; j++) {
      const tt = Math.floor((t + (j * step) / subN) / SEC) * SEC;
      const st = statusAt(tt);
      if (RANK[st] > RANK[worst]) worst = st;
      if (st === 'down') continue;
      const v = values(tt);
      const row: Record<string, number | null> = { ...v, kv_reserved_use: v.kv_reserved / KV_POOL };
      for (const f of FIELDS) {
        const x = row[f];
        if (x === null || x === undefined) continue;
        const a = (acc[f] ??= { sum: 0, n: 0, max: -Infinity });
        a.sum += x;
        a.n++;
        a.max = Math.max(a.max, x);
      }
    }
    status.push(worst);
    const allDown = worst === 'down' && !acc.busy_slots;
    for (const f of FIELDS) {
      const a = acc[f];
      cols[f]!.push(allDown || !a ? null : a.sum / a.n);
    }
    cols.busy_slots_max!.push(acc.busy_slots ? acc.busy_slots.max : null);
    cols.queued_max!.push(acc.queued ? acc.queued.max : null);
    cols.kv_use_max!.push(acc.kv_use ? acc.kv_use.max : null);
  }
  return {
    from,
    to,
    step,
    source: raw ? 'raw' : 'rollup',
    t: ts,
    ...cols,
    status,
    totals_in_range: totalsDelta(totals(Math.max(from, ORIGIN)), totals(end)),
  };
}

function snapshot(now: number) {
  const latestT = Math.floor(now / SEC) * SEC;
  return {
    info: info(now),
    cache: cache(now),
    latest: sample(latestT),
    events: eventsBetween(ORIGIN, now).slice(-100),
  };
}

// ------------------------------------------------------------------ http

const streams = new Set<ServerResponse>();
let refuseUntil = 0;

function json(res: ServerResponse, body: unknown): void {
  res.statusCode = 200;
  res.setHeader('content-type', 'application/json');
  res.setHeader('cache-control', 'no-store');
  res.end(JSON.stringify(body));
}

function num(q: URLSearchParams, k: string): number | null {
  const v = q.get(k);
  if (v === null || v === '') return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

function send(res: ServerResponse, event: string, data: unknown): void {
  res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
}

function stream(req: IncomingMessage, res: ServerResponse): void {
  if (Date.now() < refuseUntil) {
    res.statusCode = 503;
    res.end('mock: stream refused');
    return;
  }
  res.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-store',
    connection: 'keep-alive',
  });
  res.write('retry: 2000\n\n');
  send(res, 'snapshot', snapshot(Date.now()));
  streams.add(res);
  let lastT = Math.floor(Date.now() / SEC) * SEC;
  let lastInfo = JSON.stringify(info(lastT));
  let ticks = 0;
  const timer = setInterval(() => {
    const now = Date.now();
    const t = Math.floor(now / SEC) * SEC;
    if (t <= lastT) return;
    for (const e of eventsBetween(lastT + 1, t)) send(res, 'event', e);
    lastT = t;
    const s = sample(t);
    send(res, 'sample', s);
    const inf = JSON.stringify(info(t));
    if (inf !== lastInfo) {
      res.write(`event: info\ndata: ${inf}\n\n`);
      lastInfo = inf;
    }
    if (s.status !== 'down' && t % (5 * SEC) === 0) send(res, 'cache', cache(t));
    if (++ticks % 15 === 0) res.write(': keepalive\n\n');
  }, 250);
  const done = () => {
    clearInterval(timer);
    streams.delete(res);
  };
  req.on('close', done);
  res.on('close', done);
}

export function createMockHandler() {
  console.log('[mock] Collector mock active: /api/snapshot /api/series /api/events /api/stream /metrics /healthz /__mock/drop /__mock/outage /__mock/degraded');
  return (req: IncomingMessage, res: ServerResponse, next: Next): void => {
    const url = new URL(req.url ?? '/', 'http://mock');
    const p = url.pathname;
    const now = Date.now();
    if (p === '/api/snapshot') return json(res, snapshot(now));
    if (p === '/api/series') {
      const to = num(url.searchParams, 'to') ?? now;
      const from = num(url.searchParams, 'from') ?? to - 5 * MIN;
      return json(res, series(from, to, num(url.searchParams, 'step')));
    }
    if (p === '/api/events') {
      const to = num(url.searchParams, 'to') ?? now;
      const from = num(url.searchParams, 'from') ?? to - DAY;
      return json(res, eventsBetween(from, to));
    }
    if (p === '/api/stream') return stream(req, res);
    if (p === '/healthz') {
      res.end('ok');
      return;
    }
    if (p === '/metrics') {
      const s = sample(Math.floor(now / SEC) * SEC);
      res.setHeader('content-type', 'text/plain; version=0.0.4');
      res.end(
        `# mock re-export\nhalogen_dashboard_status{status="${s.status}"} 1\nllamacpp:requests_processing ${s.busy_slots ?? 0}\n`,
      );
      return;
    }
    if (p === '/__mock/outage') {
      const from = Math.ceil(now / SEC) * SEC;
      OUTAGES.push({ from, to: from + 20 * SEC, detail: 'connect: connection refused' });
      RESTARTS.push(from + 22 * SEC);
      res.end('outage for 20 s\n');
      return;
    }
    if (p === '/__mock/degraded') {
      const from = Math.ceil(now / SEC) * SEC;
      DEGRADED.push({ from, to: from + 30 * SEC, detail: 'NPU error: rknn_run timed out' });
      res.end('degraded for 30 s\n');
      return;
    }
    if (p === '/__mock/drop') {
      refuseUntil = now + 6 * SEC;
      for (const r of streams) r.destroy();
      streams.clear();
      res.end('dropped; streams refused for 6 s\n');
      return;
    }
    next();
  };
}
