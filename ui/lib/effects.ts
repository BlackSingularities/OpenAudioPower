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
  { op: "pitch", label: ["Zmiana wysokości", "Pitch shift"], params: [{ key: "semitones", label: ["Półtony", "Semitones"], min: -24, max: 24, step: 0.1, unit: "st" }], defaults: { semitones: 2 } },
  { op: "stretch", label: ["Rozciąganie w czasie", "Time stretch"], params: [{ key: "ratio", label: ["Długość", "Length"], min: 0.25, max: 4, step: 0.01, unit: "×", log: true }], defaults: { ratio: 1.25 }, changesLength: true },
  { op: "speed", label: ["Prędkość (z wysokością)", "Speed (with pitch)"], params: [{ key: "ratio", label: ["Prędkość", "Speed"], min: 0.25, max: 4, step: 0.01, unit: "×", log: true }], defaults: { ratio: 1.1 }, changesLength: true },
  { op: "reverse", label: ["Odwróć w czasie", "Reverse"], params: [], defaults: {} },
  { op: "invert", label: ["Odwróć fazę", "Invert polarity"], params: [], defaults: {} },
  { op: "silence", label: ["Wycisz", "Silence"], params: [], defaults: {} },
  { op: "removeDc", label: ["Usuń składową stałą", "Remove DC offset"], params: [], defaults: {} },
  { op: "trimSilence", label: ["Przytnij ciszę na brzegach", "Trim edge silence"], params: [db("threshold", ["Próg", "Threshold"], -90, -20)], defaults: { threshold: -60 }, changesLength: true, whole: true },
  { op: "mono", label: ["Zamień na mono", "Convert to mono"], params: [], defaults: {}, whole: true },
  { op: "stereo", label: ["Zamień na stereo", "Convert to stereo"], params: [], defaults: {}, whole: true },
  { op: "swapChannels", label: ["Zamień kanały L/P", "Swap L/R"], params: [], defaults: {}, whole: true },
];
