<script lang="ts">
  import { api, errorText, type CopyField, type EntryDetail } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Totp from "./Totp.svelte";

  let {
    id,
    oncopy,
    onedit,
    ondeleted,
  }: {
    id: string;
    oncopy: (field: CopyField, label: string) => void;
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

{#if detail}
  <article class="view">
    <div class="row head">
      <h3 class="grow">{detail.title}</h3>
      <button class="btn btn-secondary" onclick={onedit}>
        <Icon name="pencil-simple" size={15} /> Bearbeiten
      </button>
      {#if confirmDelete}
        <button class="btn btn-danger" onclick={remove}><Icon name="trash" size={15} /> Endgültig löschen</button>
        <button class="btn btn-ghost" onclick={() => (confirmDelete = false)}>Abbrechen</button>
      {:else}
        <button class="btn btn-icon btn-secondary" title="Löschen" aria-label="Löschen" onclick={() => (confirmDelete = true)}>
          <Icon name="trash" size={16} />
        </button>
      {/if}
    </div>

    <dl class="fields">
      {#if detail.folderId}
        <dt>Ordner</dt>
        <dd>
          <button class="btn btn-ghost folder" onclick={() => store.open(detail!.folderId)}>
            <Icon name="folder-simple" size={14} />
            {store.path(detail.folderId).map((f) => f.name).join(" / ")}
          </button>
        </dd>
      {/if}

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

      {#if detail.url}
        <dt>Website</dt>
        <dd><span class="value grow">{detail.url}</span></dd>
      {/if}

      {#if detail.agent.enabled}
        <dt>KI-Zugriff</dt>
        <dd>
          <span class="value grow">
            <Icon name="robot" size={14} /> Freigegeben für {detail.agent.hosts.join(", ")}
            {#if detail.agent.auth.length}
              · Einsetzen als {detail.agent.auth
                .map((a) => (a.kind === "bearer" ? "Bearer-Token" : a.kind === "basic" ? "Basic-Auth" : `Header ${a.name}`))
                .join(", ")}
            {:else}
              · nur Auflisten
            {/if}
          </span>
        </dd>
      {/if}

      {#if detail.notes}
        <dt>Notizen</dt>
        <dd><span class="value notes grow">{detail.notes}</span></dd>
      {/if}
    </dl>

    <p class="meta">Erstellt {fmt(detail.createdAt)} · Geändert {fmt(detail.updatedAt)}</p>
  </article>
{/if}

<style>
  .view {
    max-width: 680px;
  }
  .head {
    margin-bottom: var(--space-8);
  }
  h3 {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .fields {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: var(--space-4) var(--space-6);
    margin: 0;
    align-items: start;
  }
  dt {
    font-size: 12px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
    padding-top: 9px;
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .value {
    font-size: 14px;
    padding: 7px 0;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .value.masked {
    letter-spacing: 0.1em;
    color: var(--color-text-muted);
  }
  .notes {
    white-space: pre-wrap;
  }
  .folder {
    font-size: 13px;
    padding-block: 6px;
  }
  .meta {
    margin-top: var(--space-8);
    font-size: 11px;
    color: var(--color-text-muted);
  }
</style>
