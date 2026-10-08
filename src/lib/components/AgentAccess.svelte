<!-- KI-Zugriff: Einrichtung der MCP-Brücke und Protokoll der Anfragen. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errorText, type AgentInfo, type AgentOutcome } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let { enabled }: { enabled: boolean } = $props();

  let info = $state<AgentInfo | null>(null);
  let error = $state("");

  const PERMISSION = "mcp__remember-key";

  let command = $derived(info?.bridgePath ? `claude mcp add remember-key -- "${info.bridgePath}"` : "");

  const OUTCOMES: Record<AgentOutcome, { label: string; cls: string }> = {
    ok: { label: "OK", cls: "tag-accent" },
    locked: { label: "Gesperrt", cls: "tag-neutral" },
    disabled: { label: "Ausgeschaltet", cls: "tag-neutral" },
    rejected: { label: "Abgewiesen", cls: "tag-outline" },
    denied: { label: "Abgelehnt", cls: "tag-outline" },
    failed: { label: "Fehler", cls: "tag-outline" },
  };

  async function refresh() {
    try {
      info = await api.agentInfo();
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(() => {
    refresh();
    const unlisten = listen("agent-activity", refresh);
    return () => {
      unlisten.then((f) => f());
    };
  });

  async function copy(text: string, message: string) {
    try {
      await api.copyText(text);
      store.notify(message);
    } catch (e) {
      error = errorText(e);
    }
  }

  const fmt = (ts: number) =>
    new Date(ts * 1000).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "medium" });
</script>

<p class="hint">
  KI-Assistenten wie Claude sehen über die MCP-Brücke nur freigegebene Einträge und nur deren Titel, Benutzernamen
  und Hosts – nie Passwörter, Notizen oder 2FA-Schlüssel. Ist es beim Eintrag erlaubt, setzt Remember Key das Passwort
  nach deiner Bestätigung selbst in HTTPS-Anfragen ein. Freigeben lässt sich ein Eintrag beim Bearbeiten.
</p>

{#if info}
  {#if enabled}
    {#if command}
      <div class="field">
        <span class="lbl">Einrichten in Claude Code</span>
        <div class="row">
          <code class="cmd grow">{command}</code>
          <button type="button" class="btn btn-icon btn-sm" title="Befehl kopieren" aria-label="Befehl kopieren" onclick={() => copy(command, "Befehl kopiert")}>
            <Icon name="copy" size={15} />
          </button>
        </div>
      </div>
      <div class="field">
        <span class="lbl">Berechtigung in Claude Code</span>
        <div class="row">
          <code class="cmd grow">{PERMISSION}</code>
          <button type="button" class="btn btn-icon btn-sm" title="Regel kopieren" aria-label="Regel kopieren" onclick={() => copy(PERMISSION, "Regel kopiert")}>
            <Icon name="copy" size={15} />
          </button>
        </div>
        <p class="hint">
          In <span class="mono">~/.claude/settings.json</span> unter <span class="mono">permissions.allow</span> eintragen.
          Sonst kann der Auto-Modus von Claude Code Aufrufe blockieren, bevor sie Remember Key erreichen. Jede Anfrage
          mit Passwort bestätigst du trotzdem hier.
        </p>
      </div>
    {:else}
      <p class="error"><Icon name="warning-circle" size={16} /> Die MCP-Brücke (remember-key-mcp.exe) fehlt neben der App.</p>
    {/if}

    <div class="field">
      <span class="lbl">Browser-Erweiterung (Login ausfüllen)</span>
      {#if info.browsers.length}
        <p><span class="tag tag-accent">Verbunden</span> {info.browsers.join(", ")}</p>
      {:else}
        <p class="hint">Nicht verbunden. Die Erweiterung verbindet sich innerhalb von 30 s, sobald der Browser läuft.</p>
      {/if}
      {#if info.extensionDir}
        <div class="row">
          <code class="cmd grow">{info.extensionDir}</code>
          <button type="button" class="btn btn-icon btn-sm" title="Pfad kopieren" aria-label="Pfad kopieren" onclick={() => copy(info!.extensionDir!, "Pfad kopiert")}>
            <Icon name="copy" size={15} />
          </button>
        </div>
        <ul class="steps hint">
          <li>
            <strong>Chrome/Edge:</strong> <span class="mono">chrome://extensions</span> öffnen, „Entwicklermodus“ einschalten,
            „Entpackte Erweiterung laden“ und diesen Ordner wählen.
          </li>
          <li>
            <strong>Zen/Firefox:</strong> <span class="mono">about:debugging#/runtime/this-firefox</span> → „Temporäres Add-on
            laden“ → <span class="mono">manifest.json</span> aus dem Ordner. Gilt bis zum Neustart des Browsers; danach unter
            Add-ons den Zugriff auf alle Websites erlauben.
          </li>
        </ul>
      {:else}
        <p class="error"><Icon name="warning-circle" size={16} /> Der Ordner der Erweiterung fehlt neben der App.</p>
      {/if}
    </div>
  {/if}

  <div class="field">
    <span class="lbl">Protokoll</span>
    {#if info.log.length}
      <ul class="log">
        {#each info.log as l, i (i)}
          <li>
            <span class="time">{fmt(l.ts)}</span>
            <span class="what grow"><span class="mono">{l.tool}</span>{l.detail ? ` · ${l.detail}` : ""}</span>
            <span class="tag {OUTCOMES[l.outcome].cls}">{OUTCOMES[l.outcome].label}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint">Noch keine Anfragen.</p>
    {/if}
  </div>
{/if}
{#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

<style>
  /* Wie .field > label */
  .lbl {
    display: block;
    font-size: 12px;
    margin-bottom: 5px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
  }
  .field {
    margin-top: var(--space-4);
  }
  .cmd {
    font-size: 12px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-text) 6%, transparent);
    overflow-wrap: anywhere;
  }
  .steps {
    margin: var(--space-2) 0 0;
    padding-left: var(--space-6);
  }
  .steps li + li {
    margin-top: var(--space-1);
  }
  .log {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 220px;
    overflow-y: auto;
    font-size: 12px;
  }
  .log li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--color-divider);
  }
  .time {
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .what {
    overflow-wrap: anywhere;
  }
</style>
