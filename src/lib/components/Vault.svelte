<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, subtree, type CopyField } from "$lib/api";
  import { store, ROOT_DROP } from "$lib/store.svelte";
  import EntryView from "./EntryView.svelte";
  import EntryForm from "./EntryForm.svelte";
  import EntryTile from "./EntryTile.svelte";
  import FolderTree from "./FolderTree.svelte";
  import Generator from "./Generator.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import Icon from "./Icon.svelte";
  import SyncBanner from "./SyncBanner.svelte";
  import SyncIndicator from "./SyncIndicator.svelte";

  let { onlock }: { onlock: () => void } = $props();

  let query = $state("");
  let selectedId = $state<string | null>(null);
  let mode = $state<"home" | "view" | "edit" | "new">("home");
  let showGenerator = $state(false);
  let showSettings = $state(false);
  let searchInput: HTMLInputElement;

  // Ordnerwechsel schließt Detail- und Bearbeitungsansicht.
  $effect(() => {
    store.navigation;
    mode = "home";
    selectedId = null;
  });

  let visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = store.entries;
    if (store.current) {
      list = q
        ? list.filter((e) => e.folderId && subtree(store.folders, store.current!).has(e.folderId))
        : list.filter((e) => e.folderId === store.current);
    }
    if (q) list = list.filter((e) => [e.title, e.username, e.url].some((f) => f.toLowerCase().includes(q)));
    return list;
  });

  let subfolders = $derived(store.current && !query.trim() ? store.childrenOf(store.current) : []);
  let crumbs = $derived(store.path(store.current));

  async function copy(id: string, field: CopyField, label: string) {
    try {
      const secs = await api.copyField(id, field);
      store.notify(`${label} kopiert. Wird in ${secs} s aus der Zwischenablage gelöscht.`);
    } catch (e) {
      store.notify(errorText(e));
    }
  }

  function openEntry(id: string) {
    selectedId = id;
    mode = "view";
  }

  function startNew() {
    selectedId = null;
    mode = "new";
  }

  function home() {
    mode = "home";
    selectedId = null;
  }

  async function saved(id: string) {
    await store.load();
    selectedId = id;
    mode = "view";
    store.notify("Gespeichert");
  }

  async function deleted() {
    home();
    await store.load();
    store.notify("Eintrag gelöscht");
  }

  // Aktivität an das Backend melden (gedrosselt), damit die Auto-Sperre zurückgesetzt wird.
  let lastTouch = 0;
  function activity() {
    const now = Date.now();
    if (now - lastTouch > 15_000) {
      lastTouch = now;
      api.touch().catch(() => {});
    }
  }

  function onKey(e: KeyboardEvent) {
    activity();
    if (showGenerator || showSettings || store.confirmDelete) return;
    if (e.key === "Escape" && mode === "view") {
      home();
      return;
    }
    if (!e.ctrlKey) return;
    const k = e.key.toLowerCase();
    if (k === "l") {
      e.preventDefault();
      onlock();
    } else if (k === "n") {
      e.preventDefault();
      startNew();
    } else if (k === "f") {
      e.preventDefault();
      home();
      searchInput?.focus();
    }
  }

  onMount(() => {
    store.load();
    return () => store.reset();
  });
</script>

<svelte:window onkeydown={onKey} onmousedown={activity} onmousemove={activity} />

<div class="app">
  <aside>
    <div class="brand">
      <Icon name="key" size={18} />
      <span>Remember Key</span>
    </div>

    <nav class="side">
      <button class="all" class:active={store.current === null} onclick={() => store.open(null)}>
        <Icon name="squares-four" size={16} />
        <span class="grow">Alle Einträge</span>
        <span class="n">{store.entries.length}</span>
      </button>

      <div
        class="section-head"
        class:over={store.dragOver === ROOT_DROP}
        role="group"
        aria-label="Ordner (hier ablegen für oberste Ebene)"
        ondragover={(e) => store.dragOverTarget(e, ROOT_DROP)}
        ondragleave={() => store.dragLeave(ROOT_DROP)}
        ondrop={(e) => store.drop(e, null)}
      >
        <h6 class="grow">Ordner</h6>
        <button class="btn btn-icon act" title="Neuer Ordner" aria-label="Neuer Ordner" onclick={() => store.startCreate(null)}>
          <Icon name="folder-simple-plus" size={15} />
        </button>
      </div>

      <div class="tree-wrap" role="tree" aria-label="Ordner">
        <FolderTree />
        {#if store.folders.length === 0 && !store.editing}
          <p class="tree-empty">Noch keine Ordner. Mit <Icon name="folder-simple-plus" size={12} /> anlegen und Einträge per Drag &amp; Drop einsortieren.</p>
        {/if}
      </div>
    </nav>
  </aside>

  <main>
    <header class="nav">
      <div class="search">
        <span class="search-icon"><Icon name="magnifying-glass" size={15} /></span>
        <input
          class="input"
          placeholder={store.current ? `In „${store.folderName(store.current)}“ suchen` : "Alle Einträge durchsuchen"}
          aria-label="Einträge durchsuchen (Strg+F)"
          bind:value={query}
          bind:this={searchInput}
          onfocus={() => mode === "view" && home()}
          spellcheck="false"
        />
      </div>
      <span class="grow"></span>
      <button class="btn btn-primary" onclick={startNew} title="Neuer Eintrag (Strg+N)">
        <Icon name="plus" size={16} /> Neuer Eintrag
      </button>
      <button class="btn btn-secondary" onclick={() => (showGenerator = true)}>
        <Icon name="password" size={16} /> Generator
      </button>
      <SyncIndicator />
      <button class="btn btn-icon btn-secondary" onclick={() => (showSettings = true)} title="Einstellungen" aria-label="Einstellungen">
        <Icon name="gear-six" size={17} />
      </button>
      <button class="btn btn-icon btn-secondary" onclick={onlock} title="Sperren (Strg+L)" aria-label="Sperren">
        <Icon name="lock-simple" size={17} />
      </button>
    </header>

    <SyncBanner />

    <section>
      {#if mode === "new"}
        <div class="center">
          <EntryForm id={null} defaultFolder={store.current} onsaved={saved} oncancel={home} />
        </div>
      {:else if mode === "edit" && selectedId}
        <div class="center">
          <EntryForm id={selectedId} onsaved={saved} oncancel={() => (mode = "view")} />
        </div>
      {:else if mode === "view" && selectedId}
        <div class="center">
          <button class="btn btn-ghost back" onclick={home}>
            <Icon name="arrow-left" size={15} /> Zurück
          </button>
          <EntryView
            id={selectedId}
            oncopy={(field, label) => copy(selectedId!, field, label)}
            onedit={() => (mode = "edit")}
            ondeleted={deleted}
          />
        </div>
      {:else}
        <div class="home-head">
          <nav class="crumbs" aria-label="Pfad">
            <button class="crumb" class:current={!store.current} onclick={() => store.open(null)}>Alle Einträge</button>
            {#each crumbs as c, i (c.id)}
              <Icon name="caret-right" size={12} />
              <button class="crumb" class:current={i === crumbs.length - 1} onclick={() => store.open(c.id)}>{c.name}</button>
            {/each}
          </nav>
          <span class="grow"></span>
          <button class="btn btn-ghost" onclick={() => store.startCreate(store.current)}>
            <Icon name="folder-simple-plus" size={15} />
            {store.current ? "Unterordner" : "Ordner"} anlegen
          </button>
        </div>

        {#if subfolders.length}
          <div class="grid folders" role="list">
            {#each subfolders as f (f.id)}
              <button
                class="folder-tile"
                class:over={store.dragOver === `tile:${f.id}`}
                draggable="true"
                ondragstart={(e) => store.dragStart(e, { kind: "folder", id: f.id })}
                ondragover={(e) => store.dragOverTarget(e, `tile:${f.id}`)}
                ondragleave={() => store.dragLeave(`tile:${f.id}`)}
                ondrop={(e) => store.drop(e, f.id)}
                onclick={() => store.open(f.id)}
              >
                <Icon name="folder-simple" size={18} />
                <span class="grow label">{f.name}</span>
                <span class="n">{store.countIn(f.id)}</span>
              </button>
            {/each}
          </div>
        {/if}

        {#if visible.length}
          <div class="grid" role="list">
            {#each visible as e (e.id)}
              <EntryTile
                entry={e}
                showFolder={store.current === null || !!query.trim()}
                onopen={() => openEntry(e.id)}
                oncopy={(field, label) => copy(e.id, field, label)}
              />
            {/each}
          </div>
        {:else}
          <div class="empty">
            {#if query.trim()}
              <h4>Keine Treffer</h4>
              <p class="text-muted">Für „{query.trim()}“ wurde nichts gefunden.</p>
            {:else if store.current}
              <h4>Dieser Ordner ist leer</h4>
              <p class="text-muted">Ziehen Sie Kacheln hierher in der Seitenleiste oder legen Sie einen neuen Eintrag an.</p>
            {:else}
              <h4>Ihr Tresor ist leer</h4>
              <p class="text-muted">Legen Sie mit „Neuer Eintrag“ Ihren ersten Login an.</p>
            {/if}
            <dl class="keys">
              <dt><kbd>Strg</kbd> <kbd>N</kbd></dt><dd>Neuer Eintrag</dd>
              <dt><kbd>Strg</kbd> <kbd>F</kbd></dt><dd>Suchen</dd>
              <dt><kbd>Strg</kbd> <kbd>L</kbd></dt><dd>Sperren</dd>
            </dl>
          </div>
        {/if}
      {/if}
    </section>
  </main>
</div>

{#if showGenerator}
  <Generator onclose={() => (showGenerator = false)} onnotify={(m) => store.notify(m)} />
{/if}
{#if showSettings}
  <SettingsDialog onclose={() => (showSettings = false)} onnotify={(m) => store.notify(m)} />
{/if}

{#if store.confirmDelete}
  {@const f = store.confirmDelete}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog-backdrop" onclick={(e) => e.target === e.currentTarget && (store.confirmDelete = null)}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="del-title">
      <div class="dialog-title" id="del-title">Ordner „{f.name}“ löschen?</div>
      <div class="dialog-body">
        Die {store.countIn(f.id)} enthaltenen Einträge und alle Unterordner bleiben erhalten und wandern
        {f.parentId ? `nach „${store.folderName(f.parentId)}“` : "auf die oberste Ebene"}.
      </div>
      <div class="dialog-actions">
        <button class="btn btn-ghost" onclick={() => (store.confirmDelete = null)}>Abbrechen</button>
        <button class="btn btn-danger" onclick={() => store.deleteFolder(f)}>
          <Icon name="trash" size={15} /> Ordner löschen
        </button>
      </div>
    </div>
  </div>
{/if}

{#if store.toast}
  <div class="toast elev-md" role="status">
    <Icon name="check" size={15} />
    {store.toast}
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100vh;
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: color-mix(in srgb, var(--color-surface) 45%, var(--color-bg));
    box-shadow: 1px 0 0 var(--color-neutral-900);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-6) var(--space-6);
    color: var(--color-accent);
    font-weight: var(--font-heading-weight);
    font-size: 14px;
  }
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    padding: 0 var(--space-3) var(--space-4);
  }
  .all {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 8px var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .all:hover {
    background: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  .all.active {
    background: var(--color-accent-900);
    color: var(--color-accent-200);
  }
  .all.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 2px;
    border-radius: 1px;
    background: var(--color-accent);
  }
  .n {
    font-size: 11px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .section-head {
    display: flex;
    align-items: center;
    margin-top: var(--space-6);
    padding: 2px 2px 2px var(--space-3);
    border-radius: var(--radius-md);
  }
  .section-head h6 {
    margin: 0;
    color: var(--color-text-muted);
  }
  .section-head.over {
    box-shadow: inset 0 0 0 1px var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }
  .act {
    width: 28px;
    height: 28px;
    color: var(--color-text-muted);
  }
  .act:hover {
    color: var(--color-text);
  }
  .tree-wrap {
    overflow-y: auto;
    flex: 1;
    margin-top: var(--space-1);
  }
  .tree-empty {
    font-size: 12px;
    color: var(--color-text-muted);
    padding: var(--space-2) var(--space-3);
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .nav {
    padding: var(--space-4) var(--space-8);
    gap: var(--space-2);
  }
  .search {
    position: relative;
    width: min(340px, 40%);
  }
  .search .input {
    padding-left: 32px;
  }
  .search-icon {
    position: absolute;
    left: 10px;
    top: 10px;
    color: var(--color-text-muted);
    pointer-events: none;
  }
  section {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4) var(--space-8) var(--space-8);
  }
  .center {
    max-width: 640px;
    margin: 0 auto;
    padding-top: var(--space-4);
  }
  .back {
    margin-bottom: var(--space-4);
    margin-left: calc(-1 * var(--space-1));
  }
  .home-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-6);
    min-height: 36px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1);
    color: var(--color-text-muted);
    min-width: 0;
  }
  .crumb {
    border: none;
    background: none;
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: 20px;
    font-family: var(--font-heading);
    font-weight: var(--font-heading-weight);
    cursor: pointer;
  }
  .crumb:hover {
    color: var(--color-text);
  }
  .crumb.current {
    color: var(--color-text);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: var(--space-4);
  }
  .grid.folders {
    margin-bottom: var(--space-6);
  }
  .folder-tile {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    box-shadow: var(--shadow-sm);
    color: var(--color-text);
    font-size: 14px;
    text-align: left;
    cursor: pointer;
  }
  .folder-tile :global(.icon) {
    color: var(--color-accent);
  }
  .folder-tile:hover {
    background: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  .folder-tile.over {
    box-shadow: inset 0 0 0 1px var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }
  .folder-tile .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    max-width: 440px;
    margin: 10vh auto 0;
  }
  .keys {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-2) var(--space-4);
    margin: var(--space-8) 0 0;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .keys dt,
  .keys dd {
    margin: 0;
  }
  kbd {
    font-family: inherit;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm);
    color: var(--color-neutral-300);
  }
  .toast {
    position: fixed;
    left: 50%;
    transform: translateX(-50%);
    bottom: var(--space-8);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    background: var(--color-surface);
    color: var(--color-text);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    font-size: 13px;
    z-index: 20;
  }
  .toast :global(.icon) {
    color: var(--color-accent);
  }
</style>
