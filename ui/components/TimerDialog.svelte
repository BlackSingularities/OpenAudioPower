<script lang="ts">
  // Timer recording and punch-in settings.
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { S, P, app, persist, scheduleRecording, cancelTimerRecording, punchRange } from "../lib/store.svelte";
  import { fmtTime } from "../lib/format";

  let mode = $state<"in" | "at">("in");
  let inMin = $state(1);
  let atTime = $state(new Date(Date.now() + 5 * 60000).toTimeString().slice(0, 5));
  let durMin = $state(30);
  let untilStop = $state(false);
  let now = $state(Date.now());

  $effect(() => {
    if (!app.dialogs.timer && !app.timerRec) return;
    const id = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(id);
  });

  function startAt(): number {
    if (mode === "in") return Date.now() + Math.max(0, inMin) * 60000;
    const [h, m] = atTime.split(":").map(Number);
    const d = new Date();
    d.setHours(h, m, 0, 0);
    if (d.getTime() < Date.now()) d.setDate(d.getDate() + 1);
    return d.getTime();
  }
  function go() {
    scheduleRecording(startAt(), untilStop ? 0 : Math.max(1, durMin * 60));
    app.dialogs.timer = false;
  }
  const left = $derived(app.timerRec ? Math.max(0, Math.round((app.timerRec.at - now) / 1000)) : 0);
  const fmtLeft = (s: number) => `${Math.floor(s / 3600)}:${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
  const punch = $derived(punchRange());
</script>

{#if app.dialogs.timer}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.timer = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>{t("recOptions")}</h2>
      <p class="sub">{t("recOptionsSub")}</p>

      <h3><Icon name="record" size={14} />{t("punchIn")}</h3>
      <label class="chk"><input type="checkbox" bind:checked={S.settings.punch} onchange={persist} />{t("punchOn")}</label>
      <div class="hint">
        {#if P.p.loop.end > P.p.loop.start}
          {t("punchRange")}: <b class="mono">{fmtTime(P.p.loop.start, app.rate)} – {fmtTime(P.p.loop.end, app.rate)}</b>
        {:else}
          {t("punchNeedsLoop")}
        {/if}
      </div>
      <label class="row"><span>{t("preroll")}</span><input type="number" min="0" max="10" step="0.5" bind:value={S.settings.preroll} onchange={persist} /> s</label>
      {#if punch}<div class="hint ok">{t("punchReady")}</div>{/if}

      <h3><Icon name="clock" size={14} />{t("timerRec")}</h3>
      {#if app.timerRec}
        <div class="armed">
          <span>{t("timerArmed")}</span><b class="mono">{fmtLeft(left)}</b>
          <button class="btn sm" onclick={cancelTimerRecording}>{t("cancel")}</button>
        </div>
      {:else}
        <div class="seg full">
          <button class:on={mode === "in"} onclick={() => (mode = "in")}>{t("timerIn")}</button>
          <button class:on={mode === "at"} onclick={() => (mode = "at")}>{t("timerAt")}</button>
        </div>
        {#if mode === "in"}
          <label class="row"><span>{t("timerIn")}</span><input type="number" min="0" max="1440" step="1" bind:value={inMin} /> min</label>
        {:else}
          <label class="row"><span>{t("timerAt")}</span><input type="time" bind:value={atTime} /></label>
        {/if}
        <label class="row"><span>{t("duration")}</span><input type="number" min="1" max="1440" step="1" bind:value={durMin} disabled={untilStop} /> min</label>
        <label class="chk"><input type="checkbox" bind:checked={untilStop} />{t("timerUntilStop")}</label>
      {/if}

      <div class="actions">
        <button class="btn ghost" onclick={() => (app.dialogs.timer = false)}>{t("close")}</button>
        {#if !app.timerRec}<button class="btn primary" onclick={go}><Icon name="clock" size={15} />{t("timerStart")}</button>{/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(520px, 100%);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 16px 0 10px;
    font: 600 13.5px var(--font-display);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .row span {
    flex: 1;
  }
  .row input {
    width: 110px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  .hint {
    margin: 6px 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .hint.ok {
    color: #86efac;
  }
  .armed {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgba(239, 68, 68, 0.12);
  }
  .armed span {
    flex: 1;
  }
  .seg.full {
    margin-bottom: 6px;
  }
  .seg.full button {
    flex: 1;
  }
</style>
