<script lang="ts">
  import type { CopyField, EntrySummary } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let {
    entry,
    showFolder = false,
    onopen,
    oncopy,
  }: {
    entry: EntrySummary;
    showFolder?: boolean;
    onopen: () => void;
    oncopy: (field: CopyField, label: string) => void;
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
  class="tile card"
  draggable="true"
  role="listitem"
  ondragstart={(e) => store.dragStart(e, { kind: "entry", id: entry.id })}
>
  <button class="open" onclick={onopen} title="Öffnen">
    <span class="avatar">{initial}</span>
    <span class="text">
      <span class="title">{entry.title}</span>
      <span class="sub">{entry.username || host || "Kein Benutzername"}</span>
    </span>
  </button>

  <div class="foot">
    {#if showFolder && entry.folderId}
      <span class="folder" title={store.path(entry.folderId).map((f) => f.name).join(" / ")}>
        <Icon name="folder-simple" size={12} />
        <span>{store.folderName(entry.folderId)}</span>
      </span>
    {/if}
    {#if entry.hasTotp}<span class="tag tag-accent">2FA</span>{/if}
    <span class="grow"></span>
    {#if entry.username}
      <button class="btn btn-icon btn-sm" title="Benutzername kopieren" aria-label="Benutzername kopieren" onclick={() => oncopy("username", "Benutzername")}>
        <Icon name="user" size={15} />
      </button>
    {/if}
    <button class="btn btn-icon btn-sm" title="Passwort kopieren" aria-label="Passwort kopieren" onclick={() => oncopy("password", "Passwort")}>
      <Icon name="copy" size={15} />
    </button>
  </div>
</div>

<style>
  .tile {
    padding: 0;
    gap: 0;
    transition: box-shadow 0.15s;
    cursor: grab;
  }
  .tile:hover {
    box-shadow: var(--shadow-md);
  }
  .tile:active {
    cursor: grabbing;
  }
  .open {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
    border: none;
    background: none;
    color: var(--color-text);
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    min-width: 0;
  }
  .avatar {
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: var(--radius-md);
    display: grid;
    place-items: center;
    font-size: 15px;
    font-weight: var(--font-heading-weight);
    background: var(--color-accent-900);
    color: var(--color-accent-300);
  }
  .text {
    min-width: 0;
  }
  .title,
  .sub {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-size: 14px;
    font-weight: var(--font-heading-weight);
  }
  .sub {
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2) var(--space-2) var(--space-4);
    min-height: 34px;
  }
  .folder {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--color-text-muted);
    min-width: 0;
    max-width: 50%;
  }
  .folder span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot .btn {
    color: var(--color-text-muted);
  }
  .foot .btn:hover {
    color: var(--color-text);
  }
  @media (prefers-reduced-motion: reduce) {
    .tile {
      transition: none;
    }
  }
</style>
