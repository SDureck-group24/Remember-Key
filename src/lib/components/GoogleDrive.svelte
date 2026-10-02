<!-- Verbindung zu Google Drive: Einrichtung, Status, Abgleich und Wiederherstellung. -->
<script lang="ts">
  import { api, errorText } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let {
    mode = "settings",
    onrestored,
  }: { mode?: "settings" | "restore"; onrestored?: () => void } = $props();

  let clientId = $state(store.sync?.clientId ?? "");
  let clientSecret = $state("");
  let busy = $state<"" | "connect" | "sync" | "disconnect" | "restore">("");
  let error = $state("");
  let password = $state("");
  let showGuide = $state(!store.sync?.configured);
  let confirmDisconnect = $state(false);

  let s = $derived(store.sync);

  async function refresh() {
    store.sync = await api.syncStatus();
  }

  async function connect() {
    busy = "connect";
    error = "";
    try {
      if (clientSecret.trim() || !s?.configured || clientId.trim() !== s.clientId) {
        await api.syncConfigure(clientId, clientSecret);
      }
      await api.syncConnect();
      clientSecret = "";
      await refresh();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = "";
    }
  }

  async function syncNow(withPassword = false) {
    busy = "sync";
    error = "";
    try {
      await api.syncNow(withPassword ? password : undefined);
      password = "";
      store.notify("Mit Google Drive abgeglichen");
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = "";
      await refresh();
    }
  }

  async function disconnect() {
    busy = "disconnect";
    error = "";
    confirmDisconnect = false;
    try {
      await api.syncDisconnect();
      await refresh();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = "";
    }
  }

  async function restore() {
    busy = "restore";
    error = "";
    try {
      await api.syncRestore();
      onrestored?.();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = "";
    }
  }

  function ago(ts: number | null) {
    if (!ts) return "noch nie";
    const sec = Math.max(0, Math.round(Date.now() / 1000 - ts));
    if (sec < 60) return "gerade eben";
    if (sec < 3600) return `vor ${Math.round(sec / 60)} Min.`;
    return new Date(ts * 1000).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" });
  }

  const labels: Record<string, string> = {
    idle: "Synchronisiert",
    syncing: "Wird abgeglichen …",
    offline: "Offline",
    error: "Fehler",
    "needs-password": "Bestätigung nötig",
  };
</script>

{#if s?.connected}
  <div class="status" data-state={s.state}>
    <Icon
      name={s.state === "error" || s.state === "needs-password" ? "cloud-warning" : s.state === "offline" ? "cloud-slash" : "cloud-check"}
      size={20}
    />
    <div class="grow">
      <div class="title">{labels[s.state] ?? s.state}</div>
      <div class="hint">
        {s.state === "error" || s.state === "needs-password" ? s.message : `Letzter Abgleich: ${ago(s.lastSync)}`}
      </div>
    </div>
  </div>

  {#if mode === "restore"}
    <button class="btn btn-primary btn-block" onclick={restore} disabled={!!busy}>
      <Icon name="cloud-arrow-down" size={16} />
      {busy === "restore" ? "Wird geladen …" : "Tresor aus Google Drive laden"}
    </button>
  {:else}
    {#if s.state === "needs-password"}
      <form class="row pw" onsubmit={(e) => (e.preventDefault(), syncNow(true))}>
        <input class="input grow" type="password" bind:value={password} placeholder="Master-Passwort" aria-label="Master-Passwort" autocomplete="current-password" />
        <button class="btn btn-primary" disabled={!password || !!busy}>Abgleichen</button>
      </form>
    {/if}
    <div class="row actions">
      <button class="btn btn-secondary" onclick={() => syncNow()} disabled={!!busy}>
        <Icon name="arrows-clockwise" size={15} />
        {busy === "sync" ? "Gleicht ab …" : "Jetzt abgleichen"}
      </button>
      <span class="grow"></span>
      {#if confirmDisconnect}
        <button class="btn btn-ghost" onclick={() => (confirmDisconnect = false)}>Abbrechen</button>
        <button class="btn btn-danger" onclick={disconnect} disabled={!!busy}>Trennen</button>
      {:else}
        <button class="btn btn-ghost" onclick={() => (confirmDisconnect = true)} disabled={!!busy}>
          <Icon name="link-break" size={15} /> Verbindung trennen
        </button>
      {/if}
    </div>
    {#if confirmDisconnect}
      <p class="hint">Der lokale Tresor bleibt erhalten. Die Kopie in Google Drive wird nicht gelöscht.</p>
    {/if}
  {/if}
{:else}
  <p class="hint intro">
    Der Tresor wird verschlüsselt in einem versteckten App-Ordner Ihres Google Drive gespeichert. Google sieht
    nur verschlüsselte Daten, Remember Key sieht nur diesen Ordner.
  </p>

  <button class="guide-toggle btn btn-ghost" onclick={() => (showGuide = !showGuide)} aria-expanded={showGuide}>
    <Icon name={showGuide ? "caret-down" : "caret-right"} size={12} />
    Einmalige Einrichtung in der Google Cloud Console
  </button>
  {#if showGuide}
    <ol class="guide">
      <li>Unter <span class="mono">console.cloud.google.com</span> ein Projekt anlegen.</li>
      <li>„APIs &amp; Dienste“ › „Bibliothek“ › <strong>Google Drive API</strong> aktivieren.</li>
      <li>„OAuth-Zustimmungsbildschirm“: Typ <em>Extern</em>, Ihre E-Mail als Testnutzer eintragen, Bereich <span class="mono">…/auth/drive.appdata</span> hinzufügen.</li>
      <li>Status auf <strong>In Produktion</strong> setzen, sonst läuft die Anmeldung nach 7 Tagen ab. Die Warnung „App nicht überprüft“ können Sie für sich selbst bestätigen.</li>
      <li>„Anmeldedaten“ › „OAuth-Client-ID“ › Anwendungstyp <strong>Desktop-App</strong>. Client-ID und Secret hier eintragen.</li>
    </ol>
  {/if}

  <form onsubmit={(e) => (e.preventDefault(), connect())}>
    <div class="field">
      <label for="gd-id">Client-ID</label>
      <input id="gd-id" class="input mono" bind:value={clientId} spellcheck="false" autocomplete="off" placeholder="….apps.googleusercontent.com" />
    </div>
    <div class="field">
      <label for="gd-secret">Client-Secret</label>
      <input
        id="gd-secret"
        class="input mono"
        type="password"
        bind:value={clientSecret}
        spellcheck="false"
        autocomplete="off"
        placeholder={s?.configured ? "gespeichert" : ""}
      />
    </div>
    <button class="btn btn-primary btn-block" disabled={!!busy || !clientId.trim() || (!clientSecret.trim() && !s?.configured)}>
      <Icon name="google-drive-logo" size={16} />
      {busy === "connect" ? "Bitte im Browser anmelden …" : "Mit Google Drive verbinden"}
    </button>
  </form>
{/if}

{#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sm);
    color: var(--color-accent);
    margin-bottom: var(--space-4);
  }
  .status[data-state="error"],
  .status[data-state="needs-password"] {
    color: var(--color-danger);
  }
  .status[data-state="offline"] {
    color: var(--color-text-muted);
  }
  .title {
    color: var(--color-text);
    font-size: 14px;
  }
  .status .hint {
    margin-top: 0;
  }
  .pw {
    margin-bottom: var(--space-3);
  }
  .actions {
    flex-wrap: wrap;
  }
  .intro {
    margin: 0 0 var(--space-3);
    font-size: 13px;
  }
  .guide-toggle {
    padding-inline: 0;
    font-size: 13px;
  }
  .guide {
    margin: var(--space-2) 0 var(--space-4);
    padding-left: 20px;
    font-size: 13px;
    color: color-mix(in srgb, var(--color-text) 80%, transparent);
    display: grid;
    gap: var(--space-2);
  }
  form {
    margin-top: var(--space-4);
  }
  .field + .field {
    margin-top: var(--space-4);
  }
  .btn-block {
    margin-top: var(--space-6);
  }
  .error {
    margin-top: var(--space-4);
  }
</style>
