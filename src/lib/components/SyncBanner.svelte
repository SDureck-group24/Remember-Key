<!-- Erscheint, wenn der Stand in Google Drive nur mit dem Master-Passwort geöffnet werden kann. -->
<script lang="ts">
  import { api, errorText } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let password = $state("");
  let busy = $state(false);
  let error = $state("");

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      await api.syncNow(password);
      password = "";
      store.notify("Mit Google Drive abgeglichen");
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
      store.sync = await api.syncStatus();
    }
  }
</script>

{#if store.sync?.state === "needs-password"}
  <form class="banner" onsubmit={submit}>
    <Icon name="cloud-warning" size={20} />
    <div class="grow text">
      <strong>Abgleich mit Google Drive pausiert.</strong>
      <span>Der Stand in Google Drive ist mit einem anderen Schlüssel gespeichert, etwa nach einem Passwortwechsel auf einem anderen Gerät. Geben Sie das Master-Passwort ein, mit dem dieser Stand gespeichert wurde.</span>
      {#if error}<span class="error">{error}</span>{/if}
    </div>
    <input class="input" type="password" bind:value={password} placeholder="Master-Passwort" aria-label="Master-Passwort" autocomplete="current-password" />
    <button class="btn btn-primary" disabled={!password || busy}>{busy ? "Prüfe …" : "Abgleichen"}</button>
  </form>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: 0 var(--space-8) var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-danger-900);
    color: var(--color-danger);
    flex-wrap: wrap;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
    color: var(--color-text);
    min-width: 220px;
  }
  .text span {
    color: color-mix(in srgb, var(--color-text) 75%, transparent);
  }
  .input {
    width: 200px;
  }
</style>
