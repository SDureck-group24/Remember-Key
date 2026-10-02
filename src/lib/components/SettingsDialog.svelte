<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, MIN_MASTER_LEN, MIN_MASTER_SCORE, type Settings } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import GoogleDrive from "./GoogleDrive.svelte";
  import Icon from "./Icon.svelte";
  import Strength from "./Strength.svelte";

  let { onclose, onnotify }: { onclose: () => void; onnotify: (msg: string) => void } = $props();

  let settings = $state<Settings | null>(null);
  let error = $state("");

  let current = $state("");
  let next = $state("");
  let confirm = $state("");
  let pwBusy = $state(false);
  let nextScore = $state(0);
  let pwError = $state("");

  onMount(async () => {
    try {
      settings = await api.getSettings();
    } catch (e) {
      error = errorText(e);
    }
  });

  async function saveSettings(e: SubmitEvent) {
    e.preventDefault();
    if (!settings) return;
    try {
      await api.setSettings({
        autoLockMinutes: Number(settings.autoLockMinutes),
        clipboardClearSeconds: Number(settings.clipboardClearSeconds),
      });
      onnotify("Einstellungen gespeichert");
      onclose();
    } catch (err) {
      error = errorText(err);
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

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="dialog-backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="set-title">
    <div class="row">
      <div class="dialog-title grow" id="set-title">Einstellungen</div>
      <button class="btn btn-icon btn-sm" aria-label="Schließen" onclick={onclose}><Icon name="x" size={16} /></button>
    </div>

    {#if settings}
      <form onsubmit={saveSettings}>
        <h6>Sicherheit</h6>
        <div class="grid">
          <div class="field">
            <label for="lock">Sperren nach Inaktivität</label>
            <div class="unit">
              <input id="lock" class="input" type="number" min="1" max="240" bind:value={settings.autoLockMinutes} />
              <span>Min.</span>
            </div>
          </div>
          <div class="field">
            <label for="clip">Zwischenablage leeren nach</label>
            <div class="unit">
              <input id="clip" class="input" type="number" min="5" max="300" bind:value={settings.clipboardClearSeconds} />
              <span>Sek.</span>
            </div>
          </div>
        </div>
        {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}
        <div class="dialog-actions">
          <button class="btn btn-primary"><Icon name="check" size={15} /> Speichern</button>
        </div>
      </form>
    {/if}

    <section class="drive">
      <h6>Google Drive</h6>
      <GoogleDrive />
    </section>

    <form class="master" onsubmit={changePassword}>
      <h6>Master-Passwort ändern</h6>
      <div class="field">
        <label for="cur">Aktuelles Passwort</label>
        <input id="cur" class="input" type="password" bind:value={current} autocomplete="current-password" />
      </div>
      <div class="field">
        <label for="new">Neues Passwort</label>
        <input id="new" class="input" type="password" bind:value={next} autocomplete="new-password" />
        <Strength password={next} bind:score={nextScore} />
      </div>
      <div class="field">
        <label for="new2">Neues Passwort wiederholen</label>
        <input id="new2" class="input" type="password" bind:value={confirm} autocomplete="new-password" />
      </div>
      {#if pwError}<p class="error"><Icon name="warning-circle" size={16} /> {pwError}</p>{/if}
      <div class="dialog-actions">
        <button
          class="btn btn-secondary"
          disabled={pwBusy || !current || [...next].length < MIN_MASTER_LEN || nextScore < MIN_MASTER_SCORE || next !== confirm}
        >
          <Icon name="key" size={15} />
          {pwBusy ? "Wird geändert …" : "Passwort ändern"}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .dialog {
    width: min(480px, 100%);
    max-height: 90vh;
    overflow-y: auto;
  }
  h6 {
    color: var(--color-text-muted);
    margin: var(--space-4) 0 var(--space-4);
  }
  .master,
  .drive {
    margin-top: var(--space-6);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-4);
  }
  .grid .field + .field {
    margin-top: 0;
  }
  .unit {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .field + .field {
    margin-top: var(--space-4);
  }
</style>
