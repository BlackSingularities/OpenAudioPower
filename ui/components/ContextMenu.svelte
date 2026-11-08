<script lang="ts">
  import Icon from "./Icon.svelte";
  import { menu, closeMenu } from "../lib/menu.svelte";

  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    if (!menu.open) return;
    pos = { x: menu.x, y: menu.y };
    requestAnimationFrame(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      pos = {
        x: Math.min(menu.x, window.innerWidth - r.width - 8),
        y: menu.y + r.height > window.innerHeight - 8 ? Math.max(8, menu.y - r.height) : menu.y,
      };
    });
  });

  function run(f?: () => void) {
    closeMenu();
    f?.();
  }
</script>

<svelte:window onkeydown={(e) => menu.open && e.key === "Escape" && closeMenu()} onblur={closeMenu} />

{#if menu.open}
  <div class="catch" role="presentation" onpointerdown={closeMenu} oncontextmenu={(e) => (e.preventDefault(), closeMenu())}></div>
  <div class="menu glass-strong" bind:this={el} style="left:{pos.x}px;top:{pos.y}px" role="menu">
    {#each menu.items as it}
      {#if it.sep}
        <div class="sep"></div>
      {:else}
        <button class="item" class:danger={it.danger} disabled={it.disabled} role="menuitem" onclick={() => run(it.run)}>
          <span class="ic">{#if it.icon}<Icon name={it.icon} size={15} />{/if}</span>
          <span class="lbl">{it.label}</span>
          {#if it.hint}<span class="hint">{it.hint}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .catch {
    position: fixed;
    inset: 0;
    z-index: 300;
  }
  .menu {
    position: fixed;
    z-index: 301;
    min-width: 220px;
    padding: 5px;
    border-radius: 12px;
    animation: pop 0.12s ease-out;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 30px;
    padding: 0 10px 0 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  .item:hover:not(:disabled) {
    background: rgba(var(--accent-rgb), 0.25);
  }
  .item:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .item.danger {
    color: #fca5a5;
  }
  .ic {
    width: 16px;
    display: flex;
    color: var(--text-2);
  }
  .lbl {
    flex: 1;
  }
  .hint {
    color: var(--text-3);
    font-size: 11px;
    font-family: var(--mono);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
</style>
