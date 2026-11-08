<script lang="ts">
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import MiniWave from "./MiniWave.svelte";
  import { api, type SourceInfo } from "../lib/api";
  import { t, stemLabel } from "../lib/i18n.svelte";
  import { openMenu } from "../lib/menu.svelte";
  import { fmtDuration } from "../lib/format";
  import { forgetWave } from "../lib/wave";
  import { app, tp, P, pickImport, addSourceToNewTrack, openStemDialog, toast, STEM_COLORS, peakCache, tracksOfSource, toggleMute, soloOnly } from "../lib/store.svelte";

  let query = $state("");

  const roots = $derived(
    app.sources.filter((s) => (s.kind === "file" || s.kind === "recording" || s.kind === "bounce") && (!query || s.name.toLowerCase().includes(query.toLowerCase()))),
  );
  const childrenOf = (id: number) => app.sources.filter((s) => s.kind === "stem" && s.parent === id);
  const orphanStems = $derived(app.sources.filter((s) => s.kind === "stem" && !app.sources.some((p) => p.id === s.parent && p.kind !== "stem")));
  const used = $derived(new Set(P.p.tracks.flatMap((tr) => tr.clips.map((c) => c.source))));

  function dragStart(e: DragEvent, s: SourceInfo) {
    e.dataTransfer!.setData("text/oap-source", String(s.id));
    e.dataTransfer!.effectAllowed = "copy";
  }

  async function exportSource(s: SourceInfo) {
    const p = await saveDialog({ defaultPath: s.name.replace(/[<>:"/\\|?*]/g, "_") + ".wav", filters: [{ name: "WAV", extensions: ["wav"] }, { name: "FLAC", extensions: ["flac"] }] });
    if (!p) return;
    const flac = p.toLowerCase().endsWith(".flac");
    try {
      await api.exportSource(s.id, p, { format: flac ? "flac" : "wav", bits: 24, dither: true });
      toast(`${t("exported")}: ${p.split(/[\\/]/).pop()}`, "ok", { label: t("reveal"), run: () => revealItemInDir(p) });
    } catch (e) {
      toast(String(e), "error");
    }
  }

  async function remove(s: SourceInfo) {
    const ids = [s.id, ...childrenOf(s.id).map((c) => c.id)].filter((id) => !used.has(id));
    if (!ids.length) return;
    await api.removeSources(ids);
    app.sources = app.sources.filter((x) => !ids.includes(x.id));
    ids.forEach((id) => (peakCache.delete(id), forgetWave(id)));
  }

  function menu(e: MouseEvent, s: SourceInfo) {
    openMenu(e, [
      { label: t("addToNewTrack"), icon: "plus", run: () => addSourceToNewTrack(s, 0) },
      { label: t("addAtPlayhead"), icon: "right", run: () => addSourceToNewTrack(s, tp.pos) },
      { sep: true },
      { label: t("stemSeparation"), icon: "stems", disabled: s.kind === "stem", run: () => openStemDialog(s.id) },
      { label: t("export") + "…", icon: "export", run: () => exportSource(s) },
      { label: t("reveal"), icon: "folder", disabled: !s.path, run: () => s.path && revealItemInDir(s.path) },
      { sep: true },
      { label: t("remove"), icon: "trash", danger: true, disabled: used.has(s.id), run: () => remove(s) },
    ]);
  }

  const kindIcon = (s: SourceInfo) => (s.kind === "recording" ? "mic" : s.kind === "bounce" ? "bounce" : "music");

  // state of the tracks playing a stem
  function stemState(id: number) {
    const ts = tracksOfSource(id);
    return { ids: ts.map((x) => x.id), muted: ts.length > 0 && ts.every((x) => x.mute), solo: ts.length > 0 && ts.every((x) => x.solo), placed: ts.length > 0 };
  }
</script>

<aside class="lib">
  <header class="head">
    <div class="ttl">
      <Icon name="music" size={14} />
      <span>{t("library")}</span>
      {#if app.sources.length}<span class="cnt">{roots.length}</span>{/if}
    </div>
    <button class="imp" onclick={pickImport} data-tip="{t('importAudio')} (Ctrl+I)"><Icon name="plus" size={15} /></button>
  </header>
  {#if app.sources.length > 2}
    <div class="search">
      <Icon name="zoomIn" size={13} />
      <input type="search" placeholder={t("library") + "…"} bind:value={query} />
    </div>
  {/if}

  <div class="list">
    {#if !app.sources.length}
      <button class="drop" onclick={pickImport}>
        <span class="dz"><Icon name="import" size={22} /></span>
        <b>{t("import")}</b>
        <span>{t("libraryEmpty")}</span>
      </button>
    {/if}

    {#snippet song(s: SourceInfo)}
      {@const kids = childrenOf(s.id)}
      <div class="song" class:used={used.has(s.id)} draggable="true" role="listitem" ondragstart={(e) => dragStart(e, s)} ondblclick={() => addSourceToNewTrack(s, 0)} oncontextmenu={(e) => menu(e, s)}>
        <div class="srow">
          <span class="art"><Icon name={kindIcon(s)} size={15} /></span>
          <div class="txt">
            <div class="nm" title={s.name}>{s.meta.title || s.name}</div>
            <div class="meta">
              {#if s.meta.artist}<span>{s.meta.artist}</span>·{/if}
              <span class="mono">{fmtDuration(s.duration)}</span>
              {#if s.meta.codec}·<span>{s.meta.codec.toUpperCase()}</span>{/if}
            </div>
          </div>
          <button class="ib" onclick={(e) => menu(e, s)} aria-label="menu"><Icon name="more" size={15} /></button>
        </div>
        <div class="wave"><MiniWave id={s.id} color="#a78bfa" height={22} /></div>
        {#if !kids.length && s.kind !== "bounce"}
          <button class="sepbtn" onclick={() => openStemDialog(s.id)}><Icon name="stems" size={13} />{t("stemSeparationShort")}</button>
        {/if}
      </div>
      {#if kids.length}
        <div class="stems">
          {#each kids as k (k.id)}
            {@render stem(k)}
          {/each}
        </div>
      {/if}
    {/snippet}

    {#snippet stem(k: SourceInfo)}
      {@const st = stemState(k.id)}
      {@const c = STEM_COLORS[k.stem ?? ""] ?? "#94a3b8"}
      <div class="stem" class:muted={st.muted} style="--c:{c}" draggable="true" role="listitem" ondragstart={(e) => dragStart(e, k)} ondblclick={() => addSourceToNewTrack(k, 0)} oncontextmenu={(e) => menu(e, k)}>
        <span class="sic"><Icon name={k.stem ?? "other"} size={14} /></span>
        <div class="sbody">
          <span class="sn">{stemLabel(k.stem)}</span>
          <MiniWave id={k.id} color={c} height={14} />
        </div>
        {#if st.placed}
          <button class="mb" class:on={st.muted} onclick={() => toggleMute(st.ids)} aria-label={t("muteToggle")} data-tip={t("muteToggle")}><Icon name={st.muted ? "mute" : "volume"} size={14} /></button>
          <button class="sb" class:on={st.solo} onclick={(e) => soloOnly(st.ids, e.ctrlKey || e.metaKey)} aria-label={t("soloOnly")} data-tip={t("soloOnly")}><Icon name="headphones" size={14} /></button>
        {:else}
          <button class="mb" onclick={() => addSourceToNewTrack(k, 0)} data-tip={t("addToNewTrack")}><Icon name="plus" size={14} /></button>
        {/if}
      </div>
    {/snippet}

    {#each roots as s (s.id)}
      {@render song(s)}
    {/each}
    {#if orphanStems.length}
      <div class="stems">
        {#each orphanStems as k (k.id)}{@render stem(k)}{/each}
      </div>
    {/if}
  </div>

  <footer class="foot">
    <span class="mono">{(app.rate / 1000).toFixed(1)} kHz</span>
    <span>·</span>
    <span>{app.info?.threads ?? ""} {t("threadsInfo")}</span>
  </footer>
</aside>

<style>
  .lib {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 10px 0 14px;
    flex: none;
    border-bottom: 1px solid var(--line);
  }
  .ttl {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .ttl :global(svg) {
    color: var(--accent);
  }
  .cnt {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--glass-3);
    font-size: 10px;
    letter-spacing: 0;
  }
  .imp {
    width: 28px;
    height: 28px;
    border-radius: 9px;
    border: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(var(--accent-rgb), 0.85);
    color: #fff;
    cursor: pointer;
    box-shadow: 0 3px 12px rgba(var(--accent-rgb), 0.45);
    transition: transform 0.12s, box-shadow 0.15s;
  }
  .imp:hover {
    transform: translateY(-1px);
    box-shadow: 0 5px 18px rgba(var(--accent-rgb), 0.6);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    height: 36px;
    flex: none;
    color: var(--text-3);
    border-bottom: 1px solid var(--line);
  }
  .search input {
    flex: 1;
    height: 28px;
    border: 0;
    background: transparent;
    padding: 0;
    box-shadow: none !important;
  }
  .list {
    flex: 1;
    overflow-y: auto;
  }
  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: calc(100% - 24px);
    margin: 16px 12px;
    padding: 26px 14px;
    border-radius: 14px;
    border: 1px dashed var(--line-2);
    background: transparent;
    color: var(--text-3);
    text-align: center;
    font-size: 12px;
    line-height: 1.45;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .drop:hover {
    border-color: rgba(var(--accent-rgb), 0.7);
    background: rgba(var(--accent-rgb), 0.06);
  }
  .drop b {
    color: var(--text);
    font-size: 13px;
  }
  .dz {
    width: 44px;
    height: 44px;
    border-radius: 13px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #8b5cf6, #6366f1);
    color: #fff;
    margin-bottom: 4px;
  }
  .song {
    padding: 10px 10px 10px 14px;
    border-bottom: 1px solid var(--line);
    cursor: grab;
    transition: background 0.12s;
  }
  .song:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .srow {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .art {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.55), rgba(34, 211, 238, 0.35));
    color: #fff;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2);
  }
  .used .art {
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2), 0 0 0 2px rgba(var(--accent-rgb), 0.5);
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .nm {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    display: flex;
    gap: 5px;
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
  }
  .ib {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-3);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
  }
  .song:hover .ib {
    opacity: 1;
  }
  .ib:hover {
    background: var(--glass-3);
    color: var(--text);
  }
  .wave {
    margin: 8px 0 0 44px;
    opacity: 0.85;
  }
  .sepbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0 44px;
    height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid rgba(var(--accent-rgb), 0.45);
    background: rgba(var(--accent-rgb), 0.1);
    color: #ddd6fe;
    font-size: 11px;
    white-space: nowrap;
    cursor: pointer;
  }
  .sepbtn:hover {
    background: rgba(var(--accent-rgb), 0.25);
    color: #fff;
  }
  .stems {
    padding: 4px 0 6px;
    border-bottom: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.12);
  }
  .stem {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px 5px 26px;
    cursor: grab;
    transition: background 0.12s, opacity 0.15s;
  }
  .stem:hover {
    background: color-mix(in srgb, var(--c) 8%, transparent);
  }
  .stem.muted .sbody,
  .stem.muted .sic {
    opacity: 0.35;
  }
  .sic {
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--c) 22%, transparent);
    color: var(--c);
  }
  .sbody {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .sn {
    font-size: 12px;
    font-weight: 600;
  }
  .mb,
  .sb {
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 7px;
    border: 1px solid var(--line-2);
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .mb:hover,
  .sb:hover {
    color: #fff;
    border-color: var(--line-3);
  }
  .mb.on {
    background: #f59e0b;
    border-color: transparent;
    color: #1a1204;
  }
  .sb.on {
    background: #38bdf8;
    border-color: transparent;
    color: #04121a;
  }
  .foot {
    display: flex;
    gap: 6px;
    padding: 8px 14px;
    font-size: 11px;
    color: var(--text-3);
    border-top: 1px solid var(--line);
    flex: none;
  }
</style>
