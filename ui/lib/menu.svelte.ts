// Global context menu state.
export interface MenuItem {
  label?: string;
  icon?: string;
  hint?: string;
  run?: () => void;
  disabled?: boolean;
  danger?: boolean;
  sep?: boolean;
}

export const menu = $state({ open: false, x: 0, y: 0, items: [] as MenuItem[] });

export function openMenu(e: MouseEvent | { clientX: number; clientY: number }, items: MenuItem[]) {
  if ("preventDefault" in e) e.preventDefault();
  menu.items = items;
  menu.x = e.clientX;
  menu.y = e.clientY;
  menu.open = true;
}

export function closeMenu() {
  menu.open = false;
}
