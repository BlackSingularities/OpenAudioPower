<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtTime } from "../lib/format";
  import { S, P, app, ed, persist, projectLength, toast } from "../lib/store.svelte";

  const ex = S.settings.export;
  let busy = $state(false);

  const FORMATS = [
    { id: "wav", label: "WAV" },
    { id: "flac", label: "FLAC" },
    { id: "mp3", label: "MP3" },
    { id: "m4a", label: "M4A" },
    { id: "ogg", label: "OGG" },
    { id: "opus", label: "Opus" },
  ];
  /** bitrates each lossy encoder supports (kbit/s); the middle one is the default */
  const RATES: Record<string, number[]> = {
    mp3: [128, 192, 256, 320],
    m4a: [96, 128, 160, 192],
    ogg: [128, 192, 256, 320],
    opus: [96, 128, 160, 256],
  };
  const lossy = $derived(ex.format in RATES);

  function pickFormat(f: string) {
    ex.format = f;
    if (f === "flac" && ex.bits > 24) ex.bits = 24;
    const r = RATES[f];
    if (r && !r.includes(ex.bitrate)) ex.bitrate = r.reduce((a, b) => (Math.abs(b - 192) < Math.abs(a - 192) ? b : a));
  }

  const range = $derived.by(() => {
    if (app.exportMode === "selection" && ed.sel) return { start: ed.sel.start, end: ed.sel.end };
    if (app.exportMode === "loop" && P.p.loop.end > P.p.loop.start) return { start: P.p.loop.start, end: P.p.loop.end };
    return { start: 0, end: projectLength() };
  });

  function options() {
    const norm = ex.norm;
    return {
      format: ex.format,
      bits: ex.format === "flac" ? Math.min(24, ex.bits) : ex.bits,
      bitrate: ex.bitrate ?? 192,
      dither: true,
      start: range.start,
      end: range.end,
      tail: app.exportMode === "mix" ? ex.tail : 0,
      normalize: norm === "peak" ? -1 : null,
      lufs: norm === "lufs14" ? -14 : norm === "lufs16" ? -16 : norm === "lufs23" ? -23 : null,
      rate: ex.rate || null,
    };
  }

  async function go() {
    persist();
    const p = $state.snapshot(P.p);
    const project = { tracks: p.tracks, master: p.master };
    const ext = ex.format;
    busy = true;
    app.job = { kind: "export", fraction: 0, label: "" };
    try {
      if (app.exportMode === "tracks") {
        const dir = await openDialog({ directory: true });
        if (typeof dir !== "string") return;
        const files = await api.exportTracks(dir, project, options());
        toast(`${t("exported")}: ${files.length}`, "ok", { label: t("reveal"), run: () => revealItemInDir(files[0] ?? dir) });
      } else {
        const name = (P.p.name || "Miks") + "." + ext;
        const path = await saveDialog({ defaultPath: name, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
        if (!path) return;
        await api.exportMix(path, project, options());
        toast(`${t("exported")}: ${path.split(/[\\/]/).pop()}`, "ok", { label: t("reveal"), run: () => revealItemInDir(path) });
      }
      app.dialogs.export = false;
    } catch (e) {
      const m = String(e);
      toast(m === "cancelled" ? t("cancelled") : `${t("error")}: ${m}`, m === "cancelled" ? "info" : "error");
    } finally {
      app.job = null;
      busy = false;
    }
  }
</script>

{#if app.dialogs.export}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && (app.dialogs.export = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>{t("exportTitle")}</h2>
      <p class="sub">{fmtTime(range.start, app.rate)} – {fmtTime(range.end, app.rate)} · {P.p.tracks.length} {t("tracks").toLowerCase()}</p>
      <div class="grid">
        <div class="label">{t("exportWhat")}</div>
        <div class="seg full">
          <button class:on={app.exportMode === "mix"} onclick={() => (app.exportMode = "mix")}>{t("exportWholeMix")}</button>
          <button class:on={app.exportMode === "selection"} disabled={!ed.sel} onclick={() => (app.exportMode = "selection")}>{t("exportSel")}</button>
          <button class:on={app.exportMode === "loop"} disabled={P.p.loop.end <= P.p.loop.start} onclick={() => (app.exportMode = "loop")}>{t("exportLoop")}</button>
          <button class:on={app.exportMode === "tracks"} onclick={() => (app.exportMode = "tracks")}>{t("exportSeparate")}</button>
        </div>
        <div class="label">{t("format")}</div>
        <div class="seg full">
          {#each FORMATS as f}
            <button class:on={ex.format === f.id} onclick={() => pickFormat(f.id)}>{f.label}</button>
          {/each}
        </div>
        {#if lossy}
          <div class="label">{t("bitrate")}</div>
          <div class="seg full">
            {#each RATES[ex.format] as r}
              <button class:on={ex.bitrate === r} onclick={() => (ex.bitrate = r)}>{r} kbit/s</button>
            {/each}
          </div>
        {:else}
          <div class="label">{t("bitDepth")}</div>
          <div class="seg full">
            <button class:on={ex.bits === 16} onclick={() => (ex.bits = 16)}>16 bit</button>
            <button class:on={ex.bits === 24} onclick={() => (ex.bits = 24)}>24 bit</button>
            <button class:on={ex.bits === 32} disabled={ex.format === "flac"} onclick={() => (ex.bits = 32)}>32 bit float</button>
          </div>
        {/if}
        <div class="label">{t("sampleRate")}</div>
        <select bind:value={ex.rate}>
          <option value={0}>{t("projectRate")} ({app.rate} Hz)</option>
          {#each [44100, 48000, 88200, 96000] as r}<option value={r}>{r} Hz</option>{/each}
        </select>
        <div class="label">{t("normalize")}</div>
        <select bind:value={ex.norm}>
          <option value="none">{t("normNone")}</option>
          <option value="peak">{t("normPeak")}</option>
          <option value="lufs14">{t("normStreaming")}</option>
          <option value="lufs16">{t("normPodcast")}</option>
          <option value="lufs23">{t("normBroadcast")}</option>
        </select>
        {#if app.exportMode === "mix"}
          <div class="label">{t("tail")}</div>
          <input type="number" min="0" max="30" step="0.5" bind:value={ex.tail} />
        {/if}
      </div>
      <div class="actions">
        <button class="btn ghost" disabled={busy} onclick={() => (app.dialogs.export = false)}>{t("cancel")}</button>
        <button class="btn primary lg" disabled={busy || !P.p.tracks.length} onclick={go}><Icon name="export" size={16} />{t("exportGo")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(680px, 100%);
  }
  /* rows of options wrap instead of running past the dialog edge */
  .grid .seg.full {
    flex-wrap: wrap;
  }
  .grid .seg.full button {
    flex: 1 1 auto;
    padding: 0 9px;
  }
  .grid {
    display: grid;
    grid-template-columns: 130px minmax(0, 1fr);
    gap: 12px 14px;
    align-items: center;
  }
  .grid select {
    width: 100%;
  }
</style>
