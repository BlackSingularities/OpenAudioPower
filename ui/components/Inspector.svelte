<script lang="ts">
  import Icon from "./Icon.svelte";
  import Knob from "./Knob.svelte";
  import Switch from "./Switch.svelte";
  import MiniWave from "./MiniWave.svelte";
  import Meter from "./Meter.svelte";
  import EffectChain from "./EffectChain.svelte";
  import ParamEditor from "./ParamEditor.svelte";
  import AnalysisPanel from "./AnalysisPanel.svelte";
  import { lang, t, stemLabel } from "../lib/i18n.svelte";
  import { fmtTime } from "../lib/format";
  import { EFFECTS, GROUPS, OPS, effectDef } from "../lib/effects";
  import {
    P, app, ed, tp, commit, beginGesture, endGesture, syncEngine, findClip, source, track as getTrack, selectedClips, applyOp, openStemDialog,
    toast, toggleMute, soloOnly, TRACK_COLORS, STEM_COLORS,
  } from "../lib/store.svelte";

  const L = (l: [string, string]) => (lang.current === "pl" ? l[0] : l[1]);
  const tabs = [
    { id: "clip", icon: "wave", key: "tabClip" },
    { id: "track", icon: "fx", key: "tabTrack" },
    { id: "process", icon: "wand", key: "tabProcessShort" },
    { id: "analysis", icon: "chart", key: "tabAnalysis" },
    { id: "master", icon: "sliders", key: "tabMaster" },
  ] as const;

  const sel = $derived(ed.selClips.length ? findClip(ed.selClips[0]) : undefined);
  const clip = $derived(sel?.clip);
  const ctrack = $derived(sel?.track);
  const src = $derived(clip ? source(clip.source) : undefined);
  const tr = $derived(getTrack(ed.selTrack));
  const tint = $derived(ctrack?.color ?? "#8b5cf6");

  // live edits during knob drags become one undo step
  function liveClip(key: string, v: any) {
    for (const { clip: c } of selectedClips()) (c as any)[key] = v;
    syncEngine();
  }
  function setClip(key: string, v: any) {
    commit(() => {
      for (const { clip: c } of selectedClips()) (c as any)[key] = v;
    });
  }
  function liveTrack(key: string, v: any) {
    if (!tr) return;
    (getTrack(tr.id) as any)[key] = v;
    syncEngine();
  }
  function setTrack(key: string, v: any) {
    if (!tr) return;
    commit(() => ((getTrack(tr.id) as any)[key] = v), { merge: "trk:" + key });
  }

  const sec = (f: number) => f / app.rate;
  const fmtS = (v: number) => (v < 10 ? v.toFixed(2) : v.toFixed(1));
  const fmtDb = (v: number) => (v <= -59.9 ? "−∞" : (v > 0 ? "+" : "") + v.toFixed(1).replace("-", "−"));
  const fmtPan = (v: number) => (Math.abs(v) < 0.005 ? "C" : (v < 0 ? "L" : "R") + Math.round(Math.abs(v) * 100));

  // ── process tab ──
  const OP_GROUPS: { label: [string, string]; ops: string[] }[] = [
    { label: ["Głośność", "Level"], ops: ["gain", "normalize", "fadeIn", "fadeOut"] },
    { label: ["Naprawa", "Repair"], ops: ["denoise", "declick", "declip", "dehum", "deplosive", "level", "removeDc", "truncateSilence", "trimSilence"] },
    { label: ["Czas i wysokość", "Time & pitch"], ops: ["pitch", "stretch", "speed", "slidingStretch", "paulstretch", "repeat", "resample", "reverse"] },
    { label: ["Kanały i faza", "Channels & phase"], ops: ["invert", "silence", "mono", "stereo", "swapChannels"] },
  ];
  const OP_ICON: Record<string, string> = {
    gain: "volume", normalize: "chart", fadeIn: "up", fadeOut: "down", denoise: "sparkles", declick: "wand", removeDc: "minus",
    trimSilence: "scissors", pitch: "music", stretch: "fit", speed: "right", reverse: "undo", invert: "refresh", silence: "mute",
    mono: "other", stereo: "headphones", swapChannels: "split", truncateSilence: "scissors", slidingStretch: "chart", paulstretch: "sparkles", repeat: "loop", resample: "wave",
  };
  const FX_ICON: Record<string, string> = { eq: "sliders", dyn: "chart", space: "sparkles", mod: "wave", color: "fx", util: "settings" };

  let opId = $state("normalize");
  let opValues = $state<Record<string, any>>({ ...OPS.find((o) => o.op === "normalize")!.defaults });
  let fxType = $state("reverb");
  let fxValues = $state<Record<string, any>>({ type: "reverb", ...structuredClone(effectDef("reverb")!.defaults) });
  let useRange = $state(true);
  let mode = $state<"op" | "fx">("op");
  const opDef = $derived(OPS.find((o) => o.op === opId)!);

  function pickOp(id: string) {
    opId = id;
    opValues = { ...structuredClone(OPS.find((o) => o.op === id)!.defaults) };
  }
  function pickFx(type: string) {
    fxType = type;
    fxValues = { type, ...structuredClone(effectDef(type)!.defaults) };
  }

  async function runProcess() {
    if (mode === "fx") {
      const def = effectDef(fxType)!;
      await applyOp({ op: "effect", spec: $state.snapshot(fxValues), tail: def.tail ?? 0 }, useRange && !!ed.sel);
      return;
    }
    const op: any = { op: opId, ...$state.snapshot(opValues) };
    if (opId === "denoise" && app.noiseProfile && clip && app.noiseProfile.source === clip.source) {
      op.profileStart = app.noiseProfile.start;
      op.profileEnd = app.noiseProfile.end;
    }
    await applyOp(op, useRange && !!ed.sel, !!opDef.changesLength, !!opDef.whole);
  }

  function setProfile() {
    if (!clip || !ed.sel) return;
    const a = clip.offset + Math.max(0, ed.sel.start - clip.start);
    const b = clip.offset + Math.min(clip.len, ed.sel.end - clip.start);
    if (b > a) {
      app.noiseProfile = { source: clip.source, start: a, end: b };
      toast(t("profileSet"), "ok");
    }
  }

</script>

<aside class="insp">
  <nav class="tabs">
    {#each tabs as tb}
      <button class="tab" class:on={ed.inspectorTab === tb.id} onclick={() => (ed.inspectorTab = tb.id)}>
        <Icon name={tb.icon} size={16} /><span>{t(tb.key)}</span>
      </button>
    {/each}
  </nav>

  <div class="content">
    {#if ed.inspectorTab === "clip"}
      {#if clip && src && ctrack}
        <header class="hero" style="--c:{tint}">
          <div class="crumb"><span class="dot" style="background:{tint}"></span>{ctrack.name}{#if ed.selClips.length > 1}<span class="cnt">+{ed.selClips.length - 1}</span>{/if}</div>
          <input class="title" type="text" value={clip.name} onchange={(e) => setClip("name", (e.target as HTMLInputElement).value)} spellcheck="false" />
          <div class="tags">
            {#if src.stem}<span class="tag" style="--t:{STEM_COLORS[src.stem]}"><Icon name={src.stem} size={11} />{stemLabel(src.stem)}</span>{/if}
            <span class="tag">{src.channels === 1 ? "MONO" : "STEREO"}</span>
            <span class="tag">{((src.meta.source_rate || src.rate) / 1000).toFixed(1)} kHz</span>
            {#if src.meta.codec}<span class="tag">{src.meta.codec.toUpperCase()}</span>{/if}
            {#if src.kind === "edit"}<span class="tag">{t("kind_edit")}</span>{/if}
          </div>
          <div class="well">
            <MiniWave id={clip.source} offset={clip.offset} len={clip.len} color={tint} height={58} fadeIn={clip.fadeIn} fadeOut={clip.fadeOut} gain={clip.gain}
              playhead={tp.pos >= clip.start && tp.pos <= clip.start + clip.len ? (tp.pos - clip.start) / clip.len : -1} />
          </div>
          <div class="stats">
            <div><span>{t("position")}</span><b class="mono">{fmtTime(clip.start, app.rate)}</b></div>
            <div><span>{t("length")}</span><b class="mono">{fmtTime(clip.len, app.rate)}</b></div>
            <div><span>{t("offset")}</span><b class="mono">{fmtTime(clip.offset, app.rate)}</b></div>
          </div>
        </header>

        <section class="sec">
          <h4>{t("gain")} · {t("fadeIn").toLowerCase()} · {t("fadeOut").toLowerCase()}</h4>
          <div class="knobs">
            <Knob label={t("gain")} value={clip.gain} min={-36} max={18} step={0.1} unit="dB" def={0} bipolar color={tint} format={fmtDb}
              onstart={beginGesture} onchange={(v) => liveClip("gain", v)} onend={endGesture} />
            <Knob label={t("fadeIn")} value={sec(clip.fadeIn)} min={0} max={Math.max(0.05, sec(clip.len - clip.fadeOut))} step={0.01} unit="s" def={0} color={tint} format={fmtS}
              onstart={beginGesture} onchange={(v) => liveClip("fadeIn", Math.round(v * app.rate))} onend={endGesture} />
            <Knob label={t("fadeOut")} value={sec(clip.fadeOut)} min={0} max={Math.max(0.05, sec(clip.len - clip.fadeIn))} step={0.01} unit="s" def={0} color={tint} format={fmtS}
              onstart={beginGesture} onchange={(v) => liveClip("fadeOut", Math.round(v * app.rate))} onend={endGesture} />
          </div>
          <Switch label={t("muteClip")} checked={clip.muted} onchange={(v) => setClip("muted", v)} />
        </section>

        <section class="sec">
          <h4>{t("tools")}</h4>
          <div class="tiles">
            <button class="tile stems" onclick={() => openStemDialog()}><span class="ti"><Icon name="stems" size={18} /></span>{t("separate")}</button>
            <button class="tile" onclick={() => (ed.inspectorTab = "process")}><span class="ti"><Icon name="wand" size={18} /></span>{t("tabProcess")}</button>
            <button class="tile" onclick={() => (ed.inspectorTab = "analysis")}><span class="ti"><Icon name="chart" size={18} /></span>{t("tabAnalysis")}</button>
          </div>
        </section>
      {:else}
        <div class="blank"><Icon name="wave" size={30} stroke={1.3} /><p>{t("noSelection")}</p></div>
      {/if}

    {:else if ed.inspectorTab === "track"}
      {#if tr}
        <header class="hero" style="--c:{tr.color}">
          <input class="title" type="text" value={tr.name} onchange={(e) => setTrack("name", (e.target as HTMLInputElement).value)} spellcheck="false" />
          <div class="colors">
            {#each TRACK_COLORS as c}
              <button class="cdot" class:on={tr.color === c} style="background:{c}" aria-label={c} onclick={() => setTrack("color", c)}></button>
            {/each}
          </div>
        </header>
        <section class="sec">
          <div class="mixrow">
            <Knob label={t("volume")} value={tr.gain} min={-60} max={12} step={0.1} unit="dB" def={0} bipolar color={tr.color} format={fmtDb} size={58}
              onstart={beginGesture} onchange={(v) => liveTrack("gain", v)} onend={endGesture} />
            <Knob label={t("pan")} value={tr.pan} min={-1} max={1} step={0.01} def={0} bipolar color={tr.color} format={fmtPan} size={58}
              onstart={beginGesture} onchange={(v) => liveTrack("pan", v)} onend={endGesture} />
            <div class="meterbox"><Meter l={tp.meters[tr.id]?.l ?? 0} r={tp.meters[tr.id]?.r ?? 0} vertical length={78} size={5} /></div>
          </div>
          <div class="quick">
            <button class="qb" class:mute={tr.mute} onclick={() => toggleMute([tr.id])}><Icon name={tr.mute ? "mute" : "volume"} size={15} />{t("mute")}</button>
            <button class="qb" class:solo={tr.solo} onclick={(e) => soloOnly([tr.id], e.ctrlKey)} data-tip={t("soloOnly")}><Icon name="headphones" size={15} />{t("solo")}</button>
            <button class="qb" class:rec={tr.armed} onclick={() => commit(() => P.p.tracks.forEach((x) => (x.armed = x.id === tr!.id ? !x.armed : false)))}><Icon name="record" size={13} />REC</button>
          </div>
        </section>
        <section class="sec">
          <EffectChain target={tr} />
        </section>
      {:else}
        <div class="blank"><Icon name="fx" size={30} stroke={1.3} /><p>{t("noTrack")}</p></div>
      {/if}

    {:else if ed.inspectorTab === "process"}
      <header class="hero plain">
        <div class="subtabs">
          <button class="tab" class:on={mode === "op"} onclick={() => (mode = "op")}><Icon name="wand" size={16} /><span>{t("tabProcess")}</span></button>
          <button class="tab" class:on={mode === "fx"} onclick={() => (mode = "fx")}><Icon name="fx" size={16} /><span>{t("effects")}</span></button>
        </div>
        <div class="seg full">
          <button class:on={!useRange || !ed.sel} onclick={() => (useRange = false)}>{t("processWhole")}</button>
          <button class:on={useRange && !!ed.sel} disabled={!ed.sel} onclick={() => (useRange = true)}>{t("processRange")}</button>
        </div>
      </header>
      {#if mode === "op"}
        <div class="opscroll">
        {#each OP_GROUPS as g}
          <section class="sec tight">
            <h4>{L(g.label)}</h4>
            <div class="opgrid">
              {#each g.ops as id}
                {@const o = OPS.find((x) => x.op === id)!}
                <button class="op" class:on={id === opId} onclick={() => pickOp(id)}><Icon name={OP_ICON[id] ?? "wand"} size={14} /><span>{L(o.label)}</span></button>
              {/each}
            </div>
          </section>
        {/each}
        </div>
        <section class="sec params">
          <h4 class="accent">{L(opDef.label)}</h4>
          {#if opDef.params.length}
            <ParamEditor params={opDef.params} values={opValues} defaults={opDef.defaults} onchange={(k, v) => (opValues[k] = v)} />
          {/if}
          {#if opId === "denoise"}
            <div class="note">
              <span>{app.noiseProfile && clip && app.noiseProfile.source === clip.source ? t("profileSet") : t("profileAuto")}</span>
              <button class="btn sm" disabled={!ed.sel || !clip} onclick={setProfile}>{t("useSelectionAsProfile")}</button>
            </div>
          {/if}
        </section>
      {:else}
        <div class="opscroll">
        {#each GROUPS as g}
          <section class="sec tight">
            <h4>{L(g.label)}</h4>
            <div class="opgrid">
              {#each EFFECTS.filter((d) => d.group === g.id) as d}
                <button class="op" class:on={d.type === fxType} onclick={() => pickFx(d.type)}><Icon name={FX_ICON[g.id]} size={14} /><span>{L(d.label)}</span></button>
              {/each}
            </div>
          </section>
        {/each}
        </div>
        <section class="sec params">
          <h4 class="accent">{L(effectDef(fxType)!.label)}</h4>
          <ParamEditor params={effectDef(fxType)!.params} values={fxValues} defaults={effectDef(fxType)!.defaults} onchange={(k, v) => (fxValues[k] = v)} />
        </section>
      {/if}
      <div class="applybar">
        <button class="btn primary wide lg" disabled={!!app.job || (!ed.selClips.length && !ed.sel)} onclick={runProcess}>
          <Icon name="wand" size={16} />{app.job?.kind === "process" ? t("applying") : t("apply")}
        </button>
        <p class="hint">{t("processHint")}</p>
      </div>

    {:else if ed.inspectorTab === "analysis"}
      {#if clip}
        <header class="hero plain">
          <div class="target"><span>{clip.name}</span></div>
        </header>
      {/if}
      <section class="sec"><AnalysisPanel /></section>

    {:else if ed.inspectorTab === "master"}
      <header class="hero" style="--c:var(--accent)">
        <div class="mixrow">
          <Knob label={t("volume")} value={P.p.master.gain} min={-24} max={12} step={0.1} unit="dB" def={0} bipolar format={fmtDb} size={64}
            onstart={beginGesture} onchange={(v) => ((P.p.master.gain = v), syncEngine())} onend={endGesture} />
          <div class="meterbox big"><Meter l={tp.master[0]} r={tp.master[1]} vertical length={90} size={7} clip={tp.clip} /></div>
          <div class="grstat">
            <span>{t("reduction")}</span>
            <b class="mono">{tp.masterGr > 0.1 ? "−" + tp.masterGr.toFixed(1) : "0.0"} dB</b>
          </div>
        </div>
      </header>
      <section class="sec">
        <EffectChain target={null} />
      </section>
    {/if}
  </div>
</aside>

<style>
  .insp {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  .tab {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 9px 2px 8px;
    border: 0;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    font-size: 10.5px;
    letter-spacing: 0.2px;
    min-width: 0;
    transition: color 0.15s, background 0.15s;
  }
  .tab span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.on {
    color: #fff;
    background: linear-gradient(180deg, rgba(var(--accent-rgb), 0.18), transparent);
  }
  .tab.on::after {
    content: "";
    position: absolute;
    left: 20%;
    right: 20%;
    top: 0;
    height: 2px;
    border-radius: 0 0 2px 2px;
    background: var(--accent);
    box-shadow: 0 0 10px var(--accent);
  }
  .tab.on :global(svg) {
    color: var(--accent);
    filter: drop-shadow(0 0 6px rgba(var(--accent-rgb), 0.7));
  }
  .subtabs {
    display: flex;
    margin: 2px -16px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .hero > .subtabs:first-child {
    margin-top: -14px;
    border-top: 0;
  }
  .subtabs .tab {
    flex-direction: row;
    justify-content: center;
    gap: 8px;
    padding: 11px 6px;
    font-size: 12px;
  }
  .content {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  /* hero header of each tab */
  .hero {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 14px 16px 14px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--c, var(--accent)) 16%, transparent), transparent 85%);
    border-bottom: 1px solid var(--line);
  }
  .hero.plain {
    background: none;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10.5px;
    letter-spacing: 0.7px;
    text-transform: uppercase;
    color: var(--text-3);
    font-weight: 600;
  }
  .crumb .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    box-shadow: 0 0 8px currentColor;
  }
  .cnt {
    margin-left: 4px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--glass-3);
    color: var(--text-2);
    font-size: 10px;
    letter-spacing: 0;
  }
  .title {
    height: 34px !important;
    padding: 0 8px !important;
    margin: 0 -8px;
    border: 1px solid transparent !important;
    background: transparent !important;
    font: 650 18px var(--font-display) !important;
    letter-spacing: -0.2px;
    box-shadow: none !important;
  }
  .title:hover {
    border-color: var(--line) !important;
  }
  .title:focus {
    border-color: rgba(var(--accent-rgb), 0.7) !important;
    background: rgba(0, 0, 0, 0.25) !important;
  }
  .tags {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 19px;
    padding: 0 7px;
    border-radius: 5px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.4px;
    color: var(--t, var(--text-2));
    background: color-mix(in srgb, var(--t, #ffffff) 12%, transparent);
  }
  .well {
    padding: 6px 8px;
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.32);
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .stats div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .stats span {
    font-size: 10px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stats b {
    font-weight: 500;
    font-size: 12px;
  }

  .sec {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
  }
  .sec.tight {
    gap: 4px;
    padding: 6px 12px 7px;
  }
  .sec.tight h4 {
    font-size: 9.5px;
    letter-spacing: 0.7px;
  }
  h4 {
    margin: 0;
    display: flex;
    align-items: center;
    font-size: 10.5px;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    color: var(--text-3);
    font-weight: 650;
  }
  h4.accent {
    color: #fff;
    font-size: 12px;
    letter-spacing: 0.3px;
    text-transform: none;
  }
  .knobs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 2px 0 4px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 7px;
    padding: 12px 4px 10px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.025);
    font-size: 11.5px;
    color: var(--text-2);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, transform 0.12s, border-color 0.15s;
  }
  .tile:hover {
    color: #fff;
    background: var(--glass-2);
    border-color: var(--line-2);
    transform: translateY(-1px);
  }
  .ti {
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(var(--accent-rgb), 0.18);
    color: #ddd6fe;
  }
  .tile.stems .ti {
    background: linear-gradient(135deg, #f472b6, var(--accent), #22d3ee);
    color: #fff;
    box-shadow: 0 4px 14px rgba(var(--accent-rgb), 0.4);
  }
  .colors {
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
  }
  .cdot {
    width: 18px;
    height: 18px;
    border-radius: 6px;
    border: 0;
    cursor: pointer;
    transition: transform 0.12s;
  }
  .cdot:hover {
    transform: scale(1.15);
  }
  .cdot.on {
    box-shadow: 0 0 0 2px var(--bg-solid), 0 0 0 4px #fff;
  }
  .mixrow {
    display: flex;
    align-items: center;
    justify-content: space-around;
    gap: 8px;
  }
  .meterbox {
    padding: 6px 8px;
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.3);
  }
  .grstat {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10.5px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .grstat b {
    font-size: 15px;
    color: #fbbf24;
    text-transform: none;
  }
  .quick {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .qb {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    border-radius: 9px;
    border: 1px solid var(--line-2);
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .qb:hover {
    color: #fff;
    background: var(--glass-2);
  }
  .qb.mute {
    background: #f59e0b;
    border-color: transparent;
    color: #1a1204;
  }
  .qb.solo {
    background: #38bdf8;
    border-color: transparent;
    color: #04121a;
  }
  .qb.rec {
    background: var(--rec);
    border-color: transparent;
    color: #fff;
    box-shadow: 0 0 12px rgba(239, 68, 68, 0.5);
  }
  .target {
    font-size: 13px;
    color: var(--text);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .opscroll {
    flex: none;
    max-height: 210px;
    overflow-y: auto;
    border-bottom: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.12);
    scrollbar-gutter: stable;
  }
  .opscroll .sec.tight:last-child {
    border-bottom: 0;
  }
  .opgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
  }
  .op {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 23px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-2);
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    min-width: 0;
    transition: background 0.12s, color 0.12s;
  }
  .op span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .op :global(svg) {
    flex: none;
    width: 12px;
    height: 12px;
    color: var(--text-3);
  }
  .op:hover {
    background: var(--glass-2);
    color: #fff;
  }
  .op.on {
    background: rgba(var(--accent-rgb), 0.22);
    border-color: rgba(var(--accent-rgb), 0.6);
    color: #fff;
  }
  .op.on :global(svg) {
    color: var(--accent);
  }
  .params {
    background: linear-gradient(180deg, rgba(var(--accent-rgb), 0.07), transparent);
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .applybar {
    position: sticky;
    bottom: 0;
    padding: 12px 16px 14px;
    background: linear-gradient(180deg, transparent, var(--glass-strong) 30%);
    backdrop-filter: blur(8px);
    margin-top: auto;
  }
  .hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--text-3);
    line-height: 1.45;
  }
  .blank {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px 20px;
    color: var(--text-3);
    text-align: center;
  }
  .blank p {
    margin: 0;
    max-width: 200px;
    line-height: 1.5;
  }
</style>
