// Development only: when the UI is opened in a plain browser (no Tauri), fake the backend so the
// interface can be inspected. Never included in production builds of the app window.
export function installDevMock() {
  const w = window as any;
  if (w.__TAURI_INTERNALS__) return;
  const rate = 48000;
  const sources: any[] = [];
  const mkSource = (id: number, name: string, secs: number, stem: string | null = null, parent: number | null = null) => ({
    id, name, path: null, kind: stem ? "stem" : "file", parent, stem, rate, channels: 2, frames: secs * rate, duration: secs,
    meta: { title: name, artist: "", album: "", codec: "mp3", source_rate: 44100, bits: 0 }, peak: 0.9,
  });
  let pos = 0;
  let playing = false;
  let last = performance.now();
  const peaks = (id: number) => {
    const s = sources.find((x) => x.id === id) ?? { frames: rate * 30 };
    const bins = Math.ceil(s.frames / 64);
    const buf = new ArrayBuffer(12 + 2 * bins * 8);
    const dv = new DataView(buf);
    dv.setUint32(0, 2, true);
    dv.setUint32(4, bins, true);
    dv.setUint32(8, 64, true);
    const f = new Float32Array(buf, 12);
    for (let c = 0; c < 2; c++)
      for (let i = 0; i < bins; i++) {
        const t = i / 750;
        // loud, compressed "master": mostly near full scale with a quieter intro and breakdown
        const sec = (i * 64) / rate;
        const part = sec < 12 ? 0.35 : sec > 70 && sec < 82 ? 0.5 : 0.92;
        const beat = Math.pow(Math.max(0, Math.cos(sec * Math.PI * 2 * (157 / 60) / 2)), 8);
        const v = Math.min(0.99, part * (0.22 + 0.45 * Math.random() ** 2 + 0.55 * beat));
        f[(c * bins + i) * 2] = -v;
        f[(c * bins + i) * 2 + 1] = v * 0.95;
      }
    return buf;
  };
  const handlers: Record<string, (a: any) => any> = {
    app_info: () => ({ version: "1.0.0", rate, modelsDir: "C:\\models", threads: 24, extensions: ["wav", "mp3", "flac"], neural: true }),
    system_language: () => "pl-PL",
    startup_files: () => [],
    load_settings: () => null,
    save_settings: () => null,
    set_project: () => null,
    transport: (a) => {
      if (a.t.cmd === "play") playing = true;
      if (a.t.cmd === "pause") playing = false;
      if (a.t.cmd === "seek") pos = a.t.pos;
      return null;
    },
    poll: () => {
      const now = performance.now();
      if (playing) pos += ((now - last) / 1000) * rate;
      last = now;
      const lv = playing ? 0.4 + Math.random() * 0.3 : 0;
      return { pos: Math.round(pos), playing, recording: false, recFrames: 0, recStart: 0, recRate: rate, meters: [], master: [lv, lv * 0.9, lv * 0.5, lv * 0.45], masterGr: 0, clip: false, input: [0, 0], inputOpen: false, error: "", deviceRate: rate };
    },
    spectrum: (a) => Array.from({ length: a.bands }, (_, i) => (playing ? -20 - i * 0.35 - Math.random() * 12 : -110)),
    source_peaks: (a) => peaks(a.id),
    // fake spectrogram: pink-ish noise floor, harmonic voice-like bands and a short 3 kHz beep
    spectrogram: (a) => {
      const w = a.width, h = a.height;
      const img = new Uint8Array(w * h);
      for (let x = 0; x < w; x++) {
        const t = (a.start + ((a.end - a.start) * x) / w) / rate;
        for (let y = 0; y < h; y++) {
          const hz = 30 * Math.pow(24000 / 30, 1 - y / h);
          let db = -78 - 10 * Math.log10(hz / 100) + Math.random() * 8;
          const f0 = 160 + 40 * Math.sin(t * 2.1);
          for (let k = 1; k < 18; k++) if (Math.abs(hz - f0 * k) < f0 * 0.04) db = Math.max(db, -30 - k * 2.2 + 10 * Math.sin(t * 3 + k));
          if (Math.abs(hz - 3000) < 90 && t % 20 > 9 && t % 20 < 9.8) db = -12;
          img[y * w + x] = Math.max(0, Math.min(255, ((db + 100) / 100) * 255));
        }
      }
      return img.buffer;
    },
    source_samples: () => new ArrayBuffer(8),
    source_bands: (a) => {
      const s = sources.find((x) => x.id === a.id) ?? { frames: rate * 30 };
      const bins = Math.ceil(s.frames / 256);
      const buf = new ArrayBuffer(8 + bins * 16);
      const dv = new DataView(buf);
      dv.setUint32(0, bins, true);
      dv.setUint32(4, 256, true);
      const f = new Float32Array(buf, 8);
      for (let i = 0; i < bins; i++) {
        const t = i / 172;
        const beat = Math.pow(Math.max(0, Math.sin(t * Math.PI * 4)), 6);
        const low = 0.05 + 0.3 * beat * (a.id % 3 === 1 ? 0.2 : 1);
        const mid = 0.08 + 0.06 * Math.abs(Math.sin(t * 0.8 + a.id));
        const high = 0.02 + 0.05 * Math.abs(Math.sin(t * 7.3)) + (a.id % 3 === 2 ? 0.05 : 0);
        f[i * 4] = Math.sqrt(low * low + mid * mid + high * high);
        f[i * 4 + 1] = low;
        f[i * 4 + 2] = mid;
        f[i * 4 + 3] = high;
      }
      return buf;
    },
    separation_models: () => [
      { id: "htdemucs_6s", name: "HT-Demucs 6", file: "", url: "", size: 136428532, stems: ["drums", "bass", "other", "vocals", "guitar", "piano"], rate: 44100, segment: 343980, license: "MIT", source: "huggingface.co", installed: true, path: "", legacy: false },
      { id: "htdemucs", name: "HT-Demucs 4", file: "", url: "", size: 165612636, stems: ["drums", "bass", "other", "vocals"], rate: 44100, segment: 343980, license: "MIT", source: "huggingface.co", installed: false, path: "", legacy: false },
    ],
    audio_devices: () => ({ outputs: ["Głośniki (Realtek)"], inputs: ["Mikrofon (USB)"], output: "Głośniki (Realtek)", input: "", default_output: "Głośniki (Realtek)", default_input: "Mikrofon (USB)" }),
    eq_response: (a) => Array.from({ length: a.points }, (_, i) => 3 * Math.sin(i / 20)),
    import_files: () => {
      const s = mkSource(sources.length + 1, "The Wellerman", 159);
      sources.push(s);
      return { sources: [s], errors: [] };
    },
    clear_sources: () => null,
    auto_duck: (a) => [{ t: a.start + 48000, v: 0 }, { t: a.start + 60000, v: a.duck }, { t: a.start + 400000, v: a.duck }, { t: a.start + 430000, v: 0 }],
    generate_source: (a) => {
      const s = { ...mkSource(2000 + sources.length, a.name, a.seconds), kind: "generated" };
      sources.push(s);
      return s;
    },
    bounce: (a) => {
      const s = { ...mkSource(3000 + sources.length, a.name, 30), kind: "bounce" };
      sources.push(s);
      return s;
    },
    "plugin:dialog|open": (a) => (a.options?.multiple ? ["C:\\Music\\song.mp3"] : "C:\\Music\\song.mp3"),
    separate_source: (a) => {
      const parent = sources.find((s) => s.id === a.id)!;
      const out = ["vocals", "drums", "bass", "guitar", "piano", "other"].map((st, i) => {
        const s = mkSource(100 + i + sources.length, parent.name + " · " + st, parent.duration, st, parent.id);
        sources.push(s);
        return s;
      });
      return { stems: out, device: "CPU", seconds: 3.2 };
    },
    // processing: a new edit of the same length that keeps stem / parent like the real backend
    apply_op: (a) => {
      const src = sources.find((s) => s.id === a.id)!;
      const s = { ...mkSource(1000 + sources.length, src.name, src.duration, src.stem, src.parent ?? src.id), kind: "edit" };
      sources.push(s);
      return s;
    },
    // plug-ins: a few fake effects, one file that failed to load and one instrument
    plugin_dirs: () => ["C:\\Program Files\\Common Files\\VST3", "C:\\Program Files\\Common Files\\CLAP"],
    plugin_scan: () => [
      { file: "C:\\Program Files\\Common Files\\VST3\\FabFilter Pro-Q 3.vst3", stamp: "", error: null, plugins: [{ format: "vst3", path: "x", id: "a", name: "Pro-Q 3", vendor: "FabFilter", version: "3.2", category: "Fx|EQ", instrument: false }] },
      { file: "C:\\Program Files\\Common Files\\VST3\\ValhallaRoom.vst3", stamp: "", error: null, plugins: [{ format: "vst3", path: "y", id: "b", name: "ValhallaRoom", vendor: "Valhalla DSP", version: "1.6", category: "Fx|Reverb", instrument: false }] },
      { file: "C:\\Program Files\\Common Files\\CLAP\\Surge XT Effects.clap", stamp: "", error: null, plugins: [{ format: "clap", path: "z", id: "c", name: "Surge XT Effects", vendor: "Surge Synth Team", version: "1.3", category: "audio-effect|multi-effects", instrument: false }, { format: "clap", path: "z", id: "d", name: "Surge XT", vendor: "Surge Synth Team", version: "1.3", category: "instrument|synthesizer", instrument: true }] },
      { file: "C:\\Program Files\\Common Files\\VST3\\Old.vst3", stamp: "", error: "crashed while loading", plugins: [] },
    ],
    plugin_create: (a) => ({ name: a.desc.name, hasEditor: true, latency: 0 }),
    plugin_editor: () => null,
    plugin_states: () => ({}),
    plugin_retain: () => null,
    plugin_status: () => [],
    plugin_restart: () => ({ name: "", hasEditor: true, latency: 0 }),
    input_monitor: () => null,
    "plugin:event|listen": () => 1,
    "plugin:event|unlisten": () => null,
    "plugin:app|version": () => "1.0.0",
    "plugin:window|is_maximized": () => false,
    "plugin:window|set_progress_bar": () => null,
    "plugin:webview|get_all_webviews": () => [],
  };
  let cb = 1;
  w.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback: () => cb++,
    unregisterCallback: () => {},
    convertFileSrc: (p: string) => p,
    invoke: async (cmd: string, args: any) => {
      const h = handlers[cmd];
      if (!h) {
        console.debug("[mock] unhandled", cmd);
        return null;
      }
      return h(args ?? {});
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  w.__oapMock = { sources, mkSource };
}
