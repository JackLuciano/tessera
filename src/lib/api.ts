import { invoke } from '@tauri-apps/api/core';
import { i18n, t } from './i18n.svelte';

export type Cat = 'video' | 'games' | 'dev' | 'apps' | 'system' | 'images' | 'cache' | 'archives' | 'docs';
export type Kind = 'dir' | 'file' | 'bucket';

export interface Drive { name: string; mount: string; fs: string; total: number; available: number; removable: boolean; kind: string }
export interface DupGroup { size: number; wasted: number; files: { id: number; path: string; modified: number }[] }
export interface SnapshotInfo { ts: number; size: number; files: number }
export interface DiffRow { path: string; before: number; after: number; delta: number }
export type Profile = 'gaming' | 'work' | 'balanced' | 'battery';
export interface OptOutcome { applied: number; failed: string[]; reboot: boolean }
export interface OptStatus {
  supported: boolean; elevated: boolean; active: Profile | null; has_backup: boolean; cpu: number;
  power_plans: { guid: string; name: string; active: boolean }[];
  tweaks: { key: string; on: boolean; admin: boolean; reboot: boolean }[];
  memory: { total: number; used: number };
}
export interface StartupApp { id: string; name: string; command: string; enabled: boolean; scope: 'user' | 'machine' }
export interface AppProcess { name: string; pids: number[]; memory: number; cpu: number; group: string }
export interface ScanSummary { engine: 'mft' | 'walk' | 'cache' | 'live'; root: string; size: number; files: number; dirs: number; elapsed_ms: number; errors: number; cats: { cat: Cat; size: number }[] }
export interface TreeNode { id: number; name: string; path: string; size: number; files: number; dirs: number; kind: Kind; cat: Cat; modified: number; child_count: number; children: TreeNode[] }
export interface FileEntry { id: number; name: string; path: string; size: number; cat: Cat; modified: number }
export interface JunkGroup { key: string; label: string; hint: string; safe: boolean; size: number; ids: number[]; sample: string[] }
export interface DeleteResult { freed: number; deleted: number; failed: string[]; cancelled?: boolean }
export interface DeleteProgress { done: number; total: number; bytes: number; total_bytes: number }

export interface Prefs {
  close_to_tray: boolean; tray_meter: boolean; tray_hint_shown: boolean; widget: boolean; widget_on_top: boolean;
  hotkeys_enabled: boolean; hotkeys: Record<string, string>; weekly: boolean;
}
export type RestorePoint = 'created' | 'recent' | 'session' | 'needs_admin' | 'unavailable' | 'unsupported';
export interface SysJunk { key: string; safe: boolean; admin: boolean; size: number; count: number; sample: string[] }
export interface CleanOutcome { freed: number; cleaned: number; locked: number; failed: string[] }
export interface PrivacyItem { id: string; group: string; kind: string; size: number; count: number; running: boolean }
export interface PrivacyOutcome { freed: number; cleaned: number; failed: string[]; running: string[] }
export interface RegIssue { id: string; cat: string; name: string; missing: string; admin: boolean }
export interface RegBackup { name: string; created: number; items: number }
export interface RegFix { fixed: number; failed: string[]; backup: string | null; restore_point: RestorePoint }
export interface BrokenShortcut { path: string; name: string; target: string; location: string; admin: boolean }
export interface EmptyFolder { id: number; path: string; nested: number }
export interface ShredTarget { path: string; size: number; files: number; is_dir: boolean }
export interface PhysDisk { id: string; name: string; media: 'ssd' | 'hdd' | 'unknown'; bus: string; health: 'healthy' | 'warning' | 'unhealthy' | 'unknown'; size: number; temperature: number | null; wear: number | null; power_on_hours: number | null; read_errors: number | null; volumes: string[] }
export interface HealthReport { disks: PhysDisk[]; counters_need_admin: boolean; can_optimize: boolean }
export interface Program { id: string; name: string; publisher: string; version: string; installed: number; size: number; location: string; machine: boolean }
export interface Leftover { kind: 'folder' | 'registry'; path: string; size: number; admin: boolean }
export interface LeftoverFix { removed: number; freed: number; failed: string[]; backup: string | null; restore_point: RestorePoint | null }
export interface Upgrade { name: string; id: string; version: string; available: string; source: string }
export interface Adapter { index: number; name: string; description: string; guid: string; servers: string[]; manual: boolean; preset: string }
export interface TcpSetting { key: string; on: boolean; value: string }
export interface NetStatus { adapters: Adapter[]; tcp: TcpSetting[]; has_backup: boolean; elevated: boolean }
export interface Latency { key: string; server: string; ms: number | null }
export interface CareConfig { modules: string[]; privacy_kinds: string[]; schedule: 'off' | 'daily' | 'weekly' | 'logon'; time: string; auto_clean: boolean; last_run: number; last_score: number | null; last_found: number; last_issues: number }
export interface CareState extends CareConfig { running: boolean; can_schedule: boolean }
export interface FixPlan { junk: string[]; privacy: string[]; registry: string[]; shortcuts: string[] }
export interface CareReport {
  at: number; score: number; junk: SysJunk[]; privacy: PrivacyItem[]; registry: RegIssue[]; shortcuts: BrokenShortcut[];
  startup: number; disks: PhysDisk[]; updates: Upgrade[]; fixable_bytes: number; fixable_issues: number; plan: FixPlan;
}
export interface Game { id: string; name: string; launcher: string; dir: string; size: number; last_played: number; playtime: number; cover: string | null; launch: string | null; shader_cache: [string, number] | null }
export interface ExeSettings { priority: 'normal' | 'above' | 'high'; gpu_high: boolean; no_fso: boolean }
export interface GameSettings { boost: 'ask' | 'always' | 'never'; close: string[]; keep: string[] }
export interface GameModeConfig {
  enabled: boolean; default_boost: 'ask' | 'always' | 'never'; switch_plan: boolean; apply_profile: boolean; close: string[]; keep: string[];
  reopen: boolean; clear_standby: boolean; timer: boolean; unpark: boolean; record_fps: boolean; overlay: boolean;
  custom: [string, string][]; per_game: Record<string, GameSettings>;
}
export interface ThrottleEvent { start: number; end: number; kind: string; worst: number }
export interface FpsSummary { game: string; exe: string; start: number; seconds: number; frames: number; avg: number; low1: number; low01: number; timeline: number[] }
export interface FpsSample { fps: number; low1: number; frametime: number; recent: number[]; game: string }
export interface GameSession { game_id: string; game: string; start: number; end: number; boosted: boolean; closed: string[]; throttle: ThrottleEvent[]; fps: FpsSummary | null }
export interface GameStatus { game: Game | null; boosted: boolean; since: number; closed: string[]; pending: Game | null }
export interface RunningApp { id: string; name: string; memory: number }
export interface GameLeftover { path: string; size: number; launcher: string }
export interface Gpu { name: string; load: number | null; temp: number | null; mhz: number | null; max_mhz: number | null; mem_used: number | null; mem_total: number | null; power: number | null; power_limit: number | null; fan: number | null; throttle: string[] }
export interface Snapshot {
  at: number; cpu_load: number; cpu_mhz: number | null; cpu_base_mhz: number | null; cpu_limit: number | null; cpu_temp: number | null; cpu_temp_approx: boolean;
  gpus: Gpu[]; ram_used: number; ram_total: number; ram_standby: number | null; sensors: [string, number][]; fans: [string, number][];
}
export interface SensorHistory { history: Snapshot[]; throttle: ThrottleEvent[]; trend: { days: [string, number | null, number | null][]; cpu_rise: number | null; gpu_rise: number | null } }
export interface PingState { host: string; running: boolean; samples: (number | null)[]; avg: number | null; jitter: number | null; loss: number }
export interface Monitor { device: string; name: string; adapter: string; width: number; height: number; hz: number; max_hz: number; primary: boolean }
export interface DisplayInfo { monitors: Monitor[]; windowed_opt: boolean; vrr: boolean; timer: [number, number]; core_parking: number | null }
export interface Battery { present: boolean; name: string; design: number; full: number; cycles: number | null; wear: number | null; charge: number | null; charging: boolean }
export interface Crash { at: number; kind: 'app_crash' | 'app_hang' | 'bsod' | 'power_loss'; app: string; module: string; culprit: string | null; code: string }
export interface Security { defender: boolean | null; realtime: boolean | null; signature_age: number | null; firewall: boolean | null; bitlocker: boolean | null; secure_boot: boolean | null; uac: boolean | null }
export interface UpdateStatus { last_install: number | null; last_title: string | null; reboot_pending: boolean; service_running: boolean | null }
export interface BootInfo { last_boot: number | null; history: [number, number, number, number][]; slow: [string, number][]; needs_admin: boolean }
export interface Driver { name: string; vendor: string; version: string; date: number | null; display_version: string; download: string }
export interface StatsReport { from: number; to: number; freed: number; freed_by: [string, number][]; games: [string, number, number | null][]; care: [number, number][]; daily_freed: number[] }
export interface CareFix { freed: number; fixed: number; failed: string[]; running: string[]; restore_point: RestorePoint | null; registry_backup: string | null }

export const api = {
  drives: () => invoke<Drive[]>('list_drives'),
  scan: (path: string) => invoke<ScanSummary>('scan', { path }),
  cancel: () => invoke<void>('cancel_scan'),
  node: (id: number, depth = 2, maxChildren = 80, minFrac?: number) => invoke<TreeNode>('get_node', { id, depth, maxChildren, minFrac }),
  ancestors: (id: number) => invoke<[number, string][]>('get_ancestors', { id }),
  largestFiles: (limit = 300) => invoke<FileEntry[]>('largest_files', { limit }),
  junk: () => invoke<JunkGroup[]>('find_junk'),
  delete: (ids: number[], permanent: boolean, shred = false) => invoke<DeleteResult>('delete_nodes', { ids, permanent, shred }),
  cancelDelete: () => invoke<void>('cancel_delete'),
  reveal: (path: string) => invoke<void>('reveal', { path }),
  isElevated: () => invoke<boolean>('is_elevated'),
  relaunchAsAdmin: () => invoke<void>('relaunch_as_admin'),
  duplicates: () => invoke<DupGroup[]>('find_duplicates'),
  cancelDuplicates: () => invoke<void>('cancel_duplicates'),
  /** Replace duplicates with hard links; pairs are [keep, duplicate] node ids. */
  hardlinkDupes: (pairs: [number, number][]) => invoke<DeleteResult>('hardlink_dupes', { pairs }),
  setLiveWatch: (enabled: boolean) => invoke<void>('set_live_watch', { enabled }),
  summary: () => invoke<ScanSummary>('get_summary'),
  /** Saved scan of `root`, or null; changes since then arrive later as `catchup-done`. */
  loadCached: (root: string) => invoke<(ScanSummary & { cached_at: number }) | null>('load_cached', { root }),
  opt: {
    status: () => invoke<OptStatus>('opt_status'),
    applyProfile: (profile: Profile) => invoke<OptOutcome>('opt_apply_profile', { profile }),
    setTweak: (key: string, on: boolean) => invoke<OptOutcome>('opt_set_tweak', { key, on }),
    setPowerPlan: (guid: string) => invoke<void>('opt_set_power_plan', { guid }),
    restore: () => invoke<OptOutcome>('opt_restore'),
    startup: () => invoke<StartupApp[]>('opt_startup_list'),
    setStartup: (id: string, enabled: boolean) => invoke<void>('opt_startup_set', { id, enabled }),
    processes: () => invoke<AppProcess[]>('opt_processes'),
    kill: (pids: number[]) => invoke<OptOutcome>('opt_kill', { pids }),
    stats: () => invoke<{ memory: { total: number; used: number }; cpu: number }>('sys_stats'),
  },
  historyList: (root: string) => invoke<SnapshotInfo[]>('history_list', { root }),
  historyDiff: (ts: number) => invoke<DiffRow[]>('history_diff', { ts }),
  sysJunk: () => invoke<SysJunk[]>('junk_scan'),
  cleanSysJunk: (keys: string[]) => invoke<CleanOutcome>('junk_clean', { keys }),
  privacy: () => invoke<PrivacyItem[]>('privacy_scan'),
  cleanPrivacy: (ids: string[]) => invoke<PrivacyOutcome>('privacy_clean', { ids }),
  shortcuts: () => invoke<BrokenShortcut[]>('shortcuts_scan'),
  fixShortcuts: (paths: string[]) => invoke<{ fixed: number; failed: string[] }>('shortcuts_fix', { paths }),
  emptyFolders: () => invoke<EmptyFolder[]>('empty_folders'),
  shredInspect: (paths: string[]) => invoke<ShredTarget[]>('shred_inspect', { paths }),
  shred: (paths: string[], passes: number) => invoke<DeleteResult>('shred_paths', { paths, passes }),
  diskHealth: () => invoke<HealthReport>('disk_health'),
  diskOptimize: (volume: string) => invoke<string>('disk_optimize', { volume }),
  registry: () => invoke<RegIssue[]>('registry_scan'),
  fixRegistry: (ids: string[]) => invoke<RegFix>('registry_fix', { ids }),
  registryBackups: () => invoke<RegBackup[]>('registry_backups'),
  undoRegistry: (name: string) => invoke<number>('registry_undo', { name }),
  programs: () => invoke<Program[]>('programs_list'),
  uninstall: (id: string) => invoke<boolean>('program_uninstall', { id }),
  leftovers: (program: Program) => invoke<Leftover[]>('program_leftovers', { program }),
  removeLeftovers: (items: Leftover[]) => invoke<LeftoverFix>('leftovers_remove', { items }),
  updates: () => invoke<{ available: boolean; items: Upgrade[] }>('updates_list'),
  installUpdate: (id: string) => invoke<void>('update_install', { id }),
  net: {
    status: () => invoke<NetStatus>('net_status'),
    setDns: (preset: string) => invoke<number>('net_set_dns', { preset }),
    latency: (current?: string) => invoke<Latency[]>('net_latency', { current }),
    flush: () => invoke<void>('net_flush'),
    setTcp: (key: string, on: boolean) => invoke<void>('net_set_tcp', { key, on }),
    restore: () => invoke<number>('net_restore'),
    repair: () => invoke<void>('net_repair'),
  },
  care: {
    config: () => invoke<CareState>('care_config'),
    setConfig: (config: CareConfig) => invoke<void>('care_set_config', { config }),
    scan: () => invoke<CareReport>('care_scan'),
    fix: (plan: FixPlan) => invoke<CareFix>('care_fix', { plan }),
  },
  prefs: () => invoke<Prefs>('prefs_get'),
  /** Returns hotkeys that could not be registered. */
  setPrefs: (prefs: Prefs) => invoke<string[]>('prefs_set', { prefs }),
  games: {
    list: () => invoke<Game[]>('games_list'),
    launch: (target: string) => invoke<void>('game_launch', { target }),
    exes: (dir: string) => invoke<[string, ExeSettings][]>('game_exes', { dir }),
    setExe: (exe: string, settings: ExeSettings) => invoke<void>('game_set_exe', { exe, settings }),
    leftovers: () => invoke<GameLeftover[]>('game_leftovers'),
    remove: (paths: string[]) => invoke<{ freed: number; failed: string[] }>('game_remove_paths', { paths }),
  },
  gamemode: {
    config: () => invoke<GameModeConfig>('gamemode_config'),
    setConfig: (config: GameModeConfig) => invoke<void>('gamemode_set_config', { config }),
    status: () => invoke<GameStatus>('gamemode_status'),
    answer: (yes: boolean, remember: boolean) => invoke<void>('gamemode_answer', { yes, remember }),
    boostNow: () => invoke<void>('gamemode_boost_now'),
    sessions: () => invoke<GameSession[]>('gamemode_sessions'),
    runningApps: () => invoke<RunningApp[]>('running_apps'),
  },
  fps: {
    status: () => invoke<{ tool_ready: boolean; recording: string | null; elevated: boolean; overlay: boolean }>('fps_status'),
    start: (exe: string, game: string) => invoke<void>('fps_start', { exe, game }),
    stop: () => invoke<FpsSummary | null>('fps_stop'),
    sessions: () => invoke<FpsSummary[]>('fps_sessions'),
    remove: (start: number) => invoke<void>('fps_delete', { start }),
    overlay: (on: boolean) => invoke<void>('overlay_set', { on }),
  },
  sensors: () => invoke<Snapshot | null>('sensors_now'),
  sensorHistory: () => invoke<SensorHistory>('sensors_history'),
  ping: (host?: string) => invoke<PingState>('ping_get', { host }),
  gateway: () => invoke<string | null>('ping_gateway'),
  display: {
    info: () => invoke<DisplayInfo>('display_info'),
    setRefresh: (device: string, hz: number) => invoke<void>('display_set_refresh', { device, hz }),
    setSwitch: (key: string, on: boolean) => invoke<void>('display_set_switch', { key, on }),
    setParking: (percent: number) => invoke<void>('core_parking_set', { percent }),
    purgeRam: () => invoke<number>('ram_purge'),
  },
  battery: () => invoke<{ battery: Battery; history: [number, number, number][] }>('battery_info'),
  crashes: (days: number) => invoke<Crash[]>('crashes_list', { days }),
  security: () => invoke<Security>('security_status'),
  winupdate: {
    status: () => invoke<UpdateStatus>('winupdate_status'),
    pending: () => invoke<string[]>('winupdate_pending'),
    reset: () => invoke<void>('winupdate_reset'),
  },
  boot: () => invoke<BootInfo>('boot_info'),
  drivers: () => invoke<Driver[]>('drivers_list'),
  componentCleanup: () => invoke<number>('component_cleanup'),
  report: (days: number) => invoke<StatsReport>('stats_report', { days }),
  widget: (on: boolean) => invoke<void>('widget_set', { on }),
  openUri: (uri: string) => invoke<void>('open_uri', { uri }),
  exportSettings: (path: string, local: unknown) => invoke<void>('settings_export', { path, local }),
  importSettings: (path: string) => invoke<Record<string, unknown> | null>('settings_import', { path }),
  trayLabels: (labels: Record<string, string>) => invoke<void>('tray_labels', { labels }),
  launchState: () => invoke<{ hidden: boolean }>('launch_state'),
};

export const CATS: Record<Cat, { label: string; ico: string }> = {
  video: { label: 'Video', ico: 'video' },
  games: { label: 'Games', ico: 'game' },
  dev: { label: 'Dev & code', ico: 'code' },
  apps: { label: 'Applications', ico: 'app' },
  system: { label: 'System', ico: 'cpu' },
  images: { label: 'Images', ico: 'image' },
  cache: { label: 'Caches & temp', ico: 'cache' },
  archives: { label: 'Archives & ISO', ico: 'archive' },
  docs: { label: 'Documents & other', ico: 'doc' },
};

/** Human-readable size; binary units shown with the familiar GB/MB labels like Windows Explorer. */
export function fmt(bytes: number): string {
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = bytes, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}
export const num = (n: number) => n.toLocaleString('en-US');
/** "3 weeks ago" in the UI language, via the browser's own translations. */
export function ago(unixSecs: number, fine = false): string {
  if (!unixSecs) return '—';
  const rtf = new Intl.RelativeTimeFormat(i18n.locale, { numeric: 'auto' });
  const secs = Date.now() / 1000 - unixSecs;
  // `fine`: minutes/hours for recent times (saved scans); file dates only need days
  if (fine && secs < 86400) return secs < 3600 ? rtf.format(-Math.max(1, Math.round(secs / 60)), 'minute') : rtf.format(-Math.round(secs / 3600), 'hour');
  const days = Math.floor(secs / 86400);
  if (days < 14) return rtf.format(-Math.max(0, days), 'day');
  if (days < 60) return rtf.format(-Math.round(days / 7), 'week');
  if (days < 365) return rtf.format(-Math.round(days / 30), 'month');
  const y = days / 365;
  return rtf.format(-(y < 2 ? 1 : Math.round(y * 10) / 10), 'year');
}
/** Display name; "small files" buckets are named in Rust, so they are translated here. */
export const nodeName = (n: { kind: Kind; name: string; files?: number }) => (n.kind === 'bucket' ? t('common.smallFiles', { n: num(n.files ?? 0) }) : n.name);
export const css = (v: string) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
export const catColor = (c: Cat) => `var(--cat-${c})`;
