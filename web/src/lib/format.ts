// Number, unit and time formatting. All output is for tabular figures.

export const DASH = '\u2013';

const nf = (digits: number) =>
  new Intl.NumberFormat('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits });
const nf0 = nf(0);
const nf1 = nf(1);
const nf2 = nf(2);

function ok(v: number | null | undefined): v is number {
  return v !== null && v !== undefined && Number.isFinite(v);
}

/** Compact number with k/M/G suffix: 950, 9.5k, 12k, 1.2M. */
export function compact(v: number | null | undefined): string {
  if (!ok(v)) return DASH;
  const a = Math.abs(v);
  if (a < 1000) return a < 10 && !Number.isInteger(v) ? nf1.format(v) : nf0.format(v);
  const units: [number, string][] = [
    [1e12, 'T'],
    [1e9, 'G'],
    [1e6, 'M'],
    [1e3, 'k'],
  ];
  for (const [div, suf] of units) {
    if (a >= div) {
      const x = v / div;
      return (Math.abs(x) < 100 ? nf1.format(x) : nf0.format(x)) + suf;
    }
  }
  return nf0.format(v);
}

/** Integer with thousands separators. */
export function int(v: number | null | undefined): string {
  return ok(v) ? nf0.format(v) : DASH;
}

/** Token rate in tokens/s, value only. */
export function rate(v: number | null | undefined): string {
  if (!ok(v)) return DASH;
  if (Math.abs(v) >= 10000) return compact(v);
  if (Math.abs(v) >= 100) return nf0.format(v);
  return nf1.format(v);
}

/** Share 0..1 as percent value only (no sign). */
export function pctValue(v: number | null | undefined, digits = 1): string {
  if (!ok(v)) return DASH;
  return (digits === 0 ? nf0 : digits === 2 ? nf2 : nf1).format(v * 100);
}

export function pct(v: number | null | undefined, digits = 1): string {
  return ok(v) ? `${pctValue(v, digits)}%` : DASH;
}

/** Seconds as a short duration: 820 ms, 4.2 s, 3 min 10 s. */
export function seconds(v: number | null | undefined): string {
  if (!ok(v)) return DASH;
  if (v < 1) return `${nf0.format(v * 1000)} ms`;
  if (v < 60) return `${(v < 10 ? nf2 : nf1).format(v)} s`;
  const m = Math.floor(v / 60);
  const s = Math.round(v - m * 60);
  return `${m} min ${s} s`;
}

/** Split value and unit for tiles. */
export function secondsParts(v: number | null | undefined): [string, string] {
  if (!ok(v)) return [DASH, 's'];
  if (v < 1) return [nf0.format(v * 1000), 'ms'];
  if (v < 100) return [(v < 10 ? nf2 : nf1).format(v), 's'];
  return [nf0.format(v), 's'];
}

/** Bytes with binary units (KiB, MiB, GiB). */
export function bytes(v: number | null | undefined): string {
  if (!ok(v)) return DASH;
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let x = v;
  let i = 0;
  while (Math.abs(x) >= 1024 && i < units.length - 1) {
    x /= 1024;
    i++;
  }
  return `${i === 0 ? nf0.format(x) : nf1.format(x)} ${units[i]}`;
}

/** Duration in ms as a coarse human span: 42 s, 12 min, 3 h 5 min, 2 d 4 h. */
export function span(ms: number | null | undefined): string {
  if (!ok(ms)) return DASH;
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ${m % 60} min`;
  const d = Math.floor(h / 24);
  return `${d} d ${h % 24} h`;
}

/** Relative time "12 s ago" from ms timestamps. */
export function ago(t: number, now: number): string {
  const d = now - t;
  if (d < 1500) return 'now';
  const s = Math.round(d / 1000);
  if (s < 60) return `${s} s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} d ago`;
}

const dtf = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
});
const tf = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
});

export function absTime(t: number): string {
  return dtf.format(t);
}

export function clockTime(t: number): string {
  return tf.format(t);
}
