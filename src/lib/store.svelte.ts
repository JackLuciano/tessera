import { api, type Drive, type ScanSummary, fmt } from './api';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n.svelte';

export type View = 'overview' | 'explorer' | 'cleanup' | 'files' | 'dupes' | 'history' | 'optimizer' | 'settings';
/** The optimizer only exists on Windows (WebView2 reports it in the user agent). */
export const IS_WINDOWS = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);
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
  /** Bytes changed by live events since the views last refetched. */
  #liveDelta = 0;
  /** Latest totals from live events, applied to `summary` together with the next view refresh. */
  #liveTotals: { size: number; files: number } | null = null;
  #liveFoldersPending = 0;
  /** Open the last saved scan on start instead of an empty screen. */
  reopen = $state<boolean>(load('tessera-reopen', true));
  /** When the shown results come from a saved scan: its time (unix seconds); null after a fresh scan. */
  cachedAt = $state<number | null>(null);
  /** The backend is still checking what changed since the saved scan. */
  catchingUp = $state(false);

  get drive() { return this.drives.find(d => this.target.toUpperCase().startsWith(d.mount.toUpperCase())) ?? null; }

  async init() {
    await listen<{ files: number; dirs: number; bytes: number }>('scan-progress', e => { this.progress = e.payload; });
    await listen<{ folders: number; delta: number; size: number; files: number }>('live-change', e => {
      if (!this.summary || this.scanning) return;
      // A busy system drive changes every few seconds (logs, browser caches). Nothing reactive is
      // touched here: `summary` and the views only update once the changes add up to something
      // visible, at most every 15 s. Changing `summary` per event re-rendered every page constantly.
      this.#liveTotals = { size: e.payload.size, files: e.payload.files };
      this.#liveFoldersPending += e.payload.folders;
      this.#liveDelta += Math.abs(e.payload.delta);
      if (this.#liveDelta < Math.max(64 * 1024 ** 2, e.payload.size * 0.002)) return;
      this.#liveTimer ??= setTimeout(() => {
        this.#liveTimer = null;
        this.#liveDelta = 0;
        if (this.summary && this.#liveTotals) this.summary = { ...this.summary, ...this.#liveTotals };
        this.liveFolders += this.#liveFoldersPending;
        this.#liveFoldersPending = 0;
        this.liveVersion++;
      }, 15000);
    });
    await listen<{ folders: number; size: number; files: number }>('catchup-done', e => {
      if (!this.summary) return;
      this.summary = { ...this.summary, size: e.payload.size, files: e.payload.files };
      this.catchingUp = false;
      this.liveVersion++;
      if (e.payload.folders) this.toast(t('cache.caughtUp', { n: e.payload.folders.toLocaleString() }), 'info');
    });
    api.setLiveWatch(this.live).catch(() => {});
    await this.refreshDrives();
    if (!this.target && this.drives.length) this.target = this.drives[0].mount;
    this.elevated = await api.isElevated().catch(() => false);
    if (this.reopen && this.target) await this.openSaved(this.target);
  }

  /** Switch to a drive: its saved scan if there is one (and reopening is on), otherwise a new scan. */
  async openDrive(mount: string) {
    if (this.scanning) return;
    if (this.reopen && mount !== this.summary?.root && (await this.openSaved(mount))) {
      this.target = mount; save('tessera-target', mount);
      return;
    }
    await this.scan(mount);
  }

  /** Show the saved scan of `root` right away; the backend then catches up on what changed. */
  async openSaved(root: string) {
    const c = await api.loadCached(root).catch(() => null);
    if (!c) return false;
    this.summary = c;
    this.cachedAt = c.cached_at;
    this.catchingUp = true;
    this.liveFolders = 0;
    this.explorerTarget = 0;
    this.version++;
    return true;
  }
  async refreshDrives() { try { this.drives = await api.drives(); } catch (e) { this.toast(String(e), 'error'); } }

  async scan(path = this.target) {
    if (this.scanning) return;
    this.target = path; save('tessera-target', path);
    this.scanning = true; this.progress = { files: 0, dirs: 0, bytes: 0 };
    try {
      this.summary = await api.scan(path);
      this.cachedAt = null;
      this.catchingUp = false;
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
  setReopen(v: boolean) { this.reopen = v; save('tessera-reopen', v); }
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
