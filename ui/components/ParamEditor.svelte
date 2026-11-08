<script lang="ts">
  // Generic parameter form for an effect spec or an offline operation.
  import Slider from "./Slider.svelte";
  import EqEditor from "./EqEditor.svelte";
  import Switch from "./Switch.svelte";
  import { lang } from "../lib/i18n.svelte";
  import type { Param } from "../lib/effects";

  let { params, values, defaults = {}, onchange }: { params: Param[]; values: Record<string, any>; defaults?: Record<string, any>; onchange: (key: string, v: any) => void } = $props();
  const L = (l: [string, string]) => (lang.current === "pl" ? l[0] : l[1]);
</script>

<div class="pe">
  {#if values.type === "eq"}
    <EqEditor bands={values.bands} onchange={(b) => onchange("bands", b)} />
  {/if}
  {#each params as p (p.key)}
    {#if p.kind === "bool"}
      <Switch label={L(p.label)} checked={!!values[p.key]} onchange={(v) => onchange(p.key, v)} />
    {:else if p.kind === "select"}
      <label class="sel">
        <span>{L(p.label)}</span>
        <select value={values[p.key]} onchange={(e) => onchange(p.key, (e.target as HTMLSelectElement).value)}>
          {#each p.options ?? [] as o}<option value={o.value}>{L(o.label)}</option>{/each}
        </select>
      </label>
    {:else}
      <Slider
        label={L(p.label)}
        value={values[p.key]}
        min={p.min ?? 0}
        max={p.max ?? 1}
        step={p.step ?? 0.01}
        unit={p.unit ?? ""}
        log={p.log ?? false}
        def={defaults[p.key]}
        onchange={(v) => onchange(p.key, v)}
      />
    {/if}
  {/each}
</div>

<style>
  .pe {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 12px;
    color: var(--text-2);
  }
  .sel select {
    height: 26px;
    font-size: 12px;
    min-width: 140px;
  }
</style>
