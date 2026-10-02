<script lang="ts">
  import { store } from "$lib/store.svelte";
  import FolderTree from "./FolderTree.svelte";
  import FolderNameInput from "./FolderNameInput.svelte";
  import Icon from "./Icon.svelte";

  let { parentId = null, depth = 0 }: { parentId?: string | null; depth?: number } = $props();

  let children = $derived(store.childrenOf(parentId));
  let creatingHere = $derived(store.editing?.mode === "create" && store.editing.parentId === parentId);
</script>

<ul class="tree">
  {#each children as f (f.id)}
    {@const hasKids = store.childrenOf(f.id).length > 0}
    {@const open = !!store.expanded[f.id]}
    <li>
      {#if store.editing?.mode === "rename" && store.editing.id === f.id}
        <FolderNameInput initial={f.name} {depth} />
      {:else}
        <div
          class="node"
          class:active={store.current === f.id}
          class:over={store.dragOver === f.id}
          style="--depth: {depth}"
          draggable="true"
          role="treeitem"
          aria-selected={store.current === f.id}
          aria-expanded={hasKids ? open : undefined}
          tabindex="-1"
          ondragstart={(e) => store.dragStart(e, { kind: "folder", id: f.id })}
          ondragover={(e) => store.dragOverTarget(e, f.id)}
          ondragleave={() => store.dragLeave(f.id)}
          ondrop={(e) => store.drop(e, f.id)}
        >
          <button
            class="caret"
            class:hidden={!hasKids}
            aria-label={open ? "Zuklappen" : "Aufklappen"}
            onclick={() => store.toggle(f.id)}
            tabindex={hasKids ? 0 : -1}
          >
            <Icon name={open ? "caret-down" : "caret-right"} size={12} />
          </button>
          <button class="name" onclick={() => store.open(f.id)} ondblclick={() => store.startRename(f.id)}>
            <Icon name={store.current === f.id ? "folder-open" : "folder-simple"} size={15} />
            <span class="label">{f.name}</span>
            <span class="n">{store.countIn(f.id)}</span>
          </button>
          <span class="acts">
            <button class="btn btn-icon act" title="Unterordner anlegen" aria-label="Unterordner anlegen" onclick={() => store.startCreate(f.id)}>
              <Icon name="plus" size={13} />
            </button>
            <button class="btn btn-icon act" title="Umbenennen" aria-label="Umbenennen" onclick={() => store.startRename(f.id)}>
              <Icon name="pencil-simple" size={13} />
            </button>
            <button class="btn btn-icon act" title="Löschen" aria-label="Löschen" onclick={() => (store.confirmDelete = f)}>
              <Icon name="trash" size={13} />
            </button>
          </span>
        </div>
      {/if}

      {#if open || (store.editing?.mode === "create" && store.editing.parentId === f.id)}
        <FolderTree parentId={f.id} depth={depth + 1} />
      {/if}
    </li>
  {/each}

  {#if creatingHere}
    <li><FolderNameInput {depth} /></li>
  {/if}
</ul>

<style>
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .node {
    position: relative;
    display: flex;
    align-items: center;
    padding-left: calc(var(--depth) * 14px);
    border-radius: var(--radius-md);
  }
  .node:hover {
    background: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  .node.active {
    background: var(--color-accent-900);
  }
  .node.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 2px;
    border-radius: 1px;
    background: var(--color-accent);
  }
  .node.over {
    box-shadow: inset 0 0 0 1px var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }
  .caret {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 30px;
    padding: 0;
    border: none;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .caret.hidden {
    visibility: hidden;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 6px var(--space-2) 6px 0;
    border: none;
    background: none;
    color: var(--color-text);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .node.active .name {
    color: var(--color-accent-200);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    font-size: 11px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .acts {
    display: none;
    padding-right: 4px;
  }
  .node:hover .acts,
  .node:focus-within .acts {
    display: flex;
  }
  .node:hover .n,
  .node:focus-within .n {
    display: none;
  }
  .act {
    width: 24px;
    height: 24px;
    color: var(--color-text-muted);
  }
  .act:hover {
    color: var(--color-text);
  }
</style>
