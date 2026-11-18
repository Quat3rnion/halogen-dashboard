<script lang="ts">
  // One movable block of the page. In Arrange mode it shows a bar to move, size and hide it.
  import type { Snippet } from 'svelte';
  import { layout, WIDTHS, type Block } from '../lib/layout.svelte';

  let { block, first, last, children }: { block: Block; first: boolean; last: boolean; children: Snippet } = $props();

  let over = $state(false);

  function onDragStart(e: DragEvent): void {
    e.dataTransfer?.setData('text/plain', block.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDrop(e: DragEvent): void {
    e.preventDefault();
    over = false;
    const id = e.dataTransfer?.getData('text/plain');
    if (id) layout.moveTo(id, block.id);
  }
</script>

{#if layout.arranging || !block.hidden}
  <div
    class="block w{block.width}"
    class:arranging={layout.arranging}
    class:hidden={block.hidden}
    class:over
    role="group"
    aria-label={layout.arranging ? block.title : undefined}
    ondragover={(e) => {
      if (!layout.arranging) return;
      e.preventDefault();
      over = true;
    }}
    ondragleave={() => (over = false)}
    ondrop={onDrop}
  >
    {#if layout.arranging}
      <div class="abar">
        <span class="grip" draggable="true" ondragstart={onDragStart} role="img" aria-label="Drag to move" title="Drag to move">⠿</span>
        <span class="aname">{block.title}</span>
        <span class="actions">
          <button type="button" disabled={first} onclick={() => layout.step(block.id, -1)} aria-label="Move {block.title} earlier" title="Move earlier">←</button>
          <button type="button" disabled={last} onclick={() => layout.step(block.id, 1)} aria-label="Move {block.title} later" title="Move later">→</button>
          <span class="sizes" role="group" aria-label="Width of {block.title}">
            {#each WIDTHS as x (x.w)}
              <button type="button" class:on={block.width === x.w} aria-pressed={block.width === x.w} title={x.name} aria-label={x.name} onclick={() => layout.setWidth(block.id, x.w)}>{x.label}</button>
            {/each}
          </span>
          <button type="button" class="vis" aria-pressed={!block.hidden} onclick={() => layout.toggle(block.id)}>{block.hidden ? 'Show' : 'Hide'}</button>
        </span>
      </div>
    {/if}
    <div class="body" inert={layout.arranging && block.hidden}>
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .block {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .w3 {
    grid-column: span 3;
  }
  .w4 {
    grid-column: span 4;
  }
  .w6 {
    grid-column: span 6;
  }
  .w12 {
    grid-column: span 12;
  }
  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
  }
  .body > :global(*) {
    flex: 1;
  }
  .arranging {
    outline: 1px dashed var(--line);
    outline-offset: 2px;
    border-radius: var(--r-card);
  }
  .over {
    outline: 2px solid var(--accent);
  }
  .hidden .body {
    opacity: 0.35;
  }
  .abar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 0 0 var(--sp-1);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .grip {
    cursor: grab;
    font-size: var(--fs-md);
    line-height: 1;
    padding: 2px 4px;
    border-radius: 4px;
    color: var(--text);
    background: var(--raised);
  }
  .aname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: inline-flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .sizes {
    display: inline-flex;
    gap: 2px;
  }
  button {
    font: inherit;
    font-size: var(--fs-xs);
    min-height: 24px;
    min-width: 26px;
    padding: 0 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--accent-soft);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  button.on {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  @media (max-width: 1280px) {
    .w3,
    .w4 {
      grid-column: span 6;
    }
    .w6 {
      grid-column: span 12;
    }
  }
  @media (max-width: 720px) {
    .block {
      grid-column: span 12;
    }
  }
</style>
