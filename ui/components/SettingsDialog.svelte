<script lang="ts">
  import { openPath } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import MiniWave from "./MiniWave.svelte";
  import { api, type DeviceList, type ModelStatus } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtBytes } from "../lib/format";
  import { openFeedback } from "../lib/feedback.js";
  import { S, app, tp, persist, applyAppearance, applyLanguage, setMetronome, ACCENTS } from "../lib/store.svelte";

  let devices = $state<DeviceList | null>(null);
  let models = $state<ModelStatus[]>([]);
  let section = $state<"audio" | "look" | "models" | "general">("audio");

  $effect(() => {
    if (app.dialogs.settings) {
      api.audioDevices().then((d) => (devices = d));
      api.models().then((m) => (models = m));
      api.inputMonitor(true);
      return () => {
        if (!tp.recording) api.inputMonitor(false);
      };
    }
  });

  async function setDevice(kind: "output" | "input", name: string) {
    S.settings.audio[kind] = name;
    persist();
    await api.setAudioDevice(kind, name || null);
    setTimeout(() => api.audioDevices().then((d) => (devices = d)), 400);
  }

  function look() {
    applyAppearance();
    persist();
  }
</script>

{#if app.dialogs.settings}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.settings = false)}>
    <div class="dialog glass-strong st" role="dialog" aria-modal="true">
      <nav>
        <h2>{t("settingsTitle")}</h2>
        {#each [["audio", "volume", "audio"], ["look", "sparkles", "appearance"], ["models", "stems", "models"], ["general", "settings", "language"]] as [id, ic, key]}
          <button class="nv" class:on={section === id} onclick={() => (section = id as any)}><Icon name={ic} size={16} />{t(key)}</button>
        {/each}
        <div class="grow"></div>
        <button class="nv" onclick={() => openFeedback()}><Icon name="bug" size={16} />{t("reportProblem")}</button>
      </nav>
      <div class="pane">
        {#if section === "audio"}
          <div class="f">
            <div class="label">{t("outputDevice")}</div>
            <select value={S.settings.audio.output} onchange={(e) => setDevice("output", (e.target as HTMLSelectElement).value)}>
              <option value="">{t("systemDefault")}{devices ? ` (${devices.default_output})` : ""}</option>
              {#each devices?.outputs ?? [] as d}<option value={d}>{d}</option>{/each}
            </select>
          </div>
          <div class="f">
            <div class="label">{t("inputDevice")}</div>
            <select value={S.settings.audio.input} onchange={(e) => setDevice("input", (e.target as HTMLSelectElement).value)}>
              <option value="">{t("systemDefault")}{devices ? ` (${devices.default_input})` : ""}</option>
              {#each devices?.inputs ?? [] as d}<option value={d}>{d}</option>{/each}
            </select>
            <div class="lvl">
              <span class="dim">{t("inputLevel")}</span>
              <div class="bar"><i style="width:{Math.min(100, Math.max(0, (20 * Math.log10(Math.max(1e-5, Math.max(tp.input[0], tp.input[1]))) + 60) / 60) * 100)}%"></i></div>
            </div>
          </div>
          <div class="f">
            <Slider label={t("metronomeVolume")} value={S.settings.metronome.volume} min={0} max={1} unit="%" def={0.7} onchange={(v) => ((S.settings.metronome.volume = v), setMetronome(), persist())} />
            <label class="ck"><input type="checkbox" bind:checked={S.settings.metronome.recOnly} onchange={() => (setMetronome(), persist())} />{t("countIn")}</label>
          </div>
          <p class="dim small">{t("sampleRateInfo")}: {app.rate} Hz{tp.error ? ` · ${tp.error}` : ""}</p>
        {:else if section === "look"}
          <div class="f">
            <div class="label">{t("accent")}</div>
            <div class="acc">
              {#each ACCENTS as a}
                <button class="sw" class:on={S.settings.accent === a} style="background:{a}" aria-label={a} onclick={() => ((S.settings.accent = a), look())}></button>
              {/each}
              <label class="sw pick" data-tip="RGB"><input type="color" value={S.settings.accent} oninput={(e) => ((S.settings.accent = (e.target as HTMLInputElement).value), look())} /></label>
            </div>
          </div>
          <label class="ck"><input type="checkbox" bind:checked={S.settings.glass} onchange={look} />{t("glass")}</label>
          <Slider label={t("glassBlur")} value={S.settings.blur} min={0} max={48} step={1} unit="px" def={22} onchange={(v) => ((S.settings.blur = v), look())} />
          <div class="wsec">
            <div class="label">{t("waveform")}</div>
            {#if app.sources.length}
              <div class="wprev"><MiniWave id={app.sources[0].id} color="#c4b5fd" height={64} /></div>
            {/if}
            <div class="wrow">
              <span>{t("waveScale")}</span>
              <div class="seg full">
                {#each [["relative", "waveRelative"], ["db", "waveDb"], ["linear", "waveLinear"]] as [v, k]}
                  <button class:on={S.settings.wave.scale === v} onclick={() => ((S.settings.wave.scale = v as any), persist())}>{t(k)}</button>
                {/each}
              </div>
            </div>
            <p class="dim small">{t("waveScaleHint")}</p>
            <div class="wrow">
              <span>{t("waveStyle")}</span>
              <div class="seg full">
                {#each [["fill", "waveFill"], ["rms", "waveRms"], ["bars", "waveBars"], ["outline", "waveOutline"]] as [v, k]}
                  <button class:on={S.settings.wave.style === v} onclick={() => ((S.settings.wave.style = v as any), persist())}>{t(k)}</button>
                {/each}
              </div>
            </div>
            <div class="wrow">
              <span>{t("waveColor")}</span>
              <div class="seg full">
                {#each [["track", "waveTrack"], ["spectral", "waveSpectral"], ["gradient", "waveGradient"]] as [v, k]}
                  <button class:on={S.settings.wave.color === v} onclick={() => ((S.settings.wave.color = v as any), persist())}>{t(k)}</button>
                {/each}
              </div>
            </div>
            <p class="dim small">{t("waveColorHint")}</p>
          </div>
        {:else if section === "models"}
          <p class="dim small">{t("aboutModels")}</p>
          {#each models as m}
            <div class="model">
              <div class="grow">
                <b>{m.name}</b>
                <div class="dim small">{m.legacy ? t("legacyModel") : `${m.stems.length} · ${fmtBytes(m.size)} · ${m.source}`}</div>
              </div>
              {#if m.installed}
                <span class="chip">{t("installed")}</span>
                <button class="btn sm ghost icon" onclick={() => api.deleteModel(m.id).then(() => api.models().then((x) => (models = x)))} aria-label={t("deleteModel")}><Icon name="trash" size={14} /></button>
              {:else}
                <span class="chip">{t("notInstalled")}</span>
              {/if}
            </div>
          {/each}
          <button class="btn sm" onclick={() => app.info && openPath(app.info.modelsDir).catch(() => {})}><Icon name="folder" size={14} />{t("modelsDir")}</button>
        {:else}
          <div class="f">
            <div class="label">{t("language")}</div>
            <div class="seg full">
              <button class:on={S.settings.lang === "auto"} onclick={() => ((S.settings.lang = "auto"), applyLanguage(), persist())}>{t("langAuto")}</button>
              <button class:on={S.settings.lang === "pl"} onclick={() => ((S.settings.lang = "pl"), applyLanguage(), persist())}>Polski</button>
              <button class:on={S.settings.lang === "en"} onclick={() => ((S.settings.lang = "en"), applyLanguage(), persist())}>English</button>
            </div>
          </div>
          <div class="f">
            <div class="label">{t("updates")}</div>
            <label class="ck"><input type="checkbox" bind:checked={S.settings.checkUpdates} onchange={persist} />{t("updAuto")}</label>
          </div>
        {/if}
        <div class="actions"><button class="btn primary" onclick={() => (app.dialogs.settings = false)}>{t("close")}</button></div>
      </div>
    </div>
  </div>
{/if}

<style>
  .st {
    width: min(760px, 100%);
    height: min(600px, calc(100vh - 48px));
    display: flex;
    gap: 0;
    padding: 0;
    overflow: hidden;
  }
  nav {
    width: 210px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 20px 10px;
    border-right: 1px solid var(--line);
  }
  nav h2 {
    padding: 0 10px 12px;
  }
  .nv {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    text-align: left;
  }
  .nv:hover {
    background: var(--glass-2);
    color: var(--text);
  }
  .nv.on {
    background: rgba(var(--accent-rgb), 0.22);
    color: #fff;
  }
  .pane {
    flex: 1;
    padding: 22px 22px 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
    overflow-y: auto;
  }
  .f {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .f select {
    width: 100%;
  }
  .ck {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--text-2);
    cursor: pointer;
  }
  .small {
    font-size: 12px;
    margin: 0;
    line-height: 1.45;
  }
  .lvl {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #10b981, #facc15 80%, #ef4444);
    transition: width 0.08s;
  }
  .acc {
    display: flex;
    gap: 9px;
    flex-wrap: wrap;
  }
  .sw {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    border: 0;
    cursor: pointer;
    position: relative;
  }
  .sw.on {
    box-shadow: 0 0 0 2px #fff, 0 0 14px currentColor;
  }
  .pick {
    background: conic-gradient(#f44, #fd4, #4f8, #4af, #a4f, #f44);
    overflow: hidden;
  }
  .pick input {
    opacity: 0;
    position: absolute;
    inset: 0;
    cursor: pointer;
  }
  .wsec {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .wprev {
    padding: 8px 10px;
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.3);
  }
  .wrow {
    display: grid;
    grid-template-columns: 70px 1fr;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--text-2);
  }
  .wrow .seg button {
    font-size: 11.5px;
    padding: 0 6px;
  }
  .model {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 11px;
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid var(--line);
  }
  .actions {
    margin-top: auto !important;
    position: sticky;
    bottom: 0;
    padding: 12px 0 20px;
    background: linear-gradient(180deg, transparent, var(--glass-strong) 40%);
  }
</style>
