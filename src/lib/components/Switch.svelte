<!-- Ein/Aus-Einstellung als Zeile: Beschriftung und Erklärung links, Schalter rechts.
     Natives Checkbox-Element mit role="switch", damit Tastatur und Screenreader funktionieren. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    checked = $bindable(false),
    disabled = false,
    label,
    description = "",
    onchange,
    children,
  }: {
    checked?: boolean;
    disabled?: boolean;
    label: string;
    description?: string;
    onchange?: (e: Event & { currentTarget: HTMLInputElement }) => void;
    /** Zusätzlicher Inhalt unter der Erklärung, z. B. ein Link. */
    children?: Snippet;
  } = $props();
</script>

<label class="switch" class:disabled>
  <span class="text">
    <span class="label">{label}</span>
    {#if description}<span class="desc">{description}</span>{/if}
    {@render children?.()}
  </span>
  <!-- Erst den Wert übernehmen, dann onchange: der Aufrufer sieht so schon den neuen Zustand. -->
  <input
    type="checkbox"
    role="switch"
    {checked}
    {disabled}
    onchange={(e) => {
      checked = e.currentTarget.checked;
      onchange?.(e);
    }}
  />
  <span class="track" aria-hidden="true"><span class="thumb"></span></span>
</label>

<style>
  .switch {
    display: flex;
    align-items: flex-start;
    gap: var(--space-6);
    padding: var(--space-3) 0;
    cursor: pointer;
  }
  .switch.disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 14px;
  }
  .desc {
    font-size: 12px;
    line-height: 1.45;
    color: var(--color-text-muted);
  }
  input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }
  .track {
    flex: none;
    position: relative;
    width: 34px;
    height: 20px;
    margin-top: 1px;
    border-radius: 10px;
    border: 1px solid var(--color-neutral-600);
    transition: background-color 120ms, border-color 120ms;
  }
  .thumb {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--color-neutral-400);
    transition: transform 120ms, background-color 120ms;
  }
  .switch:not(.disabled):hover .track {
    border-color: var(--color-neutral-400);
  }
  input:checked + .track {
    border-color: var(--color-accent);
    background: var(--color-accent-800);
  }
  input:checked + .track .thumb {
    transform: translateX(14px);
    background: var(--color-accent-200);
  }
  input:focus-visible + .track {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .track,
    .thumb {
      transition: none;
    }
  }
</style>
