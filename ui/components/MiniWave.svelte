<script lang="ts">
  // Small waveform preview of a source range (library thumbnails, inspector header) –
  // drawn with the same scale / style / colour settings as the timeline.
  import { S, getPeaks, getBands, peakVersion } from "../lib/store.svelte";
  import { drawWave } from "../lib/wave";

  let {
    id,
    offset = 0,
    len = 0,
    color = "#a78bfa",
    height = 28,
    fadeIn = 0,
    fadeOut = 0,
    gain = 0,
    playhead = -1,
  }: { id: number; offset?: number; len?: number; color?: string; height?: number; fadeIn?: number; fadeOut?: number; gain?: number; playhead?: number } = $props();

  let cv = $state<HTMLCanvasElement>();
  let w = $state(0);

  $effect(() => {
    const ws = S.settings.wave;
    void [peakVersion.v, w, offset, len, color, fadeIn, fadeOut, gain, playhead, ws.scale, ws.style, ws.color];
    const pk = getPeaks(id);
    const bands = ws.color === "spectral" || ws.style === "rms" ? getBands(id) : undefined;
    if (!cv || !w) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(height * dpr);
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, height);
    if (!pk) return;
    const total = pk.bins * pk.spp;
    const n = len > 0 ? len : total - offset;
    const g = Math.pow(10, gain / 20);
    drawWave({
      ctx,
      id,
      peaks: pk,
      bands,
      x0: 0,
      x1: w,
      y: height / 2,
      h: height / 2 - 1,
      frameAt: (x) => offset + (x / w) * n,
      fpp: Math.max(pk.spp, n / w),
      env: (x) => {
        const t = (x / w) * n;
        let e = g;
        if (fadeIn > 0 && t < fadeIn) e *= Math.sin(((t / fadeIn) * Math.PI) / 2);
        if (fadeOut > 0 && n - t < fadeOut) e *= Math.sin((((n - t) / fadeOut) * Math.PI) / 2);
        return e;
      },
      color,
      accent: "#f0abfc",
      dpr,
      scale: ws.scale,
      style: ws.style === "bars" && height < 20 ? "fill" : ws.style,
      colorMode: ws.color,
    });
    if (playhead >= 0 && playhead <= 1) {
      ctx.fillStyle = "#fff";
      ctx.fillRect(playhead * w, 0, 1.5, height);
    }
  });
</script>

<div class="mw" bind:clientWidth={w} style="height:{height}px">
  <canvas bind:this={cv} style="width:{w}px;height:{height}px"></canvas>
</div>

<style>
  .mw {
    width: 100%;
    min-width: 0;
  }
  canvas {
    display: block;
  }
</style>
