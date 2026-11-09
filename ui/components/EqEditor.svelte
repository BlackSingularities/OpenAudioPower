<script lang="ts">
  // Parametric EQ: response curve with draggable band handles + per-band controls.
  import { api } from "../lib/api";
  import { lang } from "../lib/i18n.svelte";
  import { FILTER_KINDS } from "../lib/effects";
  import Icon from "./Icon.svelte";

  let { bands, onchange }: { bands: any[]; onchange: (b: any[]) => void } = $props();

  let cv = $state<HTMLCanvasElement>();
  let curve = $state<number[]>([]);
  let sel = $state(1);
  const W = 290;
  const HH = 120;
  const DB = 18;
  const POINTS = 200;
  const L = (l: [string, string]) => (lang.current === "pl" ? l[0] : l[1]);

  const fx = (f: number) => (Math.log(f / 20) / Math.log(1000)) * W;
  const xf = (x: number) => 20 * Math.pow(1000, x / W);
  const gy = (g: number) => HH / 2 - (g / DB) * (HH / 2 - 6);
  const yg = (y: number) => ((HH / 2 - y) / (HH / 2 - 6)) * DB;

  let pending = 0;
  $effect(() => {
    const b = JSON.parse(JSON.stringify(bands));
    const id = ++pending;
    api.eqResponse(b, POINTS).then((r) => id === pending && (curve = r)).catch(() => {});
  });

  $effect(() => {
    if (!cv) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = W * dpr;
    cv.height = HH * dpr;
    const ctx = cv.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, HH);
    ctx.strokeStyle = "rgba(255,255,255,0.07)";
    ctx.lineWidth = 1;
    for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) {
      const x = Math.round(fx(f)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, HH);
      ctx.stroke();
    }
    for (const g of [-12, -6, 0, 6, 12]) {
      const y = Math.round(gy(g)) + 0.5;
      ctx.strokeStyle = g === 0 ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.06)";
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(W, y);
      ctx.stroke();
    }
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#8b5cf6";
    if (curve.length) {
      ctx.beginPath();
      curve.forEach((v, i) => {
        const x = (i / (POINTS - 1)) * W;
        const y = gy(Math.max(-DB * 1.2, Math.min(DB * 1.2, v)));
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.lineTo(W, gy(0));
      ctx.lineTo(0, gy(0));
      ctx.closePath();
      const grd = ctx.createLinearGradient(0, 0, 0, HH);
      grd.addColorStop(0, accent + "66");
      grd.addColorStop(1, accent + "08");
      ctx.fillStyle = grd;
      ctx.fill();
      ctx.beginPath();
      curve.forEach((v, i) => {
        const x = (i / (POINTS - 1)) * W;
        const y = gy(Math.max(-DB * 1.2, Math.min(DB * 1.2, v)));
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.strokeStyle = accent;
      ctx.lineWidth = 2;
      ctx.stroke();
    }
    bands.forEach((b, i) => {
      if (!b.on) return;
      const gainful = ["peak", "lowShelf", "highShelf"].includes(b.kind);
      const x = fx(b.freq);
      const y = gainful ? gy(b.gain) : gy(0);
      ctx.beginPath();
      ctx.arc(x, y, i === sel ? 6.5 : 5, 0, Math.PI * 2);
      ctx.fillStyle = i === sel ? "#fff" : "rgba(255,255,255,0.75)";
      ctx.fill();
      ctx.fillStyle = "#111";
      ctx.font = "700 8px sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(b.target === "mid" ? "M" : b.target === "side" ? "S" : String(i + 1), x, y + 0.5);
    });
  });

  function set(i: number, patch: any) {
    const nb = bands.map((b, j) => (j === i ? { ...b, ...patch } : b));
    onchange(nb);
  }

  function down(e: PointerEvent) {
    const r = cv!.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    let best = -1;
    let bd = 18;
    bands.forEach((b, i) => {
      if (!b.on) return;
      const gainful = ["peak", "lowShelf", "highShelf"].includes(b.kind);
      const d = Math.hypot(fx(b.freq) - x, (gainful ? gy(b.gain) : gy(0)) - y);
      if (d < bd) {
        bd = d;
        best = i;
      }
    });
    if (best < 0) return;
    sel = best;
    cv!.setPointerCapture(e.pointerId);
    const mv = (ev: PointerEvent) => {
      const nx = Math.min(W, Math.max(0, ev.clientX - r.left));
      const ny = Math.min(HH, Math.max(0, ev.clientY - r.top));
      const b = bands[best];
      const gainful = ["peak", "lowShelf", "highShelf"].includes(b.kind);
      set(best, { freq: Math.round(xf(nx)), ...(gainful ? { gain: Math.round(yg(ny) * 10) / 10 } : {}) });
    };
    const up = () => {
      cv!.removeEventListener("pointermove", mv);
      cv!.removeEventListener("pointerup", up);
    };
    cv!.addEventListener("pointermove", mv);
    cv!.addEventListener("pointerup", up);
  }

  function wheel(e: WheelEvent) {
    const b = bands[sel];
    if (!b) return;
    e.preventDefault();
    set(sel, { q: Math.min(10, Math.max(0.1, +(b.q * (e.deltaY > 0 ? 0.9 : 1.1)).toFixed(2))) });
  }

  const band = $derived(bands[sel]);
</script>

<div class="eq">
  <canvas bind:this={cv} style="width:{W}px;height:{HH}px" onpointerdown={down} onwheel={wheel}></canvas>
  <div class="bands">
    {#each bands as b, i}
      <button class="bn" class:on={i === sel} class:off={!b.on} onclick={() => (sel = i)}>{i + 1}</button>
    {/each}
    <button class="bn" onclick={() => onchange([...bands, { kind: "peak", freq: 1000, q: 1, gain: 0, on: true }])} aria-label="+"><Icon name="plus" size={12} /></button>
  </div>
  {#if band}
    <div class="bandctl">
      <label class="chk"><input type="checkbox" checked={band.on} onchange={(e) => set(sel, { on: (e.target as HTMLInputElement).checked })} /></label>
      <select value={band.kind} onchange={(e) => set(sel, { kind: (e.target as HTMLSelectElement).value })}>
        {#each FILTER_KINDS as k}<option value={k.value}>{L(k.label)}</option>{/each}
      </select>
      <input type="number" min="20" max="20000" value={band.freq} onchange={(e) => set(sel, { freq: +(e.target as HTMLInputElement).value })} title="Hz" />
      <input type="number" min="-24" max="24" step="0.5" value={band.gain} onchange={(e) => set(sel, { gain: +(e.target as HTMLInputElement).value })} title="dB" />
      <input type="number" min="0.1" max="10" step="0.05" value={band.q} onchange={(e) => set(sel, { q: +(e.target as HTMLInputElement).value })} title="Q" />
      <button class="btn sm ghost icon" onclick={() => (onchange(bands.filter((_, j) => j !== sel)), (sel = Math.max(0, sel - 1)))} aria-label="−"><Icon name="trash" size={13} /></button>
    </div>
    <div class="ms seg" role="group" aria-label="M/S">
      {#each [["stereo", "L+P", "L+R"], ["mid", "Środek (M)", "Mid (M)"], ["side", "Boki (S)", "Side (S)"]] as [v, pl, en]}
        <button class:on={(band.target ?? "stereo") === v} onclick={() => set(sel, { target: v })}>{lang.current === "pl" ? pl : en}</button>
      {/each}
    </div>
    <div class="units dim"><span></span><span></span><span>Hz</span><span>dB</span><span>Q</span><span></span></div>
  {/if}
</div>

<style>
  .ms {
    display: flex;
  }
  .ms button {
    flex: 1;
    font-size: 11px;
  }
  .eq {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  canvas {
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--line);
    cursor: crosshair;
    max-width: 100%;
  }
  .bands {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .bn {
    width: 26px;
    height: 22px;
    border-radius: 6px;
    border: 1px solid var(--line-2);
    background: transparent;
    font-size: 11px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .bn.on {
    background: rgba(var(--accent-rgb), 0.8);
    border-color: transparent;
    color: #fff;
  }
  .bn.off {
    opacity: 0.45;
  }
  .bandctl,
  .units {
    display: grid;
    grid-template-columns: 20px 1fr 62px 52px 50px 26px;
    gap: 4px;
    align-items: center;
  }
  .bandctl select,
  .bandctl input {
    height: 26px;
    font-size: 11.5px;
    width: 100%;
    padding: 0 5px;
  }
  .bandctl select {
    padding-right: 20px;
    background-position: right 4px center;
  }
  .units {
    font-size: 10px;
    text-align: center;
    margin-top: -4px;
  }
</style>
