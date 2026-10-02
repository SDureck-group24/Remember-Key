<script lang="ts">
  import { api, defaultGenOptions, errorText, type GenOptions, type Generated } from "$lib/api";
  import Icon from "./Icon.svelte";
  import Strength from "./Strength.svelte";

  let {
    onclose,
    onuse,
    onnotify,
  }: {
    onclose: () => void;
    onuse?: (password: string) => void;
    onnotify?: (msg: string) => void;
  } = $props();

  let opts = $state<GenOptions>({ ...defaultGenOptions });
  let result = $state<Generated | null>(null);
  let error = $state("");

  async function regenerate() {
    try {
      result = await api.generate($state.snapshot(opts));
      error = "";
    } catch (e) {
      result = null;
      error = errorText(e);
    }
  }

  $effect(() => {
    // Alle Optionen lesen, damit jede Änderung neu generiert.
    JSON.stringify(opts);
    regenerate();
  });

  async function copy() {
    if (!result) return;
    try {
      const secs = await api.copyText(result.password);
      onnotify?.(`Passwort kopiert. Wird in ${secs} s aus der Zwischenablage gelöscht.`);
    } catch (e) {
      error = errorText(e);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  const sets: { key: keyof GenOptions; label: string; title: string }[] = [
    { key: "lowercase", label: "a–z", title: "Kleinbuchstaben" },
    { key: "uppercase", label: "A–Z", title: "Großbuchstaben" },
    { key: "digits", label: "0–9", title: "Ziffern" },
    { key: "symbols", label: "!@#", title: "Sonderzeichen" },
  ];
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="dialog-backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="gen-title">
    <div class="row">
      <div class="dialog-title grow" id="gen-title">Passwort-Generator</div>
      <button class="btn btn-icon btn-sm" aria-label="Schließen" onclick={onclose}><Icon name="x" size={16} /></button>
    </div>

    <div class="output">
      <span class="mono grow pw">{result?.password ?? ""}</span>
      <button class="btn btn-icon btn-sm" title="Neu erzeugen" aria-label="Neu erzeugen" onclick={regenerate}>
        <Icon name="arrows-clockwise" size={16} />
      </button>
    </div>
    {#if result}<Strength bits={result.entropyBits} />{/if}

    <div class="field">
      <label for="len">Länge <span class="len">{opts.length}</span></label>
      <input id="len" type="range" min="8" max="64" bind:value={opts.length} />
    </div>

    <div class="field">
      <span class="lbl">Zeichen</span>
      <div class="seg">
        {#each sets as s (s.key)}
          <label class="seg-opt" title={s.title}>
            <input type="checkbox" bind:checked={opts[s.key] as boolean} />
            {s.label}
          </label>
        {/each}
      </div>
    </div>

    <div class="field">
      <div class="seg">
        <label class="seg-opt">
          <input type="checkbox" bind:checked={opts.excludeAmbiguous} />
          Verwechselbare Zeichen vermeiden (I l 1 O 0)
        </label>
      </div>
    </div>

    {#if error}<p class="error"><Icon name="warning-circle" size={16} /> {error}</p>{/if}

    <div class="dialog-actions">
      <button class="btn btn-secondary" onclick={copy} disabled={!result}>
        <Icon name="copy" size={15} /> Kopieren
      </button>
      {#if onuse}
        <button class="btn btn-primary" disabled={!result} onclick={() => result && onuse(result.password)}>
          <Icon name="check" size={15} /> Übernehmen
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .dialog {
    width: min(480px, 100%);
  }
  .output {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    background: var(--color-bg);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sm);
    min-height: 48px;
  }
  .pw {
    font-size: 16px;
    color: var(--color-accent-300);
    overflow-wrap: anywhere;
    user-select: all;
  }
  .len {
    color: var(--color-text);
    font-variant-numeric: tabular-nums;
    margin-left: var(--space-1);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--color-accent);
  }
  .lbl {
    display: block;
    font-size: 12px;
    margin-bottom: 5px;
    color: color-mix(in srgb, var(--color-text) 70%, transparent);
  }
  .field + .field {
    margin-top: var(--space-4);
  }
</style>
