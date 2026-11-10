<script lang="ts">
  import { api, authLabel, entryHost, entryInitial, errorText, type CopyField, type EntryDetail } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Totp from "./Totp.svelte";

  let {
    id,
    oncopy,
    onvisit,
    onedit,
    ondeleted,
  }: {
    id: string;
    oncopy: (field: CopyField, label: string) => void;
    onvisit: () => void;
    onedit: () => void;
    ondeleted: () => void;
  } = $props();

  let detail = $state<EntryDetail | null>(null);
  /** Nur gesetzt, solange das Passwort sichtbar ist; nach 20 s automatisch verborgen. */
  let revealed = $state<string | null>(null);
  let tokenRevealed = $state<string | null>(null);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const REVEAL_SECONDS = 20;
  let confirmDelete = $state(false);
  let error = $state("");

  $effect(() => {
    const current = id;
    hide();
    confirmDelete = false;
    error = "";
    detail = null;
    api
      .get(current)
      .then((d) => {
        if (current === id) detail = d;
      })
      .catch((e) => (error = errorText(e)));
  });

  function hide() {
    revealed = null;
    tokenRevealed = null;
    clearTimeout(hideTimer);
  }

  async function toggleReveal() {
    if (revealed !== null) {
      hide();
      return;
    }
    try {
      revealed = await api.revealPassword(id);
      hideTimer = setTimeout(hide, REVEAL_SECONDS * 1000);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function toggleRevealToken() {
    if (tokenRevealed !== null) {
      hide();
      return;
    }
    try {
      tokenRevealed = await api.revealApiToken(id);
      clearTimeout(hideTimer);
      hideTimer = setTimeout(hide, REVEAL_SECONDS * 1000);
    } catch (e) {
      error = errorText(e);
    }
  }

  $effect(() => hide);

  async function remove() {
    try {
      await api.remove(id);
      ondeleted();
    } catch (e) {
      error = errorText(e);
    }
  }

  function fmt(ts: number) {
    return new Date(ts * 1000).toLocaleString("de-DE", { dateStyle: "medium", timeStyle: "short" });
  }
</script>

{#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

{#if !detail && !error}
  <p class="text-muted loading" role="status">Eintrag wird geladen …</p>
{:else if detail}
  <article class="view">
    <div class="head">
      <span class="mark" aria-hidden="true">{entryInitial(detail.title)}</span>
      <div class="grow">
        <h3 id="entry-title">{detail.title}</h3>
        {#if detail.url}<p class="host">{entryHost(detail.url)}</p>{/if}
      </div>
    </div>

    <div class="actions">
      {#if detail.url}
        <button class="btn btn-primary" onclick={onvisit}><Icon name="arrow-square-out" size={15} /> Öffnen und anmelden</button>
      {/if}
      {#if detail.hasPassword}
        <button class="btn btn-secondary" onclick={() => oncopy("password", "Passwort")}><Icon name="copy" size={15} /> Passwort kopieren</button>
      {/if}
      <button class="btn btn-secondary" onclick={onedit}><Icon name="pencil-simple" size={15} /> Bearbeiten</button>
    </div>

    {#if detail.username || detail.hasPassword || detail.hasApiToken || detail.totp}
    <h6 class="group-label">Anmeldung</h6>
    <dl class="group">
      {#if detail.username}
        <dt>Benutzername</dt>
        <dd>
          <span class="value grow">{detail.username}</span>
          <button class="btn btn-icon btn-sm" title="Benutzername kopieren" aria-label="Benutzername kopieren" onclick={() => oncopy("username", "Benutzername")}>
            <Icon name="copy" size={16} />
          </button>
        </dd>
      {/if}

      {#if detail.hasPassword}
        <dt>Passwort</dt>
        <dd>
          <span class="value mono grow" class:masked={revealed === null}>{revealed ?? "••••••••••••••"}</span>
          <button
            class="btn btn-icon btn-sm"
            title={revealed !== null ? "Verbergen" : `Anzeigen (${REVEAL_SECONDS} s)`}
            aria-label={revealed !== null ? "Passwort verbergen" : "Passwort anzeigen"}
            onclick={toggleReveal}
          >
            <Icon name={revealed !== null ? "eye-slash" : "eye"} size={16} />
          </button>
          <button class="btn btn-icon btn-sm" title="Passwort kopieren" aria-label="Passwort kopieren" onclick={() => oncopy("password", "Passwort")}>
            <Icon name="copy" size={16} />
          </button>
        </dd>
      {/if}

      {#if detail.hasApiToken}
        <dt>API-Token</dt>
        <dd>
          <span class="value mono grow" class:masked={tokenRevealed === null}>{tokenRevealed ?? "••••••••••••••"}</span>
          <button
            class="btn btn-icon btn-sm"
            title={tokenRevealed !== null ? "Verbergen" : `Anzeigen (${REVEAL_SECONDS} s)`}
            aria-label={tokenRevealed !== null ? "API-Token verbergen" : "API-Token anzeigen"}
            onclick={toggleRevealToken}
          >
            <Icon name={tokenRevealed !== null ? "eye-slash" : "eye"} size={16} />
          </button>
          <button class="btn btn-icon btn-sm" title="API-Token kopieren" aria-label="API-Token kopieren" onclick={() => oncopy("apiToken", "API-Token")}>
            <Icon name="copy" size={16} />
          </button>
        </dd>
      {/if}

      {#if detail.totp}
        <dt>Einmalcode</dt>
        <dd><Totp {id} oncopy={() => oncopy("totp", "Einmalcode")} /></dd>
      {/if}

    </dl>
    {/if}

    {#if detail.url || detail.folderId || detail.agent.enabled}
    <h6 class="group-label">Website und Ablage</h6>
    <dl class="group">
      {#if detail.url}
        <dt>Website</dt>
        <dd><span class="value grow">{detail.url}</span></dd>
      {/if}

      {#if detail.agent.enabled}
        <dt>KI-Zugriff</dt>
        <dd>
          <span class="value grow">
            Freigegeben für {detail.agent.hosts.join(", ")}
            {#if detail.agent.auth.length}
              · Einsetzen als {detail.agent.auth.map(authLabel).join(", ")}
            {:else if !detail.agent.fillLogin}
              · nur Auflisten
            {/if}
            {#if detail.agent.fillLogin}· Login im Browser{/if}
          </span>
        </dd>
      {/if}

      {#if detail.folderId}
        <dt>Ordner</dt>
        <dd>
          <button class="btn btn-ghost folder" onclick={() => store.open(detail!.folderId)}>
            <Icon name="folder-simple" size={14} />
            {store.path(detail.folderId).map((f) => f.name).join(" / ")}
          </button>
        </dd>
      {/if}
    </dl>
    {/if}

    {#if detail.notes}
      <h6 class="group-label">Notizen</h6>
      <div class="group"><p class="value notes">{detail.notes}</p></div>
    {/if}

    <p class="meta">Erstellt {fmt(detail.createdAt)} · Geändert {fmt(detail.updatedAt)}</p>

    <div class="danger">
      {#if confirmDelete}
        <div class="confirm" role="group" aria-label="Löschen bestätigen">
          <span class="grow">„{detail.title}“ endgültig löschen?</span>
          <button class="btn btn-ghost" onclick={() => (confirmDelete = false)}>Abbrechen</button>
          <button class="btn btn-danger" onclick={remove}><Icon name="trash" size={15} /> Löschen</button>
        </div>
      {:else}
        <button class="btn btn-danger delete" onclick={() => (confirmDelete = true)}><Icon name="trash" size={15} /> Eintrag löschen …</button>
      {/if}
    </div>
  </article>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-6);
    margin-bottom: var(--space-6);
    padding-right: 40px;
  }
  .mark {
    width: 52px;
    height: 52px;
    flex: none;
    border-radius: var(--radius-lg);
    display: grid;
    place-items: center;
    font-size: 22px;
    font-weight: var(--font-heading-weight);
    background: var(--color-accent-900);
    color: var(--color-accent-300);
  }
  h3 {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .host {
    margin: 0;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .group-label {
    margin: var(--space-8) 0 var(--space-2) var(--space-4);
    color: var(--color-text-muted);
  }
  /* Zusammengehörige Felder als eine Fläche; Trennlinien nur zwischen den Zeilen. */
  .group {
    display: grid;
    grid-template-columns: 120px 1fr;
    margin: 0;
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    overflow: hidden;
  }
  dt,
  dd {
    min-height: 46px;
    padding: 4px var(--space-4);
  }
  dt:not(:first-of-type),
  dd:not(:first-of-type) {
    border-top: 1px solid var(--color-divider);
  }
  dt {
    display: flex;
    align-items: center;
    font-size: 12px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    padding-left: 0;
  }
  .value {
    font-size: 14px;
    padding: 7px 0;
    margin: 0;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .value.masked {
    letter-spacing: 0.1em;
    color: var(--color-text-muted);
  }
  .notes {
    white-space: pre-wrap;
    padding: var(--space-4);
  }
  .folder {
    font-size: 13px;
    padding-block: 6px;
  }
  .loading {
    font-size: 13px;
  }
  .meta {
    margin: var(--space-4) 0 0 var(--space-4);
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .danger {
    margin-top: var(--space-8);
  }
  .delete {
    width: 100%;
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    font-size: 13px;
  }
</style>
