<script lang="ts">
  // Spectral editor: the clip's spectrogram on a log-frequency axis. Drag a box around a cough,
  // a phone ring or a squeak, then fill it with the surrounding background or turn it down.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtTime } from "../lib/format";
  import { app, findClip, source, applyOpToClip, seek, play, stop, tp } from "../lib/store.svelte";

  const FMIN = 30;
  const H = 380;

  let cv = $state<HTMLCanvasElement>();
  let wrap = $state<HTMLDivElement>();
  let W = $state(900);
  /** visible source range */
  let view = $state({ a: 0, b: 1 });
  /** selection in source frames and Hz */
  let sel = $state<{ a: number; b: number; f0: number; f1: number } | null>(null);
  let img: ImageData | null = null;
  let loading = $state(false);
  let gain = $state(-30);
  let busy = $state(false);
  let reqId = 0;

  const clip = $derived(app.spectralClip != null ? findClip(app.spectralClip)?.clip : undefined);
  const src = $derived(clip ? source(clip.source) : undefined);
  const nyq = $derived((src?.rate ?? 48000) / 2);

  // ── axes ──
  const xToF = (x: number) => view.a + (x / W) * (view.b - view.a);
  const fToX = (f: number) => ((f - view.a) / (view.b - view.a)) * W;
  const yToHz = (y: number) => FMIN * Math.pow(nyq / FMIN, 1 - Math.min(H, Math.max(0, y)) / H);
  const hzToY = (hz: number) => (1 - Math.log(Math.max(FMIN, hz) / FMIN) / Math.log(nyq / FMIN)) * H;

  // magma-like palette in the app's colours: black → violet → magenta → orange → pale yellow
  const LUT = (() => {
    const stops: [number, [number, number, number]][] = [
      [0, [6, 4, 14]],
      [0.3, [52, 18, 110]],
      [0.55, [160, 40, 160]],
      [0.75, [240, 100, 90]],
      [0.9, [252, 190, 80]],
      [1, [255, 250, 210]],
    ];
    const lut = new Uint8ClampedArray(256 * 3);
    for (let i = 0; i < 256; i++) {
      const v = i / 255;
      let j = 0;
      while (j < stops.length - 2 && v > stops[j + 1][0]) j++;
      const [p0, c0] = stops[j];
      const [p1, c1] = stops[j + 1];
      const k = (v - p0) / (p1 - p0);
      for (let c = 0; c < 3; c++) lut[i * 3 + c] = c0[c] + (c1[c] - c0[c]) * k;
    }
    return lut;
  })();

  async function load() {
    if (!clip || !src || W < 10) return;
    const id = ++reqId;
    loading = true;
    try {
      const bytes = await api.spectrogram(src.id, Math.floor(view.a), Math.ceil(view.b), Math.round(W), H);
      if (id !== reqId) return;
      const data = new ImageData(Math.round(W), H);
      for (let i = 0; i < bytes.length; i++) {
        const v = bytes[i];
        data.data[i * 4] = LUT[v * 3];
        data.data[i * 4 + 1] = LUT[v * 3 + 1];
        data.data[i * 4 + 2] = LUT[v * 3 + 2];
        data.data[i * 4 + 3] = 255;
      }
      img = data;
      draw();
    } catch {
      /* the dialog may have closed meanwhile */
    } finally {
      if (id === reqId) loading = false;
    }
  }

  let timer = 0;
  function reload(delay = 120) {
    clearTimeout(timer);
    timer = window.setTimeout(load, delay);
    draw();
  }

  function draw() {
    if (!cv) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(W * dpr);
    cv.height = Math.round(H * dpr);
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, cv.width, cv.height);
    if (img) {
      const tmp = document.createElement("canvas");
      tmp.width = img.width;
      tmp.height = img.height;
      tmp.getContext("2d")!.putImageData(img, 0, 0);
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(tmp, 0, 0, cv.width, cv.height);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // frequency grid
    ctx.font = "10px Inter, system-ui, sans-serif";
    for (const hz of [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
      if (hz >= nyq) continue;
      const y = Math.round(hzToY(hz)) + 0.5;
      ctx.fillStyle = "rgba(255,255,255,0.12)";
      ctx.fillRect(0, y, W, 1);
      ctx.fillStyle = "rgba(255,255,255,0.6)";
      ctx.fillText(hz >= 1000 ? `${hz / 1000}k` : String(hz), 4, y - 3);
    }
    // playhead (timeline position mapped into the clip)
    if (clip) {
      const f = tp.pos - clip.start + clip.offset;
      if (f >= view.a && f <= view.b) {
        ctx.fillStyle = "rgba(255,255,255,0.85)";
        ctx.fillRect(Math.round(fToX(f)), 0, 1, H);
      }
    }
    if (sel) {
      const x0 = fToX(sel.a);
      const x1 = fToX(sel.b);
      const y0 = hzToY(sel.f1);
      const y1 = hzToY(sel.f0);
      ctx.fillStyle = "rgba(255,255,255,0.12)";
      ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
      ctx.strokeStyle = "#fff";
      ctx.setLineDash([5, 4]);
      ctx.strokeRect(Math.round(x0) + 0.5, Math.round(y0) + 0.5, Math.round(x1 - x0), Math.round(y1 - y0));
      ctx.setLineDash([]);
    }
  }

  // redraw the playhead while playing
  $effect(() => {
    void tp.pos;
    void tp.playing;
    draw();
  });

  // ── open / close ──
  $effect(() => {
    if (!app.dialogs.spectral) return;
    const c = app.spectralClip != null ? findClip(app.spectralClip)?.clip : undefined;
    if (!c) return;
    view = { a: c.offset, b: c.offset + c.len };
    sel = null;
    img = null;
    queueMicrotask(() => {
      if (wrap) W = wrap.clientWidth;
      load();
    });
  });

  onMount(() => {
    const ro = new ResizeObserver(() => {
      if (!wrap) return;
      const w = wrap.clientWidth;
      if (Math.abs(w - W) > 2) {
        W = w;
        reload();
      }
    });
    if (wrap) ro.observe(wrap);
    return () => ro.disconnect();
  });

  // ── mouse ──
  function pos(e: MouseEvent) {
    const r = cv!.getBoundingClientRect();
    return { x: Math.min(W, Math.max(0, e.clientX - r.left)), y: Math.min(H, Math.max(0, e.clientY - r.top)) };
  }
  function down(e: PointerEvent) {
    if (!clip || e.button !== 0) return;
    const p0 = pos(e);
    cv!.setPointerCapture(e.pointerId);
    const mv = (ev: PointerEvent) => {
      const p = pos(ev);
      const [x0, x1] = [Math.min(p0.x, p.x), Math.max(p0.x, p.x)];
      const [y0, y1] = [Math.min(p0.y, p.y), Math.max(p0.y, p.y)];
      sel = { a: Math.round(xToF(x0)), b: Math.round(xToF(x1)), f0: yToHz(y1), f1: yToHz(y0) };
      draw();
    };
    const up = () => {
      cv!.removeEventListener("pointermove", mv);
      cv!.removeEventListener("pointerup", up);
      if (sel && (fToX(sel.b) - fToX(sel.a) < 3 || hzToY(sel.f0) - hzToY(sel.f1) < 3)) sel = null;
      draw();
    };
    cv!.addEventListener("pointermove", mv);
    cv!.addEventListener("pointerup", up);
  }
  function wheel(e: WheelEvent) {
    if (!clip) return;
    e.preventDefault();
    const span = view.b - view.a;
    const lo = clip.offset;
    const hi = clip.offset + clip.len;
    if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      const d = ((e.deltaX || e.deltaY) / W) * span;
      const a = Math.min(hi - span, Math.max(lo, view.a + d));
      view = { a, b: a + span };
    } else {
      const f = xToF(pos(e).x);
      const k = e.deltaY > 0 ? 1.25 : 0.8;
      const ns = Math.min(hi - lo, Math.max((src?.rate ?? 48000) * 0.05, span * k));
      let a = f - (f - view.a) * (ns / span);
      a = Math.min(hi - ns, Math.max(lo, a));
      view = { a, b: a + ns };
    }
    reload();
  }

  // ── actions ──
  function fullBand() {
    if (sel) sel = { ...sel, f0: FMIN, f1: nyq };
    draw();
  }
  function zoomToSel() {
    if (!sel || !clip) return;
    const pad = (sel.b - sel.a) * 0.6 + (src?.rate ?? 48000) * 0.05;
    view = { a: Math.max(clip.offset, sel.a - pad), b: Math.min(clip.offset + clip.len, sel.b + pad) };
    reload(0);
  }
  function showAll() {
    if (!clip) return;
    view = { a: clip.offset, b: clip.offset + clip.len };
    reload(0);
  }
  async function apply(mode: "fill" | "attenuate") {
    if (!sel || !clip) return;
    busy = true;
    const res = await applyOpToClip(clip.id, sel.a, sel.b, { op: "spectralRepair", fLo: sel.f0, fHi: sel.f1, mode, gain });
    busy = false;
    if (res) reload(0);
  }
  let stopAt = 0;
  function listen() {
    if (!sel || !clip) return;
    const at = clip.start + (sel.a - clip.offset);
    if (tp.playing) stop();
    seek(Math.max(0, at - Math.round((src?.rate ?? 48000) * 0.5)));
    play();
    clearTimeout(stopAt);
    stopAt = window.setTimeout(stop, ((sel.b - sel.a) / (src?.rate ?? 48000) + 1) * 1000);
  }

  function close() {
    clearTimeout(stopAt);
    app.dialogs.spectral = false;
  }

  const fmtHz = (hz: number) => (hz >= 1000 ? `${(hz / 1000).toFixed(hz >= 10000 ? 1 : 2)} kHz` : `${Math.round(hz)} Hz`);
</script>

{#if app.dialogs.spectral}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && close()}>
    <div class="dialog glass-strong spectral" role="dialog" aria-modal="true">
      <div class="top">
        <div>
          <h2>{t("spectralTitle")}</h2>
          <p class="sub">{t("spectralSub")}</p>
        </div>
        <div class="grow"></div>
        <button class="btn sm ghost" onclick={showAll}><Icon name="fit" size={14} />{t("spectralAll")}</button>
        <button class="btn sm ghost" disabled={!sel} onclick={zoomToSel}><Icon name="plus" size={14} />{t("spectralZoomSel")}</button>
      </div>

      <div class="view" bind:this={wrap}>
        <canvas bind:this={cv} style="width:{W}px;height:{H}px" onpointerdown={down} onwheel={wheel}></canvas>
        {#if loading}<div class="loading mono">…</div>{/if}
        {#if !clip}<div class="empty">{t("noSelection")}</div>{/if}
      </div>
      <div class="ruler mono">
        <span>{src ? fmtTime(view.a, src.rate) : ""}</span>
        <span class="hint">{t("spectralHint")}</span>
        <span>{src ? fmtTime(view.b, src.rate) : ""}</span>
      </div>

      <div class="bar">
        <div class="info mono">
          {#if sel && src}
            {fmtTime(sel.a, src.rate)} – {fmtTime(sel.b, src.rate)} · {fmtHz(sel.f0)} – {fmtHz(sel.f1)}
          {:else}
            {t("spectralNoSel")}
          {/if}
        </div>
        <button class="btn sm" disabled={!sel} onclick={fullBand}>{t("spectralFullBand")}</button>
        <button class="btn sm" disabled={!sel} onclick={listen}><Icon name="play" size={13} />{t("spectralListen")}</button>
        <div class="grow"></div>
        <div class="gain"><Slider label={t("spectralGain")} value={gain} min={-60} max={-3} step={1} unit="dB" onchange={(v: number) => (gain = v)} /></div>
        <button class="btn" disabled={!sel || busy} onclick={() => apply("attenuate")}>{t("spectralAttenuate")}</button>
        <button class="btn primary" disabled={!sel || busy} onclick={() => apply("fill")}><Icon name="wand" size={15} />{t("spectralFill")}</button>
      </div>

      <div class="actions">
        <button class="btn ghost" disabled={busy} onclick={close}>{t("close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .spectral {
    width: min(1180px, 100%);
  }
  .top {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .view {
    position: relative;
    margin-top: 12px;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--line);
    background: #06040e;
    height: 380px;
  }
  canvas {
    display: block;
    cursor: crosshair;
    touch-action: none;
  }
  .loading {
    position: absolute;
    top: 8px;
    right: 10px;
    color: var(--text-2);
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .ruler {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-top: 5px;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .hint {
    text-align: center;
    font-family: var(--font);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    margin-top: 12px;
  }
  .info {
    font-size: 11.5px;
    color: var(--text-2);
    min-width: 250px;
  }
  .gain {
    width: 200px;
  }
</style>
