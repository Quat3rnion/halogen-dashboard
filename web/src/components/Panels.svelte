<script lang="ts">
  import { ago, absTime, bytes, compact, int, pct, DASH } from '../lib/format';
  import { kvTone, STATUS_LABEL } from '../lib/status';
  import { dash } from '../lib/store.svelte';
  import type { HEvent } from '../lib/types';
  import Meter from './Meter.svelte';

  const c = $derived(dash.cache);
  const info = $derived(dash.info);
  const npu = $derived(info?.npu ?? null);
  const capBytes = $derived(info ? info.prompt_cache_cap_mb * 1024 * 1024 : null);
  const pool = $derived(c?.pool ?? null);
  const poolUse = $derived(pool && pool.positions ? pool.used / pool.positions : null);
  const reserved = $derived(dash.latest?.kv_reserved ?? null);
  const events = $derived(dash.logEvents);

  function evText(e: HEvent): string {
    switch (e.kind) {
      case 'restart':
        return 'Restart';
      case 'outage_start':
        return 'Outage started';
      case 'outage_end':
        return 'Outage ended';
      case 'status_change':
        return `Status ${e.from ? STATUS_LABEL[e.from] : DASH} to ${e.to ? STATUS_LABEL[e.to] : DASH}`;
    }
  }
  function evTone(e: HEvent): string {
    if (e.kind === 'outage_start') return 'down';
    if (e.kind === 'outage_end') return 'up';
    if (e.kind === 'status_change') return e.to ?? 'none';
    return 'restart';
  }
</script>

<section class="panels" aria-label="Details">
  <article class="card panel">
    <header>
      <h2>Prompt cache</h2>
      {#if c}<span class="aside num" title={absTime(c.t)}>Scraped {ago(Math.min(c.t, dash.now), dash.now)}</span>{/if}
    </header>
    {#if c}
      <div class="gauge">
        <div class="glabel"><span>Entries</span><span class="num">{int(c.entries)} / {int(c.max_entries)}</span></div>
        <Meter value={c.max_entries ? c.entries / c.max_entries : null} label="Prompt cache entries" />
      </div>
      <div class="gauge">
        <div class="glabel">
          <span>Size</span>
          <span class="num">{bytes(c.bytes)} / {capBytes ? bytes(capBytes) : DASH}</span>
        </div>
        <Meter value={capBytes ? c.bytes / capBytes : null} label="Prompt cache size as a share of the cap" />
      </div>
      <dl class="kv">
        <dt>Hits</dt><dd class="num">{int(c.hits)}</dd>
        <dt>Misses</dt><dd class="num">{int(c.misses)}</dd>
        <dt>Hit rate since start</dt><dd class="num">{pct(c.hit_rate)}</dd>
        <dt>Token hit rate since start</dt><dd class="num">{pct(c.token_hit_rate)}</dd>
        <dt>Evicted</dt><dd class="num">{int(c.evicted)}</dd>
        <dt>Refused</dt><dd class="num">{int(c.refused)}</dd>
        <dt>Prompt tokens saved</dt><dd class="num" title={int(c.prompt_tokens_saved)}>{compact(c.prompt_tokens_saved)}</dd>
      </dl>
    {:else}
      <p class="none">No /cache Scrape yet.</p>
    {/if}
  </article>

  <article class="card panel">
    <header>
      <h2>KV pool</h2>
      {#if info}<span class="aside num">{int(info.slots)} Slots of {compact(info.slot_ctx)} positions</span>{/if}
    </header>
    {#if pool}
      <div class="gauge">
        <div class="glabel"><span>Used</span><span class="num">{pct(poolUse, 2)}</span></div>
        <Meter value={poolUse} label="KV pool use" tone={kvTone(poolUse)} />
      </div>
      <dl class="kv">
        <dt>Positions</dt><dd class="num">{int(pool.positions)}</dd>
        <dt>Used</dt><dd class="num">{int(pool.used)}</dd>
        <dt>Reserved</dt><dd class="num">{int(reserved)}</dd>
        <dt>Busy regions</dt><dd class="num">{int(pool.busy_regions)}</dd>
        <dt>Held regions</dt><dd class="num">{int(pool.held_regions)}</dd>
        <dt>Waiting for room</dt><dd class="num" class:alert={pool.waiting_for_room > 0}>{int(pool.waiting_for_room)}</dd>
        <dt>Relocated</dt><dd class="num">{int(pool.relocated)}</dd>
      </dl>
    {:else}
      <p class="none">No /cache Scrape yet.</p>
    {/if}
  </article>

  <article class="card panel">
    <header>
      <h2>NPU</h2>
      {#if npu}
        <span class="npu-status" class:bad={npu.status !== 'ok'}>{npu.status === 'ok' ? 'Ok' : npu.status}</span>
      {/if}
    </header>
    {#if npu}
      {#if npu.error}<p class="err">{npu.error}</p>{/if}
      <ul class="models">
        {#each npu.models as m (m)}
          {@const on = npu.loaded.includes(m)}
          <li class:on>
            <span class="mdot" aria-hidden="true"></span>
            <span class="mname" translate="no">{m}</span>
            <span class="mstate">{on ? 'Loaded' : 'Not loaded'}</span>
          </li>
        {/each}
      </ul>
      <dl class="kv">
        <dt>Queued</dt><dd class="num">{int(npu.queued)}</dd>
        <dt>Passes</dt><dd class="num">{int(npu.passes)}</dd>
        <dt>Contexts</dt><dd class="num">{int(npu.contexts)}</dd>
      </dl>
    {:else}
      <p class="none">{info ? 'Halogen reports no NPU.' : 'No /health Scrape yet.'}</p>
    {/if}
  </article>

  <article class="card panel log">
    <header>
      <h2>Event log</h2>
      <span class="aside">{dash.isLive ? 'Newest 100' : dash.rangeDef.name}</span>
    </header>
    {#if events.length}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <ol class="events" tabindex="0" aria-label="Events, newest first">
        {#each events as e (e.t + e.kind + (e.to ?? ''))}
          <li class="ev e-{evTone(e)}">
            <span class="emark" aria-hidden="true"></span>
            <div class="etext">
              <span class="ekind">{evText(e)}</span>
              {#if e.detail}<span class="edetail">{e.detail}</span>{/if}
            </div>
            <time class="num" datetime={new Date(e.t).toISOString()}>
              <span>{ago(e.t, dash.now)}</span>
              <span class="abs">{absTime(e.t)}</span>
            </time>
          </li>
        {/each}
      </ol>
    {:else}
      <p class="none">No Restarts, Outages or Status changes {dash.isLive ? 'yet' : 'in this range'}.</p>
    {/if}
  </article>
</section>

<style>
  .panels {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)) minmax(0, 1.4fr);
    gap: var(--sp-3);
    align-items: start;
  }
  .panel {
    padding: var(--sp-3) var(--sp-4) var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    min-width: 0;
  }
  .panel > header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  .aside {
    font-size: var(--fs-xs);
    color: var(--faint);
    white-space: nowrap;
  }
  .gauge {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .glabel {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .glabel .num {
    color: var(--text);
  }
  .kv {
    margin: 0;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 5px var(--sp-3);
    font-size: var(--fs-sm);
  }
  .kv dt {
    color: var(--muted);
  }
  .kv dd {
    margin: 0;
    text-align: right;
  }
  .kv dd.alert {
    color: var(--degraded);
    font-weight: 600;
  }
  .none {
    color: var(--muted);
    font-size: var(--fs-sm);
  }

  .npu-status {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--up);
  }
  .npu-status.bad {
    color: var(--degraded);
  }
  .err {
    font-size: var(--fs-sm);
    color: var(--text);
    border-left: 2px solid var(--degraded);
    padding-left: var(--sp-2);
  }
  .models {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-sm);
  }
  .models li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--sp-2);
  }
  .mdot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    border: 1.5px solid var(--faint);
  }
  .on .mdot {
    background: var(--s1);
    border-color: var(--s1);
  }
  .mname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mstate {
    font-size: var(--fs-xs);
    color: var(--faint);
  }
  .on .mstate {
    color: var(--text);
  }

  .events {
    list-style: none;
    margin: 0 calc(-1 * var(--sp-2));
    padding: 0 var(--sp-2);
    max-height: 300px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-ctl);
  }
  .ev {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: var(--sp-2);
    align-items: start;
    padding: 6px 0;
    border-bottom: 1px solid var(--line-soft);
    font-size: var(--fs-sm);
  }
  .ev:last-child {
    border-bottom: 0;
  }
  .emark {
    width: 8px;
    height: 8px;
    margin-top: 6px;
    border-radius: 50%;
    background: var(--faint);
  }
  .e-down .emark {
    background: var(--down);
  }
  .e-up .emark {
    background: var(--up);
  }
  .e-degraded .emark {
    background: var(--degraded);
  }
  .e-restart .emark {
    background: transparent;
    border: 1.5px dashed var(--muted);
  }
  .etext {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .edetail {
    color: var(--muted);
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }
  time {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    white-space: nowrap;
  }
  .abs {
    font-size: var(--fs-xs);
    color: var(--faint);
  }

  @media (max-width: 1500px) {
    .panels {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 720px) {
    .panels {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
