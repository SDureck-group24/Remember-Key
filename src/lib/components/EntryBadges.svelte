<!-- 2FA- und KI-Kennzeichen eines Eintrags. Feste Plätze, damit sie in allen Zeilen untereinander stehen. -->
<script lang="ts">
  import type { EntrySummary } from "$lib/api";
  import Icon from "./Icon.svelte";

  let { entry }: { entry: EntrySummary } = $props();
</script>

<span class="badges">
  <span class="totp" title={entry.hasTotp ? "Mit Zwei-Faktor-Code" : undefined}>{entry.hasTotp ? "2FA" : ""}</span>
  <span class="ai" title={entry.agent ? "Für KI-Assistenten freigegeben" : undefined}>
    {#if entry.agent}<Icon name="sparkle" size={15} /><span class="sr">Für KI freigegeben</span>{/if}
  </span>
</span>

<style>
  .badges {
    display: grid;
    grid-template-columns: 26px 15px;
    align-items: center;
    gap: var(--space-2);
  }
  .totp {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
  }
  .ai {
    display: inline-flex;
    color: var(--color-accent-2);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
