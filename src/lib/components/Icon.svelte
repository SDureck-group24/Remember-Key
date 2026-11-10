<script lang="ts">
  import { icons, type IconName } from "$lib/icons";

  let { name, size = 18 }: { name: IconName; size?: number } = $props();

  // Phosphor-Linien sind 1/16 der Größe; bei Windows-Skalierung wirken sie unter ~19 px unscharf.
  // Deshalb alle Symbole 3 px größer als angegeben, das Logo bleibt bei seiner Größe.
  let px = $derived(name === "logo" ? size : size + 3);
</script>

<span class="icon" style="width: {px}px; height: {px}px" aria-hidden="true">{@html icons[name]}</span>

<style>
  .icon {
    display: inline-flex;
    flex: none;
  }
  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }
</style>
