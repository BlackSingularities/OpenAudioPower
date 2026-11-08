<script lang="ts">
  // Labelled slider with numeric readout; double-click resets, optional log scale.
  let {
    label = "",
    value = $bindable(0),
    min = 0,
    max = 1,
    step = 0.01,
    unit = "",
    log = false,
    def = undefined as number | undefined,
    compact = false,
    onchange = (_v: number) => {},
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
    compact?: boolean;
    onchange?: (v: number) => void;
    onend?: () => void;
  } = $props();

  const useLog = $derived(log && min > 0);
  const toPos = (v: number) => (useLog ? Math.log(v / min) / Math.log(max / min) : (v - min) / (max - min));
  const fromPos = (p: number) => (useLog ? min * Math.pow(max / min, p) : min + p * (max - min));
  const pos = $derived(Math.min(1, Math.max(0, toPos(value))));

  function quant(v: number) {
    const s = useLog ? (v < 100 ? (v < 10 ? 0.01 : 0.1) : 1) : step;
    return Math.min(max, Math.max(min, Math.round(v / s) * s));
  }

  function input(e: Event) {
    const p = +(e.target as HTMLInputElement).value / 1000;
    value = quant(fromPos(p));
    onchange(value);
  }

  function display(v: number): string {
    if (unit === "%") return Math.round(v * 100) + " %";
    const abs = Math.abs(v);
    const d = abs >= 1000 ? 0 : abs >= 100 ? 0 : abs >= 10 ? 1 : 2;
    let s = v.toFixed(Math.min(d, (String(step).split(".")[1] ?? "").length || d));
    if (unit === "Hz" && v >= 1000) s = (v / 1000).toFixed(v >= 10000 ? 1 : 2) + " k";
    return s.replace("-", "−") + (unit && unit !== "%" ? (unit === "Hz" && v >= 1000 ? "Hz" : " " + unit) : "");
  }

  function reset() {
    if (def !== undefined) {
      value = def;
      onchange(value);
      onend();
    }
  }

  let editing = $state(false);
  let draft = $state("");
  function startEdit() {
    draft = unit === "%" ? String(Math.round(value * 100)) : String(+value.toFixed(3));
    editing = true;
  }
  function commitEdit() {
    editing = false;
    const v = parseFloat(draft.replace(",", "."));
    if (isFinite(v)) {
      value = Math.min(max, Math.max(min, unit === "%" ? v / 100 : v));
      onchange(value);
      onend();
    }
  }
</script>

<div class="sl" class:compact>
  {#if label}
    <div class="top">
      <span class="lbl">{label}</span>
      {#if editing}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="val-edit" type="text" bind:value={draft} autofocus onblur={commitEdit} onkeydown={(e) => e.key === "Enter" && commitEdit()} />
      {:else}
        <button class="val mono" ondblclick={startEdit} onclick={startEdit}>{display(value)}</button>
      {/if}
    </div>
  {/if}
  <input
    type="range"
    min="0"
    max="1000"
    value={pos * 1000}
    style="--fill:{pos * 100}%"
    oninput={input}
    onchange={() => onend()}
    ondblclick={reset}
    aria-label={label}
  />
</div>

<style>
  .sl {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    height: 20px;
  }
  .lbl {
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .val {
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 11.5px;
    padding: 1px 4px;
    border-radius: 5px;
    cursor: text;
  }
  .val:hover {
    background: var(--glass-2);
  }
  .val-edit {
    width: 72px;
    height: 20px !important;
    font-size: 11.5px;
    text-align: right;
    padding: 0 5px !important;
  }
  .compact .top {
    height: 16px;
  }
</style>
