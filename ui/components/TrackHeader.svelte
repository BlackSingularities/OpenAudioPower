<script lang="ts">
  import Icon from "./Icon.svelte";
  import Meter from "./Meter.svelte";
  import { t } from "../lib/i18n.svelte";
  import { openMenu } from "../lib/menu.svelte";
  import { type Track, P, ed, tp, commit, beginGesture, endGesture, syncEngine, removeTrack, TRACK_COLORS, track as getTrack, openStemDialog, toggleMute, soloOnly } from "../lib/store.svelte";

  let { tr }: { tr: Track } = $props();

  let renaming = $state(false);
  let draft = $state("");
  const selected = $derived(ed.selTrack === tr.id);
  const m = $derived(tp.meters[tr.id] ?? { l: 0, r: 0, gr: 0 });

  function toggle(key: "mute" | "solo" | "armed") {
    commit(() => {
      const x = getTrack(tr.id)!;
      if (key === "armed") {
        const v = !x.armed;
        P.p.tracks.forEach((o) => (o.armed = false));
        x.armed = v;
      } else x[key] = !x[key];
    });
  }

  function startRename() {
    draft = tr.name;
    renaming = true;
  }
  function finishRename() {
    renaming = false;
    const v = draft.trim();
    if (v && v !== tr.name) commit(() => (getTrack(tr.id)!.name = v));
  }

  function cycleColor() {
    const i = TRACK_COLORS.indexOf(tr.color);
    commit(() => (getTrack(tr.id)!.color = TRACK_COLORS[(i + 1) % TRACK_COLORS.length]), { merge: "color" + tr.id });
  }

  function move(d: number) {
    commit(() => {
      const i = P.p.tracks.findIndex((x) => x.id === tr.id);
      const j = i + d;
      if (j < 0 || j >= P.p.tracks.length) return;
      const [x] = P.p.tracks.splice(i, 1);
      P.p.tracks.splice(j, 0, x);
    });
  }

  function menu(e: MouseEvent) {
    openMenu(e, [
      { label: t("renameTrack"), icon: "file", run: startRename },
      { label: t("moveUp"), icon: "up", run: () => move(-1) },
      { label: t("moveDown"), icon: "down", run: () => move(1) },
      { sep: true },
      { label: t("effects"), icon: "fx", run: () => ((ed.selTrack = tr.id), (ed.inspectorTab = "track")) },
      { label: t("stemSeparation"), icon: "stems", disabled: !tr.clips.length, run: () => ((ed.selClips = [tr.clips[0].id]), openStemDialog()) },
      { sep: true },
      { label: t("removeTrack"), icon: "trash", danger: true, run: () => removeTrack(tr.id) },
    ]);
  }

  // live volume drag (one undo step)
  function vol(e: Event) {
    getTrack(tr.id)!.gain = +(e.target as HTMLInputElement).value;
    syncEngine();
  }
  function pan(e: Event) {
    getTrack(tr.id)!.pan = +(e.target as HTMLInputElement).value;
    syncEngine();
  }

  // height resize
  function resizeStart(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const y0 = e.clientY;
    const h0 = tr.height;
    beginGesture();
    const mv = (ev: PointerEvent) => (getTrack(tr.id)!.height = Math.max(56, Math.min(320, h0 + ev.clientY - y0)));
    const up = () => {
      window.removeEventListener("pointermove", mv);
      window.removeEventListener("pointerup", up);
      endGesture();
    };
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", up);
  }
</script>

<div
  class="th"
  class:selected
  style="height:{tr.height}px;--c:{tr.color}"
  role="button"
  tabindex="-1"
  onpointerdown={() => (ed.selTrack = tr.id)}
  oncontextmenu={menu}
>
  <div class="top">
    <button class="swatch" style="background:{tr.color}" aria-label={t("color")} data-tip={t("color")} onclick={cycleColor}></button>
    <button class="spk" class:off={tr.mute} onclick={() => toggleMute([tr.id])} aria-label={t("muteToggle")} data-tip="{t('muteToggle')}"><Icon name={tr.mute ? "mute" : "volume"} size={15} /></button>
    {#if renaming}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="name-edit" type="text" bind:value={draft} autofocus onblur={finishRename} onkeydown={(e) => (e.key === "Enter" ? finishRename() : e.key === "Escape" && (renaming = false))} />
    {:else}
      <button class="name" ondblclick={startRename} title={tr.name}>{tr.name}</button>
    {/if}
    <button class="btn ghost sm icon" aria-label="menu" onclick={menu}><Icon name="more" size={15} /></button>
  </div>
  <div class="btns">
    <button class="tb s" class:on={tr.solo} onclick={(e) => soloOnly([tr.id], e.ctrlKey || e.metaKey)} data-tip={t("soloOnly")}><Icon name="headphones" size={11} />S</button>
    <button class="tb r" class:on={tr.armed} onclick={() => toggle("armed")} data-tip={t("arm")}><Icon name="record" size={11} /></button>
    <button class="tb fx" class:on={tr.effects.some((x) => x.on)} onclick={() => ((ed.selTrack = tr.id), (ed.inspectorTab = "track"))} data-tip={t("effects")}>
      FX{#if tr.effects.length}<span class="n">{tr.effects.length}</span>{/if}
    </button>
    <div class="grow"></div>
    {#if m.gr > 0.5}<span class="gr mono" data-tip={t("reduction")}>−{m.gr.toFixed(0)}</span>{/if}
  </div>
  {#if tr.height >= 80}
    <div class="faders">
      <input type="range" class="vol" min="-60" max="12" step="0.1" value={tr.gain} style="--fill:{((tr.gain + 60) / 72) * 100}%" onpointerdown={beginGesture} oninput={vol} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.gain = 0))} data-tip="{t('volume')}: {tr.gain.toFixed(1)} dB" />
      <input type="range" class="pan" min="-1" max="1" step="0.01" value={tr.pan} style="--fill:{((tr.pan + 1) / 2) * 100}%" onpointerdown={beginGesture} oninput={pan} onchange={endGesture} ondblclick={() => commit(() => (getTrack(tr.id)!.pan = 0))} data-tip="{t('pan')}: {tr.pan === 0 ? 'C' : (tr.pan < 0 ? 'L' : 'R') + Math.round(Math.abs(tr.pan) * 100)}" />
    </div>
  {/if}
  <div class="meter"><Meter l={m.l} r={m.r} length={186} size={3} /></div>
  <div class="resize" role="separator" onpointerdown={resizeStart}></div>
</div>

<style>
  .th {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 7px 10px 6px 12px;
    border-bottom: 1px solid var(--line);
    background: linear-gradient(90deg, color-mix(in srgb, var(--c) 9%, transparent), transparent 70%);
    overflow: hidden;
    cursor: default;
  }
  .th.selected {
    background: linear-gradient(90deg, color-mix(in srgb, var(--c) 22%, transparent), rgba(var(--accent-rgb), 0.06) 80%);
  }
  .th.selected .swatch {
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.9), 0 0 12px var(--c);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .swatch {
    width: 12px;
    height: 12px;
    border-radius: 4px;
    border: 0;
    flex: none;
    cursor: pointer;
    box-shadow: 0 0 8px color-mix(in srgb, var(--c) 60%, transparent);
  }
  .spk {
    width: 24px;
    height: 22px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }
  .spk:hover {
    background: var(--glass-3);
  }
  .spk.off {
    color: #1a1204;
    background: #f59e0b;
  }
  .name {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    text-align: left;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0;
    cursor: default;
  }
  .name-edit {
    flex: 1;
    min-width: 0;
    height: 22px !important;
  }
  .btns {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .tb {
    height: 20px;
    min-width: 24px;
    padding: 0 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    border-radius: 5px;
    border: 1px solid var(--line-2);
    background: rgba(0, 0, 0, 0.2);
    color: var(--text-2);
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
  }
  .tb:hover {
    color: var(--text);
    border-color: var(--line-3);
  }
  .tb.s.on {
    background: #38bdf8;
    border-color: #38bdf8;
    color: #04121a;
  }
  .tb.r {
    color: rgba(239, 68, 68, 0.75);
  }
  .tb.r.on {
    background: var(--rec);
    border-color: var(--rec);
    color: #fff;
    box-shadow: 0 0 10px rgba(239, 68, 68, 0.6);
  }
  .tb.fx.on {
    border-color: rgba(var(--accent-rgb), 0.8);
    color: #fff;
    background: rgba(var(--accent-rgb), 0.3);
  }
  .n {
    font-size: 9.5px;
    opacity: 0.8;
  }
  .gr {
    font-size: 10px;
    color: #fbbf24;
  }
  .faders {
    display: grid;
    grid-template-columns: 1fr 64px;
    gap: 8px;
    align-items: center;
  }
  .faders input {
    height: 14px;
  }
  .meter {
    margin-top: auto;
  }
  .resize {
    position: absolute;
    left: 0;
    right: 0;
    bottom: -3px;
    height: 7px;
    cursor: ns-resize;
  }
</style>
