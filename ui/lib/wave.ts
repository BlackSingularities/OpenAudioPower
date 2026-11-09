// Waveform rendering: min/max pyramid built from the 64-sample peak overview, frequency-band
// colouring, relative / dB / linear amplitude scales, several drawing styles, and raw samples
// fetched on demand when zoomed in below one peak bin per pixel.

import { api, type Bands, type Peaks } from "./api";

export type WaveScale = "relative" | "db" | "linear";
export type WaveStyle = "fill" | "rms" | "bars" | "outline";
export type WaveColor = "track" | "spectral" | "gradient";

interface Level {
  spp: number;
  data: Float32Array; // merged channels, [min,max,…]
}
interface Pyramid {
  levels: Level[];
  /** amplitude statistics of the whole source for the relative scale */
  lo: number; // dB of the 5th percentile of bin peaks
  hi: number; // dB of the maximum
  bandNorm: [number, number, number, number]; // 95th percentile of rms, low, mid, high
}

const pyramids = new Map<number, Pyramid>();

const toDb = (v: number) => (v <= 1e-6 ? -120 : 20 * Math.log10(v));

function pctl(values: Float32Array | number[], q: number): number {
  const step = Math.max(1, Math.floor(values.length / 20000));
  const s: number[] = [];
  for (let i = 0; i < values.length; i += step) s.push(values[i]);
  s.sort((a, b) => a - b);
  return s.length ? s[Math.min(s.length - 1, Math.floor(q * (s.length - 1)))] : 0;
}

function pyramid(id: number, pk: Peaks): Pyramid {
  let p = pyramids.get(id);
  if (p) return p;
  const n = pk.bins;
  const base = new Float32Array(n * 2);
  const amp = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    let lo = Infinity;
    let hi = -Infinity;
    for (let c = 0; c < pk.channels; c++) {
      lo = Math.min(lo, pk.data[c][i * 2]);
      hi = Math.max(hi, pk.data[c][i * 2 + 1]);
    }
    base[i * 2] = lo;
    base[i * 2 + 1] = hi;
    amp[i] = Math.max(Math.abs(lo), Math.abs(hi));
  }
  const levels: Level[] = [{ spp: pk.spp, data: base }];
  let cur = base;
  let spp = pk.spp;
  while (cur.length / 2 > 64) {
    const m = Math.ceil(cur.length / 2 / 4);
    const next = new Float32Array(m * 2);
    for (let i = 0; i < m; i++) {
      let lo = Infinity;
      let hi = -Infinity;
      for (let j = i * 4; j < Math.min(i * 4 + 4, cur.length / 2); j++) {
        lo = Math.min(lo, cur[j * 2]);
        hi = Math.max(hi, cur[j * 2 + 1]);
      }
      next[i * 2] = lo;
      next[i * 2 + 1] = hi;
    }
    spp *= 4;
    levels.push({ spp, data: next });
    cur = next;
  }
  const max = pctl(amp, 1);
  const hiDb = toDb(Math.max(max, 1e-4));
  // floor: quiet passages, but never more than 42 dB below the peak and never above peak − 12 dB
  const loDb = Math.min(hiDb - 12, Math.max(hiDb - 42, toDb(Math.max(pctl(amp, 0.05), 1e-5))));
  p = { levels, lo: loDb, hi: hiDb, bandNorm: [1, 1, 1, 1] };
  pyramids.set(id, p);
  return p;
}

const bandNorms = new Map<number, [number, number, number, number]>();
function bandNorm(id: number, b: Bands): [number, number, number, number] {
  let n = bandNorms.get(id);
  if (n) return n;
  const cols: number[][] = [[], [], [], []];
  for (let i = 0; i < b.bins; i += Math.max(1, Math.floor(b.bins / 20000))) for (let k = 0; k < 4; k++) cols[k].push(b.data[i * 4 + k]);
  n = cols.map((c) => Math.max(1e-5, pctl(c, 0.95))) as [number, number, number, number];
  bandNorms.set(id, n);
  return n;
}

export function forgetWave(id: number) {
  pyramids.delete(id);
  bandNorms.delete(id);
  for (const k of [...raw.keys()]) if (k.startsWith(id + ":")) raw.delete(k);
}

/** Maps an absolute amplitude (≥ 0, already including clip gain) to 0..1 of the half height. */
function scaler(scale: WaveScale, py: Pyramid, minSpan = 12) {
  switch (scale) {
    case "linear":
      return (v: number) => Math.min(1, v);
    case "db":
      return (v: number) => Math.max(0, Math.min(1, (toDb(v) + 54) / 54));
    default: {
      const lo = Math.min(py.lo, py.hi - minSpan);
      const span = Math.max(6, py.hi - lo);
      return (v: number) => (v <= 1e-6 ? 0 : Math.max(0.025, Math.min(1, 0.06 + (0.94 * (toDb(v) - lo)) / span)));
    }
  }
}

/** Inverse of the display scale: drawn height 0–1 → sample amplitude (for the pencil). */
export function waveInverse(id: number, pk: Peaks, scale: WaveScale): (a: number) => number {
  const py = pyramid(id, pk);
  switch (scale) {
    case "linear":
      return (a) => a;
    case "db":
      return (a) => (a <= 0 ? 0 : Math.pow(10, (a * 54 - 54) / 20));
    default: {
      const lo = Math.min(py.lo, py.hi - 12);
      const span = Math.max(6, py.hi - lo);
      return (a) => (a <= 0.03 ? 0 : Math.pow(10, (lo + ((Math.min(1, a) - 0.06) * span) / 0.94) / 20));
    }
  }
}

// band colours: bass red-orange, mids green, highs blue
const C_LOW = [255, 78, 64];
const C_MID = [96, 230, 120];
const C_HIGH = [90, 150, 255];

function spectralColor(l: number, m: number, h: number, bright: number, alpha: number): string {
  const wl = Math.pow(l, 1.6);
  const wm = Math.pow(m, 1.6);
  const wh = Math.pow(h * 1.1, 1.6);
  const s = wl + wm + wh || 1;
  const k = 0.55 + 0.45 * Math.min(1, bright);
  const r = ((C_LOW[0] * wl + C_MID[0] * wm + C_HIGH[0] * wh) / s) * k + 40 * (1 - k);
  const g = ((C_LOW[1] * wl + C_MID[1] * wm + C_HIGH[1] * wh) / s) * k + 40 * (1 - k);
  const b = ((C_LOW[2] * wl + C_MID[2] * wm + C_HIGH[2] * wh) / s) * k + 40 * (1 - k);
  return `rgba(${r | 0},${g | 0},${b | 0},${alpha})`;
}

// raw sample blocks
const BLOCK = 32768;
const raw = new Map<string, Float32Array | null>();
let onRaw: () => void = () => {};
export function setRawListener(f: () => void) {
  onRaw = f;
}

function rawBlock(id: number, b: number): Float32Array | null | undefined {
  const key = `${id}:${b}`;
  if (raw.has(key)) return raw.get(key);
  raw.set(key, null);
  if (raw.size > 400) raw.delete(raw.keys().next().value!);
  api
    .samples(id, b * BLOCK, BLOCK)
    .then((ch) => {
      const n = ch[0]?.length ?? 0;
      const m = new Float32Array(n);
      for (let i = 0; i < n; i++) {
        let s = 0;
        for (const c of ch) s += c[i];
        m[i] = s / ch.length;
      }
      raw.set(key, m);
      onRaw();
    })
    .catch(() => raw.delete(key));
  return undefined;
}

export interface WaveOpts {
  ctx: CanvasRenderingContext2D;
  id: number;
  peaks: Peaks;
  bands?: Bands;
  x0: number; // canvas x of first column (css px)
  x1: number;
  y: number; // centre line
  h: number; // half height
  /** source frame at canvas x */
  frameAt: (x: number) => number;
  fpp: number;
  /** gain envelope (fades, clip gain) at canvas x */
  env: (x: number) => number;
  color: string;
  accent?: string;
  selected?: boolean;
  dpr: number;
  scale?: WaveScale;
  style?: WaveStyle;
  colorMode?: WaveColor;
}

export function drawWave(o: WaveOpts) {
  const total = o.peaks.bins * o.peaks.spp;
  const py = pyramid(o.id, o.peaks);
  const map = scaler(o.scale ?? "relative", py, o.style === "rms" ? 30 : 12);
  if (o.fpp < o.peaks.spp / 2) {
    if (drawRaw(o, total, map)) return;
  }
  const { ctx } = o;
  let lv = py.levels[0];
  for (const l of py.levels) if (l.spp <= o.fpp) lv = l;
  const style = o.style ?? "fill";
  const colorMode = o.colorMode ?? "track";
  const nb = lv.data.length / 2;
  const bands = o.bands;
  const bn = bands ? bandNorm(o.id, bands) : null;

  // column width: bars use wider columns with gaps
  const colW = style === "bars" ? 3 : 1 / o.dpr;
  const gap = style === "bars" ? 1 : 0;
  const step = colW + gap;

  interface Col {
    x: number;
    top: number;
    bot: number;
    /** outer peak envelope (relative / dB scales draw the body from the mean level) */
    pk: number;
    rms: number;
    color?: string;
  }
  const cols: Col[] = [];
  const linear = (o.scale ?? "relative") === "linear";
  for (let x = Math.floor(o.x0 / step) * step; x < o.x1; x += step) {
    const f0 = o.frameAt(x);
    const f1 = o.frameAt(x + step);
    if (f1 <= 0 || f0 >= total) continue;
    let b0 = Math.floor(Math.max(0, f0) / lv.spp);
    let b1 = Math.ceil(Math.min(total, f1) / lv.spp);
    if (b1 <= b0) b1 = b0 + 1;
    let lo = 0;
    let hi = 0;
    let sq = 0;
    let cnt = 0;
    // read the finest level for the mean so it is not pre-maximised by the pyramid
    const base = py.levels[0];
    const nb0 = base.data.length / 2;
    const s0 = Math.floor(Math.max(0, f0) / base.spp);
    const s1 = Math.max(s0 + 1, Math.ceil(Math.min(total, f1) / base.spp));
    const stride = Math.max(1, Math.floor((s1 - s0) / 64));
    for (let b = b0; b < Math.min(b1, nb); b++) {
      lo = Math.min(lo, lv.data[b * 2]);
      hi = Math.max(hi, lv.data[b * 2 + 1]);
    }
    if (!linear) {
      for (let b = s0; b < Math.min(s1, nb0); b += stride) {
        const a = Math.max(-base.data[b * 2], base.data[b * 2 + 1]);
        sq += a * a;
        cnt++;
      }
    }
    const g = o.env(x + colW / 2);
    let top: number;
    let bot: number;
    let pk: number;
    if (linear) {
      top = map(Math.max(0, hi) * g);
      bot = map(Math.max(0, -lo) * g);
      pk = Math.max(top, bot);
    } else {
      pk = map(Math.max(hi, -lo) * g);
      top = bot = Math.min(pk, map(Math.sqrt(sq / Math.max(1, cnt)) * g));
    }
    const col: Col = { x, top, bot, pk, rms: 0 };
    if (bands && (style === "rms" || colorMode === "spectral")) {
      const c0 = Math.floor(Math.max(0, f0) / bands.spp);
      const c1 = Math.max(c0 + 1, Math.ceil(Math.min(total, f1) / bands.spp));
      let r = 0;
      let l = 0;
      let m = 0;
      let h = 0;
      let k = 0;
      for (let c = c0; c < Math.min(c1, bands.bins); c++) {
        r += bands.data[c * 4] ** 2;
        l += bands.data[c * 4 + 1] ** 2;
        m += bands.data[c * 4 + 2] ** 2;
        h += bands.data[c * 4 + 3] ** 2;
        k++;
      }
      k = Math.max(1, k);
      col.rms = map(Math.sqrt(r / k) * g);
      if (colorMode === "spectral" && bn) {
        const nl = Math.sqrt(l / k) / bn[1];
        const nm = Math.sqrt(m / k) / bn[2];
        const nh = Math.sqrt(h / k) / bn[3];
        col.color = spectralColor(nl, nm, nh, Math.max(top, bot), o.selected ? 1 : 0.95);
      }
    }
    cols.push(col);
  }
  if (!cols.length) return;

  // base fill style
  let fill: string | CanvasGradient = o.color;
  if (colorMode === "gradient") {
    const gr = ctx.createLinearGradient(0, o.y - o.h, 0, o.y + o.h);
    gr.addColorStop(0, o.accent ?? "#f0abfc");
    gr.addColorStop(0.5, o.color);
    gr.addColorStop(1, o.accent ?? "#f0abfc");
    fill = gr;
  }
  const H = o.h;

  if (style === "outline") {
    ctx.lineWidth = 1.3;
    ctx.strokeStyle = fill;
    ctx.beginPath();
    cols.forEach((c, i) => (i ? ctx.lineTo(c.x, o.y - c.top * H) : ctx.moveTo(c.x, o.y - c.top * H)));
    ctx.stroke();
    ctx.beginPath();
    cols.forEach((c, i) => (i ? ctx.lineTo(c.x, o.y + c.bot * H) : ctx.moveTo(c.x, o.y + c.bot * H)));
    ctx.stroke();
    if (colorMode === "spectral") {
      for (const c of cols) if (c.color) {
        ctx.fillStyle = c.color;
        ctx.fillRect(c.x, o.y - c.top * H, colW, 1.5);
        ctx.fillRect(c.x, o.y + c.bot * H - 1.5, colW, 1.5);
      }
    }
    return;
  }

  if (!linear && style !== "rms") {
    // faint outer peak envelope behind the body
    ctx.globalAlpha = 0.28;
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.moveTo(cols[0].x, o.y - cols[0].pk * H);
    for (const c of cols) ctx.lineTo(c.x, o.y - c.pk * H);
    for (let i = cols.length - 1; i >= 0; i--) ctx.lineTo(cols[i].x, o.y + cols[i].pk * H);
    ctx.closePath();
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  if (style === "fill" && colorMode !== "spectral") {
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.moveTo(cols[0].x, o.y - cols[0].top * H);
    for (const c of cols) ctx.lineTo(c.x, o.y - c.top * H);
    for (let i = cols.length - 1; i >= 0; i--) ctx.lineTo(cols[i].x, o.y + Math.max(cols[i].bot * H, 0.5 - cols[i].top * H));
    ctx.closePath();
    ctx.fill();
    return;
  }

  // per-column drawing (spectral colours, RMS, bars)
  for (const c of cols) {
    const t = Math.max(0.5, c.top * H);
    const b = Math.max(0.5, c.bot * H);
    const colour = c.color ?? fill;
    if (style === "rms") {
      ctx.globalAlpha = 0.38;
      ctx.fillStyle = colour;
      const p = Math.max(0.5, c.pk * H);
      ctx.fillRect(c.x, o.y - p, colW + 0.02, p * 2);
      ctx.globalAlpha = 1;
      const r = Math.max(0.5, Math.min(c.rms || c.top, c.pk) * H);
      ctx.fillRect(c.x, o.y - r, colW + 0.02, r * 2);
    } else if (style === "bars") {
      ctx.fillStyle = colour;
      ctx.beginPath();
      ctx.roundRect(c.x, o.y - t, colW, t + b, 1.5);
      ctx.fill();
    } else {
      ctx.fillStyle = colour;
      ctx.fillRect(c.x, o.y - t, colW + 0.02, t + b);
    }
  }
  ctx.globalAlpha = 1;
}

function drawRaw(o: WaveOpts, total: number, map: (v: number) => number): boolean {
  const { ctx } = o;
  const fA = Math.max(0, Math.floor(o.frameAt(o.x0)));
  const fB = Math.min(total, Math.ceil(o.frameAt(o.x1)) + 1);
  if (fB <= fA) return true;
  const bA = Math.floor(fA / BLOCK);
  const bB = Math.floor((fB - 1) / BLOCK);
  const blocks: (Float32Array | null | undefined)[] = [];
  for (let b = bA; b <= bB; b++) blocks.push(rawBlock(o.id, b));
  if (blocks.some((b) => !b)) return false;
  const at = (f: number) => {
    const b = Math.floor(f / BLOCK);
    const blk = blocks[b - bA];
    return blk ? blk[f - b * BLOCK] ?? 0 : 0;
  };
  const yOf = (x: number, v: number) => {
    const a = map(Math.abs(v) * o.env(x));
    return o.y - Math.sign(v) * a * o.h;
  };
  ctx.strokeStyle = o.color;
  ctx.lineWidth = 1.2;
  ctx.beginPath();
  const x0f = o.frameAt(o.x0);
  for (let f = fA; f < fB; f++) {
    const x = o.x0 + (f - x0f) / o.fpp;
    const y = yOf(x, at(f));
    if (f === fA) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();
  if (o.fpp < 0.12) {
    ctx.fillStyle = o.color;
    for (let f = fA; f < fB; f++) {
      const x = o.x0 + (f - x0f) / o.fpp;
      ctx.beginPath();
      ctx.arc(x, yOf(x, at(f)), 2.2, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  return true;
}
