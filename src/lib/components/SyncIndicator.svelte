<!-- Kleiner Statusknopf in der Kopfzeile; Klick gleicht sofort ab. -->
<script lang="ts">
  import { api, errorText } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "$lib/icons";

  let s = $derived(store.sync);

  const meta: Record<string, { icon: IconName; label: string }> = {
    idle: { icon: "cloud-check", label: "Mit Google Drive synchronisiert" },
    syncing: { icon: "cloud", label: "Wird mit Google Drive abgeglichen …" },
    offline: { icon: "cloud-slash", label: "Google Drive nicht erreichbar" },
    error: { icon: "cloud-warning", label: "Fehler beim Abgleich" },
    "needs-password": { icon: "cloud-warning", label: "Master-Passwort für den Abgleich bestätigen" },
  };

  async function syncNow() {
    try {
      await api.syncNow();
    } catch (e) {
      store.notify(errorText(e));
    }
  }
</script>

{#if s?.connected}
  {@const m = meta[s.state] ?? meta.idle}
  <button
    class="btn btn-icon btn-secondary sync"
    data-state={s.state}
    title={s.message ? `${m.label}: ${s.message}` : m.label}
    aria-label={m.label}
    onclick={syncNow}
    disabled={s.state === "syncing"}
  >
    <span class:spin={s.state === "syncing"}><Icon name={m.icon} size={17} /></span>
  </button>
{/if}

<style>
  .sync[data-state="idle"],
  .sync[data-state="syncing"] {
    color: var(--color-accent);
  }
  .sync[data-state="error"],
  .sync[data-state="needs-password"] {
    color: var(--color-danger);
  }
  .sync[data-state="offline"] {
    color: var(--color-text-muted);
  }
  .sync:disabled {
    opacity: 1;
  }
  .spin {
    display: inline-flex;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
