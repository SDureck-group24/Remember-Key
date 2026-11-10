<!-- Eine Zeile der Eintragsliste. Die ganze Zeile öffnet den Eintrag im Dialog. -->
<script lang="ts">
  import { entryHost, entryInitial, type EntrySummary } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import EntryBadges from "./EntryBadges.svelte";
  import Icon from "./Icon.svelte";

  let {
    entry,
    showFolder = false,
    selected = false,
    onopen,
  }: {
    entry: EntrySummary;
    showFolder?: boolean;
    selected?: boolean;
    onopen: () => void;
  } = $props();

  let host = $derived(entryHost(entry.url));
</script>

<div
  class="row-item"
  class:selected
  draggable="true"
  role="listitem"
  ondragstart={(e) => store.dragStart(e, { kind: "entry", id: entry.id })}
>
  <span class="mark" aria-hidden="true">{entryInitial(entry.title)}</span>

  <div class="main">
    <button class="open" data-entry={entry.id} onclick={onopen}>{entry.title}</button>
    <span class="sub">
      {#if host}<span class="host">{host}</span>{/if}
      {#if showFolder && entry.folderId}
        <span class="folder" title={store.path(entry.folderId).map((f) => f.name).join(" / ")}>
          <Icon name="folder-simple" size={12} />
          {store.folderName(entry.folderId)}
        </span>
      {/if}
      <!-- Ohne Platz für die Spalte rückt der Benutzername hierher. -->
      {#if entry.username}<span class="user-inline">{entry.username}</span>{/if}
    </span>
  </div>

  <span class="user" class:none={!entry.username}>{entry.username || "Kein Benutzername"}</span>

  <EntryBadges {entry} />
</div>

<style>
  .row-item {
    position: relative;
    display: grid;
    grid-template-columns: 32px minmax(0, 1.3fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4) var(--space-3) var(--space-3);
    border-radius: var(--radius-md);
  }
  .row-item:hover,
  .row-item:focus-within {
    background-color: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  /* Zuletzt geöffneter Eintrag bleibt markiert, damit man nach dem Schließen weiß, wo man war. */
  .row-item.selected {
    background-color: var(--color-accent-900);
  }
  .mark {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    display: grid;
    place-items: center;
    font-size: 14px;
    font-weight: var(--font-heading-weight);
    background: var(--color-accent-900);
    color: var(--color-accent-300);
  }
  .selected .mark {
    background: var(--color-accent-800);
    color: var(--color-accent-200);
  }
  .main {
    min-width: 0;
    display: flex;
    flex-direction: column;
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
  /* Die ganze Zeile ist Klickfläche des Öffnen-Knopfs. */
  .open::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: var(--radius-md);
  }
  .open:focus-visible {
    outline: none;
  }
  .open:focus-visible::after {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .sub > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .host {
    flex: 0 1 auto;
  }
  .folder {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 1 auto;
  }
  .user-inline {
    display: none;
  }
  .user {
    font-size: 13px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-item[draggable="true"] {
    cursor: grab;
  }
  .row-item:active {
    cursor: grabbing;
  }

  /* Schmale Liste: Benutzername wandert in die zweite Zeile. */
  @container entries (max-width: 520px) {
    .row-item {
      grid-template-columns: 32px minmax(0, 1fr) auto;
    }
    .user {
      display: none;
    }
    .user-inline {
      display: inline;
      order: -1;
      flex: 0 0 auto;
      max-width: 60%;
    }
    .host,
    .folder {
      flex-shrink: 2;
    }
  }
</style>
