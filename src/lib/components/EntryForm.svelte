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
  import Switch from "./Switch.svelte";

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
  let authForm = $state(false);
  let authFormName = $state("");
  let authJson = $state(false);
  let authJsonName = $state("");
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
          const form = d.agent.auth.find((a) => a.kind === "formField");
          authForm = !!form;
          authFormName = form?.kind === "formField" ? form.name : "";
          const json = d.agent.auth.find((a) => a.kind === "jsonField");
          authJson = !!json;
          authJsonName = json?.kind === "jsonField" ? json.name : "";
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
      if (authForm && authFormName.trim()) auth.push({ kind: "formField", name: authFormName.trim() });
      if (authJson && authJsonName.trim()) auth.push({ kind: "jsonField", name: authJsonName.trim() });
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

    <div class="pair">
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
            <option value={o.folder.id}>{"   ".repeat(o.depth)}{o.folder.name}</option>
          {/each}
        </select>
      </div>
    </div>

    <div class="field">
      <label for="url">Website</label>
      <input id="url" class="input" bind:value={form.url} spellcheck="false" placeholder="https://" />
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
          placeholder={id && hasStoredPassword && !passwordTouched ? "Unverändert. Zum Ändern neu eingeben" : ""}
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

    <h5>Weitere Zugangsdaten</h5>

    <div class="field">
      <label for="totp">2FA-Schlüssel oder otpauth-Link</label>
      <input
        id="totp"
        class="input mono"
        bind:value={form.totp}
        spellcheck="false"
        autocomplete="off"
        placeholder="Base32-Schlüssel oder otpauth://…"
        aria-describedby="totp-hint"
      />
      <div class="hint" id="totp-hint">Steht beim Einrichten der Zwei-Faktor-Anmeldung unter dem QR-Code.</div>
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
          placeholder={id && hasStoredApiToken && !apiTokenTouched ? "Unverändert. Zum Ändern neu eingeben" : "Optional"}
          autocomplete="off"
          spellcheck="false"
          aria-describedby="token-hint"
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
      <div class="hint" id="token-hint">Für Programm- und KI-Zugriffe auf eine API. Wird statt des Passworts eingesetzt, wenn vorhanden.</div>
    </div>

    <div class="field">
      <label for="notes">Notizen</label>
      <textarea id="notes" class="input" bind:value={form.notes}></textarea>
    </div>

    <h5>KI-Zugriff</h5>

    <Switch
      bind:checked={agentEnabled}
      label="Für KI-Assistenten freigeben"
      description="Die KI sieht nur Titel, Benutzername und Hosts, nie das Passwort. Remember Key setzt es nach Ihrer Bestätigung selbst ein, nur an den erlaubten Hosts und Stellen."
    />

    {#if agentEnabled}
      <div class="agent">
        <div class="field">
          <label for="agent-hosts">Erlaubte Hosts</label>
          <input
            id="agent-hosts"
            class="input mono"
            bind:value={agentHosts}
            spellcheck="false"
            autocomplete="off"
            placeholder="api.github.com, *.example.com"
            aria-describedby="hosts-hint"
          />
          <div class="hint" id="hosts-hint">Leer lassen, um nur den Host der Website zu erlauben. Mehrere mit Komma trennen.</div>
        </div>

        <Switch
          bind:checked={fillLogin}
          label="Login im Browser ausfüllen"
          description="Die Remember-Key-Erweiterung füllt Benutzername und Passwort aus und sendet das Formular ab."
        />

        <fieldset class="auth">
          <legend>Zugangsdaten in HTTPS-Anfragen einsetzen</legend>
          <label class="check">
            <input type="checkbox" bind:checked={authBearer} />
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
            <span class="opt">Bearer-Token <code>Authorization: Bearer …</code></span>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={authBasic} />
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
            <span class="opt">Basic-Auth <code>Authorization: Basic …</code></span>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={authHeader} />
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
            <span class="opt">Eigener Header</span>
          </label>
          {#if authHeader}
            <input
              class="input mono sub-input"
              bind:value={authHeaderName}
              spellcheck="false"
              autocomplete="off"
              aria-label="Header-Name"
              placeholder="Header-Name, z. B. X-Api-Key"
            />
          {/if}
          <label class="check">
            <input type="checkbox" bind:checked={authForm} />
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
            <span class="opt">Formularfeld <code>application/x-www-form-urlencoded</code></span>
          </label>
          {#if authForm}
            <input
              class="input mono sub-input"
              bind:value={authFormName}
              spellcheck="false"
              autocomplete="off"
              aria-label="Name des Formularfelds"
              placeholder="Feldname, z. B. password"
            />
          {/if}
          <label class="check">
            <input type="checkbox" bind:checked={authJson} />
            <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
            <span class="opt">JSON-Feld <code>oberste Ebene des Bodys</code></span>
          </label>
          {#if authJson}
            <input
              class="input mono sub-input"
              bind:value={authJsonName}
              spellcheck="false"
              autocomplete="off"
              aria-label="Name des JSON-Felds"
              placeholder="Feldname, z. B. password"
            />
          {/if}
          <div class="hint">
            {#if authBearer || authBasic || authHeader || authForm || authJson}
              Eingesetzt wird der API-Token, falls hinterlegt, sonst das Passwort. Die meisten APIs akzeptieren kein
              Login-Passwort. Nur über https; http ist ausschließlich für localhost erlaubt.
            {:else}
              Nichts gewählt: Die KI kann mit diesem Eintrag keine Anfragen stellen.
            {/if}
          </div>
        </fieldset>

        <div class="field">
          <label for="agent-session">Bestätigung</label>
          <select id="agent-session" class="input" bind:value={sessionMinutes}>
            <option value={0}>Jede Anfrage einzeln bestätigen</option>
            {#each [5, 15, 30, MAX_SESSION_MINUTES] as m (m)}
              <option value={m}>Freigabe für {m} Minuten erlauben</option>
            {/each}
          </select>
        </div>
      </div>
    {/if}

    {#if error}<p class="error" role="alert"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <div class="row actions">
      <button class="btn btn-primary" disabled={busy || !form.title.trim()}>
        <Icon name="check" size={16} />
        {busy ? "Wird gespeichert …" : "Speichern"}
      </button>
      <button type="button" class="btn btn-ghost" onclick={oncancel}>Abbrechen</button>
      {#if !form.title.trim()}<span class="hint need">Zum Speichern einen Titel eingeben.</span>{/if}
    </div>
  </form>
{:else}
  <p class="text-muted loading" role="status">Eintrag wird geladen …</p>
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
  /* Gruppenüberschriften: Abstand trennt die Gruppen, keine Linien. */
  h5 {
    margin: calc(var(--space-8) * 1.6) 0 var(--space-4);
  }
  .pair {
    display: grid;
    grid-template-columns: 3fr 2fr;
    gap: var(--space-4);
  }
  .field + .field,
  .pair + .field {
    margin-top: var(--space-6);
  }
  .pair .field + .field {
    margin-top: 0;
  }
  .loading {
    font-size: 13px;
  }

  /* Einstellungen der Freigabe, eingerückt unter ihrem Schalter. */
  .agent {
    margin: var(--space-2) 0 0 var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .auth {
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  legend {
    padding: 0;
    margin-bottom: 5px;
    font-size: 12px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 5px 0;
    font-size: 14px;
    cursor: pointer;
  }
  .check input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }
  .box {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-neutral-600);
    color: transparent;
  }
  .check:hover .box {
    border-color: var(--color-accent);
  }
  .check input:checked + .box {
    border-color: var(--color-accent);
    background: var(--color-accent);
    color: var(--color-bg);
  }
  .check input:focus-visible + .box {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  .opt code {
    margin-left: var(--space-3);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .sub-input {
    width: calc(100% - 16px - var(--space-3));
    margin: 0 0 var(--space-2) calc(16px + var(--space-3));
  }
  .auth .hint {
    margin-top: var(--space-2);
  }

  /* Speichern bleibt beim Scrollen erreichbar. Der negative Abstand gleicht das
     untere Padding des Scrollbereichs in Vault aus. */
  .actions {
    position: sticky;
    bottom: calc(var(--space-8) * -1);
    margin: var(--space-8) 0 calc(var(--space-8) * -1);
    padding: var(--space-4) 0 var(--space-8);
    background:
      linear-gradient(to right, transparent, var(--color-divider) 48px, var(--color-divider) calc(100% - 48px), transparent)
        no-repeat top / 100% 1px,
      var(--color-bg);
  }
  .need {
    margin: 0 0 0 var(--space-2);
  }

  @media (max-width: 560px) {
    .pair {
      grid-template-columns: 1fr;
    }
    .pair .field + .field {
      margin-top: var(--space-6);
    }
    .agent {
      margin-left: 0;
    }
    .opt code {
      display: block;
      margin-left: 0;
    }
  }
</style>
