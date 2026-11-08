<script lang="ts">
  import Icon from "./Icon.svelte";
  import Meter from "./Meter.svelte";
  import { t } from "../lib/i18n.svelte";
  import { P, ed, tp, commit, beginGesture, endGesture, syncEngine, track as getTrack, soloOnly, type Track } from "../lib/store.svelte";

  let { height }: { height: number } = $props();
  const faderLen = $derived(Math.max(60, height - 118));

  function fader(tr: Track | null, e: Event) {
    const v = +(e.target as HTMLInputElement).value;
    if (tr) getTrack(tr.id)!.gain = v;
    else P.p.master.gain = v;
    syncEngine();
  }
  function pan(tr: Track, e: Event) {
    getTrack(tr.id)!.pan = +(e.target as HTMLInputElement).value;
    syncEngine();
  }
  function toggle(tr: Track, k: "mute" | "solo") {
    commit(() => (getTrack(tr.id)![k] = !getTrack(tr.id)![k]));
  }
  const fmt = (g: number) => (g <= -59.9 ? "−∞" : (g > 0 ? "+" : "") + g.toFixed(1).replace("-", "−"));
</script>

<div class="mixer">
  {#each P.p.tracks as tr (tr.id)}
    {@const m = tp.meters[tr.id] ?? { l: 0, r: 0, gr: 0 }}
    <div class="strip" class:sel={ed.selTrack === tr.id} style="--c:{tr.color}" role="button" tabindex="-1" onpointerdown={() => (ed.selTrack = tr.id)}>
      <div class="nm" title={tr.name}><span class="dot" style="background:{tr.color}"></span><span class="tx">{tr.name}</span></div>
      <input type="range" class="pan" min="-1" max="1" step="0.01" value={tr.pan} style="--fill:{((tr.pan + 1) / 2) * 100}%" onpointerdown={beginGesture} oninput={(e) => pan(tr, e)} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.pan = 0))} data-tip={t("pan")} />
      <div class="fz" style="height:{faderLen}px">
        <input type="range" class="fader" min="-60" max="12" step="0.1" value={tr.gain} style="--fill:{((tr.gain + 60) / 72) * 100}%;height:{faderLen}px" onpointerdown={beginGesture} oninput={(e) => fader(tr, e)} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.gain = 0))} aria-label={t("volume")} />
        <Meter l={m.l} r={m.r} vertical length={faderLen} size={4} />
      </div>
      <div class="val mono">{fmt(tr.gain)}</div>
      <div class="btns">
        <button class="tb" class:m={tr.mute} onclick={() => toggle(tr, "mute")}>M</button>
        <button class="tb" class:s={tr.solo} onclick={(e) => soloOnly([tr.id], e.ctrlKey || e.metaKey)} data-tip={t("soloOnly")}>S</button>
        <button class="tb" class:fx={tr.effects.some((x) => x.on)} onclick={() => ((ed.selTrack = tr.id), (ed.inspectorTab = "track"))}>FX</button>
      </div>
    </div>
  {/each}
  <div class="grow"></div>
  <span class="msep" aria-hidden="true"></span>
  <div class="strip master">
    <div class="nm"><Icon name="sliders" size={13} />{t("master")}</div>
    <div style="height:22px"></div>
    <div class="fz" style="height:{faderLen}px">
      <input type="range" class="fader" min="-24" max="12" step="0.1" value={P.p.master.gain} style="--fill:{((P.p.master.gain + 24) / 36) * 100}%;height:{faderLen}px" onpointerdown={beginGesture} oninput={(e) => fader(null, e)} onchange={endGesture} ondblclick={() => commit(() => (P.p.master.gain = 0))} aria-label={t("master")} />
      <Meter l={tp.master[0]} r={tp.master[1]} vertical length={faderLen} size={6} clip={tp.clip} />
    </div>
    <div class="val mono">{fmt(P.p.master.gain)}</div>
    <div class="btns">
      <button class="tb" class:fx={P.p.master.effects.some((x) => x.on)} onclick={() => (ed.inspectorTab = "master")}>FX</button>
      {#if tp.masterGr > 0.3}<span class="gr mono">−{tp.masterGr.toFixed(1)}</span>{/if}
    </div>
  </div>
</div>

<style>
  .mixer {
    display: flex;
    height: 100%;
    overflow-x: auto;
    padding: 4px 0 6px;
  }
  .strip {
    flex: none;
    width: 84px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    border-right: 1px solid var(--line);
  }
  .strip.sel {
    background: color-mix(in srgb, var(--c) 10%, transparent);
  }
  .strip.master {
    width: 100px;
    border-right: 0;
    background: rgba(var(--accent-rgb), 0.07);
  }
  .nm .tx {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nm .dot {
    flex: none;
  }
  .nm {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    justify-content: center;
    min-width: 0;
  }
  .msep {
    width: 1px;
    flex: none;
    background: var(--line-2);
  }
  .pan {
    width: 64px;
    height: 16px;
  }
  .fz {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .fader {
    writing-mode: vertical-lr;
    direction: rtl;
    width: 20px;
  }
  .fader::-webkit-slider-runnable-track {
    width: 4px;
    height: 100%;
    background: linear-gradient(0deg, rgba(var(--accent-rgb), 0.95) var(--fill, 50%), rgba(255, 255, 255, 0.12) var(--fill, 50%));
  }
  .fader::-webkit-slider-thumb {
    width: 22px;
    height: 12px;
    border-radius: 4px;
    margin-top: 0;
    margin-left: -9px;
    background: linear-gradient(180deg, #fff, #cfcfe0);
  }
  .val {
    font-size: 11px;
    color: var(--text-2);
  }
  .btns {
    display: flex;
    gap: 3px;
    align-items: center;
  }
  .tb {
    height: 20px;
    min-width: 22px;
    border-radius: 5px;
    border: 1px solid var(--line-2);
    background: transparent;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-2);
    cursor: pointer;
    padding: 0 4px;
  }
  .tb.m {
    background: #f59e0b;
    color: #1a1204;
    border-color: transparent;
  }
  .tb.s {
    background: #38bdf8;
    color: #04121a;
    border-color: transparent;
  }
  .tb.fx {
    background: rgba(var(--accent-rgb), 0.5);
    color: #fff;
    border-color: transparent;
  }
  .gr {
    font-size: 10px;
    color: #fbbf24;
  }
</style>
