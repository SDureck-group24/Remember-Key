<script lang="ts">
  import { api, errorText, MIN_MASTER_LEN, MIN_MASTER_SCORE } from "$lib/api";
  import Gate from "./Gate.svelte";
  import GoogleDrive from "./GoogleDrive.svelte";
  import Icon from "./Icon.svelte";
  import Strength from "./Strength.svelte";

  let { ondone }: { ondone: () => void } = $props();

  let password = $state("");
  let confirm = $state("");
  let busy = $state(false);
  let error = $state("");
  let restoreMode = $state(false);

  let score = $state(0);
  let valid = $derived(
    [...password].length >= MIN_MASTER_LEN && password === confirm && score >= MIN_MASTER_SCORE,
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      await api.create(password);
      password = confirm = "";
      ondone();
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if restoreMode}
  <Gate
    title="Tresor aus Google Drive laden"
    lead="Verbinden Sie Google Drive mit derselben Client-ID wie auf Ihrem anderen Gerät. Danach entsperren Sie mit Ihrem bisherigen Master-Passwort."
  >
    <GoogleDrive mode="restore" onrestored={ondone} />
    <button class="btn btn-ghost back" onclick={() => (restoreMode = false)}>
      <Icon name="arrow-left" size={15} /> Stattdessen neuen Tresor anlegen
    </button>
  </Gate>
{:else}
<Gate
  title="Neuen Tresor anlegen"
  lead="Das Master-Passwort verschlüsselt alle Einträge. Es wird nirgends gespeichert und kann nicht wiederhergestellt werden."
>
  <form onsubmit={submit}>
    <div class="field">
      <label for="pw">Master-Passwort</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="pw" class="input" type="password" bind:value={password} autocomplete="new-password" autofocus />
      <Strength {password} bind:score />
      <div class="hint">
        Mindestens {MIN_MASTER_LEN} Zeichen und Stärke „stark“. Ideal sind vier oder mehr zufällige Wörter,
        z. B. „Tafel Krokus Winkel Ozean“. Bekannte Wörter, Namen und Jahreszahlen zählen kaum.
      </div>
    </div>

    <div class="field">
      <label for="pw2">Wiederholen</label>
      <input id="pw2" class="input" type="password" bind:value={confirm} autocomplete="new-password" />
      {#if confirm && confirm !== password}
        <div class="error hint"><Icon name="warning-circle" size={14} /> Die Passwörter stimmen nicht überein.</div>
      {/if}
    </div>

    {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <button class="btn btn-primary btn-block submit" disabled={!valid || busy}>
      <Icon name="shield-check" size={16} />
      {busy ? "Wird angelegt …" : "Tresor anlegen"}
    </button>
  </form>

  <button class="btn btn-ghost restore" onclick={() => (restoreMode = true)}>
    <Icon name="google-drive-logo" size={15} /> Vorhandenen Tresor aus Google Drive laden
  </button>
</Gate>
{/if}

<style>
  .submit {
    margin-top: var(--space-8);
  }
  .restore,
  .back {
    margin-top: var(--space-6);
    padding-inline: 0;
  }
</style>
