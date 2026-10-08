<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText } from "$lib/api";
  import Gate from "./Gate.svelte";
  import Icon from "./Icon.svelte";

  let { ondone, notice = "" }: { ondone: () => void; notice?: string } = $props();

  let password = $state("");
  let busy = $state(false);
  let error = $state("");
  let hello = $state(false);
  let input = $state<HTMLInputElement>();
  let helloButton = $state<HTMLButtonElement>();

  onMount(async () => {
    try {
      hello = (await api.status()).helloUnlock;
    } catch {
      hello = false;
    }
    if (hello) setTimeout(() => helloButton?.focus());
  });

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

  async function unlockHello() {
    busy = true;
    error = "";
    try {
      await api.unlockHello();
      ondone();
    } catch (err) {
      error = errorText(err);
      // Abgelaufen oder Schlüssel passt nicht mehr: nur noch Passwort anbieten.
      hello = (await api.status().catch(() => null))?.helloUnlock ?? false;
      if (!hello) setTimeout(() => input?.focus());
    } finally {
      busy = false;
    }
  }
</script>

<Gate
  title="Tresor entsperren"
  lead={notice || (hello ? "Mit Windows Hello oder dem Master-Passwort entsperren." : "Geben Sie Ihr Master-Passwort ein.")}
>
  {#if hello}
    <button class="btn btn-primary btn-block" bind:this={helloButton} disabled={busy} onclick={unlockHello}>
      <Icon name="fingerprint" size={16} />
      {busy ? "Warte auf Windows Hello …" : "Mit Windows Hello entsperren"}
    </button>
    <div class="or"><span>oder</span></div>
  {/if}

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
        autofocus={!hello}
        disabled={busy}
      />
    </div>

    {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <button class="btn btn-block submit" class:btn-primary={!hello} class:btn-secondary={hello} disabled={!password || busy}>
      <Icon name="lock-simple" size={16} />
      {busy && !hello ? "Entsperre …" : "Entsperren"}
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
  .or {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: var(--space-6) 0;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .or::before,
  .or::after {
    content: "";
    flex: 1;
    border-top: 1px solid var(--color-divider);
  }
</style>
