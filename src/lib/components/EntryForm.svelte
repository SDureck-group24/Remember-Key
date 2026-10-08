<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    defaultGenOptions,
    errorText,
    flattenFolders,
    MAX_SESSION_MINUTES,
    type AuthLocation,
    type EntryInput,
  } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Generator from "./Generator.svelte";
  import Icon from "./Icon.svelte";
  import Strength from "./Strength.svelte";

  let {
    id,
    defaultFolder = null,
    onsaved,
    oncancel,
  }: {
    id: string | null;
    defaultFolder?: string | null;
    onsaved: (id: string) => void;
    oncancel: () => void;
  } = $props();

  let folderOptions = $derived(flattenFolders(store.folders));

  // Das Passwort liegt im Formular nur als Text, wenn es neu ist, geändert oder
  // ausdrücklich geladen wurde. Sonst bleibt es im Backend (password: null).
  let password = $state("");
  let passwordTouched = $state(false);
  let hasStoredPassword = $state(false);
  // API-Token: wie das Passwort nur im Formular, wenn neu, geändert oder geladen.
  let apiToken = $state("");
  let apiTokenTouched = $state(false);
  let hasStoredApiToken = $state(false);
  let revealToken = $state(false);
  let agentEnabled = $state(false);
  let agentHosts = $state("");
  // Einsetz-Stellen für HTTP-Anfragen; neue Freigaben starten mit Bearer-Token.
  let authBearer = $state(true);
  let authBasic = $state(false);
  let authHeader = $state(false);
  let authHeaderName = $state("");
  let sessionMinutes = $state(0);
  let fillLogin = $state(false);
  let form = $state<Omit<EntryInput, "password" | "apiToken" | "agent">>({
    id: null,
    title: "",
    username: "",
    url: "",
    notes: "",
    totp: "",
    folderId: null,
  });
  let loaded = $state(false);
  let reveal = $state(false);
  let busy = $state(false);
  let error = $state("");
  let showGenerator = $state(false);

  onMount(async () => {
    if (id) {
      try {
        const d = await api.get(id);
        hasStoredPassword = d.hasPassword;
        hasStoredApiToken = d.hasApiToken;
        form = {
          id: d.id,
          title: d.title,
          username: d.username,
          url: d.url,
          notes: d.notes,
          totp: d.totp,
          folderId: d.folderId,
        };
        agentEnabled = d.agent.enabled;
        agentHosts = d.agent.hosts.join(", ");
        if (d.agent.enabled || d.agent.hosts.length) {
          const header = d.agent.auth.find((a) => a.kind === "header");
          authBearer = d.agent.auth.some((a) => a.kind === "bearer");
          authBasic = d.agent.auth.some((a) => a.kind === "basic");
          authHeader = !!header;
          authHeaderName = header?.kind === "header" ? header.name : "";
        }
        sessionMinutes = d.agent.sessionMinutes;
        fillLogin = d.agent.fillLogin;
      } catch (e) {
        error = errorText(e);
      }
    } else {
      form.folderId = defaultFolder;
    }
    loaded = true;
  });

  async function toggleReveal() {
    if (!reveal && id && hasStoredPassword && !passwordTouched) {
      try {
        password = await api.revealPassword(id);
        passwordTouched = true;
      } catch (e) {
        error = errorText(e);
        return;
      }
    }
    reveal = !reveal;
  }

  async function toggleRevealToken() {
    if (!revealToken && id && hasStoredApiToken && !apiTokenTouched) {
      try {
        apiToken = await api.revealApiToken(id);
        apiTokenTouched = true;
      } catch (e) {
        error = errorText(e);
        return;
      }
    }
    revealToken = !revealToken;
  }

  async function quickGenerate() {
    try {
      password = (await api.generate(defaultGenOptions)).password;
      passwordTouched = true;
      reveal = true;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      const auth: AuthLocation[] = [];
      if (authBearer) auth.push({ kind: "bearer" });
      if (authBasic) auth.push({ kind: "basic" });
      if (authHeader && authHeaderName.trim()) auth.push({ kind: "header", name: authHeaderName.trim() });
      const agent = {
        enabled: agentEnabled,
        hosts: agentHosts.split(/[\s,;]+/).filter(Boolean),
        auth,
        sessionMinutes: Number(sessionMinutes) || 0,
        fillLogin,
      };
      onsaved(
        await api.save({
          ...form,
          password: !id || passwordTouched ? password : null,
          apiToken: !id || apiTokenTouched ? apiToken : null,
          agent,
        }),
      );
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if loaded}
  <form class="form" onsubmit={submit}>
    <h3>{id ? "Eintrag bearbeiten" : "Neuer Eintrag"}</h3>

    <div class="field">
      <label for="title">Titel</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="title" class="input" bind:value={form.title} autofocus spellcheck="false" placeholder="z. B. Firmen-Mail" />
    </div>

    <div class="field">
      <label for="folder">Ordner</label>
      <select
        id="folder"
        class="input"
        value={form.folderId ?? ""}
        onchange={(e) => (form.folderId = e.currentTarget.value || null)}
      >
        <option value="">Kein Ordner</option>
        {#each folderOptions as o (o.folder.id)}
          <option value={o.folder.id}>{"   ".repeat(o.depth)}{o.folder.name}</option>
        {/each}
      </select>
    </div>

    <div class="field">
      <label for="user">Benutzername oder E-Mail</label>
      <input id="user" class="input" bind:value={form.username} spellcheck="false" autocomplete="off" />
    </div>

    <div class="field">
      <label for="pw">Passwort</label>
      <div class="row">
        <input
          id="pw"
          class="input mono grow"
          type={reveal ? "text" : "password"}
          bind:value={password}
          oninput={() => (passwordTouched = true)}
          placeholder={id && hasStoredPassword && !passwordTouched ? "Unverändert – zum Ändern neu eingeben" : ""}
          autocomplete="off"
          spellcheck="false"
        />
        <button
          type="button"
          class="btn btn-icon btn-secondary"
          title={reveal ? "Verbergen" : "Anzeigen"}
          aria-label={reveal ? "Passwort verbergen" : "Passwort anzeigen"}
          onclick={toggleReveal}
        >
          <Icon name={reveal ? "eye-slash" : "eye"} size={16} />
        </button>
        <button type="button" class="btn btn-icon btn-secondary" title="Sicheres Passwort erzeugen" aria-label="Sicheres Passwort erzeugen" onclick={quickGenerate}>
          <Icon name="dice-five" size={16} />
        </button>
        <button type="button" class="btn btn-icon btn-secondary" title="Generator-Optionen" aria-label="Generator-Optionen" onclick={() => (showGenerator = true)}>
          <Icon name="password" size={16} />
        </button>
      </div>
      {#if passwordTouched || !id}<Strength {password} />{/if}
    </div>

    <div class="field">
      <label for="api-token">API-Token</label>
      <div class="row">
        <input
          id="api-token"
          class="input mono grow"
          type={revealToken ? "text" : "password"}
          bind:value={apiToken}
          oninput={() => (apiTokenTouched = true)}
          placeholder={id && hasStoredApiToken && !apiTokenTouched ? "Unverändert – zum Ändern neu eingeben" : "Optional"}
          autocomplete="off"
          spellcheck="false"
        />
        <button
          type="button"
          class="btn btn-icon btn-secondary"
          title={revealToken ? "Verbergen" : "Anzeigen"}
          aria-label={revealToken ? "API-Token verbergen" : "API-Token anzeigen"}
          onclick={toggleRevealToken}
        >
          <Icon name={revealToken ? "eye-slash" : "eye"} size={16} />
        </button>
      </div>
      <div class="hint">Für Programm- und KI-Zugriffe auf eine API. Wird statt des Passworts eingesetzt, wenn vorhanden.</div>
    </div>

    <div class="field">
      <label for="url">Website</label>
      <input id="url" class="input" bind:value={form.url} spellcheck="false" placeholder="https://" />
    </div>

    <div class="field">
      <label for="totp">2FA-Schlüssel oder otpauth-Link</label>
      <input
        id="totp"
        class="input mono"
        bind:value={form.totp}
        spellcheck="false"
        autocomplete="off"
        placeholder="JBSWY3DPEHPK3PXP"
      />
      <div class="hint">Der Schlüssel, der beim Einrichten der Zwei-Faktor-Anmeldung unter dem QR-Code steht.</div>
    </div>

    <div class="field">
      <label for="notes">Notizen</label>
      <textarea id="notes" class="input" bind:value={form.notes}></textarea>
    </div>

    <div class="field">
      <span class="lbl">KI-Zugriff</span>
      <div class="seg">
        <label class="seg-opt">
          <input type="checkbox" bind:checked={agentEnabled} />
          Für KI-Assistenten freigeben
        </label>
      </div>
      {#if agentEnabled}
        <input
          id="agent-hosts"
          class="input hosts"
          bind:value={agentHosts}
          spellcheck="false"
          autocomplete="off"
          aria-label="Erlaubte Hosts"
          placeholder="Leer = Host der Website, z. B. api.github.com, *.example.com"
        />

        <span class="lbl sub">Login im Browser</span>
        <div class="seg">
          <label class="seg-opt" title="Die Remember-Key-Erweiterung füllt Benutzername und Passwort aus und sendet ab">
            <input type="checkbox" bind:checked={fillLogin} /> Login-Formular ausfüllen lassen
          </label>
        </div>

        <span class="lbl sub">Passwort in HTTPS-Anfragen einsetzen</span>
        <div class="seg">
          <label class="seg-opt" title="Authorization: Bearer <Passwort>">
            <input type="checkbox" bind:checked={authBearer} /> Bearer-Token
          </label>
          <label class="seg-opt" title="Authorization: Basic <Benutzername:Passwort>">
            <input type="checkbox" bind:checked={authBasic} /> Basic-Auth
          </label>
          <label class="seg-opt" title="Eigener Header mit dem Passwort als Wert">
            <input type="checkbox" bind:checked={authHeader} /> Eigener Header
          </label>
        </div>
        {#if authBearer || authBasic || authHeader}
          <div class="hint">
            Eingesetzt wird der API-Token, falls hinterlegt, sonst das Passwort. Die meisten APIs akzeptieren kein
            Login-Passwort. Nur https – http ist ausschließlich für localhost erlaubt.
          </div>
        {/if}
        {#if authHeader}
          <input
            class="input hosts mono"
            bind:value={authHeaderName}
            spellcheck="false"
            autocomplete="off"
            aria-label="Header-Name"
            placeholder="Header-Name, z. B. X-Api-Key"
          />
        {/if}

        <label class="lbl sub" for="agent-session">Bestätigung</label>
        <select id="agent-session" class="input" bind:value={sessionMinutes}>
          <option value={0}>Jede Anfrage einzeln bestätigen</option>
          {#each [5, 15, 30, MAX_SESSION_MINUTES] as m (m)}
            <option value={m}>Freigabe für {m} Minuten erlauben</option>
          {/each}
        </select>
      {/if}
      <div class="hint">
        Die KI sieht nur Titel, Benutzername und Hosts – nie das Passwort. Remember Key setzt es nach deiner Bestätigung
        selbst ein, nur an den genannten Hosts und nur an den gewählten Stellen.
      </div>
    </div>

    {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <div class="row actions">
      <button class="btn btn-primary" disabled={busy || !form.title.trim()}>
        <Icon name="check" size={16} /> Speichern
      </button>
      <button type="button" class="btn btn-ghost" onclick={oncancel}>Abbrechen</button>
    </div>
  </form>
{/if}

{#if showGenerator}
  <Generator
    onclose={() => (showGenerator = false)}
    onuse={(pw) => {
      password = pw;
      passwordTouched = true;
      reveal = true;
      showGenerator = false;
    }}
  />
{/if}

<style>
  .form {
    max-width: 560px;
    margin: 0 auto;
  }
  select.input {
    appearance: auto;
  }
  h3 {
    margin-bottom: var(--space-8);
  }
  .actions {
    margin-top: var(--space-8);
  }
  /* Wie .field > label */
  .lbl {
    display: block;
    font-size: 12px;
    margin-bottom: 5px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
  }
  .hosts {
    margin-top: var(--space-3);
  }
  .lbl.sub {
    margin-top: var(--space-4);
  }
</style>
