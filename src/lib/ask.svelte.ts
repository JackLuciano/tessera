/**
 * Promise-based confirmation for actions that aren't plain file deletes (privacy sweep, registry
 * fixes, uninstalls, updates…): `if (await ask({...})) doIt()`.
 */
export interface AskItem { name: string; detail?: string }
export interface AskOptions {
  title: string;
  text?: string;
  items?: AskItem[];
  /** Highlighted note under the text, e.g. the restore point status. */
  note?: string;
  confirm: string;
  danger?: boolean;
  /** Extra checkbox the user has to tick first (irreversible actions). */
  ack?: string;
  ico?: string;
}

class AskState {
  open = $state(false);
  opts = $state<AskOptions | null>(null);
  ack = $state(false);
  #resolve: ((ok: boolean) => void) | null = null;

  ask(o: AskOptions): Promise<boolean> {
    this.#resolve?.(false);
    this.opts = o;
    this.ack = false;
    this.open = true;
    return new Promise(r => (this.#resolve = r));
  }

  done(ok: boolean) {
    this.open = false;
    this.#resolve?.(ok);
    this.#resolve = null;
  }
}

export const asker = new AskState();
export const ask = (o: AskOptions) => asker.ask(o);
