<script lang="ts">
  // Rotary control: vertical drag (Shift = fine), wheel, double-click resets, click on value to type.
  let {
    label = "",
    value = 0,
    min = 0,
    max = 1,
    step = 0.01,
    unit = "",
    log = false,
    def = undefined as number | undefined,
    bipolar = false,
    size = 54,
    color = "",
    format = undefined as ((v: number) => string) | undefined,
    onchange = (_v: number) => {},
    onstart = () => {},
    onend = () => {},
  }: {
    label?: string;
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    unit?: string;
    log?: boolean;
    def?: number;
    bipolar?: boolean;
    size?: number;
    color?: string;
    format?: (v: number) => string;
    onchange?: (v: number) => void;
    onstart?: () => void;
    onend?: () => void;
  } = $props();

  const useLog = $derived(log && min > 0);
  const toPos = (v: number) => (useLog ? Math.log(v / min) / Math.log(max / min) : (v - min) / (max - min));
  const fromPos = (p: number) => (useLog ? min * Math.pow(max / min, p) : min + p * (max - min));
  const pos = $derived(Math.min(1, Math.max(0, toPos(value))));
  const A0 = -135;
  const A1 = 135;
  const ang = $derived(A0 + pos * (A1 - A0));
  const r = $derived(size / 2 - 5);
  const c = $derived(size / 2);

  function pt(a: number, rad: number) {
    const t = ((a - 90) * Math.PI) / 180;
    return [c + rad * Math.cos(t), c + rad * Math.sin(t)];
  }
  function arc(a0: number, a1: number, rad: number) {
    if (Math.abs(a1 - a0) < 0.01) return "";
    const [x0, y0] = pt(a0, rad);
    const [x1, y1] = pt(a1, rad);
    const large = Math.abs(a1 - a0) > 180 ? 1 : 0;
    const sweep = a1 > a0 ? 1 : 0;
    return `M ${x0} ${y0} A ${rad} ${rad} 0 ${large} ${sweep} ${x1} ${y1}`;
  }
  const zeroAng = $derived(bipolar ? A0 + Math.min(1, Math.max(0, toPos(0))) * (A1 - A0) : A0);

  function quant(v: number) {
    const s = useLog ? (v < 100 ? (v < 10 ? 0.01 : 0.1) : 1) : step;
    return Math.min(max, Math.max(min, Math.round(v / s) * s));
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    let p = pos;
    let y = e.clientY;
    onstart();
    const mv = (ev: PointerEvent) => {
      const k = ev.shiftKey ? 0.0012 : 0.006;
      p = Math.min(1, Math.max(0, p + (y - ev.clientY) * k));
      y = ev.clientY;
      onchange(quant(fromPos(p)));
    };
    const up = () => {
      el.removeEventListener("pointermove", mv);
      el.removeEventListener("pointerup", up);
      onend();
    };
    el.addEventListener("pointermove", mv);
    el.addEventListener("pointerup", up);
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    onstart();
    const p = Math.min(1, Math.max(0, pos + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 0.005 : 0.025)));
    onchange(quant(fromPos(p)));
    onend();
  }

  function reset() {
    if (def === undefined) return;
    onstart();
    onchange(def);
    onend();
  }

  function display(v: number) {
    if (format) return format(v);
    if (unit === "%") return Math.round(v * 100) + "%";
    if (unit === "Hz" && v >= 1000) return (v / 1000).toFixed(v >= 10000 ? 1 : 2) + "k";
    const a = Math.abs(v);
    const s = v.toFixed(a >= 100 ? 0 : a >= 10 ? 1 : 2).replace("-", "−");
    return (v > 0 && bipolar ? "+" : "") + s;
  }

  let editing = $state(false);
  let draft = $state("");
  function startEdit() {
    draft = String(unit === "%" ? Math.round(value * 100) : +value.toFixed(3));
    editing = true;
  }
  function commitEdit() {
    editing = false;
    const v = parseFloat(draft.replace(",", ".").replace("−", "-"));
    if (isFinite(v)) {
      onstart();
      onchange(Math.min(max, Math.max(min, unit === "%" ? v / 100 : v)));
      onend();
    }
  }
</script>

<div class="knob" style="--k:{color || 'var(--accent)'}">
  <div class="dial" role="slider" tabindex="0" aria-label={label} aria-valuenow={value} aria-valuemin={min} aria-valuemax={max} onpointerdown={down} onwheel={wheel} ondblclick={reset} style="width:{size}px;height:{size}px">
    <svg width={size} height={size} viewBox="0 0 {size} {size}">
      <path d={arc(A0, A1, r)} class="track" />
      <path d={arc(Math.min(zeroAng, ang), Math.max(zeroAng, ang), r)} class="val" />
      <circle cx={c} cy={c} r={r - 7} class="cap" />
      <line x1={pt(ang, r - 15)[0]} y1={pt(ang, r - 15)[1]} x2={pt(ang, r - 8)[0]} y2={pt(ang, r - 8)[1]} class="ptr" />
    </svg>
  </div>
  {#if editing}
    <!-- svelte-ignore a11y_autofocus -->
    <input class="edit mono" type="text" bind:value={draft} autofocus onblur={commitEdit} onkeydown={(e) => e.key === "Enter" && commitEdit()} />
  {:else}
    <button class="v mono" onclick={startEdit}>{display(value)}<small>{unit && unit !== "%" ? unit : ""}</small></button>
  {/if}
  {#if label}<div class="l">{label}</div>{/if}
</div>

<style>
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    min-width: 0;
  }
  .dial {
    cursor: ns-resize;
    border-radius: 50%;
    touch-action: none;
  }
  .dial:focus-visible {
    outline-offset: 0;
  }
  svg {
    display: block;
    overflow: visible;
  }
  .track {
    fill: none;
    stroke: rgba(255, 255, 255, 0.1);
    stroke-width: 4;
    stroke-linecap: round;
  }
  .val {
    fill: none;
    stroke: var(--k);
    stroke-width: 4;
    stroke-linecap: round;
    filter: drop-shadow(0 0 4px color-mix(in srgb, var(--k) 70%, transparent));
  }
  .cap {
    fill: url(#none);
    fill: rgba(255, 255, 255, 0.06);
    stroke: rgba(255, 255, 255, 0.12);
    stroke-width: 1;
  }
  .ptr {
    stroke: #fff;
    stroke-width: 2.4;
    stroke-linecap: round;
  }
  .v {
    border: 0;
    background: transparent;
    font-size: 11.5px;
    color: var(--text);
    padding: 1px 4px;
    border-radius: 5px;
    cursor: text;
    white-space: nowrap;
  }
  .v small {
    margin-left: 2px;
    color: var(--text-3);
    font-size: 9.5px;
  }
  .v:hover {
    background: var(--glass-2);
  }
  .edit {
    width: 58px;
    height: 20px !important;
    font-size: 11px;
    text-align: center;
    padding: 0 3px !important;
  }
  .l {
    font-size: 10px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-3);
    text-align: center;
    white-space: nowrap;
  }
</style>
