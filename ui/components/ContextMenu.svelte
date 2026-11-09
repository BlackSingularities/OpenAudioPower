<script lang="ts">
  import Icon from "./Icon.svelte";
  import { menu, closeMenu, type MenuItem } from "../lib/menu.svelte";
  import { labelNext } from "../lib/store.svelte";

  let el = $state<HTMLDivElement>();
  let subEl = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  // open submenu: index of the parent item and where to put it
  let sub = $state<{ i: number; items: MenuItem[]; x: number; y: number; left: number } | null>(null);

  $effect(() => {
    if (!menu.open) return;
    sub = null;
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

  function openSub(i: number, it: MenuItem, e: Event) {
    if (!it.children || sub?.i === i) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    sub = { i, items: it.children, x: r.right + 2, y: r.top - 5, left: r.left };
    requestAnimationFrame(() => {
      if (!subEl || !sub) return;
      const s = subEl.getBoundingClientRect();
      // flip to the left when there is no room on the right, keep inside the window vertically
      const x = sub.x + s.width > window.innerWidth - 8 ? Math.max(8, sub.left - s.width - 2) : sub.x;
      const y = Math.max(8, Math.min(sub.y, window.innerHeight - s.height - 8));
      sub = { ...sub, x, y };
    });
  }

  function run(f?: () => void, label?: string) {
    closeMenu();
    // the history list names the step after the menu item that made it
    if (label) labelNext(label.replace(/^[✓ ]+/, "").replace(/…$/, ""));
    f?.();
  }
</script>

<svelte:window onkeydown={(e) => menu.open && e.key === "Escape" && closeMenu()} onblur={closeMenu} />

{#snippet item(it: MenuItem, i: number, top: boolean)}
  {#if it.sep}
    <div class="sep"></div>
  {:else if it.children}
    <button
      class="item"
      class:on={top && sub?.i === i}
      disabled={it.disabled}
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={sub?.i === i}
      onpointerenter={(e) => openSub(i, it, e)}
      onclick={(e) => openSub(i, it, e)}
    >
      <span class="ic">{#if it.icon}<Icon name={it.icon} size={15} />{/if}</span>
      <span class="lbl">{it.label}</span>
      <span class="hint">{it.children.length}</span>
      <span class="ic more"><Icon name="right" size={14} /></span>
    </button>
  {:else}
    <button
      class="item"
      class:danger={it.danger}
      disabled={it.disabled}
      role="menuitem"
      onpointerenter={() => top && (sub = null)}
      onclick={() => run(it.run, it.label)}
    >
      <span class="ic">{#if it.icon}<Icon name={it.icon} size={15} />{/if}</span>
      <span class="lbl">{it.label}</span>
      {#if it.hint}<span class="hint">{it.hint}</span>{/if}
    </button>
  {/if}
{/snippet}

{#if menu.open}
  <div class="catch" role="presentation" onpointerdown={closeMenu} oncontextmenu={(e) => (e.preventDefault(), closeMenu())}></div>
  <div class="menu glass-strong" bind:this={el} style="left:{pos.x}px;top:{pos.y}px" role="menu">
    {#each menu.items as it, i}
      {@render item(it, i, true)}
    {/each}
  </div>
  {#if sub}
    <div class="menu glass-strong" bind:this={subEl} style="left:{sub.x}px;top:{sub.y}px" role="menu">
      {#each sub.items as it, i}
        {@render item(it, i, false)}
      {/each}
    </div>
  {/if}
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
    max-height: calc(100vh - 16px);
    overflow-y: auto;
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
    white-space: nowrap;
  }
  .item:hover:not(:disabled),
  .item.on {
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
  .ic.more {
    width: auto;
    color: var(--text-3);
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
