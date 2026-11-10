<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, subtree, type CopyField } from "$lib/api";
  import { store, ROOT_DROP } from "$lib/store.svelte";
  import EntryView from "./EntryView.svelte";
  import EntryForm from "./EntryForm.svelte";
  import EntryRow from "./EntryRow.svelte";
  import EntryTile from "./EntryTile.svelte";
  import FolderTree from "./FolderTree.svelte";
  import Generator from "./Generator.svelte";
  import SettingsDialog from "./SettingsDialog.svelte";
  import Icon from "./Icon.svelte";
  import SyncBanner from "./SyncBanner.svelte";
  import SyncIndicator from "./SyncIndicator.svelte";

  let { onlock }: { onlock: () => void } = $props();

  const VIEW_KEY = "rk.view";
  type ViewMode = "list" | "grid";

  let query = $state("");
  let selectedId = $state<string | null>(null);
  /** Eintragsdialog; `null` = geschlossen. */
  let dialog = $state<"view" | "edit" | "new" | null>(null);
  let view = $state<ViewMode>(readView());
  let showSidebar = $state(true);
  let showGenerator = $state(false);
  let showSettings = $state(false);
  let searchInput: HTMLInputElement;
  let listEl = $state<HTMLElement>();
  let dialogEl = $state<HTMLElement>();

  // Nur eine Anzeige-Vorliebe dieses Geräts, deshalb im WebView-Speicher statt in den Tresor-Einstellungen.
  function readView(): ViewMode {
    try {
      return localStorage.getItem(VIEW_KEY) === "grid" ? "grid" : "list";
    } catch {
      return "list";
    }
  }

  function setView(v: ViewMode) {
    view = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      // Nicht speicherbar: Die Wahl gilt nur bis zum Neustart.
    }
  }

  // Ordner- oder Filterwechsel schließt den Dialog.
  $effect(() => {
    store.navigation;
    dialog = null;
    selectedId = null;
  });

  let visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = store.entries;
    if (store.filter === "totp") list = list.filter((e) => e.hasTotp);
    else if (store.filter === "agent") list = list.filter((e) => e.agent);
    else if (store.current) {
      list = q
        ? list.filter((e) => e.folderId && subtree(store.folders, store.current!).has(e.folderId))
        : list.filter((e) => e.folderId === store.current);
    }
    if (q) list = list.filter((e) => [e.title, e.username, e.url].some((f) => f.toLowerCase().includes(q)));
    return list;
  });

  let scopeTitle = $derived(
    store.filter === "totp"
      ? "Mit 2FA-Code"
      : store.filter === "agent"
        ? "Für KI freigegeben"
        : store.current
          ? store.folderName(store.current)
          : "Alle Einträge",
  );
  let showFolder = $derived(store.current === null || !!query.trim());

  async function copy(id: string, field: CopyField, label: string) {
    try {
      const secs = await api.copyField(id, field);
      store.notify(`${label} kopiert. Wird in ${secs} s aus der Zwischenablage gelöscht.`);
    } catch (e) {
      store.fail(e);
    }
  }

  async function visit(id: string) {
    try {
      if (await api.openUrl(id)) store.notify("Website geöffnet. Das Login wird ausgefüllt, sobald die Seite geladen ist.");
    } catch (e) {
      store.fail(e);
    }
  }

  async function openDialog(mode: "view" | "new", id: string | null = null) {
    selectedId = id;
    dialog = mode;
    await tick();
    // Das Formular setzt den Fokus selbst ins Titelfeld; nur sonst den Dialog fokussieren.
    if (!dialogEl?.contains(document.activeElement)) dialogEl?.focus();
  }

  async function closeDialog() {
    dialog = null;
    await tick();
    if (selectedId) listEl?.querySelector<HTMLElement>(`[data-entry="${selectedId}"]`)?.focus();
  }

  async function saved(id: string) {
    await store.load();
    selectedId = id;
    dialog = "view";
    store.notify("Gespeichert");
  }

  async function deleted() {
    dialog = null;
    selectedId = null;
    await store.load();
    store.notify("Eintrag gelöscht");
  }

  // Pfeiltasten wandern durch Liste bzw. Raster; Enter öffnet (nativer Knopf).
  function onListKey(e: KeyboardEvent) {
    if (!listEl || !["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"].includes(e.key)) return;
    const items = [...listEl.querySelectorAll<HTMLElement>("[data-entry]")];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    const grid = listEl.querySelector(".tiles");
    const cols = grid ? getComputedStyle(grid).gridTemplateColumns.split(" ").length : 1;
    const steps: Record<string, number> = { ArrowDown: cols, ArrowUp: -cols, ArrowRight: grid ? 1 : 0, ArrowLeft: grid ? -1 : 0 };
    const next = steps[e.key] ? items[i + steps[e.key]] : undefined;
    if (!next) return;
    e.preventDefault();
    next.focus();
    next.scrollIntoView({ block: "nearest" });
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
    // Beim Bearbeiten schließt Esc nicht, damit keine Eingaben verloren gehen.
    if (e.key === "Escape" && dialog === "view") {
      closeDialog();
      return;
    }
    if (!e.ctrlKey) return;
    const k = e.key.toLowerCase();
    if (k === "l") {
      e.preventDefault();
      onlock();
    } else if (dialog === "edit" || dialog === "new") {
      return;
    } else if (k === "n") {
      e.preventDefault();
      openDialog("new");
    } else if (k === "f") {
      e.preventDefault();
      dialog = null;
      searchInput?.focus();
    }
  }

  onMount(() => {
    store.load();
    const unlisten = listen<{ ok: boolean; title?: string; submitted?: boolean; message?: string }>("visit-fill", (e) => {
      const r = e.payload;
      if (r.ok) store.notify(r.submitted ? `Login für „${r.title}“ ausgefüllt und abgesendet` : `Login für „${r.title}“ ausgefüllt`);
      else store.notify(`Login nicht ausgefüllt: ${r.message}`);
    });
    return () => {
      store.reset();
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={onKey} onmousedown={activity} onmousemove={activity} />

<div class="app" class:no-side={!showSidebar}>
  {#if showSidebar}
    <aside>
      <div class="brand">
        <Icon name="logo" size={20} />
        <span>Remember Key</span>
      </div>

      <nav class="side">
        <button class="all" class:active={store.current === null && !store.filter} onclick={() => store.open(null)}>
          <Icon name="squares-four" size={16} />
          <span class="grow">Alle Einträge</span>
          <span class="n">{store.entries.length}</span>
        </button>
        <button class="all" class:active={store.filter === "totp"} onclick={() => store.showFilter("totp")}>
          <Icon name="clock-countdown" size={16} />
          <span class="grow">Mit 2FA-Code</span>
          <span class="n">{store.entries.filter((e) => e.hasTotp).length}</span>
        </button>
        <button class="all" class:active={store.filter === "agent"} onclick={() => store.showFilter("agent")}>
          <Icon name="sparkle" size={16} />
          <span class="grow">Für KI freigegeben</span>
          <span class="n">{store.entries.filter((e) => e.agent).length}</span>
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

      <div class="side-foot">
        <button class="all" onclick={() => (showSettings = true)}>
          <Icon name="gear-six" size={16} />
          <span class="grow">Einstellungen</span>
        </button>
      </div>
    </aside>
  {/if}

  <main>
    <header class="nav">
      <button
        class="btn btn-icon side-toggle"
        onclick={() => (showSidebar = !showSidebar)}
        title={showSidebar ? "Seitenleiste ausblenden" : "Seitenleiste einblenden"}
        aria-label={showSidebar ? "Seitenleiste ausblenden" : "Seitenleiste einblenden"}
        aria-pressed={showSidebar}
      >
        <Icon name="sidebar-simple" size={18} />
      </button>
      <div class="scope">
        <span class="scope-title">{scopeTitle}</span>
        {#if store.loaded}<span class="scope-n">{visible.length} {visible.length === 1 ? "Eintrag" : "Einträge"}</span>{/if}
      </div>
      <span class="grow"></span>
      <div class="view-switch" role="group" aria-label="Darstellung">
        <button aria-pressed={view === "list"} onclick={() => setView("list")} title="Als Liste">
          <Icon name="list-bullets" size={15} /><span class="label">Liste</span>
        </button>
        <button aria-pressed={view === "grid"} onclick={() => setView("grid")} title="Als Raster">
          <Icon name="squares-four" size={15} /><span class="label">Raster</span>
        </button>
      </div>
      <div class="search">
        <span class="search-icon"><Icon name="magnifying-glass" size={15} /></span>
        <input
          class="input"
          placeholder="Suchen"
          aria-label="Einträge durchsuchen (Strg+F)"
          bind:value={query}
          bind:this={searchInput}
          spellcheck="false"
        />
      </div>
      <button class="btn btn-icon btn-primary" onclick={() => openDialog("new")} title="Neuer Eintrag (Strg+N)" aria-label="Neuer Eintrag">
        <Icon name="plus" size={17} />
      </button>
      <button class="btn btn-icon btn-secondary" onclick={() => (showGenerator = true)} title="Passwort-Generator" aria-label="Passwort-Generator">
        <Icon name="password" size={17} />
      </button>
      <SyncIndicator />
      <button class="btn btn-icon btn-secondary" onclick={onlock} title="Sperren (Strg+L)" aria-label="Sperren">
        <Icon name="lock-simple" size={17} />
      </button>
    </header>

    <SyncBanner />

    <!-- Tastendrücke der Einträge darin werden hier gesammelt ausgewertet. -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <section bind:this={listEl} onkeydown={onListKey} aria-label="Einträge">
      {#if !store.loaded}
        <p class="loading text-muted" role="status">Einträge werden geladen …</p>
      {:else if store.loadError}
        <div class="empty" role="alert">
          <h4>Einträge konnten nicht geladen werden</h4>
          <p class="error"><Icon name="warning-circle" size={16} /> {store.loadError}</p>
          <button class="btn btn-secondary" onclick={() => store.load()}>
            <Icon name="arrows-clockwise" size={15} /> Erneut laden
          </button>
        </div>
      {:else if visible.length}
        {#if view === "grid"}
          <div class="tiles" role="list" aria-label="Einträge">
            {#each visible as e (e.id)}
              <EntryTile entry={e} selected={e.id === selectedId} onopen={() => openDialog("view", e.id)} />
            {/each}
          </div>
        {:else}
          <div class="list" role="list" aria-label="Einträge">
            {#each visible as e (e.id)}
              <EntryRow entry={e} {showFolder} selected={e.id === selectedId} onopen={() => openDialog("view", e.id)} />
            {/each}
          </div>
        {/if}
      {:else}
        <div class="empty">
          {#if query.trim()}
            <h4>Keine Treffer</h4>
            <p class="text-muted">Für „{query.trim()}“ wurde nichts gefunden.</p>
          {:else if store.filter}
            <h4>Keine Einträge</h4>
            <p class="text-muted">Kein Eintrag passt zu „{scopeTitle}“.</p>
          {:else if store.current}
            <h4>Dieser Ordner ist leer</h4>
            <p class="text-muted">Ziehen Sie Einträge auf diesen Ordner in der Seitenleiste oder legen Sie hier einen neuen Eintrag an.</p>
          {:else}
            <h4>Ihr Tresor ist leer</h4>
            <p class="text-muted">Legen Sie mit <Icon name="plus" size={12} /> Ihren ersten Login an.</p>
          {/if}
          <dl class="keys">
            <dt><kbd>Strg</kbd> <kbd>N</kbd></dt><dd>Neuer Eintrag</dd>
            <dt><kbd>Strg</kbd> <kbd>F</kbd></dt><dd>Suchen</dd>
            <dt><kbd>Strg</kbd> <kbd>L</kbd></dt><dd>Sperren</dd>
          </dl>
        </div>
      {/if}
    </section>
  </main>
</div>

{#if dialog}
  <!-- Beim Bearbeiten schließt ein Klick daneben nicht, damit keine Eingaben verloren gehen. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog-backdrop" onclick={(e) => e.target === e.currentTarget && dialog === "view" && closeDialog()}>
    <div
      class="entry-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={dialog === "new" ? "Neuer Eintrag" : dialog === "edit" ? "Eintrag bearbeiten" : "Eintrag"}
      tabindex="-1"
      bind:this={dialogEl}
    >
      {#if dialog === "view"}
        <button class="btn btn-icon close" onclick={closeDialog} title="Schließen (Esc)" aria-label="Schließen">
          <Icon name="x" size={17} />
        </button>
      {/if}
      <div class="dialog-scroll">
        {#if dialog === "new"}
          <EntryForm id={null} defaultFolder={store.current} onsaved={saved} oncancel={closeDialog} />
        {:else if dialog === "edit" && selectedId}
          <EntryForm id={selectedId} onsaved={saved} oncancel={() => (dialog = "view")} />
        {:else if selectedId}
          <EntryView
            id={selectedId}
            oncopy={(field, label) => copy(selectedId!, field, label)}
            onvisit={() => visit(selectedId!)}
            onedit={() => (dialog = "edit")}
            ondeleted={deleted}
          />
        {/if}
      </div>
    </div>
  </div>
{/if}

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
  <div class="toast elev-md" class:failed={store.toastError} role={store.toastError ? "alert" : "status"}>
    <Icon name={store.toastError ? "warning-circle" : "check"} size={15} />
    {store.toast}
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 248px 1fr;
    height: 100vh;
  }
  .app.no-side {
    grid-template-columns: 1fr;
  }
  /* Seitenleiste als schwebende Fläche über dem Fensterhintergrund. */
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: var(--space-3) 0 var(--space-3) var(--space-3);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface) 70%, var(--color-bg));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-text) 8%, transparent);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-6) var(--space-4);
    color: var(--color-accent);
    font-weight: var(--font-heading-weight);
    font-size: 14px;
  }
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    padding: 0 var(--space-3) var(--space-3);
  }
  .side-foot {
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid color-mix(in srgb, var(--color-text) 8%, transparent);
  }
  .all {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 7px var(--space-3);
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
  .all :global(.icon) {
    color: var(--color-accent);
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
    padding: var(--space-4) var(--space-6);
    gap: var(--space-2);
  }
  .nav .btn {
    flex: none;
  }
  .side-toggle {
    color: var(--color-text-muted);
  }
  .side-toggle:hover {
    color: var(--color-text);
  }
  .scope {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }
  .scope-title {
    font-family: var(--font-heading);
    font-weight: var(--font-heading-weight);
    font-size: 18px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scope-n {
    flex: none;
    font-size: 12px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .view-switch {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    box-shadow: inset 0 0 0 1px var(--color-divider);
  }
  .view-switch button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border: none;
    border-radius: calc(var(--radius-md) - 2px);
    background: none;
    color: var(--color-text-muted);
    font-size: 13px;
    cursor: pointer;
  }
  .view-switch button:hover {
    color: var(--color-text);
  }
  .view-switch button[aria-pressed="true"] {
    background: color-mix(in srgb, var(--color-text) 10%, transparent);
    color: var(--color-text);
  }
  .search {
    position: relative;
    flex: 0 1 220px;
    min-width: 120px;
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
    padding: var(--space-2) var(--space-6) var(--space-8);
  }
  .list {
    container: entries / inline-size;
    margin-inline: calc(var(--space-3) * -1);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--space-4);
    padding-top: var(--space-2);
  }
  .empty {
    max-width: 440px;
    margin: 8vh 0 0;
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
  /* Eintragsdialog: nie höher als das Fenster; gescrollt wird innen, mit Abstand zu den runden Ecken. */
  .entry-dialog {
    position: relative;
    display: flex;
    width: min(600px, 100%);
    max-height: calc(100vh - 2 * var(--space-4));
    padding: var(--space-4) var(--space-2) var(--space-4) 0;
    border-radius: var(--radius-lg);
    background: var(--color-bg);
    box-shadow: var(--shadow-lg);
  }
  .entry-dialog:focus {
    outline: none;
  }
  .dialog-scroll {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-2) var(--space-4) var(--space-2) var(--space-8);
  }
  .close {
    position: absolute;
    top: var(--space-4);
    right: var(--space-4);
    z-index: 1;
    color: var(--color-text-muted);
  }
  .close:hover {
    color: var(--color-text);
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
  .toast.failed :global(.icon) {
    color: var(--color-danger);
  }
  .loading {
    font-size: 13px;
  }
  /* Schmales Fenster: Umschalter nur mit Symbolen. */
  @media (max-width: 1000px) {
    .view-switch .label {
      display: none;
    }
  }
</style>
