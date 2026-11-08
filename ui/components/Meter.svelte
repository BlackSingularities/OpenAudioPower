<script lang="ts">
  // Stereo peak meter with ballistics (instant rise, smooth fall) and peak hold.
  import { onMount } from "svelte";

  let { l = 0, r = 0, vertical = false, size = 6, length = 120, clip = false }: { l?: number; r?: number; vertical?: boolean; size?: number; length?: number; clip?: boolean } = $props();

  let shown = $state([0, 0]);
  let hold = $state([0, 0]);
  let holdAt = [0, 0];
  const db = (g: number) => (g <= 1e-5 ? -100 : 20 * Math.log10(g));
  // −60..+3 dB mapped to 0..1
  const pos = (g: number) => Math.min(1, Math.max(0, (db(g) + 60) / 63));

  onMount(() => {
    let raf = 0;
    let last = performance.now();
    const loop = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const target = [pos(l), pos(r)];
      for (let i = 0; i < 2; i++) {
        if (target[i] > shown[i]) shown[i] = target[i];
        else shown[i] = Math.max(target[i], shown[i] - dt * 0.55);
        if (shown[i] >= hold[i] || now - holdAt[i] > 1400) {
          hold[i] = shown[i];
          holdAt[i] = now;
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div class="meter" class:vertical style={vertical ? `height:${length}px;width:${size * 2 + 3}px` : `width:${length}px;height:${size * 2 + 3}px`}>
  {#each [0, 1] as i}
    <div class="bar" style={vertical ? `width:${size}px` : `height:${size}px`}>
      <div class="fill" style={vertical ? `clip-path: inset(${(1 - shown[i]) * 100}% 0 0 0)` : `clip-path: inset(0 ${(1 - shown[i]) * 100}% 0 0)`}></div>
      <div class="hold" style={vertical ? `bottom:${hold[i] * 100}%` : `left:${hold[i] * 100}%`}></div>
    </div>
  {/each}
  {#if clip}<div class="clip"></div>{/if}
</div>

<style>
  .meter {
    display: flex;
    flex-direction: column;
    gap: 3px;
    position: relative;
    flex: none;
  }
  .meter.vertical {
    flex-direction: row;
  }
  .bar {
    position: relative;
    flex: 1;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }
  .fill {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, #10b981 0%, #34d399 60%, #facc15 85%, #f97316 93%, #ef4444 100%);
  }
  .vertical .fill {
    background: linear-gradient(0deg, #10b981 0%, #34d399 60%, #facc15 85%, #f97316 93%, #ef4444 100%);
  }
  .hold {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: rgba(255, 255, 255, 0.8);
    transform: translateX(-2px);
  }
  .vertical .hold {
    top: auto;
    left: 0;
    right: 0;
    width: auto;
    height: 2px;
    transform: translateY(2px);
  }
  .clip {
    position: absolute;
    right: -5px;
    top: 50%;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--err);
    transform: translateY(-50%);
    box-shadow: 0 0 6px var(--err);
  }
  .vertical .clip {
    right: auto;
    left: 50%;
    top: -6px;
    transform: translateX(-50%);
  }
</style>
