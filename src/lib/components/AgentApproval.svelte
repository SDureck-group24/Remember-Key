<!-- Bestätigung, bevor Remember Key ein Passwort in eine Anfrage der KI einsetzt. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errorText, type ApprovalDecision, type PendingApproval } from "$lib/api";
  import Icon from "./Icon.svelte";

  let queue = $state<PendingApproval[]>([]);
  let current = $derived(queue[0]);
  let now = $state(Date.now() / 1000);
  let busy = $state(false);
  let error = $state("");

  let remaining = $derived(current ? Math.max(0, Math.ceil(current.expiresAt - now)) : 0);

  function authText(auth: string): string {
    if (auth === "bearer") return "als Bearer-Token (Authorization-Header)";
    if (auth === "basic") return "als Basic-Anmeldung mit Benutzername";
    if (auth.startsWith("header:")) return `im Header ${auth.slice(7)}`;
    if (auth.startsWith("form:")) return `im Formularfeld ${auth.slice(5)} des Bodys`;
    if (auth.startsWith("json:")) return `im JSON-Feld ${auth.slice(5)} des Bodys`;
    return auth;
  }

  function add(p: PendingApproval) {
    if (!queue.some((q) => q.id === p.id)) queue.push(p);
  }

  function remove(id: number) {
    queue = queue.filter((q) => q.id !== id);
    error = "";
  }

  onMount(() => {
    api
      .agentPending()
      .then((list) => list.forEach(add))
      .catch(() => {});
    const timer = setInterval(() => (now = Date.now() / 1000), 500);
    const unlisteners = [
      listen<PendingApproval>("agent-approval", (e) => add(e.payload)),
      listen<number>("agent-approval-done", (e) => remove(e.payload)),
    ];
    return () => {
      clearInterval(timer);
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  });

  async function decide(decision: ApprovalDecision) {
    if (!current || busy) return;
    const id = current.id;
    busy = true;
    error = "";
    try {
      await api.agentDecide(id, decision);
      remove(id);
    } catch (e) {
      // z. B. Windows Hello abgebrochen: Anfrage bleibt offen – erneut versuchen oder ablehnen.
      error = errorText(e);
      if (decision === "deny") remove(id);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (current && e.key === "Escape") decide("deny");
  }
</script>

<svelte:window onkeydown={onKey} />

{#if current}
  <div class="dialog-backdrop">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="appr-title" aria-describedby="appr-body">
      <div class="row">
        <div class="dialog-title grow" id="appr-title">KI-Anfrage bestätigen</div>
        <span class="countdown" title="Wird danach automatisch abgelehnt">{remaining} s</span>
      </div>

      <div class="dialog-body" id="appr-body">
        <p>Ein KI-Assistent möchte die Zugangsdaten von <strong>{current.entryTitle}</strong> verwenden:</p>
        {#if current.action === "fill"}
          <dl>
            <dt>Aktion</dt>
            <dd>
              Login im Browser ausfüllen und absenden{current.auth.includes("totp")
                ? ". Fragt die Seite nach einem 2FA-Code, setzt Remember Key den aktuellen Code ein."
                : ""}
            </dd>
            <dt>Seite</dt>
            <dd class="mono">{current.host}</dd>
          </dl>
          <p class="hint">
            Die Remember-Key-Erweiterung füllt nur auf dieser Seite aus. Die KI sieht das Passwort nicht. Steuert sie den
            Browser, könnte sie das Feld vor dem Absenden aber technisch auslesen. Erlauben Sie das nur, wenn Sie die Anfrage
            erwarten.
          </p>
        {:else}
          <dl>
            <dt>Anfrage</dt>
            <dd class="mono">{current.method} {current.host}{current.path}</dd>
            {#if current.session === "use"}
              <dt>Anmeldung</dt>
              <dd>über die bestehende Sitzung (Cookies aus dem Login), ohne Passwort</dd>
            {:else}
              <dt>Passwort</dt>
              <dd>wird {authText(current.auth)} eingesetzt</dd>
            {/if}
            {#if current.session === "new"}
              <dt>Sitzung</dt>
              <dd>Remember Key behält die Anmelde-Cookies für Folgeanfragen an {current.host}</dd>
            {/if}
          </dl>
          <p class="hint">
            Die KI sieht das Passwort nicht; es wird auch aus der Antwort entfernt.{current.session
              ? " Die Sitzungs-Cookies sieht sie ebenfalls nicht. Die Sitzung endet nach 15 Min. ohne Nutzung, spätestens nach 1 Std. und beim Sperren."
              : ""}
          </p>
        {/if}
      </div>

      {#if queue.length > 1}<p class="hint">Weitere wartende Anfragen: {queue.length - 1}</p>{/if}
      {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

      <div class="dialog-actions">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn btn-secondary" autofocus disabled={busy} onclick={() => decide("deny")}>
          <Icon name="x" size={15} /> Ablehnen
        </button>
        {#if current.sessionMinutes > 0}
          <button class="btn btn-secondary" disabled={busy} onclick={() => decide("session")}>
            Für {current.sessionMinutes} Min. erlauben
          </button>
        {/if}
        <button class="btn btn-primary" disabled={busy} onclick={() => decide("once")}>
          <Icon name="check" size={15} /> Einmal erlauben
        </button>
      </div>
      {#if current.sessionMinutes > 0}
        <p class="hint">
          „Für {current.sessionMinutes} Min.“ gilt für alle {current.action === "fill" ? "Logins auf" : "Anfragen an"}
          {current.host} mit diesem Eintrag, bis der Tresor gesperrt wird.
        </p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(480px, 100%);
  }
  .countdown {
    font-size: 12px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-2) var(--space-4);
    margin: var(--space-4) 0;
  }
  dt {
    color: var(--color-text-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .dialog-actions {
    flex-wrap: wrap;
  }
</style>
