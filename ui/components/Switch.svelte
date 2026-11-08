<script lang="ts">
  let { checked = false, label = "", hint = "", onchange = (_v: boolean) => {} }: { checked?: boolean; label?: string; hint?: string; onchange?: (v: boolean) => void } = $props();
</script>

<button class="sw" class:on={checked} role="switch" aria-checked={checked} onclick={() => onchange(!checked)}>
  <span class="txt">
    <span class="lb">{label}</span>
    {#if hint}<span class="hn">{hint}</span>{/if}
  </span>
  <span class="tr"><i></i></span>
</button>

<style>
  .sw {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px 0;
    border: 0;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .txt {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .lb {
    font-size: 12.5px;
    color: var(--text);
  }
  .hn {
    font-size: 11px;
    color: var(--text-3);
  }
  .tr {
    position: relative;
    width: 34px;
    height: 19px;
    flex: none;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.12);
    transition: background 0.18s;
  }
  .tr i {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    transition: transform 0.18s cubic-bezier(0.3, 1.4, 0.5, 1);
  }
  .on .tr {
    background: var(--accent);
    box-shadow: 0 0 12px rgba(var(--accent-rgb), 0.5);
  }
  .on .tr i {
    transform: translateX(15px);
  }
</style>
