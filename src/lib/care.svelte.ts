import { listen } from '@tauri-apps/api/event';
import { api, type CareReport } from './api';

/** Care results outlive the page, so switching views doesn't throw a finished scan away. */
class CareStore {
  report = $state<CareReport | null>(null);
  scanning = $state(false);
  fixing = $state(false);
  #listening = false;

  /** Scans started from the tray or the schedule report here too. */
  async listen() {
    if (this.#listening) return;
    this.#listening = true;
    await listen('care-started', () => (this.scanning = true));
    await listen<CareReport>('care-report', e => { this.report = e.payload; this.scanning = false; });
  }

  async scan() {
    if (this.scanning) return;
    this.scanning = true;
    try { this.report = await api.care.scan(); } finally { this.scanning = false; }
  }
}

export const care = new CareStore();
