<script lang="ts">
  // Label list (Audacity-style label track): point and region labels, editing, import/export
  // of Audacity .txt label files and "export multiple" by labels.
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fmtTime } from "../lib/format";
  import { P, app, ed, commit, seek, addLabel, labelsToText, labelsFromText, labelRegions, toast } from "../lib/store.svelte";

  const labels = $derived([...P.p.markers].sort((a, b) => a.pos - b.pos));

  function rename(id: number, name: string) {
    commit(() => {
      const m = P.p.markers.find((x) => x.id === id);
      if (m) m.name = name;
    }, { merge: "lbl" + id, label: t("renameLabel") });
  }
  function remove(id: number) {
    commit(() => (P.p.markers = P.p.markers.filter((x) => x.id !== id)), { label: t("removeLabel") });
  }
  function toRegion(id: number) {
    commit(() => {
      const m = P.p.markers.find((x) => x.id === id);
      if (!m) return;
      if (m.end !== undefined) delete m.end;
      else if (ed.sel && ed.sel.end > ed.sel.start) {
        m.pos = ed.sel.start;
        m.end = ed.sel.end;
      } else m.end = m.pos + app.rate * 5;
    }, { label: t("editLabel") });
  }
  function go(m: { pos: number; end?: number }) {
    seek(m.pos);
    ed.sel = m.end !== undefined ? { start: m.pos, end: m.end } : null;
  }

  async function importTxt() {
    const f = await openDialog({ filters: [{ name: "Audacity labels", extensions: ["txt"] }] });
    if (typeof f !== "string") return;
    try {
      const n = labelsFromText(await api.readTextFile(f));
      toast(`${t("labelsImported")}: ${n}`, n ? "ok" : "info");
    } catch (e) {
      toast(`${t("error")}: ${e}`, "error");
    }
  }
  async function exportTxt() {
    const f = await saveDialog({ defaultPath: (P.p.name || "labels") + ".txt", filters: [{ name: "Audacity labels", extensions: ["txt"] }] });
    if (!f) return;
    try {
      await api.writeTextFile(f, labelsToText());
      toast(t("labelsExported"), "ok");
    } catch (e) {
      toast(`${t("error")}: ${e}`, "error");
    }
  }
  function exportMultiple() {
    app.exportMode = "labels";
    app.dialogs.labels = false;
    app.dialogs.export = true;
  }
  const parts = $derived(labelRegions().length);
</script>

{#if app.dialogs.labels}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.labels = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>{t("labelsTitle")}</h2>
      <p class="sub">{t("labelsSub")}</p>
      <div class="bar">
        <button class="btn sm" onclick={() => addLabel()}><Icon name="label" size={14} />{ed.sel ? t("addLabelSel") : t("addLabel")}</button>
        <div class="grow"></div>
        <button class="btn sm ghost" onclick={importTxt}><Icon name="import" size={14} />{t("labelsImport")}</button>
        <button class="btn sm ghost" disabled={!labels.length} onclick={exportTxt}><Icon name="export" size={14} />{t("labelsExport")}</button>
      </div>
      <div class="list">
        {#if !labels.length}
          <div class="empty">{t("labelsEmpty")}</div>
        {/if}
        {#each labels as m (m.id)}
          <div class="lb">
            <button class="kind" class:region={m.end !== undefined} onclick={() => toRegion(m.id)} data-tip={m.end !== undefined ? t("labelToPoint") : t("labelToRegion")}><Icon name={m.end !== undefined ? "fit" : "marker"} size={13} /></button>
            <input class="nm" type="text" value={m.name} onchange={(e) => rename(m.id, (e.target as HTMLInputElement).value)} />
            <button class="tm mono" onclick={() => go(m)} data-tip={t("goTo")}>
              {fmtTime(m.pos, app.rate)}{#if m.end !== undefined} – {fmtTime(m.end, app.rate)}{/if}
            </button>
            <button class="btn sm ghost icon" onclick={() => remove(m.id)} aria-label={t("remove")}><Icon name="trash" size={13} /></button>
          </div>
        {/each}
      </div>
      <div class="actions">
        <button class="btn ghost" onclick={() => (app.dialogs.labels = false)}>{t("close")}</button>
        <button class="btn primary" disabled={!parts} onclick={exportMultiple} data-tip={t("exportMultipleHint")}><Icon name="export" size={15} />{t("exportMultiple")} ({parts})</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(620px, 100%);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 46vh;
    overflow: auto;
  }
  .empty {
    padding: 22px;
    text-align: center;
    color: var(--text-3);
  }
  .lb {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
  }
  .kind {
    display: flex;
    border: 0;
    border-radius: 6px;
    padding: 5px;
    background: rgba(251, 191, 36, 0.15);
    color: #fbbf24;
    cursor: pointer;
  }
  .kind.region {
    background: rgba(56, 189, 248, 0.15);
    color: #38bdf8;
  }
  .nm {
    flex: 1;
    min-width: 0;
  }
  .tm {
    border: 0;
    background: transparent;
    color: var(--text-2);
    font-size: 11.5px;
    cursor: pointer;
    white-space: nowrap;
  }
  .tm:hover {
    color: var(--text);
  }
</style>
