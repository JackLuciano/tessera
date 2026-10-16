export interface MenuItem { label: string; ico: string; danger?: boolean; disabled?: boolean; run: (ev: MouseEvent) => void }

class MenuState {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  items = $state<(MenuItem | '-')[]>([]);
  show(ev: MouseEvent, items: (MenuItem | '-')[]) {
    ev.preventDefault();
    this.items = items;
    this.x = ev.clientX;
    this.y = ev.clientY;
    this.open = true;
  }
  close() { this.open = false; }
}
export const menu = new MenuState();
