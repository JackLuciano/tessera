import { api, type Drive, type ScanSummary, fmt } from './api';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n.svelte';

export type View = 'overview' | 'explorer' | 'cleanup' | 'files' | 'dupes' | 'history' | 'settings';
type ToastKind = 'success' | 'info' | 'warning' | 'error';
export interface Toast { id: number; msg: string; kind: ToastKind }

function load<T>(key: string, fallback: T): T {
  try { const v = localStorage.getItem(key); return v === null ? fallback : JSON.parse(v); } catch { return fallback; }
}
function save(key: string, v: unknown) { try { localStorage.setItem(key, JSON.stringify(v)); } catch {} }

class AppStore {
  view = $state<View>('overview');
  drives = $state<Drive[]>([]);
  target = $state<string>(load('tessera-target', ''));
  summary = $state<ScanSummary | null>(null);
  scanning = $state(false);
  progress = $state({ files: 0, dirs: 0, bytes: 0 });
  /** Bumped after deletes so views refetch from the Rust tree. */
  version = $state(0);
  /** Node id the Explorer should open (set by other views before switching). */
  explorerTarget = $state(0);
  toasts = $state<Toast[]>([]);
  permanentDelete = $state<boolean>(load('tessera-perm', false));
  elevated = $state(false);
  paletteOpen = $state(false);
  mode = $state<'light' | 'dark' | 'system'>(load('tessera-mode-v', 'system'));
  animations = $state<boolean>(load('tessera-anim', true));
  /** Live change tracking (file system watcher) on/off. */
  live = $state<boolean>(load('tessera-live', true));
  /** Bumped when the watcher folded disk changes into the tree; size views refetch on it. */
  liveVersion = $state(0);
  /** Folders refreshed by the watcher since the last scan, for the header indicator. */
  liveFolders = $state(0);
  #liveTimer: ReturnType<typeof setTimeout> | null = null;

  get drive() { return this.drives.find(d => this.target.toUpperCase().startsWith(d.mount.toUpperCase())) ?? null; }

  async init() {
    await listen<{ files: number; dirs: number; bytes: number }>('scan-progress', e => { this.progress = e.payload; });
    await listen<{ folders: number; delta: number; size: number; files: number }>('live-change', e => {
      if (!this.summary || this.scanning) return;
      this.liveFolders += e.payload.folders;
      this.summary = { ...this.summary, size: e.payload.size, files: e.payload.files };
      // Views refetch at most every 5 s, however busy the disk is
      this.#liveTimer ??= setTimeout(() => { this.#liveTimer = null; this.liveVersion++; }, 5000);
    });
    api.setLiveWatch(this.live).catch(() => {});
    await this.refreshDrives();
    if (!this.target && this.drives.length) this.target = this.drives[0].mount;
    this.elevated = await api.isElevated().catch(() => false);
  }
  async refreshDrives() { try { this.drives = await api.drives(); } catch (e) { this.toast(String(e), 'error'); } }

  async scan(path = this.target) {
    if (this.scanning) return;
    this.target = path; save('tessera-target', path);
    this.scanning = true; this.progress = { files: 0, dirs: 0, bytes: 0 };
    try {
      this.summary = await api.scan(path);
      this.liveFolders = 0;
      this.explorerTarget = 0;
      this.version++;
      await this.refreshDrives();
      this.toast(t('toast.scanned', { files: this.summary.files.toLocaleString(), size: fmt(this.summary.size), secs: (this.summary.elapsed_ms / 1000).toFixed(1) }), 'success');
    } catch (e) {
      this.toast(String(e) === 'cancelled' ? t('toast.scanCancelled') : t('toast.scanFailed', { error: String(e) }), String(e) === 'cancelled' ? 'info' : 'error');
    } finally { this.scanning = false; }
  }

  /** Called after a successful delete so totals and drive bars stay accurate. */
  async afterDelete(freed: number) {
    if (this.summary) this.summary = { ...this.summary, size: Math.max(0, this.summary.size - freed) };
    this.version++;
    await this.refreshDrives();
  }

  toast(msg: string, kind: ToastKind = 'success') {
    const id = Date.now() + Math.random();
    this.toasts.push({ id, msg, kind });
    setTimeout(() => { this.toasts = this.toasts.filter(t => t.id !== id); }, 4200);
  }

  setPermanent(v: boolean) { this.permanentDelete = v; save('tessera-perm', v); }
  setLive(v: boolean) {
    this.live = v; save('tessera-live', v);
    api.setLiveWatch(v).catch(e => this.toast(String(e), 'error'));
  }
  setAnimations(v: boolean) {
    this.animations = v; save('tessera-anim', v);
    document.documentElement.classList.toggle('no-anim', !v);
  }
  setMode(m: 'light' | 'dark' | 'system') {
    this.mode = m; save('tessera-mode-v', m);
    try { localStorage.setItem('tessera-mode', m); } catch {}
    const light = m === 'light' || (m === 'system' && matchMedia('(prefers-color-scheme: light)').matches);
    const apply = () => { document.documentElement.dataset.theme = light ? 'tessera-light' : 'tessera-dark'; };
    const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
    doc.startViewTransition ? doc.startViewTransition(apply) : apply();
  }
}

export const app = new AppStore();
