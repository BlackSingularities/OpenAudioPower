<script lang="ts">
  // Real-time insert chain of a track (or the master bus).
  import Icon from "./Icon.svelte";
  import ParamEditor from "./ParamEditor.svelte";
  import { lang, t } from "../lib/i18n.svelte";
  import { openMenu } from "../lib/menu.svelte";
  import { EFFECTS, GROUPS, PRESETS, effectDef } from "../lib/effects";
  import { P, tp, commit, addEffect, applyPreset, track as getTrack, type Track, type FxSlot } from "../lib/store.svelte";

  let { target }: { target: Track | null } = $props();
  const L = (l: [string, string]) => (lang.current === "pl" ? l[0] : l[1]);
  const slots = $derived(target ? target.effects : P.p.master.effects);
  let open = $state<Record<number, boolean>>({});

  function list(): FxSlot[] {
    return target ? getTrack(target.id)!.effects : P.p.master.effects;
  }

  function addMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items = GROUPS.flatMap((g, gi) => [
      ...(gi ? [{ sep: true }] : []),
      ...EFFECTS.filter((x) => x.group === g.id).map((d) => ({ label: L(d.label), icon: "fx", run: () => {
        addEffect(target, d.type);
        const s = list();
        open[s[s.length - 1].id] = true;
      } })),
    ]);
    openMenu({ clientX: r.left, clientY: r.bottom + 4 }, items);
  }

  function presetMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ clientX: r.left, clientY: r.bottom + 4 }, PRESETS.map((p) => ({ label: L(p.label), icon: "sparkles", run: () => applyPreset(target, p.id) })));
  }

  function set(slot: FxSlot, key: string, v: any) {
    commit(() => {
      const s = list().find((x) => x.id === slot.id);
      if (s) s.spec[key] = v;
    }, { merge: `fx${slot.id}:${key}` });
  }

  function toggle(slot: FxSlot) {
    commit(() => {
      const s = list().find((x) => x.id === slot.id);
      if (s) s.on = !s.on;
    });
  }

  function remove(slot: FxSlot) {
    commit(() => {
      const l = list();
      l.splice(l.findIndex((x) => x.id === slot.id), 1);
    });
  }

  function move(slot: FxSlot, d: number) {
    commit(() => {
      const l = list();
      const i = l.findIndex((x) => x.id === slot.id);
      const j = i + d;
      if (j < 0 || j >= l.length) return;
      const [x] = l.splice(i, 1);
      l.splice(j, 0, x);
    });
  }

  const gr = $derived(target ? (tp.meters[target.id]?.gr ?? 0) : tp.masterGr);
</script>

<div class="chain">
  <div class="hdr">
    <h4>{t("effects")} <span class="cnt">{slots.length}</span></h4>
    {#if gr > 0.3}<span class="gr mono">GR −{gr.toFixed(1)}</span>{/if}
    <div class="grow"></div>
    <button class="hb" onclick={presetMenu} data-tip={t("presets")}><Icon name="sparkles" size={13} />{t("presets")}</button>
    <button class="hb add" onclick={addMenu} data-tip={t("addEffect")}><Icon name="plus" size={13} />{t("addShort")}</button>
  </div>
  {#if !slots.length}
    <div class="empty">{t("noEffects")}</div>
  {/if}
  {#each slots as slot, i (slot.id)}
    {@const def = effectDef(slot.spec.type)}
    <div class="slot" class:off={!slot.on}>
      <div class="sh">
        <button class="pw" class:on={slot.on} onclick={() => toggle(slot)} aria-label={t("bypass")} data-tip={t("bypass")}><Icon name="power" size={12} /></button>
        <button class="title" onclick={() => (open[slot.id] = !open[slot.id])}>
          <span class="n">{i + 1}</span>{def ? L(def.label) : slot.spec.type}
          <Icon name={open[slot.id] ? "up" : "down"} size={13} />
        </button>
        <button class="btn sm ghost icon" onclick={() => move(slot, -1)} disabled={i === 0} aria-label={t("moveUp")}><Icon name="up" size={13} /></button>
        <button class="btn sm ghost icon" onclick={() => move(slot, 1)} disabled={i === slots.length - 1} aria-label={t("moveDown")}><Icon name="down" size={13} /></button>
        <button class="btn sm ghost icon" onclick={() => remove(slot)} aria-label={t("remove")}><Icon name="trash" size={13} /></button>
      </div>
      {#if open[slot.id] && def}
        <div class="sb">
          <ParamEditor params={def.params} values={slot.spec} defaults={def.defaults} onchange={(k, v) => set(slot, k, v)} />
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .chain {
    display: flex;
    flex-direction: column;
  }
  .hdr {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }
  h4 {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    color: var(--text-3);
    font-weight: 650;
  }
  .cnt {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--glass-3);
    color: var(--text-2);
    font-size: 10px;
    letter-spacing: 0;
  }
  .gr {
    font-size: 10px;
    color: #fbbf24;
  }
  .hb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    border-radius: 7px;
    border: 1px solid var(--line-2);
    background: transparent;
    color: var(--text-2);
    font-size: 11px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .hb:hover {
    color: #fff;
    background: var(--glass-2);
  }
  .hb.add {
    border-color: transparent;
    background: rgba(var(--accent-rgb), 0.85);
    color: #fff;
    font-weight: 600;
  }
  .hb.add:hover {
    background: var(--accent);
  }
  .slot {
    border-top: 1px solid var(--line);
  }
  .slot:last-child {
    border-bottom: 1px solid var(--line);
  }
  .slot.off .title {
    opacity: 0.5;
  }
  .sh {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 5px 0;
  }
  .sh .btn {
    opacity: 0;
    transition: opacity 0.12s;
  }
  .slot:hover .sh .btn {
    opacity: 1;
  }
  .pw {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 1px solid var(--line-2);
    background: transparent;
    color: var(--text-3);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex: none;
  }
  .pw.on {
    color: #fff;
    background: rgba(var(--accent-rgb), 0.85);
    border-color: transparent;
    box-shadow: 0 0 10px rgba(var(--accent-rgb), 0.6);
  }
  .title {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: transparent;
    text-align: left;
    font-weight: 600;
    padding: 4px 6px;
    cursor: pointer;
    min-width: 0;
  }
  .title :global(svg) {
    margin-left: auto;
    color: var(--text-3);
  }
  .n {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    color: var(--text-2);
    background: var(--glass-3);
    font-family: var(--mono);
  }
  .sb {
    padding: 4px 2px 14px 30px;
  }
</style>
