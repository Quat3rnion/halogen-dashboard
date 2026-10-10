import { clockTime } from './format';
import type { Info, Sample, Status } from './types';

export const STATUS_LABEL: Record<Status, string> = { up: 'Up', degraded: 'Degraded', down: 'Down' };

/** Short reason for the current Status. */
export function statusReason(status: Status | null, info: Info | null): string {
  if (!status) return 'Waiting for the first Sample';
  if (status === 'down') {
    return info?.last_scrape_ok ? `Scrapes fail since ${clockTime(info.last_scrape_ok)}` : 'Scrapes fail';
  }
  if (status === 'degraded') {
    const parts: string[] = [];
    if (info && !info.engine_responds) parts.push('engine does not respond');
    const npu = info?.npu;
    if (npu && npu.status !== 'ok') parts.push(`NPU ${npu.status}${npu.error ? `: ${npu.error}` : ''}`);
    if (!parts.length) return 'Engine or NPU reports a fault';
    const s = parts.join('; ');
    return s.charAt(0).toUpperCase() + s.slice(1);
  }
  return 'Engine and NPU report ok';
}

export type Tone = 'none' | 'warn' | 'crit';

/**
 * Saturation tone. Saturation is not a Status.
 * Slots: warn when all Slots are busy and requests are queued, crit when the queue is at least one per Slot.
 * KV pool: warn above 90 % use, crit above 97 %.
 */
export function slotTone(s: Sample | null, slots: number | null): Tone {
  if (!s || s.busy_slots === null || s.queued === null || !slots) return 'none';
  if (s.busy_slots < slots || s.queued <= 0) return 'none';
  return s.queued >= slots ? 'crit' : 'warn';
}

export function kvTone(kvUse: number | null | undefined): Tone {
  if (kvUse === null || kvUse === undefined) return 'none';
  if (kvUse > 0.97) return 'crit';
  if (kvUse > 0.9) return 'warn';
  return 'none';
}
