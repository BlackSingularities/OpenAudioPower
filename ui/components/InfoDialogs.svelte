<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "./Icon.svelte";
  import { t, lang } from "../lib/i18n.svelte";
  import { app } from "../lib/store.svelte";
  import { upd } from "../lib/updates.svelte";

  const shortcuts: [string, string, string][] = [
    ["Space", "Odtwarzaj / pauza", "Play / pause"],
    ["R", "Nagrywaj", "Record"],
    ["Home / End", "Początek / koniec", "Start / end"],
    ["L", "Pętla", "Loop"],
    ["K", "Metronom", "Metronome"],
    ["S", "Podziel klip w miejscu głowicy", "Split clip at playhead"],
    ["M", "Dodaj znacznik", "Add marker"],
    ["G", "Przyciąganie do siatki", "Snap to grid"],
    ["F", "Śledź głowicę", "Follow playhead"],
    ["Del", "Usuń klipy / zakres", "Delete clips / range"],
    ["Ctrl+Del", "Usuń zakres z przesunięciem", "Ripple-delete range"],
    ["Ctrl+Z / Ctrl+Y", "Cofnij / ponów", "Undo / redo"],
    ["Ctrl+C / X / V", "Kopiuj / wytnij / wklej", "Copy / cut / paste"],
    ["Ctrl+D", "Duplikuj", "Duplicate"],
    ["Ctrl+A", "Zaznacz wszystko", "Select all"],
    ["Ctrl+T", "Nowa ścieżka", "New track"],
    ["Ctrl+I", "Importuj audio", "Import audio"],
    ["Ctrl+E", "Eksportuj miks", "Export mix"],
    ["Ctrl+Shift+E", "Separacja ścieżek", "Stem separation"],
    ["Ctrl+S / Ctrl+O", "Zapisz / otwórz projekt", "Save / open project"],
    ["Ctrl+kółko", "Powiększenie", "Zoom"],
    ["Shift+kółko", "Przewijanie w poziomie", "Horizontal scroll"],
    ["Ctrl+0", "Dopasuj do okna", "Fit to window"],
    ["← / →", "Przesuń głowicę (Shift = 10×)", "Nudge playhead (Shift = 10×)"],
    ["Alt+przeciąganie", "Zaznacz zakres na klipie", "Select range over a clip"],
    ["Ctrl+przeciąganie", "Bez przyciągania", "Drag without snapping"],
    ["F2 / F3 / F4 / F5", "Biblioteka / inspektor / mikser / analizator", "Library / inspector / mixer / analyser"],
  ];
</script>

{#if app.dialogs.about}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.about = false)}>
    <div class="dialog glass-strong about" role="dialog" aria-modal="true">
      <img src="/icon.png" alt="" width="84" height="84" />
      <h2>OpenAudioPower</h2>
      <div class="dim">{t("version")} {upd.current || app.info?.version}</div>
      <p class="sub">{t("aboutText")}</p>
      <p class="small dim">{t("aboutModels")}</p>
      <p class="small">{t("author")}: <button class="link" onclick={() => openUrl("https://redzikowski.com")}>redzikowski.com</button></p>
      <div class="actions center">
        <button class="btn" onclick={() => openUrl("https://redzikowski.com")}><Icon name="globe" size={15} />redzikowski.com</button>
        <button class="btn primary" onclick={() => (app.dialogs.about = false)}>{t("close")}</button>
      </div>
    </div>
  </div>
{/if}

{#if app.dialogs.shortcuts}
  <div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (app.dialogs.shortcuts = false)}>
    <div class="dialog glass-strong sc" role="dialog" aria-modal="true">
      <h2>{t("shortcuts")}</h2>
      <div class="list">
        {#each shortcuts as [k, pl, en]}
          <kbd>{k}</kbd><span>{lang.current === "pl" ? pl : en}</span>
        {/each}
      </div>
      <div class="actions"><button class="btn primary" onclick={() => (app.dialogs.shortcuts = false)}>{t("close")}</button></div>
    </div>
  </div>
{/if}

<style>
  .about {
    text-align: center;
    width: min(440px, 100%);
  }
  .about img {
    filter: drop-shadow(0 10px 30px rgba(var(--accent-rgb), 0.55));
    margin-bottom: 8px;
  }
  .about .sub {
    margin-top: 14px;
  }
  .small {
    font-size: 12px;
    line-height: 1.5;
  }
  .center {
    justify-content: center !important;
  }
  .link {
    border: 0;
    background: none;
    color: #c4b5fd;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  .sc {
    width: min(640px, 100%);
  }
  .list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 7px 16px;
    align-items: center;
    margin-top: 10px;
  }
  kbd {
    font-family: var(--mono);
    font-size: 11.5px;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--glass-3);
    border: 1px solid var(--line-2);
    box-shadow: inset 0 -2px 0 rgba(0, 0, 0, 0.3);
    white-space: nowrap;
    justify-self: start;
  }
</style>
