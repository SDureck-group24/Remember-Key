<script lang="ts">
  import { store } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";

  let { initial = "", depth = 0 }: { initial?: string; depth?: number } = $props();

  // Nur der Startwert wird übernommen; danach gehört der Text dem Eingabefeld.
  // svelte-ignore state_referenced_locally
  let value = $state(initial);
  let done = false;

  function commit() {
    if (done) return;
    done = true;
    store.commitName(value);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      done = true;
      store.cancelEdit();
    }
  }
</script>

<div class="name-input" style="--depth: {depth}">
  <Icon name="folder-simple" size={15} />
  <!-- svelte-ignore a11y_autofocus -->
  <input
    class="input"
    bind:value
    onkeydown={onKey}
    onblur={commit}
    placeholder="Ordnername"
    aria-label="Ordnername"
    maxlength="80"
    autofocus
    spellcheck="false"
  />
</div>

<style>
  .name-input {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 2px var(--space-2) 2px calc(var(--space-3) + 22px + var(--depth) * 14px);
    color: var(--color-accent);
  }
  .input {
    min-height: 30px;
    padding: 4px 8px;
    font-size: 13px;
  }
</style>
