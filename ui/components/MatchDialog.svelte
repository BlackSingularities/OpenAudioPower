<script lang="ts">
  // Match EQ: compare the selected clip with a reference recording and apply the difference.
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { app, ed, processTargets, applyOp, toast } from "../lib/store.svelte";

  let reference = $state(0);
  let smoothing = $state(1 / 3);
  let amount = $state(0.75);
  let curve = $state<[number, number][]>([]);
  let busy = $state(false);
  let cv = $state<HTMLCanvasElement>();
  const W = 460;
  const H = 150;
  const DB = 12;

  const targets = $derived(app.dialogs.match ? processTargets(!!ed.sel) : []);
  const target = $derived(targets[0]);
  const refs = $derived(app.sources.filter((s) => s.id !== target?.clip.source));

  $effect(() => {
    if (app.dialogs.match && !refs.some((s) => s.id === reference)) reference = refs[0]?.id ?? 0;
  });
  $effect(() => {
    if (!app.dialogs.match) curve = [];
  });

  async function analyse() {
    if (!target || !reference) return;
    busy = true;
    try {
      curve = await api.matchEqCurve(target.clip.source, target.a, target.b, reference, 0, 0, smoothing);
    } catch (e) {
      toast(`${t("error")}: ${e}`, "error");
    } finally {
      busy = false;
    }
  }
  async function apply() {
    if (!curve.length) return;
    busy = true;
    try {
      await applyOp({ op: "matchEq", curve, amount }, !!ed.sel);
      app.dialogs.match = false;
    } finally {
      busy = false;
    }
  }

  const fx = (f: number) => (Math.log(f / 20) / Math.log(1000)) * W;
  const gy = (g: number) => H / 2 - (g / DB) * (H / 2 - 8);
  $effect(() => {
    if (!cv) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = W * dpr;
    cv.height = H * dpr;
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    ctx.font = "10px 'Segoe UI', sans-serif";
    for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) {
      const x = Math.round(fx(f)) + 0.5;
      ctx.fillStyle = "rgba(255,255,255,0.07)";
      ctx.fillRect(x, 0, 1, H);
      ctx.fillStyle = "rgba(255,255,255,0.35)";
      ctx.fillText(f >= 1000 ? f / 1000 + "k" : String(f), x + 3, H - 4);
    }
    for (const g of [-12, -6, 0, 6, 12]) {
      ctx.fillStyle = g === 0 ? "rgba(255,255,255,0.2)" : "rgba(255,255,255,0.06)";
      ctx.fillRect(0, Math.round(gy(g)), W, 1);
    }
    if (!curve.length) return;
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#8b5cf6";
    // full curve faint, applied amount strong
    for (const [k, style, w] of [[1, "rgba(255,255,255,0.3)", 1], [amount, accent, 2]] as [number, string, number][]) {
      ctx.strokeStyle = style;
      ctx.lineWidth = w;
      ctx.beginPath();
      curve.forEach(([f, g], i) => (i ? ctx.lineTo(fx(f), gy(g * k)) : ctx.moveTo(fx(f), gy(g * k))));
      ctx.stroke();
    }
  });
</script>

{#if app.dialogs.match}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && (app.dialogs.match = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>Match EQ</h2>
      <p class="sub">{t("matchSub")}</p>
      {#if !target}
        <div class="empty">{t("matchNoClip")}</div>
      {:else}
        <div class="form">
          <div class="row"><span>{t("matchTarget")}</span><b>{target.clip.name}{ed.sel ? ` (${t("selection").toLowerCase()})` : ""}</b></div>
          <label class="row">
            <span>{t("matchReference")}</span>
            <select bind:value={reference} onchange={() => (curve = [])}>
              {#each refs as s}<option value={s.id}>{s.name}</option>{/each}
            </select>
          </label>
          <Slider label={t("matchSmoothing")} bind:value={smoothing} min={0.1} max={1} step={0.01} unit="okt" def={1 / 3} onchange={() => (curve = [])} />
          <canvas bind:this={cv} style="width:{W}px;height:{H}px"></canvas>
          {#if !curve.length}<div class="hint">{t("matchHint")}</div>{/if}
          <Slider label={t("matchAmount")} bind:value={amount} min={0} max={1} step={0.01} unit="%" def={0.75} />
        </div>
      {/if}
      <div class="actions">
        <button class="btn ghost" disabled={busy} onclick={() => (app.dialogs.match = false)}>{t("cancel")}</button>
        <button class="btn" disabled={busy || !target || !reference} onclick={analyse}><Icon name="chart" size={15} />{t("matchAnalyse")}</button>
        <button class="btn primary" disabled={busy || !curve.length} onclick={apply}><Icon name="wand" size={15} />{t("apply")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(540px, 100%);
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .row select {
    min-width: 220px;
  }
  canvas {
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--line);
    max-width: 100%;
  }
  .hint,
  .empty {
    color: var(--text-3);
    font-size: 12px;
  }
  .empty {
    padding: 18px;
    text-align: center;
  }
</style>
