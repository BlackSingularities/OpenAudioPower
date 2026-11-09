<script lang="ts">
  // Floating remix panel: the song's stems with their own faders, karaoke / a cappella in one
  // click and transposing the backing track. It stays beside the timeline so changes are heard
  // while playing.
  import Icon from "./Icon.svelte";
  import { t, stemLabel } from "../lib/i18n.svelte";
  import {
    app, S, findClip, source, track as getTrack, commit, beginGesture, endGesture, syncEngine, play, tp,
    stemTracksOf, separateForRemix, remixPreset, remixTranspose, bounceRemix, STEM_COLORS, type Track,
  } from "../lib/store.svelte";

  let semis = $state(0);
  let withVocals = $state(false);
  let busy = $state(false);

  const r = $derived(app.remix);
  const songClip = $derived(r ? findClip(r.clip)?.clip : undefined);
  const song = $derived(r ? source(r.song) : undefined);
  const stems = $derived(r ? stemTracksOf(r.song) : []);
  const stemOf = (tr: Track) => source(tr.clips[0]?.source ?? -1)?.stem ?? "";
  const ORDER = ["vocals", "drums", "bass", "guitar", "piano", "other"];
  const sorted = $derived([...stems].sort((a, b) => ORDER.indexOf(stemOf(a)) - ORDER.indexOf(stemOf(b))));
  const mode = $derived.by(() => {
    if (!stems.length) return "";
    const v = stems.filter((x) => stemOf(x) === "vocals");
    const o = stems.filter((x) => stemOf(x) !== "vocals");
    if (v.every((x) => x.mute) && o.every((x) => !x.mute)) return "karaoke";
    if (v.every((x) => !x.mute) && o.every((x) => x.mute)) return "acappella";
    if (stems.every((x) => !x.mute)) return "all";
    return "";
  });

  $effect(() => {
    if (r) {
      semis = r.semis;
      withVocals = r.withVocals;
    }
  });

  // fader: 0…150 % shown, stored as track dB
  const toPct = (db: number) => Math.round(Math.pow(10, db / 20) * 100);
  const toDb = (pct: number) => (pct <= 0 ? -60 : Math.max(-60, 20 * Math.log10(pct / 100)));
  function fader(tr: Track, e: Event) {
    const x = getTrack(tr.id);
    if (!x) return;
    x.gain = toDb(+(e.target as HTMLInputElement).value);
    syncEngine();
  }
  function toggleMute(tr: Track) {
    commit(() => {
      const x = getTrack(tr.id);
      if (x) x.mute = !x.mute;
    });
  }
  function solo(tr: Track) {
    commit(() => {
      for (const x of stems) {
        const y = getTrack(x.id);
        if (y) y.mute = x.id !== tr.id;
      }
    });
  }

  async function separate() {
    busy = true;
    await separateForRemix();
    busy = false;
  }
  async function transpose() {
    busy = true;
    await remixTranspose(semis, withVocals);
    busy = false;
  }
  async function bounce() {
    busy = true;
    await bounceRemix();
    busy = false;
  }

  const engineName = $derived(S.settings.stems.engine === "dsp" ? "DSP" : S.settings.stems.engine === "htdemucs" ? "HT-Demucs 4" : "HT-Demucs 6");
</script>

{#if app.dialogs.remix && r}
  <aside class="remix glass-strong" aria-label={t("remixTitle")}>
    <header>
      <span class="ic"><Icon name="sliders" size={16} /></span>
      <div class="ttl">
        <b>{t("remixTitle")}</b>
        <span class="song" title={song?.name}>{song?.meta?.title || song?.name || "—"}</span>
      </div>
      <button class="btn ghost sm icon" disabled={!songClip} aria-label={t("play")} onclick={play}><Icon name={tp.playing ? "pause" : "play"} size={15} /></button>
      <button class="btn ghost sm icon" aria-label={t("close")} onclick={() => (app.dialogs.remix = false)}><Icon name="x" size={15} /></button>
    </header>

    {#if !songClip}
      <p class="empty">{t("noSelection")}</p>
    {:else if !stems.length}
      <p class="empty">{t("remixNeedsStems")}</p>
      <button class="btn primary full" disabled={busy || !!app.job} onclick={separate}><Icon name="stems" size={15} />{t("remixSeparate")} · {engineName}</button>
    {:else}
      <div class="presets seg full">
        <button class:on={mode === "karaoke"} onclick={() => remixPreset("karaoke")}><Icon name="mic" size={13} />{t("remixKaraoke")}</button>
        <button class:on={mode === "acappella"} onclick={() => remixPreset("acappella")}><Icon name="vocals" size={13} />{t("remixAcappella")}</button>
        <button class:on={mode === "all"} onclick={() => remixPreset("all")}><Icon name="music" size={13} />{t("remixAll")}</button>
      </div>

      <div class="rows">
        {#each sorted as tr (tr.id)}
          {@const st = stemOf(tr)}
          <div class="row" class:muted={tr.mute}>
            <span class="dot" style="background:{STEM_COLORS[st] ?? tr.color}"></span>
            <span class="nm">{stemLabel(st) || tr.name}</span>
            <input
              type="range"
              min="0"
              max="150"
              step="1"
              value={toPct(tr.gain)}
              style="--fill:{(toPct(tr.gain) / 150) * 100}%"
              onpointerdown={beginGesture}
              oninput={(e) => fader(tr, e)}
              onchange={endGesture}
              ondblclick={() => commit(() => (getTrack(tr.id)!.gain = 0))}
              aria-label={stemLabel(st)}
            />
            <span class="pct mono">{tr.mute ? "—" : toPct(tr.gain) + "%"}</span>
            <button class="tb" class:on={tr.mute} onclick={() => toggleMute(tr)} aria-label={t("muteToggle")}>M</button>
            <button class="tb s" onclick={() => solo(tr)} aria-label={t("soloOnly")}>S</button>
          </div>
        {/each}
      </div>

      <div class="key">
        <div class="kh">
          <span class="label">{t("remixKey")}</span>
          <span class="mono val">{semis > 0 ? "+" : ""}{semis} {t("remixSemis")}</span>
        </div>
        <input type="range" min="-6" max="6" step="1" bind:value={semis} style="--fill:{((semis + 6) / 12) * 100}%" />
        <div class="kf">
          <label class="check"><input type="checkbox" bind:checked={withVocals} />{t("remixWithVocals")}</label>
          <button class="btn sm" disabled={busy || !!app.job || (semis === r.semis && withVocals === r.withVocals)} onclick={transpose}><Icon name="wand" size={13} />{t("apply")}</button>
        </div>
      </div>

      <button class="btn full" disabled={busy || !!app.job} onclick={bounce}><Icon name="bounce" size={14} />{t("remixBounce")}</button>
    {/if}
  </aside>
{/if}

<style>
  .remix {
    position: fixed;
    right: 18px;
    bottom: 64px;
    z-index: 150;
    width: 340px;
    max-height: calc(100vh - 140px);
    overflow: auto;
    padding: 14px;
    border-radius: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    animation: pop 0.18s cubic-bezier(0.2, 0.9, 0.3, 1.2);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: rgba(var(--accent-rgb), 0.25);
    color: #ddd6fe;
  }
  .ttl {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .song {
    font-size: 11.5px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .full {
    width: 100%;
    justify-content: center;
  }
  .presets button {
    font-size: 12px;
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: grid;
    grid-template-columns: 10px 64px 1fr 38px 22px 22px;
    align-items: center;
    gap: 7px;
  }
  .row.muted .nm,
  .row.muted input {
    opacity: 0.45;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .nm {
    font-size: 12.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pct {
    font-size: 11px;
    color: var(--text-2);
    text-align: right;
  }
  .tb {
    height: 20px;
    border-radius: 5px;
    border: 1px solid var(--line-2);
    background: transparent;
    color: var(--text-2);
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
  }
  .tb.on {
    background: #f87171;
    border-color: #f87171;
    color: #1a0606;
  }
  .tb.s:hover {
    background: #38bdf8;
    border-color: #38bdf8;
    color: #04121a;
  }
  .key {
    padding: 10px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.18);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .kh,
  .kf {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .val {
    font-size: 12px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
  }
</style>
