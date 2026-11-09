<script lang="ts">
  // Batch processing: one chain of offline operations / effects applied to many files,
  // written next to the originals or into a chosen folder.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import ParamEditor from "./ParamEditor.svelte";
  import { api } from "../lib/api";
  import { t, lang } from "../lib/i18n.svelte";
  import { OPS, EFFECTS, effectDef, type Param } from "../lib/effects";
  import { S, app, toast, cancelJob } from "../lib/store.svelte";

  const L = (l: [string, string]) => (lang.current === "pl" ? l[0] : l[1]);
  /** operations that make sense on a whole file without a selection */
  const BATCH_OPS = OPS.filter((o) => !["silence"].includes(o.op));

  interface Step {
    id: number;
    /** operation id, or "effect" with `values.type` naming the effect */
    kind: string;
    values: Record<string, any>;
    open: boolean;
  }

  let files = $state<string[]>([]);
  let steps = $state<Step[]>([{ id: 1, kind: "normalize", values: { mode: "lufs", target: -14, perChannel: false }, open: false }]);
  let nextId = 2;
  let recursive = $state(false);
  let outDir = $state("");
  let keepFormat = $state(false);
  let suffix = $state(lang.current === "pl" ? " (przetworzone)" : " (processed)");
  let format = $state(S.settings.export.format);
  let bitrate = $state(S.settings.export.bitrate ?? 192);
  let bits = $state(Math.min(24, S.settings.export.bits));
  let busy = $state(false);
  let results = $state<{ input: string; output: string | null; error: string | null }[]>([]);
  let adding = $state("");

  const FORMATS = ["wav", "flac", "mp3", "m4a", "ogg", "opus"];
  const RATES: Record<string, number[]> = { mp3: [128, 192, 256, 320], m4a: [96, 128, 160, 192], ogg: [128, 192, 256, 320], opus: [96, 128, 160, 256] };
  const lossy = $derived(format in RATES);
  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

  function stepDef(s: Step): { label: string; params: Param[]; defaults: Record<string, any> } {
    if (s.kind === "effect") {
      const d = effectDef(s.values.type)!;
      return { label: `${t("batchEffect")}: ${L(d.label)}`, params: d.params, defaults: d.defaults };
    }
    const d = OPS.find((o) => o.op === s.kind)!;
    return { label: L(d.label), params: d.params, defaults: d.defaults };
  }

  function addStep(v: string) {
    if (!v) return;
    const [kind, type] = v.split(":");
    const values = kind === "effect" ? { type, ...structuredClone(effectDef(type)!.defaults) } : structuredClone(OPS.find((o) => o.op === kind)!.defaults);
    steps.push({ id: nextId++, kind, values, open: true });
    adding = "";
  }
  function moveStep(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= steps.length) return;
    const [x] = steps.splice(i, 1);
    steps.splice(j, 0, x);
  }

  function addPaths(list: string[]) {
    const seen = new Set(files);
    for (const p of list) if (!seen.has(p)) files.push(p);
    results = [];
  }
  async function addFiles() {
    const f = await openDialog({ multiple: true, filters: [{ name: "Audio", extensions: ["wav", "flac", "mp3", "ogg", "m4a", "aac", "aif", "aiff", "opus", "webm", "mka"] }] });
    if (Array.isArray(f)) addPaths(f);
    else if (typeof f === "string") addPaths([f]);
  }
  async function addFolder() {
    const d = await openDialog({ directory: true });
    if (typeof d !== "string") return;
    addPaths(await api.listAudioFiles(d, recursive));
  }
  async function pickOut() {
    const d = await openDialog({ directory: true });
    if (typeof d === "string") outDir = d;
  }

  function payload() {
    return steps.map((s) => {
      const v = $state.snapshot(s.values);
      if (s.kind === "effect") return { op: "effect", spec: v, tail: effectDef(v.type)?.tail ?? 0 };
      return { op: s.kind, ...v };
    });
  }

  async function run() {
    busy = true;
    results = [];
    app.job = { kind: "batch", fraction: 0, label: "" };
    try {
      results = await api.batchProcess($state.snapshot(files), payload(), {
        format,
        bits: format === "flac" ? Math.min(24, bits) : bits,
        dither: true,
        bitrate,
        outDir,
        suffix,
        keepFormat,
      });
      const bad = results.filter((r) => r.error).length;
      toast(`${t("batchDone")}: ${results.length - bad}${bad ? ` · ${t("batchFailed")}: ${bad}` : ""}`, bad ? "error" : "ok", results.find((r) => r.output) ? { label: t("reveal"), run: () => revealItemInDir(results.find((r) => r.output)!.output!) } : undefined);
    } catch (e) {
      const m = String(e);
      toast(m === "cancelled" ? t("cancelled") : `${t("error")}: ${m}`, m === "cancelled" ? "info" : "error");
    } finally {
      app.job = null;
      busy = false;
    }
  }

  const status = (f: string) => results.find((r) => r.input === f);
</script>

{#if app.dialogs.batch}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && (app.dialogs.batch = false)}>
    <div class="dialog glass-strong batch" role="dialog" aria-modal="true">
      <h2>{t("batchTitle")}</h2>
      <p class="sub">{t("batchSub")}</p>

      <div class="cols">
        <section>
          <div class="head">
            <span class="label">{t("batchFiles")} · {files.length}</span>
            <div class="grow"></div>
            <button class="btn sm" disabled={busy} onclick={addFiles}><Icon name="file" size={13} />{t("batchAddFiles")}</button>
            <button class="btn sm" disabled={busy} onclick={addFolder}><Icon name="folder" size={13} />{t("batchAddFolder")}</button>
          </div>
          <label class="check"><input type="checkbox" bind:checked={recursive} />{t("batchSubfolders")}</label>
          <div class="list files">
            {#each files as f, i (f)}
              {@const st = status(f)}
              <div class="row" class:ok={st?.output} class:bad={st?.error} title={st?.error ?? st?.output ?? f}>
                <Icon name={st?.error ? "x" : st?.output ? "check" : "music"} size={13} />
                <span class="name">{baseName(f)}</span>
                {#if st?.error}<span class="err">{st.error}</span>{/if}
                <button class="x" disabled={busy} aria-label="remove" onclick={() => files.splice(i, 1)}><Icon name="x" size={12} /></button>
              </div>
            {:else}
              <div class="empty">{t("batchNoFiles")}</div>
            {/each}
          </div>
          {#if files.length}<button class="btn ghost sm small" disabled={busy} onclick={() => ((files = []), (results = []))}>{t("batchClear")}</button>{/if}
        </section>

        <section>
          <div class="head">
            <span class="label">{t("batchSteps")} · {steps.length}</span>
            <div class="grow"></div>
            <select class="add" bind:value={adding} onchange={() => addStep(adding)} disabled={busy}>
              <option value="">+ {t("batchAddStep")}</option>
              <optgroup label={t("tabProcess")}>
                {#each BATCH_OPS as o}<option value={o.op}>{L(o.label)}</option>{/each}
              </optgroup>
              <optgroup label={t("effects")}>
                {#each EFFECTS as e}<option value={"effect:" + e.type}>{L(e.label)}</option>{/each}
              </optgroup>
            </select>
          </div>
          <div class="list steps">
            {#each steps as s, i (s.id)}
              {@const d = stepDef(s)}
              <div class="step" class:open={s.open}>
                <div class="sh">
                  <span class="num mono">{i + 1}</span>
                  <button class="title" onclick={() => (s.open = !s.open)}>{d.label}</button>
                  <button class="x" disabled={busy || i === 0} aria-label="up" onclick={() => moveStep(i, -1)}><Icon name="up" size={12} /></button>
                  <button class="x" disabled={busy || i === steps.length - 1} aria-label="down" onclick={() => moveStep(i, 1)}><Icon name="down" size={12} /></button>
                  <button class="x" disabled={busy} aria-label="remove" onclick={() => steps.splice(i, 1)}><Icon name="trash" size={12} /></button>
                </div>
                {#if s.open && d.params.length}
                  <div class="params"><ParamEditor params={d.params} values={s.values} defaults={d.defaults} onchange={(k, v) => (s.values[k] = v)} /></div>
                {/if}
              </div>
            {:else}
              <div class="empty">{t("batchNoSteps")}</div>
            {/each}
          </div>
        </section>
      </div>

      <div class="out">
        <span class="label">{t("batchOutput")}</span>
        <div class="seg">
          <button class:on={!outDir} onclick={() => (outDir = "")}>{t("batchNextTo")}</button>
          <button class:on={!!outDir} onclick={pickOut} title={outDir}>{outDir ? baseName(outDir) : t("batchPickDir")}</button>
        </div>
        <label class="check"><input type="checkbox" bind:checked={keepFormat} />{t("batchKeepFormat")}</label>
        <div class="seg fmt" class:dim={keepFormat}>
          {#each FORMATS as f}<button class:on={format === f} onclick={() => ((format = f), RATES[f] && !RATES[f].includes(bitrate) && (bitrate = RATES[f][1]))}>{f.toUpperCase()}</button>{/each}
        </div>
        {#if lossy}
          <select bind:value={bitrate}>{#each RATES[format] as r}<option value={r}>{r} kbit/s</option>{/each}</select>
        {:else}
          <select bind:value={bits}><option value={16}>16 bit</option><option value={24}>24 bit</option>{#if format === "wav"}<option value={32}>32 bit float</option>{/if}</select>
        {/if}
        <label class="suffix"><span>{t("batchSuffix")}</span><input type="text" bind:value={suffix} /></label>
      </div>

      <div class="actions">
        {#if busy}
          <button class="btn ghost" onclick={cancelJob}>{t("cancel")}</button>
        {:else}
          <button class="btn ghost" onclick={() => (app.dialogs.batch = false)}>{t("close")}</button>
        {/if}
        <button class="btn primary lg" disabled={busy || !files.length} onclick={run}><Icon name="wand" size={16} />{t("batchRun")} ({files.length})</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .batch {
    width: min(900px, 100%);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 16px;
    margin-top: 14px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
  }
  .grow {
    flex: 1;
  }
  .list {
    height: 290px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.2);
    padding: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 7px;
    border-radius: 7px;
    font-size: 12px;
    color: var(--text-2);
  }
  .row:hover {
    background: var(--glass-2);
  }
  .row.ok {
    color: #86efac;
  }
  .row.bad {
    color: #fca5a5;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err {
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .x {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .x:hover:not(:disabled) {
    background: var(--glass-2);
    color: var(--text);
  }
  .x:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .empty {
    padding: 28px 12px;
    text-align: center;
    color: var(--text-3);
    font-size: 12px;
  }
  .step {
    border: 1px solid var(--line);
    border-radius: 9px;
    margin-bottom: 5px;
    background: rgba(255, 255, 255, 0.025);
  }
  .step.open {
    border-color: rgba(var(--accent-rgb), 0.5);
  }
  .sh {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 4px 4px 8px;
  }
  .num {
    width: 18px;
    font-size: 11px;
    color: var(--text-3);
  }
  .title {
    flex: 1;
    min-width: 0;
    text-align: left;
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .params {
    padding: 4px 10px 10px;
  }
  select.add {
    max-width: 190px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
  }
  .out {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 12px;
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .fmt.dim {
    opacity: 0.55;
  }
  .suffix {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .suffix input {
    width: 150px;
  }
  .small {
    align-self: flex-start;
    font-size: 11.5px;
  }
  @media (max-width: 760px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
