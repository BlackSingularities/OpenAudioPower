<script lang="ts">
  // Live spectrum analyser of the master output (log frequency, peak hold, smoothing).
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { tp, app } from "../lib/store.svelte";

  let cv = $state<HTMLCanvasElement>();
  let box = $state<HTMLDivElement>();
  const BANDS = 120;
  let smooth = new Float32Array(BANDS).fill(-100);
  let peak = new Float32Array(BANDS).fill(-100);
  let peakAge = new Float32Array(BANDS);

  onMount(() => {
    let alive = true;
    let busy = false;
    const loop = async () => {
      if (!alive) return;
      if (!busy && cv && box) {
        busy = true;
        try {
          const s = tp.playing ? await api.spectrum(BANDS) : new Array(BANDS).fill(-110);
          draw(s);
        } catch {
          /* ignore */
        }
        busy = false;
      }
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
    return () => (alive = false);
  });

  function draw(s: number[]) {
    const w = box!.clientWidth;
    const h = box!.clientHeight;
    const dpr = window.devicePixelRatio || 1;
    if (cv!.width !== w * dpr || cv!.height !== h * dpr) {
      cv!.width = w * dpr;
      cv!.height = h * dpr;
    }
    const ctx = cv!.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const top = 6;
    const bottom = h - 18;
    const yOf = (db: number) => top + ((0 - Math.max(-90, Math.min(0, db))) / 90) * (bottom - top);
    ctx.font = "10px 'Cascadia Mono', Consolas, monospace";
    ctx.fillStyle = "rgba(255,255,255,0.35)";
    for (const db of [0, -18, -36, -54, -72]) {
      const y = Math.round(yOf(db)) + 0.5;
      ctx.fillStyle = "rgba(255,255,255,0.06)";
      ctx.fillRect(34, y, w - 34, 1);
      ctx.fillStyle = "rgba(255,255,255,0.4)";
      ctx.fillText(String(db), 4, y + 3);
    }
    const nyq = app.rate / 2;
    const xOfF = (f: number) => 34 + (Math.log(f / 20) / Math.log(nyq / 20)) * (w - 40);
    for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) {
      const x = Math.round(xOfF(f)) + 0.5;
      ctx.fillStyle = "rgba(255,255,255,0.06)";
      ctx.fillRect(x, top, 1, bottom - top);
      ctx.fillStyle = "rgba(255,255,255,0.4)";
      ctx.fillText(f >= 1000 ? f / 1000 + "k" : String(f), x - 8, h - 4);
    }
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#8b5cf6";
    const grad = ctx.createLinearGradient(0, bottom, 0, top);
    grad.addColorStop(0, accent + "22");
    grad.addColorStop(0.6, accent + "aa");
    grad.addColorStop(1, "#f0abfc");
    const bw = (w - 40) / BANDS;
    ctx.beginPath();
    ctx.moveTo(34, bottom);
    for (let i = 0; i < BANDS; i++) {
      const v = s[i] ?? -110;
      smooth[i] = v > smooth[i] ? v : smooth[i] * 0.86 + v * 0.14;
      if (smooth[i] >= peak[i]) {
        peak[i] = smooth[i];
        peakAge[i] = 0;
      } else if (++peakAge[i] > 40) peak[i] -= 0.8;
      const x = 34 + i * bw;
      ctx.lineTo(x, yOf(smooth[i]));
      ctx.lineTo(x + bw, yOf(smooth[i]));
    }
    ctx.lineTo(w - 6, bottom);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.fillStyle = "rgba(255,255,255,0.75)";
    for (let i = 0; i < BANDS; i++) ctx.fillRect(34 + i * bw, yOf(peak[i]), Math.max(1, bw - 1), 1.5);
  }
</script>

<div class="an" bind:this={box}>
  <canvas bind:this={cv}></canvas>
</div>

<style>
  .an {
    position: relative;
    height: 100%;
    padding: 6px 10px;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
