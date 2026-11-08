<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { S, persist } from "../lib/store.svelte";
  import { upd, openUpdate } from "../lib/updates.svelte";

  let { hidden = $bindable(false) }: { hidden?: boolean } = $props();
  const show = $derived(!!upd.available && !hidden && S.settings.skippedVersion !== upd.available.version);

  function skip() {
    if (upd.available) S.settings.skippedVersion = upd.available.version;
    persist();
  }
</script>

{#if show && upd.available}
  <div class="upd glass-strong" role="status">
    <span class="ic"><Icon name="download" size={18} /></span>
    <div class="txt">
      <b>{t("updAvailable")}: {upd.available.version}</b>
      <span>{t("updText")} {upd.current}.</span>
      <div class="btns">
        {#if upd.available.download}<button class="btn primary sm" onclick={openUpdate}><Icon name="download" size={14} />{t("updDownload")}</button>{/if}
        <button class="btn ghost sm" onclick={skip}>{t("updSkip")}</button>
      </div>
    </div>
    <button class="btn ghost sm icon" data-tip={t("updLater")} aria-label={t("updLater")} onclick={() => (hidden = true)}><Icon name="x" size={14} /></button>
  </div>
{/if}

<style>
  .upd {
    position: fixed;
    top: 96px;
    right: 16px;
    z-index: 150;
    width: 360px;
    display: flex;
    align-items: flex-start;
    gap: 11px;
    padding: 12px 10px 12px 13px;
    animation: pop 0.18s ease-out;
  }
  .ic {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(var(--accent-rgb), 0.85);
    color: #fff;
  }
  .txt {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .txt span {
    font-size: 12px;
    color: var(--text-2);
  }
  .btns {
    display: flex;
    gap: 6px;
    margin-top: 7px;
  }
</style>
