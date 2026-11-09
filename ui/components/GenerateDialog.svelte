<script lang="ts">
  // Signal generator: tone, sweep, noise, DTMF, silence and click track, placed at the playhead.
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { t, lang } from "../lib/i18n.svelte";
  import { app, P, insertGenerated } from "../lib/store.svelte";

  type Kind = "tone" | "chirp" | "noise" | "dtmf" | "silence" | "clicks";
  const KINDS: { id: Kind; pl: string; en: string }[] = [
    { id: "tone", pl: "Ton", en: "Tone" },
    { id: "chirp", pl: "Przemiatanie", en: "Chirp" },
    { id: "noise", pl: "Szum", en: "Noise" },
    { id: "dtmf", pl: "DTMF", en: "DTMF" },
    { id: "clicks", pl: "Ścieżka klików", en: "Click track" },
    { id: "silence", pl: "Cisza", en: "Silence" },
  ];
  const WAVES = [
    { id: "sine", pl: "Sinus", en: "Sine" },
    { id: "square", pl: "Prostokąt", en: "Square" },
    { id: "saw", pl: "Piła", en: "Sawtooth" },
    { id: "triangle", pl: "Trójkąt", en: "Triangle" },
  ];
  const COLORS = [
    { id: "white", pl: "Biały", en: "White" },
    { id: "pink", pl: "Różowy", en: "Pink" },
    { id: "brown", pl: "Brązowy", en: "Brown" },
  ];
  const L = (x: { pl: string; en: string }) => (lang.current === "pl" ? x.pl : x.en);

  let kind = $state<Kind>("tone");
  let wave = $state("sine");
  let freq = $state(440);
  let f0 = $state(100);
  let f1 = $state(10000);
  let linear = $state(false);
  let color = $state("pink");
  let level = $state(-12);
  let digits = $state("0123456789");
  let toneMs = $state(120);
  let gapMs = $state(80);
  let seconds = $state(5);
  let stereo = $state(true);
  let bpm = $state(P.p.bpm);
  let beats = $state(P.p.beats);
  let busy = $state(false);

  $effect(() => {
    if (app.dialogs.generate) {
      bpm = P.p.bpm;
      beats = P.p.beats;
    }
  });

  function spec(): any {
    switch (kind) {
      case "tone":
        return { kind, wave, freq, level };
      case "chirp":
        return { kind, wave, f0, f1, level, linear };
      case "noise":
        return { kind, color, level };
      case "dtmf":
        return { kind, digits, toneMs, gapMs, level };
      case "clicks":
        return { kind, bpm, beats, level };
      default:
        return { kind: "silence" };
    }
  }
  function name(): string {
    const k = L(KINDS.find((x) => x.id === kind)!);
    if (kind === "tone") return `${k} ${freq} Hz`;
    if (kind === "chirp") return `${k} ${f0}–${f1} Hz`;
    if (kind === "noise") return `${k} ${L(COLORS.find((c) => c.id === color)!).toLowerCase()}`;
    if (kind === "clicks") return `${k} ${bpm} BPM`;
    return k;
  }
  // DTMF length follows the digits
  const dtmfSeconds = $derived((digits.length * (toneMs + gapMs)) / 1000);

  async function go() {
    busy = true;
    try {
      await insertGenerated(spec(), kind === "dtmf" ? Math.max(0.05, dtmfSeconds) : seconds, stereo, name());
      app.dialogs.generate = false;
    } finally {
      busy = false;
    }
  }
</script>

{#if app.dialogs.generate}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && (app.dialogs.generate = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>{t("generateTitle")}</h2>
      <p class="sub">{t("generateSub")}</p>
      <div class="seg full kinds">
        {#each KINDS as k}<button class:on={kind === k.id} onclick={() => (kind = k.id)}>{L(k)}</button>{/each}
      </div>
      <div class="form">
        {#if kind === "tone" || kind === "chirp"}
          <div class="row">
            <span>{t("waveform")}</span>
            <div class="seg">
              {#each WAVES as w}<button class:on={wave === w.id} onclick={() => (wave = w.id)}>{L(w)}</button>{/each}
            </div>
          </div>
        {/if}
        {#if kind === "tone"}
          <Slider label={t("frequency")} bind:value={freq} min={20} max={20000} step={1} unit="Hz" log def={440} />
        {:else if kind === "chirp"}
          <Slider label={t("fromFreq")} bind:value={f0} min={20} max={20000} step={1} unit="Hz" log def={100} />
          <Slider label={t("toFreq")} bind:value={f1} min={20} max={20000} step={1} unit="Hz" log def={10000} />
          <label class="chk"><input type="checkbox" bind:checked={linear} />{t("linearSweep")}</label>
        {:else if kind === "noise"}
          <div class="row">
            <span>{t("noiseColor")}</span>
            <div class="seg">
              {#each COLORS as c}<button class:on={color === c.id} onclick={() => (color = c.id)}>{L(c)}</button>{/each}
            </div>
          </div>
        {:else if kind === "dtmf"}
          <label class="row"><span>{t("dtmfDigits")}</span><input type="text" bind:value={digits} maxlength="64" /></label>
          <Slider label={t("toneLength")} bind:value={toneMs} min={20} max={1000} step={1} unit="ms" def={120} />
          <Slider label={t("gapLength")} bind:value={gapMs} min={0} max={1000} step={1} unit="ms" def={80} />
        {:else if kind === "clicks"}
          <Slider label="BPM" bind:value={bpm} min={20} max={300} step={1} def={120} />
          <Slider label={t("beatsPerBar")} bind:value={beats} min={1} max={12} step={1} def={4} />
        {/if}
        {#if kind !== "silence"}
          <Slider label={t("level")} bind:value={level} min={-60} max={0} step={0.1} unit="dB" def={-12} />
        {/if}
        {#if kind !== "dtmf"}
          <Slider label={t("duration")} bind:value={seconds} min={0.1} max={600} step={0.1} unit="s" log def={5} />
        {:else}
          <div class="row"><span>{t("duration")}</span><span class="mono">{dtmfSeconds.toFixed(2)} s</span></div>
        {/if}
        <label class="chk"><input type="checkbox" bind:checked={stereo} />{t("stereo")}</label>
      </div>
      <div class="actions">
        <button class="btn ghost" disabled={busy} onclick={() => (app.dialogs.generate = false)}>{t("cancel")}</button>
        <button class="btn primary lg" disabled={busy} onclick={go}><Icon name="tone" size={16} />{t("generate")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(560px, 100%);
  }
  .kinds {
    flex-wrap: wrap;
    margin-bottom: 16px;
  }
  .kinds button {
    flex: 1 1 auto;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .row input[type="text"] {
    flex: 1;
    max-width: 260px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
</style>
