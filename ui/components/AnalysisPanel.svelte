<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api, type Analysis } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtDb, fmtDuration } from "../lib/format";
  import { app, ed, findClip, source, toast, commit, P, setMetronome } from "../lib/store.svelte";

  let res = $state<Analysis | null>(null);
  let busy = $state(false);
  let cv = $state<HTMLCanvasElement>();
  let forKey = $state("");

  const sel = $derived(ed.selClips.length ? findClip(ed.selClips[0]) : undefined);
  const range = $derived.by(() => {
    if (!sel) return null;
    const c = sel.clip;
    let a = c.offset;
    let b = c.offset + c.len;
    if (ed.sel) {
      const aa = c.offset + Math.max(0, ed.sel.start - c.start);
      const bb = c.offset + Math.min(c.len, ed.sel.end - c.start);
      if (bb > aa) {
        a = aa;
        b = bb;
      }
    }
    return { id: c.source, a, b };
  });
  const key = $derived(range ? `${range.id}:${range.a}:${range.b}` : "");

  async function run() {
    if (!range) return;
    busy = true;
    try {
      res = await api.analyze(range.id, range.a, range.b);
      forKey = key;
      drawSpec();
    } catch (e) {
      toast(String(e), "error");
    } finally {
      busy = false;
    }
  }

  async function drawSpec() {
    if (!range || !cv) return;
    const w = 300;
    const h = 150;
    const img = await api.spectrogram(range.id, range.a, range.b, w, h);
    const dpr = 1;
    cv.width = w * dpr;
    cv.height = h * dpr;
    const ctx = cv.getContext("2d")!;
    const id = ctx.createImageData(w, h);
    for (let i = 0; i < w * h; i++) {
      const v = img[i] / 255;
      // magma-like ramp
      const r = Math.min(255, 255 * Math.pow(v, 0.8) * 1.15);
      const g = Math.min(255, 255 * Math.pow(v, 2.2));
      const b = Math.min(255, 255 * (0.35 * Math.sin(v * Math.PI) + Math.pow(v, 4) * 0.8));
      id.data[i * 4] = r;
      id.data[i * 4 + 1] = g;
      id.data[i * 4 + 2] = b + 40 * v;
      id.data[i * 4 + 3] = 255;
    }
    ctx.putImageData(id, 0, 0);
  }

  function useTempo() {
    if (!res?.bpm) return;
    commit(() => (P.p.bpm = Math.round(res!.bpm * 10) / 10));
    setMetronome();
  }

  const lufsColor = (l: number) => (l > -9 ? "#f87171" : l > -16 ? "#34d399" : "#fbbf24");
</script>

<div class="ap">
  <p class="hint">{t("analysisHint")}</p>
  {#if !sel}
    <div class="empty">{t("noSelection")}</div>
  {:else}
    <button class="btn primary wide" onclick={run} disabled={busy}><Icon name="chart" size={15} />{busy ? "…" : t("runAnalysis")}{ed.sel ? ` · ${t("selection")}` : ""}</button>
    {#if res}
      {#if forKey !== key}<div class="stale dim">↻ {t("runAnalysis")}</div>{/if}
      <div class="big">
        <div class="tile">
          <div class="v mono" style="color:{lufsColor(res.lufs)}">{isFinite(res.lufs) ? res.lufs.toFixed(1) : "−∞"}</div>
          <div class="k">LUFS · {t("lufs")}</div>
        </div>
        <div class="tile">
          <div class="v mono">{fmtDb(res.truePeakDb)}</div>
          <div class="k">dBTP · {t("truePeak")}</div>
        </div>
      </div>
      <div class="big">
        <div class="tile">
          <div class="v mono">{res.bpm ? res.bpm.toFixed(1) : "—"}</div>
          <div class="k">BPM · {t("tempo")} <span class="dim">{res.bpm ? Math.round(res.bpmConfidence * 100) + "%" : ""}</span></div>
          {#if res.bpm}<button class="btn sm ghost use" onclick={useTempo}>→ {t("bpm")}</button>{/if}
        </div>
        <div class="tile">
          <div class="v">{res.key || "—"}</div>
          <div class="k">{t("key")} <span class="dim">{res.key ? Math.round(res.keyConfidence * 100) + "%" : ""}</span></div>
        </div>
      </div>
      <div class="kv">
        <span>{t("duration")}</span><b class="mono">{fmtDuration(res.duration)}</b>
        <span>{t("lufsShort")}</span><b class="mono">{isFinite(res.lufsShortMax) ? res.lufsShortMax.toFixed(1) + " LUFS" : "—"}</b>
        <span>{t("lra")}</span><b class="mono">{res.lra.toFixed(1)} LU</b>
        <span>{t("samplePeak")}</span><b class="mono">{fmtDb(res.peakDb)} dBFS</b>
        <span>{t("rms")}</span><b class="mono">{fmtDb(res.rmsDb)} dBFS</b>
        <span>{t("crest")}</span><b class="mono">{res.crestDb.toFixed(1)} dB</b>
        <span>{t("correlation")}</span><b class="mono" style="color:{res.correlation < 0 ? 'var(--err)' : ''}">{res.correlation.toFixed(2)}</b>
        <span>{t("dc")}</span><b class="mono">{(res.dcOffset * 100).toFixed(3)} %</b>
        <span>{t("clipped")}</span><b class="mono" style="color:{res.clipped ? 'var(--err)' : ''}">{res.clipped}</b>
        <span>{t("silence")}</span><b class="mono">{Math.round(res.silenceRatio * 100)} %</b>
      </div>
    {/if}
    <div class="label">{t("spectrogram")}</div>
    <canvas bind:this={cv} class="spec" width="300" height="150"></canvas>
  {/if}
</div>

<style>
  .ap {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .hint {
    margin: 0;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.45;
  }
  .big {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .tile {
    position: relative;
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--line);
  }
  .tile .v {
    font-size: 20px;
    font-weight: 600;
  }
  .tile .k {
    font-size: 10.5px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .use {
    position: absolute;
    right: 4px;
    top: 4px;
    height: 20px;
    font-size: 10.5px;
  }
  .kv {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 5px 10px;
    font-size: 12px;
  }
  .kv span {
    color: var(--text-3);
  }
  .kv b {
    font-weight: 500;
  }
  .spec {
    width: 100%;
    height: 150px;
    border-radius: 10px;
    background: #000;
    border: 1px solid var(--line);
    image-rendering: auto;
  }
  .stale {
    font-size: 11px;
  }
</style>
