<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { SeriesField } from '../lib/types';
  import type { Tone } from '../lib/status';
  import Spark from './Spark.svelte';

  let {
    label,
    value,
    unit = '',
    note = '',
    field = null,
    sparkMax = null,
    tone = 'none',
    extra,
  }: {
    label: string;
    value: string;
    unit?: string;
    note?: string;
    field?: SeriesField | null;
    sparkMax?: number | null;
    tone?: Tone;
    extra?: Snippet;
  } = $props();
</script>

<div class="tile" class:warn={tone === 'warn'} class:crit={tone === 'crit'}>
  <div class="label">
    <span>{label}</span>
    {#if tone !== 'none'}
      <span class="tone">Saturated</span>
    {/if}
  </div>
  <div class="row">
    <p class="value num">{value}{#if unit}<span class="unit">{unit}</span>{/if}</p>
    {#if extra}{@render extra()}{/if}
  </div>
  {#if field}
    <Spark {field} max={sparkMax} {tone} />
  {:else if note}
    <p class="note">{note}</p>
  {/if}
</div>

<style>
  .tile {
    background: var(--surface);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-tile);
    padding: 10px var(--sp-3) 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .tile.warn {
    background: linear-gradient(var(--warn-bg), var(--warn-bg)), var(--surface);
    border-color: var(--warn-line);
  }
  .tile.crit {
    background: linear-gradient(var(--crit-bg), var(--crit-bg)), var(--surface);
    border-color: var(--crit-line);
  }
  .label {
    display: flex;
    justify-content: space-between;
    gap: var(--sp-2);
    font-size: var(--fs-xs);
    color: var(--muted);
    white-space: nowrap;
    min-width: 0;
  }
  .label > span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tone {
    font-weight: 600;
    color: var(--degraded);
  }
  .crit .tone {
    color: var(--down);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    min-width: 0;
  }
  .value {
    font-size: var(--fs-lg);
    font-weight: 500;
    line-height: 1.15;
    letter-spacing: -0.02em;
    white-space: nowrap;
  }
  .unit {
    font-size: var(--fs-xs);
    font-weight: 400;
    color: var(--muted);
    letter-spacing: 0;
    margin-left: 4px;
  }
  .note {
    font-size: var(--fs-xs);
    color: var(--faint);
    height: 26px;
    display: flex;
    align-items: flex-end;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
