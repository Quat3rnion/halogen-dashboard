<script lang="ts">
  import { onMount } from 'svelte';
  import Chart from './components/Chart.svelte';
  import Header from './components/Header.svelte';
  import Panels from './components/Panels.svelte';
  import Tiles from './components/Tiles.svelte';
  import { CHARTS } from './lib/charts';
  import { dash } from './lib/store.svelte';

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

  <section class="charts" aria-label="Charts">
    <h2 class="sr-only">Charts, {dash.rangeDef.name.toLowerCase()}{dash.source === 'rollup' ? ', one-minute Rollups' : ''}</h2>
    {#each CHARTS as def (def.id)}
      <Chart {def} />
    {/each}
  </section>

  <div class="key" aria-label="Chart key">
    <span><i class="k-outage" aria-hidden="true"></i>Outage</span>
    <span><i class="k-degraded" aria-hidden="true"></i>Degraded</span>
    <span><i class="k-restart" aria-hidden="true"></i>Restart</span>
    <span class="src">{dash.source === 'rollup' ? 'One-minute Rollups: lines show the mean, shaded bands reach the maximum' : 'Raw Samples, 30 s rate window'}</span>
  </div>

  <Panels />
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
  .charts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--sp-3);
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

  @media (max-width: 1280px) {
    .charts {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 720px) {
    main {
      padding: var(--sp-2) var(--sp-3) var(--sp-4);
    }
    .charts {
      grid-template-columns: minmax(0, 1fr);
      --plot-h: 190px;
    }
    .src {
      margin-left: 0;
    }
  }
</style>
