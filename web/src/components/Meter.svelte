<script lang="ts">
  // Horizontal bar for a part of a whole. Values above 1 show as full with an over-limit mark.
  let { value, label, tone = 'none' }: { value: number | null; label: string; tone?: 'none' | 'warn' | 'crit' } = $props();
  const pct = $derived(value === null ? 0 : Math.max(0, Math.min(1, value)) * 100);
  const over = $derived(value !== null && value > 1);
</script>

<div
  class="meter t-{tone}"
  class:over
  role="meter"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={value === null ? undefined : Math.round(value * 100)}
>
  <span style="width: {pct}%"></span>
</div>

<style>
  .meter {
    height: 6px;
    border-radius: 3px;
    background: var(--raised);
    overflow: hidden;
    position: relative;
  }
  span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--s1);
  }
  .t-warn span {
    background: var(--degraded);
  }
  .t-crit span,
  .over span {
    background: var(--down);
  }
</style>
