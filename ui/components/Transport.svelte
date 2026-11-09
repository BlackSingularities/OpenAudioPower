<script lang="ts">
  import Icon from "./Icon.svelte";
  import Meter from "./Meter.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fmtTime, fmtBars } from "../lib/format";
  import {
    S, P, app, ed, tp, hist, play, stop, toStart, toEnd, startRecording, toggleLoop, setMetronome, commit, persist, undo, redo,
    zoomAt, zoomFit, separateCmd, cancelJob,
  } from "../lib/store.svelte";

  let bpmDraft = $state("");
  let editingBpm = $state(false);

  function setBpm() {
    editingBpm = false;
    const v = parseFloat(bpmDraft.replace(",", "."));
    if (isFinite(v) && v >= 20 && v <= 400) {
      commit(() => (P.p.bpm = Math.round(v * 100) / 100));
      setMetronome();
    }
  }

  function setBeats(e: Event) {
    commit(() => (P.p.beats = +(e.target as HTMLSelectElement).value));
    setMetronome();
  }

  // tap tempo
  let taps: number[] = [];
  function tap() {
    const now = performance.now();
    taps = taps.filter((x) => now - x < 2500);
    taps.push(now);
    if (taps.length >= 3) {
      const d = (taps[taps.length - 1] - taps[0]) / (taps.length - 1);
      commit(() => (P.p.bpm = Math.round(60000 / d)), { merge: "tap" });
      setMetronome();
    }
  }

  const jobLabel = $derived(app.job ? t("job_" + app.job.kind) : "");
</script>

<div class="bar">
  <div class="group">
    <button class="btn icon ghost" onclick={undo} disabled={!hist.canUndo} data-tip="{t('undo')} (Ctrl+Z)"><Icon name="undo" size={17} /></button>
    <button class="btn icon ghost" onclick={redo} disabled={!hist.canRedo} data-tip="{t('redo')} (Ctrl+Y)"><Icon name="redo" size={17} /></button>
  </div>

    <span class="vsep" aria-hidden="true"></span>
  <div class="transport">
    <button class="tbtn" onclick={toStart} data-tip="{t('toStart')} (Home)"><Icon name="toStart" size={17} /></button>
    <button class="tbtn play" class:on={tp.playing && !tp.recording} onclick={play} data-tip="{tp.playing ? t('pause') : t('play')} (Space)">
      <Icon name={tp.playing && !tp.recording ? "pause" : "play"} size={20} />
    </button>
    <button class="tbtn" onclick={stop} data-tip={t("stop")}><Icon name="stop" size={16} /></button>
    <button class="tbtn rec" class:on={tp.recording} onclick={startRecording} data-tip="{t('record')} (R)"><Icon name="record" size={17} /></button>
    <button class="tbtn" onclick={toEnd} data-tip="{t('toEnd')} (End)"><Icon name="toEnd" size={17} /></button>
    <span class="sep"></span>
    <button class="tbtn" class:on2={P.p.loop.on} onclick={toggleLoop} data-tip="{t('loop')} (L)"><Icon name="loop" size={16} /></button>
    <button class="tbtn" class:on2={tp.metronome} onclick={() => setMetronome(!tp.metronome)} data-tip="{t('metronome')} (K)"><Icon name="metronome" size={16} /></button>
    <button class="tbtn punch" class:on2={S.settings.punch} class:timer={!!app.timerRec} onclick={() => (app.dialogs.timer = true)} oncontextmenu={(e) => (e.preventDefault(), (S.settings.punch = !S.settings.punch), persist())} data-tip={t("recOptionsTip")}>
      {#if app.timerRec}<Icon name="clock" size={15} />{:else}<span class="pi">PI</span>{/if}
    </button>
  </div>

    <span class="vsep" aria-hidden="true"></span>
  <div class="clock" class:rec={tp.recording}>
    <div class="main mono">{S.settings.timeMode === "bars" ? fmtBars(tp.pos, app.rate, P.p.bpm, P.p.beats) : fmtTime(tp.pos, app.rate)}</div>
    <div class="subline mono">
      {S.settings.timeMode === "bars" ? fmtTime(tp.pos, app.rate) : fmtBars(tp.pos, app.rate, P.p.bpm, P.p.beats)}
      · {(app.rate / 1000).toFixed(1)} kHz
    </div>
  </div>

    <span class="vsep" aria-hidden="true"></span>
  <div class="tempo">
    {#if editingBpm}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="bpm-in mono" type="text" bind:value={bpmDraft} autofocus onblur={setBpm} onkeydown={(e) => e.key === "Enter" && setBpm()} />
    {:else}
      <button class="bpm mono" onclick={() => ((bpmDraft = String(P.p.bpm)), (editingBpm = true))} data-tip={t("bpm")}>{P.p.bpm}</button>
    {/if}
    <span class="unit">BPM</span>
    <button class="btn sm ghost" onclick={tap} data-tip="Tap tempo">TAP</button>
    <select value={P.p.beats} onchange={setBeats} data-tip={t("beats")}>
      {#each [2, 3, 4, 5, 6, 7, 12] as b}<option value={b}>{b}/4</option>{/each}
    </select>
  </div>

  <span class="vsep" aria-hidden="true"></span>
  <div class="group">
    <button class="btn icon ghost" class:on={S.settings.snap} onclick={() => ((S.settings.snap = !S.settings.snap), persist())} data-tip="{t('snap')} (G)"><Icon name="magnet" size={16} /></button>
    <button class="btn icon ghost" onclick={() => zoomAt(1.5)} data-tip={t("zoomOut")}><Icon name="zoomOut" size={16} /></button>
    <button class="btn icon ghost" onclick={() => zoomAt(1 / 1.5)} data-tip={t("zoomIn")}><Icon name="zoomIn" size={16} /></button>
    <button class="btn icon ghost" onclick={zoomFit} data-tip="{t('zoomFit')} (Ctrl+0)"><Icon name="fit" size={16} /></button>
  </div>

  <div class="grow"></div>

  {#if app.job}
    <div class="job" role="status">
      <div class="jl">
        <span>{jobLabel}{app.job.label ? " · " + app.job.label : ""}</span>
        <span class="mono">{Math.round(app.job.fraction * 100)}%</span>
      </div>
      <div class="pb"><i style="width:{Math.max(3, app.job.fraction * 100)}%"></i></div>
      <button class="btn sm ghost icon x" onclick={cancelJob} aria-label={t("cancel")} data-tip={t("cancel")}><Icon name="x" size={13} /></button>
    </div>
    <span class="vsep" aria-hidden="true"></span>
  {/if}

  <button class="btn primary stems" onclick={separateCmd} data-tip={t("stemSeparation")}>
    <Icon name="stems" size={16} />{t("separate")}
  </button>

    <span class="vsep" aria-hidden="true"></span>
  <div class="master" data-tip={t("master")}>
    {#if tp.inputOpen || tp.recording}
      <div class="in"><Icon name="mic" size={13} /><Meter l={tp.input[0]} r={tp.input[1]} length={70} size={3} /></div>
    {/if}
    <div class="mm">
      <Meter l={tp.master[0]} r={tp.master[1]} length={130} size={4} clip={tp.clip} />
    </div>
    {#if tp.clip}<button class="clipb" onclick={() => (tp.clip = false)} aria-label="clip">CLIP</button>{/if}
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 12px;
    height: 56px;
    flex: none;
    min-width: 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .vsep {
    flex: none;
    width: 1px;
    align-self: stretch;
    margin: 6px 0;
    background: linear-gradient(180deg, transparent, var(--line-2) 20%, var(--line-2) 80%, transparent);
  }
  .group {
    display: flex;
    gap: 2px;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .tbtn {
    width: 36px;
    height: 34px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, box-shadow 0.15s;
  }
  .tbtn.punch .pi {
    font: 700 10px var(--mono);
    letter-spacing: 0.04em;
  }
  .tbtn.timer {
    color: #f87171;
  }
  .tbtn:hover {
    background: var(--glass-3);
    color: var(--text);
  }
  .tbtn.play {
    width: 44px;
    color: var(--text);
  }
  .tbtn.play.on {
    background: rgba(var(--accent-rgb), 0.9);
    color: #fff;
    box-shadow: 0 0 18px rgba(var(--accent-rgb), 0.55);
  }
  .tbtn.rec {
    color: #f87171;
  }
  .tbtn.rec.on {
    background: var(--rec);
    color: #fff;
    box-shadow: 0 0 18px rgba(239, 68, 68, 0.7);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .tbtn.on2 {
    color: #fff;
    background: rgba(var(--accent-rgb), 0.32);
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 6px rgba(239, 68, 68, 0.4);
    }
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--line-2);
    margin: 0 4px;
  }
  .clock {
    padding: 0 6px;
    min-width: 160px;
    text-align: center;
  }
  .clock .main {
    font-size: 21px;
    font-weight: 600;
    letter-spacing: 0.5px;
    line-height: 1.15;
    color: #fff;
    text-shadow: 0 0 18px rgba(var(--accent-rgb), 0.55);
  }
  .clock.rec .main {
    color: #fecaca;
    text-shadow: 0 0 18px rgba(239, 68, 68, 0.7);
  }
  .clock .subline {
    font-size: 10.5px;
    color: var(--text-3);
  }
  .tempo {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 44px;
  }
  .bpm {
    border: 0;
    background: transparent;
    font-size: 17px;
    font-weight: 600;
    cursor: text;
    padding: 0 4px;
    border-radius: 6px;
  }
  .bpm:hover {
    background: var(--glass-2);
  }
  .bpm-in {
    width: 60px;
    height: 26px !important;
  }
  .unit {
    font-size: 10px;
    color: var(--text-3);
    letter-spacing: 0.6px;
  }
  .tempo select {
    height: 26px;
    font-size: 12px;
  }
  .job {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: 240px;
    padding: 0 30px 0 0;
  }
  .jl {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
  }
  .jl span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pb {
    height: 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
    overflow: hidden;
  }
  .pb i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, rgba(var(--accent-rgb), 0.7), var(--accent));
    box-shadow: 0 0 10px var(--accent);
    transition: width 0.25s;
  }
  .job .x {
    position: absolute;
    right: 5px;
    top: 50%;
    transform: translateY(-50%);
  }
  .stems {
    height: 38px;
    border-radius: 12px;
    font-weight: 600;
  }
  .master {
    display: flex;
    flex-direction: column;
    gap: 4px;
    justify-content: center;
    height: 44px;
    position: relative;
  }
  .in {
    display: flex;
    align-items: center;
    gap: 5px;
    color: #fca5a5;
  }
  .clipb {
    position: absolute;
    right: -6px;
    top: -6px;
    font-size: 9px;
    font-weight: 800;
    padding: 1px 5px;
    border-radius: 5px;
    border: 0;
    background: var(--err);
    color: #fff;
    cursor: pointer;
  }
</style>
