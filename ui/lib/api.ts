// Typed wrappers around the Rust commands.
import { invoke } from "@tauri-apps/api/core";

export interface Meta {
  title: string;
  artist: string;
  album: string;
  codec: string;
  source_rate: number;
  bits: number;
}

export interface SourceInfo {
  id: number;
  name: string;
  path: string | null;
  kind: "file" | "recording" | "stem" | "edit" | "bounce" | "generated";
  parent: number | null;
  stem: string | null;
  rate: number;
  channels: number;
  frames: number;
  duration: number;
  meta: Meta;
  peak: number;
}

export interface AppInfo {
  version: string;
  rate: number;
  modelsDir: string;
  threads: number;
  extensions: string[];
  neural: boolean;
}

export interface TrackMeter {
  id: number;
  l: number;
  r: number;
  gr: number;
}

export interface PollState {
  pos: number;
  playing: boolean;
  recording: boolean;
  recFrames: number;
  recStart: number;
  recRate: number;
  meters: TrackMeter[];
  master: [number, number, number, number];
  masterGr: number;
  clip: boolean;
  input: [number, number];
  inputOpen: boolean;
  error: string;
  deviceRate: number;
}

export interface Analysis {
  duration: number;
  rate: number;
  channels: number;
  peakDb: number;
  truePeakDb: number;
  rmsDb: number;
  lufs: number;
  lufsShortMax: number;
  lra: number;
  crestDb: number;
  dcOffset: number;
  correlation: number;
  clipped: number;
  bpm: number;
  bpmConfidence: number;
  key: string;
  keyConfidence: number;
  silenceRatio: number;
}

export interface ModelStatus {
  id: string;
  name: string;
  file: string;
  url: string;
  size: number;
  stems: string[];
  rate: number;
  segment: number;
  license: string;
  source: string;
  installed: boolean;
  path: string;
  legacy: boolean;
}

export interface DeviceList {
  outputs: string[];
  inputs: string[];
  output: string;
  input: string;
  default_output: string;
  default_input: string;
}

export interface Peaks {
  channels: number;
  bins: number;
  spp: number;
  data: Float32Array[]; // per channel [min,max,…]
}

export interface Bands {
  bins: number;
  spp: number;
  data: Float32Array; // [rms, low, mid, high] per bin
}

export interface PluginDesc {
  format: "vst3" | "clap" | string;
  path: string;
  id: string;
  name: string;
  vendor: string;
  version: string;
  category: string;
  instrument: boolean;
}
export interface PluginScanEntry {
  file: string;
  stamp: string;
  plugins: PluginDesc[];
  error: string | null;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  systemLanguage: () => invoke<string>("system_language"),
  startupFiles: () => invoke<string[]>("startup_files"),
  loadSettings: () => invoke<string | null>("load_settings"),
  saveSettings: (json: string) => invoke<void>("save_settings", { json }),
  cancelJob: () => invoke<void>("cancel_job"),
  importFiles: (paths: string[]) => invoke<{ sources: SourceInfo[]; errors: string[] }>("import_files", { paths }),
  listSources: () => invoke<SourceInfo[]>("list_sources"),
  async peaks(id: number): Promise<Peaks> {
    const buf = await invoke<ArrayBuffer>("source_peaks", { id });
    const dv = new DataView(buf);
    const channels = dv.getUint32(0, true);
    const bins = dv.getUint32(4, true);
    const spp = dv.getUint32(8, true);
    const data: Float32Array[] = [];
    for (let c = 0; c < channels; c++) data.push(new Float32Array(buf.slice(12 + c * bins * 8, 12 + (c + 1) * bins * 8)));
    return { channels, bins, spp, data };
  },
  async samples(id: number, start: number, len: number): Promise<Float32Array[]> {
    const buf = await invoke<ArrayBuffer>("source_samples", { id, start: Math.max(0, Math.floor(start)), len: Math.floor(len) });
    const dv = new DataView(buf);
    const ch = dv.getUint32(0, true);
    const n = dv.getUint32(4, true);
    const out: Float32Array[] = [];
    for (let c = 0; c < ch; c++) out.push(new Float32Array(buf.slice(8 + c * n * 4, 8 + (c + 1) * n * 4)));
    return out;
  },
  async bands(id: number): Promise<Bands> {
    const buf = await invoke<ArrayBuffer>("source_bands", { id });
    const dv = new DataView(buf);
    const bins = dv.getUint32(0, true);
    const spp = dv.getUint32(4, true);
    return { bins, spp, data: new Float32Array(buf.slice(8, 8 + bins * 16)) };
  },
  removeSources: (ids: number[]) => invoke<void>("remove_sources", { ids }),
  renameSource: (id: number, name: string) => invoke<SourceInfo>("rename_source", { id, name }),
  clearSources: () => invoke<void>("clear_sources"),
  setProject: (project: unknown) => invoke<void>("set_project", { project }),
  transport: (t: Record<string, unknown>) => invoke<void>("transport", { t }),
  poll: () => invoke<PollState>("poll"),
  spectrum: (bands: number) => invoke<number[]>("spectrum", { bands }),
  audioDevices: () => invoke<DeviceList>("audio_devices"),
  setAudioDevice: (kind: "output" | "input", name: string | null) => invoke<void>("set_audio_device", { kind, name }),
  inputMonitor: (on: boolean) => invoke<void>("input_monitor", { on }),
  recordStart: (pos: number) => invoke<void>("record_start", { pos }),
  recordStop: (name: string) => invoke<{ source: SourceInfo; start: number } | null>("record_stop", { name }),
  applyOp: (id: number, start: number, end: number, op: unknown, name?: string) =>
    invoke<SourceInfo>("apply_op", { id, start: Math.max(0, Math.round(start)), end: Math.max(0, Math.round(end)), op, name: name ?? null }),
  bounce: (project: unknown, start: number, end: number, name: string, tail = 0) => invoke<SourceInfo>("bounce", { project, start, end, name, tail }),
  analyze: (id: number, start = 0, end = 0) => invoke<Analysis>("analyze", { id, start: Math.round(start), end: Math.round(end) }),
  async spectrogram(id: number, start: number, end: number, width: number, height: number): Promise<Uint8Array> {
    const b = await invoke<ArrayBuffer>("spectrogram", { id, start: Math.round(start), end: Math.round(end), width, height });
    return new Uint8Array(b);
  },
  eqResponse: (bands: unknown[], points: number) => invoke<number[]>("eq_response", { bands, points }),
  models: () => invoke<ModelStatus[]>("separation_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  separate: (id: number, start: number, end: number, options: { engine: string; gpu: boolean; quality: number }, keep: string[]) =>
    invoke<{ stems: SourceInfo[]; device: string; seconds: number }>("separate_source", { id, start: Math.round(start), end: Math.round(end), options, keep }),
  exportMix: (path: string, project: unknown, options: unknown) => invoke<void>("export_mix", { path, project, options }),
  exportTracks: (dir: string, project: unknown, options: unknown) => invoke<string[]>("export_tracks", { dir, project, options }),
  exportRegions: (dir: string, project: unknown, regions: { name: string; start: number; end: number }[], options: unknown) => invoke<string[]>("export_regions", { dir, project, regions, options }),
  autoDuck: (project: unknown, control: number, start: number, end: number, threshold: number, duck: number, fall: number, rise: number, hold: number) =>
    invoke<{ t: number; v: number }[]>("auto_duck", { project, control, start, end, threshold, duck, fall, rise, hold }),
  generateSource: (spec: unknown, seconds: number, stereo: boolean, name: string) => invoke<SourceInfo>("generate_source", { spec, seconds, stereo, name }),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
  writeTextFile: (path: string, text: string) => invoke<void>("write_text_file", { path, text }),
  exportSource: (id: number, path: string, options: unknown) => invoke<void>("export_source", { id, path, options }),
  pluginDirs: () => invoke<string[]>("plugin_dirs"),
  pluginScan: (dirs: string[], full: boolean) => invoke<PluginScanEntry[]>("plugin_scan", { dirs, full }),
  pluginCreate: (instance: number, desc: PluginDesc, state: string) => invoke<{ name: string; hasEditor: boolean; latency: number }>("plugin_create", { instance, desc, state }),
  pluginRestart: (instance: number) => invoke<{ name: string; hasEditor: boolean; latency: number }>("plugin_restart", { instance }),
  pluginEditor: (instance: number, open: boolean) => invoke<void>("plugin_editor", { instance, open }),
  pluginStates: (instances: number[]) => invoke<Record<string, string>>("plugin_states", { instances }),
  pluginStatus: () => invoke<{ instance: number; name: string; crashed: boolean; hasEditor: boolean }[]>("plugin_status"),
  pluginRetain: (keep: number[]) => invoke<void>("plugin_retain", { keep }),
  listAudioFiles: (dir: string, recursive: boolean) => invoke<string[]>("list_audio_files", { dir, recursive }),
  batchProcess: (files: string[], steps: unknown[], options: unknown) =>
    invoke<{ input: string; output: string | null; error: string | null }[]>("batch_process", { files, steps, options }),
  saveProject: (path: string, state: unknown, used: number[]) => invoke<void>("save_project", { path, state, used }),
  loadProject: (path: string) => invoke<{ state: any; sources: SourceInfo[]; ratio: number; missing: string[] }>("load_project", { path }),
};
