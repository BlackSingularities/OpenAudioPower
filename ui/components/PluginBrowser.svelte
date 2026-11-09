<script lang="ts">
  // VST3 / CLAP browser: scans the plug-in folders (each file is opened by a short-lived helper
  // process, results are cached) and adds the chosen effect to a track or the master.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { api, type PluginDesc, type PluginScanEntry } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { S, app, persist, addPlugin, track as getTrack } from "../lib/store.svelte";

  let entries = $state<PluginScanEntry[]>([]);
  let defaults = $state<string[]>([]);
  let query = $state("");
  let scanning = $state(false);
  let showFailed = $state(false);
  let showFolders = $state(false);
  let error = $state("");

  const dirs = $derived([...defaults, ...(S.settings.plugins?.dirs ?? [])]);
  const all = $derived(entries.flatMap((e) => e.plugins));
  const effects = $derived(all.filter((p) => !p.instrument));
  const instruments = $derived(all.length - effects.length);
  const failed = $derived(entries.filter((e) => e.error));
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? effects.filter((p) => `${p.name} ${p.vendor} ${p.category}`.toLowerCase().includes(q)) : effects;
    return [...list].sort((a, b) => a.vendor.localeCompare(b.vendor) || a.name.localeCompare(b.name));
  });
  const groups = $derived.by(() => {
    const m = new Map<string, PluginDesc[]>();
    for (const p of filtered) {
      const k = p.vendor || "—";
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(p);
    }
    return [...m.entries()];
  });
  const targetName = $derived(app.pluginTarget === "master" ? "Master" : app.pluginTarget != null ? (getTrack(app.pluginTarget)?.name ?? "") : "");

  async function scan(full: boolean) {
    scanning = true;
    error = "";
    app.job = { kind: "scan", fraction: 0, label: "" };
    try {
      entries = await api.pluginScan($state.snapshot(dirs), full);
    } catch (e) {
      error = String(e);
    } finally {
      app.job = null;
      scanning = false;
    }
  }

  $effect(() => {
    if (!app.dialogs.plugins) return;
    (async () => {
      if (!defaults.length) defaults = await api.pluginDirs().catch(() => []);
      await scan(false);
    })();
  });

  async function addFolder() {
    const d = await openDialog({ directory: true });
    if (typeof d !== "string") return;
    if (!S.settings.plugins) S.settings.plugins = { dirs: [] };
    if (!S.settings.plugins.dirs.includes(d)) S.settings.plugins.dirs.push(d);
    persist();
    await scan(false);
  }
  function removeFolder(d: string) {
    S.settings.plugins.dirs = S.settings.plugins.dirs.filter((x: string) => x !== d);
    persist();
    scan(false);
  }

  async function pick(p: PluginDesc) {
    const target = app.pluginTarget;
    if (target == null) return;
    app.dialogs.plugins = false;
    await addPlugin(target, $state.snapshot(p));
  }

  const fileName = (f: string) => f.split(/[\\/]/).pop() ?? f;
</script>

{#if app.dialogs.plugins}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !scanning && (app.dialogs.plugins = false)}>
    <div class="dialog glass-strong pb" role="dialog" aria-modal="true">
      <div class="top">
        <div>
          <h2>{t("pluginsTitle")}</h2>
          <p class="sub">{t("pluginsSub")}{targetName ? ` · ${targetName}` : ""}</p>
        </div>
      </div>

      <div class="bar">
        <input type="search" placeholder={t("pluginsSearch")} bind:value={query} />
        <button class="btn sm" disabled={scanning} onclick={() => (showFolders = !showFolders)}><Icon name="folder" size={13} />{t("pluginsFolders")} · {dirs.length}</button>
        <button class="btn sm" disabled={scanning} onclick={() => scan(true)}><Icon name="refresh" size={13} />{t("pluginsRescan")}</button>
      </div>

      {#if showFolders}
        <div class="folders">
          {#each defaults as d}<div class="fr mono"><Icon name="folder" size={12} />{d}</div>{/each}
          {#each S.settings.plugins?.dirs ?? [] as d}
            <div class="fr mono"><Icon name="folder" size={12} />{d}<button class="x" aria-label="remove" onclick={() => removeFolder(d)}><Icon name="x" size={11} /></button></div>
          {/each}
          <button class="btn sm ghost" onclick={addFolder}><Icon name="plus" size={13} />{t("pluginsAddFolder")}</button>
        </div>
      {/if}

      <div class="list">
        {#if scanning && !entries.length}
          <div class="empty">{t("job_scan")}…</div>
        {:else if error}
          <div class="empty err">{error}</div>
        {:else if !effects.length}
          <div class="empty">{t("pluginsNone")}</div>
        {:else}
          {#each groups as [vendor, list] (vendor)}
            <div class="vendor">{vendor}</div>
            {#each list as p (p.format + p.path + p.id)}
              <button class="item" onclick={() => pick(p)}>
                <span class="fmt mono">{p.format.toUpperCase()}</span>
                <span class="nm">{p.name}</span>
                <span class="cat">{p.category.split("|").filter((x) => x && !["audio-effect", "Fx"].includes(x)).slice(0, 3).join(" · ")}</span>
                <Icon name="plus" size={14} />
              </button>
            {/each}
          {/each}
        {/if}
      </div>

      <div class="foot">
        {#if instruments}<span class="note">{t("pluginsInstruments")} ({instruments})</span>{/if}
        {#if failed.length}
          <button class="link" onclick={() => (showFailed = !showFailed)}>{t("pluginsFailed")}: {failed.length}</button>
        {/if}
      </div>
      {#if showFailed}
        <div class="failed">
          {#each failed as f}<div class="fr mono" title={f.file}>{fileName(f.file)} — {f.error}</div>{/each}
        </div>
      {/if}

      <div class="actions">
        <button class="btn ghost" disabled={scanning} onclick={() => (app.dialogs.plugins = false)}>{t("close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .pb {
    width: min(720px, 100%);
  }
  .bar {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .bar input {
    flex: 1;
    min-width: 0;
  }
  .folders,
  .failed {
    margin-top: 10px;
    padding: 8px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.18);
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow: auto;
  }
  .fr {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-2);
    word-break: break-all;
  }
  .list {
    margin-top: 10px;
    height: 360px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.2);
    padding: 4px;
  }
  .vendor {
    padding: 8px 8px 4px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .item {
    width: 100%;
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr) auto 16px;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .item:hover {
    background: rgba(var(--accent-rgb), 0.18);
  }
  .fmt {
    font-size: 9.5px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--glass-3);
    color: var(--text-2);
    text-align: center;
  }
  .nm {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cat {
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .empty {
    padding: 40px 20px;
    text-align: center;
    color: var(--text-3);
    font-size: 12.5px;
  }
  .empty.err {
    color: #fca5a5;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-top: 8px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .link {
    border: 0;
    background: transparent;
    color: #fca5a5;
    cursor: pointer;
    font-size: 11.5px;
    padding: 0;
  }
  .x {
    margin-left: auto;
    border: 0;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
</style>
