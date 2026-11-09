<script lang="ts">
  // Auto Duck: lower one track (music) wherever another track (voice) plays, as volume automation.
  import Icon from "./Icon.svelte";
  import Slider from "./Slider.svelte";
  import { t } from "../lib/i18n.svelte";
  import { P, app, ed, track, autoDuck, toast } from "../lib/store.svelte";

  let control = $state(0);
  let threshold = $state(-35);
  let duck = $state(-12);
  let fall = $state(300);
  let rise = $state(600);
  let hold = $state(500);
  let useRange = $state(false);
  let busy = $state(false);

  const target = $derived(track(app.duckTarget));
  const others = $derived(P.p.tracks.filter((x) => x.id !== app.duckTarget && !x.bus));

  $effect(() => {
    if (app.dialogs.duck) {
      useRange = !!ed.sel;
      if (!others.some((x) => x.id === control)) control = others[0]?.id ?? 0;
    }
  });

  async function go() {
    if (!target || !control) return;
    busy = true;
    try {
      const n = await autoDuck(target.id, control, { threshold, duck, fall, rise, hold }, useRange);
      if (n) {
        toast(`${t("duckDone")}: ${n}`, "ok");
        app.dialogs.duck = false;
      } else toast(t("duckNone"), "info");
    } finally {
      busy = false;
    }
  }
</script>

{#if app.dialogs.duck && target}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && (app.dialogs.duck = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>Auto Duck</h2>
      <p class="sub">{t("duckSub")}</p>
      <div class="form">
        <div class="row"><span>{t("duckTarget")}</span><b>{target.name}</b></div>
        <label class="row">
          <span>{t("duckControl")}</span>
          <select bind:value={control}>
            {#each others as x}<option value={x.id}>{x.name}</option>{/each}
          </select>
        </label>
        <Slider label={t("duckThreshold")} bind:value={threshold} min={-70} max={-6} step={0.5} unit="dB" def={-35} />
        <Slider label={t("duckAmount")} bind:value={duck} min={-40} max={-1} step={0.5} unit="dB" def={-12} />
        <Slider label={t("duckFall")} bind:value={fall} min={0} max={3000} step={10} unit="ms" def={300} />
        <Slider label={t("duckRise")} bind:value={rise} min={0} max={5000} step={10} unit="ms" def={600} />
        <Slider label={t("duckHold")} bind:value={hold} min={0} max={5000} step={10} unit="ms" def={500} />
        <label class="chk"><input type="checkbox" bind:checked={useRange} disabled={!ed.sel} />{t("duckOnlySel")}</label>
      </div>
      <div class="actions">
        <button class="btn ghost" disabled={busy} onclick={() => (app.dialogs.duck = false)}>{t("cancel")}</button>
        <button class="btn primary lg" disabled={busy || !control} onclick={go}><Icon name="chart" size={16} />{t("duckGo")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(520px, 100%);
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
    min-width: 200px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
</style>
