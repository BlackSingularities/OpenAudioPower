<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { api, type ModelStatus, type SourceInfo } from "../lib/api";
  import { t, stemLabel } from "../lib/i18n.svelte";
  import { fmtBytes, fmtDuration } from "../lib/format";
  import { S, app, persist, source, addStemTracks, toast, STEM_COLORS, cancelJob, importAndSeparate } from "../lib/store.svelte";

  let models = $state<ModelStatus[]>([]);
  let keep = $state<Record<string, boolean>>({});
  let saveFolder = $state(false);
  let running = $state(false);
  let downloading = $state("");
  let result = $state<{ stems: SourceInfo[]; device: string; seconds: number } | null>(null);

  const st = S.settings.stems;
  const src = $derived(app.stemSource ? source(app.stemSource.id) : undefined);
  const candidates = $derived(app.sources.filter((s) => s.kind !== "stem"));
  const engineStems = $derived(st.engine === "dsp" ? ["vocals", "drums", "bass", "other"] : (models.find((m) => m.id === st.engine && !m.legacy)?.stems ?? []));
  const model = $derived(models.find((m) => m.id === st.engine && !m.legacy));
  const ready = $derived(st.engine === "dsp" || !!model?.installed);
  const seconds = $derived(app.stemSource ? (app.stemSource.end - app.stemSource.start) / app.rate : 0);

  async function refresh() {
    models = await api.models();
  }
  $effect(() => {
    if (app.dialogs.stems) {
      refresh();
      result = null;
    }
  });

  function pickSource(e: Event) {
    const id = +(e.target as HTMLSelectElement).value;
    const s = source(id);
    if (s) app.stemSource = { id, start: 0, end: s.frames, trackId: null, clipStart: 0 };
  }

  async function download(id: string) {
    downloading = id;
    app.job = { kind: "download", fraction: 0, label: "" };
    try {
      await api.downloadModel(id);
      await refresh();
    } catch (e) {
      const m = String(e);
      if (m !== "cancelled") toast(`${t("error")}: ${m}`, "error");
    } finally {
      app.job = null;
      downloading = "";
    }
  }

  async function run() {
    const ss = app.stemSource;
    if (!ss || !src) return;
    persist();
    let folder: string | null = null;
    if (saveFolder) {
      const f = await openDialog({ directory: true });
      if (typeof f !== "string") return;
      folder = f;
    }
    running = true;
    app.job = { kind: "separate", fraction: 0, label: "" };
    try {
      const want = engineStems.filter((s) => keep[s] !== false);
      const r = await api.separate(ss.id, ss.start, ss.end, { engine: st.engine, gpu: st.useGpu, quality: st.quality }, want);
      app.sources.push(...r.stems);
      addStemTracks(r.stems, ss.trackId ? ss.clipStart : 0, ss.trackId);
      result = r;
      if (r.device.includes("GPU:")) toast(t("gpuFallback"), "info");
      if (folder) {
        for (const s of r.stems) {
          const base = (src.meta.title || src.name).replace(/[<>:"/\\|?*]/g, "_");
          await api.exportSource(s.id, `${folder}\\${base} - ${stemLabel(s.stem)}.flac`, { format: "flac", bits: 24, dither: false });
        }
        toast(`${t("exported")}: ${r.stems.length}`, "ok", { label: t("reveal"), run: () => revealItemInDir(folder!) });
      }
    } catch (e) {
      const m = String(e);
      if (m === "cancelled") toast(t("cancelled"));
      else if (m === "model-missing") toast(t("notInstalled"), "error");
      else toast(`${t("error")}: ${m}`, "error");
    } finally {
      app.job = null;
      running = false;
    }
  }

  function close() {
    if (running) return;
    app.dialogs.stems = false;
  }

  const engines = $derived([
    { id: "htdemucs_6s", name: "HT-Demucs · 6", desc: t("stem6Desc"), icon: "sparkles" },
    { id: "htdemucs", name: "HT-Demucs · 4", desc: t("stem4Desc"), icon: "vocals" },
    { id: "dsp", name: t("stemDsp"), desc: t("stemDspDesc"), icon: "cpu" },
  ]);
</script>

{#if app.dialogs.stems}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog glass-strong wide" role="dialog" aria-modal="true">
      <div class="hdr">
        <div class="hic"><Icon name="stems" size={22} /></div>
        <div>
          <h2>{t("stemTitle")}</h2>
          <p class="sub">{t("stemIntro")}</p>
        </div>
      </div>

      {#if result}
        <div class="done">
          <div class="ok"><Icon name="check" size={20} />{t("stemDone")} · {result.seconds.toFixed(1)} s · {t("usedDevice")}: {result.device.split(" · ")[0]}</div>
          <div class="stemgrid">
            {#each result.stems as s}
              <div class="stem" style="--c:{STEM_COLORS[s.stem ?? ''] ?? '#94a3b8'}"><Icon name={s.stem ?? "other"} size={18} />{stemLabel(s.stem)}</div>
            {/each}
          </div>
          <div class="actions">
            <button class="btn primary" onclick={() => (app.dialogs.stems = false)}>{t("ok")}</button>
          </div>
        </div>
      {:else}
        <div class="field">
          <div class="label">{t("stemSource")}</div>
          <div class="srcrow">
            {#if candidates.length}
              <select value={app.stemSource?.id ?? ""} onchange={pickSource} disabled={running}>
                {#if !src}<option value="">—</option>{/if}
                {#each candidates as c}<option value={c.id}>{c.name} · {fmtDuration(c.duration)}</option>{/each}
              </select>
            {/if}
            <button class="btn" disabled={running} onclick={importAndSeparate}><Icon name="folder" size={15} />{t("openFile")}</button>
          </div>
          {#if candidates.length}
            {#if src && app.stemSource && (app.stemSource.start > 0 || app.stemSource.end < src.frames)}
              <div class="dim small">{t("selection")}: {fmtDuration(seconds)}</div>
            {/if}
          {:else}
            <div class="dim small">{t("selectSource")}</div>
          {/if}
        </div>

        <div class="field">
          <div class="label">{t("stemEngine")}</div>
          <div class="engines">
            {#each engines as en}
              {@const m = models.find((x) => x.id === en.id && !x.legacy)}
              <div class="engine" class:on={st.engine === en.id} role="radio" aria-checked={st.engine === en.id} tabindex="0" onclick={() => !running && (st.engine = en.id)} onkeydown={(e) => e.key === "Enter" && (st.engine = en.id)}>
                <div class="eic"><Icon name={en.icon} size={18} /></div>
                <div class="grow">
                  <div class="en">{en.name}</div>
                  <div class="ed">{en.desc}</div>
                  {#if m}
                    <div class="es">
                      {#if m.installed}
                        <span class="chip ok"><Icon name="check" size={11} />{t("installed")}</span>
                      {:else if downloading === m.id}
                        <span class="chip">{t("downloading")} {Math.round((app.job?.fraction ?? 0) * 100)}%</span>
                        <button class="link" onclick={(e) => (e.stopPropagation(), cancelJob())}>{t("cancel")}</button>
                      {:else}
                        <button class="btn sm" onclick={(e) => (e.stopPropagation(), download(m.id))} disabled={!!downloading || running}><Icon name="download" size={13} />{t("download")} · {fmtBytes(m.size)}</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>

        <div class="cols">
          <div class="field">
            <div class="label">{t("stemKeep")}</div>
            <div class="keep">
              {#each engineStems as s}
                <label class="kp" style="--c:{STEM_COLORS[s] ?? '#94a3b8'}"><input type="checkbox" checked={keep[s] !== false} onchange={(e) => (keep[s] = (e.target as HTMLInputElement).checked)} /><Icon name={s} size={14} />{stemLabel(s)}</label>
              {/each}
            </div>
          </div>
          <div class="field">
            {#if st.engine !== "dsp"}
              <div class="label">{t("stemQuality")}</div>
              <div class="seg full">
                <button class:on={st.quality === 0} onclick={() => (st.quality = 0)}>{t("qFast")}</button>
                <button class:on={st.quality === 1} onclick={() => (st.quality = 1)}>{t("qHigh")}</button>
                <button class:on={st.quality === 2} onclick={() => (st.quality = 2)}>{t("qBest")}</button>
              </div>
              <label class="ck"><input type="checkbox" bind:checked={st.useGpu} />{t("stemGpu")}</label>
            {/if}
            <label class="ck"><input type="checkbox" bind:checked={st.muteOriginal} />{t("stemMuteOriginal")}</label>
            <label class="ck"><input type="checkbox" bind:checked={saveFolder} />{t("stemSaveFolder")}</label>
          </div>
        </div>

        {#if running}
          <div class="prog">
            <div class="pl"><span>{t("stemRunning")} {app.job?.label && app.job.label !== "load" ? "· " + app.job.label : ""}</span><span class="mono">{Math.round((app.job?.fraction ?? 0) * 100)}%</span></div>
            <div class="pb"><i style="width:{Math.max(2, (app.job?.fraction ?? 0) * 100)}%"></i></div>
          </div>
        {/if}

        <div class="actions">
          {#if running}
            <button class="btn" onclick={cancelJob}>{t("cancel")}</button>
          {:else}
            <button class="btn ghost" onclick={close}>{t("close")}</button>
          {/if}
          <button class="btn primary lg" disabled={running || !src || !ready || !engineStems.some((s) => keep[s] !== false)} onclick={run}>
            <Icon name="stems" size={17} />{running ? t("stemRunning") : t("stemRun")}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .wide {
    width: min(720px, 100%);
  }
  .hdr {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .hic {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: 13px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #f472b6, var(--accent), #22d3ee);
    color: #fff;
    box-shadow: 0 6px 22px rgba(var(--accent-rgb), 0.45);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin-bottom: 16px;
  }
  .small {
    font-size: 11.5px;
  }
  .srcrow {
    display: flex;
    gap: 8px;
  }
  .srcrow select {
    flex: 1;
    min-width: 0;
  }
  .engines {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .engine {
    display: flex;
    gap: 10px;
    padding: 12px;
    border-radius: 13px;
    border: 1px solid var(--line-2);
    background: rgba(0, 0, 0, 0.18);
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
  }
  .engine:hover {
    border-color: var(--line-3);
  }
  .engine.on {
    border-color: rgba(var(--accent-rgb), 0.9);
    background: rgba(var(--accent-rgb), 0.14);
    box-shadow: 0 0 0 3px rgba(var(--accent-rgb), 0.18);
  }
  .eic {
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 9px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--glass-3);
  }
  .engine.on .eic {
    background: rgba(var(--accent-rgb), 0.8);
    color: #fff;
  }
  .en {
    font-weight: 650;
    margin-bottom: 3px;
  }
  .ed {
    font-size: 11.5px;
    color: var(--text-2);
    line-height: 1.4;
  }
  .es {
    margin-top: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .chip.ok {
    color: #a7f3d0;
    border-color: rgba(52, 211, 153, 0.4);
  }
  .link {
    border: 0;
    background: transparent;
    color: var(--text-3);
    font-size: 11px;
    text-decoration: underline;
    cursor: pointer;
    padding: 0;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }
  .keep {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .kp {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 9px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--c) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--c) 35%, transparent);
    cursor: pointer;
    font-size: 12.5px;
  }
  .kp :global(svg) {
    color: var(--c);
  }
  .ck {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
    cursor: pointer;
    margin-top: 4px;
  }
  .prog {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .pl {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-2);
  }
  .pb {
    height: 6px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.1);
    overflow: hidden;
  }
  .pb i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #f472b6, var(--accent), #22d3ee);
    box-shadow: 0 0 12px var(--accent);
    transition: width 0.3s;
  }
  .done .ok {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #a7f3d0;
    margin-bottom: 14px;
  }
  .stemgrid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .stem {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--c) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--c) 45%, transparent);
    font-weight: 600;
  }
  .stem :global(svg) {
    color: var(--c);
  }
</style>
