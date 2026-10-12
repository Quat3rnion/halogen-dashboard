import { bytes, compact, pct, rate, seconds } from './format';
import type { SeriesField } from './types';

export type ColorVar = '--s1' | '--s2' | '--s3' | '--s4' | '--s5';

export interface LineDef {
  field: SeriesField;
  label: string;
  color: ColorVar;
  scale: string;
  fmt: (v: number | null) => string;
  stepped?: boolean;
  /** Faint line that shows the maximum in each Rollup. Only for Rollup data. */
  maxOf?: SeriesField;
  fill?: boolean;
}

export interface AxisDef {
  scale: string;
  side: 'left' | 'right';
  /** Short unit text for the axis. */
  unit: string;
  tick: (v: number) => string;
  /** Fixed range or minimum span. */
  range?: [number, number];
  minMax?: number;
  integer?: boolean;
  /** Byte values: round the range and the ticks to binary units (KiB, MiB, GiB). */
  binary?: boolean;
}

export interface ChartDef {
  id: string;
  title: string;
  lines: LineDef[];
  axes: AxisDef[];
  /** Use the full row of the chart grid. */
  wide?: boolean;
}

const tokS = (v: number | null) => (v === null ? rate(v) : `${rate(v)} tokens/s`);
const pctTick = (v: number) => `${Math.round(v * 100)}%`;
const numTick = (v: number) => compact(v);
const secTick = (v: number) => (v < 1 && v > 0 ? `${Math.round(v * 1000)} ms` : `${compact(v)} s`);
const perMin = (v: number | null) => (v === null ? rate(v) : `${rate(v)}/min`);
const count = (v: number | null) => compact(v);
const tickNf = (d: number) => new Intl.NumberFormat('en-US', { maximumFractionDigits: d });
const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
/** Short binary size for an axis: 0, 512 MiB, 1.5 GiB. */
const byteTick = (v: number) => {
  if (v === 0) return '0';
  let x = v;
  let i = 0;
  while (Math.abs(x) >= 1024 && i < BYTE_UNITS.length - 1) {
    x /= 1024;
    i++;
  }
  return `${tickNf(Math.abs(x) < 10 && !Number.isInteger(x) ? 1 : 0).format(x)} ${BYTE_UNITS[i]}`;
};

export const CHARTS: ChartDef[] = [
  {
    id: 'gen',
    title: 'Generation speed and Throughput',
    lines: [
      { field: 'gen_speed', label: 'Generation speed', color: '--s1', scale: 'tps', fmt: tokS, fill: true },
      { field: 'throughput', label: 'Throughput', color: '--s2', scale: 'tps', fmt: tokS },
    ],
    axes: [{ scale: 'tps', side: 'left', unit: 'tokens/s', tick: numTick, minMax: 10 }],
  },
  {
    id: 'prefill',
    title: 'Prefill speed and Prefill throughput',
    lines: [
      { field: 'prefill_speed', label: 'Prefill speed', color: '--s1', scale: 'tps', fmt: tokS, fill: true },
      { field: 'prefill_throughput', label: 'Prefill throughput', color: '--s2', scale: 'tps', fmt: tokS },
    ],
    axes: [{ scale: 'tps', side: 'left', unit: 'tokens/s', tick: numTick, minMax: 10 }],
  },
  {
    id: 'slots',
    title: 'Busy Slots, Queued requests and Waiting for room',
    lines: [
      {
        field: 'busy_slots',
        label: 'Busy Slots',
        color: '--s1',
        scale: 'n',
        fmt: (v) => compact(v),
        stepped: true,
        fill: true,
        maxOf: 'busy_slots_max',
      },
      {
        field: 'queued',
        label: 'Queued requests',
        color: '--s4',
        scale: 'n',
        fmt: (v) => compact(v),
        stepped: true,
        maxOf: 'queued_max',
      },
      {
        field: 'kv_waiting',
        label: 'Waiting for room',
        color: '--s3',
        scale: 'n',
        fmt: count,
        stepped: true,
        maxOf: 'kv_waiting_max',
      },
    ],
    axes: [{ scale: 'n', side: 'left', unit: 'count', tick: numTick, minMax: 4, integer: true }],
  },
  {
    id: 'kv',
    title: 'KV pool use',
    lines: [
      { field: 'kv_use', label: 'KV pool use', color: '--s1', scale: 'pct', fmt: (v) => pct(v), fill: true, maxOf: 'kv_use_max' },
      { field: 'kv_reserved_use', label: 'Reserved', color: '--s2', scale: 'pct', fmt: (v) => pct(v) },
    ],
    axes: [{ scale: 'pct', side: 'left', unit: '%', tick: pctTick, minMax: 1 }],
  },
  {
    id: 'cache',
    title: 'Prompt-cache hit rates and Draft acceptance',
    lines: [
      { field: 'prompt_cache_hit_rate', label: 'Prompt-cache hit rate', color: '--s1', scale: 'pct', fmt: (v) => pct(v) },
      { field: 'cache_lookup_hit_rate', label: 'Cache lookup hit rate', color: '--s5', scale: 'pct', fmt: (v) => pct(v) },
      { field: 'draft_acceptance', label: 'Draft acceptance', color: '--s2', scale: 'pct', fmt: (v) => pct(v) },
    ],
    axes: [{ scale: 'pct', side: 'left', unit: '%', tick: pctTick, range: [0, 1] }],
  },
  {
    id: 'req',
    title: 'Requests/min, Mean service time and Oldest request age',
    lines: [
      { field: 'requests_per_min', label: 'Requests/min', color: '--s1', scale: 'rpm', fmt: (v) => (v === null ? compact(v) : `${rate(v)}/min`) },
      { field: 'mean_service_time', label: 'Mean service time', color: '--s2', scale: 'sec', fmt: (v) => seconds(v) },
      {
        field: 'busy_for',
        label: 'Oldest request age',
        color: '--s3',
        scale: 'sec',
        fmt: (v) => seconds(v),
        maxOf: 'busy_for_max',
      },
    ],
    axes: [
      { scale: 'rpm', side: 'left', unit: 'requests/min', tick: numTick, minMax: 1 },
      { scale: 'sec', side: 'right', unit: 's', tick: secTick, minMax: 1 },
    ],
  },
  {
    id: 'cache-activity',
    title: 'Prompt-cache activity',
    wide: true,
    lines: [
      { field: 'cache_bytes', label: 'Prompt-cache size', color: '--s1', scale: 'bytes', fmt: (v) => bytes(v), fill: true },
      { field: 'cache_evictions_per_min', label: 'Evictions/min', color: '--s4', scale: 'pm', fmt: perMin },
      { field: 'cache_rewrites_per_min', label: 'Last-message rewrites/min', color: '--s2', scale: 'pm', fmt: perMin },
      { field: 'cache_disk_hits_per_min', label: 'Disk hits/min', color: '--s5', scale: 'pm', fmt: perMin },
      { field: 'cache_disk_cut_backs_per_min', label: 'Disk cut-backs/min', color: '--s3', scale: 'pm', fmt: perMin },
    ],
    axes: [
      { scale: 'pm', side: 'left', unit: 'per min', tick: numTick, minMax: 1 },
      { scale: 'bytes', side: 'right', unit: 'bytes', tick: byteTick, minMax: 1024 * 1024, binary: true },
    ],
  },
];
