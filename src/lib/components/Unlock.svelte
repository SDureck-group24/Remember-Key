<script lang="ts">
  import { api, errorText } from "$lib/api";
  import Gate from "./Gate.svelte";
  import Icon from "./Icon.svelte";

  let { ondone, notice = "" }: { ondone: () => void; notice?: string } = $props();

  let password = $state("");
  let busy = $state(false);
  let error = $state("");
  let input: HTMLInputElement;

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!password) return;
    busy = true;
    error = "";
    try {
      await api.unlock(password);
      password = "";
      ondone();
    } catch (err) {
      error = errorText(err);
      password = "";
      setTimeout(() => input?.focus());
    } finally {
      busy = false;
    }
  }
</script>

<Gate title="Tresor entsperren" lead={notice || "Geben Sie Ihr Master-Passwort ein."}>
  <form onsubmit={submit}>
    <div class="field">
      <label for="pw">Master-Passwort</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        id="pw"
        class="input"
        type="password"
        bind:this={input}
        bind:value={password}
        autocomplete="current-password"
        autofocus
        disabled={busy}
      />
    </div>

    {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <button class="btn btn-primary btn-block submit" disabled={!password || busy}>
      <Icon name="lock-simple" size={16} />
      {busy ? "Entsperre …" : "Entsperren"}
    </button>
  </form>
</Gate>

<style>
  .submit {
    margin-top: var(--space-8);
  }
  .error {
    margin-top: var(--space-4);
  }
</style>
