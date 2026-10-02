<script lang="ts">
  import { api, type TotpCode } from "$lib/api";
  import Icon from "./Icon.svelte";

  let { id, oncopy }: { id: string; oncopy: () => void } = $props();

  let code = $state<TotpCode | null>(null);
  let error = $state(false);

  $effect(() => {
    const current = id;
    let alive = true;
    const tick = async () => {
      try {
        const c = await api.totp(current);
        if (alive) {
          code = c;
          error = false;
        }
      } catch {
        if (alive) error = true;
      }
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  });

  let pretty = $derived(code ? code.code.replace(/^(\d{3,4})(\d{3,4})$/, "$1 $2") : "");
  const R = 9;
  const C = 2 * Math.PI * R;
</script>

{#if error}
  <span class="error"><Icon name="warning-circle" size={14} /> Code konnte nicht berechnet werden</span>
{:else if code}
  <span class="code mono grow">{pretty}</span>
  <span class="timer" class:low={code.remaining <= 5} title="Sekunden bis zum nächsten Code">
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
      <circle cx="12" cy="12" r={R} fill="none" stroke="var(--color-neutral-800)" stroke-width="2" />
      <circle
        cx="12"
        cy="12"
        r={R}
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-dasharray={C}
        stroke-dashoffset={C * (1 - code.remaining / code.period)}
        transform="rotate(-90 12 12)"
      />
    </svg>
    <span>{code.remaining} s</span>
  </span>
  <button class="btn btn-icon btn-sm" title="Einmalcode kopieren" aria-label="Einmalcode kopieren" onclick={oncopy}>
    <Icon name="copy" size={16} />
  </button>
{/if}

<style>
  .code {
    font-size: 22px;
    letter-spacing: 0.12em;
    color: var(--color-accent-300);
    font-variant-numeric: tabular-nums;
  }
  .timer {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-accent);
    font-size: 12px;
    min-width: 58px;
    font-variant-numeric: tabular-nums;
  }
  .timer.low {
    color: var(--color-danger);
  }
</style>
