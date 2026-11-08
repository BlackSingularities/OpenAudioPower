export function fmtTime(frames: number, rate: number, ms = true): string {
  const s = Math.max(0, frames / Math.max(1, rate));
  const m = Math.floor(s / 60);
  const sec = s - m * 60;
  const h = Math.floor(m / 60);
  const mm = h ? String(m % 60).padStart(2, "0") : String(m);
  const ss = ms ? sec.toFixed(3).padStart(6, "0") : String(Math.floor(sec)).padStart(2, "0");
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function fmtBars(frames: number, rate: number, bpm: number, beats: number): string {
  const fpb = (rate * 60) / Math.max(1, bpm);
  const totalBeats = frames / fpb;
  const bar = Math.floor(totalBeats / beats) + 1;
  const beat = Math.floor(totalBeats % beats) + 1;
  const tick = Math.floor((totalBeats % 1) * 100);
  return `${bar}.${beat}.${String(tick).padStart(2, "0")}`;
}

export function fmtDuration(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function fmtDb(v: number, digits = 1): string {
  if (!isFinite(v) || v <= -119) return "−∞";
  return (v > 0 ? "+" : "") + v.toFixed(digits).replace("-", "−");
}

export function fmtBytes(n: number): string {
  if (n > 1e9) return (n / 1e9).toFixed(1) + " GB";
  if (n > 1e6) return (n / 1e6).toFixed(0) + " MB";
  return (n / 1e3).toFixed(0) + " kB";
}

export const toDb = (g: number) => (g <= 1e-6 ? -120 : 20 * Math.log10(g));
