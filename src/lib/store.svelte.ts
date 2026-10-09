// Gemeinsamer UI-Zustand des entsperrten Tresors (Einträge, Ordner, Drag & Drop, Meldungen).
import {
  api,
  DND_TYPE,
  errorText,
  folderPath,
  subtree,
  type DragItem,
  type EntrySummary,
  type Folder,
  type SyncStatus,
} from "$lib/api";

export const ROOT_DROP = "__root__";

type Editing = { mode: "rename"; id: string } | { mode: "create"; parentId: string | null } | null;

class VaultStore {
  entries = $state<EntrySummary[]>([]);
  folders = $state<Folder[]>([]);
  /** Geöffneter Ordner; `null` = alle Einträge. */
  current = $state<string | null>(null);
  expanded = $state<Record<string, boolean>>({});
  editing = $state<Editing>(null);
  confirmDelete = $state<Folder | null>(null);
  dragOver = $state<string | null>(null);
  toast = $state("");
  toastError = $state(false);
  /** Erst nach dem ersten Laden true; bis dahin zeigt die Ansicht keinen Leerzustand. */
  loaded = $state(false);
  loadError = $state("");
  sync = $state<SyncStatus | null>(null);
  /** Erhöht sich bei jedem Ordnerwechsel, damit die Ansicht Details schließen kann. */
  navigation = $state(0);
  #toastTimer: ReturnType<typeof setTimeout> | undefined;

  async load() {
    try {
      const [entries, folders] = await Promise.all([api.list(), api.folders()]);
      this.entries = entries;
      this.folders = folders;
      if (this.current && !folders.some((f) => f.id === this.current)) this.current = null;
      this.loadError = "";
    } catch (e) {
      this.loadError = errorText(e);
    } finally {
      this.loaded = true;
    }
  }

  reset() {
    this.entries = [];
    this.folders = [];
    this.current = null;
    this.editing = null;
    this.confirmDelete = null;
    this.loaded = false;
    this.loadError = "";
  }

  notify(msg: string) {
    this.toast = msg;
    this.toastError = false;
    this.#showToast();
  }

  /** Fehlermeldung im selben Toast, aber mit Warnsymbol und Fehlerton statt Häkchen. */
  fail(e: unknown) {
    this.toast = errorText(e);
    this.toastError = true;
    this.#showToast();
  }

  #showToast() {
    clearTimeout(this.#toastTimer);
    this.#toastTimer = setTimeout(() => (this.toast = ""), 3500);
  }

  childrenOf(parent: string | null) {
    return this.folders.filter((f) => f.parentId === parent);
  }

  folderName(id: string | null) {
    return id ? (this.folders.find((f) => f.id === id)?.name ?? "") : "";
  }

  path(id: string | null) {
    return folderPath(this.folders, id);
  }

  /** Anzahl Einträge im Ordner inklusive aller Unterordner. */
  countIn(id: string) {
    const ids = subtree(this.folders, id);
    return this.entries.filter((e) => e.folderId && ids.has(e.folderId)).length;
  }

  open(id: string | null) {
    this.current = id;
    this.navigation++;
    // Pfad im Baum aufklappen, damit der aktive Ordner sichtbar ist.
    for (const f of this.path(id).slice(0, -1)) this.expanded[f.id] = true;
  }

  toggle(id: string) {
    this.expanded[id] = !this.expanded[id];
  }

  startCreate(parentId: string | null) {
    if (parentId) this.expanded[parentId] = true;
    this.editing = { mode: "create", parentId };
  }

  startRename(id: string) {
    this.editing = { mode: "rename", id };
  }

  cancelEdit() {
    this.editing = null;
  }

  async commitName(name: string) {
    const edit = this.editing;
    this.editing = null;
    if (!edit || !name.trim()) return;
    try {
      if (edit.mode === "create") {
        const id = await api.saveFolder({ id: null, name, parentId: edit.parentId });
        await this.load();
        this.notify(`Ordner „${name.trim()}“ angelegt`);
        this.open(id);
      } else {
        const f = this.folders.find((f) => f.id === edit.id);
        if (!f || f.name === name.trim()) return;
        await api.saveFolder({ id: f.id, name, parentId: f.parentId });
        await this.load();
      }
    } catch (e) {
      this.fail(e);
    }
  }

  async deleteFolder(f: Folder) {
    this.confirmDelete = null;
    try {
      await api.removeFolder(f.id);
      if (this.current && subtree(this.folders, f.id).has(this.current)) this.current = f.parentId;
      await this.load();
      this.notify(`Ordner „${f.name}“ gelöscht`);
    } catch (e) {
      this.fail(e);
    }
  }

  // ---------- Drag & Drop ----------

  dragStart(e: DragEvent, item: DragItem) {
    if (!e.dataTransfer) return;
    e.dataTransfer.setData(DND_TYPE, JSON.stringify(item));
    e.dataTransfer.effectAllowed = "move";
  }

  dragOverTarget(e: DragEvent, key: string) {
    if (!e.dataTransfer?.types.includes(DND_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    this.dragOver = key;
  }

  dragLeave(key: string) {
    if (this.dragOver === key) this.dragOver = null;
  }

  async drop(e: DragEvent, target: string | null) {
    e.preventDefault();
    this.dragOver = null;
    const raw = e.dataTransfer?.getData(DND_TYPE);
    if (!raw) return;
    const item = JSON.parse(raw) as DragItem;
    const label = target ? `„${this.folderName(target)}“` : "die oberste Ebene";
    try {
      if (item.kind === "entry") {
        const entry = this.entries.find((x) => x.id === item.id);
        if (!entry || entry.folderId === target) return;
        await api.move(item.id, target);
        this.notify(`„${entry.title}“ nach ${label} verschoben`);
      } else {
        const f = this.folders.find((x) => x.id === item.id);
        if (!f || f.id === target || f.parentId === target) return;
        await api.saveFolder({ id: f.id, name: f.name, parentId: target });
        if (target) this.expanded[target] = true;
        this.notify(`Ordner „${f.name}“ nach ${label} verschoben`);
      }
      await this.load();
    } catch (err) {
      this.fail(err);
    }
  }
}

export const store = new VaultStore();
