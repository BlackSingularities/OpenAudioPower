<script lang="ts">
  import Icon from "./Icon.svelte";
  import Meter from "./Meter.svelte";
  import { t } from "../lib/i18n.svelte";
  import { openMenu, type MenuItem } from "../lib/menu.svelte";
  import {
    P, ed, tp, commit, beginGesture, endGesture, syncEngine, track as getTrack, soloOnly, toggleMute, setGainLinked,
    buses, setOutput, addSend, removeSend, addBusCmd, addReverbSend, routesInto, GROUP_COLORS, freezeTrack, unfreezeTrack, type Track,
  } from "../lib/store.svelte";

  let { height }: { height: number } = $props();
  const maxSends = $derived(Math.max(0, ...P.p.tracks.map((x) => (x.sends ?? []).length)));
  // routing row + one row per send (+ the "add send" row)
  const faderLen = $derived(Math.max(40, height - 118 - 46 - 20 * maxSends));

  function fader(tr: Track | null, e: Event) {
    const v = +(e.target as HTMLInputElement).value;
    if (tr) setGainLinked(tr.id, v);
    else {
      P.p.master.gain = v;
      syncEngine();
    }
  }
  function pan(tr: Track, e: Event) {
    getTrack(tr.id)!.pan = +(e.target as HTMLInputElement).value;
    syncEngine();
  }
  function sendGain(tr: Track, id: number, e: Event) {
    const s = getTrack(tr.id)!.sends!.find((x) => x.id === id)!;
    s.gain = +(e.target as HTMLInputElement).value;
    syncEngine();
  }
  const fmt = (g: number) => (g <= -59.9 ? "−∞" : (g > 0 ? "+" : "") + g.toFixed(1).replace("-", "−"));
  const busName = (id: number | undefined) => (id ? getTrack(id)?.name ?? t("master") : t("master"));

  function outputMenu(tr: Track, e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = [
      { label: t("master"), icon: tr.output ? undefined : "check", run: () => setOutput(tr.id, 0) },
      ...buses()
        .filter((b) => b.id !== tr.id)
        .map((b) => ({ label: b.name, icon: tr.output === b.id ? "check" : "bus", disabled: routesInto(tr.id, b.id), run: () => setOutput(tr.id, b.id) })),
      { sep: true },
      { label: t("addBus"), icon: "plus", run: () => setOutput(tr.id, addBusCmd()) },
    ];
    openMenu({ clientX: r.left, clientY: r.bottom + 2 }, items);
  }

  function sendMenu(tr: Track, e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = buses()
      .filter((b) => b.id !== tr.id)
      .map((b) => ({ label: b.name, icon: "bus", disabled: routesInto(tr.id, b.id), run: () => addSend(tr.id, b.id) }));
    if (items.length) items.push({ sep: true });
    if (!tr.bus) items.push({ label: t("reverbSend"), icon: "sparkles", run: () => addReverbSend([tr.id]) });
    items.push({ label: t("addBus"), icon: "plus", run: () => addSend(tr.id, addBusCmd()) });
    openMenu({ clientX: r.left, clientY: r.bottom + 2 }, items);
  }

  function sendItemMenu(tr: Track, id: number, e: MouseEvent) {
    e.preventDefault();
    const s = tr.sends!.find((x) => x.id === id)!;
    openMenu(e, [
      { label: s.pre ? t("sendPost") : t("sendPre"), icon: "send", run: () => commit(() => (getTrack(tr.id)!.sends!.find((x) => x.id === id)!.pre = !s.pre)) },
      { label: s.on ? t("disable") : t("enable"), icon: "power", run: () => commit(() => (getTrack(tr.id)!.sends!.find((x) => x.id === id)!.on = !s.on)) },
      { sep: true },
      { label: t("remove"), icon: "trash", danger: true, run: () => removeSend(tr.id, id) },
    ]);
  }

  function stripMenu(tr: Track, e: MouseEvent) {
    e.preventDefault();
    const sel = ed.selTrack && ed.selTrack !== tr.id ? [tr.id, ed.selTrack] : [tr.id];
    openMenu(e, [
      ...(tr.bus
        ? []
        : [tr.freeze ? { label: t("unfreeze"), icon: "snow", run: () => unfreezeTrack(tr.id) } : { label: t("freeze"), icon: "snow", disabled: !tr.clips.length, run: () => freezeTrack(tr.id) }]),
      { sep: true },
      {
        label: t("group"),
        icon: "link",
        children: [
          { label: t("noGroup"), icon: tr.group ? undefined : "check", run: () => commit(() => (getTrack(tr.id)!.group = 0)) },
          ...GROUP_COLORS.map((_, i) => ({ label: `${t("group")} ${i + 1}`, icon: tr.group === i + 1 ? "check" : "link", run: () => commit(() => sel.forEach((id) => (getTrack(id)!.group = i + 1))) })),
        ],
      },
    ]);
  }
</script>

<div class="mixer">
  {#each P.p.tracks as tr (tr.id)}
    {@const m = tp.meters[tr.id] ?? { l: 0, r: 0, gr: 0 }}
    <div class="strip" class:bus={tr.bus} class:sel={ed.selTrack === tr.id} style="--c:{tr.color}" role="button" tabindex="-1" onpointerdown={() => (ed.selTrack = tr.id)} oncontextmenu={(e) => stripMenu(tr, e)}>
      <div class="nm" title={tr.name}>
        {#if tr.bus}<Icon name="bus" size={11} />{:else}<span class="dot" style="background:{tr.color}"></span>{/if}
        <span class="tx">{tr.name}</span>
        {#if tr.freeze}<span class="frz" data-tip={t("frozen")}><Icon name="snow" size={11} /></span>{/if}
        {#if tr.group}<span class="grp" style="background:{GROUP_COLORS[(tr.group - 1) % GROUP_COLORS.length]}" data-tip="{t('group')} {tr.group}"></span>{/if}
      </div>
      <button class="route" onclick={(e) => outputMenu(tr, e)} data-tip={t("output")}>→ {busName(tr.output)}</button>
      <div class="sends">
        {#each tr.sends ?? [] as s (s.id)}
          <div class="send" class:off={!s.on} oncontextmenu={(e) => sendItemMenu(tr, s.id, e)} role="group" title="{busName(s.target)} · {s.pre ? 'pre' : 'post'} · {fmt(s.gain)} dB">
            <span class="sn">{busName(s.target)}</span>
            <input type="range" min="-60" max="6" step="0.1" value={s.gain} style="--fill:{((s.gain + 60) / 66) * 100}%" onpointerdown={beginGesture} oninput={(e) => sendGain(tr, s.id, e)} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.sends!.find((x) => x.id === s.id)!.gain = 0))} aria-label={t("sends")} />
          </div>
        {/each}
        <button class="addsend" onclick={(e) => sendMenu(tr, e)}><Icon name="send" size={10} />{t("send")}</button>
      </div>
      <input type="range" class="pan" min="-1" max="1" step="0.01" value={tr.pan} style="--fill:{((tr.pan + 1) / 2) * 100}%" onpointerdown={beginGesture} oninput={(e) => pan(tr, e)} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.pan = 0))} data-tip={t("pan")} />
      <div class="fz" style="height:{faderLen}px">
        <input type="range" class="fader" min="-60" max="12" step="0.1" value={tr.gain} style="--fill:{((tr.gain + 60) / 72) * 100}%;height:{faderLen}px" onpointerdown={beginGesture} oninput={(e) => fader(tr, e)} onchange={endGesture} ondblclick={() => (beginGesture(), setGainLinked(tr.id, 0), endGesture())} aria-label={t("volume")} />
        <Meter l={m.l} r={m.r} vertical length={faderLen} size={4} />
      </div>
      <div class="val mono">{fmt(tr.gain)}</div>
      <div class="btns">
        <button class="tb" class:m={tr.mute} onclick={() => toggleMute([tr.id])}>M</button>
        <button class="tb" class:s={tr.solo} onclick={(e) => soloOnly([tr.id], e.ctrlKey || e.metaKey)} data-tip={t("soloOnly")}>S</button>
        <button class="tb" class:fx={tr.effects.some((x) => x.on)} onclick={() => ((ed.selTrack = tr.id), (ed.inspectorTab = "track"))}>FX</button>
      </div>
    </div>
  {/each}
  <button class="newbus" onclick={() => addBusCmd()} data-tip={t("addBus")}><Icon name="plus" size={14} /><span>{t("bus")}</span></button>
  <div class="grow"></div>
  <span class="msep" aria-hidden="true"></span>
  <div class="strip master">
    <div class="nm"><Icon name="sliders" size={13} />{t("master")}</div>
    <div style="height:{46 + 20 * maxSends + 22}px"></div>
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
    overflow-y: auto;
    padding: 4px 0 6px;
  }
  /* a short dock scrolls instead of squeezing the strip's rows into each other */
  .strip > * {
    flex: none;
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
  .strip.bus {
    background: rgba(148, 163, 184, 0.07);
  }
  .strip.bus.sel {
    background: rgba(148, 163, 184, 0.16);
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
  .frz {
    flex: none;
    display: flex;
    color: #7dd3fc;
  }
  .grp {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 2px;
  }
  .route {
    width: 100%;
    height: 18px;
    border: 1px solid var(--line-2);
    border-radius: 5px;
    background: transparent;
    font-size: 10px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
    padding: 0 4px;
  }
  .route:hover,
  .addsend:hover {
    background: rgba(var(--accent-rgb), 0.18);
  }
  .sends {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .send {
    display: flex;
    flex-direction: column;
    height: 18px;
    justify-content: center;
  }
  .send.off {
    opacity: 0.4;
  }
  .send .sn {
    font-size: 9px;
    color: var(--text-3);
    line-height: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .send input {
    width: 100%;
    height: 9px;
  }
  .addsend {
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    border: 1px dashed var(--line-2);
    border-radius: 5px;
    background: transparent;
    font-size: 9.5px;
    color: var(--text-3);
    cursor: pointer;
  }
  .newbus {
    flex: none;
    align-self: flex-start;
    margin: 4px 8px;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 10px;
    border: 1px dashed var(--line-2);
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-size: 11px;
    cursor: pointer;
  }
  .newbus:hover {
    background: rgba(var(--accent-rgb), 0.15);
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
