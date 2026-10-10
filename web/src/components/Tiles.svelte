<script lang="ts">
  import { compact, pctValue, rate, secondsParts, DASH, int } from '../lib/format';
  import { kvTone, slotTone } from '../lib/status';
  import { dash } from '../lib/store.svelte';
  import Tile from './Tile.svelte';

  const s = $derived(dash.latest);
  const slots = $derived(dash.info?.slots ?? null);
  const sTone = $derived(slotTone(s, slots));
  const kTone = $derived(kvTone(s?.kv_use));
  const busy = $derived(s?.busy_slots ?? null);
  const svc = $derived(secondsParts(s?.mean_service_time));
  const tot = $derived(dash.rangeTotals);
</script>

<section class="tiles" aria-label="Live values">
  <Tile label="Generation speed" value={rate(s?.gen_speed)} unit="tokens/s" field="gen_speed" />
  <Tile label="Throughput" value={rate(s?.throughput)} unit="tokens/s" field="throughput" />
  <Tile label="Prefill speed" value={rate(s?.prefill_speed)} unit="tokens/s" field="prefill_speed" />
  <Tile label="Prefill throughput" value={rate(s?.prefill_throughput)} unit="tokens/s" field="prefill_throughput" />
  <Tile
    label="Busy Slots"
    value={busy === null ? DASH : String(busy)}
    unit={slots ? `/ ${slots}` : ''}
    field="busy_slots"
    sparkMax={slots}
    tone={sTone}
  >
    {#snippet extra()}
      {#if slots}
        <span class="slots" aria-hidden="true">
          {#each { length: slots } as _, i (i)}
            <span class="slot" class:on={busy !== null && i < busy}></span>
          {/each}
        </span>
      {/if}
    {/snippet}
  </Tile>
  <Tile label="Queued requests" value={s?.queued === null || s?.queued === undefined ? DASH : int(s.queued)} field="queued" tone={sTone} />
  <Tile label="KV pool use" value={pctValue(s?.kv_use)} unit="%" field="kv_use" sparkMax={1} tone={kTone} />
  <Tile label="Prompt-cache hit rate" value={pctValue(s?.prompt_cache_hit_rate)} unit="%" field="prompt_cache_hit_rate" sparkMax={1} />
  <Tile label="Draft acceptance" value={pctValue(s?.draft_acceptance)} unit="%" field="draft_acceptance" sparkMax={1} />
  <Tile label="Requests/min" value={rate(s?.requests_per_min)} field="requests_per_min" />
  <Tile label="Mean service time" value={svc[0]} unit={svc[1]} field="mean_service_time" />

  <div class="totals">
    <p class="tlabel">Totals, {dash.rangeDef.name.toLowerCase()}</p>
    <dl>
      <div>
        <dt>Requests</dt>
        <dd class="num" title={tot ? int(tot.requests) : undefined}>{compact(tot?.requests)}</dd>
      </div>
      <div>
        <dt>Tokens generated</dt>
        <dd class="num" title={tot ? int(tot.predicted_tokens) : undefined}>{compact(tot?.predicted_tokens)}</dd>
      </div>
      <div>
        <dt>Prompt tokens</dt>
        <dd class="num" title={tot ? int(tot.prompt_tokens) : undefined}>{compact(tot?.prompt_tokens)}</dd>
      </div>
    </dl>
  </div>
</section>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: var(--sp-2);
  }
  .totals {
    grid-column: span 3;
    background: var(--raised);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-tile);
    padding: 10px var(--sp-3) 8px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    min-width: 0;
  }
  .tlabel {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--sp-3);
  }
  dt {
    font-size: var(--fs-xs);
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  dd {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 500;
    line-height: 1.15;
    letter-spacing: -0.02em;
  }
  dl > div {
    min-width: 0;
  }
  .slots {
    display: inline-flex;
    gap: 3px;
    flex: none;
  }
  .slot {
    width: 9px;
    height: 18px;
    border-radius: 2px;
    border: 1px solid var(--line);
    background: transparent;
  }
  .slot.on {
    background: var(--accent);
    border-color: var(--accent);
  }

  @media (max-width: 1280px) {
    .tiles {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
    .totals {
      grid-column: span 1;
    }
    .totals dl {
      gap: var(--sp-2);
    }
    .totals dd {
      font-size: 1.125rem;
    }
  }
  @media (max-width: 720px) {
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .totals {
      grid-column: 1 / -1;
    }
  }
</style>
