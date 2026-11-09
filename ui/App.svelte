<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, ProgressBarStatus, UserAttentionType } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getVersion } from "@tauri-apps/api/app";
  import TitleBar from "./components/TitleBar.svelte";
  import Transport from "./components/Transport.svelte";
  import Timeline from "./components/Timeline.svelte";
  import Library from "./components/Library.svelte";
  import Inspector from "./components/Inspector.svelte";
  import Mixer from "./components/Mixer.svelte";
  import Analyzer from "./components/Analyzer.svelte";
  import StemDialog from "./components/StemDialog.svelte";
  import ExportDialog from "./components/ExportDialog.svelte";
  import BatchDialog from "./components/BatchDialog.svelte";
  import SpectralEditor from "./components/SpectralEditor.svelte";
  import RemixPanel from "./components/RemixPanel.svelte";
  import PluginBrowser from "./components/PluginBrowser.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import InfoDialogs from "./components/InfoDialogs.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import Tooltip from "./components/Tooltip.svelte";
  import Icon from "./components/Icon.svelte";
  import { t, getLang } from "./lib/i18n.svelte";
  import { initFeedback } from "./lib/feedback.js";
  import { upd, startUpdateChecks, checkForUpdate } from "./lib/updates.svelte";
  import {
    S, P, app, ed, tp, init, persist, importPaths, pickImport, openProject, saveProject, newProject, play, stop, startRecording,
    toStart, toEnd, seek, toggleLoop, setMetronome, splitAt, deleteSelected, deleteRange, undo, redo, copySelected, paste,
    duplicateSelected, selectAll, addMarker, addTrackCmd, zoomAt, zoomFit, hasUnsavedWork, dismissToast, toast,
    cancelJob, separateCmd,
    openSpectral,
    openRemix,
    karaoke,
  } from "./lib/store.svelte";

  let updHidden = $state(false);
  let allowClose = false;

  onMount(() => {
    init();
    initFeedback({ app: "openaudiopower", appName: "OpenAudioPower", version: () => getVersion(), lang: () => getLang() });
    startUpdateChecks(() => S.settings.checkUpdates);
    const unsubs: (() => void)[] = [];
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const p: any = e.payload;
        if (p.type === "enter" || p.type === "over") app.dragOver = true;
        else if (p.type === "leave") app.dragOver = false;
        else if (p.type === "drop") {
          app.dragOver = false;
          if (p.paths?.length) importPaths(p.paths);
        }
      })
      .then((u) => unsubs.push(u));
    getCurrentWindow()
      .onCloseRequested((e) => {
        if (!allowClose && hasUnsavedWork()) {
          e.preventDefault();
          app.dialogs.exit = true;
        }
      })
      .then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  });

  // taskbar progress for long jobs
  let lastJob = "";
  $effect(() => {
    const j = app.job;
    const w = getCurrentWindow();
    if (j) {
      w.setProgressBar({ status: ProgressBarStatus.Normal, progress: Math.round(Math.min(1, Math.max(0, j.fraction)) * 100) }).catch(() => {});
      lastJob = j.kind;
    } else {
      w.setProgressBar({ status: ProgressBarStatus.None }).catch(() => {});
      if ((lastJob === "separate" || lastJob === "export") && !document.hasFocus()) w.requestUserAttention(UserAttentionType.Informational).catch(() => {});
      lastJob = "";
    }
  });

  async function quit() {
    if (hasUnsavedWork()) {
      app.dialogs.exit = true;
      return;
    }
    allowClose = true;
    await getCurrentWindow().destroy();
  }
  async function saveAndQuit() {
    if (await saveProject()) {
      allowClose = true;
      await getCurrentWindow().destroy();
    }
  }
  async function quitAnyway() {
    allowClose = true;
    stop();
    await getCurrentWindow().destroy();
  }

  async function manualUpdateCheck() {
    const ok = await checkForUpdate();
    if (!ok) return toast(t("updFailed"), "error");
    if (upd.available) {
      updHidden = false;
      S.settings.skippedVersion = "";
      persist();
    } else toast(`${t("updLatest")} (${upd.current})`, "ok");
  }

  function anyDialog() {
    return Object.values(app.dialogs).some(Boolean);
  }

  function onKey(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") {
      if (e.key === "Escape") (e.target as HTMLElement).blur();
      return;
    }
    const k = e.key.toLowerCase();
    const ctrl = e.ctrlKey || e.metaKey;
    if (e.key === "Escape") {
      if (anyDialog()) {
        if (!app.job) Object.keys(app.dialogs).forEach((d) => ((app.dialogs as any)[d] = false));
      } else if (app.job) cancelJob();
      else {
        ed.selClips = [];
        ed.sel = null;
      }
      return;
    }
    if (anyDialog()) return;
    const run = (f: () => void) => {
      e.preventDefault();
      f();
    };
    if (ctrl) {
      if (k === "z" && !e.shiftKey) return run(undo);
      if (k === "y" || (k === "z" && e.shiftKey)) return run(redo);
      if (k === "s") return run(() => saveProject(e.shiftKey));
      if (k === "o") return run(() => openProject());
      if (k === "n") return run(newProject);
      if (k === "i") return run(pickImport);
      if (k === "e" && e.shiftKey) return run(separateCmd);
      if (k === "e") return run(() => ((app.exportMode = "mix"), (app.dialogs.export = true)));
      if (k === "b") return run(() => (app.dialogs.batch = true));
      if (k === "r" && e.shiftKey) return run(() => openSpectral());
      if (k === "k" && e.shiftKey) return run(karaoke);
      if (k === "m" && e.shiftKey) return run(() => openRemix());
      if (k === "c") return run(() => copySelected());
      if (k === "x") return run(() => copySelected(true));
      if (k === "v") return run(paste);
      if (k === "d") return run(duplicateSelected);
      if (k === "a") return run(selectAll);
      if (k === "t") return run(addTrackCmd);
      if (k === "delete") return run(() => deleteRange(true));
      if (k === "=" || k === "+") return run(() => zoomAt(1 / 1.5));
      if (k === "-") return run(() => zoomAt(1.5));
      if (k === "0") return run(zoomFit);
      if (k === ",") return run(() => (app.dialogs.settings = true));
      return;
    }
    if (e.altKey) return;
    const step = (e.shiftKey ? 10 : 1) * app.rate * 0.1 * Math.max(1, ed.fpp / 200);
    switch (e.key) {
      case " ":
        return run(play);
      case "Home":
        return run(toStart);
      case "End":
        return run(toEnd);
      case "ArrowLeft":
        return run(() => seek(tp.pos - step));
      case "ArrowRight":
        return run(() => seek(tp.pos + step));
      case "Delete":
      case "Backspace":
        return run(deleteSelected);
      case "F1":
        return run(() => (app.dialogs.shortcuts = true));
      case "F2":
        return run(() => ((S.settings.ui.library = !S.settings.ui.library), persist()));
      case "F3":
        return run(() => ((S.settings.ui.inspector = !S.settings.ui.inspector), persist()));
      case "F4":
        return run(() => ((S.settings.ui.dock = S.settings.ui.dock === "mixer" ? null : "mixer"), persist()));
      case "F5":
        return run(() => ((S.settings.ui.dock = S.settings.ui.dock === "analyzer" ? null : "analyzer"), persist()));
    }
    switch (k) {
      case "r":
        return run(startRecording);
      case "l":
        return run(toggleLoop);
      case "k":
        return run(() => setMetronome(!tp.metronome));
      case "s":
        return run(() => splitAt());
      case "m":
        return run(addMarker);
      case "g":
        return run(() => ((S.settings.snap = !S.settings.snap), persist()));
      case "f":
        return run(() => ((S.settings.follow = !S.settings.follow), persist()));
    }
  }

  // resizable side panels and dock
  function resize(which: "lib" | "insp" | "dock", e: PointerEvent) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const y0 = e.clientY;
    const u = S.settings.ui;
    const w0 = which === "lib" ? u.libraryWidth : which === "insp" ? u.inspectorWidth : u.dockHeight;
    const mv = (ev: PointerEvent) => {
      if (which === "lib") u.libraryWidth = Math.max(200, Math.min(480, w0 + ev.clientX - x0));
      else if (which === "insp") u.inspectorWidth = Math.max(280, Math.min(560, w0 - (ev.clientX - x0)));
      else u.dockHeight = Math.max(140, Math.min(520, w0 - (ev.clientY - y0)));
    };
    const up = () => {
      el.removeEventListener("pointermove", mv);
      el.removeEventListener("pointerup", up);
      persist();
    };
    el.addEventListener("pointermove", mv);
    el.addEventListener("pointerup", up);
  }

  const empty = $derived(!P.p.tracks.length);
  const ui = $derived(S.settings.ui);
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <TitleBar onUpdateCheck={manualUpdateCheck} onQuit={quit} />
  <Transport />

  <main class="work" style="--lib:{ui.library ? ui.libraryWidth + 'px' : '0px'};--insp:{ui.inspector ? ui.inspectorWidth + 'px' : '0px'}">
    {#if ui.library}
      <div class="side left">
        <Library />
        <div class="vresize r" role="separator" onpointerdown={(e) => resize("lib", e)}></div>
      </div>
    {/if}

    <section class="center">
      <div class="tlwrap">
        {#if empty && app.ready}
          <div class="welcome">
            <div class="wl-hero">
              <img src="/icon.png" alt="" width="76" height="76" />
              <h1>{t("welcomeTitle")}</h1>
              <p>{t("welcomeText")}</p>
            </div>
            <div class="acts">
              <button class="act" onclick={pickImport}>
                <svg class="motif m-import" viewBox="0 0 120 56" aria-hidden="true">
                  {#each Array(15) as _, i}<rect x={6 + i * 7.4} y={28 - (5 + 18 * Math.abs(Math.sin(i * 0.9)))} width="3.6" height={2 * (5 + 18 * Math.abs(Math.sin(i * 0.9)))} rx="1.8" style="--d:{i * 60}ms" />{/each}
                  <path class="arrow" d="M60 2v20M52 15l8 8 8-8" />
                </svg>
                <b>{t("welcomeImport")}</b>
                <small>{t("welcomeImportSub")}</small>
                <kbd>Ctrl I</kbd>
              </button>
              <span class="vs" aria-hidden="true"></span>
              <button class="act" onclick={startRecording}>
                <svg class="motif m-rec" viewBox="0 0 120 56" aria-hidden="true">
                  <circle cx="60" cy="28" r="7" class="core" />
                  <circle cx="60" cy="28" r="14" class="ring r1" />
                  <circle cx="60" cy="28" r="22" class="ring r2" />
                  <path class="line" d="M2 28h30M88 28h30" />
                </svg>
                <b>{t("welcomeRecord")}</b>
                <small>{t("welcomeRecordSub")}</small>
                <kbd>R</kbd>
              </button>
              <span class="vs" aria-hidden="true"></span>
              <button class="act feat" onclick={separateCmd}>
                <svg class="motif m-stems" viewBox="0 0 120 56" aria-hidden="true">
                  {#each ["#f472b6", "#f59e0b", "#22d3ee", "#a3e635", "#a78bfa"] as c, k}
                    <g style="--c:{c};--k:{k}">
                      {#each Array(13) as _, i}<rect x={8 + i * 8.2} y={4 + k * 10.5} width="5" height={3 + 5 * Math.abs(Math.sin(i * 1.3 + k))} rx="1.5" />{/each}
                    </g>
                  {/each}
                </svg>
                <b>{t("welcomeSeparate")}</b>
                <small>{t("welcomeSeparateSub")}</small>
                <kbd>Ctrl Shift E</kbd>
              </button>
              <span class="vs" aria-hidden="true"></span>
              <button class="act" onclick={() => openProject()}>
                <svg class="motif m-proj" viewBox="0 0 120 56" aria-hidden="true">
                  <rect x="30" y="30" width="60" height="16" rx="4" class="l3" />
                  <rect x="30" y="20" width="60" height="16" rx="4" class="l2" />
                  <rect x="30" y="10" width="60" height="16" rx="4" class="l1" />
                </svg>
                <b>{t("welcomeOpen")}</b>
                <small>{t("welcomeOpenSub")}</small>
                <kbd>Ctrl O</kbd>
              </button>
            </div>
            {#if S.settings.recent.length}
              <div class="recent">
                {#each S.settings.recent.slice(0, 5) as r}
                  <button class="rc" onclick={() => openProject(r)}><Icon name="file" size={14} />{r.split(/[\\/]/).pop()}</button>
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <Timeline />
        {/if}
      </div>

      {#if ui.dock && !empty}
        <div class="dock" style="height:{ui.dockHeight}px">
          <div class="hresize" role="separator" onpointerdown={(e) => resize("dock", e)}></div>
          <div class="dock-tabs">
            <button class:on={ui.dock === "mixer"} onclick={() => ((ui.dock = "mixer"), persist())}><Icon name="sliders" size={14} />{t("showMixer")}</button>
            <button class:on={ui.dock === "analyzer"} onclick={() => ((ui.dock = "analyzer"), persist())}><Icon name="chart" size={14} />{t("showAnalyzer")}</button>
            <div class="grow"></div>
            <button class="x" onclick={() => ((ui.dock = null), persist())} aria-label={t("close")}><Icon name="down" size={14} /></button>
          </div>
          <div class="dock-body">
            {#if ui.dock === "mixer"}<Mixer height={ui.dockHeight - 34} />{:else}<Analyzer />{/if}
          </div>
        </div>
      {/if}
    </section>

    {#if ui.inspector}
      <div class="side right">
        <div class="vresize l" role="separator" onpointerdown={(e) => resize("insp", e)}></div>
        <Inspector />
      </div>
    {/if}
  </main>

  <footer class="status">
    <button class="sb" class:on={ui.library} onclick={() => ((ui.library = !ui.library), persist())} data-tip="{t('showLibrary')} (F2)"><Icon name="sidebarL" size={14} /></button>
    <button class="sb" class:on={!!ui.dock} onclick={() => ((ui.dock = ui.dock ? null : "mixer"), persist())} data-tip="{t('showMixer')} (F4)"><Icon name="panelB" size={14} /></button>
    <button class="sb" class:on={ui.inspector} onclick={() => ((ui.inspector = !ui.inspector), persist())} data-tip="{t('showInspector')} (F3)"><Icon name="sidebarR" size={14} /></button>
    <span class="dim">{P.p.tracks.length} {t("tracks").toLowerCase()} · {(app.rate / 1000).toFixed(1)} kHz{tp.error ? " · ⚠ " + tp.error : ""}</span>
    <div class="grow"></div>
    <span class="dim">{t("appTagline")} · redzikowski.com</span>
  </footer>
</div>

{#if app.dragOver}
  <div class="drop">
    <div class="drop-in glass-strong"><Icon name="import" size={34} />{t("dropHere")}</div>
  </div>
{/if}

<div class="toasts">
  {#each app.toasts as ts (ts.id)}
    <div class="toast glass-strong {ts.kind}" role="status">
      <span class="tdot"></span>
      <span class="tt">{ts.text}</span>
      {#if ts.action}<button class="btn sm" onclick={() => (ts.action!.run(), dismissToast(ts.id))}>{ts.action.label}</button>{/if}
      <button class="btn sm ghost icon" onclick={() => dismissToast(ts.id)} aria-label={t("close")}><Icon name="x" size={13} /></button>
    </div>
  {/each}
</div>

{#if app.dialogs.exit}
  <div class="backdrop" role="presentation">
    <div class="dialog glass-strong" role="alertdialog" aria-modal="true">
      <h2>{t("unsavedTitle")}</h2>
      <p class="sub">{t("unsavedText")}</p>
      <div class="actions">
        <button class="btn ghost" onclick={() => (app.dialogs.exit = false)}>{t("cancel")}</button>
        <button class="btn danger" onclick={quitAnyway}>{t("dontSave")}</button>
        <button class="btn primary" onclick={saveAndQuit}>{t("save")}</button>
      </div>
    </div>
  </div>
{/if}

<StemDialog />
<ExportDialog />
<BatchDialog />
<SpectralEditor />
<RemixPanel />
<PluginBrowser />
<SettingsDialog />
<InfoDialogs />
<UpdateBanner bind:hidden={updHidden} />
<ContextMenu />
<Tooltip />

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }
  .work {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .side {
    position: relative;
    flex: none;
    min-height: 0;
  }
  .side.left {
    width: var(--lib);
  }
  .side.right {
    width: var(--insp);
  }
  .vresize {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 9px;
    cursor: ew-resize;
    z-index: 5;
    background: linear-gradient(90deg, transparent 4px, var(--line) 4px, var(--line) 5px, transparent 5px);
    transition: background 0.15s;
  }
  .vresize:hover {
    background: linear-gradient(90deg, transparent 3px, rgba(var(--accent-rgb), 0.7) 3px, rgba(var(--accent-rgb), 0.7) 6px, transparent 6px);
  }
  .vresize.r {
    right: -5px;
  }
  .vresize.l {
    left: -5px;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .tlwrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
  .dock {
    position: relative;
    flex: none;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .hresize {
    position: absolute;
    left: 0;
    right: 0;
    top: -5px;
    height: 9px;
    cursor: ns-resize;
    z-index: 5;
  }
  .dock-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 6px 0;
    height: 34px;
    flex: none;
  }
  .dock-tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    font-size: 12px;
  }
  .dock-tabs button:hover {
    color: var(--text);
    background: var(--glass-2);
  }
  .dock-tabs button.on {
    color: #fff;
    background: rgba(var(--accent-rgb), 0.22);
  }
  .dock-body {
    flex: 1;
    min-height: 0;
  }
  .status {
    border-top: 1px solid var(--line);
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    font-size: 11px;
    flex: none;
  }
  .sb {
    width: 24px;
    height: 20px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sb.on {
    color: var(--text);
    background: var(--glass-2);
  }
  .welcome {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 28px;
    padding: 30px;
    overflow: auto;
  }
  .wl-hero {
    text-align: center;
  }
  .wl-hero img {
    filter: drop-shadow(0 12px 34px rgba(var(--accent-rgb), 0.6));
  }
  .wl-hero h1 {
    margin: 14px 0 6px;
    font: 650 30px var(--font-display);
    letter-spacing: -0.3px;
    background: linear-gradient(90deg, #fff, #ddd6fe 50%, #a5f3fc);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .wl-hero p {
    margin: 0;
    color: var(--text-2);
    font-size: 14px;
  }
  .acts {
    display: flex;
    align-items: stretch;
    width: min(980px, 100%);
    padding: 6px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .vs {
    width: 1px;
    flex: none;
    margin: 18px 0;
    background: linear-gradient(180deg, transparent, var(--line-2), transparent);
  }
  .act {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 18px 22px 18px;
    border: 0;
    background: transparent;
    text-align: center;
    cursor: pointer;
    border-radius: 14px;
    transition: background 0.25s;
  }
  .act:hover {
    background: radial-gradient(120% 90% at 50% 0%, rgba(var(--accent-rgb), 0.14), transparent 70%);
  }
  .act.feat {
    flex: 1.25;
    background: radial-gradient(120% 100% at 50% 0%, rgba(var(--accent-rgb), 0.12), transparent 75%);
  }
  .act.feat:hover {
    background: radial-gradient(120% 100% at 50% 0%, rgba(var(--accent-rgb), 0.24), transparent 75%);
  }
  .act b {
    font: 650 15px var(--font-display);
    margin-top: 6px;
    color: var(--text);
  }
  .act.feat b {
    background: linear-gradient(90deg, #f9a8d4, #c4b5fd 50%, #67e8f9);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .act small {
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
    min-height: 32px;
  }
  .act kbd {
    margin-top: 6px;
    font-family: var(--mono);
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 5px;
    color: var(--text-3);
    background: var(--glass-2);
    opacity: 0;
    transform: translateY(3px);
    transition: opacity 0.2s, transform 0.2s;
  }
  .act:hover kbd {
    opacity: 1;
    transform: none;
  }
  .motif {
    width: 120px;
    height: 56px;
    margin: 6px 0 4px;
    overflow: visible;
  }
  .m-import rect {
    fill: rgba(167, 139, 250, 0.55);
    transform-origin: center;
    transform-box: fill-box;
  }
  .m-import .arrow {
    fill: none;
    stroke: #fff;
    stroke-width: 2.4;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0;
    transform: translateY(-6px);
    transition: opacity 0.25s, transform 0.3s;
  }
  .act:hover .m-import .arrow {
    opacity: 1;
    transform: none;
  }
  .act:hover .m-import rect {
    animation: bounce 0.9s ease-in-out infinite;
    animation-delay: var(--d);
    fill: #a78bfa;
  }
  @keyframes bounce {
    50% {
      transform: scaleY(0.45);
    }
  }
  .m-rec .core {
    fill: #ef4444;
    filter: drop-shadow(0 0 6px #ef4444);
  }
  .m-rec .ring {
    fill: none;
    stroke: rgba(239, 68, 68, 0.45);
    stroke-width: 1.5;
    transform-origin: 60px 28px;
  }
  .m-rec .line {
    stroke: rgba(255, 255, 255, 0.18);
    stroke-width: 1.5;
    stroke-dasharray: 3 4;
  }
  .act:hover .m-rec .ring {
    animation: ripple 1.6s ease-out infinite;
  }
  .act:hover .m-rec .r2 {
    animation-delay: 0.5s;
  }
  @keyframes ripple {
    from {
      transform: scale(0.5);
      opacity: 1;
    }
    to {
      transform: scale(1.5);
      opacity: 0;
    }
  }
  .m-stems g rect {
    fill: var(--c);
    opacity: 0.85;
    transition: transform 0.45s cubic-bezier(0.3, 1.3, 0.5, 1);
    transform: translateY(calc((2 - var(--k)) * 10.5px));
    filter: drop-shadow(0 0 3px color-mix(in srgb, var(--c) 60%, transparent));
  }
  .act:hover .m-stems g rect,
  .act.feat .m-stems g rect {
    transform: none;
  }
  .act.feat:not(:hover) .m-stems g rect {
    transform: translateY(calc((2 - var(--k)) * 3px));
  }
  .m-proj rect {
    fill: rgba(14, 165, 233, 0.12);
    stroke: rgba(103, 232, 249, 0.55);
    stroke-width: 1.3;
    transition: transform 0.35s cubic-bezier(0.3, 1.3, 0.5, 1);
  }
  .m-proj .l1 {
    fill: rgba(16, 185, 129, 0.25);
  }
  .act:hover .m-proj .l1 {
    transform: translate(-8px, -6px);
  }
  .act:hover .m-proj .l3 {
    transform: translate(8px, 6px);
  }
  .recent {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: center;
    max-width: 820px;
  }
  .rc {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 11px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12px;
  }
  .rc:hover {
    color: var(--text);
    background: var(--glass-2);
  }
  .drop {
    position: fixed;
    inset: 0;
    z-index: 250;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(var(--accent-rgb), 0.12);
    border: 2px dashed rgba(var(--accent-rgb), 0.8);
    border-radius: 12px;
    pointer-events: none;
  }
  .drop-in {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 28px 40px;
    font-size: 16px;
    font-weight: 600;
  }
  .toasts {
    position: fixed;
    left: 50%;
    bottom: 40px;
    transform: translateX(-50%);
    z-index: 260;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
    max-width: 640px;
    animation: pop 0.2s ease-out;
  }
  .tdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
    background: var(--accent);
  }
  .toast.ok .tdot {
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok);
  }
  .toast.error .tdot {
    background: var(--err);
    box-shadow: 0 0 8px var(--err);
  }
  .tt {
    line-height: 1.4;
  }
</style>
