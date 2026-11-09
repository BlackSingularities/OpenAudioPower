// Application state: project (tracks, clips, effects), sources, editor view, transport,
// undo history, settings and jobs. Every project mutation goes through `commit()`.

import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, type AppInfo, type PollState, type SourceInfo, type Peaks, type Bands } from "./api";
import { t, setLang, stemLabel, type Lang } from "./i18n.svelte";
import { newSpec, PRESETS } from "./effects";
import type { PluginDesc } from "./api";

// ───────────────────────────── types ─────────────────────────────

export interface Clip {
  id: number;
  source: number;
  start: number;
  offset: number;
  len: number;
  gain: number;
  fadeIn: number;
  fadeOut: number;
  muted: boolean;
  name: string;
}
export interface FxSlot {
  id: number;
  on: boolean;
  spec: any;
}
export interface AutoPoint {
  /** timeline frame */
  t: number;
  /** dB for volume, −1…1 for pan */
  v: number;
}
export type AutoLane = "volume" | "pan";
export interface Automation {
  /** which envelope is drawn and edited on the lane; automation plays back either way */
  show: "off" | AutoLane;
  volume: AutoPoint[];
  pan: AutoPoint[];
}
export interface Track {
  id: number;
  name: string;
  color: string;
  gain: number;
  pan: number;
  mute: boolean;
  solo: boolean;
  armed: boolean;
  height: number;
  effects: FxSlot[];
  clips: Clip[];
  /** missing in projects saved before automation existed */
  auto?: Automation;
  /** bus: no clips, sums what other tracks route or send to it */
  bus?: boolean;
  /** bus id this track feeds instead of the master (0 = master) */
  output?: number;
  sends?: SendSlot[];
  /** pre-rendered clips + effects played instead of them */
  freeze?: { source: number; start: number } | null;
  /** tracks sharing a group number move faders, mute and solo together (0 = none) */
  group?: number;
}
export interface SendSlot {
  id: number;
  target: number;
  gain: number;
  pre: boolean;
  on: boolean;
}
export const GROUP_COLORS = ["#f43f5e", "#f59e0b", "#10b981", "#3b82f6", "#a855f7", "#ec4899"];
export interface Marker {
  id: number;
  pos: number;
  name: string;
  /** end of a region label (Audacity-style); absent for a point label */
  end?: number;
}
export interface ProjectState {
  name: string;
  tracks: Track[];
  master: { gain: number; effects: FxSlot[] };
  bpm: number;
  beats: number;
  markers: Marker[];
  loop: { start: number; end: number; on: boolean };
}

export const TRACK_COLORS = ["#8b5cf6", "#3b82f6", "#06b6d4", "#10b981", "#84cc16", "#f59e0b", "#f97316", "#ef4444", "#ec4899", "#a855f7"];
export const STEM_COLORS: Record<string, string> = {
  vocals: "#f472b6",
  drums: "#f59e0b",
  bass: "#22d3ee",
  guitar: "#a3e635",
  piano: "#a78bfa",
  other: "#94a3b8",
};
export const ACCENTS = ["#8b5cf6", "#6366f1", "#3b82f6", "#06b6d4", "#10b981", "#f59e0b", "#f43f5e", "#ec4899"];

// ───────────────────────────── settings ─────────────────────────────

const defaultSettings = () => ({
  lang: "auto" as "auto" | Lang,
  accent: "#8b5cf6",
  glass: true,
  blur: 22,
  checkUpdates: true,
  skippedVersion: "",
  audio: { output: "", input: "" },
  metronome: { volume: 0.7, recOnly: false },
  snap: true,
  follow: true,
  /** record only inside the loop range, with pre-roll */
  punch: false,
  preroll: 2,
  wave: { scale: "relative" as "relative" | "db" | "linear", style: "fill" as "fill" | "rms" | "bars" | "outline", color: "track" as "track" | "spectral" | "gradient" },
  timeMode: "time" as "time" | "bars",
  ui: { library: true, inspector: true, dock: "mixer" as "mixer" | "analyzer" | null, dockHeight: 230, libraryWidth: 270, inspectorWidth: 330, onboarded: false },
  plugins: { dirs: [] as string[] },
  stems: { engine: "htdemucs_6s", quality: 1, useGpu: true, muteOriginal: true },
  export: { format: "wav", bits: 24, bitrate: 192, norm: "none", rate: 0, tail: 2 },
  recent: [] as string[],
});

export const S = $state({ settings: defaultSettings() });

let persistTimer: number | undefined;
export function persist() {
  clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => api.saveSettings(JSON.stringify($state.snapshot(S.settings))).catch(() => {}), 300);
}

function deepMerge(a: any, b: any): any {
  if (!b || typeof b !== "object" || Array.isArray(b)) return b ?? a;
  const out: any = Array.isArray(a) ? [...a] : { ...a };
  for (const k of Object.keys(b)) out[k] = a && typeof a[k] === "object" && !Array.isArray(a[k]) ? deepMerge(a[k], b[k]) : b[k];
  return out;
}

// ───────────────────────────── app state ─────────────────────────────

export interface Job {
  kind: string;
  fraction: number;
  label: string;
}
export interface Toast {
  id: number;
  text: string;
  kind: "ok" | "error" | "info";
  action?: { label: string; run: () => void };
}

export const app = $state({
  info: null as AppInfo | null,
  rate: 48000,
  sources: [] as SourceInfo[],
  job: null as Job | null,
  toasts: [] as Toast[],
  dialogs: { stems: false, export: false, batch: false, spectral: false, remix: false, plugins: false, settings: false, about: false, shortcuts: false, exit: false, rename: false, generate: false, labels: false, history: false, timer: false, duck: false, match: false },
  /** track that Auto Duck writes automation to */
  duckTarget: null as number | null,
  /** scheduled timer recording */
  timerRec: null as null | { at: number; seconds: number },
  stemSource: null as { id: number; start: number; end: number; trackId: number | null; clipStart: number } | null,
  /** effect chain the plug-in browser adds to: a track id, or "master" */
  pluginTarget: null as null | number | "master",
  /** plug-in instances whose helper process died */
  pluginCrashed: {} as Record<number, boolean>,
  /** plug-in instances with an open editor window */
  pluginEditor: {} as Record<number, boolean>,
  /** song the remix panel works on */
  remix: null as null | { clip: number; track: number; song: number; orig: Record<number, number>; semis: number; withVocals: boolean },
  /** clip opened in the spectral editor */
  spectralClip: null as number | null,
  exportMode: "mix" as "mix" | "loop" | "selection" | "tracks" | "labels",
  path: "" as string,
  dirty: false,
  dragOver: false,
  ready: false,
  noiseProfile: null as { source: number; start: number; end: number } | null,
});

export const tp = $state({
  pos: 0,
  playing: false,
  recording: false,
  recFrames: 0,
  recStart: 0,
  recRate: 48000,
  meters: {} as Record<number, { l: number; r: number; gr: number }>,
  master: [0, 0, 0, 0] as number[],
  masterGr: 0,
  clip: false,
  input: [0, 0] as number[],
  inputOpen: false,
  error: "",
  recPeaks: [] as number[],
  metronome: false,
});

export const ed = $state({
  fpp: 1200, // frames per pixel
  scroll: 0, // frames at left edge
  vscroll: 0,
  sel: null as { start: number; end: number } | null,
  selClips: [] as number[],
  selTrack: null as number | null,
  inspectorTab: "clip" as "clip" | "track" | "process" | "analysis" | "master",
  /** pencil draws samples when zoomed in far enough */
  tool: "select" as "select" | "pencil",
  viewWidth: 1000,
});

const emptyProject = (): ProjectState => ({
  name: "",
  tracks: [],
  master: { gain: 0, effects: [] },
  bpm: 120,
  beats: 4,
  markers: [],
  loop: { start: 0, end: 0, on: false },
});

export const P = $state({ p: emptyProject() });

let nextId = 1;
export const uid = () => nextId++;

function bumpIds(p: ProjectState) {
  let m = 0;
  const see = (x: number) => (m = Math.max(m, x));
  for (const tr of p.tracks) {
    see(tr.id);
    tr.effects.forEach((e) => see(e.id));
    tr.clips.forEach((c) => see(c.id));
  }
  p.master.effects.forEach((e) => see(e.id));
  p.markers.forEach((x) => see(x.id));
  nextId = Math.max(nextId, m + 1);
}

// ───────────────────────────── peaks cache ─────────────────────────────

export const peakCache = new Map<number, Peaks>();
const peakWaiters = new Map<number, Promise<Peaks>>();
export const peakVersion = $state({ v: 0 });

export function getPeaks(id: number): Peaks | undefined {
  const p = peakCache.get(id);
  if (p) return p;
  if (!peakWaiters.has(id)) {
    const pr = api.peaks(id).then((pk) => {
      peakCache.set(id, pk);
      peakWaiters.delete(id);
      peakVersion.v++;
      return pk;
    });
    pr.catch(() => peakWaiters.delete(id));
    peakWaiters.set(id, pr);
  }
  return undefined;
}

export const bandCache = new Map<number, Bands>();
const bandWaiters = new Set<number>();
/** Frequency-band overview (coloured / RMS waveforms), loaded lazily. */
export function getBands(id: number): Bands | undefined {
  const b = bandCache.get(id);
  if (b) return b;
  if (!bandWaiters.has(id)) {
    bandWaiters.add(id);
    api
      .bands(id)
      .then((x) => {
        bandCache.set(id, x);
        peakVersion.v++;
      })
      .catch(() => {})
      .finally(() => bandWaiters.delete(id));
  }
  return undefined;
}

export function source(id: number): SourceInfo | undefined {
  return app.sources.find((s) => s.id === id);
}

function addSources(list: SourceInfo[]) {
  for (const s of list) {
    if (!app.sources.some((x) => x.id === s.id)) app.sources.push(s);
  }
}

// ───────────────────────────── toasts & jobs ─────────────────────────────

let toastId = 1;
export function toast(text: string, kind: Toast["kind"] = "info", action?: Toast["action"], ms = 4200) {
  const id = toastId++;
  app.toasts.push({ id, text, kind, action });
  setTimeout(() => dismissToast(id), action ? ms * 2 : ms);
}
export function dismissToast(id: number) {
  const i = app.toasts.findIndex((x) => x.id === id);
  if (i >= 0) app.toasts.splice(i, 1);
}

async function job<T>(kind: string, f: () => Promise<T>): Promise<T | undefined> {
  app.job = { kind, fraction: 0, label: "" };
  try {
    return await f();
  } catch (e: any) {
    const msg = String(e?.message ?? e);
    if (msg === "cancelled") toast(t("cancelled"), "info");
    else toast(`${t("error")}: ${msg}`, "error");
    return undefined;
  } finally {
    app.job = null;
  }
}

export function cancelJob() {
  api.cancelJob();
}

// ───────────────────────────── history ─────────────────────────────

interface HistEntry {
  /** project before the action */
  json: string;
  label: string;
  at: number;
}
const undoStack: HistEntry[] = [];
const redoStack: HistEntry[] = [];
export const hist = $state({ canUndo: false, canRedo: false, /** labels for the history list */ done: [] as { label: string; at: number }[], undone: [] as { label: string; at: number }[] });

/** Name for the next undo step (menus set it just before running their action). */
let pendingLabel = "";
export function labelNext(label: string) {
  pendingLabel = label;
  setTimeout(() => (pendingLabel = ""), 0);
}
function pushUndo(json: string, label?: string) {
  undoStack.push({ json, label: label || pendingLabel || t("histEdit"), at: Date.now() });
  pendingLabel = "";
  if (undoStack.length > 200) undoStack.shift();
  redoStack.length = 0;
  histSync();
}
function histSync() {
  hist.canUndo = undoStack.length > 0;
  hist.canRedo = redoStack.length > 0;
  hist.done = undoStack.map((e) => ({ label: e.label, at: e.at }));
  hist.undone = redoStack.map((e) => ({ label: e.label, at: e.at })).reverse();
}

function snapshot(): string {
  return JSON.stringify($state.snapshot(P.p));
}

let syncQueued = false;
export function syncEngine() {
  if (syncQueued) return;
  syncQueued = true;
  queueMicrotask(() => {
    syncQueued = false;
    const p = $state.snapshot(P.p);
    api.setProject({ tracks: p.tracks, master: p.master }).catch(() => {});
    api.transport({ cmd: "loop", start: p.loop.start, end: p.loop.end, on: p.loop.on }).catch(() => {});
  });
}

/** Run a project mutation as one undoable step. */
export function commit(f: () => void, opts: { merge?: string; label?: string } = {}) {
  const before = snapshot();
  f();
  const now = Date.now();
  if (!(opts.merge && opts.merge === lastMerge.key && now - lastMerge.at < 1200)) pushUndo(before, opts.label);
  lastMerge.key = opts.merge ?? "";
  lastMerge.at = now;
  app.dirty = true;
  syncEngine();
}
const lastMerge = { key: "", at: 0 };

// Continuous gestures (dragging clips, faders) mutate the project live and become one undo step.
let gestureBefore: string | null = null;
let gestureLabel = "";
export function beginGesture(label: unknown = "") {
  gestureBefore = snapshot();
  gestureLabel = typeof label === "string" ? label : "";
}
export function endGesture(label: unknown = "") {
  if (gestureBefore !== null && gestureBefore !== snapshot()) {
    pushUndo(gestureBefore, (typeof label === "string" && label) || gestureLabel);
    app.dirty = true;
  }
  gestureBefore = null;
  lastMerge.key = "";
  syncEngine();
}

function restore(json: string) {
  P.p = JSON.parse(json);
  const ids = new Set(P.p.tracks.flatMap((tr) => tr.clips.map((c) => c.id)));
  ed.selClips = ed.selClips.filter((id) => ids.has(id));
  if (ed.selTrack && !P.p.tracks.some((tr) => tr.id === ed.selTrack)) ed.selTrack = null;
  app.dirty = true;
  syncEngine();
}

export function undo() {
  const s = undoStack.pop();
  if (!s) return;
  redoStack.push({ json: snapshot(), label: s.label, at: s.at });
  restore(s.json);
  histSync();
  lastMerge.key = "";
}
export function redo() {
  const s = redoStack.pop();
  if (!s) return;
  undoStack.push({ json: snapshot(), label: s.label, at: s.at });
  restore(s.json);
  histSync();
  lastMerge.key = "";
}
/** Jump in history: `done` actions stay applied (0 = before everything). */
export function historyGoto(done: number) {
  while (undoStack.length > done && undoStack.length) undo();
  while (undoStack.length < done && redoStack.length) redo();
}

// ───────────────────────────── helpers ─────────────────────────────

export function track(id: number | null): Track | undefined {
  return P.p.tracks.find((x) => x.id === id);
}
export function findClip(id: number): { track: Track; clip: Clip } | undefined {
  for (const tr of P.p.tracks) {
    const c = tr.clips.find((x) => x.id === id);
    if (c) return { track: tr, clip: c };
  }
  return undefined;
}
export function projectLength(): number {
  let m = 0;
  for (const tr of P.p.tracks) for (const c of tr.clips) m = Math.max(m, c.start + c.len);
  return m;
}
export const selectedClips = () => ed.selClips.map(findClip).filter(Boolean) as { track: Track; clip: Clip }[];

export function framesPerBeat() {
  return (app.rate * 60) / Math.max(20, P.p.bpm);
}

export function snapFrame(f: number, force = false): number {
  if (!S.settings.snap && !force) return Math.round(f);
  // grid density follows zoom: beats, half beats … or bars
  const fpb = framesPerBeat();
  const px = fpb / ed.fpp;
  let step = fpb;
  if (px < 6) step = fpb * P.p.beats;
  else if (px > 120) step = fpb / 4;
  else if (px > 50) step = fpb / 2;
  return Math.round(f / step) * step;
}

/** The track's automation, created on first use (call inside `commit`). */
export function autoOf(tr: Track): Automation {
  if (!tr.auto) tr.auto = { show: "off", volume: [], pan: [] };
  return tr.auto;
}

/** Display / default range of each envelope. */
export const AUTO_RANGE: Record<AutoLane, { min: number; max: number; def: (tr: Track) => number }> = {
  volume: { min: -36, max: 6, def: () => 0 },
  pan: { min: -1, max: 1, def: (tr) => tr.pan },
};

/** Envelope value at frame `t`, linear between points, held outside them. */
export function envelopeAt(points: AutoPoint[], t: number, def: number): number {
  if (!points.length) return def;
  if (t <= points[0].t) return points[0].v;
  const last = points[points.length - 1];
  if (t >= last.t) return last.v;
  let i = 1;
  while (points[i].t <= t) i++;
  const a = points[i - 1];
  const b = points[i];
  return a.v + ((b.v - a.v) * (t - a.t)) / Math.max(1, b.t - a.t);
}

export function newTrack(name?: string, color?: string, at?: number): Track {
  const tr: Track = {
    id: uid(),
    name: name ?? `${t("track")} ${P.p.tracks.length + 1}`,
    color: color ?? TRACK_COLORS[P.p.tracks.length % TRACK_COLORS.length],
    gain: 0,
    pan: 0,
    mute: false,
    solo: false,
    armed: false,
    height: 96,
    effects: [],
    clips: [],
    auto: { show: "off", volume: [], pan: [] },
  };
  if (at !== undefined) P.p.tracks.splice(at, 0, tr);
  else P.p.tracks.push(tr);
  return tr;
}

export function makeClip(src: SourceInfo, start: number): Clip {
  return { id: uid(), source: src.id, start: Math.max(0, Math.round(start)), offset: 0, len: src.frames, gain: 0, fadeIn: 0, fadeOut: 0, muted: false, name: src.name };
}

export function addTrackCmd() {
  commit(() => {
    const tr = newTrack();
    ed.selTrack = tr.id;
  }, { label: t("addTrack") });
}

export function removeTrack(id: number) {
  commit(() => {
    const i = P.p.tracks.findIndex((x) => x.id === id);
    if (i >= 0) P.p.tracks.splice(i, 1);
    if (ed.selTrack === id) ed.selTrack = null;
  }, { label: t("removeTrack") });
}

export function addSourceToNewTrack(src: SourceInfo, start = 0, color?: string, name?: string) {
  let clipId = 0;
  commit(() => {
    const tr = newTrack(name ?? src.name, color ?? (src.stem ? STEM_COLORS[src.stem] : undefined));
    const c = makeClip(src, start);
    clipId = c.id;
    tr.clips.push(c);
    ed.selTrack = tr.id;
    ed.selClips = [c.id];
  }, { label: `${t("addTrack")}: ${name ?? src.name}` });
  return clipId;
}

export function addSourceToTrack(src: SourceInfo, trackId: number, start: number) {
  commit(() => {
    const tr = track(trackId);
    if (!tr) return;
    const c = makeClip(src, start);
    tr.clips.push(c);
    ed.selTrack = tr.id;
    ed.selClips = [c.id];
  });
}

// ───────────────────────────── import / projects ─────────────────────────────

export async function importPaths(paths: string[], toTracks = true) {
  if (!paths.length) return;
  const proj = paths.find((p) => p.toLowerCase().endsWith(".oap"));
  if (proj) return openProject(proj);
  const r = await job("import", () => api.importFiles(paths));
  if (!r) return;
  addSources(r.sources);
  if (r.errors.length) toast(`${t("importErrors")}: ${r.errors.join(", ")}`, "error", undefined, 8000);
  if (r.sources.length) {
    if (toTracks) {
      commit(() => {
        for (const s of r.sources) {
          const tr = newTrack(s.name);
          tr.clips.push(makeClip(s, 0));
        }
      });
      if (P.p.tracks.length === r.sources.length) zoomFit();
    }
    toast(`${t("imported")}: ${r.sources.length}`, "ok");
  }
}

export async function pickImport() {
  const f = await openDialog({
    multiple: true,
    filters: [{ name: "Audio", extensions: app.info?.extensions ?? ["wav", "mp3", "flac"] }, { name: "OpenAudioPower", extensions: ["oap"] }],
  });
  if (!f) return;
  importPaths(Array.isArray(f) ? f : [f]);
}

export async function pickImportFolder() {
  const f = await openDialog({ directory: true });
  if (typeof f === "string") importPaths([f]);
}

function usedSources(): number[] {
  const ids = new Set<number>();
  const walk = (p: ProjectState) =>
    p.tracks.forEach((tr) => {
      tr.clips.forEach((c) => ids.add(c.source));
      if (tr.freeze) ids.add(tr.freeze.source);
    });
  walk(P.p);
  // keep library items too
  app.sources.forEach((s) => ids.add(s.id));
  return [...ids].filter((id) => app.sources.some((s) => s.id === id));
}

export async function saveProject(as = false): Promise<boolean> {
  let path = app.path;
  if (!path || as) {
    const p = await saveDialog({ defaultPath: (P.p.name || "Projekt") + ".oap", filters: [{ name: "OpenAudioPower", extensions: ["oap"] }] });
    if (!p) return false;
    path = p;
  }
  const name = path.split(/[\\/]/).pop()!.replace(/\.oap$/i, "");
  P.p.name = name;
  await collectPluginStates();
  const ok = await job("save", () => api.saveProject(path, $state.snapshot(P.p), usedSources()));
  if (ok === undefined) return false;
  app.path = path;
  app.dirty = false;
  remember(path);
  toast(t("saved"), "ok");
  return true;
}

function remember(path: string) {
  S.settings.recent = [path, ...S.settings.recent.filter((x) => x !== path)].slice(0, 8);
  persist();
}

export async function openProject(path?: string) {
  if (!path) {
    const f = await openDialog({ filters: [{ name: "OpenAudioPower", extensions: ["oap"] }] });
    if (typeof f !== "string") return;
    path = f;
  }
  stop();
  const r = await job("open", () => api.loadProject(path!));
  if (!r) return;
  peakCache.clear();
  app.sources = r.sources;
  const st: ProjectState = { ...emptyProject(), ...r.state };
  if (Math.abs(r.ratio - 1) > 1e-6) {
    const k = r.ratio;
    for (const tr of st.tracks) for (const c of tr.clips) {
      c.start = Math.round(c.start * k);
      c.offset = Math.round(c.offset * k);
      c.len = Math.round(c.len * k);
      c.fadeIn = Math.round(c.fadeIn * k);
      c.fadeOut = Math.round(c.fadeOut * k);
    }
    st.markers.forEach((m) => {
      m.pos = Math.round(m.pos * k);
      if (m.end !== undefined) m.end = Math.round(m.end * k);
    });
    for (const tr of st.tracks) for (const pt of [...(tr.auto?.volume ?? []), ...(tr.auto?.pan ?? [])]) pt.t = Math.round(pt.t * k);
    for (const tr of st.tracks) tr.freeze = null;
    st.loop.start = Math.round(st.loop.start * k);
    st.loop.end = Math.round(st.loop.end * k);
  }
  P.p = st;
  bumpIds(st);
  undoStack.length = 0;
  redoStack.length = 0;
  histSync();
  ed.selClips = [];
  ed.sel = null;
  ed.selTrack = st.tracks[0]?.id ?? null;
  app.path = path!;
  app.dirty = false;
  await startProjectPlugins();
  syncEngine();
  zoomFit();
  remember(path!);
  if (r.missing.length) toast(`${t("missingFiles")}: ${r.missing.join(", ")}`, "error", undefined, 9000);
  else toast(t("opened"), "ok");
}

export async function newProject() {
  stop();
  await api.clearSources();
  app.sources = [];
  peakCache.clear();
  P.p = emptyProject();
  api.pluginRetain([]).catch(() => {});
  app.pluginCrashed = {};
  app.pluginEditor = {};
  undoStack.length = 0;
  redoStack.length = 0;
  histSync();
  ed.selClips = [];
  ed.sel = null;
  ed.selTrack = null;
  ed.scroll = 0;
  app.path = "";
  app.dirty = false;
  syncEngine();
  seek(0);
}

// ───────────────────────────── transport ─────────────────────────────

export function play() {
  if (tp.playing) return pause();
  api.transport({ cmd: "play" });
  tp.playing = true;
}
export function pause() {
  if (tp.recording) return stopRecording();
  api.transport({ cmd: "pause" });
  tp.playing = false;
}
export function stop() {
  if (tp.recording) stopRecording();
  api.transport({ cmd: "pause" });
  tp.playing = false;
}
export function seek(pos: number) {
  pos = Math.max(0, Math.round(pos));
  api.transport({ cmd: "seek", pos });
  tp.pos = pos;
}
export function toStart() {
  seek(ed.sel ? ed.sel.start : 0);
  ed.scroll = Math.min(ed.scroll, tp.pos);
}
export function toEnd() {
  seek(projectLength());
}

export function setMetronome(on = tp.metronome) {
  tp.metronome = on;
  api.transport({ cmd: "metronome", on, bpm: P.p.bpm, beats: P.p.beats, volume: S.settings.metronome.volume, recOnly: S.settings.metronome.recOnly });
}

export function toggleLoop() {
  commit(() => {
    const l = P.p.loop;
    if (!l.on && l.end <= l.start) {
      if (ed.sel) {
        l.start = ed.sel.start;
        l.end = ed.sel.end;
      } else {
        const fpb = framesPerBeat() * P.p.beats;
        l.start = snapFrame(tp.pos, true);
        l.end = l.start + fpb * 4;
      }
    }
    l.on = !l.on;
  }, { merge: "loop" });
}

export async function startRecording() {
  if (tp.recording) return stopRecording();
  let tr = P.p.tracks.find((x) => x.armed) ?? track(ed.selTrack);
  if (!tr) {
    commit(() => {
      tr = newTrack(t("recording"), "#ef4444");
      tr.armed = true;
    });
  } else if (!tr.armed) {
    const id = tr.id;
    commit(() => {
      P.p.tracks.forEach((x) => (x.armed = x.id === id));
    });
  }
  tp.recPeaks = [];
  const punch = punchRange();
  if (punch) {
    // the loop would wrap the take; play straight through from the pre-roll instead
    api.transport({ cmd: "loop", start: P.p.loop.start, end: P.p.loop.end, on: false }).catch(() => {});
    seek(Math.max(0, punch.start - Math.round(S.settings.preroll * app.rate)));
  }
  recPunch = punch;
  await api.recordStart(tp.pos);
  tp.recording = true;
  tp.playing = true;
}

/** In/out points for punch recording (the loop range) when punch is on. */
export function punchRange(): { start: number; end: number } | null {
  const l = P.p.loop;
  return S.settings.punch && l.end > l.start ? { start: l.start, end: l.end } : null;
}
let recPunch: { start: number; end: number } | null = null;
/** Recording also stops on its own (punch out, timer). */
let recStopAt = 0;

/** Starts a recording at a given time for a given length (seconds; 0 = until stopped). */
export function scheduleRecording(at: number, seconds: number) {
  cancelTimerRecording();
  app.timerRec = { at, seconds };
  const wait = Math.max(0, at - Date.now());
  timerHandle = setTimeout(async () => {
    timerHandle = 0;
    await startRecording();
    recStopAt = seconds > 0 ? Date.now() + seconds * 1000 : 0;
    app.timerRec = null;
  }, wait);
}
let timerHandle: any = 0;
export function cancelTimerRecording() {
  if (timerHandle) clearTimeout(timerHandle);
  timerHandle = 0;
  app.timerRec = null;
}

export async function stopRecording() {
  if (!tp.recording) return;
  tp.recording = false;
  tp.playing = false;
  recStopAt = 0;
  const punch = recPunch;
  recPunch = null;
  if (punch) syncEngine();
  const tr = P.p.tracks.find((x) => x.armed);
  const n = P.p.tracks.reduce((a, x) => a + x.clips.filter((c) => source(c.source)?.kind === "recording").length, 0);
  const name = `${t("recording")} ${n + 1}`;
  const take = await api.recordStop(name);
  tp.recPeaks = [];
  if (!take) return;
  addSources([take.source]);
  commit(() => {
    const target = (tr && track(tr.id)) ?? newTrack(t("recording"), "#ef4444");
    const c = makeClip(take.source, take.start);
    if (punch) {
      // keep only what was recorded between the punch points, with short fades
      const a = Math.max(punch.start, c.start);
      const b = Math.min(punch.end, c.start + c.len);
      if (b > a) {
        c.offset += a - c.start;
        c.start = a;
        c.len = b - a;
        c.fadeIn = c.fadeOut = Math.min(Math.round(app.rate * 0.005), Math.floor(c.len / 2));
      }
    }
    target.clips.push(c);
    ed.selClips = [c.id];
    ed.selTrack = target.id;
  }, { label: t("recording") });
}

// ───────────────────────────── view ─────────────────────────────

export function zoomAt(factor: number, anchorFrame?: number) {
  const a = anchorFrame ?? tp.pos;
  const px = (a - ed.scroll) / ed.fpp;
  ed.fpp = Math.min(Math.max(ed.fpp * factor, 1), 200000);
  ed.scroll = Math.max(0, a - px * ed.fpp);
}

export function zoomFit() {
  const len = Math.max(projectLength(), app.rate * 10);
  ed.fpp = Math.max(1, (len * 1.04) / Math.max(200, ed.viewWidth));
  ed.scroll = 0;
}

// ───────────────────────────── editing ─────────────────────────────

let clipboard: { trackOffset: number; clip: Clip }[] = [];

export function splitAt(pos = tp.pos) {
  const targets = ed.selClips.length ? selectedClips() : P.p.tracks.flatMap((tr) => tr.clips.map((clip) => ({ track: tr, clip })));
  const hits = targets.filter(({ clip }) => pos > clip.start + 8 && pos < clip.start + clip.len - 8);
  if (!hits.length) return;
  commit(() => {
    const newSel: number[] = [];
    for (const h of hits) {
      const { track: tr, clip: c } = findClip(h.clip.id)!;
      const cut = Math.round(pos - c.start);
      const right: Clip = { ...c, id: uid(), start: c.start + cut, offset: c.offset + cut, len: c.len - cut, fadeIn: 0 };
      c.len = cut;
      c.fadeOut = 0;
      tr.clips.push(right);
      newSel.push(right.id);
    }
    ed.selClips = newSel;
  });
}

export function deleteSelected() {
  if (ed.selClips.length) {
    commit(() => {
      for (const tr of P.p.tracks) tr.clips = tr.clips.filter((c) => !ed.selClips.includes(c.id));
      ed.selClips = [];
    });
  } else if (ed.sel) deleteRange(false);
}

/** Removes the selected time range from clips (on the selected track, or all tracks); `ripple` closes the gap. */
export function deleteRange(ripple = true, silenceOnly = false) {
  const s = ed.sel;
  if (!s || s.end <= s.start) return;
  const scope = ed.selTrack ? P.p.tracks.filter((x) => x.id === ed.selTrack) : P.p.tracks;
  const ids = scope.map((x) => x.id);
  commit(() => {
    for (const tr of P.p.tracks) {
      if (!ids.includes(tr.id)) continue;
      const out: Clip[] = [];
      for (const c of tr.clips) {
        const a = c.start;
        const b = c.start + c.len;
        if (b <= s.start || a >= s.end) {
          if (ripple && !silenceOnly && a >= s.end) c.start -= s.end - s.start;
          out.push(c);
          continue;
        }
        if (a < s.start) out.push({ ...c, len: s.start - a, fadeOut: 0 });
        if (b > s.end) {
          const cut = s.end - a;
          out.push({ ...c, id: uid(), start: ripple && !silenceOnly ? s.start : s.end, offset: c.offset + cut, len: b - s.end, fadeIn: 0 });
        }
      }
      tr.clips = out;
    }
    ed.selClips = [];
    if (ripple && !silenceOnly) ed.sel = null;
  });
}

export function cropToRange() {
  const s = ed.sel;
  if (!s) return;
  commit(() => {
    for (const tr of P.p.tracks) {
      if (ed.selTrack && tr.id !== ed.selTrack) continue;
      tr.clips = tr.clips
        .filter((c) => c.start < s.end && c.start + c.len > s.start)
        .map((c) => {
          const a = Math.max(c.start, s.start);
          const b = Math.min(c.start + c.len, s.end);
          return { ...c, start: a, offset: c.offset + (a - c.start), len: b - a };
        });
    }
  });
}

export function copySelected(cut = false) {
  const sel = selectedClips();
  if (!sel.length) return;
  const minStart = Math.min(...sel.map((x) => x.clip.start));
  const ti = (tr: Track) => P.p.tracks.indexOf(tr);
  const base = Math.min(...sel.map((x) => ti(x.track)));
  clipboard = sel.map((x) => ({ trackOffset: ti(x.track) - base, clip: { ...$state.snapshot(x.clip), start: x.clip.start - minStart } }));
  if (cut) deleteSelected();
}

export function paste() {
  if (!clipboard.length) return;
  commit(() => {
    let baseIdx = Math.max(0, P.p.tracks.findIndex((x) => x.id === ed.selTrack));
    if (!P.p.tracks.length) newTrack();
    const ids: number[] = [];
    for (const it of clipboard) {
      let idx = baseIdx + it.trackOffset;
      while (idx >= P.p.tracks.length) newTrack();
      const c = { ...it.clip, id: uid(), start: Math.round(tp.pos + it.clip.start) };
      P.p.tracks[idx].clips.push(c);
      ids.push(c.id);
    }
    ed.selClips = ids;
  });
}

export function duplicateSelected() {
  const sel = selectedClips();
  if (!sel.length) return;
  const end = Math.max(...sel.map((x) => x.clip.start + x.clip.len));
  const start = Math.min(...sel.map((x) => x.clip.start));
  commit(() => {
    const ids: number[] = [];
    for (const { track: tr, clip } of sel) {
      const c = { ...$state.snapshot(clip), id: uid(), start: clip.start + (end - start) };
      track(tr.id)!.clips.push(c);
      ids.push(c.id);
    }
    ed.selClips = ids;
  });
}

export function selectAll() {
  ed.selClips = P.p.tracks.flatMap((tr) => tr.clips.map((c) => c.id));
}

export function addMarker() {
  commit(() => {
    P.p.markers.push({ id: uid(), pos: tp.pos, name: `${P.p.markers.length + 1}` });
    P.p.markers.sort((a, b) => a.pos - b.pos);
  }, { label: t("addMarker") });
}

/** Region label over the selection (or a point label at the playhead). */
export function addLabel(name?: string) {
  commit(() => {
    const n = name ?? `${t("label")} ${P.p.markers.length + 1}`;
    if (ed.sel && ed.sel.end > ed.sel.start) P.p.markers.push({ id: uid(), pos: ed.sel.start, end: ed.sel.end, name: n });
    else P.p.markers.push({ id: uid(), pos: tp.pos, name: n });
    P.p.markers.sort((a, b) => a.pos - b.pos);
  }, { label: t("addLabel") });
  app.dialogs.labels = true;
}

/** Parts for "export multiple": region labels as they are, otherwise the spans between point labels. */
export function labelRegions(): { name: string; start: number; end: number }[] {
  const ms = [...P.p.markers].sort((a, b) => a.pos - b.pos);
  const regions = ms.filter((m) => m.end !== undefined && m.end > m.pos).map((m) => ({ name: m.name, start: m.pos, end: m.end! }));
  if (regions.length) return regions;
  const end = projectLength();
  const cuts = ms.filter((m) => m.pos > 0 && m.pos < end);
  if (!cuts.length) return [];
  const pts = [0, ...cuts.map((m) => m.pos), end];
  return pts.slice(0, -1).map((a, i) => ({ name: i === 0 ? (P.p.name || t("label")) + " 1" : cuts[i - 1].name, start: a, end: pts[i + 1] }));
}

/** Audacity label file: start, end (seconds) and text, tab-separated. */
export function labelsToText(): string {
  const r = app.rate;
  return P.p.markers.map((m) => `${(m.pos / r).toFixed(6)}\t${((m.end ?? m.pos) / r).toFixed(6)}\t${m.name}`).join("\n") + "\n";
}
export function labelsFromText(text: string): number {
  const r = app.rate;
  const add: Marker[] = [];
  for (const line of text.split(/\r?\n/)) {
    // lines starting with a backslash carry spectral ranges in Audacity files
    if (!line.trim() || line.startsWith("\\")) continue;
    const [a, b, ...rest] = line.split("\t");
    const s = parseFloat(a);
    const e = parseFloat(b);
    if (!isFinite(s)) continue;
    const m: Marker = { id: uid(), pos: Math.round(s * r), name: rest.join("\t").trim() || `${add.length + 1}` };
    if (isFinite(e) && e > s) m.end = Math.round(e * r);
    add.push(m);
  }
  if (add.length)
    commit(() => {
      P.p.markers.push(...add);
      P.p.markers.sort((x, y) => x.pos - y.pos);
    }, { label: t("labelsImport") });
  return add.length;
}

// ───────────────────────────── auto duck ─────────────────────────────

export function openAutoDuck(target: number | null = ed.selTrack) {
  if (!target) return toast(t("duckPickTrack"), "info");
  app.duckTarget = target;
  app.dialogs.duck = true;
}

/** Writes a volume envelope on `target` that dips while `control` plays (in the selection or everywhere). */
export async function autoDuck(target: number, control: number, o: { threshold: number; duck: number; fall: number; rise: number; hold: number }, useRange: boolean) {
  const p = $state.snapshot(P.p);
  const range = useRange && ed.sel ? ed.sel : { start: 0, end: projectLength() };
  const pts = await job("duck", () => api.autoDuck({ tracks: p.tracks, master: p.master }, control, range.start, range.end, o.threshold, o.duck, o.fall, o.rise, o.hold));
  if (!pts) return 0;
  commit(() => {
    const a = autoOf(track(target)!);
    // keep the envelope outside the range, replace it inside
    const keep = a.volume.filter((x) => x.t < range.start || x.t > range.end);
    const edge = useRange && ed.sel ? [{ t: range.start, v: 0 }, ...pts, { t: range.end, v: 0 }] : pts;
    a.volume = [...keep, ...edge].sort((x, y) => x.t - y.t).filter((x, i, arr) => i === 0 || x.t !== arr[i - 1].t);
    a.show = "volume";
  }, { label: "Auto Duck" });
  return pts.length / 4;
}

// ───────────────────────────── generators ─────────────────────────────

/** Generates a signal and places it at the playhead: on the selected track if it is free there, otherwise on a new track. */
export async function insertGenerated(spec: any, seconds: number, stereo: boolean, name: string) {
  const src = await api.generateSource(spec, seconds, stereo, name).catch((e) => (toast(`${t("error")}: ${e}`, "error"), null));
  if (!src) return;
  addSources([src]);
  const at = ed.sel ? ed.sel.start : tp.pos;
  const tr = track(ed.selTrack);
  const free = tr && !tr.bus && !tr.freeze && !tr.clips.some((c) => c.start < at + src.frames && c.start + c.len > at);
  commit(() => {
    const target = free ? track(tr!.id)! : newTrack(name, "#22d3ee");
    const c = makeClip(src, at);
    target.clips.push(c);
    ed.selClips = [c.id];
    ed.selTrack = target.id;
  }, { label: t("generate") + ": " + name });
}

// ───────────────────────────── pencil ─────────────────────────────

/** Writes drawn samples into a clip's source (new version of the source, undoable). */
export async function drawSamples(clipId: number, srcStart: number, values: Float32Array) {
  labelNext(t("pencil"));
  await applyOpToClip(clipId, srcStart, srcStart + values.length, { op: "draw", values: Array.from(values) });
}

/** Clips that the Process tab acts on, with the source range for each. */
export function processTargets(useRange: boolean) {
  const out: { track: Track; clip: Clip; a: number; b: number }[] = [];
  const s = useRange ? ed.sel : null;
  let list = selectedClips();
  if (!list.length && s) {
    list = P.p.tracks
      .filter((tr) => !ed.selTrack || tr.id === ed.selTrack)
      .flatMap((tr) => tr.clips.filter((c) => c.start < s.end && c.start + c.len > s.start).map((clip) => ({ track: tr, clip })));
  }
  for (const { track: tr, clip: c } of list) {
    let a = c.offset;
    let b = c.offset + c.len;
    if (s) {
      a = c.offset + Math.max(0, s.start - c.start);
      b = c.offset + Math.min(c.len, s.end - c.start);
      if (b <= a) continue;
    }
    out.push({ track: tr, clip: c, a, b });
  }
  return out;
}

/** Applies a destructive operation; clips are pointed at the new sources (undoable). */
export async function applyOp(op: any, useRange: boolean, changesLength = false, whole = false) {
  const targets = processTargets(useRange && !whole);
  if (!targets.length) return toast(t("noSelection"), "info");
  const done: { clipId: number; src: SourceInfo; a: number; b: number; oldFrames: number }[] = [];
  const ok = await job("process", async () => {
    // one operation per distinct (source, range)
    const cache = new Map<string, SourceInfo>();
    for (const tg of targets) {
      const s = source(tg.clip.source)!;
      const a = whole ? 0 : tg.a;
      const b = whole ? s.frames : tg.b;
      const key = `${s.id}:${a}:${b}`;
      let res = cache.get(key);
      if (!res) {
        res = await api.applyOp(s.id, a, b, op);
        cache.set(key, res);
      }
      done.push({ clipId: tg.clip.id, src: res, a, b, oldFrames: s.frames });
    }
    return true;
  });
  if (!ok) return;
  addSources(done.map((d) => d.src));
  commit(() => {
    for (const d of done) {
      const f = findClip(d.clipId);
      if (!f) continue;
      const c = f.clip;
      c.source = d.src.id;
      if (whole) {
        if (changesLength || d.src.frames !== d.oldFrames) {
          const k = d.src.frames / Math.max(1, d.oldFrames);
          c.offset = Math.round(c.offset * k);
          c.len = Math.min(Math.round(c.len * k), d.src.frames - c.offset);
        }
      } else {
        const delta = d.src.frames - d.oldFrames;
        if (delta !== 0) c.len = Math.max(1, Math.min(c.len + delta, d.src.frames - c.offset));
      }
    }
  });
}

/** Runs a length-preserving operation on source frames `[a, b)` of one clip and points the clip
 * at the result (spectral editor). Returns the new source, or null when cancelled / failed. */
/** Opens the spectral editor on the selected clip (or the first clip under the playhead). */
export function openSpectral(clipId?: number) {
  const id = clipId ?? ed.selClips[0] ?? P.p.tracks.flatMap((t) => t.clips).find((c) => c.start <= tp.pos && c.start + c.len > tp.pos)?.id;
  if (id == null) return toast(t("noSelection"), "info");
  app.spectralClip = id;
  app.dialogs.spectral = true;
}

export async function applyOpToClip(clipId: number, a: number, b: number, op: any): Promise<SourceInfo | null> {
  const f = findClip(clipId);
  if (!f) return null;
  const s = source(f.clip.source);
  if (!s) return null;
  const res = await job("process", () => api.applyOp(s.id, a, b, op));
  if (!res) return null;
  addSources([res]);
  commit(() => {
    const c = findClip(clipId)?.clip;
    if (c) c.source = res.id;
  });
  return res;
}

export async function bounceRange() {
  const s = ed.sel ?? (P.p.loop.on ? { start: P.p.loop.start, end: P.p.loop.end } : { start: 0, end: projectLength() });
  if (s.end <= s.start) return;
  const p = $state.snapshot(P.p);
  const src = await job("bounce", () => api.bounce({ tracks: p.tracks, master: p.master }, s.start, s.end, `${t("kind_bounce")} ${new Date().toLocaleTimeString()}`));
  if (!src) return;
  addSources([src]);
  addSourceToNewTrack(src, s.start, "#64748b");
}

// ───────────────────────────── effects ─────────────────────────────

export function addEffect(target: Track | null, type: string) {
  commit(() => {
    const slot: FxSlot = { id: uid(), on: true, spec: newSpec(type) };
    if (target) track(target.id)!.effects.push(slot);
    else P.p.master.effects.push(slot);
  });
}

export function applyPreset(target: Track | null, presetId: string) {
  const pr = PRESETS.find((x) => x.id === presetId);
  if (!pr) return;
  commit(() => {
    const slots = pr.chain().map((spec) => ({ id: uid(), on: true, spec }));
    if (target) track(target.id)!.effects = slots;
    else P.p.master.effects = slots;
  });
}

// ───────────────────────────── stems ─────────────────────────────

export function openStemDialog(srcId?: number) {
  if (srcId !== undefined) {
    const s = source(srcId);
    if (s) app.stemSource = { id: s.id, start: 0, end: s.frames, trackId: null, clipStart: 0 };
  } else {
    const sel = selectedClips()[0];
    if (sel) app.stemSource = { id: sel.clip.source, start: sel.clip.offset, end: sel.clip.offset + sel.clip.len, trackId: sel.track.id, clipStart: sel.clip.start };
    else if (!app.stemSource) {
      const first = app.sources.find((s) => s.kind !== "stem");
      if (first) app.stemSource = { id: first.id, start: 0, end: first.frames, trackId: null, clipStart: 0 };
    }
  }
  app.dialogs.stems = true;
}

/** Pick a song, import it onto a new track and open the separation dialog for it right away. */
export async function importAndSeparate() {
  const f = await openDialog({ multiple: false, filters: [{ name: "Audio", extensions: app.info?.extensions ?? ["wav", "mp3", "flac"] }] });
  if (typeof f !== "string") return;
  const before = new Set(app.sources.map((s) => s.id));
  await importPaths([f]);
  const s = app.sources.find((x) => !before.has(x.id));
  if (!s) return;
  const clip = P.p.tracks.flatMap((tr) => tr.clips).find((c) => c.source === s.id);
  if (clip) {
    ed.selClips = [clip.id];
    openStemDialog();
  } else openStemDialog(s.id);
}

/** "Separate" entry point: with nothing loaded it starts with picking a file. */
export function separateCmd() {
  if (!app.sources.some((s) => s.kind !== "stem")) importAndSeparate();
  else openStemDialog();
}

// ───────────────────────────── VST3 / CLAP plug-ins ─────────────────────────────

/** Every plug-in slot in the project (tracks and master). */
export function pluginSlots(p: ProjectState = P.p): FxSlot[] {
  return [...p.tracks.flatMap((tr) => tr.effects), ...p.master.effects].filter((s) => s.spec?.type === "plugin");
}

/** Starts the plug-in in its own process and adds it to the chain, then opens its window. */
export async function addPlugin(target: number | "master", desc: PluginDesc) {
  const id = uid();
  const r = await job("plugin", () => api.pluginCreate(id, desc, ""));
  if (!r) return;
  commit(() => {
    const slot: FxSlot = { id, on: true, spec: { type: "plugin", instance: id, name: r.name || desc.name, format: desc.format, path: desc.path, id: desc.id, vendor: desc.vendor, state: "" } };
    if (target === "master") P.p.master.effects.push(slot);
    else track(target)?.effects.push(slot);
  });
  delete app.pluginCrashed[id];
  if (r.hasEditor) openPluginEditor(id);
}

export async function openPluginEditor(instance: number) {
  try {
    await api.pluginEditor(instance, true);
    app.pluginEditor[instance] = true;
  } catch (e) {
    toast(`${t("error")}: ${e}`, "error");
  }
}

export async function restartPlugin(instance: number) {
  const r = await job("plugin", () => api.pluginRestart(instance));
  if (!r) return;
  delete app.pluginCrashed[instance];
  toast(t("pluginRestarted"), "ok");
}

/** Writes each running plug-in's state into its slot (before saving the project). */
async function collectPluginStates() {
  const slots = pluginSlots();
  if (!slots.length) return;
  const states = await api.pluginStates(slots.map((s) => s.spec.instance)).catch(() => ({}) as Record<string, string>);
  for (const s of slots) {
    const v = states[String(s.spec.instance)];
    if (v) s.spec.state = v;
  }
}

/** Starts the plug-ins of a freshly opened project; ends the ones no longer used. */
async function startProjectPlugins() {
  const slots = pluginSlots();
  await api.pluginRetain(slots.map((s) => s.spec.instance)).catch(() => {});
  app.pluginCrashed = {};
  app.pluginEditor = {};
  const failed: string[] = [];
  for (const s of slots) {
    const d = s.spec;
    try {
      await api.pluginCreate(d.instance, { format: d.format, path: d.path, id: d.id, name: d.name, vendor: d.vendor ?? "", version: "", category: "", instrument: false }, d.state ?? "");
    } catch {
      failed.push(d.name);
      app.pluginCrashed[d.instance] = true;
    }
  }
  if (failed.length) toast(`${t("pluginsMissing")}: ${failed.join(", ")}`, "error", undefined, 9000);
}

// ───────────────────────────── remix ─────────────────────────────

/** The original file behind a source (edits point at it through `parent`). */
function songRoot(id: number): number {
  const s = source(id);
  return s && s.kind !== "stem" && s.parent != null ? s.parent : id;
}

/** Tracks holding stems separated from `songId` (or from any edit of it). */
export function stemTracksOf(songId: number): Track[] {
  const root = songRoot(songId);
  return P.p.tracks.filter((tr) =>
    tr.clips.some((c) => {
      const cs = source(c.source);
      return !!(cs?.stem && cs.parent != null && songRoot(cs.parent) === root);
    }),
  );
}

const stemOf = (tr: Track) => source(tr.clips[0]?.source ?? -1)?.stem ?? "";

/** Opens the remix panel for the selected song clip (a selected stem leads back to its song). */
export function openRemix(): boolean {
  let sel = selectedClips()[0];
  if (sel) {
    const s = source(sel.clip.source);
    if (s?.stem && s.parent != null) {
      const root = songRoot(s.parent);
      const songClip = P.p.tracks.flatMap((tr) => tr.clips.map((c) => ({ clip: c, track: tr }))).find((x) => songRoot(x.clip.source) === root && !source(x.clip.source)?.stem);
      if (songClip) sel = songClip;
    }
  } else {
    sel = P.p.tracks.flatMap((tr) => tr.clips.map((c) => ({ clip: c, track: tr }))).find((x) => !source(x.clip.source)?.stem)!;
  }
  if (!sel) {
    toast(t("noSelection"), "info");
    return false;
  }
  if (app.remix?.clip !== sel.clip.id) app.remix = { clip: sel.clip.id, track: sel.track.id, song: sel.clip.source, orig: {}, semis: 0, withVocals: false };
  app.dialogs.remix = true;
  return true;
}

/** Separates the remix song with the engine chosen in the separation settings (all stems). */
export async function separateForRemix(): Promise<boolean> {
  const r = app.remix;
  const f = r ? findClip(r.clip) : null;
  if (!r || !f) return false;
  const engine = S.settings.stems.engine;
  if (engine !== "dsp") {
    const models = await api.models().catch(() => []);
    if (!models.find((m) => m.id === engine && m.installed)) {
      // the model has to be downloaded first – the separation dialog does that
      ed.selClips = [f.clip.id];
      openStemDialog();
      return false;
    }
  }
  const c = f.clip;
  const res = await job("separate", () => api.separate(c.source, c.offset, c.offset + c.len, { engine, gpu: S.settings.stems.useGpu, quality: S.settings.stems.quality }, []));
  if (!res) return false;
  app.sources.push(...res.stems);
  addStemTracks(res.stems, c.start, f.track.id);
  return true;
}

/** Karaoke / a cappella / everything: mutes stems by kind; the original song track stays muted. */
export function remixPreset(kind: "karaoke" | "acappella" | "all") {
  const r = app.remix;
  if (!r) return;
  commit(() => {
    for (const tr of stemTracksOf(r.song)) {
      const v = stemOf(tr) === "vocals";
      tr.mute = kind === "karaoke" ? v : kind === "acappella" ? !v : false;
      tr.solo = false;
      tr.gain = 0;
    }
    const o = track(r.track);
    if (o) o.mute = true;
  });
}

/** One click: separate if needed, then mute the vocals. */
export async function karaoke() {
  if (!openRemix()) return;
  if (!stemTracksOf(app.remix!.song).length && !(await separateForRemix())) return;
  remixPreset("karaoke");
  toast(t("karaokeReady"), "ok");
}

/** Transposes the backing stems (optionally also the vocals) by `semis`, always from the
 * original stems so repeated changes do not pile up. */
export async function remixTranspose(semis: number, withVocals: boolean) {
  const r = app.remix;
  if (!r) return;
  const tracks = stemTracksOf(r.song);
  for (const tr of tracks) if (r.orig[tr.id] == null && tr.clips[0]) r.orig[tr.id] = tr.clips[0].source;
  const work = tracks.filter((tr) => tr.clips[0] && (withVocals || stemOf(tr) !== "vocals"));
  const out = new Map<number, number>();
  const ok = await job("process", async () => {
    let i = 0;
    for (const tr of work) {
      const orig = r.orig[tr.id];
      if (semis === 0) out.set(tr.id, orig);
      else {
        const res = await api.applyOp(orig, 0, 0, { op: "pitch", semitones: semis });
        addSources([res]);
        out.set(tr.id, res.id);
      }
      app.job = { kind: "process", fraction: ++i / work.length, label: tr.name };
    }
    return true;
  });
  if (!ok) return;
  commit(() => {
    for (const tr of tracks) {
      const x = track(tr.id);
      const id = out.get(tr.id) ?? (withVocals ? undefined : r.orig[tr.id]);
      if (x?.clips[0] && id != null) x.clips[0].source = id;
    }
  });
  r.semis = semis;
  r.withVocals = withVocals;
}

/** Mixes the song's span down to a new track (the remix as one file). */
export async function bounceRemix() {
  const f = app.remix ? findClip(app.remix.clip) : null;
  if (!f) return;
  const { start, len } = f.clip;
  const p = $state.snapshot(P.p);
  const src = await job("bounce", () => api.bounce({ tracks: p.tracks, master: p.master }, start, start + len, `${t("remixTitle")} ${new Date().toLocaleTimeString()}`));
  if (!src) return;
  addSources([src]);
  addSourceToNewTrack(src, start, "#64748b");
}

// ───────────────────────────── quick mute / solo ─────────────────────────────

/** Tracks that play the given source (or later edits of the same stem). */
export function tracksOfSource(id: number): Track[] {
  const s = source(id);
  return P.p.tracks.filter((tr) =>
    tr.clips.some((c) => {
      if (c.source === id) return true;
      const cs = source(c.source);
      return !!(s?.stem && cs && cs.stem === s.stem && (cs.parent === s.parent || cs.parent === s.id));
    }),
  );
}

/** The ids plus every track sharing a group with one of them. */
export function withGroups(ids: number[]): number[] {
  const groups = new Set(P.p.tracks.filter((x) => ids.includes(x.id) && x.group).map((x) => x.group));
  return P.p.tracks.filter((x) => ids.includes(x.id) || (x.group && groups.has(x.group))).map((x) => x.id);
}

/** Moves a fader; grouped tracks follow by the same amount of dB. Call inside a gesture or commit. */
export function setGainLinked(id: number, v: number) {
  const tr = track(id);
  if (!tr) return;
  const d = v - tr.gain;
  for (const x of P.p.tracks) if (x.id === id || (tr.group && x.group === tr.group)) x.gain = Math.max(-60, Math.min(12, x.id === id ? v : x.gain + d));
  syncEngine();
}

export function buses(): Track[] {
  return P.p.tracks.filter((x) => x.bus);
}

export function addBusCmd(name?: string): number {
  let id = 0;
  commit(() => {
    const tr = newTrack(name ?? `${t("bus")} ${buses().length + 1}`, "#64748b");
    tr.bus = true;
    tr.height = 64;
    id = tr.id;
    ed.selTrack = tr.id;
  }, { label: t("addBus") });
  return id;
}

/** Would routing `from` into bus `to` close a loop? */
export function routesInto(from: number, to: number): boolean {
  const seen = new Set<number>();
  const walk = (id: number): boolean => {
    if (id === from) return true;
    if (seen.has(id)) return false;
    seen.add(id);
    const tr = track(id);
    if (!tr) return false;
    return (!!tr.output && walk(tr.output)) || (tr.sends ?? []).some((s) => s.on && walk(s.target));
  };
  return walk(to);
}

export function setOutput(id: number, bus: number) {
  if (bus && routesInto(id, bus)) return toast(t("routeLoop"), "error");
  commit(() => (track(id)!.output = bus), { label: `${t("output")}: ${bus ? track(bus)?.name : t("master")}` });
}

export function addSend(id: number, bus: number) {
  if (routesInto(id, bus)) return toast(t("routeLoop"), "error");
  commit(() => {
    const tr = track(id)!;
    (tr.sends ??= []).push({ id: uid(), target: bus, gain: -6, pre: false, on: true });
  }, { label: `${t("send")}: ${track(bus)?.name ?? ""}` });
}

export function removeSend(id: number, sendId: number) {
  commit(() => {
    const tr = track(id)!;
    tr.sends = (tr.sends ?? []).filter((s) => s.id !== sendId);
  }, { label: t("remove") + ": " + t("send").toLowerCase() });
}

/** New bus with a reverb, fed by a send from each given track. */
export function addReverbSend(ids: number[]) {
  let bus = 0;
  commit(() => {
    const n = buses().filter((b) => b.name.startsWith(t("reverb"))).length;
    const tr = newTrack(n ? `${t("reverb")} ${n + 1}` : t("reverb"), "#64748b");
    tr.bus = true;
    tr.height = 64;
    tr.effects.push({ id: uid(), on: true, spec: { type: "reverb", room: 0.7, damping: 0.45, wet: 1, dry: 0, width: 1, predelay: 20, lowcut: 200 } });
    bus = tr.id;
    for (const id of ids) {
      const x = track(id);
      if (x && !x.bus) (x.sends ??= []).push({ id: uid(), target: bus, gain: -12, pre: false, on: true });
    }
  }, { label: t("reverbSend") });
}

export function setGroup(ids: number[], group: number) {
  commit(() => P.p.tracks.forEach((x) => ids.includes(x.id) && (x.group = group)));
}

export function nextGroup(): number {
  const used = new Set(P.p.tracks.map((x) => x.group ?? 0));
  let g = 1;
  while (used.has(g)) g++;
  return g;
}

/** Renders a track with its effects to a new source and plays that instead (saves CPU). */
export async function freezeTrack(id: number) {
  const tr = track(id);
  if (!tr || tr.bus || !tr.clips.length) return;
  const end = Math.max(...tr.clips.map((c) => c.start + c.len));
  const solo = { ...$state.snapshot(tr), gain: 0, pan: 0, mute: false, solo: true, output: 0, sends: [], freeze: null, auto: { show: "off", volume: [], pan: [] } };
  const keys = new Set<number>(tr.effects.filter((e) => e.on && e.spec.sidechain).map((e) => e.spec.sidechain));
  // sidechain sources must exist in the render, but stay silent
  const others = P.p.tracks.filter((x) => keys.has(x.id)).map((x) => ({ ...$state.snapshot(x), mute: true, solo: false, sends: [], output: 0 }));
  const tail = tr.effects.some((e) => e.on) ? 4 : 0;
  const src = await job("freeze", () => api.bounce({ tracks: [solo, ...others], master: { gain: 0, effects: [] } }, 0, end, `${tr.name} (${t("frozen").toLowerCase()})`, tail));
  if (!src) return;
  addSources([src]);
  commit(() => (track(id)!.freeze = { source: src.id, start: 0 }), { label: t("freeze") });
  toast(t("frozenOk"), "ok");
}

export function unfreezeTrack(id: number) {
  commit(() => (track(id)!.freeze = null), { label: t("unfreeze") });
}

export function toggleMute(ids: number[]) {
  ids = withGroups(ids);
  if (!ids.length) return;
  commit(() => {
    const ts = P.p.tracks.filter((x) => ids.includes(x.id));
    const v = !ts.every((x) => x.mute);
    ts.forEach((x) => (x.mute = v));
  });
}

/** Solo: only these tracks are heard. `add` keeps other soloed tracks; clicking an exclusive solo again clears it. */
export function soloOnly(ids: number[], add = false) {
  ids = withGroups(ids);
  if (!ids.length) return;
  commit(() => {
    const already = P.p.tracks.filter((x) => x.solo).map((x) => x.id);
    const same = already.length === ids.length && ids.every((i) => already.includes(i));
    if (add) {
      const on = ids.every((i) => already.includes(i));
      P.p.tracks.forEach((x) => ids.includes(x.id) && (x.solo = !on));
    } else if (same) P.p.tracks.forEach((x) => (x.solo = false));
    else
      P.p.tracks.forEach((x) => {
        x.solo = ids.includes(x.id);
        if (x.solo) x.mute = false;
      });
  });
}

export function addStemTracks(stems: SourceInfo[], at: number, originalTrack: number | null) {
  commit(() => {
    let idx = originalTrack ? P.p.tracks.findIndex((x) => x.id === originalTrack) + 1 : P.p.tracks.length;
    const order = ["vocals", "drums", "bass", "guitar", "piano", "other"];
    const sorted = [...stems].sort((a, b) => order.indexOf(a.stem ?? "") - order.indexOf(b.stem ?? ""));
    for (const s of sorted) {
      const tr = newTrack(stemLabel(s.stem) || s.name, STEM_COLORS[s.stem ?? ""] ?? undefined, idx++);
      tr.clips.push({ ...makeClip(s, at), name: s.name });
    }
    if (originalTrack && S.settings.stems.muteOriginal) {
      const o = track(originalTrack);
      if (o) o.mute = true;
    }
  });
}

// ───────────────────────────── polling & init ─────────────────────────────

let polling = false;
async function pollLoop() {
  if (polling) return;
  polling = true;
  const tick = async () => {
    try {
      const r: PollState = await api.poll();
      if (!draggingPlayhead) tp.pos = r.pos;
      tp.playing = r.playing;
      tp.recFrames = r.recFrames;
      tp.recStart = r.recStart;
      tp.recRate = r.recRate;
      const m: Record<number, { l: number; r: number; gr: number }> = {};
      for (const x of r.meters) m[x.id] = { l: x.l, r: x.r, gr: x.gr };
      if (r.meters.length || !r.playing) tp.meters = m;
      tp.master = r.master;
      tp.masterGr = r.masterGr;
      if (r.clip) tp.clip = true;
      tp.input = r.input;
      tp.inputOpen = r.inputOpen;
      tp.error = r.error;
      if (tp.recording) {
        tp.recPeaks.push(Math.max(r.input[0], r.input[1]));
        // punch out a moment after the out point; timer recordings stop when their time is up
        if ((recPunch && r.pos > recPunch.end + app.rate * 0.25) || (recStopAt && Date.now() >= recStopAt)) stopRecording();
      }
      if (S.settings.follow && r.playing) {
        const px = (r.pos - ed.scroll) / ed.fpp;
        if (px > ed.viewWidth * 0.92 || px < 0) ed.scroll = Math.max(0, r.pos - ed.viewWidth * 0.1 * ed.fpp);
      }
    } catch {
      /* engine busy */
    }
    setTimeout(tick, tp.playing || tp.inputOpen ? 33 : 120);
  };
  tick();
}
let draggingPlayhead = false;
export function setDraggingPlayhead(v: boolean) {
  draggingPlayhead = v;
}

export function applyAppearance() {
  const r = document.documentElement;
  r.style.setProperty("--accent", S.settings.accent);
  const h = S.settings.accent.replace("#", "");
  const n = parseInt(h, 16);
  r.style.setProperty("--accent-rgb", `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`);
  r.style.setProperty("--blur", `${S.settings.blur}px`);
  r.classList.toggle("solid", !S.settings.glass);
}

export async function applyLanguage() {
  let l = S.settings.lang;
  if (l === "auto") {
    const sys = await api.systemLanguage().catch(() => navigator.language);
    l = String(sys).toLowerCase().startsWith("pl") ? "pl" : "en";
  }
  setLang(l as Lang);
}

export async function init() {
  try {
    const raw = await api.loadSettings();
    if (raw) S.settings = deepMerge(defaultSettings(), JSON.parse(raw));
  } catch {
    /* defaults */
  }
  applyAppearance();
  await applyLanguage();
  app.info = await api.appInfo();
  app.rate = app.info.rate;
  ed.fpp = app.rate / 50;
  await listen<{ instance: number; kind: string; message: string }>("plugin", (e) => {
    const { instance, kind } = e.payload;
    if (kind === "crashed") {
      app.pluginCrashed[instance] = true;
      app.pluginEditor[instance] = false;
      toast(`${t("pluginCrashed")}: ${e.payload.message}`, "error", { label: t("pluginRestart"), run: () => restartPlugin(instance) }, 12000);
    } else if (kind === "editorClosed") app.pluginEditor[instance] = false;
    else if (kind === "editorOpened") app.pluginEditor[instance] = true;
  });
  await listen<Job>("job", (e) => {
    if (app.job) app.job = { ...app.job, fraction: e.payload.fraction, label: e.payload.label };
  });
  if (S.settings.audio.input) api.setAudioDevice("input", S.settings.audio.input).catch(() => {});
  setMetronome(false);
  syncEngine();
  pollLoop();
  app.ready = true;
  const files = await api.startupFiles().catch(() => []);
  if (files.length) importPaths(files);
}

export function hasUnsavedWork() {
  return app.dirty && P.p.tracks.some((tr) => tr.clips.length > 0);
}
