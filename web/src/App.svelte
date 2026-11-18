<script lang="ts">
  import { onMount } from 'svelte';
  import Block from './components/Block.svelte';
  import Chart from './components/Chart.svelte';
  import Header from './components/Header.svelte';
  import Panels from './components/Panels.svelte';
  import Tiles from './components/Tiles.svelte';
  import { CHARTS } from './lib/charts';
  import { layout, type BlockDef } from './lib/layout.svelte';
  import { dash } from './lib/store.svelte';

  const PANELS = [
    { id: 'cache', title: 'Prompt cache' },
    { id: 'kv', title: 'KV pool' },
    { id: 'npu', title: 'NPU' },
    { id: 'log', title: 'Event log' },
  ] as const;
  // Block ids: chart ids and panel ids overlap (kv, cache), so each kind has a prefix.
  const chartDef = new Map(CHARTS.map((c) => [`chart-${c.id}`, c]));
  const panelId = new Map(PANELS.map((p) => [`panel-${p.id}`, p.id]));
  layout.setDefaults([
    ...CHARTS.map((c): BlockDef => ({ id: `chart-${c.id}`, title: c.title, width: c.wide ? 12 : 4 })),
    ...PANELS.map((p): BlockDef => ({ id: `panel-${p.id}`, title: p.title, width: 3 })),
  ]);
  const blocks = $derived(layout.blocks);

  onMount(() => {
    dash.start();
    return () => dash.stop();
  });
</script>

<a class="skip" href="#main">Skip to live values</a>
<Header />

<main id="main" tabindex="-1">
  {#if dash.error}
    <p class="banner" role="alert">
      The Collector did not return data: {dash.error}. The page tries again automatically.
    </p>
  {/if}

  <Tiles />

  {#if layout.arranging}
    <div class="arrange" role="region" aria-label="Arrange the page">
      <p>Drag a block by <span aria-hidden="true">⠿</span> or use the arrows, pick a width, or hide what you do not need. Saved in this browser.</p>
      <button type="button" onclick={() => layout.reset()} disabled={!layout.changed}>Reset to default</button>
      <button type="button" class="done" onclick={() => (layout.arranging = false)}>Done</button>
    </div>
  {/if}

  <div class="key" aria-label="Chart key">
    <span><i class="k-outage" aria-hidden="true"></i>Outage</span>
    <span><i class="k-degraded" aria-hidden="true"></i>Degraded</span>
    <span><i class="k-restart" aria-hidden="true"></i>Restart</span>
    <span class="src">{dash.source === 'rollup' ? 'One-minute Rollups: lines show the mean, shaded bands reach the maximum' : 'Raw Samples, 30 s rate window'}</span>
  </div>

  <section class="blocks" aria-label="Charts and details">
    <h2 class="sr-only">Charts, {dash.rangeDef.name.toLowerCase()}{dash.source === 'rollup' ? ', one-minute Rollups' : ''}</h2>
    {#each blocks as b, i (b.id)}
      <Block block={b} first={i === 0} last={i === blocks.length - 1}>
        {#if chartDef.has(b.id)}
          <Chart def={chartDef.get(b.id)!} />
        {:else if panelId.has(b.id)}
          <Panels id={panelId.get(b.id)!} />
        {/if}
      </Block>
    {/each}
  </section>
</main>

<style>
  main {
    padding: var(--sp-3) var(--sp-5) var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    max-width: 2400px;
    margin: 0 auto;
  }
  main:focus {
    outline: none;
  }
  .banner {
    border: 1px solid var(--crit-line);
    background: var(--crit-bg);
    color: var(--text);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-ctl);
    font-size: var(--fs-sm);
  }
  .blocks {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: var(--sp-3);
    align-items: start;
  }
  .arrange {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2) var(--sp-3);
    padding: var(--sp-2) var(--sp-3);
    border: 1px solid var(--accent);
    border-radius: var(--r-ctl);
    background: var(--accent-soft);
    font-size: var(--fs-sm);
  }
  .arrange p {
    flex: 1;
    margin: 0;
    min-width: 16rem;
  }
  .arrange button {
    font: inherit;
    min-height: 30px;
    padding: 0 var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .arrange button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .arrange .done {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .arrange button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .key {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-4);
    font-size: var(--fs-xs);
    color: var(--muted);
    margin-top: calc(-1 * var(--sp-1));
    padding: 0 var(--sp-1);
  }
  .key span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .key i {
    display: inline-block;
    width: 14px;
    height: 10px;
    border-radius: 2px;
  }
  .k-outage {
    background: var(--band-outage);
    outline: 1px solid var(--band-outage);
  }
  .k-degraded {
    background: var(--band-degraded);
    outline: 1px solid var(--warn-line);
  }
  .k-restart {
    width: 1px !important;
    border-left: 1px dashed var(--marker);
    border-radius: 0 !important;
  }
  .src {
    margin-left: auto;
    color: var(--faint);
  }

  @media (max-width: 720px) {
    main {
      padding: var(--sp-2) var(--sp-3) var(--sp-4);
    }
    .blocks {
      --plot-h: 190px;
    }
    .src {
      margin-left: 0;
    }
  }
</style>
