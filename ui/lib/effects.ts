// Catalogue of effects (real-time inserts) and offline processing operations with their
// parameter descriptions. Labels are [pl, en].

export type L = [string, string];

export interface Param {
  key: string;
  label: L;
  kind?: "num" | "bool" | "select";
  min?: number;
  max?: number;
  step?: number;
  unit?: string;
  log?: boolean;
  options?: { value: string; label: L }[];
}

export interface EffectDef {
  type: string;
  label: L;
  group: "eq" | "dyn" | "space" | "mod" | "color" | "util";
  params: Param[];
  defaults: Record<string, any>;
  tail?: number; // seconds of tail when rendered offline
}

const hz = (key: string, label: L, min = 20, max = 20000): Param => ({ key, label, min, max, step: 1, unit: "Hz", log: true });
const db = (key: string, label: L, min = -60, max = 24, step = 0.1): Param => ({ key, label, min, max, step, unit: "dB" });
const ms = (key: string, label: L, min = 0.1, max = 2000, log = true): Param => ({ key, label, min, max, step: 0.1, unit: "ms", log });
const pct = (key: string, label: L): Param => ({ key, label, min: 0, max: 1, step: 0.01, unit: "%" });

export const FILTER_KINDS: { value: string; label: L }[] = [
  { value: "lowShelf", label: ["Półka niska", "Low shelf"] },
  { value: "peak", label: ["Dzwon", "Bell"] },
  { value: "highShelf", label: ["Półka wysoka", "High shelf"] },
  { value: "lowPass", label: ["Dolnoprzepustowy", "Low-pass"] },
  { value: "highPass", label: ["Górnoprzepustowy", "High-pass"] },
  { value: "bandPass", label: ["Pasmowy", "Band-pass"] },
  { value: "notch", label: ["Zaporowy", "Notch"] },
];

export const defaultEqBands = () => [
  { kind: "highPass", freq: 30, q: 0.707, gain: 0, on: false },
  { kind: "lowShelf", freq: 100, q: 0.707, gain: 0, on: true },
  { kind: "peak", freq: 400, q: 1, gain: 0, on: true },
  { kind: "peak", freq: 2000, q: 1, gain: 0, on: true },
  { kind: "highShelf", freq: 8000, q: 0.707, gain: 0, on: true },
  { kind: "lowPass", freq: 18000, q: 0.707, gain: 0, on: false },
];

export const EFFECTS: EffectDef[] = [
  { type: "eq", label: ["Korektor parametryczny", "Parametric EQ"], group: "eq", params: [], defaults: { bands: defaultEqBands() } },
  {
    type: "filter",
    label: ["Filtr", "Filter"],
    group: "eq",
    params: [
      { key: "kind", label: ["Typ", "Type"], kind: "select", options: FILTER_KINDS.filter((k) => !["lowShelf", "highShelf", "peak"].includes(k.value)) },
      hz("freq", ["Częstotliwość", "Frequency"]),
      { key: "q", label: ["Dobroć Q", "Q"], min: 0.1, max: 10, step: 0.01, log: true },
      { key: "slope", label: ["Nachylenie (×12 dB/okt)", "Slope (×12 dB/oct)"], min: 1, max: 4, step: 1 },
    ],
    defaults: { kind: "highPass", freq: 80, q: 0.707, slope: 2 },
  },
  {
    type: "compressor",
    label: ["Kompresor", "Compressor"],
    group: "dyn",
    params: [
      db("threshold", ["Próg", "Threshold"], -60, 0),
      { key: "ratio", label: ["Stosunek", "Ratio"], min: 1, max: 20, step: 0.1, unit: ":1", log: true },
      ms("attack", ["Atak", "Attack"], 0.1, 200),
      ms("release", ["Zwolnienie", "Release"], 5, 2000),
      db("knee", ["Kolano", "Knee"], 0, 24),
      db("makeup", ["Wzmocnienie", "Makeup"], 0, 24),
      pct("mix", ["Mieszanie", "Mix"]),
    ],
    defaults: { threshold: -20, ratio: 3, attack: 10, release: 120, knee: 6, makeup: 3, mix: 1 },
  },
  {
    type: "limiter",
    label: ["Limiter", "Limiter"],
    group: "dyn",
    params: [db("input", ["Wejście", "Input"], 0, 24), db("ceiling", ["Sufit", "Ceiling"], -12, 0), ms("release", ["Zwolnienie", "Release"], 5, 1000), ms("lookahead", ["Wyprzedzenie", "Look-ahead"], 0.5, 10, false)],
    defaults: { input: 0, ceiling: -1, release: 80, lookahead: 3 },
  },
  {
    type: "gate",
    label: ["Bramka szumów", "Noise gate"],
    group: "dyn",
    params: [db("threshold", ["Próg", "Threshold"], -90, 0), ms("attack", ["Atak", "Attack"], 0.1, 100), ms("hold", ["Podtrzymanie", "Hold"], 0, 1000, false), ms("release", ["Zwolnienie", "Release"], 5, 2000), db("range", ["Tłumienie", "Range"], -90, 0)],
    defaults: { threshold: -45, attack: 1, hold: 40, release: 150, range: -60 },
  },
  {
    type: "deEsser",
    label: ["De-esser", "De-esser"],
    group: "dyn",
    params: [hz("freq", ["Częstotliwość", "Frequency"], 2000, 12000), db("threshold", ["Próg", "Threshold"], -60, 0), db("reduction", ["Maks. redukcja", "Max reduction"], 0, 24)],
    defaults: { freq: 6500, threshold: -30, reduction: 8 },
  },
  {
    type: "reverb",
    label: ["Pogłos", "Reverb"],
    group: "space",
    tail: 4,
    params: [pct("room", ["Rozmiar", "Size"]), pct("damping", ["Tłumienie", "Damping"]), pct("wet", ["Efekt", "Wet"]), pct("dry", ["Sygnał", "Dry"]), pct("width", ["Szerokość", "Width"]), ms("predelay", ["Opóźnienie wstępne", "Pre-delay"], 0, 200, false), hz("lowcut", ["Obcięcie basu", "Low cut"], 20, 1000)],
    defaults: { room: 0.65, damping: 0.45, wet: 0.25, dry: 1, width: 1, predelay: 15, lowcut: 180 },
  },
  {
    type: "delay",
    label: ["Echo / Delay", "Echo / Delay"],
    group: "space",
    tail: 4,
    params: [ms("time", ["Czas", "Time"], 10, 2000), pct("feedback", ["Sprzężenie", "Feedback"]), pct("mix", ["Mieszanie", "Mix"]), { key: "pingpong", label: ["Ping-pong", "Ping-pong"], kind: "bool" }, hz("tone", ["Barwa", "Tone"], 500, 20000)],
    defaults: { time: 375, feedback: 0.35, mix: 0.25, pingpong: true, tone: 6000 },
  },
  {
    type: "chorus",
    label: ["Chorus", "Chorus"],
    group: "mod",
    params: [{ key: "rate", label: ["Szybkość", "Rate"], min: 0.05, max: 5, step: 0.01, unit: "Hz", log: true }, pct("depth", ["Głębokość", "Depth"]), pct("mix", ["Mieszanie", "Mix"])],
    defaults: { rate: 0.8, depth: 0.5, mix: 0.5 },
  },
  {
    type: "flanger",
    label: ["Flanger", "Flanger"],
    group: "mod",
    params: [{ key: "rate", label: ["Szybkość", "Rate"], min: 0.05, max: 5, step: 0.01, unit: "Hz", log: true }, pct("depth", ["Głębokość", "Depth"]), { key: "feedback", label: ["Sprzężenie", "Feedback"], min: -0.95, max: 0.95, step: 0.01 }, pct("mix", ["Mieszanie", "Mix"])],
    defaults: { rate: 0.25, depth: 0.7, feedback: 0.5, mix: 0.5 },
  },
  {
    type: "phaser",
    label: ["Phaser", "Phaser"],
    group: "mod",
    params: [
      { key: "rate", label: ["Szybkość", "Rate"], min: 0.02, max: 5, step: 0.01, unit: "Hz", log: true },
      hz("min", ["Od", "From"], 50, 5000),
      hz("max", ["Do", "To"], 500, 16000),
      { key: "feedback", label: ["Sprzężenie", "Feedback"], min: -0.95, max: 0.95, step: 0.01 },
      pct("mix", ["Mieszanie", "Mix"]),
      { key: "stages", label: ["Stopnie", "Stages"], min: 2, max: 12, step: 2 },
    ],
    defaults: { rate: 0.35, min: 300, max: 3000, feedback: 0.5, mix: 0.5, stages: 6 },
  },
  {
    type: "tremolo",
    label: ["Tremolo / Auto-pan", "Tremolo / Auto-pan"],
    group: "mod",
    params: [{ key: "rate", label: ["Szybkość", "Rate"], min: 0.1, max: 20, step: 0.01, unit: "Hz", log: true }, pct("depth", ["Głębokość", "Depth"]), { key: "pan", label: ["Auto-pan", "Auto-pan"], kind: "bool" }],
    defaults: { rate: 5, depth: 0.5, pan: false },
  },
  {
    type: "saturation",
    label: ["Saturacja", "Saturation"],
    group: "color",
    params: [
      db("drive", ["Nasycenie", "Drive"], 0, 36),
      {
        key: "curve",
        label: ["Charakter", "Character"],
        kind: "select",
        options: [
          { value: "tape", label: ["Taśma", "Tape"] },
          { value: "tube", label: ["Lampa", "Tube"] },
          { value: "hard", label: ["Twarde obcinanie", "Hard clip"] },
          { value: "fuzz", label: ["Fuzz", "Fuzz"] },
        ],
      },
      hz("tone", ["Barwa", "Tone"], 1000, 20000),
      pct("mix", ["Mieszanie", "Mix"]),
      db("output", ["Wyjście", "Output"], -24, 12),
    ],
    defaults: { drive: 6, curve: "tape", tone: 14000, mix: 1, output: 0 },
  },
  {
    type: "bitCrusher",
    label: ["Bit crusher", "Bit crusher"],
    group: "color",
    params: [{ key: "bits", label: ["Bity", "Bits"], min: 1, max: 16, step: 0.5 }, { key: "downsample", label: ["Redukcja próbkowania", "Downsample"], min: 1, max: 32, step: 1 }, pct("mix", ["Mieszanie", "Mix"])],
    defaults: { bits: 8, downsample: 4, mix: 1 },
  },
  {
    type: "stereo",
    label: ["Narzędzie stereo", "Stereo tool"],
    group: "util",
    params: [
      db("gain", ["Wzmocnienie", "Gain"], -24, 24),
      { key: "pan", label: ["Balans", "Balance"], min: -1, max: 1, step: 0.01 },
      { key: "width", label: ["Szerokość", "Width"], min: 0, max: 2, step: 0.01, unit: "×" },
      { key: "invertL", label: ["Odwróć fazę L", "Invert L"], kind: "bool" },
      { key: "invertR", label: ["Odwróć fazę P", "Invert R"], kind: "bool" },
      { key: "mono", label: ["Mono", "Mono"], kind: "bool" },
    ],
    defaults: { gain: 0, pan: 0, width: 1, invertL: false, invertR: false, mono: false },
  },
];

const hzr = (key: string, label: L, min: number, max: number): Param => ({ key, label, min, max, step: 0.01, unit: "Hz", log: true });
const FILTER_MODES: { value: string; label: L }[] = [
  { value: "lowPass", label: ["Dolnoprzepustowy", "Low-pass"] },
  { value: "bandPass", label: ["Pasmowy", "Band-pass"] },
  { value: "highPass", label: ["Górnoprzepustowy", "High-pass"] },
];
const modeParam: Param = { key: "mode", label: ["Typ", "Type"], kind: "select", options: FILTER_MODES };

EFFECTS.push(
  {
    type: "ladder",
    label: ["Filtr analogowy (ladder)", "Ladder filter"],
    group: "eq",
    params: [modeParam, hz("cutoff", ["Odcięcie", "Cutoff"]), pct("resonance", ["Rezonans", "Resonance"]), db("drive", ["Nasycenie", "Drive"], 0, 24)],
    defaults: { mode: "lowPass", cutoff: 1200, resonance: 0.4, drive: 3 },
  },
  {
    type: "autoFilter",
    label: ["Filtr automatyczny / wah", "Auto filter / wah"],
    group: "eq",
    params: [
      modeParam,
      hz("base", ["Częstotliwość bazowa", "Base frequency"], 20, 18000),
      { key: "depth", label: ["Zakres", "Range"], min: 0, max: 7, step: 0.1, unit: "okt" },
      hzr("rate", ["Szybkość LFO", "LFO rate"], 0.01, 20),
      { key: "envelope", label: ["Sterowanie obwiednią", "Envelope follower"], kind: "bool" },
      { key: "sensitivity", label: ["Czułość obwiedni", "Envelope sensitivity"], min: 0, max: 4, step: 0.01 },
      { key: "q", label: ["Dobroć Q", "Q"], min: 0.5, max: 20, step: 0.01, log: true },
      pct("mix", ["Mieszanie", "Mix"]),
    ],
    defaults: { mode: "lowPass", base: 400, depth: 3, rate: 0.5, envelope: false, sensitivity: 1, q: 3, mix: 1 },
  },
  {
    type: "vowel",
    label: ["Filtr formantowy (samogłoski)", "Formant filter (vowels)"],
    group: "eq",
    params: [
      { key: "vowel", label: ["Samogłoska A→E→I→O→U", "Vowel A→E→I→O→U"], min: 0, max: 4, step: 0.01 },
      pct("resonance", ["Rezonans", "Resonance"]),
      hzr("rate", ["Szybkość LFO", "LFO rate"], 0, 10),
      { key: "depth", label: ["Głębokość LFO", "LFO depth"], min: 0, max: 4, step: 0.01 },
      pct("mix", ["Mieszanie", "Mix"]),
    ],
    defaults: { vowel: 0, resonance: 0.4, rate: 0, depth: 0, mix: 1 },
  },
  {
    type: "tilt",
    label: ["Korektor nachylenia (tilt)", "Tilt EQ"],
    group: "eq",
    params: [db("tilt", ["Nachylenie (− ciemniej, + jaśniej)", "Tilt (− darker, + brighter)"], -12, 12), hz("pivot", ["Punkt obrotu", "Pivot"], 100, 8000)],
    defaults: { tilt: 0, pivot: 1000 },
  },
  {
    type: "comb",
    label: ["Filtr grzebieniowy", "Comb filter"],
    group: "eq",
    tail: 1,
    params: [hz("freq", ["Częstotliwość", "Frequency"], 20, 5000), { key: "feedback", label: ["Sprzężenie", "Feedback"], min: -0.97, max: 0.97, step: 0.01 }, pct("mix", ["Mieszanie", "Mix"])],
    defaults: { freq: 220, feedback: 0.6, mix: 0.5 },
  },
  {
    type: "multiband",
    label: ["Kompresor wielopasmowy", "Multiband compressor"],
    group: "dyn",
    params: [
      hz("lowX", ["Podział niski", "Low crossover"], 40, 2000),
      hz("highX", ["Podział wysoki", "High crossover"], 500, 16000),
      db("thrLow", ["Próg – bas", "Threshold – low"], -60, 0),
      db("thrMid", ["Próg – środek", "Threshold – mid"], -60, 0),
      db("thrHigh", ["Próg – góra", "Threshold – high"], -60, 0),
      db("gainLow", ["Wzmocnienie – bas", "Gain – low"], -12, 12),
      db("gainMid", ["Wzmocnienie – środek", "Gain – mid"], -12, 12),
      db("gainHigh", ["Wzmocnienie – góra", "Gain – high"], -12, 12),
      { key: "ratio", label: ["Stosunek", "Ratio"], min: 1, max: 20, step: 0.1, unit: ":1", log: true },
      ms("attack", ["Atak", "Attack"], 0.1, 200),
      ms("release", ["Zwolnienie", "Release"], 5, 2000),
    ],
    defaults: { lowX: 200, highX: 3000, thrLow: -20, thrMid: -18, thrHigh: -16, gainLow: 0, gainMid: 0, gainHigh: 0, ratio: 3, attack: 10, release: 150 },
  },
  {
    type: "transient",
    label: ["Kształtowanie transjentów", "Transient shaper"],
    group: "dyn",
    params: [
      { key: "attack", label: ["Atak", "Attack"], min: -1, max: 1, step: 0.01, unit: "%" },
      { key: "sustain", label: ["Wybrzmienie", "Sustain"], min: -1, max: 1, step: 0.01, unit: "%" },
      db("output", ["Wyjście", "Output"], -24, 12),
    ],
    defaults: { attack: 0.3, sustain: 0, output: 0 },
  },
  {
    type: "clipper",
    label: ["Clipper", "Clipper"],
    group: "dyn",
    params: [db("input", ["Wejście", "Input"], 0, 24), db("ceiling", ["Sufit", "Ceiling"], -24, 0), pct("softness", ["Miękkość", "Softness"]), db("output", ["Wyjście", "Output"], -24, 12)],
    defaults: { input: 3, ceiling: -0.3, softness: 0.3, output: 0 },
  },
  {
    type: "exciter",
    label: ["Exciter (harmoniczne)", "Exciter (harmonics)"],
    group: "color",
    params: [hz("freq", ["Od częstotliwości", "From frequency"], 500, 16000), db("drive", ["Nasycenie", "Drive"], 0, 36), pct("mix", ["Ilość", "Amount"])],
    defaults: { freq: 3500, drive: 12, mix: 0.2 },
  },
  {
    type: "tape",
    label: ["Symulacja taśmy", "Tape simulation"],
    group: "color",
    params: [
      db("drive", ["Nasycenie", "Drive"], 0, 24),
      pct("wow", ["Wow (wolne falowanie)", "Wow"]),
      pct("flutter", ["Flutter (szybkie drżenie)", "Flutter"]),
      db("hiss", ["Szum taśmy", "Hiss"], -96, -30),
      hz("tone", ["Barwa", "Tone"], 1000, 20000),
      pct("mix", ["Mieszanie", "Mix"]),
    ],
    defaults: { drive: 6, wow: 0.2, flutter: 0.2, hiss: -96, tone: 12000, mix: 1 },
  },
  {
    type: "ringMod",
    label: ["Modulator pierścieniowy", "Ring modulator"],
    group: "mod",
    params: [hzr("freq", ["Częstotliwość", "Frequency"], 1, 5000), pct("mix", ["Mieszanie", "Mix"])],
    defaults: { freq: 30, mix: 0.5 },
  },
  {
    type: "vibrato",
    label: ["Vibrato", "Vibrato"],
    group: "mod",
    params: [hzr("rate", ["Szybkość", "Rate"], 0.1, 14), ms("depth", ["Głębokość", "Depth"], 0, 10, false)],
    defaults: { rate: 5, depth: 2 },
  },
  {
    type: "haas",
    label: ["Poszerzenie Haasa", "Haas widener"],
    group: "space",
    params: [ms("delay", ["Opóźnienie", "Delay"], 0, 40, false), { key: "right", label: ["Opóźniaj prawy kanał", "Delay right channel"], kind: "bool" }, pct("mix", ["Mieszanie", "Mix"])],
    defaults: { delay: 12, right: true, mix: 1 },
  },
);

export const GROUPS: { id: EffectDef["group"]; label: L }[] = [
  { id: "eq", label: ["Korekcja", "EQ & filters"] },
  { id: "dyn", label: ["Dynamika", "Dynamics"] },
  { id: "space", label: ["Przestrzeń", "Space"] },
  { id: "mod", label: ["Modulacja", "Modulation"] },
  { id: "color", label: ["Barwa", "Colour"] },
  { id: "util", label: ["Narzędzia", "Utility"] },
];

export function effectDef(type: string): EffectDef | undefined {
  return EFFECTS.find((e) => e.type === type);
}

export function newSpec(type: string): any {
  const d = effectDef(type)!;
  return { type, ...structuredClone(d.defaults) };
}

/** Ready-made chains. */
export const PRESETS: { id: string; label: L; chain: () => any[] }[] = [
  {
    id: "vocal",
    label: ["Wokal – czysty i bliski", "Vocal – clean and close"],
    chain: () => [
      { type: "filter", kind: "highPass", freq: 90, q: 0.707, slope: 2 },
      { type: "eq", bands: [
        { kind: "peak", freq: 300, q: 1.2, gain: -2.5, on: true },
        { kind: "peak", freq: 3500, q: 0.9, gain: 2.5, on: true },
        { kind: "highShelf", freq: 10000, q: 0.707, gain: 2, on: true },
      ] },
      { type: "deEsser", freq: 6500, threshold: -32, reduction: 6 },
      { type: "compressor", threshold: -22, ratio: 3.5, attack: 6, release: 90, knee: 6, makeup: 5, mix: 1 },
      { type: "reverb", room: 0.5, damping: 0.5, wet: 0.14, dry: 1, width: 0.9, predelay: 25, lowcut: 250 },
    ],
  },
  {
    id: "podcast",
    label: ["Mowa / podcast", "Speech / podcast"],
    chain: () => [
      { type: "filter", kind: "highPass", freq: 80, q: 0.707, slope: 2 },
      { type: "gate", threshold: -50, attack: 1, hold: 60, release: 200, range: -20 },
      { type: "eq", bands: [
        { kind: "peak", freq: 250, q: 1, gain: -2, on: true },
        { kind: "peak", freq: 3000, q: 1, gain: 2, on: true },
      ] },
      { type: "compressor", threshold: -24, ratio: 4, attack: 5, release: 120, knee: 6, makeup: 6, mix: 1 },
      { type: "limiter", input: 0, ceiling: -1, release: 60, lookahead: 3 },
    ],
  },
  {
    id: "master",
    label: ["Mastering – głośno i bezpiecznie", "Mastering – loud and safe"],
    chain: () => [
      { type: "eq", bands: [
        { kind: "highPass", freq: 25, q: 0.707, gain: 0, on: true },
        { kind: "lowShelf", freq: 90, q: 0.707, gain: 1, on: true },
        { kind: "highShelf", freq: 12000, q: 0.707, gain: 1.5, on: true },
      ] },
      { type: "compressor", threshold: -14, ratio: 2, attack: 30, release: 200, knee: 8, makeup: 2, mix: 1 },
      { type: "limiter", input: 4, ceiling: -1, release: 80, lookahead: 4 },
    ],
  },
  {
    id: "lofi",
    label: ["Lo-fi", "Lo-fi"],
    chain: () => [
      { type: "filter", kind: "lowPass", freq: 5000, q: 0.707, slope: 2 },
      { type: "saturation", drive: 10, curve: "tape", tone: 7000, mix: 1, output: -4 },
      { type: "bitCrusher", bits: 12, downsample: 2, mix: 0.6 },
      { type: "chorus", rate: 0.3, depth: 0.2, mix: 0.3 },
    ],
  },
];

// ─────────────────────── offline processing operations ───────────────────────

export interface OpDef {
  op: string;
  label: L;
  params: Param[];
  defaults: Record<string, any>;
  /** operation changes the length of the processed range */
  changesLength?: boolean;
  /** works on whole source only */
  whole?: boolean;
}

export const FADE_CURVES: { value: string; label: L }[] = [
  { value: "sCurve", label: ["Krzywa S", "S-curve"] },
  { value: "linear", label: ["Liniowa", "Linear"] },
  { value: "logarithmic", label: ["Logarytmiczna", "Logarithmic"] },
  { value: "exponential", label: ["Wykładnicza", "Exponential"] },
  { value: "equalPower", label: ["Stała moc", "Equal power"] },
];

export const OPS: OpDef[] = [
  { op: "gain", label: ["Wzmocnienie", "Amplify"], params: [db("db", ["Zmiana", "Change"], -48, 48)], defaults: { db: 3 } },
  {
    op: "normalize",
    label: ["Normalizacja", "Normalise"],
    params: [
      { key: "mode", label: ["Tryb", "Mode"], kind: "select", options: [
        { value: "peak", label: ["Szczyt (dBFS)", "Peak (dBFS)"] },
        { value: "lufs", label: ["Głośność (LUFS)", "Loudness (LUFS)"] },
        { value: "rms", label: ["RMS (dBFS)", "RMS (dBFS)"] },
      ] },
      { key: "target", label: ["Cel", "Target"], min: -40, max: 0, step: 0.1 },
      { key: "perChannel", label: ["Kanały osobno", "Channels independently"], kind: "bool" },
    ],
    defaults: { mode: "peak", target: -1, perChannel: false },
  },
  { op: "fadeIn", label: ["Narastanie", "Fade in"], params: [{ key: "curve", label: ["Krzywa", "Curve"], kind: "select", options: FADE_CURVES }], defaults: { curve: "sCurve" } },
  { op: "fadeOut", label: ["Wygaszanie", "Fade out"], params: [{ key: "curve", label: ["Krzywa", "Curve"], kind: "select", options: FADE_CURVES }], defaults: { curve: "sCurve" } },
  {
    op: "denoise",
    label: ["Redukcja szumu", "Noise reduction"],
    params: [db("reduction", ["Redukcja", "Reduction"], 0, 40), pct("sensitivity", ["Czułość", "Sensitivity"])],
    defaults: { reduction: 18, sensitivity: 0.5 },
  },
  {
    op: "voiceDenoise",
    label: ["Czysta mowa (AI)", "Clean voice (AI)"],
    params: [
      pct("strength", ["Usuwanie szumu", "Noise removal"]),
      pct("dereverb", ["Usuwanie pogłosu", "Reverb removal"]),
      { key: "room", label: ["Pogłos pomieszczenia", "Room reverb time"], min: 0.2, max: 2.5, step: 0.05, unit: "s" },
    ],
    defaults: { strength: 0.85, dereverb: 0.5, room: 0.6 },
  },
  {
    op: "dereverb",
    label: ["Usuwanie pogłosu", "Reverb removal"],
    params: [pct("amount", ["Siła", "Amount"]), { key: "room", label: ["Pogłos pomieszczenia", "Room reverb time"], min: 0.2, max: 2.5, step: 0.05, unit: "s" }],
    defaults: { amount: 0.6, room: 0.6 },
  },
  { op: "declick", label: ["Usuwanie trzasków", "Declick"], params: [pct("strength", ["Siła", "Strength"])], defaults: { strength: 0.5 } },
  {
    op: "declip",
    label: ["Napraw przester", "Declip"],
    params: [{ key: "tolerance", label: ["Tolerancja wykrywania", "Detection tolerance"], min: 0, max: 0.2, step: 0.005, unit: "%" }, db("gain", ["Wzmocnienie po naprawie", "Gain after repair"], -12, 0)],
    defaults: { tolerance: 0.01, gain: -3 },
  },
  {
    op: "dehum",
    label: ["Usuń przydźwięk", "Hum removal"],
    params: [
      { key: "base", label: ["Częstotliwość sieci (50 Europa, 60 USA)", "Mains frequency (50 Europe, 60 USA)"], min: 50, max: 60, step: 10, unit: "Hz" },
      { key: "harmonics", label: ["Harmoniczne", "Harmonics"], min: 1, max: 16, step: 1 },
      { key: "q", label: ["Wąskość filtra", "Notch narrowness"], min: 2, max: 100, step: 1, log: true },
    ],
    defaults: { base: 50, harmonics: 8, q: 30 },
  },
  {
    op: "deplosive",
    label: ["Głoski wybuchowe", "Plosive removal"],
    params: [hz("freq", ["Pasmo do", "Band up to"], 60, 400), pct("sensitivity", ["Czułość", "Sensitivity"]), db("reduction", ["Maks. redukcja", "Max reduction"], 0, 40)],
    defaults: { freq: 150, sensitivity: 0.6, reduction: 18 },
  },
  {
    op: "level",
    label: ["Wyrównaj głośność", "Leveler"],
    params: [
      db("target", ["Docelowy RMS", "Target RMS"], -40, -6),
      db("maxGain", ["Maks. zmiana", "Max change"], 0, 30),
      db("gate", ["Pomijaj ciszę poniżej", "Ignore silence below"], -90, -20),
      ms("speed", ["Szybkość reakcji", "Response time"], 50, 10000),
    ],
    defaults: { target: -20, maxGain: 12, gate: -50, speed: 800 },
  },
  { op: "pitch", label: ["Zmiana wysokości", "Pitch shift"], params: [{ key: "semitones", label: ["Półtony", "Semitones"], min: -24, max: 24, step: 0.1, unit: "st" }], defaults: { semitones: 2 } },
  { op: "stretch", label: ["Rozciąganie w czasie", "Time stretch"], params: [{ key: "ratio", label: ["Długość", "Length"], min: 0.25, max: 4, step: 0.01, unit: "×", log: true }], defaults: { ratio: 1.25 }, changesLength: true },
  { op: "speed", label: ["Prędkość (z wysokością)", "Speed (with pitch)"], params: [{ key: "ratio", label: ["Prędkość", "Speed"], min: 0.25, max: 4, step: 0.01, unit: "×", log: true }], defaults: { ratio: 1.1 }, changesLength: true },
  {
    op: "slidingStretch",
    label: ["Płynne tempo / wysokość", "Sliding stretch"],
    params: [
      { key: "tempoStart", label: ["Tempo na początku", "Tempo at start"], min: -90, max: 300, step: 1, unit: " %" },
      { key: "tempoEnd", label: ["Tempo na końcu", "Tempo at end"], min: -90, max: 300, step: 1, unit: " %" },
      { key: "pitchStart", label: ["Wysokość na początku", "Pitch at start"], min: -24, max: 24, step: 0.1, unit: "st" },
      { key: "pitchEnd", label: ["Wysokość na końcu", "Pitch at end"], min: -24, max: 24, step: 0.1, unit: "st" },
    ],
    defaults: { tempoStart: 0, tempoEnd: -30, pitchStart: 0, pitchEnd: 0 },
    changesLength: true,
  },
  {
    op: "paulstretch",
    label: ["Paulstretch", "Paulstretch"],
    params: [
      { key: "factor", label: ["Rozciągnięcie", "Stretch"], min: 1, max: 50, step: 0.1, unit: "×", log: true },
      { key: "window", label: ["Okno (gładkość)", "Window (smoothness)"], min: 0.05, max: 2, step: 0.01, unit: "s", log: true },
    ],
    defaults: { factor: 8, window: 0.25 },
    changesLength: true,
  },
  { op: "repeat", label: ["Powtórz", "Repeat"], params: [{ key: "count", label: ["Liczba powtórzeń", "Repeat count"], min: 1, max: 50, step: 1, unit: "×" }], defaults: { count: 1 }, changesLength: true },
  {
    op: "resample",
    label: ["Częstotliwość próbkowania", "Sample rate"],
    params: [
      { key: "rate", label: ["Częstotliwość", "Rate"], min: 4000, max: 96000, step: 1, unit: "Hz", log: true },
      { key: "reinterpret", label: ["Zmień prędkość (graj jak nagrane w tej częstotliwości)", "Change speed (play as if recorded at this rate)"], kind: "bool" },
    ],
    defaults: { rate: 8000, reinterpret: false },
    changesLength: true,
  },
  { op: "reverse", label: ["Odwróć w czasie", "Reverse"], params: [], defaults: {} },
  { op: "invert", label: ["Odwróć fazę", "Invert polarity"], params: [], defaults: {} },
  { op: "silence", label: ["Wycisz", "Silence"], params: [], defaults: {} },
  { op: "removeDc", label: ["Usuń składową stałą", "Remove DC offset"], params: [], defaults: {} },
  {
    op: "truncateSilence",
    label: ["Skróć pauzy", "Truncate silence"],
    params: [
      db("threshold", ["Próg ciszy", "Silence threshold"], -80, -20),
      ms("minLength", ["Skracaj pauzy dłuższe niż", "Shorten pauses longer than"], 100, 10000),
      ms("keep", ["Zostaw pauzę długości", "Leave a pause of"], 0, 5000, false),
    ],
    defaults: { threshold: -45, minLength: 600, keep: 350 },
    changesLength: true,
  },
  { op: "trimSilence", label: ["Przytnij ciszę na brzegach", "Trim edge silence"], params: [db("threshold", ["Próg", "Threshold"], -90, -20)], defaults: { threshold: -60 }, changesLength: true, whole: true },
  { op: "mono", label: ["Zamień na mono", "Convert to mono"], params: [], defaults: {}, whole: true },
  { op: "stereo", label: ["Zamień na stereo", "Convert to stereo"], params: [], defaults: {}, whole: true },
  { op: "swapChannels", label: ["Zamień kanały L/P", "Swap L/R"], params: [], defaults: {}, whole: true },
];
