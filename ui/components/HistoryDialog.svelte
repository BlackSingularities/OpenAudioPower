<script lang="ts">
  // Undo history as a list: click a step to go back (or forward) to it.
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { app, hist, historyGoto } from "../lib/store.svelte";

  const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
</script>

{#if app.dialogs.history}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.history = false)}>
    <div class="dialog glass-strong" role="dialog" aria-modal="true">
      <h2>{t("historyTitle")}</h2>
      <p class="sub">{t("historySub")}</p>
      <div class="list">
        <button class="st" class:cur={hist.done.length === 0} onclick={() => historyGoto(0)}>
          <span class="n mono">0</span><span class="lbl">{t("historyStart")}</span>
        </button>
        {#each hist.done as h, i}
          <button class="st" class:cur={i === hist.done.length - 1} onclick={() => historyGoto(i + 1)}>
            <span class="n mono">{i + 1}</span><span class="lbl">{h.label}</span><span class="tm mono">{time(h.at)}</span>
          </button>
        {/each}
        {#each hist.undone as h, i}
          <button class="st undone" onclick={() => historyGoto(hist.done.length + i + 1)}>
            <span class="n mono">{hist.done.length + i + 1}</span><span class="lbl">{h.label}</span><span class="tm mono">{time(h.at)}</span>
          </button>
        {/each}
      </div>
      <div class="actions">
        <button class="btn ghost" onclick={() => (app.dialogs.history = false)}><Icon name="x" size={14} />{t("close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    width: min(480px, 100%);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 56vh;
    overflow: auto;
  }
  .st {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .st:hover {
    background: rgba(var(--accent-rgb), 0.15);
  }
  .st.cur {
    background: rgba(var(--accent-rgb), 0.3);
  }
  .st.undone {
    opacity: 0.45;
  }
  .n {
    width: 26px;
    color: var(--text-3);
    font-size: 11px;
  }
  .lbl {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tm {
    color: var(--text-3);
    font-size: 11px;
  }
</style>
