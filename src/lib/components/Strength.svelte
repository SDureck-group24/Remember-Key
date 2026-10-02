<!-- Stärkeanzeige. Mit `password` bewertet das Backend per zxcvbn (Wörterbücher, Muster);
     mit `bits` wird eine bekannte Zufallsentropie angezeigt (Generator). -->
<script lang="ts">
  import { api, SCORE_LABELS, scoreFromBits, type Strength } from "$lib/api";

  let {
    password = undefined,
    bits = undefined,
    score = $bindable(0),
  }: { password?: string; bits?: number; score?: number } = $props();

  let result = $state<Strength | null>(null);

  $effect(() => {
    if (bits !== undefined) {
      result = { score: scoreFromBits(bits), bits, guessesLog10: bits / Math.log2(10) };
      return;
    }
    const pw = password ?? "";
    if (!pw) {
      result = null;
      return;
    }
    let alive = true;
    const t = setTimeout(async () => {
      try {
        const r = await api.passwordStrength(pw);
        if (alive) result = r;
      } catch {
        if (alive) result = null;
      }
    }, 150);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  });

  $effect(() => {
    score = result?.score ?? 0;
  });

  const colors = [
    "var(--color-danger)",
    "var(--color-danger)",
    "var(--color-accent-600)",
    "var(--color-accent)",
    "var(--color-accent-400)",
  ];
</script>

{#if result}
  <div class="strength">
    <div class="track">
      <div class="bar" style="width: {(result.score + 1) * 20}%; background: {colors[result.score]}"></div>
    </div>
    <span class="label">{SCORE_LABELS[result.score]} · ≈ {Math.round(result.bits)} Bit</span>
  </div>
{/if}

<style>
  .strength {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .track {
    flex: 1;
    height: 2px;
    background: var(--color-neutral-800);
    border-radius: 1px;
    overflow: hidden;
  }
  .bar {
    height: 100%;
    transition: width 0.2s;
  }
  .label {
    font-size: 11px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
  }
</style>
