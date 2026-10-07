<script lang="ts">
  import { onMount } from "svelte";
  import { api, defaultGenOptions, errorText, flattenFolders, type EntryInput } from "$lib/api";
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
  let agentEnabled = $state(false);
  let agentHosts = $state("");
  let form = $state<Omit<EntryInput, "password" | "agent">>({
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
      const agent = { enabled: agentEnabled, hosts: agentHosts.split(/[\s,;]+/).filter(Boolean) };
      onsaved(await api.save({ ...form, password: !id || passwordTouched ? password : null, agent }));
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
      {/if}
      <div class="hint">
        Die KI sieht nur Titel, Benutzername und Hosts – nie das Passwort. Zugangsdaten werden nur an die genannten
        Hosts gebunden.
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
</style>
