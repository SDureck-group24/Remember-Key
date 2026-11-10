<!-- Eine Kachel der Rasteransicht. Öffnet den Eintrag im Dialog. -->
<script lang="ts">
  import { entryHost, entryInitial, type EntrySummary } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import EntryBadges from "./EntryBadges.svelte";

  let {
    entry,
    selected = false,
    onopen,
  }: {
    entry: EntrySummary;
    selected?: boolean;
    onopen: () => void;
  } = $props();
</script>

<div class="tile" class:selected role="listitem" draggable="true" ondragstart={(e) => store.dragStart(e, { kind: "entry", id: entry.id })}>
  <div class="top">
    <span class="mark" aria-hidden="true">{entryInitial(entry.title)}</span>
    <EntryBadges {entry} />
  </div>
  <button class="open" data-entry={entry.id} onclick={onopen}>{entry.title}</button>
  <span class="host">{entryHost(entry.url) || entry.username || " "}</span>
</div>

<style>
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    cursor: grab;
  }
  .tile:hover,
  .tile:focus-within {
    background: color-mix(in srgb, var(--color-text) 6%, var(--color-surface));
  }
  .tile.selected {
    box-shadow: inset 0 0 0 1.5px var(--color-accent);
  }
  .tile:active {
    cursor: grabbing;
  }
  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    margin-bottom: var(--space-3);
  }
  .mark {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-lg);
    display: grid;
    place-items: center;
    font-size: 18px;
    font-weight: var(--font-heading-weight);
    background: var(--color-accent-900);
    color: var(--color-accent-300);
  }
  .open {
    padding: 0;
    border: none;
    background: none;
    color: var(--color-text);
    font: inherit;
    font-size: 14px;
    font-weight: var(--font-heading-weight);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Die ganze Kachel ist Klickfläche des Öffnen-Knopfs. */
  .open::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: var(--radius-lg);
  }
  .open:focus-visible {
    outline: none;
  }
  .open:focus-visible::after {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
  .host {
    font-size: 12px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
