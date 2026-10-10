import type { HEvent, Series, Snapshot } from './types';

async function getJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const res = await fetch(url, { signal, headers: { accept: 'application/json' } });
  if (!res.ok) throw new Error(`${url.split('?')[0]} returned HTTP ${res.status}`);
  return (await res.json()) as T;
}

export function fetchSnapshot(signal?: AbortSignal): Promise<Snapshot> {
  return getJson<Snapshot>('/api/snapshot', signal);
}

export function fetchSeries(from: number, to: number, step?: number, signal?: AbortSignal): Promise<Series> {
  const q = new URLSearchParams({ from: String(Math.round(from)), to: String(Math.round(to)) });
  if (step) q.set('step', String(Math.round(step)));
  return getJson<Series>(`/api/series?${q}`, signal);
}

export function fetchEvents(from: number, to: number, signal?: AbortSignal): Promise<HEvent[]> {
  const q = new URLSearchParams({ from: String(Math.round(from)), to: String(Math.round(to)) });
  return getJson<HEvent[]>(`/api/events?${q}`, signal);
}
