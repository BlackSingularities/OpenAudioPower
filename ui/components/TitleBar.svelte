<script lang="ts">
  // Custom frameless title bar: logo, application menus, project name, window buttons.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { openMenu, type MenuItem } from "../lib/menu.svelte";
  import { openFeedback } from "../lib/feedback.js";
  import {
    S, P, app, ed, hist, persist, newProject, openProject, saveProject, pickImport, pickImportFolder, undo, redo, copySelected, paste,
    duplicateSelected, deleteSelected, splitAt, selectAll, deleteRange, cropToRange, bounceRange, zoomAt, zoomFit, addTrackCmd,
    removeTrack, separateCmd, addMarker, toggleLoop,
    openSpectral,
    openRemix,
    karaoke, addLabel, addBusCmd, addReverbSend, freezeTrack, unfreezeTrack, track as trackOf } from "../lib/store.svelte";

  let { onUpdateCheck, onQuit }: { onUpdateCheck: () => void; onQuit: () => void } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);
  $effect(() => {
    win.isMaximized().then((m) => (maximized = m));
    const un = win.onResized(() => win.isMaximized().then((m) => (maximized = m)));
    return () => {
      un.then((f) => f());
    };
  });

  function show(e: MouseEvent, items: MenuItem[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ clientX: r.left, clientY: r.bottom + 4 }, items);
  }

  const recent = () =>
    S.settings.recent.slice(0, 6).map((p) => ({ label: p.split(/[\\/]/).pop()!, icon: "file", hint: "", run: () => openProject(p) }));

  const menus: { id: string; items: () => MenuItem[] }[] = [
    {
      id: "file",
      items: () => [
        { label: t("newProject"), icon: "file", hint: "Ctrl+N", run: newProject },
        { label: t("openProject"), icon: "folder", hint: "Ctrl+O", run: () => openProject() },
        ...(S.settings.recent.length ? [{ sep: true }, ...recent()] : []),
        { sep: true },
        { label: t("saveProject"), icon: "save", hint: "Ctrl+S", run: () => saveProject() },
        { label: t("saveProjectAs"), icon: "save", hint: "Ctrl+Shift+S", run: () => saveProject(true) },
        { sep: true },
        { label: t("importAudio"), icon: "import", hint: "Ctrl+I", run: pickImport },
        { label: t("importFolder"), icon: "folder", run: pickImportFolder },
        { sep: true },
        { label: t("exportMix"), icon: "export", hint: "Ctrl+E", run: () => ((app.exportMode = "mix"), (app.dialogs.export = true)) },
        { label: t("exportSelection"), icon: "export", disabled: !ed.sel, run: () => ((app.exportMode = "selection"), (app.dialogs.export = true)) },
        { label: t("exportTracks"), icon: "export", run: () => ((app.exportMode = "tracks"), (app.dialogs.export = true)) },
        { sep: true },
        { label: t("settings"), icon: "settings", hint: "Ctrl+,", run: () => (app.dialogs.settings = true) },
        { sep: true },
        { label: t("quit"), icon: "power", run: onQuit },
      ],
    },
    {
      id: "edit",
      items: () => [
        { label: t("undo"), icon: "undo", hint: "Ctrl+Z", disabled: !hist.canUndo, run: undo },
        { label: t("redo"), icon: "redo", hint: "Ctrl+Y", disabled: !hist.canRedo, run: redo },
        { label: t("historyTitle") + "…", icon: "history", hint: "Ctrl+H", run: () => (app.dialogs.history = true) },
        { sep: true },
        { label: t("cut"), icon: "scissors", hint: "Ctrl+X", disabled: !ed.selClips.length, run: () => copySelected(true) },
        { label: t("copy"), icon: "copy", hint: "Ctrl+C", disabled: !ed.selClips.length, run: () => copySelected() },
        { label: t("paste"), icon: "paste", hint: "Ctrl+V", run: paste },
        { label: t("duplicate"), icon: "copy", hint: "Ctrl+D", disabled: !ed.selClips.length, run: duplicateSelected },
        { label: t("delete"), icon: "trash", hint: "Del", run: deleteSelected },
        { sep: true },
        { label: t("split"), icon: "split", hint: "S", run: () => splitAt() },
        { label: t("deleteRange"), icon: "scissors", hint: "Ctrl+Del", disabled: !ed.sel, run: () => deleteRange(true) },
        { label: t("silenceRange"), icon: "mute", disabled: !ed.sel, run: () => deleteRange(false, true) },
        { label: t("cropToRange"), icon: "fit", disabled: !ed.sel, run: cropToRange },
        { sep: true },
        { label: t("selectAll"), icon: "check", hint: "Ctrl+A", run: selectAll },
        { label: t("deselect"), icon: "x", hint: "Esc", run: () => ((ed.selClips = []), (ed.sel = null)) },
        { label: t("addMarker"), icon: "marker", hint: "M", run: addMarker },
        { label: ed.sel ? t("addLabelSel") : t("addLabel"), icon: "label", hint: "Ctrl+L", run: () => addLabel() },
        { label: t("labelsTitle") + "…", icon: "label", hint: "Ctrl+Shift+L", run: () => (app.dialogs.labels = true) },
        { sep: true },
        { label: (ed.tool === "pencil" ? "✓ " : "   ") + t("pencil"), icon: "pencil", hint: "P", run: () => (ed.tool = ed.tool === "pencil" ? "select" : "pencil") },
      ],
    },
    {
      id: "view",
      items: () => [
        { label: t("zoomIn"), icon: "zoomIn", hint: "Ctrl++", run: () => zoomAt(1 / 1.5) },
        { label: t("zoomOut"), icon: "zoomOut", hint: "Ctrl+−", run: () => zoomAt(1.5) },
        { label: t("zoomFit"), icon: "fit", hint: "Ctrl+0", run: zoomFit },
        { sep: true },
        { label: (S.settings.ui.library ? "✓ " : "   ") + t("showLibrary"), icon: "sidebarL", hint: "F2", run: () => ((S.settings.ui.library = !S.settings.ui.library), persist()) },
        { label: (S.settings.ui.inspector ? "✓ " : "   ") + t("showInspector"), icon: "sidebarR", hint: "F3", run: () => ((S.settings.ui.inspector = !S.settings.ui.inspector), persist()) },
        { label: (S.settings.ui.dock === "mixer" ? "✓ " : "   ") + t("showMixer"), icon: "sliders", hint: "F4", run: () => ((S.settings.ui.dock = S.settings.ui.dock === "mixer" ? null : "mixer"), persist()) },
        { label: (S.settings.ui.dock === "analyzer" ? "✓ " : "   ") + t("showAnalyzer"), icon: "chart", hint: "F5", run: () => ((S.settings.ui.dock = S.settings.ui.dock === "analyzer" ? null : "analyzer"), persist()) },
        { sep: true },
        { label: (S.settings.snap ? "✓ " : "   ") + t("snap"), icon: "magnet", hint: "G", run: () => ((S.settings.snap = !S.settings.snap), persist()) },
        { label: (S.settings.follow ? "✓ " : "   ") + t("follow"), icon: "right", hint: "F", run: () => ((S.settings.follow = !S.settings.follow), persist()) },
      ],
    },
    {
      id: "tracks",
      items: () => [
        { label: t("addTrack"), icon: "plus", hint: "Ctrl+T", run: addTrackCmd },
        { label: t("addBus"), icon: "bus", hint: "Ctrl+Shift+T", run: () => addBusCmd() },
        { label: t("reverbSend"), icon: "sparkles", disabled: !ed.selTrack || !!trackOf(ed.selTrack)?.bus, run: () => ed.selTrack && addReverbSend([ed.selTrack]) },
        { label: t("removeTrack"), icon: "trash", disabled: !ed.selTrack, run: () => ed.selTrack && removeTrack(ed.selTrack) },
        { sep: true },
        trackOf(ed.selTrack)?.freeze
          ? { label: t("unfreeze"), icon: "snow", run: () => ed.selTrack && unfreezeTrack(ed.selTrack) }
          : { label: t("freeze"), icon: "snow", disabled: !trackOf(ed.selTrack)?.clips.length || !!trackOf(ed.selTrack)?.bus, run: () => ed.selTrack && freezeTrack(ed.selTrack) },
        { label: t("recOptions") + "…", icon: "clock", run: () => (app.dialogs.timer = true) },
        { sep: true },
        { label: t("loop"), icon: "loop", hint: "L", run: toggleLoop },
        { label: t("bounceRange"), icon: "bounce", run: bounceRange },
      ],
    },
    {
      id: "tools",
      items: () => [
        { label: t("generateTitle") + "…", icon: "tone", hint: "Ctrl+Shift+G", run: () => (app.dialogs.generate = true) },
        { sep: true },
        { label: t("stemSeparation"), icon: "stems", hint: "Ctrl+Shift+E", run: separateCmd },
        { label: t("remixTitle") + "…", icon: "sliders", hint: "Ctrl+Shift+M", run: () => openRemix() },
        { label: t("remixKaraokeCmd"), icon: "mic", hint: "Ctrl+Shift+K", run: karaoke },
        { label: t("tabProcess"), icon: "wand", run: () => ((S.settings.ui.inspector = true), (ed.inspectorTab = "process")) },
        { label: t("spectralTitle") + "…", icon: "chart", hint: "Ctrl+Shift+R", run: () => openSpectral() },
        { label: t("batchTitle") + "…", icon: "folder", hint: "Ctrl+B", run: () => (app.dialogs.batch = true) },
        { label: t("tabAnalysis"), icon: "chart", run: () => ((S.settings.ui.inspector = true), (ed.inspectorTab = "analysis")) },
        { label: t("tabMaster"), icon: "sliders", run: () => ((S.settings.ui.inspector = true), (ed.inspectorTab = "master")) },
      ],
    },
    {
      id: "help",
      items: () => [
        { label: t("shortcuts"), icon: "keyboard", hint: "F1", run: () => (app.dialogs.shortcuts = true) },
        { label: t("reportProblem"), icon: "bug", run: () => openFeedback() },
        { label: t("checkUpdates"), icon: "refresh", run: onUpdateCheck },
        { sep: true },
        { label: t("website"), icon: "globe", run: () => openUrl("https://redzikowski.com") },
        { label: t("about"), icon: "info", run: () => (app.dialogs.about = true) },
      ],
    },
  ];

  const title = $derived((P.p.name || (app.path ? app.path.split(/[\\/]/).pop()!.replace(/\.oap$/i, "") : "")) || "");
</script>

<header class="tb" data-tauri-drag-region>
  <div class="logo" data-tauri-drag-region>
    <img src="/icon.png" alt="" width="22" height="22" />
    <span class="name">Open<b>Audio</b>Power</span>
  </div>
  <nav class="menus">
    {#each menus as m}
      <button class="mi" onclick={(e) => show(e, m.items())}>{t(m.id)}</button>
    {/each}
  </nav>
  <div class="center" data-tauri-drag-region>
    {#if title}<span class="proj">{title}</span>{/if}
    {#if app.dirty && title}<span class="dirty" title="●"></span>{/if}
  </div>
  <div class="win">
    <button class="wb" aria-label="minimize" onclick={() => win.minimize()}><Icon name="win_min" size={14} /></button>
    <button class="wb" aria-label="maximize" onclick={() => win.toggleMaximize()}><Icon name={maximized ? "win_restore" : "win_max"} size={13} /></button>
    <button class="wb close" aria-label="close" onclick={() => win.close()}><Icon name="x" size={15} /></button>
  </div>
</header>

<style>
  .tb {
    display: flex;
    align-items: center;
    height: 38px;
    flex: none;
    padding-left: 12px;
    gap: 10px;
  }
  .logo {
    display: flex;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }
  .logo img {
    filter: drop-shadow(0 2px 6px rgba(var(--accent-rgb), 0.5));
  }
  .name {
    font: 500 13.5px var(--font-display);
    letter-spacing: 0.2px;
    color: var(--text-2);
  }
  .name b {
    color: var(--text);
    font-weight: 700;
  }
  .menus {
    display: flex;
    gap: 1px;
    margin-left: 6px;
  }
  .mi {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .mi:hover {
    background: var(--glass-2);
    color: var(--text);
  }
  .center {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 100%;
    color: var(--text-2);
    min-width: 0;
  }
  .proj {
    pointer-events: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dirty {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
    pointer-events: none;
  }
  .win {
    display: flex;
    height: 100%;
  }
  .wb {
    width: 46px;
    height: 100%;
    border: 0;
    background: transparent;
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .wb:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }
  .wb.close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
