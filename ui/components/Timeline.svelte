<script lang="ts">
  // Multitrack timeline: ruler (seek, loop range, markers), track headers and a canvas with
  // clips, waveforms, fades, selection and grid. All mouse editing happens here.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import TrackHeader from "./TrackHeader.svelte";
  import { t } from "../lib/i18n.svelte";
  import { openMenu } from "../lib/menu.svelte";
  import { drawWave, setRawListener } from "../lib/wave";
  import { fmtTime } from "../lib/format";
  import {
    P, S, app, ed, tp, peakVersion, getPeaks, getBands, source, findClip, track as getTrack, commit, beginGesture, endGesture, syncEngine,
    seek, snapFrame, framesPerBeat, zoomAt, projectLength, splitAt, deleteSelected, duplicateSelected, copySelected, paste,
    addTrackCmd, addSourceToTrack, addSourceToNewTrack, openStemDialog, applyOp, setDraggingPlayhead, addMarker, deleteRange,
    cropToRange, bounceRange, toggleLoop, type Clip, type Track,
    autoOf, envelopeAt, AUTO_RANGE, type AutoLane, type AutoPoint,
    openSpectral,
    openRemix,
  } from "../lib/store.svelte";

  const HEADER_W = 214;
  const RULER_H = 34;

  let laneCv = $state<HTMLCanvasElement>();
  let rulerCv = $state<HTMLCanvasElement>();
  let lanesEl = $state<HTMLDivElement>();
  let W = $state(800);
  let H = $state(400);
  let dpr = window.devicePixelRatio || 1;
  let hover = $state<{ clip: number; zone: string } | null>(null);
  let dropHint = $state<{ track: number | null; frame: number; y: number } | null>(null);
  /** automation point under the mouse or being dragged */
  let autoHot = $state<{ track: number; i: number } | null>(null);

  const totalH = $derived(P.p.tracks.reduce((a, x) => a + x.height, 0));
  const xOf = (f: number) => (f - ed.scroll) / ed.fpp;
  const fOf = (x: number) => ed.scroll + x * ed.fpp;

  $effect(() => {
    ed.viewWidth = W;
  });

  // ─────────────── drawing ───────────────

  let raf = 0;
  function schedule() {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      drawLanes();
      drawRuler();
    });
  }

  $effect(() => {
    // dependencies
    JSON.stringify(P.p.tracks);
    JSON.stringify(P.p.loop);
    JSON.stringify(P.p.markers);
    void [ed.fpp, ed.scroll, ed.vscroll, ed.sel?.start, ed.sel?.end, ed.selClips.length, ed.selClips[0], ed.selTrack, peakVersion.v, W, H, hover?.clip, hover?.zone, P.p.bpm, P.p.beats, S.settings.timeMode, S.settings.wave.scale, S.settings.wave.style, S.settings.wave.color, tp.recording, tp.recording ? tp.recFrames : 0, app.sources.length];
    schedule();
  });

  onMount(() => {
    setRawListener(schedule);
    const ro = new ResizeObserver(() => {
      if (!lanesEl) return;
      W = lanesEl.clientWidth;
      H = lanesEl.clientHeight;
      dpr = window.devicePixelRatio || 1;
      schedule();
    });
    if (lanesEl) ro.observe(lanesEl);
    return () => ro.disconnect();
  });

  function gridStep(): { step: number; major: number; label: (f: number) => string } {
    const rate = app.rate;
    if (S.settings.timeMode === "bars") {
      const fpb = framesPerBeat();
      const bar = fpb * P.p.beats;
      const candidates = [fpb / 4, fpb / 2, fpb, bar, bar * 2, bar * 4, bar * 8, bar * 16, bar * 32];
      const step = candidates.find((c) => c / ed.fpp >= 14) ?? bar * 64;
      let major = bar;
      while (major / ed.fpp < 70) major *= 2;
      return { step, major, label: (f) => String(Math.round(f / bar) + 1) };
    }
    const nice = [0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800];
    const majS = nice.find((s) => (s * rate) / ed.fpp >= 90) ?? 3600;
    const minS = nice.find((s) => (s * rate) / ed.fpp >= 12) ?? majS;
    return {
      step: minS * rate,
      major: majS * rate,
      label: (f) => {
        const s = f / rate;
        if (majS < 1) return s.toFixed(majS < 0.01 ? 3 : majS < 0.1 ? 2 : 1) + "s";
        const m = Math.floor(s / 60);
        return `${m}:${String(Math.round(s % 60)).padStart(2, "0")}`;
      },
    };
  }

  function setup(cv: HTMLCanvasElement, w: number, h: number) {
    const pw = Math.round(w * dpr);
    const ph = Math.round(h * dpr);
    if (cv.width !== pw || cv.height !== ph) {
      cv.width = pw;
      cv.height = ph;
    }
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    return ctx;
  }

  function rr(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2));
  }

  function alpha(hex: string, a: number) {
    const n = parseInt(hex.replace("#", ""), 16);
    return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
  }

  function fadeGain(t: number) {
    const x = Math.min(1, Math.max(0, t));
    return Math.sin((x * Math.PI) / 2);
  }

  function drawLanes() {
    if (!laneCv) return;
    const ctx = setup(laneCv, W, H);
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#8b5cf6";
    const g = gridStep();
    // grid
    const f0 = Math.floor(fOf(0) / g.step) * g.step;
    for (let f = f0; f < fOf(W); f += g.step) {
      const x = Math.round(xOf(f)) + 0.5;
      const major = Math.abs(f / g.major - Math.round(f / g.major)) < 1e-6;
      ctx.fillStyle = major ? "rgba(255,255,255,0.085)" : "rgba(255,255,255,0.035)";
      ctx.fillRect(x, 0, 1, H);
    }
    // loop range
    const lp = P.p.loop;
    if (lp.end > lp.start) {
      const a = xOf(lp.start);
      const b = xOf(lp.end);
      ctx.fillStyle = lp.on ? alpha(accent, 0.07) : "rgba(255,255,255,0.025)";
      ctx.fillRect(a, 0, b - a, H);
    }
    let y = -ed.vscroll;
    for (const tr of P.p.tracks) {
      const h = tr.height;
      if (y + h > 0 && y < H) {
        if (tr.id === ed.selTrack) {
          ctx.fillStyle = alpha(accent, 0.045);
          ctx.fillRect(0, y, W, h);
        }
        ctx.fillStyle = "rgba(255,255,255,0.06)";
        ctx.fillRect(0, y + h - 1, W, 1);
        const anySolo = P.p.tracks.some((x) => x.solo);
        const dim = tr.mute || (anySolo && !tr.solo);
        for (const c of tr.clips) drawClip(ctx, tr, c, y, h, dim);
        if (tp.recording && tr.armed) drawRecording(ctx, y, h);
        if (tr.auto && tr.auto.show !== "off") drawAuto(ctx, tr, tr.auto.show, y, h);
      }
      y += h;
    }
    // selection range
    if (ed.sel && ed.sel.end > ed.sel.start) {
      const a = xOf(ed.sel.start);
      const b = xOf(ed.sel.end);
      let ty = 0;
      let th = Math.min(H, totalH - ed.vscroll);
      if (ed.selTrack) {
        let yy = -ed.vscroll;
        for (const tr of P.p.tracks) {
          if (tr.id === ed.selTrack) {
            ty = yy;
            th = tr.height;
          }
          yy += tr.height;
        }
      }
      ctx.fillStyle = alpha(accent, 0.2);
      ctx.fillRect(a, ty, b - a, th);
      ctx.fillStyle = alpha(accent, 0.9);
      ctx.fillRect(Math.round(a), ty, 1, th);
      ctx.fillRect(Math.round(b), ty, 1, th);
    }
    // markers
    for (const m of P.p.markers) {
      const x = Math.round(xOf(m.pos)) + 0.5;
      if (x < -2 || x > W + 2) continue;
      ctx.strokeStyle = "rgba(251,191,36,0.55)";
      ctx.setLineDash([3, 4]);
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, H);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    // drop hint
    if (dropHint) {
      ctx.fillStyle = alpha(accent, 0.9);
      ctx.fillRect(xOf(dropHint.frame), 0, 2, H);
    }
  }

  // ─────────────── automation ───────────────

  const AUTO_PAD = 9;
  const AUTO_COLOR = "#fbbf24";
  function autoY(lane: AutoLane, v: number, top: number, h: number) {
    const r = AUTO_RANGE[lane];
    const k = (Math.min(r.max, Math.max(r.min, v)) - r.min) / (r.max - r.min);
    return top + AUTO_PAD + (1 - k) * (h - 2 * AUTO_PAD);
  }
  function autoV(lane: AutoLane, y: number, top: number, h: number) {
    const r = AUTO_RANGE[lane];
    const k = 1 - (y - top - AUTO_PAD) / Math.max(1, h - 2 * AUTO_PAD);
    const v = r.min + Math.min(1, Math.max(0, k)) * (r.max - r.min);
    return lane === "pan" ? Math.round(v * 100) / 100 : Math.round(v * 10) / 10;
  }
  function autoLabel(lane: AutoLane, v: number) {
    if (lane === "volume") return `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
    return Math.abs(v) < 0.005 ? "C" : `${v < 0 ? "L" : "R"}${Math.round(Math.abs(v) * 100)}`;
  }

  function drawAuto(ctx: CanvasRenderingContext2D, tr: Track, lane: AutoLane, top: number, h: number) {
    const pts = tr.auto![lane];
    const def = AUTO_RANGE[lane].def(tr);
    // dim the clips so the envelope reads clearly
    ctx.fillStyle = "rgba(8,6,16,0.42)";
    ctx.fillRect(0, top, W, h - 1);
    // reference line: 0 dB or centre
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.setLineDash([2, 4]);
    ctx.beginPath();
    const ry = Math.round(autoY(lane, 0, top, h)) + 0.5;
    ctx.moveTo(0, ry);
    ctx.lineTo(W, ry);
    ctx.stroke();
    ctx.setLineDash([]);
    // envelope polyline across the view
    ctx.strokeStyle = AUTO_COLOR;
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.moveTo(0, autoY(lane, envelopeAt(pts, fOf(0), def), top, h));
    for (const p of pts) {
      const x = xOf(p.t);
      if (x < -50 || x > W + 50) continue;
      ctx.lineTo(x, autoY(lane, p.v, top, h));
    }
    ctx.lineTo(W, autoY(lane, envelopeAt(pts, fOf(W), def), top, h));
    ctx.stroke();
    ctx.lineWidth = 1;
    // translucent fill under the line
    ctx.lineTo(W, top + h - 1);
    ctx.lineTo(0, top + h - 1);
    ctx.closePath();
    ctx.fillStyle = "rgba(251,191,36,0.07)";
    ctx.fill();
    // points
    pts.forEach((p, i) => {
      const x = xOf(p.t);
      if (x < -6 || x > W + 6) return;
      const y = autoY(lane, p.v, top, h);
      const hot = autoHot?.track === tr.id && autoHot.i === i;
      ctx.beginPath();
      ctx.arc(x, y, hot ? 5 : 3.6, 0, Math.PI * 2);
      ctx.fillStyle = hot ? "#fff" : AUTO_COLOR;
      ctx.fill();
      ctx.strokeStyle = "rgba(0,0,0,0.6)";
      ctx.stroke();
      if (hot) {
        const label = autoLabel(lane, p.v);
        ctx.font = "600 11px Inter, system-ui, sans-serif";
        const tw = ctx.measureText(label).width + 10;
        const lx = Math.min(W - tw - 2, x + 8);
        const ly = Math.max(top + 2, y - 22);
        ctx.fillStyle = "rgba(10,8,20,0.85)";
        rr(ctx, lx, ly, tw, 17, 5);
        ctx.fill();
        ctx.fillStyle = "#fff";
        ctx.fillText(label, lx + 5, ly + 12.5);
      }
    });
    // caption
    ctx.font = "600 10px Inter, system-ui, sans-serif";
    ctx.fillStyle = "rgba(251,191,36,0.85)";
    ctx.fillText(lane === "volume" ? t("autoVolume") : t("autoPan"), 8, top + h - 8);
  }

  /** Automation lane under `y` with the nearest point within reach of (x, y). */
  function autoHit(x: number, y: number): { tr: Track; lane: AutoLane; top: number; i: number } | null {
    const ta = trackAt(y);
    if (!ta || !ta.tr.auto || ta.tr.auto.show === "off") return null;
    const lane = ta.tr.auto.show;
    const pts = ta.tr.auto[lane];
    let best = -1;
    let bd = 8;
    pts.forEach((p, i) => {
      const d = Math.hypot(xOf(p.t) - x, autoY(lane, p.v, ta.top, ta.tr.height) - y);
      if (d < bd) {
        bd = d;
        best = i;
      }
    });
    return { tr: ta.tr, lane, top: ta.top, i: best };
  }

  /** Pointer down on an automation lane: grab a point or add one, then drag it (Shift: value only). */
  function autoDown(e: PointerEvent, ah: { tr: Track; lane: AutoLane; top: number; i: number }, snapF: (f: number) => number) {
    const id = ah.tr.id;
    const lane = ah.lane;
    const { x, y } = localXY(e);
    beginGesture();
    let i = ah.i;
    if (i < 0) {
      const pts = autoOf(getTrack(id)!)[lane];
      const pt: AutoPoint = { t: Math.max(0, snapF(fOf(x))), v: autoV(lane, y, ah.top, ah.tr.height) };
      i = pts.findIndex((p) => p.t > pt.t);
      if (i < 0) i = pts.length;
      pts.splice(i, 0, pt);
      syncEngine();
    }
    autoHot = { track: id, i };
    laneCv!.setPointerCapture(e.pointerId);
    dragging = true;
    const mv = (ev: PointerEvent) => {
      const tr = getTrack(id);
      if (!tr?.auto) return;
      const pts = tr.auto[lane];
      const r = laneCv!.getBoundingClientRect();
      const lo = i > 0 ? pts[i - 1].t + 1 : 0;
      const hi = i < pts.length - 1 ? pts[i + 1].t - 1 : Number.MAX_SAFE_INTEGER;
      if (!ev.shiftKey) pts[i].t = Math.min(hi, Math.max(lo, snapF(fOf(ev.clientX - r.left))));
      pts[i].v = autoV(lane, ev.clientY - r.top, ah.top, tr.height);
      syncEngine();
      schedule();
    };
    const up = () => {
      dragging = false;
      laneCv?.releasePointerCapture(e.pointerId);
      laneCv?.removeEventListener("pointermove", mv);
      laneCv?.removeEventListener("pointerup", up);
      endGesture();
    };
    laneCv!.addEventListener("pointermove", mv);
    laneCv!.addEventListener("pointerup", up);
  }

  function drawClip(ctx: CanvasRenderingContext2D, tr: Track, c: Clip, y: number, h: number, dim: boolean) {
    const x0 = xOf(c.start);
    const x1 = xOf(c.start + c.len);
    if (x1 < -2 || x0 > W + 2) return;
    const sel = ed.selClips.includes(c.id);
    const cy = y + 3;
    const ch = h - 7;
    const header = ch > 44 ? 15 : 0;
    ctx.save();
    ctx.globalAlpha = dim || c.muted ? 0.38 : 1;
    rr(ctx, x0, cy, Math.max(2, x1 - x0), ch, 7);
    const grad = ctx.createLinearGradient(0, cy, 0, cy + ch);
    grad.addColorStop(0, alpha(tr.color, sel ? 0.42 : 0.28));
    grad.addColorStop(1, alpha(tr.color, sel ? 0.26 : 0.14));
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.lineWidth = sel ? 1.6 : 1;
    ctx.strokeStyle = sel ? "rgba(255,255,255,0.92)" : alpha(tr.color, 0.75);
    ctx.stroke();
    ctx.clip();
    if (header) {
      ctx.fillStyle = alpha(tr.color, sel ? 0.55 : 0.38);
      ctx.fillRect(x0, cy, x1 - x0, header);
      if (x1 - x0 > 30) {
        ctx.fillStyle = "rgba(255,255,255,0.92)";
        ctx.font = "600 10.5px 'Segoe UI Variable Text', 'Segoe UI', sans-serif";
        ctx.textBaseline = "middle";
        const label = (c.muted ? "[M] " : "") + (c.name || source(c.source)?.name || "");
        ctx.fillText(label, Math.max(x0, 0) + 6, cy + header / 2 + 0.5, Math.max(10, x1 - Math.max(x0, 0) - 12));
      }
    }
    // waveform
    const pk = getPeaks(c.source);
    const wy = cy + header + (ch - header) / 2;
    const wh = (ch - header) / 2 - 3;
    ctx.fillStyle = "rgba(255,255,255,0.08)";
    ctx.fillRect(Math.max(0, x0), wy, Math.min(W, x1) - Math.max(0, x0), 1);
    if (pk) {
      const gain = Math.pow(10, c.gain / 20);
      drawWave({
        ctx,
        id: c.source,
        peaks: pk,
        x0: Math.max(0, x0),
        x1: Math.min(W, x1),
        y: wy,
        h: wh,
        frameAt: (x) => c.offset + (fOf(x) - c.start),
        fpp: ed.fpp,
        env: (x) => {
          const tt = fOf(x) - c.start;
          let g = gain;
          if (c.fadeIn > 0 && tt < c.fadeIn) g *= fadeGain(tt / c.fadeIn);
          if (c.fadeOut > 0 && c.len - tt < c.fadeOut) g *= fadeGain((c.len - tt) / c.fadeOut);
          return g;
        },
        color: sel && S.settings.wave.color !== "spectral" ? "rgba(255,255,255,0.95)" : alpha(lighten(tr.color), 0.92),
        accent: sel ? "#ffffff" : lighten(lighten(tr.color)),
        selected: sel,
        bands: S.settings.wave.color === "spectral" || S.settings.wave.style === "rms" ? getBands(c.source) : undefined,
        scale: S.settings.wave.scale,
        style: S.settings.wave.style,
        colorMode: S.settings.wave.color,
        dpr,
      });
    }
    // fades
    const fy0 = cy + header;
    for (const [len, left] of [[c.fadeIn, true], [c.fadeOut, false]] as [number, boolean][]) {
      const fw = len / ed.fpp;
      const xa = left ? x0 : x1 - fw;
      const xb = left ? x0 + fw : x1;
      if (fw > 1) {
        ctx.fillStyle = "rgba(0,0,0,0.25)";
        ctx.beginPath();
        ctx.moveTo(xa, fy0);
        for (let i = 0; i <= 24; i++) {
          const u = i / 24;
          const g = fadeGain(left ? u : 1 - u);
          ctx.lineTo(xa + u * (xb - xa), fy0 + (1 - g) * (ch - header));
        }
        ctx.lineTo(xb, fy0);
        ctx.closePath();
        ctx.fill();
        ctx.strokeStyle = "rgba(255,255,255,0.6)";
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        for (let i = 0; i <= 24; i++) {
          const u = i / 24;
          const g = fadeGain(left ? u : 1 - u);
          const px = xa + u * (xb - xa);
          const py = fy0 + (1 - g) * (ch - header);
          if (i === 0) ctx.moveTo(px, py);
          else ctx.lineTo(px, py);
        }
        ctx.stroke();
      }
    }
    ctx.restore();
    // fade handles
    if (sel || hover?.clip === c.id) {
      const hx = [x0 + c.fadeIn / ed.fpp, x1 - c.fadeOut / ed.fpp];
      for (const [i, hxx] of hx.entries()) {
        const active = hover?.clip === c.id && hover.zone === (i === 0 ? "fadeIn" : "fadeOut");
        ctx.fillStyle = active ? "#fff" : "rgba(255,255,255,0.75)";
        ctx.beginPath();
        ctx.arc(Math.min(Math.max(hxx, x0 + 5), x1 - 5), cy + 5, active ? 4.5 : 3.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  function lighten(hex: string) {
    const n = parseInt(hex.replace("#", ""), 16);
    const m = (v: number) => Math.round(v + (255 - v) * 0.45);
    return "#" + [m((n >> 16) & 255), m((n >> 8) & 255), m(n & 255)].map((v) => v.toString(16).padStart(2, "0")).join("");
  }

  function drawRecording(ctx: CanvasRenderingContext2D, y: number, h: number) {
    const len = tp.recFrames * (app.rate / Math.max(1, tp.recRate));
    const x0 = xOf(tp.recStart);
    const x1 = xOf(tp.recStart + len);
    rr(ctx, x0, y + 3, Math.max(2, x1 - x0), h - 7, 7);
    ctx.fillStyle = "rgba(239,68,68,0.25)";
    ctx.fill();
    ctx.strokeStyle = "rgba(239,68,68,0.9)";
    ctx.stroke();
    const n = tp.recPeaks.length;
    if (n > 1) {
      const cy = y + h / 2;
      ctx.fillStyle = "rgba(254,202,202,0.9)";
      for (let i = 0; i < n; i++) {
        const xa = x0 + (i / n) * (x1 - x0);
        const v = Math.min(1, tp.recPeaks[i]) * (h / 2 - 8);
        ctx.fillRect(xa, cy - v, Math.max(1, (x1 - x0) / n), v * 2);
      }
    }
  }

  function drawRuler() {
    if (!rulerCv) return;
    const ctx = setup(rulerCv, W, RULER_H);
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#8b5cf6";
    const g = gridStep();
    // loop band
    const lp = P.p.loop;
    if (lp.end > lp.start) {
      const a = xOf(lp.start);
      const b = xOf(lp.end);
      rr(ctx, a, 3, b - a, 11, 4);
      ctx.fillStyle = lp.on ? alpha(accent, 0.75) : "rgba(255,255,255,0.18)";
      ctx.fill();
    }
    ctx.font = "10.5px 'Cascadia Mono', Consolas, monospace";
    ctx.textBaseline = "top";
    const f0 = Math.floor(fOf(0) / g.step) * g.step;
    for (let f = f0; f < fOf(W); f += g.step) {
      const x = Math.round(xOf(f)) + 0.5;
      const major = Math.abs(f / g.major - Math.round(f / g.major)) < 1e-6;
      ctx.fillStyle = major ? "rgba(255,255,255,0.45)" : "rgba(255,255,255,0.18)";
      ctx.fillRect(x, major ? 18 : 26, 1, major ? 16 : 8);
      if (major) {
        ctx.fillStyle = "rgba(255,255,255,0.7)";
        ctx.fillText(g.label(f), x + 4, 18);
      }
    }
    // selection
    if (ed.sel && ed.sel.end > ed.sel.start) {
      ctx.fillStyle = alpha(accent, 0.35);
      ctx.fillRect(xOf(ed.sel.start), RULER_H - 4, xOf(ed.sel.end) - xOf(ed.sel.start), 4);
    }
    // markers
    for (const m of P.p.markers) {
      const x = xOf(m.pos);
      if (x < -20 || x > W + 20) continue;
      ctx.fillStyle = "#fbbf24";
      ctx.beginPath();
      ctx.moveTo(x, 16);
      ctx.lineTo(x + 9, 16);
      ctx.lineTo(x + 9, 25);
      ctx.lineTo(x, 30);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = "#1a1204";
      ctx.font = "700 8.5px 'Segoe UI', sans-serif";
      ctx.fillText(m.name.slice(0, 2), x + 1.5, 17);
      ctx.font = "10.5px 'Cascadia Mono', Consolas, monospace";
    }
  }

  // ─────────────── hit testing ───────────────

  function trackAt(y: number): { tr: Track; top: number } | null {
    let yy = -ed.vscroll;
    for (const tr of P.p.tracks) {
      if (y >= yy && y < yy + tr.height) return { tr, top: yy };
      yy += tr.height;
    }
    return null;
  }

  function hit(x: number, y: number): { tr: Track; clip: Clip; zone: string; top: number } | null {
    const ta = trackAt(y);
    if (!ta) return null;
    const { tr, top } = ta;
    // topmost = last drawn
    for (let i = tr.clips.length - 1; i >= 0; i--) {
      const c = tr.clips[i];
      const x0 = xOf(c.start);
      const x1 = xOf(c.start + c.len);
      if (x < x0 - 1 || x > x1 + 1) continue;
      const ly = y - top;
      let zone = "body";
      const fiX = x0 + c.fadeIn / ed.fpp;
      const foX = x1 - c.fadeOut / ed.fpp;
      if (ly < 16 && Math.abs(x - Math.max(fiX, x0 + 5)) < 8) zone = "fadeIn";
      else if (ly < 16 && Math.abs(x - Math.min(foX, x1 - 5)) < 8) zone = "fadeOut";
      else if (x - x0 < 7 && x1 - x0 > 20) zone = "trimL";
      else if (x1 - x < 7 && x1 - x0 > 20) zone = "trimR";
      return { tr, clip: c, zone, top };
    }
    return null;
  }

  // ─────────────── lane interactions ───────────────

  function localXY(e: MouseEvent) {
    const r = laneCv!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function onMove(e: PointerEvent) {
    if (dragging) return;
    const { x, y } = localXY(e);
    const ah = autoHit(x, y);
    if (ah) {
      const nh = ah.i >= 0 ? { track: ah.tr.id, i: ah.i } : null;
      if (nh?.track !== autoHot?.track || nh?.i !== autoHot?.i) {
        autoHot = nh;
        schedule();
      }
      if (hover) hover = null;
      laneCv!.style.cursor = ah.i >= 0 ? "move" : "crosshair";
      return;
    }
    if (autoHot) {
      autoHot = null;
      schedule();
    }
    const h = hit(x, y);
    const nh = h ? { clip: h.clip.id, zone: h.zone } : null;
    if (nh?.clip !== hover?.clip || nh?.zone !== hover?.zone) hover = nh;
    laneCv!.style.cursor = !h ? "text" : h.zone === "trimL" || h.zone === "trimR" ? "ew-resize" : h.zone.startsWith("fade") ? "pointer" : "grab";
  }

  let dragging = false;

  function onDown(e: PointerEvent) {
    if (e.button === 2) return;
    const { x, y } = localXY(e);
    const h = hit(x, y);
    const ta = trackAt(y);
    if (ta) ed.selTrack = ta.tr.id;
    const startX = e.clientX;
    const startF = fOf(x);
    const noSnap = e.ctrlKey;
    const snapF = (f: number) => (noSnap ? Math.round(f) : snapFrame(f));
    const ah = autoHit(x, y);
    if (ah) {
      autoDown(e, ah, snapF);
      return;
    }
    laneCv!.setPointerCapture(e.pointerId);
    dragging = true;
    let moved = false;

    let onMoveFn: (ev: PointerEvent) => void;
    let onUpFn: (ev: PointerEvent) => void = () => {};

    if (h && !e.altKey) {
      const c = h.clip;
      if (e.shiftKey) {
        ed.selClips = ed.selClips.includes(c.id) ? ed.selClips.filter((i) => i !== c.id) : [...ed.selClips, c.id];
      } else if (!ed.selClips.includes(c.id)) ed.selClips = [c.id];
      ed.inspectorTab = ed.inspectorTab === "track" || ed.inspectorTab === "master" ? "clip" : ed.inspectorTab;
      beginGesture();
      const orig = new Map(ed.selClips.map((id) => [id, { ...findClip(id)!.clip, trackIdx: P.p.tracks.indexOf(findClip(id)!.track) }]));
      const o = { ...c };
      const srcFrames = source(c.source)?.frames ?? c.offset + c.len;
      const startTrackIdx = P.p.tracks.indexOf(h.tr);
      onMoveFn = (ev) => {
        const dx = (ev.clientX - startX) * ed.fpp;
        if (!moved && Math.abs(ev.clientX - startX) < 3 && Math.abs(ev.clientY - e.clientY) < 3) return;
        moved = true;
        const cur = findClip(c.id);
        if (!cur) return;
        const cl = cur.clip;
        switch (h.zone) {
          case "trimL": {
            let ns = snapF(o.start + dx);
            ns = Math.max(o.start - o.offset, Math.min(ns, o.start + o.len - 64));
            const d = ns - o.start;
            cl.start = ns;
            cl.offset = o.offset + d;
            cl.len = o.len - d;
            cl.fadeIn = Math.min(cl.fadeIn, cl.len);
            break;
          }
          case "trimR": {
            let ne = snapF(o.start + o.len + dx);
            ne = Math.max(o.start + 64, Math.min(ne, o.start + (srcFrames - o.offset)));
            cl.len = ne - o.start;
            cl.fadeOut = Math.min(cl.fadeOut, cl.len);
            break;
          }
          case "fadeIn":
            cl.fadeIn = Math.max(0, Math.min(Math.round(o.fadeIn + dx), cl.len - cl.fadeOut));
            break;
          case "fadeOut":
            cl.fadeOut = Math.max(0, Math.min(Math.round(o.fadeOut - dx), cl.len - cl.fadeIn));
            break;
          default: {
            // move all selected clips; vertical by whole tracks
            const r = laneCv!.getBoundingClientRect();
            const ta2 = trackAt(ev.clientY - r.top);
            const dT = ta2 ? P.p.tracks.indexOf(ta2.tr) - startTrackIdx : 0;
            const snapped = snapF(o.start + dx) - o.start;
            for (const [id, oc] of orig) {
              const f = findClip(id);
              if (!f) continue;
              f.clip.start = Math.max(0, oc.start + snapped);
              const ti = Math.min(P.p.tracks.length - 1, Math.max(0, oc.trackIdx + dT));
              if (P.p.tracks[ti] !== f.track) {
                const from = f.track.clips;
                from.splice(from.findIndex((x) => x.id === id), 1);
                P.p.tracks[ti].clips.push(f.clip);
              }
            }
          }
        }
        syncEngine();
      };
      onUpFn = () => {
        endGesture();
        if (!moved && !e.shiftKey && h.zone === "body" && ed.selClips.length > 1) ed.selClips = [c.id];
      };
    } else {
      // range selection
      ed.selClips = e.shiftKey ? ed.selClips : [];
      const a = snapF(startF);
      onMoveFn = (ev) => {
        if (!moved && Math.abs(ev.clientX - startX) < 3) return;
        moved = true;
        const r = laneCv!.getBoundingClientRect();
        const b = snapF(fOf(ev.clientX - r.left));
        ed.sel = { start: Math.max(0, Math.min(a, b)), end: Math.max(a, b) };
        autoScroll(ev.clientX - r.left);
      };
      onUpFn = () => {
        if (!moved) {
          ed.sel = null;
          seek(snapF(startF));
        }
      };
    }
    const up = (ev: PointerEvent) => {
      dragging = false;
      laneCv?.releasePointerCapture(e.pointerId);
      laneCv?.removeEventListener("pointermove", onMoveFn);
      laneCv?.removeEventListener("pointerup", up);
      onUpFn(ev);
    };
    laneCv!.addEventListener("pointermove", onMoveFn);
    laneCv!.addEventListener("pointerup", up);
  }

  function autoScroll(x: number) {
    if (x > W - 20) ed.scroll += ed.fpp * 12;
    else if (x < 20) ed.scroll = Math.max(0, ed.scroll - ed.fpp * 12);
  }

  function onDbl(e: MouseEvent) {
    const { x, y } = localXY(e);
    const ah = autoHit(x, y);
    if (ah) {
      if (ah.i >= 0) {
        commit(() => autoOf(getTrack(ah.tr.id)!)[ah.lane].splice(ah.i, 1));
        autoHot = null;
      }
      return;
    }
    const h = hit(x, y);
    if (h) ed.sel = { start: h.clip.start, end: h.clip.start + h.clip.len };
  }

  function onContext(e: MouseEvent) {
    const { x, y } = localXY(e);
    const h = hit(x, y);
    const at = fOf(x);
    if (h) {
      if (!ed.selClips.includes(h.clip.id)) ed.selClips = [h.clip.id];
      openMenu(e, [
        { label: t("split"), icon: "split", hint: "S", run: () => splitAt(snapFrame(at)) },
        { label: t("duplicate"), icon: "copy", hint: "Ctrl+D", run: duplicateSelected },
        { label: t("copy"), icon: "copy", hint: "Ctrl+C", run: () => copySelected() },
        { label: t("cut"), icon: "scissors", hint: "Ctrl+X", run: () => copySelected(true) },
        { sep: true },
        { label: t("stemSeparation"), icon: "stems", run: () => openStemDialog() },
        { label: t("spectralTitle") + "…", icon: "chart", run: () => openSpectral(h.clip.id) },
        { label: t("remixTitle") + "…", icon: "sliders", run: () => ((ed.selClips = [h.clip.id]), openRemix()) },
        { label: "Normalizacja / Normalise −1 dB", icon: "wand", run: () => applyOp({ op: "normalize", mode: "peak", target: -1, perChannel: false }, false) },
        { label: t("analyze"), icon: "chart", run: () => (ed.inspectorTab = "analysis") },
        { label: t("muteClip"), icon: "mute", run: () => commit(() => (findClip(h.clip.id)!.clip.muted = !h.clip.muted)) },
        { sep: true },
        { label: t("delete"), icon: "trash", hint: "Del", danger: true, run: deleteSelected },
      ]);
    } else {
      openMenu(e, [
        { label: t("paste"), icon: "paste", hint: "Ctrl+V", run: () => (seek(snapFrame(at)), paste()) },
        { label: t("addMarker"), icon: "marker", hint: "M", run: () => (seek(snapFrame(at)), addMarker()) },
        { sep: true },
        { label: t("deleteRange"), icon: "scissors", disabled: !ed.sel, run: () => deleteRange(true) },
        { label: t("silenceRange"), icon: "mute", disabled: !ed.sel, run: () => deleteRange(false, true) },
        { label: t("cropToRange"), icon: "fit", disabled: !ed.sel, run: cropToRange },
        { label: t("bounceRange"), icon: "bounce", run: bounceRange },
        { label: t("loop"), icon: "loop", hint: "L", run: toggleLoop },
        { sep: true },
        { label: t("addTrack"), icon: "plus", run: addTrackCmd },
      ]);
    }
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const r = laneCv!.getBoundingClientRect();
    if (e.ctrlKey) {
      zoomAt(e.deltaY > 0 ? 1.18 : 1 / 1.18, fOf(e.clientX - r.left));
      return;
    }
    const horiz = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
    const d = horiz ? (e.shiftKey ? e.deltaY : e.deltaX) : e.deltaY;
    if (horiz || totalH <= H) ed.scroll = Math.max(0, ed.scroll + d * ed.fpp);
    else ed.vscroll = Math.max(0, Math.min(totalH - H + 40, ed.vscroll + d));
  }

  // ─────────────── ruler interactions ───────────────

  function rulerDown(e: PointerEvent) {
    const r = rulerCv!.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    rulerCv!.setPointerCapture(e.pointerId);
    const loopZone = y < 15;
    if (loopZone) {
      beginGesture();
      const a = snapFrame(fOf(x), true);
      const mv = (ev: PointerEvent) => {
        const b = snapFrame(fOf(ev.clientX - r.left), true);
        P.p.loop.start = Math.min(a, b);
        P.p.loop.end = Math.max(a, b);
        if (P.p.loop.end > P.p.loop.start) P.p.loop.on = true;
        syncEngine();
      };
      const up = () => {
        rulerCv!.removeEventListener("pointermove", mv);
        rulerCv!.removeEventListener("pointerup", up);
        endGesture();
      };
      rulerCv!.addEventListener("pointermove", mv);
      rulerCv!.addEventListener("pointerup", up);
      return;
    }
    setDraggingPlayhead(true);
    const go = (cx: number) => seek(e.ctrlKey ? fOf(cx - r.left) : snapFrame(fOf(cx - r.left)));
    go(e.clientX);
    const mv = (ev: PointerEvent) => go(ev.clientX);
    const up = () => {
      setDraggingPlayhead(false);
      rulerCv!.removeEventListener("pointermove", mv);
      rulerCv!.removeEventListener("pointerup", up);
    };
    rulerCv!.addEventListener("pointermove", mv);
    rulerCv!.addEventListener("pointerup", up);
  }

  function rulerContext(e: MouseEvent) {
    const r = rulerCv!.getBoundingClientRect();
    const x = e.clientX - r.left;
    const m = P.p.markers.find((mk) => Math.abs(xOf(mk.pos) - x) < 8);
    openMenu(e, [
      { label: t("addMarker"), icon: "marker", run: () => commit(() => P.p.markers.push({ id: Date.now(), pos: snapFrame(fOf(x)), name: String(P.p.markers.length + 1) })) },
      { label: t("remove") + " " + t("markers").toLowerCase(), icon: "trash", disabled: !m, run: () => commit(() => (P.p.markers = P.p.markers.filter((k) => k !== m))) },
      { sep: true },
      { label: t("loop"), icon: "loop", run: toggleLoop },
      { label: S.settings.timeMode === "time" ? t("bars") : t("seconds"), icon: "metronome", run: () => (S.settings.timeMode = S.settings.timeMode === "time" ? "bars" : "time") },
    ]);
  }

  // ─────────────── drag & drop from the library ───────────────

  function dragOver(e: DragEvent) {
    if (!e.dataTransfer?.types.includes("text/oap-source")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    const { x, y } = localXY(e);
    const ta = trackAt(y);
    dropHint = { track: ta?.tr.id ?? null, frame: snapFrame(fOf(x)), y };
  }
  function drop(e: DragEvent) {
    const id = +(e.dataTransfer?.getData("text/oap-source") ?? 0);
    const hint = dropHint;
    dropHint = null;
    if (!id || !hint) return;
    e.preventDefault();
    const s = source(id);
    if (!s) return;
    if (hint.track !== null) addSourceToTrack(s, hint.track, hint.frame);
    else addSourceToNewTrack(s, hint.frame);
  }

  // ─────────────── horizontal scrollbar ───────────────

  const content = $derived(Math.max(projectLength(), fOf(W), app.rate * 30) * 1.15);
  const thumbW = $derived(Math.max(30, (W * ed.fpp * W) / content));
  const thumbX = $derived((ed.scroll / content) * W);

  function scrollDown(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const r = el.getBoundingClientRect();
    const sx = e.clientX;
    const s0 = ed.scroll;
    const onThumb = e.clientX - r.left >= thumbX && e.clientX - r.left <= thumbX + thumbW;
    if (!onThumb) ed.scroll = Math.max(0, ((e.clientX - r.left - thumbW / 2) / W) * content);
    const s1 = ed.scroll;
    const mv = (ev: PointerEvent) => (ed.scroll = Math.max(0, (onThumb ? s0 : s1) + ((ev.clientX - sx) / W) * content));
    const up = () => {
      el.removeEventListener("pointermove", mv);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", mv);
    el.addEventListener("pointerup", up);
  }

  const playX = $derived(xOf(tp.pos));
</script>

<div class="tl">
  <div class="ruler-row">
    <div class="corner" style="width:{HEADER_W}px">
      <button class="btn sm ghost" onclick={addTrackCmd} data-tip={t("addTrack")}><Icon name="plus" size={14} />{t("track")}</button>
      <div class="grow"></div>
      <div class="seg mode">
        <button class:on={S.settings.timeMode === "time"} onclick={() => (S.settings.timeMode = "time")} data-tip={t("seconds")}>s</button>
        <button class:on={S.settings.timeMode === "bars"} onclick={() => (S.settings.timeMode = "bars")} data-tip={t("bars")}>♩</button>
      </div>
    </div>
    <div class="ruler">
      <canvas bind:this={rulerCv} style="width:{W}px;height:{RULER_H}px" onpointerdown={rulerDown} oncontextmenu={rulerContext}></canvas>
      {#if playX >= -1 && playX <= W + 1}
        <div class="ph-head" style="left:{playX}px"></div>
      {/if}
    </div>
  </div>

  <div class="body">
    <div class="headers" style="width:{HEADER_W}px">
      <div class="hs" style="transform:translateY({-ed.vscroll}px)">
        {#each P.p.tracks as tr (tr.id)}
          <TrackHeader {tr} />
        {/each}
        <button class="add-track" onclick={addTrackCmd}><Icon name="plus" size={15} />{t("addTrack")}</button>
      </div>
    </div>
    <div class="lanes" bind:this={lanesEl} role="application" ondragover={dragOver} ondragleave={() => (dropHint = null)} ondrop={drop}>
      <canvas
        bind:this={laneCv}
        style="width:{W}px;height:{H}px"
        onpointerdown={onDown}
        onpointermove={onMove}
        onpointerleave={() => (hover = null)}
        ondblclick={onDbl}
        oncontextmenu={onContext}
        onwheel={onWheel}
      ></canvas>
      {#if playX >= -1 && playX <= W + 1}
        <div class="playhead" class:rec={tp.recording} style="left:{playX}px"></div>
      {/if}
      {#if ed.sel && ed.sel.end > ed.sel.start}
        <div class="sel-info mono glass">{fmtTime(ed.sel.start, app.rate)} – {fmtTime(ed.sel.end, app.rate)} · {fmtTime(ed.sel.end - ed.sel.start, app.rate)}</div>
      {/if}
    </div>
  </div>
  <div class="hbar">
    <div style="width:{HEADER_W}px"></div>
    <div class="track" role="scrollbar" aria-controls="lanes" aria-valuenow={ed.scroll} tabindex="-1" onpointerdown={scrollDown}>
      <div class="thumb" style="left:{thumbX}px;width:{thumbW}px"></div>
    </div>
  </div>
</div>

<style>
  .tl {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }
  .ruler-row {
    display: flex;
    height: 34px;
    flex: none;
    border-bottom: 1px solid var(--line);
  }
  .corner {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border-right: 1px solid var(--line);
  }
  .mode button {
    height: 20px;
    padding: 0 7px;
    font-size: 11px;
  }
  .ruler {
    flex: 1;
    position: relative;
    min-width: 0;
    overflow: hidden;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.12);
  }
  .ruler canvas {
    display: block;
  }
  .ph-head {
    position: absolute;
    bottom: 0;
    width: 0;
    height: 0;
    margin-left: -6px;
    border-left: 6px solid transparent;
    border-right: 6px solid transparent;
    border-top: 9px solid #fff;
    pointer-events: none;
    filter: drop-shadow(0 0 4px rgba(var(--accent-rgb), 0.9));
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .headers {
    flex: none;
    overflow: hidden;
    border-right: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.1);
  }
  .add-track {
    display: flex;
    align-items: center;
    gap: 8px;
    width: calc(100% - 16px);
    margin: 8px;
    height: 34px;
    justify-content: center;
    border-radius: 10px;
    border: 1px dashed var(--line-2);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .add-track:hover {
    color: var(--text);
    border-color: rgba(var(--accent-rgb), 0.7);
    background: rgba(var(--accent-rgb), 0.08);
  }
  .lanes {
    flex: 1;
    position: relative;
    min-width: 0;
    overflow: hidden;
  }
  .lanes canvas {
    display: block;
    touch-action: none;
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 10px rgba(var(--accent-rgb), 0.95);
    pointer-events: none;
  }
  .playhead.rec {
    background: var(--rec);
    box-shadow: 0 0 12px rgba(239, 68, 68, 0.9);
  }
  .sel-info {
    position: absolute;
    right: 10px;
    bottom: 8px;
    padding: 4px 9px;
    font-size: 11px;
    border-radius: 8px;
    pointer-events: none;
    color: var(--text-2);
  }
  .hbar {
    display: flex;
    height: 12px;
    flex: none;
    border-top: 1px solid var(--line);
  }
  .hbar .track {
    flex: 1;
    position: relative;
    cursor: pointer;
  }
  .thumb {
    position: absolute;
    top: 3px;
    height: 6px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.2);
  }
  .thumb:hover {
    background: rgba(255, 255, 255, 0.35);
  }
</style>
