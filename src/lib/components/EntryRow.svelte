<!-- Eine Zeile der Eintragsliste. Die ganze Zeile öffnet den Eintrag; die Kopier-Knöpfe
     liegen darüber und bleiben eigene Tab-Stopps. -->
<script lang="ts">
  import type { CopyField, EntrySummary } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let {
    entry,
    showFolder = false,
    onopen,
    oncopy,
    onvisit,
  }: {
    entry: EntrySummary;
    showFolder?: boolean;
    onopen: () => void;
    oncopy: (field: CopyField, label: string) => void;
    onvisit: () => void;
  } = $props();

  let initial = $derived([...entry.title.trim()][0]?.toUpperCase() ?? "?");
  let host = $derived.by(() => {
    try {
      return entry.url ? new URL(entry.url.includes("://") ? entry.url : `https://${entry.url}`).hostname : "";
    } catch {
      return entry.url;
    }
  });
</script>

<div
  class="row-item"
  draggable="true"
  role="listitem"
  ondragstart={(e) => store.dragStart(e, { kind: "entry", id: entry.id })}
>
  <span class="mark" aria-hidden="true">{initial}</span>

  <div class="main">
    <button class="open" onclick={onopen}>{entry.title}</button>
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

  <span class="tags">
    {#if entry.hasTotp}<span class="tag tag-accent" title="Mit Zwei-Faktor-Code">2FA</span>{/if}
    {#if entry.agent}<span class="tag tag-neutral" title="Für KI-Assistenten freigegeben">KI</span>{/if}
  </span>

  <span class="actions">
    {#if entry.username}
      <button class="btn btn-icon btn-sm" title="Benutzername kopieren" aria-label="Benutzername von {entry.title} kopieren" onclick={() => oncopy("username", "Benutzername")}>
        <Icon name="user" size={15} />
      </button>
    {/if}
    <button class="btn btn-icon btn-sm" title="Passwort kopieren" aria-label="Passwort von {entry.title} kopieren" onclick={() => oncopy("password", "Passwort")}>
      <Icon name="copy" size={15} />
    </button>
    {#if entry.url}
      <button class="btn btn-icon btn-sm" title="Website aufrufen" aria-label="Website von {entry.title} aufrufen" onclick={onvisit}>
        <Icon name="arrow-square-out" size={15} />
      </button>
    {/if}
  </span>
</div>

<style>
  .row-item {
    position: relative;
    display: grid;
    grid-template-columns: 28px minmax(0, 1.3fr) minmax(0, 1fr) 76px 98px;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-2) var(--space-3) var(--space-3);
    border-radius: var(--radius-md);
    /* Zeilenlinie wie .table in Nocturne: blendet an beiden Enden aus. */
    background: linear-gradient(
        to right,
        transparent,
        color-mix(in srgb, var(--color-text) 8%, transparent) 48px,
        color-mix(in srgb, var(--color-text) 8%, transparent) calc(100% - 48px),
        transparent
      )
      no-repeat bottom / 100% 1px;
  }
  .row-item:hover,
  .row-item:focus-within {
    background-color: color-mix(in srgb, var(--color-text) 4%, transparent);
  }
  .mark {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    display: grid;
    place-items: center;
    font-size: 13px;
    font-weight: var(--font-heading-weight);
    background: var(--color-accent-900);
    color: var(--color-accent-300);
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .user.none {
    color: var(--color-text-muted);
  }
  .tags {
    display: flex;
    gap: var(--space-2);
  }
  .actions {
    position: relative;
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }
  .actions .btn {
    color: var(--color-text-muted);
  }
  .actions .btn:hover,
  .actions .btn:focus-visible {
    color: var(--color-text);
  }
  .row-item[draggable="true"] {
    cursor: grab;
  }
  .row-item:active {
    cursor: grabbing;
  }

  /* Schmale Liste: Benutzername wandert in die zweite Zeile. */
  @container entries (max-width: 560px) {
    .row-item {
      grid-template-columns: 28px minmax(0, 1fr) auto 98px;
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
