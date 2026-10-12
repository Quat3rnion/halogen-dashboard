// Types of the Collector API. Source of truth: docs/collector-api.md.

export type Status = 'up' | 'degraded' | 'down';

export interface Totals {
  requests: number;
  prompt_tokens: number;
  predicted_tokens: number;
  cached_tokens: number;
  draft_tokens: number;
  draft_accepted: number;
  structured_requests: number;
  cache_hits: number;
  cache_misses: number;
  cache_evicted: number;
  cache_rewrites: number;
  cache_disk_hits: number;
  cache_disk_cut_backs: number;
}

export interface Sample {
  t: number;
  status: Status;
  busy_slots: number | null;
  queued: number | null;
  kv_used: number | null;
  kv_reserved: number | null;
  kv_use: number | null;
  gen_speed: number | null;
  throughput: number | null;
  prefill_speed: number | null;
  prefill_throughput: number | null;
  requests_per_min: number | null;
  mean_service_time: number | null;
  prompt_cache_hit_rate: number | null;
  draft_acceptance: number | null;
  // Halogen 0.17.5+. Null when the series is missing (an old Halogen) or undefined.
  /** Run time of the oldest running request, s. */
  busy_for: number | null;
  kv_waiting: number | null;
  cache_entries: number | null;
  cache_bytes: number | null;
  /** Age of the engine's last cache report, s. Cache values can lag by this much. */
  cache_stats_age: number | null;
  /** 0 or 1. */
  cache_disk_enabled: number | null;
  cache_disk_bytes: number | null;
  cache_disk_budget: number | null;
  /** Share of prompt-cache lookups that found an entry. Not the Prompt-cache hit rate (token share). */
  cache_lookup_hit_rate: number | null;
  cache_evictions_per_min: number | null;
  cache_rewrites_per_min: number | null;
  cache_disk_hits_per_min: number | null;
  cache_disk_cut_backs_per_min: number | null;
  totals: Totals | null;
}

export interface NpuInfo {
  status: string;
  models: string[];
  loaded: string[];
  queued: number;
  passes: number;
  contexts: number;
  error?: string;
}

export interface Info {
  halogen_url: string;
  model: string;
  version_api: string;
  version_engine: string;
  slots: number;
  slot_ctx: number;
  kv_pool: number;
  drafter_default: string;
  prompt_cache_cap_mb: number;
  engine_responds: boolean;
  engine_probe_s: number;
  npu: NpuInfo | null;
  uptime_since: number | null;
  last_scrape_ok: number | null;
}

export interface CacheInfo {
  t: number;
  entries: number;
  max_entries: number;
  bytes: number;
  hits: number;
  misses: number;
  evicted: number;
  refused: number;
  hit_rate: number;
  token_hit_rate: number;
  prompt_tokens_saved: number;
  pool: {
    positions: number;
    used: number;
    busy_regions: number;
    held_regions: number;
    waiting_for_room: number;
    relocated: number;
  };
}

export type EventKind = 'restart' | 'outage_start' | 'outage_end' | 'status_change';

export interface HEvent {
  t: number;
  kind: EventKind;
  from?: Status;
  to?: Status;
  detail?: string;
}

export interface Snapshot {
  info: Info | null;
  cache: CacheInfo | null;
  latest: Sample | null;
  events: HEvent[];
}

/** Numeric columns of a Series, in the order the page uses them. */
export const SERIES_FIELDS = [
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
  'busy_for',
  'kv_waiting',
  'cache_entries',
  'cache_bytes',
  'cache_lookup_hit_rate',
  'cache_evictions_per_min',
  'cache_rewrites_per_min',
  'cache_disk_hits_per_min',
  'cache_disk_cut_backs_per_min',
  'cache_disk_bytes',
  'busy_slots_max',
  'queued_max',
  'kv_use_max',
  'busy_for_max',
  'kv_waiting_max',
] as const;

export type SeriesField = (typeof SERIES_FIELDS)[number];

export type Column = (number | null)[];

export type SeriesColumns = { [K in SeriesField]: Column };

export interface Series extends SeriesColumns {
  from: number;
  to: number;
  step: number;
  source: 'raw' | 'rollup';
  t: number[];
  status: (Status | null)[];
  totals_in_range: Totals;
}
