<!-- Gemeinsames Gerüst für Anlegen und Entsperren: zentrierte Spalte. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let { title, lead, children }: { title: string; lead: string; children: Snippet } = $props();
</script>

<div class="gate">
  <div class="column">
    <div class="brand">
      <Icon name="key" size={22} />
      <span>Remember Key</span>
    </div>
    <h2>{title}</h2>
    <p class="text-muted lead">{lead}</p>
    {@render children()}
  </div>
</div>

<style>
  .gate {
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-inline: var(--space-8);
    background:
      radial-gradient(70% 60% at 50% 100%, color-mix(in srgb, var(--color-accent-900) 70%, transparent), transparent 60%),
      var(--color-bg);
  }
  .column {
    width: min(380px, 100%);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-accent);
    font-size: 14px;
    margin-bottom: var(--space-8);
  }
  .lead {
    font-size: 14px;
    margin-bottom: var(--space-8);
  }
</style>
