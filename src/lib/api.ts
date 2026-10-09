import { invoke } from "@tauri-apps/api/core";

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
  /** Entsperren mit Windows Hello ist gerade möglich. */
  helloUnlock: boolean;
}

export interface HelloUnlockStatus {
  /** Windows Hello ist auf diesem Gerät eingerichtet. */
  available: boolean;
  /** Entsperren mit Windows Hello ist auf diesem Gerät eingeschaltet. */
  enabled: boolean;
}

export interface EntrySummary {
  id: string;
  title: string;
  username: string;
  url: string;
  hasTotp: boolean;
  folderId: string | null;
  updatedAt: number;
  /** Für KI-Assistenten freigegeben. */
  agent: boolean;
}

/** Stelle, an der das Passwort in eine HTTP-Anfrage eingesetzt wird. */
export type AuthLocation =
  | { kind: "bearer" }
  | { kind: "basic" }
  | { kind: "header"; name: string }
  | { kind: "formField"; name: string }
  | { kind: "jsonField"; name: string };

/** Lesbare Bezeichnung einer Einsetz-Stelle. */
export function authLabel(a: AuthLocation): string {
  switch (a.kind) {
    case "bearer":
      return "Bearer-Token";
    case "basic":
      return "Basic-Auth";
    case "header":
      return `Header ${a.name}`;
    case "formField":
      return `Formularfeld ${a.name}`;
    case "jsonField":
      return `JSON-Feld ${a.name}`;
  }
}

/** Freigabe eines Eintrags für KI-Assistenten (MCP). */
export interface AgentPolicy {
  enabled: boolean;
  /** Hosts, an die die Zugangsdaten gebunden sind (`example.com`, `*.example.com`). */
  hosts: string[];
  /** Leer = Passwort wird nie in Anfragen eingesetzt. */
  auth: AuthLocation[];
  /** 0 = jede Anfrage einzeln bestätigen. */
  sessionMinutes: number;
  /** Login darf von der Browser-Erweiterung ausgefüllt werden. */
  fillLogin: boolean;
}

export const MAX_SESSION_MINUTES = 60;

export interface EntryDetail {
  id: string;
  title: string;
  username: string;
  /** Das Passwort selbst kommt nur über `revealPassword`. */
  hasPassword: boolean;
  /** Der Token selbst kommt nur über `revealApiToken`. */
  hasApiToken: boolean;
  url: string;
  notes: string;
  totp: string;
  folderId: string | null;
  createdAt: number;
  updatedAt: number;
  agent: AgentPolicy;
}

export interface EntryInput {
  id: string | null;
  title: string;
  username: string;
  /** `null` = beim Bearbeiten unverändert lassen. */
  password: string | null;
  /** `null` = beim Bearbeiten unverändert lassen. */
  apiToken: string | null;
  url: string;
  notes: string;
  totp: string;
  folderId: string | null;
  agent: AgentPolicy;
}

export interface Strength {
  score: 0 | 1 | 2 | 3 | 4;
  guessesLog10: number;
  bits: number;
}

/** Mindest-Score (zxcvbn) für das Master-Passwort – muss zu strength.rs passen. */
export const MIN_MASTER_SCORE = 3;

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
}

export interface FolderInput {
  id: string | null;
  name: string;
  parentId: string | null;
}

export interface Settings {
  autoLockMinutes: number;
  clipboardClearSeconds: number;
  agentEnabled: boolean;
  /** Freigaben zusätzlich mit Windows Hello bestätigen. */
  agentHello: boolean;
  /** Stunden nach der letzten Eingabe des Master-Passworts, in denen Windows Hello genügt. */
  helloUnlockHours: number;
}

export type AgentOutcome = "ok" | "locked" | "disabled" | "rejected" | "denied" | "failed";

/** Anfrage der KI, die auf Bestätigung wartet. */
export interface PendingApproval {
  id: number;
  /** `http` = Anfrage mit eingesetztem Geheimnis, `fill` = Login im Browser. */
  action: "http" | "fill";
  entryTitle: string;
  method: string;
  host: string;
  path: string;
  /** `bearer`, `basic` oder `header:<Name>` */
  auth: string;
  /** 0 = keine Sitzungsfreigabe möglich. */
  sessionMinutes: number;
  /** Unix-Zeit der automatischen Ablehnung. */
  expiresAt: number;
}

export type ApprovalDecision = "deny" | "once" | "session";

export interface AgentLogEntry {
  ts: number;
  tool: string;
  detail: string;
  outcome: AgentOutcome;
}

export interface AgentInfo {
  /** Pfad der MCP-Brücke; `null`, wenn sie nicht neben der App liegt. */
  bridgePath: string | null;
  helloAvailable: boolean;
  /** Verbundene Browser-Erweiterungen, z. B. „Chrome“. */
  browsers: string[];
  /** Ordner der Erweiterung zum Laden als entpackte Erweiterung. */
  extensionDir: string | null;
  extensionId: string;
  log: AgentLogEntry[];
}

export interface GenOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

export interface Generated {
  password: string;
  entropyBits: number;
}

export interface TotpCode {
  code: string;
  remaining: number;
  period: number;
}

export type SyncState = "disabled" | "idle" | "syncing" | "offline" | "error" | "needs-password";

export interface SyncStatus {
  state: SyncState;
  message: string;
  lastSync: number | null;
  configured: boolean;
  connected: boolean;
  clientId: string;
}

export type CopyField = "username" | "password" | "apiToken" | "totp";

export const MIN_MASTER_LEN = 10;

export const defaultGenOptions: GenOptions = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  excludeAmbiguous: false,
};

export const api = {
  status: () => invoke<VaultStatus>("vault_status"),
  create: (password: string) => invoke<void>("create_vault", { password }),
  unlock: (password: string) => invoke<void>("unlock", { password }),
  unlockHello: () => invoke<void>("unlock_hello"),
  helloUnlockStatus: () => invoke<HelloUnlockStatus>("hello_unlock_status"),
  setHelloUnlock: (enabled: boolean) => invoke<void>("set_hello_unlock", { enabled }),
  lock: () => invoke<void>("lock"),
  touch: () => invoke<void>("touch"),
  changeMaster: (current: string, newPassword: string) =>
    invoke<void>("change_master_password", { current, newPassword }),

  list: () => invoke<EntrySummary[]>("list_entries"),
  // Das Backend lässt leere/Standardfelder der Freigabe weg (kompakte Tresordatei).
  get: (id: string) =>
    invoke<EntryDetail>("get_entry", { id }).then((d) => ({
      ...d,
      agent: { enabled: false, hosts: [], auth: [], sessionMinutes: 0, fillLogin: false, ...(d.agent as Partial<AgentPolicy>) },
    })),
  revealPassword: (id: string) => invoke<string>("reveal_password", { id }),
  revealApiToken: (id: string) => invoke<string>("reveal_api_token", { id }),
  passwordStrength: (password: string) => invoke<Strength>("password_strength", { password }),
  save: (entry: EntryInput) => invoke<string>("save_entry", { entry }),
  remove: (id: string) => invoke<void>("delete_entry", { id }),
  move: (id: string, folderId: string | null) => invoke<void>("move_entry", { id, folderId }),

  folders: () => invoke<Folder[]>("list_folders"),
  saveFolder: (folder: FolderInput) => invoke<string>("save_folder", { folder }),
  removeFolder: (id: string) => invoke<void>("delete_folder", { id }),

  totp: (id: string) => invoke<TotpCode | null>("totp_code", { id }),

  copyField: (id: string, field: CopyField) => invoke<number>("copy_field", { id, field }),
  copyText: (text: string) => invoke<number>("copy_text", { text }),
  generate: (options: GenOptions) => invoke<Generated>("generate_password", { options }),

  syncStatus: () => invoke<SyncStatus>("sync_status"),
  syncConfigure: (clientId: string, clientSecret: string) =>
    invoke<void>("sync_configure", { clientId, clientSecret }),
  syncConnect: () => invoke<void>("sync_connect"),
  syncDisconnect: () => invoke<void>("sync_disconnect"),
  syncNow: (password?: string) => invoke<void>("sync_now", { password: password ?? null }),
  syncRestore: () => invoke<void>("sync_restore"),

  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),

  agentInfo: () => invoke<AgentInfo>("agent_info"),
  agentPending: () => invoke<PendingApproval[]>("agent_pending"),
  agentDecide: (id: number, decision: ApprovalDecision) => invoke<void>("agent_decide", { id, decision }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export const SCORE_LABELS = ["sehr schwach", "schwach", "mittel", "stark", "sehr stark"] as const;

/** Score aus Bit-Entropie (für zufällig generierte Passwörter, analog zu zxcvbn-Schwellen). */
export function scoreFromBits(bits: number): Strength["score"] {
  const log10 = bits / Math.LOG2E / Math.LN10;
  if (log10 < 3) return 0;
  if (log10 < 6) return 1;
  if (log10 < 8) return 2;
  if (log10 < 10) return 3;
  return 4;
}

/** Ordner als flache, tiefensortierte Liste mit Einrückungstiefe (für Auswahllisten). */
export function flattenFolders(folders: Folder[]): { folder: Folder; depth: number }[] {
  const out: { folder: Folder; depth: number }[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const f of folders.filter((f) => f.parentId === parent)) {
      out.push({ folder: f, depth });
      walk(f.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

/** Pfad vom obersten Ordner bis einschließlich `id`. */
export function folderPath(folders: Folder[], id: string | null): Folder[] {
  const path: Folder[] = [];
  let cur = id ? folders.find((f) => f.id === id) : undefined;
  while (cur && path.length < 64) {
    path.unshift(cur);
    cur = cur.parentId ? folders.find((f) => f.id === cur!.parentId) : undefined;
  }
  return path;
}

/** Alle Ordner-IDs im Teilbaum unter `id` (inklusive `id`). */
export function subtree(folders: Folder[], id: string): Set<string> {
  const ids = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const f of folders) {
      if (f.parentId && ids.has(f.parentId) && !ids.has(f.id)) {
        ids.add(f.id);
        grew = true;
      }
    }
  }
  return ids;
}

/** Drag-and-Drop-Nutzlast zwischen Kacheln und Ordnerbaum. */
export const DND_TYPE = "application/x-remember-key";
export type DragItem = { kind: "entry" | "folder"; id: string };
