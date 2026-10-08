<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type SyncStatus } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Setup from "$lib/components/Setup.svelte";
  import Unlock from "$lib/components/Unlock.svelte";
  import Vault from "$lib/components/Vault.svelte";
  import AgentApproval from "$lib/components/AgentApproval.svelte";

  let view = $state<"loading" | "setup" | "unlock" | "vault">("loading");
  let lockReason = $state("");

  async function refresh() {
    const s = await api.status();
    view = !s.exists ? "setup" : s.unlocked ? "vault" : "unlock";
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

{#if view === "setup"}
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
