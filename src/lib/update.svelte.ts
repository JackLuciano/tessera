import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { app } from './store.svelte';
import { t } from './i18n.svelte';

type State = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error';

/** Auto-update from GitHub Releases: signed `latest.json` → download → verify → install → relaunch. */
class Updater {
  state = $state<State>('idle');
  version = $state('');
  notes = $state('');
  downloaded = $state(0);
  total = $state(0);
  modal = $state(false);
  lastCheck = $state<number | null>(null);
  auto = $state(true);
  private update: Update | null = null;

  constructor() {
    try { this.auto = localStorage.getItem('tessera-autoupdate') !== '0'; } catch {}
  }
  setAuto(v: boolean) { this.auto = v; try { localStorage.setItem('tessera-autoupdate', v ? '1' : '0'); } catch {} }

  async check(silent = false) {
    if (this.state === 'downloading' || this.state === 'checking') return;
    this.state = 'checking';
    try {
      this.update = await check();
      this.lastCheck = Date.now();
      if (!this.update) {
        this.state = 'idle';
        if (!silent) app.toast(t('update.upToDate'), 'success');
        return;
      }
      this.version = this.update.version;
      this.notes = this.update.body ?? '';
      this.state = 'available';
      if (silent && this.auto) await this.download();
      else if (!silent) this.modal = true;
    } catch (e) {
      this.state = 'error';
      if (!silent) app.toast(t('update.failed', { error: String(e) }), 'error');
    }
  }

  async download() {
    if (!this.update) return;
    this.state = 'downloading';
    this.downloaded = 0;
    try {
      await this.update.download(ev => {
        if (ev.event === 'Started') this.total = ev.data.contentLength ?? 0;
        else if (ev.event === 'Progress') this.downloaded += ev.data.chunkLength;
      });
      this.state = 'ready';
      app.toast(t('update.ready', { version: this.version }), 'success');
    } catch (e) {
      this.state = 'error';
      app.toast(t('update.failed', { error: String(e) }), 'error');
    }
  }

  async installAndRestart() {
    if (!this.update) return;
    await this.update.install();
    await relaunch();
  }
}

export const updater = new Updater();
