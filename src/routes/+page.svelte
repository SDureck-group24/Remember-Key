<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errorText, type SyncStatus } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import { store } from "$lib/store.svelte";
  import Setup from "$lib/components/Setup.svelte";
  import Unlock from "$lib/components/Unlock.svelte";
  import Vault from "$lib/components/Vault.svelte";
  import AgentApproval from "$lib/components/AgentApproval.svelte";

  let view = $state<"loading" | "error" | "setup" | "unlock" | "vault">("loading");
  let startError = $state("");
  let lockReason = $state("");

  async function refresh() {
    try {
      const s = await api.status();
      view = !s.exists ? "setup" : s.unlocked ? "vault" : "unlock";
    } catch (e) {
      startError = errorText(e);
      view = "error";
    }
  }

  async function lockNow() {
    await api.lock();
    lockReason = "";
    view = "unlock";
  }

  onMount(() => {
    refresh();
    api.syncStatus().then((s) => (store.sync = s));
    const unlisteners = [
      listen<string>("vault-locked", (e) => {
        lockReason =
          e.payload === "system"
            ? "Der Tresor wurde gesperrt, weil Windows gesperrt wurde oder in den Energiesparmodus ging."
            : "Der Tresor wurde wegen Inaktivität gesperrt.";
        view = "unlock";
      }),
      listen<SyncStatus>("sync-status", (e) => (store.sync = e.payload)),
      listen("vault-changed", () => {
        if (view === "vault") store.load();
      }),
    ];
    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  });
</script>

{#if view === "error"}
  <div class="start" role="alert">
    <h4>Remember Key konnte den Tresor nicht prüfen</h4>
    <p class="error"><Icon name="warning-circle" size={16} /> {startError}</p>
    <button class="btn btn-secondary" onclick={() => ((view = "loading"), refresh())}>
      <Icon name="arrows-clockwise" size={15} /> Erneut versuchen
    </button>
  </div>
{:else if view === "setup"}
  <Setup ondone={refresh} />
{:else if view === "unlock"}
  <Unlock
    notice={lockReason}
    ondone={() => {
      lockReason = "";
      refresh();
    }}
  />
{:else if view === "vault"}
  <Vault onlock={lockNow} />
  <AgentApproval />
{/if}

<style>
  .start {
    max-width: 440px;
    margin: 20vh auto 0;
    padding-inline: var(--space-8);
  }
</style>
