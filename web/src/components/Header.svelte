<script lang="ts">
  import { ago, span, DASH } from '../lib/format';
  import { STATUS_LABEL, statusReason } from '../lib/status';
  import { dash, RANGES, type RangeId } from '../lib/store.svelte';
  import { theme, type ThemePref } from '../lib/theme.svelte';

  const info = $derived(dash.info);
  const status = $derived(dash.latest?.status ?? null);
  const reason = $derived(statusReason(status, info));
  const version = $derived.by(() => {
    if (!info) return DASH;
    return info.version_api === info.version_engine
      ? `Halogen ${info.version_api}`
      : `Halogen ${info.version_api}, engine ${info.version_engine}`;
  });
  const uptime = $derived(
    info?.uptime_since && status !== 'down' ? span(Math.max(0, dash.now - info.uptime_since)) : DASH,
  );
  const scrapeAge = $derived(info?.last_scrape_ok ? ago(Math.min(info.last_scrape_ok, dash.now), dash.now) : DASH);
  const connLabel = $derived(
    dash.conn === 'live' ? 'Live' : dash.conn === 'reconnecting' ? 'Reconnecting' : 'Connecting',
  );
  const host = $derived.by(() => {
    if (!info?.halogen_url) return DASH;
    try {
      return new URL(info.halogen_url).host;
    } catch {
      return info.halogen_url;
    }
  });

  const themes: { id: ThemePref; label: string }[] = [
    { id: 'system', label: 'System theme' },
    { id: 'dark', label: 'Dark theme' },
    { id: 'light', label: 'Light theme' },
  ];

  function pickRange(e: Event): void {
    dash.setRange((e.currentTarget as HTMLInputElement).value as RangeId);
  }
  function pickTheme(e: Event): void {
    theme.set((e.currentTarget as HTMLInputElement).value as ThemePref);
  }
</script>

<header class="bar">
  <div class="id">
    <h1 class="brand" translate="no">Halogen</h1>
    <div class="status s-{status ?? 'none'}" role="status">
      <span class="pill"><span class="dot" aria-hidden="true"></span>{status ? STATUS_LABEL[status] : 'No data'}</span>
      <span class="reason">{reason}</span>
    </div>
  </div>

  <dl class="meta">
    <div class="m-model">
      <dt>Model</dt>
      <dd title={info?.model} translate="no">{info?.model ?? DASH}</dd>
    </div>
    <div>
      <dt>Version</dt>
      <dd>{version}</dd>
    </div>
    <div>
      <dt>Uptime</dt>
      <dd class="num">{uptime}</dd>
    </div>
    <div class="m-url">
      <dt>Halogen URL</dt>
      <dd title={info?.halogen_url} translate="no">{host}</dd>
    </div>
    <div>
      <dt>Last Scrape</dt>
      <dd class="num">{scrapeAge}</dd>
    </div>
  </dl>

  <div class="controls">
    <span class="conn c-{dash.conn}" title={dash.conn === 'live' ? 'Connected to the Collector stream' : 'The Collector stream is not connected'}>
      <span class="cdot" aria-hidden="true"></span>{connLabel}
    </span>

    <fieldset class="seg ranges">
      <legend class="sr-only">Time range</legend>
      {#each RANGES as r (r.id)}
        <label class:on={dash.range === r.id}>
          <input type="radio" name="range" value={r.id} checked={dash.range === r.id} onchange={pickRange} aria-label={r.name} />
          <span>{r.label}</span>
        </label>
      {/each}
    </fieldset>

    <fieldset class="seg theme">
      <legend class="sr-only">Theme</legend>
      {#each themes as t (t.id)}
        <label class:on={theme.pref === t.id} title={t.label}>
          <input type="radio" name="theme" value={t.id} checked={theme.pref === t.id} onchange={pickTheme} aria-label={t.label} />
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            {#if t.id === 'system'}
              <rect x="2" y="3" width="12" height="8" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.4" />
              <path d="M5.5 13.5h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
            {:else if t.id === 'dark'}
              <path d="M13 9.6A5.5 5.5 0 0 1 6.4 3a5.5 5.5 0 1 0 6.6 6.6Z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" />
            {:else}
              <circle cx="8" cy="8" r="2.8" fill="none" stroke="currentColor" stroke-width="1.4" />
              <path
                d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            {/if}
          </svg>
        </label>
      {/each}
    </fieldset>
  </div>
</header>

<style>
  .bar {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: var(--sp-3) var(--sp-5);
    flex-wrap: wrap;
    padding: 10px var(--sp-5);
    background: var(--header-bg);
    backdrop-filter: blur(8px);
    border-bottom: 1px solid var(--line-soft);
  }
  .id {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-width: 0;
  }
  .brand {
    font-size: 1.0625rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--accent);
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-width: 0;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px 2px 8px;
    border-radius: var(--r-pill);
    font-weight: 600;
    font-size: var(--fs-sm);
    border: 1px solid currentColor;
    color: var(--muted);
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
  }
  .s-up .pill {
    color: var(--up);
  }
  .s-degraded .pill {
    color: var(--degraded);
  }
  .s-down .pill {
    color: var(--down);
  }
  .reason {
    color: var(--muted);
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 34ch;
  }
  .s-degraded .reason,
  .s-down .reason {
    color: var(--text);
  }

  .meta {
    display: flex;
    gap: var(--sp-1) var(--sp-5);
    margin: 0;
    flex-wrap: wrap;
    min-width: 0;
    flex: 1 1 auto;
  }
  .meta > div {
    min-width: 0;
  }
  dt {
    font-size: 0.6875rem;
    color: var(--faint);
    line-height: 1.2;
  }
  dd {
    margin: 0;
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .m-model dd {
    max-width: 26ch;
  }
  .m-url dd {
    max-width: 24ch;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    margin-left: auto;
    flex-wrap: wrap;
  }
  .conn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--muted);
    min-width: 7.5rem;
    justify-content: flex-end;
  }
  .cdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid currentColor;
  }
  .c-live {
    color: var(--text);
  }
  .c-live .cdot {
    background: var(--accent);
    border-color: var(--accent);
    animation: pulse 2s var(--ease) infinite;
  }
  .c-reconnecting .cdot,
  .c-connecting .cdot {
    animation: blink 1s steps(2, start) infinite;
  }
  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 var(--accent-soft);
    }
    70% {
      box-shadow: 0 0 0 6px transparent;
    }
    100% {
      box-shadow: 0 0 0 0 transparent;
    }
  }
  @keyframes blink {
    to {
      visibility: hidden;
    }
  }

  .seg {
    display: inline-flex;
    margin: 0;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--surface);
    min-width: 0;
  }
  .seg label {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 28px;
    padding: 0 9px;
    border-radius: 4px;
    font-size: var(--fs-sm);
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
    user-select: none;
    touch-action: manipulation;
    -webkit-tap-highlight-color: transparent;
  }
  .seg label:hover {
    color: var(--text);
    background: var(--accent-soft);
  }
  .seg label.on {
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .seg input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }
  .seg label:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .theme label {
    padding: 0 7px;
  }

  @media (max-width: 720px) {
    .bar {
      position: static;
      padding: var(--sp-2) var(--sp-3);
      gap: var(--sp-2) var(--sp-3);
    }
    .id {
      flex: 1 1 100%;
      flex-wrap: wrap;
    }
    .reason {
      max-width: none;
      white-space: normal;
    }
    .meta {
      gap: var(--sp-1) var(--sp-4);
    }
    .controls {
      margin-left: 0;
      width: 100%;
      justify-content: space-between;
    }
    .conn {
      order: 3;
      min-width: 0;
    }
    .ranges {
      flex: 1 1 100%;
      overflow-x: auto;
    }
    .ranges label {
      flex: 1 0 auto;
      padding: 0 6px;
    }
  }
</style>
