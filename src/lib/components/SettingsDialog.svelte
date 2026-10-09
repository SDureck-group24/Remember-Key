<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { api, errorText, MIN_MASTER_LEN, MIN_MASTER_SCORE, type HelloUnlockStatus, type Settings } from "$lib/api";
  import type { IconName } from "$lib/icons";
  import { store } from "$lib/store.svelte";
  import AgentAccess from "./AgentAccess.svelte";
  import GoogleDrive from "./GoogleDrive.svelte";
  import Icon from "./Icon.svelte";
  import Strength from "./Strength.svelte";
  import Switch from "./Switch.svelte";

  let { onclose, onnotify }: { onclose: () => void; onnotify: (msg: string) => void } = $props();

  type Tab = "security" | "agent" | "drive" | "master";
  const TABS: { id: Tab; label: string; icon: IconName }[] = [
    { id: "security", label: "Sicherheit", icon: "shield-check" },
    { id: "agent", label: "KI-Zugriff", icon: "plugs-connected" },
    { id: "drive", label: "Google Drive", icon: "google-drive-logo" },
    { id: "master", label: "Master-Passwort", icon: "key" },
  ];
  let tab = $state<Tab>("security");
  let tabButtons: HTMLButtonElement[] = [];

  let settings = $state<Settings | null>(null);
  let loadError = $state("");
  let saveError = $state("");
  let saved = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  let hello = $state<HelloUnlockStatus | null>(null);
  let helloBusy = $state(false);
  let helloError = $state("");

  let current = $state("");
  let next = $state("");
  let confirm = $state("");
  let pwBusy = $state(false);
  let nextScore = $state(0);
  let pwError = $state("");

  let nextLen = $derived([...next].length);
  let mismatch = $derived(confirm.length > 0 && next !== confirm);
  let canChange = $derived(
    !pwBusy && !!current && nextLen >= MIN_MASTER_LEN && nextScore >= MIN_MASTER_SCORE && next === confirm,
  );

  onMount(async () => {
    tabButtons[0]?.focus();
    try {
      settings = await api.getSettings();
    } catch (e) {
      loadError = errorText(e);
    }
    hello = await api.helloUnlockStatus().catch(() => null);
  });

  onDestroy(() => clearTimeout(savedTimer));

  /** Jede Änderung gilt sofort, wie schon das Entsperren mit Windows Hello. */
  async function persist() {
    if (!settings) return;
    saveError = "";
    try {
      await api.setSettings({
        autoLockMinutes: Number(settings.autoLockMinutes),
        clipboardClearSeconds: Number(settings.clipboardClearSeconds),
        agentEnabled: settings.agentEnabled,
        agentHello: settings.agentHello,
        helloUnlockHours: Number(settings.helloUnlockHours),
      });
      saved = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (saved = false), 2500);
    } catch (err) {
      saveError = errorText(err);
    }
  }

  /** Zahlenfelder speichern erst, wenn der Wert im erlaubten Bereich liegt. */
  function persistNumber(e: Event & { currentTarget: HTMLInputElement }, what: string, unit: string) {
    const input = e.currentTarget;
    if (input.value === "" || !input.checkValidity()) {
      saveError = `${what}: einen Wert von ${input.min} bis ${input.max} ${unit} eingeben.`;
      return;
    }
    persist();
  }

  async function toggleHello(e: Event & { currentTarget: HTMLInputElement }) {
    const box = e.currentTarget;
    const enabled = box.checked;
    helloBusy = true;
    helloError = "";
    try {
      await api.setHelloUnlock(enabled);
      hello = { available: true, enabled };
    } catch (err) {
      helloError = errorText(err);
      box.checked = !enabled;
    } finally {
      helloBusy = false;
    }
  }

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    pwBusy = true;
    pwError = "";
    try {
      await api.changeMaster(current, next);
      current = next = confirm = "";
      onnotify(
        store.sync?.connected
          ? "Master-Passwort geändert. Alte Sicherungen wurden gelöscht, die Kopie in Google Drive wird ersetzt."
          : "Master-Passwort geändert. Alte Sicherungen wurden gelöscht.",
      );
    } catch (err) {
      pwError = errorText(err);
    } finally {
      pwBusy = false;
    }
  }

  /** Pfeiltasten wechseln zwischen den Bereichen (WAI-ARIA Tabs). */
  async function onTabKey(e: KeyboardEvent, i: number) {
    const keys: Record<string, number> = {
      ArrowDown: i + 1,
      ArrowRight: i + 1,
      ArrowUp: i - 1,
      ArrowLeft: i - 1,
      Home: 0,
      End: TABS.length - 1,
    };
    if (!(e.key in keys)) return;
    e.preventDefault();
    const n = (keys[e.key] + TABS.length) % TABS.length;
    tab = TABS[n].id;
    await tick();
    tabButtons[n]?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="dialog-backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog settings" role="dialog" aria-modal="true" aria-labelledby="set-title">
    <nav class="side">
      <div class="dialog-title" id="set-title">Einstellungen</div>
      <div class="tabs" role="tablist" aria-orientation="vertical" aria-labelledby="set-title">
        {#each TABS as t, i (t.id)}
          <button
            bind:this={tabButtons[i]}
            id="tab-{t.id}"
            class="tab"
            class:active={tab === t.id}
            role="tab"
            aria-selected={tab === t.id}
            aria-controls="panel-{t.id}"
            tabindex={tab === t.id ? 0 : -1}
            onclick={() => (tab = t.id)}
            onkeydown={(e) => onTabKey(e, i)}
          >
            <Icon name={t.icon} size={16} />
            {t.label}
          </button>
        {/each}
      </div>
      <p class="autosave" role="status">
        {#if saved}
          <Icon name="check" size={14} /> Gespeichert
        {:else if tab === "security" || tab === "agent"}
          Änderungen gelten sofort.
        {/if}
      </p>
    </nav>

    <div class="panel" id="panel-{tab}" role="tabpanel" aria-labelledby="tab-{tab}" tabindex="-1">
      <header class="panel-head">
        <h4>{TABS.find((t) => t.id === tab)?.label}</h4>
        <button class="btn btn-icon btn-sm" aria-label="Einstellungen schließen" onclick={onclose}>
          <Icon name="x" size={16} />
        </button>
      </header>

      {#if tab === "security" || tab === "agent"}
        {#if settings}
          {#if tab === "security"}
            <div class="setting">
              <label class="text" for="lock">
                <span class="label">Automatisch sperren</span>
                <span class="desc">Nach so vielen Minuten ohne Eingabe wird der Tresor gesperrt.</span>
              </label>
              <span class="unit">
                <input
                  id="lock"
                  class="input"
                  type="number"
                  min="1"
                  max="240"
                  bind:value={settings.autoLockMinutes}
                  onchange={(e) => persistNumber(e, "Automatisch sperren", "Minuten")}
                />
                Min.
              </span>
            </div>
            <div class="setting">
              <label class="text" for="clip">
                <span class="label">Zwischenablage leeren</span>
                <span class="desc">Kopierte Passwörter und Codes werden danach aus der Zwischenablage entfernt.</span>
              </label>
              <span class="unit">
                <input
                  id="clip"
                  class="input"
                  type="number"
                  min="5"
                  max="300"
                  bind:value={settings.clipboardClearSeconds}
                  onchange={(e) => persistNumber(e, "Zwischenablage leeren", "Sekunden")}
                />
                Sek.
              </span>
            </div>

            {#if hello}
              <Switch
                checked={hello.enabled}
                disabled={!hello.available || helloBusy}
                label="Mit Windows Hello entsperren"
                description={hello.available
                  ? "Gilt nur auf diesem Gerät. Nach einem Neustart der App ist das Master-Passwort nötig."
                  : "Auf diesem Gerät ist Windows Hello nicht eingerichtet."}
                onchange={toggleHello}
              />
              {#if hello.enabled}
                <div class="setting nested">
                  <label class="text" for="hello-hours">
                    <span class="label">Master-Passwort erneut verlangen</span>
                    <span class="desc">Spätestens nach dieser Zeit reicht Windows Hello allein nicht mehr.</span>
                  </label>
                  <span class="unit">
                    <input
                      id="hello-hours"
                      class="input"
                      type="number"
                      min="1"
                      max="24"
                      bind:value={settings.helloUnlockHours}
                      onchange={(e) => persistNumber(e, "Master-Passwort erneut verlangen", "Stunden")}
                    />
                    Std.
                  </span>
                </div>
              {/if}
              {#if helloError}<p class="error"><Icon name="warning-circle" size={16} /> {helloError}</p>{/if}
            {/if}
          {:else}
            <Switch
              bind:checked={settings.agentEnabled}
              label="KI-Assistenten Zugriff erlauben"
              description="Claude und andere MCP-Clients sehen nur Einträge, die Sie beim Bearbeiten freigeben, und davon nur Titel, Benutzername und Hosts."
              onchange={persist}
            />
            <Switch
              bind:checked={settings.agentHello}
              disabled={!settings.agentEnabled}
              label="Freigaben mit Windows Hello bestätigen"
              description="PIN, Fingerabdruck oder Gesicht, zusätzlich zum Klick im Bestätigungsdialog."
              onchange={persist}
            />
            <AgentAccess enabled={settings.agentEnabled} />
          {/if}
          {#if saveError}<p class="error" role="alert"><Icon name="warning-circle" size={16} /> {saveError}</p>{/if}
        {:else if loadError}
          <p class="error" role="alert"><Icon name="warning-circle" size={16} /> {loadError}</p>
        {:else}
          <p class="hint" role="status">Einstellungen werden geladen …</p>
        {/if}
      {:else if tab === "drive"}
        <GoogleDrive />
      {:else}
        <form class="master" onsubmit={changePassword}>
          <p class="hint lead">
            Nach der Änderung werden alte Sicherungen gelöscht{store.sync?.connected
              ? " und die Kopie in Google Drive ersetzt"
              : ""}. Mit dem alten Passwort lässt sich der Tresor dann nicht mehr öffnen.
          </p>
          <div class="field">
            <label for="cur">Aktuelles Passwort</label>
            <input id="cur" class="input" type="password" bind:value={current} autocomplete="current-password" />
          </div>
          <div class="field">
            <label for="new">Neues Passwort</label>
            <input
              id="new"
              class="input"
              type="password"
              bind:value={next}
              autocomplete="new-password"
              aria-describedby="new-hint"
            />
            {#if next}<Strength password={next} bind:score={nextScore} />{/if}
            {#if nextLen < MIN_MASTER_LEN}
              <p class="hint" id="new-hint">Mindestens {MIN_MASTER_LEN} Zeichen. Eine Passphrase aus mehreren Wörtern ist gut.</p>
            {/if}
          </div>
          <div class="field">
            <label for="new2">Neues Passwort wiederholen</label>
            <input
              id="new2"
              class="input"
              type="password"
              bind:value={confirm}
              autocomplete="new-password"
              aria-invalid={mismatch}
            />
            {#if mismatch}<p class="hint warn">Die beiden Passwörter stimmen nicht überein.</p>{/if}
          </div>
          {#if pwError}<p class="error" role="alert"><Icon name="warning-circle" size={16} /> {pwError}</p>{/if}
          <button class="btn btn-primary submit" disabled={!canChange}>
            <Icon name="key" size={15} />
            {pwBusy ? "Wird geändert …" : "Master-Passwort ändern"}
          </button>
        </form>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings {
    width: min(780px, 100%);
    height: min(600px, 100%);
    padding: 0;
    gap: 0;
    display: grid;
    grid-template-columns: 200px 1fr;
    overflow: hidden;
  }

  /* Linke Spalte im Ton der App-Seitenleiste, damit der Dialog als Teil der App liest. */
  .side {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    padding: var(--space-6) var(--space-3) var(--space-4);
    background: color-mix(in srgb, var(--color-bg) 55%, var(--color-surface));
  }
  .side .dialog-title {
    padding-left: var(--space-3);
  }
  .tabs {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 8px var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .tab:hover {
    background: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  .tab.active {
    background: var(--color-accent-900);
    color: var(--color-accent-200);
  }
  .tab.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 2px;
    border-radius: 1px;
    background: var(--color-accent);
  }
  .tab:focus-visible {
    outline-offset: -2px;
  }
  .autosave {
    margin: auto 0 0;
    padding-left: var(--space-3);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .panel {
    overflow-y: auto;
    padding: var(--space-6) var(--space-8) var(--space-8);
  }
  .panel:focus {
    outline: none;
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0 calc(var(--space-3) * -1) var(--space-4) 0;
  }
  .panel-head h4 {
    margin: 0;
  }

  /* Zeile wie im Switch: Text links, Bedienelement rechts. */
  .setting {
    display: flex;
    align-items: flex-start;
    gap: var(--space-6);
    padding: var(--space-3) 0;
  }
  .setting.nested {
    margin-left: var(--space-6);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 14px;
  }
  .desc {
    font-size: 12px;
    line-height: 1.45;
    color: var(--color-text-muted);
  }
  .unit {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 116px;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .unit .input {
    width: 72px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .master {
    max-width: 380px;
  }
  .lead {
    margin: 0 0 var(--space-6);
    font-size: 13px;
  }
  .field + .field {
    margin-top: var(--space-4);
  }
  .warn {
    color: var(--color-danger);
  }
  .submit {
    margin-top: var(--space-8);
  }
  .error {
    margin-top: var(--space-4);
  }

  @media (max-width: 640px) {
    .settings {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
      height: 100%;
    }
    .side {
      gap: var(--space-3);
      padding-bottom: var(--space-3);
    }
    .tabs {
      flex-direction: row;
      overflow-x: auto;
    }
    .tab {
      flex: none;
    }
    .tab.active::before {
      display: none;
    }
    .autosave {
      margin: 0;
    }
    .panel {
      padding: var(--space-4);
    }
  }
</style>
