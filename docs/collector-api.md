# Collector API contract

This document is the contract between the Collector (Rust, `crates/`) and the web page (`web/`). Terms are defined in `CONTEXT.md`. Halogen surface shapes are in `fixtures/halogen-0.18.1/`. The Collector also supports the older shapes in `fixtures/halogen-0.17.3/`.

## Ports (development)

| Process | Port |
| --- | --- |
| Collector (`halogen-dashboard`) | 8790 |
| Fake Halogen (`fake-halogen`) | 8791 |
| Vite dev server | 5179, proxies `/api` and `/metrics` to 8790 |

Do not use ports 3000 or 4095.

## Configuration

Each Collector flag also reads an environment variable. `docker-compose.yml` sets them.

| Flag | Variable | Default |
| --- | --- | --- |
| `--halogen-url` | `HALOGEN_URL` | `http://192.168.50.177:8731` |
| `--listen` | `HALOGEN_DASHBOARD_LISTEN` | `0.0.0.0:8790` |
| `--db` | `HALOGEN_DASHBOARD_DB` | `./data/halogen-dashboard.sqlite` |

Log level: `RUST_LOG`. `fake-halogen` reads `FAKE_HALOGEN_LISTEN`, `FAKE_HALOGEN_SEED`, `FAKE_HALOGEN_SPEED` and `FAKE_HALOGEN_CHAOS`.

## Scrape schedule

| Halogen surface | Interval | Timeout |
| --- | --- | --- |
| `/metrics` | 1 s | 2 s |
| `/health` | 2 s | 3 s |
| `/cache` | 5 s | 3 s |

A failed `/metrics` Scrape starts or continues an Outage. The Sample for that tick has `status: "down"` and `null` for all Halogen values.

## Rate calculation

The Collector keeps **adjusted counters**. An adjusted counter goes up for all time. At a Restart (one of the original request, token or seconds counters goes down), the Collector adds the last value before the Restart to an offset for each counter. A decrease in a cache counter adds its last adjusted value to its offset, but does not cause a Restart event.

The Halogen seconds counters have only 6 significant digits. Calculate the change in prefill time as `delta(prompt_tokens) / llamacpp:prompt_tokens_seconds` when the gauge is more than 0. Do the same for generation time with `llamacpp:predicted_tokens_seconds`. Otherwise use the change in the seconds counter.

All rates use a rolling window of 30 s (the "rate window") in live data. In range queries, the window is the larger of 30 s and the step.

| Field | Formula over the window | Unit |
| --- | --- | --- |
| `gen_speed` | d(predicted_tokens) / d(predicted_seconds) | tokens/s, `null` if d(seconds) = 0 |
| `throughput` | d(predicted_tokens) / wall seconds | tokens/s |
| `prefill_speed` | d(prompt_tokens) / d(prompt_seconds) | tokens/s, `null` if d(seconds) = 0 |
| `prefill_throughput` | d(prompt_tokens) / wall seconds | tokens/s |
| `requests_per_min` | d(requests) / wall seconds x 60 | requests/min |
| `mean_service_time` | d(prompt_seconds + predicted_seconds) / d(requests) | s, `null` if d(requests) = 0 |
| `prompt_cache_hit_rate` | d(cached) / (d(cached) + d(prompt_tokens)) | 0..1, `null` if denominator = 0 |
| `draft_acceptance` | d(draft_accepted) / d(draft_tokens) | 0..1, `null` if d(draft_tokens) = 0 |
| `cache_lookup_hit_rate` | d(cache_hits) / (d(cache_hits) + d(cache_misses)) | 0..1, `null` if denominator = 0 |
| `cache_evictions_per_min` | d(cache_evicted) / wall seconds x 60 | entries/min |
| `cache_rewrites_per_min` | d(cache_rewrites) / wall seconds x 60 | rewrites/min |
| `cache_disk_hits_per_min` | d(cache_disk_hits) / wall seconds x 60 | hits/min |
| `cache_disk_cut_backs_per_min` | d(cache_disk_cut_backs) / wall seconds x 60 | cut-backs/min |

Cache rates are `null` when the counter is missing at either end of the window. All new Sample fields are `null` on Halogen 0.17.3. New Totals fields start at 0 when the counters are missing. Cache values come from the engine's last report. They can lag by `cache_stats_age` seconds.

## Data types

```ts
type Status = "up" | "degraded" | "down";

interface Sample {
  t: number;                 // Unix time, ms
  status: Status;
  busy_slots: number | null; // llamacpp:requests_processing
  queued: number | null;     // llamacpp:requests_deferred
  kv_used: number | null;    // llamacpp:kv_cache_tokens
  kv_reserved: number | null;// halogen:kv_pool_reserved_tokens
  kv_use: number | null;     // llamacpp:kv_cache_usage_ratio, 0..1
  gen_speed: number | null;
  throughput: number | null;
  prefill_speed: number | null;
  prefill_throughput: number | null;
  requests_per_min: number | null;
  mean_service_time: number | null;
  prompt_cache_hit_rate: number | null;
  draft_acceptance: number | null;
  busy_for: number | null;             // oldest running request, seconds
  kv_waiting: number | null;           // halogen:kv_pool_waiting_for_room
  cache_entries: number | null;
  cache_bytes: number | null;
  cache_stats_age: number | null;      // halogen:cache_stats_age_seconds
  cache_disk_enabled: number | null;   // 0 or 1
  cache_disk_bytes: number | null;
  cache_disk_budget: number | null;    // halogen:cache_disk_budget_bytes
  cache_lookup_hit_rate: number | null;
  cache_evictions_per_min: number | null;
  cache_rewrites_per_min: number | null;
  cache_disk_hits_per_min: number | null;
  cache_disk_cut_backs_per_min: number | null;
  totals: Totals | null;     // adjusted counters, for tiles
}

interface Totals {
  requests: number; prompt_tokens: number; predicted_tokens: number;
  cached_tokens: number; draft_tokens: number; draft_accepted: number;
  structured_requests: number;
  cache_hits: number; cache_misses: number; cache_evicted: number;
  cache_rewrites: number; cache_disk_hits: number; cache_disk_cut_backs: number;
}

interface Info {                 // from the last good /health and /v1/models
  halogen_url: string;
  model: string; version_api: string; version_engine: string;
  slots: number; slot_ctx: number; kv_pool: number;
  drafter_default: string;
  prompt_cache_cap_mb: number;
  engine_responds: boolean; engine_probe_s: number;
  npu: { status: string; models: string[]; loaded: string[];
         queued: number; passes: number; contexts: number;
         error?: string } | null;
  uptime_since: number | null;   // ms; see Uptime in CONTEXT.md
  last_scrape_ok: number | null; // ms
}

interface CacheInfo {            // from the last good /cache
  t: number;
  entries: number; max_entries: number; bytes: number;
  hits: number; misses: number; evicted: number; refused: number;
  hit_rate: number; token_hit_rate: number; prompt_tokens_saved: number;
  pool: { positions: number; used: number; busy_regions: number;
          held_regions: number; waiting_for_room: number; relocated: number };
}

type EventKind = "restart" | "outage_start" | "outage_end" | "status_change";
interface Event { t: number; kind: EventKind; from?: Status; to?: Status; detail?: string }
```

## Endpoints

`GET /api/snapshot` returns `{ info: Info | null, cache: CacheInfo | null, latest: Sample | null, events: Event[] }`. `events` holds the 100 newest events.

`GET /api/series?from=<ms>&to=<ms>&step=<ms>` returns a columnar series. `step` is optional. The default makes about 1,000 points. The minimum is 1,000 ms. The Collector uses raw Samples when `from` is in the last 48 h and `step` is less than 60 s. Otherwise it uses Rollups.

```ts
interface Series {
  from: number; to: number; step: number; source: "raw" | "rollup";
  t: number[];                      // bucket start, ms
  // columns have the same length as t; null for no data or Outage:
  busy_slots: (number|null)[]; queued: ...; kv_use: ...; kv_reserved_use: ...; // kv_reserved / kv_pool
  gen_speed: ...; throughput: ...; prefill_speed: ...; prefill_throughput: ...;
  requests_per_min: ...; mean_service_time: ...;
  prompt_cache_hit_rate: ...; draft_acceptance: ...;
  busy_for: ...; kv_waiting: ...; cache_entries: ...; cache_bytes: ...;
  cache_lookup_hit_rate: ...; cache_evictions_per_min: ...; cache_rewrites_per_min: ...;
  cache_disk_hits_per_min: ...; cache_disk_cut_backs_per_min: ...; cache_disk_bytes: ...;
  // gauge maxima in each bucket, for spikes:
  busy_slots_max: ...; queued_max: ...; kv_use_max: ...;
  busy_for_max: ...; kv_waiting_max: ...;
  status: (Status|null)[];          // worst status in the bucket
  totals_in_range: Totals;          // adjusted counter change from `from` to `to`
}
```

Gauge columns show the mean in each bucket. Rate columns use counter changes over the rate window, not the mean of Sample rates. `cache_stats_age`, `cache_disk_enabled` and `cache_disk_budget` are live Sample fields only.

`GET /api/events?from=<ms>&to=<ms>` returns `Event[]`, oldest first.

`GET /api/stream` is Server-Sent Events. On connect, it sends `event: snapshot` with the `/api/snapshot` body. After that it sends `event: sample` (a `Sample`) each second, `event: info` when `Info` changes, `event: cache` after each `/cache` Scrape, and `event: event` for each new `Event`. It sends a `: keepalive` comment every 15 s.

`GET /metrics` is the Prometheus re-export for ADR 0001. It shows the adjusted counters with the Halogen names, the current gauges, `halogen_dashboard_status{status="up|degraded|down"}` and `halogen_dashboard_restarts_total`.

`GET /healthz` returns `200 ok` while the Collector runs.

## Storage

SQLite (WAL) at `--db` (default `./data/halogen-dashboard.sqlite`). Raw Samples: 48 h. One-minute Rollups (min, mean, max of each Sample number field, last adjusted counters, worst status): 30 d. Events: 30 d. A prune task runs every 10 min.
